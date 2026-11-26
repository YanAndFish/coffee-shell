//! The shop over SSH: the real `coffee-tui` behind an in-process `tty-gateway` running the
//! repo's own `deploy/gateway.toml` (only what a test needs is overridden: a program, a
//! limit), a russh client with a `vt100` screen, and a stub backend serving the contract
//! fixtures. A change to the shipped banner, messages or routes shows up here.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
};
use tty_gateway_testkit::{
  Login, Terminal, TestServer, alive, children_named, connect, connect_as, crash_bin, field,
  probe_bin, temp_dir,
};

const WAIT: Duration = Duration::from_secs(10);

/// The configuration the repo ships (`pnpm dev`, Docker, scripts/e2e).
const SHIPPED: &str = include_str!("../../../deploy/gateway.toml");

/// The shipped configuration, with overrides.
#[derive(Clone)]
struct Config(toml::Table);

impl Config {
  fn shipped() -> Self {
    Self(SHIPPED.parse().expect("deploy/gateway.toml is TOML"))
  }

  /// Sets `path` (dotted, e.g. `limits.max_sessions`); every table on the way must
  /// already be in the shipped file.
  fn set(mut self, path: &str, value: impl Into<toml::Value>) -> Self {
    let mut keys: Vec<&str> = path.split('.').collect();
    let last = keys.pop().unwrap();
    let mut table = &mut self.0;
    for key in keys {
      table = table
        .get_mut(key)
        .and_then(toml::Value::as_table_mut)
        .unwrap_or_else(|| panic!("no table {key:?} on the way to {path:?}"));
    }
    table.insert(last.to_owned(), value.into());
    self
  }

  /// Runs `program` for `route` instead of the shipped command.
  fn command(self, route: &str, program: &Path) -> Self {
    let program = toml::Value::String(program.to_string_lossy().into_owned());
    self.set(&format!("routes.{route}.command"), vec![program])
  }

  /// A shipped string (e.g. `messages.full`, `server.banner`).
  fn text(&self, path: &str) -> String {
    let mut keys = path.split('.');
    let mut value = keys.next().and_then(|key| self.0.get(key));
    for key in keys {
      value = value.and_then(|value| value.get(key));
    }
    value
      .and_then(toml::Value::as_str)
      .unwrap_or_else(|| panic!("no string {path:?} in the config"))
      .to_owned()
  }

  /// A shipped message with `{name}` filled in, as the gateway does.
  fn message(&self, key: &str, name: &str, value: &str) -> String {
    self
      .text(&format!("messages.{key}"))
      .replace(&format!("{{{name}}}"), value)
  }

  /// A gateway on this configuration, `env` its environment. A bare `coffee-tui` is the
  /// one Cargo built for these tests (as next to a real gateway's executable).
  async fn start(&self, env: &[(&str, &str)]) -> TestServer {
    let text = toml::to_string(&self.0).unwrap();
    TestServer::start_with_exe_dir(&text, env, Some(tui_dir())).await
  }
}

/// Where Cargo put `coffee-tui` for these tests.
fn tui_dir() -> &'static Path {
  Path::new(env!("CARGO_BIN_EXE_coffee-tui"))
    .parent()
    .unwrap()
}

fn tui_bin() -> PathBuf {
  PathBuf::from(env!("CARGO_BIN_EXE_coffee-tui"))
}

/// The backend's address the tests give the gateway (nothing listens there).
const NO_API: (&str, &str) = ("COFFEE_API_URL", "http://127.0.0.1:9");

/// A tiny HTTP server answering `GET /v1/products` and `/v1/health` with the contract
/// fixtures; returns its base URL.
async fn stub_backend() -> String {
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

/// What the probe stand-in prints.
const COFFEE_VARS: &[&str] = &[
  "COFFEE_SSH_FINGERPRINT",
  "COFFEE_CLIENT_IP",
  "COFFEE_API_URL",
  "COFFEE_INTERNAL_KEY",
  "COFFEE_COLORS",
  "COFFEE_UI_GALLERY",
];

/// Logs in as `user`, opens a shell and returns its output once a probe is up.
async fn probe_session(server: &TestServer, user: &str, login: Login) -> String {
  let (handle, _) = connect_as(server.addr, user, login).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("SIZE=", WAIT).await,
    "{user}: {}",
    terminal.raw_text()
  );
  terminal.raw_text()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shop_route_hands_the_coffee_environment_to_the_session() {
  let dir = temp_dir("shop-env");
  let probe = probe_bin(&dir, "probe.sh", COFFEE_VARS);
  let config = Config::shipped().command("coffee", &probe);
  let server = config
    .start(&[NO_API, ("COFFEE_INTERNAL_KEY", "internal-secret")])
    .await;
  let raw = probe_session(&server, "coffee", Login::Keyless).await;
  assert_eq!(
    field(&raw, "COFFEE_SSH_FINGERPRINT").as_deref(),
    Some("none")
  );
  assert_eq!(
    field(&raw, "COFFEE_CLIENT_IP").as_deref(),
    Some("127.0.0.1")
  );
  assert_eq!(
    field(&raw, "COFFEE_API_URL").as_deref(),
    Some("http://127.0.0.1:9")
  );
  assert_eq!(
    field(&raw, "COFFEE_INTERNAL_KEY").as_deref(),
    Some("internal-secret")
  );
  assert_eq!(field(&raw, "COFFEE_COLORS").as_deref(), Some("none"));
  assert_eq!(field(&raw, "COFFEE_UI_GALLERY").as_deref(), Some("none"));

  let (login, fingerprint) = Login::new_key();
  let raw = probe_session(&server, "shop", login).await;
  assert_eq!(field(&raw, "COFFEE_SSH_FINGERPRINT"), Some(fingerprint));

  // Nothing in the gateway's environment: the shipped defaults; a forced colour depth.
  let plain = config.start(&[("COFFEE_COLORS", "16")]).await;
  let raw = probe_session(&plain, "someone", Login::Keyless).await;
  assert_eq!(
    field(&raw, "COFFEE_API_URL").as_deref(),
    Some("http://127.0.0.1:3000")
  );
  assert_eq!(field(&raw, "COFFEE_INTERNAL_KEY").as_deref(), Some("none"));
  assert_eq!(field(&raw, "COFFEE_COLORS").as_deref(), Some("16"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_gallery_route_gets_neither_the_backend_nor_the_key() {
  let dir = temp_dir("gallery-env");
  let probe = probe_bin(&dir, "probe.sh", COFFEE_VARS);
  // The shipped gallery route with its env table, only its program swapped for a probe.
  let server = Config::shipped()
    .command("gallery", &probe)
    .start(&[
      NO_API,
      ("COFFEE_INTERNAL_KEY", "internal-secret"),
      ("COFFEE_COLORS", "256"),
    ])
    .await;
  let raw = probe_session(&server, "gallery", Login::Keyless).await;
  assert_eq!(field(&raw, "COFFEE_UI_GALLERY").as_deref(), Some("1"));
  assert_eq!(field(&raw, "COFFEE_INTERNAL_KEY").as_deref(), Some("none"));
  assert_eq!(field(&raw, "COFFEE_API_URL").as_deref(), Some("none"));
  assert_eq!(
    field(&raw, "COFFEE_SSH_FINGERPRINT").as_deref(),
    Some("none")
  );
  assert_eq!(
    field(&raw, "COFFEE_CLIENT_IP").as_deref(),
    Some("127.0.0.1")
  );
  assert_eq!(field(&raw, "COFFEE_COLORS").as_deref(), Some("256"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn user_names_pick_the_shop_or_the_gallery() {
  let api = stub_backend().await;
  let server = Config::shipped()
    .start(&[("COFFEE_API_URL", api.as_str())])
    .await;
  for user in ["coffee", "shop", "nobody-in-particular"] {
    let (handle, banner) = connect_as(server.addr, user, Login::Keyless).await;
    assert!(banner.is_some_and(|b| b.contains("演示环境")), "{user}");
    let mut terminal = Terminal::open(&handle, 160, 50).await;
    assert!(
      terminal
        .wait_for(WAIT, |t| t.text().contains("登录 · SIGN IN"))
        .await,
      "{user}:\n{}",
      terminal.text()
    );
    assert!(!terminal.text().contains("GALLERY"), "{user}");
  }
  let (handle, _) = connect_as(server.addr, "gallery", Login::Keyless).await;
  let mut gallery = Terminal::open(&handle, 160, 50).await;
  assert!(
    gallery.wait_text("UI 组件库 GALLERY", WAIT).await,
    "{}",
    gallery.text()
  );
  assert!(!gallery.text().contains("登录 · SIGN IN"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shipped_texts_reach_the_client() {
  use tokio::io::AsyncReadExt;
  let dir = temp_dir("shop-text");
  let config = Config::shipped();
  let crashing = config.clone().command("coffee", &crash_bin(&dir));
  let server = crashing.start(&[NO_API]).await;

  // The server id and the banner, with its demo notice.
  let mut socket = tokio::net::TcpStream::connect(server.addr).await.unwrap();
  let id = format!("{}\r\n", config.text("server.server_id"));
  let mut seen = vec![0u8; id.len()];
  socket.read_exact(&mut seen).await.unwrap();
  assert_eq!(String::from_utf8_lossy(&seen), id);
  drop(socket);

  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert_eq!(banner, Some(config.text("server.banner")));
  assert!(
    config
      .text("server.banner")
      .contains("演示环境 · 不是真实商店")
  );
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await, "{}", terminal.raw_text());
  let crashed = config.message("crashed", "code", "139");
  assert!(
    terminal.raw_text().contains(&crashed),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(139));

  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let channel = handle.channel_open_session().await.unwrap();
  channel.request_shell(true).await.unwrap();
  let mut nopty = Terminal::over(channel, 80, 24);
  assert!(nopty.wait_closed(WAIT).await);
  assert!(
    nopty.raw_text().contains(&config.text("messages.no_pty")),
    "{}",
    nopty.raw_text()
  );

  let missing = config
    .clone()
    .command("coffee", &dir.join("no-such-program"))
    .start(&[NO_API])
    .await;
  let (handle, _) = connect(missing.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    terminal
      .raw_text()
      .contains(&config.text("messages.unavailable")),
    "{}",
    terminal.raw_text()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shipped_limit_texts_reach_the_client() {
  let dir = temp_dir("shop-limits");
  let probe = probe_bin(&dir, "probe.sh", &[]);
  let config = Config::shipped();

  // In all: in the banner and on the refused shell.
  let full = config
    .clone()
    .command("coffee", &probe)
    .set("limits.max_sessions", 1)
    .start(&[NO_API])
    .await;
  let (first, _) = connect(full.addr, Login::Keyless).await;
  let mut first = Terminal::open(&first, 80, 24).await;
  assert!(first.wait_raw("SIZE=", WAIT).await);
  let text = config.message("full", "max", "1");
  let (handle, banner) = connect(full.addr, Login::Keyless).await;
  assert!(
    banner
      .as_deref()
      .is_some_and(|b| b.starts_with(&config.text("server.banner")) && b.contains(&text)),
    "{banner:?}"
  );
  let mut second = Terminal::open(&handle, 80, 24).await;
  assert!(second.wait_closed(WAIT).await);
  assert!(second.raw_text().contains(&text), "{}", second.raw_text());
  drop(first);

  // Per route: the shop is full, the gallery is not.
  let route_full = config
    .clone()
    .command("coffee", &probe)
    .set("routes.coffee.max_sessions", 1)
    .start(&[NO_API])
    .await;
  let (first, _) = connect_as(route_full.addr, "coffee", Login::Keyless).await;
  let mut first = Terminal::open(&first, 80, 24).await;
  assert!(first.wait_raw("SIZE=", WAIT).await);
  let (handle, _) = connect_as(route_full.addr, "shop", Login::Keyless).await;
  let mut second = Terminal::open(&handle, 80, 24).await;
  assert!(second.wait_closed(WAIT).await);
  let text = config.message("full_route", "max", "1");
  assert!(second.raw_text().contains(&text), "{}", second.raw_text());
  assert_eq!(second.exit_status, Some(1));

  // The session time limit.
  let timed = config
    .clone()
    .command("coffee", &probe)
    .set("limits.max_session_time", "1s")
    .start(&[NO_API])
    .await;
  let (handle, _) = connect(timed.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    terminal
      .raw_text()
      .contains(&config.text("messages.time_limit")),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(124));

  // Idle.
  let idle = config
    .clone()
    .command("coffee", &probe)
    .set("limits.idle_timeout", "1s")
    .start(&[NO_API])
    .await;
  let (handle, _) = connect(idle.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  let text = config.message("idle", "minutes", "0");
  assert!(
    terminal.raw_text().contains(&text),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(124));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shutdown_tells_the_shop_client_and_ends_coffee_tui() {
  // A name of its own, so other tests' sessions running alongside are not counted.
  let dir = temp_dir("shop-shutdown");
  let bin = dir.join("coffee-tui-shutdown-test");
  std::os::unix::fs::symlink(tui_bin(), &bin).unwrap();
  let config = Config::shipped();
  let server = config
    .clone()
    .command("coffee", &bin)
    .start(&[NO_API])
    .await;
  let grace = Duration::from_secs(2);
  assert_eq!(config.text("server.shutdown_grace"), "2s");
  let before = children_named("coffee-tui-shutdown-test");
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 120, 40).await;
  assert!(
    terminal.wait_text("◆ROAST/TTY", WAIT).await,
    "{}",
    terminal.text()
  );
  let pids: Vec<u32> = children_named("coffee-tui-shutdown-test")
    .into_iter()
    .filter(|pid| !before.contains(pid))
    .collect();
  assert_eq!(pids.len(), 1, "{pids:?}");
  let took = server.shutdown().await;
  assert!(
    took <= grace + Duration::from_millis(1300),
    "the drain took {took:?}"
  );
  assert!(terminal.wait_closed(WAIT).await);
  let raw = terminal.raw_text();
  let text = config.text("messages.shutting_down");
  // The terminal is put back (main screen, cursor shown) before the notice.
  let reset = raw
    .rfind("\x1b[?1049l\x1b[?25h")
    .expect("no terminal reset");
  let notice = raw.rfind(&text).expect("no shutting_down text");
  assert!(reset < notice);
  assert!(!alive(pids[0]), "coffee-tui {} still running", pids[0]);
  drop(handle);
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_real_tui_shows_the_brand_and_the_demo_notice_at_160x50() {
  let server = Config::shipped().start(&[NO_API]).await;
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(
    banner
      .as_deref()
      .is_some_and(|b| b.contains("演示环境 · 不是真实商店")),
    "banner: {banner:?}"
  );
  let mut terminal = Terminal::open(&handle, 160, 50).await;
  // A new connection opens on the login page; the system row carries the demo notice.
  assert!(
    terminal
      .wait_for(WAIT, |t| t.text().contains("登录 · SIGN IN")
        && t.text().contains("演示环境 · 不是真实商店"))
      .await,
    "screen:\n{}",
    terminal.text()
  );
  // The palette pin: the session asked for xterm-256color, so the theme's nearest
  // xterm entries (background #14110e → 233) are drawn, not the terminal's defaults.
  assert!(
    terminal.raw_text().contains("48;5;233"),
    "no pinned background"
  );

  // Live resize: the app lays itself out again at the new size (under 32 rows the block
  // letters give way to the one-line wordmark).
  terminal.resize(100, 30).await;
  assert!(
    terminal
      .wait_for(WAIT, |t| {
        let screen = t.parser.screen();
        (0..30).any(|row| {
          screen
            .contents_between(row, 0, row, 100)
            .contains("◆ROAST/TTY")
        }) && !t.text().contains("再按一次")
      })
      .await
  );

  // Ctrl+C once shows the hint and keeps the session; twice quits with status 0.
  terminal.send(b"\x03").await;
  assert!(
    terminal.wait_text("再按一次 Ctrl+C 退出", WAIT).await,
    "{}",
    terminal.text()
  );
  assert!(!terminal.closed);
  terminal.send(b"\x03").await;
  assert!(terminal.wait_closed(WAIT).await);
  assert_eq!(terminal.exit_status, Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_ends_the_real_tui_within_two_seconds() {
  // A name of its own, so other tests' sessions running alongside are not counted.
  let dir = temp_dir("real-disconnect");
  let bin = dir.join("coffee-tui-disconnect-test");
  std::os::unix::fs::symlink(tui_bin(), &bin).unwrap();
  let server = Config::shipped()
    .command("coffee", &bin)
    .start(&[NO_API])
    .await;
  let before = children_named("coffee-tui-disconnect-test");
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 120, 40).await;
  assert!(terminal.wait_text("◆ROAST/TTY", WAIT).await);
  let pids: Vec<u32> = children_named("coffee-tui-disconnect-test")
    .into_iter()
    .filter(|pid| !before.contains(pid))
    .collect();
  assert_eq!(pids.len(), 1, "{pids:?}");
  drop(terminal);
  drop(handle);
  let gone = Instant::now();
  while alive(pids[0]) && gone.elapsed() < Duration::from_secs(3) {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  assert!(!alive(pids[0]), "coffee-tui {} still running", pids[0]);
  assert!(
    gone.elapsed() <= Duration::from_secs(2),
    "took {:?}",
    gone.elapsed()
  );
  let _ = std::fs::remove_dir_all(dir);
}

/// The SGR colour sequences in `raw`: whether any is 24-bit (`38;2;`/`48;2;`) and any is
/// from the 256-colour table (`38;5;`/`48;5;`).
fn color_kinds(raw: &str) -> (bool, bool) {
  let truecolor = raw.contains("38;2;") || raw.contains("48;2;");
  let indexed = raw.contains("38;5;") || raw.contains("48;5;");
  (truecolor, indexed)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_truecolor_term_brand_gets_24_bit_colour_without_colorterm() {
  let server = Config::shipped().start(&[NO_API]).await;
  // kitty's TERM survives SSH; COLORTERM does not (and is not sent here).
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut kitty = Terminal::open_as(&handle, "xterm-kitty", 120, 40, false).await;
  assert!(
    kitty.wait_text("◆ROAST/TTY", WAIT).await,
    "{}",
    kitty.text()
  );
  assert_eq!(color_kinds(&kitty.raw_text()), (true, false));
  // The background pinned exactly (#14110e).
  assert!(kitty.raw_text().contains("48;2;20;17;14"));

  // A plain xterm-256color session, whose terminal does not answer XTGETTCAP: 256 colours.
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut plain = Terminal::open(&handle, 120, 40).await;
  assert!(
    plain.wait_text("◆ROAST/TTY", WAIT).await,
    "{}",
    plain.text()
  );
  assert_eq!(color_kinds(&plain.raw_text()), (false, true));
  assert!(plain.raw_text().contains("\x1bP+q524742"), "RGB not asked");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_that_reports_rgb_gets_24_bit_colour() {
  let server = Config::shipped().start(&[NO_API]).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open_as(&handle, "xterm-256color", 120, 40, true).await;
  assert!(
    terminal.wait_text("◆ROAST/TTY", WAIT).await,
    "{}",
    terminal.text()
  );
  assert_eq!(color_kinds(&terminal.raw_text()), (true, false));
  // Nothing of the answer leaked into the app as typed keys or onto the screen.
  assert!(!terminal.text().contains("524742"), "{}", terminal.text());
}

#[tokio::test(flavor = "multi_thread")]
async fn products_from_the_backend_show_at_160x50() {
  let api = stub_backend().await;
  let server = Config::shipped()
    .start(&[("COFFEE_API_URL", api.as_str())])
    .await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 160, 50).await;
  // A new connection opens on the login page with the focus in 用户名 (K1): `Esc` leaves
  // the field, then `g` browses as a guest. Each key waits for the status bar to show its
  // effect: `Esc` and `g` read in one go would be Alt+g.
  let status_bar = |t: &Terminal, hint: &str| {
    t.parser
      .screen()
      .contents_between(49, 0, 49, 160)
      .contains(hint)
  };
  assert!(
    terminal
      .wait_for(Duration::from_secs(15), |t| t
        .text()
        .contains("登录 · SIGN IN")
        && status_bar(t, "Esc  离开输入"))
      .await,
    "screen:\n{}",
    terminal.text()
  );
  terminal.send(b"\x1b").await;
  assert!(
    terminal
      .wait_for(Duration::from_secs(15), |t| status_bar(t, "g  访客浏览"))
      .await,
    "screen:\n{}",
    terminal.text()
  );
  terminal.send(b"g").await;
  assert!(
    terminal
      .wait_text("耶加雪菲 孔加", Duration::from_secs(15))
      .await,
    "screen:\n{}",
    terminal.text()
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_small_terminal_shows_the_too_small_notice_at_60x20() {
  let api = stub_backend().await;
  let server = Config::shipped()
    .start(&[("COFFEE_API_URL", api.as_str())])
    .await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 60, 20).await;
  assert!(
    terminal
      .wait_text("终端太小", Duration::from_secs(15))
      .await,
    "screen:\n{}",
    terminal.text()
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stub_backend_serves_the_contract_fixtures() {
  let api = stub_backend().await;
  for (path, needle) in [("/v1/products", "耶加雪菲 孔加"), ("/v1/health", "\"ok\"")] {
    let mut socket = tokio::net::TcpStream::connect(api.trim_start_matches("http://"))
      .await
      .unwrap();
    socket
      .write_all(format!("GET {path} HTTP/1.1\r\nhost: x\r\n\r\n").as_bytes())
      .await
      .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains(needle), "{path}");
  }
}

fn usage(pid: u32) -> Option<(u64, f64)> {
  let output = std::process::Command::new("ps")
    .args(["-o", "rss=,time=", "-p", &pid.to_string()])
    .output()
    .ok()?;
  let text = String::from_utf8_lossy(&output.stdout);
  let mut parts = text.split_whitespace();
  let rss = parts.next()?.parse().ok()?;
  // `m:ss.cc` (macOS) or `[dd-]hh:mm:ss` (procps).
  let time = parts.next()?;
  let seconds = time
    .rsplit('-')
    .next()?
    .split(':')
    .try_fold(0.0, |total, part| {
      part.parse::<f64>().ok().map(|v| total * 60.0 + v)
    })?;
  Some((rss, seconds))
}

/// Children of `parent` (all of them, by pid).
fn children_of(parent: u32) -> Vec<u32> {
  let output = std::process::Command::new("ps")
    .args(["-A", "-o", "pid=,ppid=,stat="])
    .output()
    .unwrap();
  String::from_utf8_lossy(&output.stdout)
    .lines()
    .filter_map(|line| {
      let mut parts = line.split_whitespace();
      let pid: u32 = parts.next()?.parse().ok()?;
      let ppid: u32 = parts.next()?.parse().ok()?;
      let stat = parts.next()?;
      (ppid == parent && !stat.starts_with('Z')).then_some(pid)
    })
    .collect()
}

/// The measurement behind `docs/adr/0001-ssh-session-model.md`: 20 concurrent sessions
/// of the real `coffee-tui`. Against a running `tty-gateway` when `COFFEE_SPIKE_ADDR`
/// and `COFFEE_SPIKE_PID` are set, else an in-process server (`COFFEE_TUI_BIN` picks
/// the program, e.g. a release build). The first frame is the login page (its demo
/// notice); with `COFFEE_SPIKE_WAIT` (text of the product list, e.g. `耶加雪菲 孔加`)
/// each session also walks as a guest: list → the first product's detail → `a` → cart,
/// and the memory and idle CPU are measured with every session resting on its cart.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement for docs/adr/0001; run with --ignored --nocapture"]
async fn measure_twenty_sessions() {
  const SESSIONS: usize = 20;
  let external = std::env::var("COFFEE_SPIKE_ADDR")
    .ok()
    .zip(std::env::var("COFFEE_SPIKE_PID").ok());
  let _server;
  let (addr, server_pid) = match &external {
    Some((addr, pid)) => (addr.parse().unwrap(), pid.parse().unwrap()),
    None => {
      let bin: PathBuf = std::env::var("COFFEE_TUI_BIN").map_or_else(|_| tui_bin(), Into::into);
      let server = Config::shipped()
        .command("coffee", &bin)
        .set("limits.max_sessions", 64)
        .set("limits.max_sessions_per_ip", 64)
        .set("limits.connections_per_minute", 0)
        .start(&[NO_API])
        .await;
      let addr = server.addr;
      _server = server;
      (addr, std::process::id())
    }
  };
  let before = children_of(server_pid);
  let server_rss_before = usage(server_pid).map_or(0, |(rss, _)| rss);

  // With a backend behind the server, also time the products' arrival
  // (e.g. COFFEE_SPIKE_WAIT='耶加雪菲 孔加').
  let content = std::env::var("COFFEE_SPIKE_WAIT").ok();
  let mut tasks = Vec::new();
  for _ in 0..SESSIONS {
    let content = content.clone();
    tasks.push(tokio::spawn(async move {
      let start = Instant::now();
      let (handle, _) = connect(addr, Login::Keyless).await;
      let mut terminal = Terminal::open(&handle, 160, 50).await;
      assert!(terminal.wait_text("演示环境", WAIT).await);
      let frame = start.elapsed();
      let mut loaded = frame;
      if let Some(content) = &content {
        let status = |t: &Terminal, hint: &str| {
          t.parser
            .screen()
            .contents_between(49, 0, 49, 160)
            .contains(hint)
        };
        // Each key waits for its effect: Esc and `g` read in one go would be Alt+g.
        let steps: [(&[u8], &str, bool); 5] = [
          (b"", "离开输入", true),
          (b"\x1b", "访客浏览", true),
          (b"g", content.as_str(), false),
          // The SKU line shows once the detail has loaded: `a` before that adds nothing.
          (b"\r", "SKU RT-", false),
          (b"a", "已加入购物车", false),
        ];
        for (keys, until, on_status) in steps {
          if !keys.is_empty() {
            terminal.send(keys).await;
          }
          assert!(
            terminal
              .wait_for(WAIT, |t| if on_status {
                status(t, until)
              } else {
                t.text().contains(until)
              })
              .await,
            "{}",
            terminal.text()
          );
          if until == content.as_str() {
            loaded = start.elapsed();
          }
        }
        terminal.send(b"2").await;
        assert!(
          terminal.wait_text("购物车 CART", WAIT).await,
          "{}",
          terminal.text()
        );
      }
      (frame, loaded, handle, terminal)
    }));
  }
  let mut sessions = Vec::new();
  for task in tasks {
    sessions.push(task.await.unwrap());
  }
  let mut startup: Vec<Duration> = sessions.iter().map(|(elapsed, ..)| *elapsed).collect();
  startup.sort();
  let mut loaded: Vec<Duration> = sessions.iter().map(|(_, loaded, ..)| *loaded).collect();
  loaded.sort();

  tokio::time::sleep(Duration::from_secs(2)).await;
  let children: Vec<u32> = children_of(server_pid)
    .into_iter()
    .filter(|pid| !before.contains(pid))
    .collect();
  assert_eq!(children.len(), SESSIONS, "{children:?}");
  let first: Vec<(u64, f64)> = children.iter().filter_map(|pid| usage(*pid)).collect();
  let server_first = usage(server_pid).map_or(0.0, |(_, cpu)| cpu);
  let window = Duration::from_secs(10);
  tokio::time::sleep(window).await;
  let second: Vec<(u64, f64)> = children.iter().filter_map(|pid| usage(*pid)).collect();
  let (server_rss, server_second) = usage(server_pid).unwrap_or_default();

  let rss: Vec<f64> = second.iter().map(|(rss, _)| *rss as f64 / 1024.0).collect();
  let cpu: Vec<f64> = first
    .iter()
    .zip(&second)
    .map(|((_, a), (_, b))| (b - a) / window.as_secs_f64() * 100.0)
    .collect();
  let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
  let max = |values: &[f64]| values.iter().copied().fold(0.0, f64::max);
  let min = |values: &[f64]| values.iter().copied().fold(f64::MAX, f64::min);

  let left = Instant::now();
  drop(sessions);
  while children.iter().any(|pid| alive(*pid)) && left.elapsed() < Duration::from_secs(5) {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  let remaining = children.iter().filter(|pid| alive(**pid)).count();

  println!("| measure | value |");
  println!("|---|---|");
  println!(
    "| sessions | {SESSIONS} concurrent, 160×50, keyless{} |",
    if content.is_some() {
      ", each walked list → detail → add → cart"
    } else {
      ", on the login page"
    }
  );
  println!(
    "| startup (connect → first frame, the login page) | p50 {:?} · p95 {:?} · max {:?} |",
    startup[SESSIONS / 2],
    startup[SESSIONS * 95 / 100],
    startup[SESSIONS - 1]
  );
  if let Some(content) = &content {
    println!(
      "| connect → list with {content:?} (through Esc, g) | p50 {:?} · p95 {:?} · max {:?} |",
      loaded[SESSIONS / 2],
      loaded[SESSIONS * 95 / 100],
      loaded[SESSIONS - 1]
    );
  }
  println!(
    "| RSS per coffee-tui | min {:.1} MB · mean {:.1} MB · max {:.1} MB |",
    min(&rss),
    mean(&rss),
    max(&rss)
  );
  println!(
    "| idle CPU per coffee-tui ({}s) | mean {:.2} % · max {:.2} % |",
    window.as_secs(),
    mean(&cpu),
    max(&cpu)
  );
  println!(
    "| server ({}) | RSS {:.1} MB (was {:.1} MB) · idle CPU {:.2} % |",
    if external.is_some() {
      "tty-gateway"
    } else {
      "test process"
    },
    server_rss as f64 / 1024.0,
    server_rss_before as f64 / 1024.0,
    (server_second - server_first) / window.as_secs_f64() * 100.0
  );
  println!(
    "| disconnect all at once | all {SESSIONS} children gone after {:?}, {remaining} left |",
    left.elapsed()
  );
  assert_eq!(remaining, 0);
}
