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
  sync::{mpsc, oneshot, watch},
  task::JoinHandle,
};

use crate::{
  config::Config,
  limits::{Gate, Limits, Slot},
};

/// The notice every client sees before logging in (SSH auth banner).
pub const BANNER: &str = "\n  ◆ROAST/TTY 终端咖啡馆\n  演示环境 · 不是真实商店 · 不收款、不发货 · 请勿使用真实密码、地址或电话\n\n";

/// How long a session's child has to leave after SIGHUP or SIGTERM before it is killed.
const KILL_GRACE: Duration = Duration::from_millis(1000);
/// On shutdown: how long the sessions have to say goodbye (their program restores the
/// client's terminal and leaves, the notice goes out) before the server stops anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(3000);
/// What a client sees when the server stops under its session.
const STOPPING_NOTICE: &str = "ROAST/TTY 服务正在停止，连接已关闭。请稍后重新连接。";
/// Keepalives detect a client that vanished without closing its connection.
const KEEPALIVE: Duration = Duration::from_secs(30);
/// Tries at starting a session's program, and the pause between them.
const SPAWN_ATTEMPTS: u32 = 3;
const SPAWN_RETRY: Duration = Duration::from_millis(150);
/// Undoes what a session program that died may have left on the client's terminal: kitty
/// keys, mouse, paste and focus reports, alternate scroll, a synchronized update, the
/// alternate screen, a hidden cursor, colours.
const TERMINAL_RESET: &str = "\x1b[<u\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?2004l\x1b[?1004l\x1b[?1007l\x1b[?2026l\x1b[?1049l\x1b[?25h\x1b[0m";

/// What the server shares between connections.
struct Shared {
  config: Config,
  limits: Arc<Limits>,
  gate: Arc<Gate>,
  next_id: AtomicU64,
  connections: AtomicUsize,
  started: Instant,
  /// `true` once the server stops: sessions and connections end on their own, politely.
  stopping: watch::Sender<bool>,
}

/// How far a connection got: the login watchdog and the shutdown read it.
#[derive(Default)]
struct Progress {
  authenticated: AtomicBool,
  /// A shell was asked for (and started or refused with a message).
  settled: AtomicBool,
  /// Its session program runs (or is still saying goodbye).
  running: AtomicBool,
}

/// Runs the SSH server on `listener` until `shutdown` resolves. Then it takes no new
/// connection, and every session ends as a client leaving it would not: its program gets
/// SIGTERM and restores the client's terminal (leaves the alternate screen, …), the client
/// reads why the connection closes, and only then the connection goes. It returns once all
/// connections are gone, or after [`SHUTDOWN_GRACE`].
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
    // Twice the sessions: room for the extra ones to hear why they are refused.
    gate: Gate::new(
      config.max_sessions_per_ip.saturating_mul(2),
      config.connections_per_minute,
    ),
    config,
    next_id: AtomicU64::new(1),
    connections: AtomicUsize::new(0),
    started: Instant::now(),
    stopping: watch::Sender::new(false),
  });
  // Connections beyond this are dropped before the handshake: refused sessions still
  // get a polite message, but only up to a point.
  let max_connections = shared.config.max_sessions * 2 + 16;
  tokio::pin!(shutdown);
  loop {
    let (socket, peer) = tokio::select! {
      () = &mut shutdown => break,
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
    let ticket = match shared.gate.admit(peer.ip(), Instant::now()) {
      Ok(ticket) => ticket,
      Err((turned, again)) => {
        if !again {
          tracing::warn!(peer = %peer.ip(), reason = turned.reason(), "connections turned away");
        }
        continue;
      }
    };
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let progress = Arc::new(Progress::default());
    let handler = Connection::new(shared.clone(), id, peer, progress.clone());
    let config = russh_config.clone();
    let mut stopping = shared.stopping.subscribe();
    // From accept to a running session: handshake, login and the shell request. A client
    // that stalls anywhere in between (even before its SSH version line) is cut off.
    let deadline = tokio::time::Instant::now() + shared.config.auth_timeout;
    tokio::spawn(async move {
      let _counted = counted;
      let _ticket = ticket;
      let handshake =
        tokio::time::timeout_at(deadline, server::run_stream(config, socket, handler));
      let session = tokio::select! {
        result = handshake => match result {
          Ok(Ok(session)) => session,
          Ok(Err(error)) => {
            tracing::debug!(conn = id, %error, "handshake failed");
            return;
          }
          Err(_) => {
            tracing::info!(conn = id, "handshake timed out");
            return;
          }
        },
        // Nothing on the client's screen yet: just go.
        () = stopped(&mut stopping) => return,
      };
      let handle = session.handle();
      let goodbye = handle.clone();
      let running = progress.clone();
      let watchdog = tokio::spawn(async move {
        tokio::time::sleep_until(deadline).await;
        if progress.settled.load(Ordering::Acquire) {
          return;
        }
        let why = if progress.authenticated.load(Ordering::Acquire) {
          "no session started in time"
        } else {
          "login timed out"
        };
        tracing::info!(conn = id, "{why}");
        let _ = handle
          .disconnect(russh::Disconnect::ByApplication, why.into(), String::new())
          .await;
      });
      tokio::pin!(session);
      let result = tokio::select! {
        result = &mut session => result,
        () = stopped(&mut stopping) => {
          // The session (if any) says goodbye on its own channel first; then the connection.
          let until = tokio::time::Instant::now() + SHUTDOWN_GRACE;
          while running.running.load(Ordering::Acquire) && tokio::time::Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(20)).await;
          }
          let _ = goodbye
            .disconnect(russh::Disconnect::ByApplication, "server stopping".into(), String::new())
            .await;
          tokio::time::timeout_at(until, &mut session)
            .await
            .unwrap_or(Ok(()))
        }
      };
      if let Err(error) = result {
        tracing::debug!(conn = id, %error, "connection ended with an error");
      }
      watchdog.abort();
    });
  }
  // Stopping: every connection ends itself (see above); wait for them, not for ever.
  shared.stopping.send_replace(true);
  let open = shared.connections.load(Ordering::Acquire);
  if open > 0 {
    tracing::info!(open, "closing the connections");
  }
  let until = Instant::now() + SHUTDOWN_GRACE + Duration::from_millis(500);
  while shared.connections.load(Ordering::Acquire) > 0 && Instant::now() < until {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  let left = shared.connections.load(Ordering::Acquire);
  if left > 0 {
    tracing::warn!(left, "connections still open at shutdown");
  }
  Ok(())
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
  progress: Arc<Progress>,
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
  fn new(shared: Arc<Shared>, id: u64, peer: SocketAddr, progress: Arc<Progress>) -> Self {
    tracing::info!(conn = id, peer = %peer, "connected");
    let last_input = Arc::new(AtomicU64::new(shared.millis()));
    Self {
      shared,
      id,
      peer,
      connected: Instant::now(),
      progress,
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
    self.progress.authenticated.store(true, Ordering::Release);
    tracing::info!(
      conn = self.id,
      method,
      fingerprint = self.fingerprint.as_deref().unwrap_or("-"),
      "authenticated"
    );
    Auth::Accept
  }

  /// Tells the client `message` on the channel, then ends it with `status`, on a task
  /// of its own: a handler callback must not wait on the session loop it runs in.
  fn refuse(handle: Handle, channel: ChannelId, message: String, status: u32) {
    tokio::spawn(async move { tell_and_close(&handle, channel, &message, status).await });
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
        handle,
        channel,
        "ROAST/TTY 需要一个终端：请用 `ssh -t`，不要加 -T。".into(),
        1,
      );
      return;
    };
    // The place is taken only now: a connection that is still logging in holds none.
    let slot = match self.shared.limits.acquire(self.peer.ip()) {
      Ok(slot) => slot,
      Err(refusal) => {
        tracing::warn!(
          conn = self.id,
          refused = refusal.reason(),
          "session refused"
        );
        Self::refuse(handle, channel, refusal.message(), 1);
        return;
      }
    };
    let env = self.child_env(&pty);
    // A start can fail for a moment (no process or PTY to spare under load, the program
    // being replaced): try again shortly before giving up on the session.
    let mut attempt = 1;
    let (child, pty_master) = loop {
      match spawn_child(&self.shared.config, &pty, env.clone()) {
        Ok(spawned) => break spawned,
        Err(error) if attempt < SPAWN_ATTEMPTS => {
          tracing::warn!(
            conn = self.id,
            attempt,
            %error,
            "could not start the session program; trying again"
          );
          tokio::time::sleep(SPAWN_RETRY).await;
          attempt += 1;
        }
        Err(error) => {
          tracing::error!(
            conn = self.id,
            bin = %self.shared.config.tui_bin.display(),
            attempt,
            %error,
            "could not start the session program"
          );
          Self::refuse(
            handle,
            channel,
            "ROAST/TTY 暂时无法启动，请稍后再试。".into(),
            1,
          );
          return;
        }
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
    self.progress.running.store(true, Ordering::Release);
    tokio::spawn(run_session(SessionTask {
      conn: self.id,
      child,
      pty: pty_master,
      from_connection,
      handle,
      channel,
      last_input: self.last_input.clone(),
      shared: self.shared.clone(),
      progress: self.progress.clone(),
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
  progress: Arc<Progress>,
  _slot: Slot,
}

/// How a session ended.
enum End {
  Exited(io::Result<ExitStatus>),
  Disconnected,
  Idle,
  Stopping,
}

/// Clears [`Progress::running`] when the session task ends, however it ends.
struct Running(Arc<Progress>);

impl Drop for Running {
  fn drop(&mut self) {
    self.0.running.store(false, Ordering::Release);
  }
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
    progress,
    _slot,
  } = task;
  let _running = Running(progress);
  let mut stopping = shared.stopping.subscribe();
  let started = Instant::now();
  let pid = child.id();
  let (mut reader, mut writer) = pty.into_split();

  // The idle clock runs from the program's first output: the time a client takes to
  // connect, and the program to start, never counts against the session.
  let shown = Arc::new(AtomicBool::new(false));
  let output = handle.clone();
  let (first_output, clock, idle_since) = (shown.clone(), shared.clone(), last_input.clone());
  let mut pump: JoinHandle<()> = tokio::spawn(async move {
    let mut buffer = vec![0u8; 32 * 1024];
    loop {
      match reader.read(&mut buffer).await {
        Ok(0) | Err(_) => break,
        Ok(read) => {
          if !first_output.load(Ordering::Relaxed) {
            idle_since.store(clock.millis(), Ordering::Relaxed);
            first_output.store(true, Ordering::Release);
          }
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
  // How long ago the last input (or the first output) was, once there was output.
  let quiet = || {
    shown.load(Ordering::Acquire).then(|| {
      let since = shared
        .millis()
        .saturating_sub(last_input.load(Ordering::Relaxed));
      Duration::from_millis(since)
    })
  };
  let end = loop {
    let check = match (idle, quiet()) {
      (Some(idle), Some(quiet)) => idle.saturating_sub(quiet),
      // Nothing shown yet: look again after a full period.
      (Some(idle), None) => idle,
      (None, _) => Duration::from_secs(3600),
    };
    tokio::select! {
      status = child.wait() => break End::Exited(status),
      _ = &mut gone => break End::Disconnected,
      () = stopped(&mut stopping) => break End::Stopping,
      () = tokio::time::sleep(check) => {
        if let (Some(idle), Some(quiet)) = (idle, quiet()) && quiet >= idle {
          break End::Idle;
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
      let seconds = started.elapsed().as_secs();
      if code == 0 {
        tracing::info!(conn, ?pid, code, seconds, "session ended: program exited");
      } else {
        // It crashed or gave up: it may have left the client's terminal in its modes,
        // and a bare close would leave the user guessing.
        tracing::warn!(conn, ?pid, code, seconds, "session ended: program failed");
        let notice = format!(
          "{TERMINAL_RESET}\r\nROAST/TTY 出错了，会话已结束（代码 {code}）。请重新连接；如果一直这样，请稍后再试。\r\n"
        );
        let _ = handle.data(channel, Bytes::from(notice.into_bytes())).await;
      }
      let _ = handle.exit_status_request(channel, code).await;
      let _ = handle.eof(channel).await;
      let _ = handle.close(channel).await;
    }
    End::Disconnected => {
      feeder.abort();
      let how = stop(&mut child, pid, rustix::process::Signal::HUP).await;
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
      let how = stop(&mut child, pid, rustix::process::Signal::HUP).await;
      let _ = tokio::time::timeout(Duration::from_millis(300), &mut pump).await;
      pump.abort();
      let minutes = idle.map_or(0, |idle| idle.as_secs() / 60);
      tracing::info!(conn, ?pid, how, "session ended: idle");
      tell_and_close(
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
    End::Stopping => {
      feeder.abort();
      // SIGTERM: the program restores the client's terminal and leaves (coffee-host does
      // on SIGHUP, SIGTERM and SIGINT); its last output, that restore, is still in the PTY.
      let how = stop(&mut child, pid, rustix::process::Signal::TERM).await;
      let drained = tokio::time::timeout(Duration::from_millis(500), &mut pump)
        .await
        .is_ok();
      pump.abort();
      tracing::info!(
        conn,
        ?pid,
        how,
        seconds = started.elapsed().as_secs(),
        "session ended: server stopping"
      );
      if how == "killed" || !drained {
        // Its own restore may be missing: undo what it may have left.
        let _ = handle
          .data(channel, Bytes::from_static(TERMINAL_RESET.as_bytes()))
          .await;
      }
      // The connection task disconnects once this task is done.
      tell_and_close(&handle, channel, STOPPING_NOTICE, 143).await;
    }
  }
}

/// Resolves once the server stops (at once if it already did).
async fn stopped(stopping: &mut watch::Receiver<bool>) {
  // The borrow `wait_for` gives back must not live on in a `select!` arm.
  let _ = stopping.wait_for(|stopping| *stopping).await;
}

/// Writes `message` on the channel, then ends it with `status`.
async fn tell_and_close(handle: &Handle, channel: ChannelId, message: &str, status: u32) {
  let text = format!("\r\n{message}\r\n");
  let _ = handle.data(channel, Bytes::from(text.into_bytes())).await;
  let _ = handle.exit_status_request(channel, status).await;
  let _ = handle.eof(channel).await;
  let _ = handle.close(channel).await;
}

/// `signal` (SIGHUP: the client left; SIGTERM: the server stops) to the child's process
/// group (it is a session leader on its own PTY), then SIGKILL if it is still there after
/// [`KILL_GRACE`].
async fn stop(
  child: &mut tokio::process::Child,
  pid: Option<u32>,
  signal: rustix::process::Signal,
) -> &'static str {
  let group = pid
    .and_then(|pid| i32::try_from(pid).ok())
    .and_then(rustix::process::Pid::from_raw);
  if let Some(group) = group {
    let _ = rustix::process::kill_process_group(group, signal);
  }
  if tokio::time::timeout(KILL_GRACE, child.wait()).await.is_ok() {
    return if signal == rustix::process::Signal::TERM {
      "terminated"
    } else {
      "hangup"
    };
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
    if let Err(refusal) = self.shared.limits.check(self.peer.ip()) {
      tracing::info!(
        conn = self.id,
        refused = refusal.reason(),
        "over the limit at login"
      );
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
    self.progress.settled.store(true, Ordering::Release);
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
