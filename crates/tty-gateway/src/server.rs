//! The SSH server: passwordless logins; the user name picks a route, and each session
//! starts that route's upstream on a PTY.

use std::{
  future::Future,
  io,
  net::SocketAddr,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use russh::{
  Channel, ChannelId, MethodKind, MethodSet, Pty, Sig,
  keys::{HashAlg, PrivateKey},
  server::{self, Auth, ChannelOpenHandle, Handle, Msg, Response, Session, VerifiedPublicKey},
};
use tokio::{
  net::TcpListener,
  sync::{mpsc, watch},
};

use crate::{
  config::{Route, SessionVars, Snapshot},
  limits::{Caps, Gate, Limits, Refusal},
  session::{Clock, SessionTask, ToUpstream, run_session, tell_and_close},
  upstream::Launch,
};

/// Keepalives detect a client that vanished without closing its connection.
const KEEPALIVE: Duration = Duration::from_secs(30);
/// Tries at starting a session's upstream, and the pause between them.
const SPAWN_ATTEMPTS: u32 = 3;
const SPAWN_RETRY: Duration = Duration::from_millis(150);
/// Client environment variables kept per session, at most.
const MAX_CLIENT_ENV: usize = 16;
/// What every session gets from the gateway's own environment.
const INHERITED_ENV: [&str; 5] = ["PATH", "HOME", "TMPDIR", "USER", "LOGNAME"];

/// What the server shares between connections, for as long as it runs.
struct Shared {
  limits: Arc<Limits>,
  gate: Arc<Gate>,
  next_id: AtomicU64,
  connections: AtomicUsize,
  clock: Clock,
}

/// How far a connection got: the login watchdog reads it.
#[derive(Default)]
struct Progress {
  authenticated: AtomicBool,
  /// A shell was asked for (and started or refused with a message).
  settled: AtomicBool,
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
    nodelay: true,
    ..Default::default()
  })
}

/// Runs the SSH server on `listener` until `shutdown` resolves. Each connection takes
/// the snapshot in `snapshots` when it is accepted and keeps it to its end.
pub async fn serve(
  listener: TcpListener,
  host_key: PrivateKey,
  snapshots: watch::Receiver<Arc<Snapshot>>,
  shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
  let shared = Arc::new(Shared {
    limits: Limits::new(),
    gate: Gate::new(),
    next_id: AtomicU64::new(1),
    connections: AtomicUsize::new(0),
    clock: Clock(Instant::now()),
  });
  // The russh settings of the snapshot last seen, rebuilt when it changes.
  let mut current: Option<(Arc<Snapshot>, Arc<server::Config>)> = None;
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
    let counted = Counted(shared.clone());
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
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let progress = Arc::new(Progress::default());
    // From accept to a running session: handshake, login and the shell request. A client
    // that stalls anywhere in between (even before its SSH version line) is cut off.
    let deadline = tokio::time::Instant::now() + limits.auth_timeout;
    let handler = Connection::new(shared.clone(), snapshot, id, peer, progress.clone());
    tokio::spawn(async move {
      let _counted = counted;
      let _ticket = ticket;
      let session = match tokio::time::timeout_at(
        deadline,
        server::run_stream(config, socket, handler),
      )
      .await
      {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => {
          tracing::debug!(conn = id, %error, "handshake failed");
          return;
        }
        Err(_) => {
          tracing::info!(conn = id, "handshake timed out");
          return;
        }
      };
      let handle = session.handle();
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

/// One SSH connection (russh calls it a session).
struct Connection {
  shared: Arc<Shared>,
  snapshot: Arc<Snapshot>,
  id: u64,
  peer: SocketAddr,
  connected: Instant,
  progress: Arc<Progress>,
  /// The user name of the accepted login, as the client sent it (logged with `?`).
  user: Option<String>,
  /// The route that login picked.
  route: Option<Arc<Route>>,
  fingerprint: Option<String>,
  channel: Option<ChannelId>,
  pty: Option<PtyRequest>,
  env: Vec<(String, String)>,
  started: bool,
  to_upstream: Option<mpsc::Sender<ToUpstream>>,
  last_input: Arc<AtomicU64>,
}

impl Connection {
  fn new(
    shared: Arc<Shared>,
    snapshot: Arc<Snapshot>,
    id: u64,
    peer: SocketAddr,
    progress: Arc<Progress>,
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
      user: None,
      route: None,
      fingerprint: None,
      channel: None,
      pty: None,
      env: Vec::new(),
      started: false,
      to_upstream: None,
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

  /// Tells the client `message` on the channel, then ends it with `status`, on a task
  /// of its own: a handler callback must not wait on the session loop it runs in.
  fn refuse(handle: Handle, channel: ChannelId, message: String, status: u32) {
    tokio::spawn(async move { tell_and_close(&handle, channel, &message, status).await });
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

  async fn start(&mut self, channel: ChannelId, handle: Handle) {
    let route = self.route();
    let messages = &route.messages;
    let Some(pty) = self.pty.clone() else {
      tracing::info!(conn = self.id, route = %route.name, "shell without a pty refused");
      Self::refuse(handle, channel, messages.no_pty.clone(), 1);
      return;
    };
    // The place is taken only now: a connection that is still logging in holds none.
    let slot = match self.shared.limits.acquire(
      self.peer.ip(),
      &route.name,
      self.caps(),
      route.limits.max_sessions,
    ) {
      Ok(slot) => slot,
      Err(refusal) => {
        tracing::warn!(
          conn = self.id,
          route = %route.name,
          refused = refusal.reason(),
          "session refused"
        );
        Self::refuse(handle, channel, refusal_message(&route, refusal), 1);
        return;
      }
    };
    let launch = Launch {
      env: self.session_env(&route, &pty),
      cols: pty.cols,
      rows: pty.rows,
    };
    // A start can fail for a moment (no process or PTY to spare under load, the program
    // being replaced): try again shortly before giving up on the session.
    let mut attempt = 1;
    let running = loop {
      match route.upstream.start(&launch) {
        Ok(running) => break running,
        Err(error) if attempt < SPAWN_ATTEMPTS => {
          tracing::warn!(
            conn = self.id,
            route = %route.name,
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
            route = %route.name,
            upstream = %route.upstream.describe(),
            attempt,
            %error,
            "could not start the session program"
          );
          Self::refuse(handle, channel, messages.unavailable.clone(), 1);
          return;
        }
      }
    };
    tracing::info!(
      conn = self.id,
      user = ?self.user.as_deref().unwrap_or(""),
      route = %route.name,
      pid = running.pid().unwrap_or_default(),
      term = %pty.term,
      size = format!("{}x{}", pty.cols, pty.rows),
      active = self.shared.limits.active(),
      "session started"
    );
    let (to_upstream, from_connection) = mpsc::channel(64);
    self.to_upstream = Some(to_upstream);
    tokio::spawn(run_session(SessionTask {
      conn: self.id,
      route: route.clone(),
      running,
      from_connection,
      handle,
      channel,
      last_input: self.last_input.clone(),
      clock: self.shared.clock,
      slot,
    }));
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

/// What the client is told when its session is refused, in the route's words.
fn refusal_message(route: &Route, refusal: Refusal) -> String {
  let messages = &route.messages;
  match refusal {
    Refusal::Full { max } => messages.full(max),
    Refusal::PerIp { max } => messages.full_per_ip(max),
    Refusal::Route { max } => messages.full_route(max),
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
    if let Some(to_upstream) = &self.to_upstream {
      let _ = to_upstream.send(ToUpstream::Resize { cols, rows }).await;
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
    if let Some(to_upstream) = &self.to_upstream {
      let _ = to_upstream.send(ToUpstream::Input(data.to_vec())).await;
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
      self.to_upstream = None;
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
