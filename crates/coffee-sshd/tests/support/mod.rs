//! What the end-to-end tests share: a server on an ephemeral port, a russh client with
//! a `vt100` screen, the real `coffee-tui`, a size-reporting stand-in child, and a stub
//! backend serving the contract fixtures.

use std::{
  net::SocketAddr,
  path::{Path, PathBuf},
  process::Command,
  sync::{Arc, Mutex, OnceLock},
  time::{Duration, Instant},
};

use coffee_sshd::{Config, serve};
use russh::{
  ChannelMsg,
  client::{self, Handle},
  keys::{Algorithm, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, ssh_key},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  sync::oneshot,
};

/// A fresh, empty directory for one test.
pub fn temp_dir(name: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!(
    "coffee-sshd-test-{}-{name}-{n}",
    std::process::id()
  ));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

/// The real `coffee-tui`, built once per test run with the workspace's cargo.
pub fn tui_bin() -> PathBuf {
  static BIN: OnceLock<PathBuf> = OnceLock::new();
  BIN
    .get_or_init(|| {
      let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
      let output = Command::new(cargo)
        .args([
          "build",
          "-p",
          "coffee-host",
          "--bin",
          "coffee-tui",
          "--message-format=json-render-diagnostics",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo build coffee-tui");
      assert!(
        output.status.success(),
        "building coffee-tui failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
      );
      String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["target"]["name"] == "coffee-tui")
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .expect("cargo reported no coffee-tui executable")
    })
    .clone()
}

/// A stand-in session program: prints its pid, the session's environment and its
/// terminal size, and its size again on every SIGWINCH.
pub fn probe_bin(dir: &Path) -> PathBuf {
  let path = dir.join("probe.sh");
  std::fs::write(
    &path,
    r#"#!/bin/sh
echo "PID=$$"
echo "FP=${COFFEE_SSH_FINGERPRINT:-none}"
echo "IP=${COFFEE_CLIENT_IP:-none} API=${COFFEE_API_URL:-none} KEY=${COFFEE_INTERNAL_KEY:-none}"
echo "TERM=${TERM:-none} LANG=${LANG:-none} COLORTERM=${COLORTERM:-none}"
trap 'echo "SIZE=$(stty size)"' WINCH
echo "SIZE=$(stty size)"
while :; do sleep 0.05; done
"#,
  )
  .unwrap();
  use std::os::unix::fs::PermissionsExt;
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path
}

/// A server running on 127.0.0.1 with `tui_bin` as the session program.
pub struct TestServer {
  pub addr: SocketAddr,
  stop: Option<oneshot::Sender<()>>,
  pub dir: PathBuf,
}

impl TestServer {
  pub async fn start(tui_bin: PathBuf, tune: impl FnOnce(&mut Config)) -> Self {
    let dir = temp_dir("server");
    let mut config = Config::from_vars(|_| None, None).unwrap();
    config.addr = "127.0.0.1:0".parse().unwrap();
    config.host_key = dir.join("host_key");
    config.tui_bin = tui_bin;
    config.api_url = "http://127.0.0.1:9".into();
    tune(&mut config);
    let (key, _) = coffee_sshd::hostkey::load_or_generate(&config.host_key).unwrap();
    let listener = TcpListener::bind(config.addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    tokio::spawn(async move {
      let _ = serve(listener, config, key, async {
        let _ = stopped.await;
      })
      .await;
    });
    Self {
      addr,
      stop: Some(stop),
      dir,
    }
  }
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
  pub fn new_key() -> (Self, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fingerprint = key
      .public_key()
      .fingerprint(ssh_key::HashAlg::Sha256)
      .to_string();
    (Self::Key(Box::new(key)), fingerprint)
  }
}

/// Connects and logs in; returns the handle and the banner. Panics if a prompt is asked.
pub async fn connect(addr: SocketAddr, login: Login) -> (Handle<Client>, Option<String>) {
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
        .authenticate_keyboard_interactive_start("guest", None)
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
        .authenticate_publickey("guest", PrivateKeyWithHashAlg::new(Arc::new(*key), None))
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
}

impl Terminal {
  /// Opens a session channel with a `cols`×`rows` PTY and starts the shell.
  pub async fn open(handle: &Handle<Client>, cols: u16, rows: u16) -> Self {
    let channel = handle
      .channel_open_session()
      .await
      .expect("session channel");
    channel
      .request_pty(true, "xterm-256color", cols.into(), rows.into(), 0, 0, &[])
      .await
      .unwrap();
    channel.request_shell(true).await.unwrap();
    Self::over(channel, cols, rows)
  }

  /// A screen over a channel the caller set up.
  pub fn over(channel: russh::Channel<client::Msg>, cols: u16, rows: u16) -> Self {
    Self {
      channel,
      parser: vt100::Parser::new(nonzero(rows), nonzero(cols), 0),
      raw: Vec::new(),
      exit_status: None,
      closed: false,
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
            // Answer DA1 as a terminal would, so the host's startup query ends at once.
            if data.windows(3).any(|w| w == b"\x1b[c") {
              let _ = self.channel.data(&b"\x1b[?62;22c"[..]).await;
            }
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

/// The value after `KEY=` on a line of the raw output.
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

/// Whether process `pid` still exists (a zombie of another parent counts as gone).
pub fn alive(pid: u32) -> bool {
  let output = Command::new("ps")
    .args(["-o", "stat=", "-p", &pid.to_string()])
    .output()
    .unwrap();
  let stat = String::from_utf8_lossy(&output.stdout);
  output.status.success() && !stat.trim().is_empty() && !stat.trim().starts_with('Z')
}

/// A tiny HTTP server answering `GET /v1/products` and `/v1/health` with the contract
/// fixtures; returns its base URL.
pub async fn stub_backend() -> String {
  let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/contract/fixtures/v1");
  let products = std::fs::read(fixtures.join("products.json")).unwrap();
  let health = std::fs::read(fixtures.join("health.json")).unwrap();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move {
    loop {
      let Ok((mut socket, _)) = listener.accept().await else {
        return;
      };
      let (products, health) = (products.clone(), health.clone());
      tokio::spawn(async move {
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
          match socket.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&chunk[..read]),
          }
        }
        let line = String::from_utf8_lossy(&request);
        let path = line.split_whitespace().nth(1).unwrap_or("/");
        let (status, body) = match path.split('?').next().unwrap_or("/") {
          "/v1/products" => ("200 OK", products),
          "/v1/health" => ("200 OK", health),
          _ => ("404 Not Found", br#"{"error":"not_found"}"#.to_vec()),
        };
        let head = format!(
          "HTTP/1.1 {status}\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
          body.len()
        );
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(&body).await;
        let _ = socket.shutdown().await;
      });
    }
  });
  format!("http://{addr}")
}
