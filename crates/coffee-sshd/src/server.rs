//! The SSH server: passwordless logins, one `coffee-tui` child per session on a PTY.

use std::{
  future::Future,
  io,
  net::SocketAddr,
  os::unix::process::ExitStatusExt,
  process::ExitStatus,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use bytes::Bytes;
use russh::{
  Channel, ChannelId, MethodKind, MethodSet, Pty, Sig,
  keys::{HashAlg, PrivateKey},
  server::{self, Auth, ChannelOpenHandle, Handle, Msg, Response, Session, VerifiedPublicKey},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  sync::{mpsc, oneshot},
  task::JoinHandle,
};

use crate::{
  config::Config,
  limits::{Limits, Refusal, Slot},
};

/// The notice every client sees before logging in (SSH auth banner).
pub const BANNER: &str =
  "\n  ◆ROAST/TTY 终端咖啡馆\n  演示环境 · 不是真实商店 · 不收款、不发货 · 请勿使用真实密码\n\n";

/// How long a session's child has to leave after SIGHUP before it is killed.
const KILL_GRACE: Duration = Duration::from_millis(1000);
/// Keepalives detect a client that vanished without closing its connection.
const KEEPALIVE: Duration = Duration::from_secs(30);

/// What the server shares between connections.
struct Shared {
  config: Config,
  limits: Arc<Limits>,
  next_id: AtomicU64,
  connections: AtomicUsize,
  started: Instant,
}

/// Runs the SSH server on `listener` until `shutdown` resolves.
pub async fn serve(
  listener: TcpListener,
  config: Config,
  host_key: PrivateKey,
  shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
  let russh_config = Arc::new(server::Config {
    server_id: russh::SshId::Standard("SSH-2.0-ROAST_TTY".into()),
    methods: MethodSet::from(&[MethodKind::PublicKey, MethodKind::KeyboardInteractive][..]),
    // Nothing is secret here: the first `none` probe is answered at once, and no
    // other attempt is ever rejected.
    auth_rejection_time: Duration::from_millis(100),
    auth_rejection_time_initial: Some(Duration::ZERO),
    keys: vec![host_key],
    inactivity_timeout: None,
    keepalive_interval: Some(KEEPALIVE),
    keepalive_max: 3,
    nodelay: true,
    ..Default::default()
  });
  let shared = Arc::new(Shared {
    limits: Limits::new(config.max_sessions, config.max_sessions_per_ip),
    config,
    next_id: AtomicU64::new(1),
    connections: AtomicUsize::new(0),
    started: Instant::now(),
  });
  // Connections beyond this are dropped before the handshake: refused sessions still
  // get a polite message, but only up to a point.
  let max_connections = shared.config.max_sessions * 2 + 16;
  tokio::pin!(shutdown);
  loop {
    let (socket, peer) = tokio::select! {
      () = &mut shutdown => return Ok(()),
      accepted = listener.accept() => match accepted {
        Ok(accepted) => accepted,
        Err(error) => {
          tracing::warn!(%error, "accept failed");
          tokio::time::sleep(Duration::from_millis(50)).await;
          continue;
        }
      },
    };
    let open = shared.connections.fetch_add(1, Ordering::AcqRel) + 1;
    let counted = Counted(shared.clone());
    if open > max_connections {
      tracing::warn!(peer = %peer, open, "too many connections; dropped");
      continue;
    }
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let authenticated = Arc::new(AtomicBool::new(false));
    let handler = Connection::new(shared.clone(), id, peer, authenticated.clone());
    let config = russh_config.clone();
    let auth_timeout = shared.config.auth_timeout;
    tokio::spawn(async move {
      let _counted = counted;
      let session = match server::run_stream(config, socket, handler).await {
        Ok(session) => session,
        Err(error) => {
          tracing::debug!(conn = id, %error, "handshake failed");
          return;
        }
      };
      let handle = session.handle();
      let watchdog = tokio::spawn(async move {
        tokio::time::sleep(auth_timeout).await;
        if !authenticated.load(Ordering::Acquire) {
          tracing::info!(conn = id, "login timed out");
          let _ = handle
            .disconnect(
              russh::Disconnect::ByApplication,
              "login timed out".into(),
              String::new(),
            )
            .await;
        }
      });
      if let Err(error) = session.await {
        tracing::debug!(conn = id, %error, "connection ended with an error");
      }
      watchdog.abort();
    });
  }
}

/// One open TCP connection, counted until it ends.
struct Counted(Arc<Shared>);

impl Drop for Counted {
  fn drop(&mut self) {
    self.0.connections.fetch_sub(1, Ordering::AcqRel);
  }
}

/// The requested terminal.
#[derive(Clone, Debug)]
struct PtyRequest {
  term: String,
  cols: u16,
  rows: u16,
}

/// What a session task gets from its connection.
enum Msg2Child {
  Input(Vec<u8>),
  Resize { cols: u16, rows: u16 },
}

/// One SSH connection (russh calls it a session).
struct Connection {
  shared: Arc<Shared>,
  id: u64,
  peer: SocketAddr,
  connected: Instant,
  authenticated: Arc<AtomicBool>,
  slot: Option<Result<Slot, Refusal>>,
  fingerprint: Option<String>,
  method: &'static str,
  channel: Option<ChannelId>,
  pty: Option<PtyRequest>,
  env: Vec<(String, String)>,
  started: bool,
  to_child: Option<mpsc::Sender<Msg2Child>>,
  last_input: Arc<AtomicU64>,
}

impl Connection {
  fn new(shared: Arc<Shared>, id: u64, peer: SocketAddr, authenticated: Arc<AtomicBool>) -> Self {
    let slot = shared.limits.acquire(peer.ip());
    match &slot {
      Ok(_) => tracing::info!(conn = id, peer = %peer, "connected"),
      Err(refusal) => {
        tracing::warn!(conn = id, peer = %peer, refused = refusal.reason(), "connected over the limit")
      }
    }
    let last_input = Arc::new(AtomicU64::new(shared.millis()));
    Self {
      shared,
      id,
      peer,
      connected: Instant::now(),
      authenticated,
      slot: Some(slot),
      fingerprint: None,
      method: "none",
      channel: None,
      pty: None,
      env: Vec::new(),
      started: false,
      to_child: None,
      last_input,
    }
  }

  fn touch(&self) {
    self
      .last_input
      .store(self.shared.millis(), Ordering::Relaxed);
  }

  fn accept(&mut self, method: &'static str) -> Auth {
    self.method = method;
    self.authenticated.store(true, Ordering::Release);
    tracing::info!(
      conn = self.id,
      method,
      fingerprint = self.fingerprint.as_deref().unwrap_or("-"),
      "authenticated"
    );
    Auth::Accept
  }

  /// Tells the client `message` on the channel, then ends it with `status`.
  async fn refuse(handle: &Handle, channel: ChannelId, message: &str, status: u32) {
    let text = format!("\r\n{message}\r\n");
    let _ = handle.data(channel, Bytes::from(text.into_bytes())).await;
    let _ = handle.exit_status_request(channel, status).await;
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
  }

  fn child_env(&self, pty: &PtyRequest) -> Vec<(String, String)> {
    let config = &self.shared.config;
    let mut env: Vec<(String, String)> = Vec::new();
    for name in ["PATH", "HOME", "TMPDIR", "USER", "LOGNAME"] {
      if let Ok(value) = std::env::var(name) {
        env.push((name.into(), value));
      }
    }
    env.push(("TERM".into(), pty.term.clone()));
    let utf8 = |value: &str| {
      let lower = value.to_ascii_lowercase();
      lower.contains("utf-8") || lower.contains("utf8")
    };
    let mut lang = None;
    for (name, value) in &self.env {
      match name.as_str() {
        "LANG" if utf8(value) => lang = Some(value.clone()),
        "LANG" => {}
        _ => env.push((name.clone(), value.clone())),
      }
    }
    env.push(("LANG".into(), lang.unwrap_or_else(|| "C.UTF-8".into())));
    env.push(("COFFEE_API_URL".into(), config.api_url.clone()));
    if let Some(key) = &config.internal_key {
      env.push(("COFFEE_INTERNAL_KEY".into(), key.clone()));
    }
    if let Some(fingerprint) = &self.fingerprint {
      env.push(("COFFEE_SSH_FINGERPRINT".into(), fingerprint.clone()));
    }
    env.push(("COFFEE_CLIENT_IP".into(), self.peer.ip().to_string()));
    if let Some(colors) = &config.colors {
      env.push(("COFFEE_COLORS".into(), colors.clone()));
    }
    env
  }

  async fn start(&mut self, channel: ChannelId, handle: Handle) {
    let Some(pty) = self.pty.clone() else {
      tracing::info!(conn = self.id, "shell without a pty refused");
      Self::refuse(
        &handle,
        channel,
        "ROAST/TTY 需要一个终端：请用 `ssh -t`，不要加 -T。",
        1,
      )
      .await;
      return;
    };
    let slot = match self.slot.take() {
      Some(Ok(slot)) => slot,
      Some(Err(refusal)) => {
        tracing::warn!(
          conn = self.id,
          refused = refusal.reason(),
          "session refused"
        );
        Self::refuse(&handle, channel, &refusal.message(), 1).await;
        return;
      }
      None => return,
    };
    let env = self.child_env(&pty);
    let spawned = spawn_child(&self.shared.config, &pty, env);
    let (child, pty_master) = match spawned {
      Ok(spawned) => spawned,
      Err(error) => {
        tracing::error!(
          conn = self.id,
          bin = %self.shared.config.tui_bin.display(),
          %error,
          "could not start the session program"
        );
        Self::refuse(&handle, channel, "ROAST/TTY 暂时无法启动，请稍后再试。", 1).await;
        return;
      }
    };
    let pid = child.id().unwrap_or_default();
    tracing::info!(
      conn = self.id,
      pid,
      term = %pty.term,
      size = format!("{}x{}", pty.cols, pty.rows),
      active = self.shared.limits.active(),
      "session started"
    );
    let (to_child, from_connection) = mpsc::channel(64);
    self.to_child = Some(to_child);
    tokio::spawn(run_session(SessionTask {
      conn: self.id,
      child,
      pty: pty_master,
      from_connection,
      handle,
      channel,
      last_input: self.last_input.clone(),
      shared: self.shared.clone(),
      _slot: slot,
    }));
  }
}

impl Shared {
  fn millis(&self) -> u64 {
    u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
  }
}

impl Drop for Connection {
  fn drop(&mut self) {
    tracing::info!(
      conn = self.id,
      seconds = self.connected.elapsed().as_secs(),
      "disconnected"
    );
  }
}

/// Starts the session program on a new PTY of `pty`'s size, as its session leader.
fn spawn_child(
  config: &Config,
  pty: &PtyRequest,
  env: Vec<(String, String)>,
) -> io::Result<(tokio::process::Child, pty_process::Pty)> {
  let (master, slave) = pty_process::open().map_err(io::Error::other)?;
  master
    .resize(pty_process::Size::new(pty.rows, pty.cols))
    .map_err(io::Error::other)?;
  let child = pty_process::Command::new(&config.tui_bin)
    .env_clear()
    .envs(env)
    .current_dir(std::env::temp_dir())
    .kill_on_drop(true)
    .spawn(slave)
    .map_err(io::Error::other)?;
  Ok((child, master))
}

struct SessionTask {
  conn: u64,
  child: tokio::process::Child,
  pty: pty_process::Pty,
  from_connection: mpsc::Receiver<Msg2Child>,
  handle: Handle,
  channel: ChannelId,
  last_input: Arc<AtomicU64>,
  shared: Arc<Shared>,
  _slot: Slot,
}

/// How a session ended.
enum End {
  Exited(io::Result<ExitStatus>),
  Disconnected,
  Idle,
}

/// Pumps the PTY to the channel and the channel to the PTY until the child exits or
/// the client goes; then stops the child (SIGHUP to its process group, SIGKILL after
/// [`KILL_GRACE`]) and closes the channel with the child's exit status.
async fn run_session(task: SessionTask) {
  let SessionTask {
    conn,
    mut child,
    pty,
    mut from_connection,
    handle,
    channel,
    last_input,
    shared,
    _slot,
  } = task;
  let started = Instant::now();
  let pid = child.id();
  let (mut reader, mut writer) = pty.into_split();

  let output = handle.clone();
  let mut pump: JoinHandle<()> = tokio::spawn(async move {
    let mut buffer = vec![0u8; 32 * 1024];
    loop {
      match reader.read(&mut buffer).await {
        Ok(0) | Err(_) => break,
        Ok(read) => {
          if output
            .data(channel, Bytes::copy_from_slice(&buffer[..read]))
            .await
            .is_err()
          {
            break;
          }
        }
      }
    }
  });

  // Input and resizes go to the PTY on their own task, so a child that stops reading
  // can never keep a disconnect from being noticed.
  let (gone_tx, mut gone) = oneshot::channel::<()>();
  let feeder = tokio::spawn(async move {
    while let Some(message) = from_connection.recv().await {
      match message {
        Msg2Child::Input(bytes) => {
          if writer.write_all(&bytes).await.is_err() {
            break;
          }
        }
        Msg2Child::Resize { cols, rows } => {
          let _ = writer.resize(pty_process::Size::new(rows, cols));
        }
      }
    }
    let _ = gone_tx.send(());
  });

  let idle = shared.config.idle_timeout;
  let end = loop {
    let check = idle.map_or(Duration::from_secs(3600), |idle| {
      let since = shared
        .millis()
        .saturating_sub(last_input.load(Ordering::Relaxed));
      idle.saturating_sub(Duration::from_millis(since))
    });
    tokio::select! {
      status = child.wait() => break End::Exited(status),
      _ = &mut gone => break End::Disconnected,
      () = tokio::time::sleep(check) => {
        if let Some(idle) = idle {
          let since = shared.millis().saturating_sub(last_input.load(Ordering::Relaxed));
          if Duration::from_millis(since) >= idle {
            break End::Idle;
          }
        }
      }
    }
  };

  match end {
    End::Exited(status) => {
      feeder.abort();
      // The last frame (the restored screen) may still be in the PTY.
      if tokio::time::timeout(Duration::from_millis(500), &mut pump)
        .await
        .is_err()
      {
        pump.abort();
      }
      let code = match &status {
        Ok(status) => exit_code(*status),
        Err(_) => 255,
      };
      tracing::info!(
        conn,
        ?pid,
        code,
        seconds = started.elapsed().as_secs(),
        "session ended: program exited"
      );
      let _ = handle.exit_status_request(channel, code).await;
      let _ = handle.eof(channel).await;
      let _ = handle.close(channel).await;
    }
    End::Disconnected => {
      feeder.abort();
      let how = stop(&mut child, pid).await;
      pump.abort();
      tracing::info!(
        conn,
        ?pid,
        how,
        seconds = started.elapsed().as_secs(),
        "session ended: client left"
      );
    }
    End::Idle => {
      feeder.abort();
      let how = stop(&mut child, pid).await;
      let _ = tokio::time::timeout(Duration::from_millis(300), &mut pump).await;
      pump.abort();
      let minutes = idle.map_or(0, |idle| idle.as_secs() / 60);
      tracing::info!(conn, ?pid, how, "session ended: idle");
      Connection::refuse(
        &handle,
        channel,
        &format!("ROAST/TTY 空闲超过 {minutes} 分钟，连接已关闭。"),
        124,
      )
      .await;
      let _ = handle
        .disconnect(
          russh::Disconnect::ByApplication,
          "idle".into(),
          String::new(),
        )
        .await;
    }
  }
}

/// SIGHUP to the child's process group (it is a session leader on its own PTY), then
/// SIGKILL if it is still there after [`KILL_GRACE`].
async fn stop(child: &mut tokio::process::Child, pid: Option<u32>) -> &'static str {
  let group = pid
    .and_then(|pid| i32::try_from(pid).ok())
    .and_then(rustix::process::Pid::from_raw);
  if let Some(group) = group {
    let _ = rustix::process::kill_process_group(group, rustix::process::Signal::HUP);
  }
  if tokio::time::timeout(KILL_GRACE, child.wait()).await.is_ok() {
    return "hangup";
  }
  if let Some(group) = group {
    let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
  }
  let _ = child.start_kill();
  let _ = child.wait().await;
  "killed"
}

/// The exit status the client sees: the code, or 128 + the signal.
fn exit_code(status: ExitStatus) -> u32 {
  match (status.code(), status.signal()) {
    (Some(code), _) => u32::try_from(code).unwrap_or(255),
    (None, Some(signal)) => 128 + u32::try_from(signal).unwrap_or(0),
    (None, None) => 255,
  }
}

/// An environment variable a client may set: the locale and the colour hint.
fn allowed_env(name: &str, value: &str) -> bool {
  let name_ok = name == "LANG" || name == "COLORTERM" || name.starts_with("LC_");
  name_ok
    && name.len() <= 32
    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    && value.len() <= 128
    && value
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._-@+".contains(&b))
}

/// A `TERM` value safe to hand on; anything odd becomes `xterm-256color`.
fn sane_term(term: &str) -> String {
  if !term.is_empty()
    && term.len() <= 64
    && term
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
  {
    term.to_owned()
  } else {
    "xterm-256color".to_owned()
  }
}

fn clamp_size(value: u32) -> u16 {
  u16::try_from(value.clamp(1, 1000)).unwrap_or(1000)
}

impl server::Handler for Connection {
  type Error = russh::Error;

  async fn authentication_banner(&mut self) -> Result<Option<String>, Self::Error> {
    let mut banner = BANNER.to_owned();
    if let Some(Err(refusal)) = &self.slot {
      banner.push_str(&format!("  {}\n\n", refusal.message()));
    }
    Ok(Some(banner))
  }

  async fn auth_publickey(
    &mut self,
    _user: &str,
    key: VerifiedPublicKey<'_>,
  ) -> Result<Auth, Self::Error> {
    self.fingerprint = Some(key.public_key.fingerprint(HashAlg::Sha256).to_string());
    Ok(self.accept("publickey"))
  }

  async fn auth_keyboard_interactive<'a>(
    &'a mut self,
    _user: &str,
    _submethods: &str,
    _response: Option<Response<'a>>,
  ) -> Result<Auth, Self::Error> {
    // Zero prompts: the client logs in without asking its user anything.
    Ok(self.accept("keyboard-interactive"))
  }

  async fn channel_open_session(
    &mut self,
    channel: Channel<Msg>,
    reply: ChannelOpenHandle,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel.is_some() {
      tracing::info!(conn = self.id, "second session channel refused");
      reply
        .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
        .await;
      return Ok(());
    }
    self.channel = Some(channel.id());
    reply.accept().await;
    Ok(())
  }

  async fn pty_request(
    &mut self,
    channel: ChannelId,
    term: &str,
    col_width: u32,
    row_height: u32,
    _pix_width: u32,
    _pix_height: u32,
    _modes: &[(Pty, u32)],
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel != Some(channel) || self.started {
      return session.channel_failure(channel);
    }
    self.pty = Some(PtyRequest {
      term: sane_term(term),
      cols: clamp_size(col_width),
      rows: clamp_size(row_height),
    });
    session.channel_success(channel)
  }

  async fn env_request(
    &mut self,
    channel: ChannelId,
    variable_name: &str,
    variable_value: &str,
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.started || !allowed_env(variable_name, variable_value) || self.env.len() >= 16 {
      tracing::debug!(conn = self.id, name = variable_name, "env refused");
      return session.channel_failure(channel);
    }
    self
      .env
      .push((variable_name.to_owned(), variable_value.to_owned()));
    session.channel_success(channel)
  }

  async fn shell_request(
    &mut self,
    channel: ChannelId,
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel != Some(channel) || self.started {
      return session.channel_failure(channel);
    }
    self.started = true;
    session.channel_success(channel)?;
    self.start(channel, session.handle()).await;
    Ok(())
  }

  async fn exec_request(
    &mut self,
    channel: ChannelId,
    data: &[u8],
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    tracing::info!(conn = self.id, bytes = data.len(), "exec refused");
    session.channel_failure(channel)
  }

  async fn subsystem_request(
    &mut self,
    channel: ChannelId,
    name: &str,
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    tracing::info!(conn = self.id, subsystem = name, "subsystem refused");
    session.channel_failure(channel)
  }

  async fn x11_request(
    &mut self,
    channel: ChannelId,
    _single_connection: bool,
    _x11_auth_protocol: &str,
    _x11_auth_cookie: &str,
    _x11_screen_number: u32,
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    session.channel_failure(channel)
  }

  async fn agent_request(
    &mut self,
    _channel: ChannelId,
    _session: &mut Session,
  ) -> Result<bool, Self::Error> {
    Ok(false)
  }

  async fn tcpip_forward(
    &mut self,
    _address: &str,
    _port: &mut u32,
    _session: &mut Session,
  ) -> Result<bool, Self::Error> {
    tracing::info!(conn = self.id, "port forwarding refused");
    Ok(false)
  }

  async fn window_change_request(
    &mut self,
    channel: ChannelId,
    col_width: u32,
    row_height: u32,
    _pix_width: u32,
    _pix_height: u32,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel != Some(channel) {
      return Ok(());
    }
    let (cols, rows) = (clamp_size(col_width), clamp_size(row_height));
    if let Some(pty) = &mut self.pty {
      pty.cols = cols;
      pty.rows = rows;
    }
    self.touch();
    if let Some(to_child) = &self.to_child {
      let _ = to_child.send(Msg2Child::Resize { cols, rows }).await;
    }
    Ok(())
  }

  async fn data(
    &mut self,
    channel: ChannelId,
    data: &[u8],
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel != Some(channel) {
      return Ok(());
    }
    self.touch();
    if let Some(to_child) = &self.to_child {
      let _ = to_child.send(Msg2Child::Input(data.to_vec())).await;
    }
    Ok(())
  }

  async fn channel_close(
    &mut self,
    channel: ChannelId,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel == Some(channel) {
      // Dropping the sender tells the session task the client is gone.
      self.to_child = None;
    }
    Ok(())
  }

  async fn signal(
    &mut self,
    _channel: ChannelId,
    _signal: Sig,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_locale_and_colour_variables_pass() {
    assert!(allowed_env("LANG", "zh_CN.UTF-8"));
    assert!(allowed_env("LC_ALL", "en_US.UTF-8"));
    assert!(allowed_env("COLORTERM", "truecolor"));
    assert!(!allowed_env("LD_PRELOAD", "/tmp/x.so"));
    assert!(!allowed_env("COFFEE_SSH_FINGERPRINT", "SHA256:forged"));
    assert!(!allowed_env("PATH", "/tmp"));
    assert!(!allowed_env("LANG", "$(reboot)"));
  }

  #[test]
  fn odd_terms_are_replaced() {
    assert_eq!(sane_term("xterm-kitty"), "xterm-kitty");
    assert_eq!(sane_term(""), "xterm-256color");
    assert_eq!(sane_term("x;rm -rf"), "xterm-256color");
  }

  #[test]
  fn sizes_are_clamped() {
    assert_eq!(clamp_size(0), 1);
    assert_eq!(clamp_size(160), 160);
    assert_eq!(clamp_size(100_000), 1000);
  }

  #[test]
  fn exit_codes_follow_the_shell_convention() {
    assert_eq!(exit_code(ExitStatus::from_raw(0)), 0);
    assert_eq!(exit_code(ExitStatus::from_raw(3 << 8)), 3);
    assert_eq!(exit_code(ExitStatus::from_raw(9)), 137);
  }
}
