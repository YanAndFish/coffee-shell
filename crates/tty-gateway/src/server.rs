//! The SSH server: passwordless logins; the user name picks a route, and each session
//! starts that route's upstream on a PTY.

use std::{
  collections::HashMap,
  future::Future,
  io,
  net::SocketAddr,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use russh::{
  Channel, ChannelId, MethodKind, MethodSet, Pty, Sig,
  keys::{HashAlg, PrivateKey},
  server::{self, Auth, ChannelOpenHandle, Handle, Msg, Response, Session, VerifiedPublicKey},
};
use tokio::{net::TcpListener, sync::watch};

use crate::{
  config::{Route, SessionVars, Snapshot},
  limits::{Caps, Gate, Limits, Refusal},
  session::{
    Alive, Clock, Hangup, Link, SessionTask, disconnect, link, run_session, tell_and_close,
  },
  tasks::{self, Tracker},
};

/// Keepalives detect a client that vanished without closing its connection.
const KEEPALIVE: Duration = Duration::from_secs(30);
/// Client environment variables kept per session, at most.
const MAX_CLIENT_ENV: usize = 16;
/// What every session gets from the gateway's own environment.
const INHERITED_ENV: [&str; 5] = ["PATH", "HOME", "TMPDIR", "USER", "LOGNAME"];
/// A shutdown takes at most `shutdown_grace` and this; sockets still open this long
/// before its end are closed hard.
const DRAIN_EXTRA: Duration = Duration::from_secs(1);
const HARD_CLOSE_BEFORE_END: Duration = Duration::from_millis(300);

/// What the server shares between connections, for as long as it runs.
struct Shared {
  limits: Arc<Limits>,
  gate: Arc<Gate>,
  next_id: AtomicU64,
  connections: AtomicUsize,
  clock: Clock,
  /// `true` once the server shuts down.
  shutdown: watch::Sender<bool>,
  /// A second handle on every open connection's socket, to close it hard at the end of
  /// a shutdown.
  sockets: Mutex<HashMap<u64, std::net::TcpStream>>,
}

/// How far a connection got: the login watchdog reads it.
#[derive(Default)]
struct Progress {
  authenticated: AtomicBool,
  /// A shell was asked for (and started or refused with a message).
  settled: AtomicBool,
  /// A session task runs for this connection.
  session: Arc<AtomicBool>,
}

/// The russh settings for one snapshot.
fn russh_config(snapshot: &Snapshot, host_key: &PrivateKey) -> Arc<server::Config> {
  Arc::new(server::Config {
    server_id: russh::SshId::Standard(snapshot.server.server_id.clone().into()),
    methods: MethodSet::from(&[MethodKind::PublicKey, MethodKind::KeyboardInteractive][..]),
    // Nothing is secret here: the first `none` probe is answered at once, and no
    // other attempt is ever rejected.
    auth_rejection_time: Duration::from_millis(100),
    auth_rejection_time_initial: Some(Duration::ZERO),
    keys: vec![host_key.clone()],
    inactivity_timeout: None,
    keepalive_interval: Some(KEEPALIVE),
    keepalive_max: 3,
    // Only honoured by `run_on_socket`: `serve` sets it on each socket itself.
    nodelay: true,
    ..Default::default()
  })
}

/// Runs the SSH server on `listener` until `shutdown` resolves. Each connection takes
/// the snapshot in `snapshots` when it is accepted and keeps it to its end.
///
/// Then it drains, within the current `server.shutdown_grace` and a second: no new
/// connections; every session tells its client, hangs up its program and kills it
/// after the grace; connections without a session are dropped; sockets still open at the
/// end are closed hard; and it returns once every task is done (or the time is up).
pub async fn serve(
  listener: TcpListener,
  host_key: PrivateKey,
  snapshots: watch::Receiver<Arc<Snapshot>>,
  shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
  let (down, _) = watch::channel(false);
  let shared = Arc::new(Shared {
    limits: Limits::new(),
    gate: Gate::new(),
    next_id: AtomicU64::new(1),
    connections: AtomicUsize::new(0),
    clock: Clock(Instant::now()),
    shutdown: down,
    sockets: Mutex::new(HashMap::new()),
  });
  let (tracker, mut tasks) = tasks::tracker();
  // The russh settings of the snapshot last seen, rebuilt when it changes.
  let mut current: Option<(Arc<Snapshot>, Arc<server::Config>)> = None;
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
    let snapshot = snapshots.borrow().clone();
    let config = match &current {
      Some((seen, config)) if Arc::ptr_eq(seen, &snapshot) => config.clone(),
      _ => {
        let config = russh_config(&snapshot, &host_key);
        current = Some((snapshot.clone(), config.clone()));
        config
      }
    };
    let limits = snapshot.limits;
    let open = shared.connections.fetch_add(1, Ordering::AcqRel) + 1;
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let counted = Counted {
      shared: shared.clone(),
      id,
    };
    // Connections beyond this are dropped before the handshake: refused sessions still
    // get a polite message, but only up to a point.
    let max_connections = limits.max_sessions.saturating_mul(2).saturating_add(16);
    if open > max_connections {
      tracing::warn!(peer = %peer, open, "too many connections; dropped");
      continue;
    }
    // Twice the sessions: room for the extra ones to hear why they are refused.
    let ticket = match shared.gate.admit(
      peer.ip(),
      Instant::now(),
      limits.max_sessions_per_ip.saturating_mul(2),
      limits.connections_per_minute,
    ) {
      Ok(ticket) => ticket,
      Err((turned, again)) => {
        if !again {
          tracing::warn!(peer = %peer.ip(), reason = turned.reason(), "connections turned away");
        }
        continue;
      }
    };
    // Keystrokes go out at once (`Config.nodelay` is not applied by `run_stream`).
    let _ = socket.set_nodelay(true);
    let socket = match keep_handle(socket) {
      Ok((socket, kept)) => {
        lock(&shared.sockets).insert(id, kept);
        socket
      }
      Err(error) => {
        tracing::warn!(%error, "cannot keep a handle on the socket; dropped");
        continue;
      }
    };
    let progress = Arc::new(Progress::default());
    // From accept to a running session: handshake, login and the shell request. A client
    // that stalls anywhere in between (even before its SSH version line) is cut off.
    let deadline = tokio::time::Instant::now() + limits.auth_timeout;
    let handler = Connection::new(
      shared.clone(),
      snapshot,
      id,
      peer,
      progress.clone(),
      tracker.clone(),
    );
    let mut down = shared.shutdown.subscribe();
    tasks::spawn("connection", id, &tracker, async move {
      let _counted = counted;
      let _ticket = ticket;
      let session = tokio::select! {
        started = tokio::time::timeout_at(deadline, server::run_stream(config, socket, handler)) => {
          match started {
            Ok(Ok(session)) => session,
            Ok(Err(error)) => {
              tracing::debug!(conn = id, %error, "handshake failed");
              return;
            }
            Err(_) => {
              tracing::info!(conn = id, "handshake timed out");
              return;
            }
          }
        }
        () = shut(&mut down) => return,
      };
      let handle = session.handle();
      let watched = handle.clone();
      let watched_progress = progress.clone();
      let watchdog = tokio::spawn(async move {
        tokio::time::sleep_until(deadline).await;
        if watched_progress.settled.load(Ordering::Acquire) {
          return;
        }
        let why = if watched_progress.authenticated.load(Ordering::Acquire) {
          "no session started in time"
        } else {
          "login timed out"
        };
        tracing::info!(conn = id, "{why}");
        disconnect(&watched, why).await;
      });
      tokio::pin!(session);
      tokio::select! {
        ended = &mut session => {
          if let Err(error) = ended {
            tracing::debug!(conn = id, %error, "connection ended with an error");
          }
        }
        () = shut(&mut down) => {
          if progress.session.load(Ordering::Acquire) {
            // The session tells its client and ends the connection; the hard close at
            // the end of the drain bounds this.
            let _ = (&mut session).await;
          } else {
            disconnect(&handle, "shutting down").await;
          }
        }
      }
      watchdog.abort();
    });
  }

  // The drain.
  let started = Instant::now();
  let grace = snapshots.borrow().server.shutdown_grace;
  let end = started + grace + DRAIN_EXTRA;
  drop(listener);
  drop(tracker);
  shared.shutdown.send_replace(true);
  let open = shared.connections.load(Ordering::Acquire);
  tracing::info!(connections = open, "draining");
  let hard_close = end - HARD_CLOSE_BEFORE_END;
  let mut done = tasks
    .wait(hard_close.saturating_duration_since(Instant::now()))
    .await;
  if !done {
    let sockets: Vec<std::net::TcpStream> = lock(&shared.sockets).drain().map(|(_, s)| s).collect();
    tracing::info!(sockets = sockets.len(), "closing what is still open");
    for socket in sockets {
      let _ = socket.shutdown(std::net::Shutdown::Both);
    }
    done = tasks
      .wait(end.saturating_duration_since(Instant::now()))
      .await;
  }
  tracing::info!(
    done,
    millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    "stopped"
  );
  Ok(())
}

/// Splits off a second handle on `socket` (same connection), for a hard close later.
fn keep_handle(
  socket: tokio::net::TcpStream,
) -> io::Result<(tokio::net::TcpStream, std::net::TcpStream)> {
  let socket = socket.into_std()?;
  let kept = socket.try_clone()?;
  Ok((tokio::net::TcpStream::from_std(socket)?, kept))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Resolves once the server shuts down; never if it is gone without one.
async fn shut(down: &mut watch::Receiver<bool>) {
  if down.wait_for(|down| *down).await.is_err() {
    std::future::pending::<()>().await;
  }
}

/// One open TCP connection, counted (and its kept socket handle held) until it ends.
struct Counted {
  shared: Arc<Shared>,
  id: u64,
}

impl Drop for Counted {
  fn drop(&mut self) {
    lock(&self.shared.sockets).remove(&self.id);
    self.shared.connections.fetch_sub(1, Ordering::AcqRel);
  }
}

/// 客户端请求的终端设置。
#[derive(Clone, Debug)]
struct PtyRequest {
  /// 终端类型，传给子进程的 `TERM`。
  term: String,
  /// 终端列数。
  cols: u16,
  /// 终端行数。
  rows: u16,
  /// 删除光标左侧字符的按键值。`None` 使用系统默认值，255 表示禁用。
  erase: Option<u8>,
}

/// One SSH connection (russh calls it a session).
struct Connection {
  shared: Arc<Shared>,
  snapshot: Arc<Snapshot>,
  id: u64,
  peer: SocketAddr,
  connected: Instant,
  progress: Arc<Progress>,
  tracker: Tracker,
  /// The user name of the accepted login, as the client sent it (logged with `?`).
  user: Option<String>,
  /// The route that login picked.
  route: Option<Arc<Route>>,
  fingerprint: Option<String>,
  channel: Option<ChannelId>,
  pty: Option<PtyRequest>,
  env: Vec<(String, String)>,
  started: bool,
  link: Option<Link>,
  overflowed: bool,
  last_input: Arc<AtomicU64>,
}

impl Connection {
  fn new(
    shared: Arc<Shared>,
    snapshot: Arc<Snapshot>,
    id: u64,
    peer: SocketAddr,
    progress: Arc<Progress>,
    tracker: Tracker,
  ) -> Self {
    tracing::info!(conn = id, peer = %peer, "connected");
    let last_input = Arc::new(AtomicU64::new(shared.clock.millis()));
    Self {
      shared,
      snapshot,
      id,
      peer,
      connected: Instant::now(),
      progress,
      tracker,
      user: None,
      route: None,
      fingerprint: None,
      channel: None,
      pty: None,
      env: Vec::new(),
      started: false,
      link: None,
      overflowed: false,
      last_input,
    }
  }

  fn touch(&self) {
    self
      .last_input
      .store(self.shared.clock.millis(), Ordering::Relaxed);
  }

  fn caps(&self) -> Caps {
    Caps {
      max_sessions: self.snapshot.limits.max_sessions,
      max_sessions_per_ip: self.snapshot.limits.max_sessions_per_ip,
    }
  }

  /// The route of this connection: the login's (the default one before any).
  fn route(&self) -> Arc<Route> {
    self
      .route
      .clone()
      .unwrap_or_else(|| self.snapshot.default_route().clone())
  }

  /// Accepts the login of `user` by `method` and binds its route.
  fn accept(&mut self, user: &str, method: &'static str) -> Auth {
    let route = self.snapshot.route_for(user).clone();
    self.progress.authenticated.store(true, Ordering::Release);
    tracing::info!(
      conn = self.id,
      method,
      ?user,
      route = %route.name,
      fingerprint = self.fingerprint.as_deref().unwrap_or("-"),
      "authenticated"
    );
    self.user = Some(user.to_owned());
    self.route = Some(route);
    Auth::Accept
  }

  fn vars(&self, route: &Route) -> SessionVars {
    SessionVars {
      user: self.user.as_deref().and_then(SessionVars::sanitize_user),
      route: route.name.clone(),
      fingerprint: self.fingerprint.clone(),
      client_ip: Some(self.peer.ip()),
    }
  }

  /// The session's whole environment: a few variables of the gateway's own, `TERM`,
  /// `LANG` and what the client sent that the route accepts, then the route's entries.
  fn session_env(&self, route: &Route, pty: &PtyRequest) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    for name in INHERITED_ENV {
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
      if !client_may_set(route, name) {
        continue;
      }
      match name.as_str() {
        "LANG" if utf8(value) => lang = Some(value.clone()),
        "LANG" => {}
        _ => {
          env.retain(|(existing, _)| existing != name);
          env.push((name.clone(), value.clone()));
        }
      }
    }
    env.push(("LANG".into(), lang.unwrap_or_else(|| "C.UTF-8".into())));
    let vars = self.vars(route);
    for (name, template) in &route.env {
      if let Some(value) = template.render(&vars) {
        env.retain(|(existing, _)| existing != name);
        env.push((name.clone(), value));
      }
    }
    env
  }

  /// Hands the shell request to a session task of its own and returns at once: the
  /// place, the program's start and its retries all happen there. Input that arrives
  /// meanwhile is queued on the link.
  fn start(&mut self, channel: ChannelId, handle: Handle) {
    let route = self.route();
    let Some(pty) = self.pty.clone() else {
      tracing::info!(conn = self.id, route = %route.name, "shell without a pty refused");
      let message = route.messages.no_pty.clone();
      tasks::spawn("refusal", self.id, &self.tracker, async move {
        tell_and_close(&handle, channel, &message, 1).await;
      });
      return;
    };
    tracing::info!(
      conn = self.id,
      user = ?self.user.as_deref().unwrap_or(""),
      route = %route.name,
      term = %pty.term,
      size = format!("{}x{}", pty.cols, pty.rows),
      "session started"
    );
    let (link, link_end) = link(pty.cols, pty.rows);
    self.link = Some(link);
    let task = SessionTask {
      conn: self.id,
      env: self.session_env(&route, &pty),
      erase: pty.erase,
      route,
      link: link_end,
      handle,
      channel,
      last_input: self.last_input.clone(),
      clock: self.shared.clock,
      limits: self.shared.limits.clone(),
      ip: self.peer.ip(),
      caps: self.caps(),
      shutdown: self.shared.shutdown.subscribe(),
      shutdown_grace: self.snapshot.server.shutdown_grace,
      alive: Alive::new(self.progress.session.clone()),
    };
    tasks::spawn("session", self.id, &self.tracker, run_session(task));
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

/// Whether a client variable may be passed on: accepted by the route, and neither one
/// the gateway sets itself nor one of the route's own entries (which a client must never
/// be able to forge, even when the entry is absent for this session).
fn client_may_set(route: &Route, name: &str) -> bool {
  route.accept_env.iter().any(|pattern| pattern.matches(name))
    && !INHERITED_ENV.contains(&name)
    && name != "TERM"
    && !route.env.iter().any(|(own, _)| own == name)
}

/// The shape of a client variable the gateway takes at all.
fn plain_env(name: &str, value: &str) -> bool {
  !name.is_empty()
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
    // Before any user name: only the global banner and caps are known.
    let mut banner = self.snapshot.server.banner.clone();
    if let Err(refusal) = self.shared.limits.check(self.peer.ip(), self.caps()) {
      tracing::info!(
        conn = self.id,
        refused = refusal.reason(),
        "over the limit at login"
      );
      let messages = &self.snapshot.messages;
      let message = match refusal {
        Refusal::PerIp { max } => messages.full_per_ip(max),
        Refusal::Full { max } | Refusal::Route { max } => messages.full(max),
      };
      banner.push_str(&format!("  {message}\n\n"));
    }
    Ok((!banner.is_empty()).then_some(banner))
  }

  async fn auth_publickey(
    &mut self,
    user: &str,
    key: VerifiedPublicKey<'_>,
  ) -> Result<Auth, Self::Error> {
    self.fingerprint = Some(key.public_key.fingerprint(HashAlg::Sha256).to_string());
    Ok(self.accept(user, "publickey"))
  }

  async fn auth_keyboard_interactive<'a>(
    &'a mut self,
    user: &str,
    _submethods: &str,
    _response: Option<Response<'a>>,
  ) -> Result<Auth, Self::Error> {
    // Zero prompts: the client logs in without asking its user anything.
    Ok(self.accept(user, "keyboard-interactive"))
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
    modes: &[(Pty, u32)],
    session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel != Some(channel) || self.started {
      return session.channel_failure(channel);
    }
    self.pty = Some(PtyRequest {
      term: sane_term(term),
      cols: clamp_size(col_width),
      rows: clamp_size(row_height),
      erase: modes
        .iter()
        .filter(|(mode, _)| *mode == Pty::VERASE)
        .filter_map(|(_, value)| u8::try_from(*value).ok())
        .next_back(),
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
    let route = self.route();
    if self.started
      || !plain_env(variable_name, variable_value)
      || !client_may_set(&route, variable_name)
      || self.env.len() >= MAX_CLIENT_ENV
    {
      tracing::debug!(conn = self.id, name = ?variable_name, "env refused");
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
    self.start(channel, session.handle());
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
    tracing::info!(conn = self.id, subsystem = ?name, "subsystem refused");
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
    if let Some(link) = &self.link {
      link.resize(cols, rows);
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
    if let Some(link) = &self.link
      && !link.input(data)
      && !std::mem::replace(&mut self.overflowed, true)
    {
      tracing::debug!(conn = self.id, "input over the budget");
    }
    Ok(())
  }

  async fn channel_eof(
    &mut self,
    channel: ChannelId,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel == Some(channel)
      && let Some(link) = &self.link
    {
      link.hang_up(Hangup::Eof);
    }
    Ok(())
  }

  async fn channel_close(
    &mut self,
    channel: ChannelId,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    if self.channel == Some(channel)
      && let Some(link) = &self.link
    {
      link.hang_up(Hangup::Closed);
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
  use crate::config::load_str;

  fn route(toml: &str) -> Arc<Route> {
    let (snapshot, _) = load_str(toml, |_| None, None).unwrap();
    snapshot.default_route().clone()
  }

  #[test]
  fn only_accepted_plain_variables_pass() {
    let app = route(
      r#"
[routes.app]
command = ["/bin/true"]
env = { APP_FINGERPRINT = "${fingerprint}" }
"#,
    );
    let ok = |name: &str, value: &str| plain_env(name, value) && client_may_set(&app, name);
    assert!(ok("LANG", "zh_CN.UTF-8"));
    assert!(ok("LC_ALL", "en_US.UTF-8"));
    assert!(ok("COLORTERM", "truecolor"));
    assert!(!ok("LD_PRELOAD", "/tmp/x.so"));
    assert!(!ok("APP_FINGERPRINT", "SHA256:forged"));
    assert!(!ok("PATH", "/tmp"));
    assert!(!ok("LANG", "$(reboot)"));

    // Even a route that accepts everything keeps its own entries and the gateway's.
    let open = route(
      r#"
[routes.app]
command = ["/bin/true"]
accept_env = ["*"]
env = { APP_FINGERPRINT = "${fingerprint}" }
"#,
    );
    assert!(client_may_set(&open, "ANYTHING"));
    assert!(!client_may_set(&open, "APP_FINGERPRINT"));
    assert!(!client_may_set(&open, "PATH"));
    assert!(!client_may_set(&open, "TERM"));
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
}
