//! What tests of `tty-gateway` (and of the programs behind it) share: an in-process
//! server on an ephemeral port, a russh client with a `vt100` screen, and stand-in
//! session programs.

use std::{
  net::SocketAddr,
  path::{Path, PathBuf},
  process::Command,
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};

use russh::{
  ChannelMsg,
  client::{self, Handle},
  keys::{Algorithm, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, ssh_key},
};
use tokio::{
  net::TcpListener,
  sync::{oneshot, watch},
  task::JoinHandle,
};
use tty_gateway::{ConfigError, Snapshot, load_str, serve};

/// A fresh, empty directory for one test.
pub fn temp_dir(name: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!(
    "tty-gateway-test-{}-{name}-{n}",
    std::process::id()
  ));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

/// `text` as a TOML basic string (quoted and escaped).
pub fn quote(text: &str) -> String {
  let mut out = String::from("\"");
  for c in text.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
      c => out.push(c),
    }
  }
  out.push('"');
  out
}

/// A path as a TOML string.
pub fn quote_path(path: &Path) -> String {
  quote(&path.to_string_lossy())
}

fn write_script(path: &Path, text: &str) {
  use std::os::unix::fs::PermissionsExt;
  std::fs::write(path, text).unwrap();
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A stand-in session program `dir/name`: prints `PROBE=<name>`, its pid, `NAME=value`
/// (or `NAME=none`) for each of `vars`, and its terminal size, then its size again on
/// every SIGWINCH. Each on a line of its own (see [`field`]).
pub fn probe_bin(dir: &Path, name: &str, vars: &[&str]) -> PathBuf {
  let path = dir.join(name);
  let mut script = format!("#!/bin/sh\necho \"PROBE={name}\"\necho \"PID=$$\"\n");
  for var in vars {
    assert!(
      var.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
      "{var}"
    );
    script.push_str(&format!("echo \"{var}=${{{var}:-none}}\"\n"));
  }
  script.push_str(
    "trap 'echo \"SIZE=$(stty size)\"' WINCH\necho \"SIZE=$(stty size)\"\nwhile :; do sleep 0.05; done\n",
  );
  write_script(&path, &script);
  path
}

/// An executable `/bin/sh` script `dir/name` with `body` after the `#!` line.
pub fn script_bin(dir: &Path, name: &str, body: &str) -> PathBuf {
  let path = dir.join(name);
  write_script(&path, &format!("#!/bin/sh\n{body}"));
  path
}

/// A stand-in session program that prints a line and then dies of SIGSEGV, as a crashed
/// program would.
pub fn crash_bin(dir: &Path) -> PathBuf {
  let path = dir.join("crash.sh");
  write_script(
    &path,
    "#!/bin/sh\necho \"PID=$$\"\nsleep 0.2\nkill -SEGV $$\n",
  );
  path
}

/// A server running in this process on 127.0.0.1 (an ephemeral port) with a host key
/// of its own; its configuration is TOML text, with `server.listen` and
/// `server.host_key` overridden.
pub struct TestServer {
  pub addr: SocketAddr,
  pub dir: PathBuf,
  env: Vec<(String, String)>,
  exe_dir: Option<PathBuf>,
  snapshots: watch::Sender<Arc<Snapshot>>,
  stop: Option<oneshot::Sender<()>>,
  task: Option<JoinHandle<()>>,
}

impl TestServer {
  /// Starts a server on `toml`, with an empty gateway environment for `${env:…}`.
  pub async fn start(toml: &str) -> Self {
    Self::start_with_env(toml, &[]).await
  }

  /// Starts a server on `toml`; `env` is the gateway's environment for `${env:…}`.
  pub async fn start_with_env(toml: &str, env: &[(&str, &str)]) -> Self {
    Self::start_with_exe_dir(toml, env, None).await
  }

  /// [`TestServer::start_with_env`], with bare program names (`command = ["prog"]`)
  /// looked for in `exe_dir` first, as next to a real gateway's executable (else only on
  /// `PATH`). E.g. the directory of a `CARGO_BIN_EXE_*` program, to run a shipped
  /// configuration unchanged.
  pub async fn start_with_exe_dir(
    toml: &str,
    env: &[(&str, &str)],
    exe_dir: Option<&Path>,
  ) -> Self {
    // The server's log on stderr (uncaptured) when asked for, e.g. TTY_GATEWAY_LOG=info.
    if std::env::var_os("TTY_GATEWAY_LOG").is_some() {
      tty_gateway::init_logging();
    }
    let dir = temp_dir("server");
    let mut env: Vec<(String, String)> = env
      .iter()
      .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
      .collect();
    env.push(("TTY_GATEWAY_ADDR".into(), "127.0.0.1:0".into()));
    env.push((
      "TTY_GATEWAY_HOST_KEY".into(),
      dir.join("host_key").to_string_lossy().into_owned(),
    ));
    let exe_dir = exe_dir.map(Path::to_owned);
    let snapshot = load(toml, &env, exe_dir.as_deref())
      .unwrap_or_else(|error| panic!("bad test config: {error}"));
    let (key, _) = tty_gateway::hostkey::load_or_generate(&snapshot.server.host_key).unwrap();
    let listener = TcpListener::bind(snapshot.server.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (snapshots, receiver) = watch::channel(Arc::new(snapshot));
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
      let _ = serve(listener, key, receiver, async {
        let _ = stopped.await;
      })
      .await;
    });
    Self {
      addr,
      dir,
      env,
      exe_dir,
      snapshots,
      stop: Some(stop),
      task: Some(task),
    }
  }

  /// Puts `toml` in force for new connections (as a reload would); open ones keep
  /// theirs. A configuration that does not load changes nothing.
  pub fn reload(&self, toml: &str) -> Result<(), ConfigError> {
    let snapshot = load(toml, &self.env, self.exe_dir.as_deref())?;
    self.snapshots.send_replace(Arc::new(snapshot));
    Ok(())
  }

  /// Shuts the server down as a SIGTERM would (the whole drain) and waits for `serve`
  /// to return, at most 10 s; how long it took (10 s if it did not return).
  pub async fn shutdown(mut self) -> Duration {
    let started = Instant::now();
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
    if let Some(task) = self.task.take()
      && tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .is_err()
    {
      return Duration::from_secs(10);
    }
    started.elapsed()
  }
}

fn load(
  toml: &str,
  env: &[(String, String)],
  exe_dir: Option<&Path>,
) -> Result<Snapshot, ConfigError> {
  let lookup = |name: &str| {
    env
      .iter()
      .rev()
      .find(|(key, _)| key == name)
      .map(|(_, value)| value.clone())
  };
  load_str(toml, lookup, exe_dir).map(|(snapshot, _)| snapshot)
}

impl Drop for TestServer {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
    let _ = std::fs::remove_dir_all(&self.dir);
  }
}

/// The client side: trusts any host key, remembers the auth banner.
pub struct Client {
  pub banner: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Client {
  type Error = russh::Error;

  async fn check_server_key(&mut self, _key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
    Ok(true)
  }

  async fn auth_banner(
    &mut self,
    banner: &str,
    _session: &mut client::Session,
  ) -> Result<(), Self::Error> {
    *self.banner.lock().unwrap() = Some(banner.to_owned());
    Ok(())
  }
}

/// How a test client logs in.
pub enum Login {
  /// No key: keyboard-interactive, answering whatever is asked (nothing).
  Keyless,
  /// This key.
  Key(Box<PrivateKey>),
}

impl Login {
  /// A new ed25519 key, and its `SHA256:…` fingerprint.
  pub fn new_key() -> (Self, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fingerprint = key
      .public_key()
      .fingerprint(ssh_key::HashAlg::Sha256)
      .to_string();
    (Self::Key(Box::new(key)), fingerprint)
  }
}

/// [`connect_as`] the user `guest`.
pub async fn connect(addr: SocketAddr, login: Login) -> (Handle<Client>, Option<String>) {
  connect_as(addr, "guest", login).await
}

/// Connects and logs in as `user`; returns the handle and the banner. Panics if a prompt
/// is asked or the login is refused.
pub async fn connect_as(
  addr: SocketAddr,
  user: &str,
  login: Login,
) -> (Handle<Client>, Option<String>) {
  let banner = Arc::new(Mutex::new(None));
  let config = Arc::new(client::Config::default());
  let mut handle = client::connect(
    config,
    addr,
    Client {
      banner: banner.clone(),
    },
  )
  .await
  .expect("connect");
  match login {
    Login::Keyless => {
      let reply = handle
        .authenticate_keyboard_interactive_start(user, None)
        .await
        .expect("keyboard-interactive");
      match reply {
        client::KeyboardInteractiveAuthResponse::Success => {}
        client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
          panic!("the server asked {} question(s)", prompts.len())
        }
        client::KeyboardInteractiveAuthResponse::Failure { .. } => panic!("refused"),
      }
    }
    Login::Key(key) => {
      let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(*key), None))
        .await
        .expect("publickey");
      assert!(result.success(), "public key refused");
    }
  }
  let banner = banner.lock().unwrap().clone();
  (handle, banner)
}

/// One shell channel with a terminal emulator behind it.
pub struct Terminal {
  pub channel: russh::Channel<client::Msg>,
  pub parser: vt100::Parser,
  /// Everything received, raw.
  pub raw: Vec<u8>,
  pub exit_status: Option<u32>,
  pub closed: bool,
  /// Answer XTGETTCAP `RGB` as a 24-bit terminal does (by default it goes unanswered,
  /// as in most terminals).
  pub answers_rgb: bool,
  /// Startup queries answered so far (DA1, XTGETTCAP).
  answered: (usize, usize),
}

/// DA1, which every terminal answers.
const DA1: &[u8] = b"\x1b[c";
/// XTGETTCAP, as a terminal host sends it.
const XTGETTCAP: &[u8] = b"\x1bP+q";

impl Terminal {
  /// Opens a session channel with a `cols`×`rows` `xterm-256color` PTY and starts the
  /// shell.
  pub async fn open(handle: &Handle<Client>, cols: u16, rows: u16) -> Self {
    Self::open_as(handle, "xterm-256color", cols, rows, false).await
  }

  /// [`Terminal::open`] with this `TERM`, answering XTGETTCAP `RGB` when `rgb`.
  pub async fn open_as(
    handle: &Handle<Client>,
    term: &str,
    cols: u16,
    rows: u16,
    rgb: bool,
  ) -> Self {
    Self::open_with_env(handle, term, cols, rows, rgb, &[]).await
  }

  /// [`Terminal::open_as`], sending `env` (env requests) before the shell request.
  pub async fn open_with_env(
    handle: &Handle<Client>,
    term: &str,
    cols: u16,
    rows: u16,
    rgb: bool,
    env: &[(&str, &str)],
  ) -> Self {
    let channel = handle
      .channel_open_session()
      .await
      .expect("session channel");
    channel
      .request_pty(true, term, cols.into(), rows.into(), 0, 0, &[])
      .await
      .unwrap();
    for (name, value) in env {
      channel.set_env(false, *name, *value).await.unwrap();
    }
    channel.request_shell(true).await.unwrap();
    let mut terminal = Self::over(channel, cols, rows);
    terminal.answers_rgb = rgb;
    terminal
  }

  /// A screen over a channel the caller set up.
  pub fn over(channel: russh::Channel<client::Msg>, cols: u16, rows: u16) -> Self {
    Self {
      channel,
      parser: vt100::Parser::new(nonzero(rows), nonzero(cols), 0),
      raw: Vec::new(),
      exit_status: None,
      closed: false,
      answers_rgb: false,
      answered: (0, 0),
    }
  }

  /// Answers the startup queries received so far, as a terminal would: XTGETTCAP (when
  /// asked to) before DA1, so the host's query ends at once. Counted over everything
  /// received, so a query split across two messages is answered too.
  async fn answer_queries(&mut self) {
    let count = |needle: &[u8]| {
      self
        .raw
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
    };
    let (xtgettcap, da1) = (count(XTGETTCAP), count(DA1));
    let mut reply = Vec::new();
    for _ in self.answered.1..xtgettcap {
      if self.answers_rgb {
        // `RGB` = 8/8/8 (hex-encoded name and value), and `Tc` unknown.
        reply.extend_from_slice(b"\x1bP1+r524742=382F382F38\x1b\\\x1bP0+r5463\x1b\\");
      }
    }
    for _ in self.answered.0..da1 {
      reply.extend_from_slice(b"\x1b[?62;22c");
    }
    self.answered = (da1, xtgettcap);
    if !reply.is_empty() {
      let _ = self.channel.data(&reply[..]).await;
    }
  }

  /// The screen's text.
  pub fn text(&self) -> String {
    self.parser.screen().contents()
  }

  /// Everything received, as text with escapes left in.
  pub fn raw_text(&self) -> String {
    String::from_utf8_lossy(&self.raw).into_owned()
  }

  /// Handles one message; `false` once the channel is gone.
  async fn step(&mut self, wait: Duration) -> bool {
    match tokio::time::timeout(wait, self.channel.wait()).await {
      Err(_) => true,
      Ok(None) => {
        self.closed = true;
        false
      }
      Ok(Some(message)) => {
        match message {
          ChannelMsg::Data { data } => {
            self.raw.extend_from_slice(&data);
            self.parser.process(&data);
            self.answer_queries().await;
          }
          ChannelMsg::ExitStatus { exit_status } => self.exit_status = Some(exit_status),
          ChannelMsg::Close => {
            self.closed = true;
            return false;
          }
          _ => {}
        }
        true
      }
    }
  }

  /// Reads until `done` holds for the screen or raw output, or `timeout` passes.
  pub async fn wait_for(&mut self, timeout: Duration, done: impl Fn(&Self) -> bool) -> bool {
    let end = Instant::now() + timeout;
    loop {
      if done(self) {
        return true;
      }
      let left = end.saturating_duration_since(Instant::now());
      if left.is_zero() || !self.step(left.min(Duration::from_millis(100))).await {
        return done(self);
      }
    }
  }

  /// Reads until the screen shows `needle`.
  pub async fn wait_text(&mut self, needle: &str, timeout: Duration) -> bool {
    self
      .wait_for(timeout, |terminal| terminal.text().contains(needle))
      .await
  }

  /// Reads until the raw output holds `needle`.
  pub async fn wait_raw(&mut self, needle: &str, timeout: Duration) -> bool {
    self
      .wait_for(timeout, |terminal| terminal.raw_text().contains(needle))
      .await
  }

  /// Reads until the channel closes.
  pub async fn wait_closed(&mut self, timeout: Duration) -> bool {
    self.wait_for(timeout, |terminal| terminal.closed).await
  }

  /// Resizes the client terminal and tells the server.
  pub async fn resize(&mut self, cols: u16, rows: u16) {
    self
      .parser
      .screen_mut()
      .set_size(nonzero(rows), nonzero(cols));
    self
      .channel
      .window_change(cols.into(), rows.into(), 0, 0)
      .await
      .unwrap();
  }

  /// Types `bytes`.
  pub async fn send(&self, bytes: &[u8]) {
    self.channel.data(bytes).await.unwrap();
  }
}

fn nonzero(value: u16) -> std::num::NonZero<u16> {
  std::num::NonZero::new(value).expect("a terminal is at least 1×1")
}

/// The value after `KEY=` on a line of the raw output (the last such line).
pub fn field(raw: &str, key: &str) -> Option<String> {
  raw
    .lines()
    .rev()
    .filter_map(|line| {
      line
        .trim_end_matches('\r')
        .split(' ')
        .find_map(|part| part.strip_prefix(&format!("{key}=")))
    })
    .next()
    .map(str::to_owned)
}

/// Whether process `pid` still exists (a zombie counts as gone). A process that is
/// gone is seen at once (`kill(pid, 0)`), not after a `ps` that may itself be slow on a
/// busy machine.
pub fn alive(pid: u32) -> bool {
  let Some(target) = i32::try_from(pid)
    .ok()
    .and_then(rustix::process::Pid::from_raw)
  else {
    return false;
  };
  if rustix::process::test_kill_process(target) == Err(rustix::io::Errno::SRCH) {
    return false;
  }
  let output = Command::new("ps")
    .args(["-o", "stat=", "-p", &pid.to_string()])
    .output()
    .unwrap();
  let stat = String::from_utf8_lossy(&output.stdout);
  output.status.success() && !stat.trim().is_empty() && !stat.trim().starts_with('Z')
}

/// The pids of this process's children whose command line contains `name` (an
/// in-process server's session programs are its children).
pub fn children_named(name: &str) -> Vec<u32> {
  let output = Command::new("ps")
    .args(["-A", "-o", "pid=,ppid=,stat=,command="])
    .output()
    .unwrap();
  let me = std::process::id();
  String::from_utf8_lossy(&output.stdout)
    .lines()
    .filter_map(|line| {
      let mut parts = line.split_whitespace();
      let pid: u32 = parts.next()?.parse().ok()?;
      let ppid: u32 = parts.next()?.parse().ok()?;
      let stat = parts.next()?;
      let command: Vec<&str> = parts.collect();
      (ppid == me && !stat.starts_with('Z') && command.join(" ").contains(name)).then_some(pid)
    })
    .collect()
}
