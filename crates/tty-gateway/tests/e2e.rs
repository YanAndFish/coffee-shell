//! End to end: a russh client with a `vt100` screen against `tty-gateway` on an
//! ephemeral port, with stand-in session programs and the default (neutral) messages.

use std::{
  path::Path,
  time::{Duration, Instant},
};

use tty_gateway_testkit::{
  Login, Terminal, TestServer, alive, connect, crash_bin, field, probe_bin, quote_path, script_bin,
  temp_dir,
};

const WAIT: Duration = Duration::from_secs(10);

/// What the probe prints.
const VARS: &[&str] = &[
  "APP_FINGERPRINT",
  "APP_CLIENT_IP",
  "APP_KEY",
  "TERM",
  "LANG",
  "COLORTERM",
];

/// One route, `app`, running `program`, with `limits` as the `[limits]` table's body.
fn app(program: &Path, limits: &str) -> String {
  format!(
    r#"
[limits]
{limits}

[routes.app]
command = [{program}]

[routes.app.env]
APP_FINGERPRINT = "${{fingerprint}}"
APP_CLIENT_IP = "${{client_ip}}"
APP_KEY = "${{env:APP_KEY}}"
"#,
    program = quote_path(program),
  )
}

fn probe(dir: &Path) -> std::path::PathBuf {
  probe_bin(dir, "probe.sh", VARS)
}

/// 通过 SSH 发送输入，检查子进程实际读到的完整一行。
async fn assert_canonical_input(modes: &[(russh::Pty, u32)], input: &[u8], expected: &str) {
  let dir = temp_dir("erase");
  let program = script_bin(
    &dir,
    "read-line.sh",
    "stty -echo\nprintf 'READY\\n'\nIFS= read -r line\nprintf 'LINE=<%s>\\n' \"$line\"\n",
  );
  let server = TestServer::start(&app(&program, "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let channel = handle.channel_open_session().await.unwrap();
  channel
    .request_pty(true, "xterm-256color", 80, 24, 0, 0, modes)
    .await
    .unwrap();
  channel.request_shell(true).await.unwrap();
  let mut terminal = Terminal::over(channel, 80, 24);
  assert!(
    terminal.wait_raw("READY", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  terminal.send(input).await;
  assert!(terminal.wait_closed(WAIT).await, "{}", terminal.raw_text());
  assert_eq!(
    field(&terminal.raw_text(), "LINE"),
    Some(format!("<{expected}>")),
    "终端模式：{modes:?}，输出：{:?}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(0));
  server.shutdown().await;
  std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn requested_erase_characters_edit_the_childs_input() {
  for erase in *b"\x08\x7f" {
    assert_canonical_input(
      &[(russh::Pty::VERASE, u32::from(erase))],
      &[b'a', b'b', erase, b'c', b'\n'],
      "ac",
    )
    .await;
  }
}

/// 读取新建 PTY 的默认 erase 字符，避免测试依赖某个平台的缺省按键。
fn default_erase_character() -> u8 {
  let (_master, slave) = pty_process::open().unwrap();
  rustix::termios::tcgetattr(&slave).unwrap().special_codes
    [rustix::termios::SpecialCodeIndex::VERASE]
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omitted_erase_mode_uses_the_system_default() {
  let erase = default_erase_character();
  assert_canonical_input(&[], &[b'a', b'b', erase, b'c', b'\n'], "ac").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_erase_value_larger_than_a_byte_uses_the_system_default() {
  let erase = default_erase_character();
  assert_canonical_input(
    &[(russh::Pty::VERASE, 0x108)],
    &[b'a', b'b', erase, b'c', b'\n'],
    "ac",
  )
  .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_erase_mode_passes_backspace_and_delete_to_the_child() {
  assert_canonical_input(
    &[(russh::Pty::VERASE, 255)],
    b"ab\x08\x7fc\n",
    "ab\x08\x7fc",
  )
  .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn keyless_and_key_logins_need_no_prompt_and_the_fingerprint_reaches_the_child() {
  let dir = temp_dir("logins");
  let server =
    TestServer::start_with_env(&app(&probe(&dir), ""), &[("APP_KEY", "internal-secret")]).await;

  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert_eq!(banner, None, "no banner unless one is set");
  let mut keyless = Terminal::open(&handle, 80, 24).await;
  assert!(
    keyless.wait_raw("SIZE=", WAIT).await,
    "{}",
    keyless.raw_text()
  );
  let raw = keyless.raw_text();
  // No key, no `${fingerprint}`: the entry does not exist at all.
  assert_eq!(field(&raw, "APP_FINGERPRINT").as_deref(), Some("none"));
  assert_eq!(field(&raw, "APP_CLIENT_IP").as_deref(), Some("127.0.0.1"));
  assert_eq!(field(&raw, "APP_KEY").as_deref(), Some("internal-secret"));
  assert_eq!(field(&raw, "TERM").as_deref(), Some("xterm-256color"));
  assert!(field(&raw, "LANG").is_some_and(|lang| lang.contains("UTF-8")));

  let (login, fingerprint) = Login::new_key();
  assert!(fingerprint.starts_with("SHA256:"));
  let (handle, _) = connect(server.addr, login).await;
  let mut keyed = Terminal::open(&handle, 80, 24).await;
  assert!(keyed.wait_raw("SIZE=", WAIT).await);
  assert_eq!(
    field(&keyed.raw_text(), "APP_FINGERPRINT"),
    Some(fingerprint)
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_banner_is_the_configured_one() {
  let dir = temp_dir("banner");
  let toml = format!(
    "[server]\nbanner = \"\\n  Hello there\\n\\n\"\n{}",
    app(&probe(&dir), "")
  );
  let server = TestServer::start(&toml).await;
  let (_handle, banner) = connect(server.addr, Login::Keyless).await;
  assert_eq!(banner.as_deref(), Some("\n  Hello there\n\n"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn resizes_reach_the_child() {
  let dir = temp_dir("resize");
  let server = TestServer::start(&app(&probe(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 160, 50).await;
  assert!(
    terminal.wait_raw("SIZE=50 160", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  terminal.resize(60, 20).await;
  assert!(
    terminal.wait_raw("SIZE=20 60", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  terminal.resize(132, 43).await;
  assert!(
    terminal.wait_raw("SIZE=43 132", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_ends_the_child_within_two_seconds() {
  let dir = temp_dir("disconnect");
  let server = TestServer::start(&app(&probe(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  assert!(alive(pid));
  drop(terminal);
  drop(handle);
  let gone = Instant::now();
  while alive(pid) && gone.elapsed() < Duration::from_secs(3) {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  assert!(!alive(pid), "child {pid} still running");
  assert!(
    gone.elapsed() <= Duration::from_secs(2),
    "took {:?}",
    gone.elapsed()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn exec_sftp_and_forwarding_are_refused() {
  let dir = temp_dir("refused");
  let server = TestServer::start(&app(&probe(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;

  let mut exec = handle.channel_open_session().await.unwrap();
  exec.exec(true, "cat /etc/passwd").await.unwrap();
  let reply = tokio::time::timeout(WAIT, exec.wait()).await.unwrap();
  assert!(
    matches!(reply, Some(russh::ChannelMsg::Failure)),
    "{reply:?}"
  );

  let mut sftp = handle.channel_open_session().await;
  // One session channel per connection: a second is refused outright, which refuses
  // sftp too. On a fresh connection the subsystem request itself is refused.
  if let Ok(channel) = sftp.as_mut() {
    channel.request_subsystem(true, "sftp").await.unwrap();
    let reply = tokio::time::timeout(WAIT, channel.wait()).await.unwrap();
    assert!(
      matches!(reply, Some(russh::ChannelMsg::Failure)),
      "{reply:?}"
    );
  }
  let (fresh, _) = connect(server.addr, Login::Keyless).await;
  let mut sftp = fresh.channel_open_session().await.unwrap();
  sftp.request_subsystem(true, "sftp").await.unwrap();
  let reply = tokio::time::timeout(WAIT, sftp.wait()).await.unwrap();
  assert!(
    matches!(reply, Some(russh::ChannelMsg::Failure)),
    "{reply:?}"
  );

  let forward = fresh.tcpip_forward("127.0.0.1", 0).await;
  assert!(forward.is_err(), "remote forwarding accepted: {forward:?}");
  let direct = fresh
    .channel_open_direct_tcpip("127.0.0.1", 22, "127.0.0.1", 5555)
    .await;
  assert!(direct.is_err(), "local forwarding accepted");
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_without_a_pty_is_told_why() {
  let dir = temp_dir("nopty");
  let server = TestServer::start(&app(&probe(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let channel = handle.channel_open_session().await.unwrap();
  channel.request_shell(true).await.unwrap();
  let mut terminal = Terminal::over(channel, 80, 24);
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    terminal.raw_text().contains("ssh -t"),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(1));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn session_limits_hold_in_all_and_per_address() {
  let dir = temp_dir("limits");
  let server = TestServer::start(&app(&probe(&dir), "max_sessions = 2")).await;
  let mut open = Vec::new();
  for _ in 0..2 {
    let (handle, _) = connect(server.addr, Login::Keyless).await;
    let mut terminal = Terminal::open(&handle, 80, 24).await;
    assert!(terminal.wait_raw("SIZE=", WAIT).await);
    open.push((handle, terminal));
  }
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(
    banner
      .as_deref()
      .is_some_and(|b| b.contains("The server is full (2 sessions)")),
    "{banner:?}"
  );
  let mut third = Terminal::open(&handle, 80, 24).await;
  assert!(third.wait_closed(WAIT).await);
  assert!(
    third.raw_text().contains("The server is full (2 sessions)"),
    "{}",
    third.raw_text()
  );
  assert_eq!(third.exit_status, Some(1));
  drop(third);
  drop(handle);

  // A place frees up when a session ends.
  open.pop();
  let deadline = Instant::now() + WAIT;
  let mut admitted = false;
  while !admitted && Instant::now() < deadline {
    let (handle, _) = connect(server.addr, Login::Keyless).await;
    let mut terminal = Terminal::open(&handle, 80, 24).await;
    admitted = terminal.wait_raw("SIZE=", Duration::from_secs(2)).await;
  }
  assert!(admitted);
  drop(open);

  let per_ip = TestServer::start(&app(&probe(&dir), "max_sessions_per_ip = 1")).await;
  let (first, _) = connect(per_ip.addr, Login::Keyless).await;
  let mut first = (Terminal::open(&first, 80, 24).await, first);
  assert!(first.0.wait_raw("SIZE=", WAIT).await);
  let (handle, _) = connect(per_ip.addr, Login::Keyless).await;
  let mut second = Terminal::open(&handle, 80, 24).await;
  assert!(second.wait_closed(WAIT).await);
  assert!(
    second
      .raw_text()
      .contains("Your address has 1 sessions open"),
    "{}",
    second.raw_text()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_idle_session_is_closed() {
  let dir = temp_dir("idle");
  let idle = Duration::from_secs(1);
  let server = TestServer::start(&app(&probe(&dir), "idle_timeout = \"1s\"")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  // The idle clock starts at the program's first output, so the session is still there
  // to show it however long connecting and starting took.
  assert!(
    terminal.wait_raw("PID=", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  let shown = Instant::now();
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  assert!(terminal.wait_closed(WAIT).await);
  // Not before the idle time (less the client's share of the transit), then promptly.
  assert!(shown.elapsed() >= idle * 4 / 5, "{:?}", shown.elapsed());
  assert!(
    terminal.raw_text().contains("No input for 0 minutes"),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(124));
  assert!(!alive(pid));
  let _ = std::fs::remove_dir_all(dir);
}

/// The flake behind this test: the idle clock used to start at the TCP accept, so a
/// login slower than the idle time (a busy machine, a debug build's crypto) closed the
/// session as idle before its program had shown anything.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_login_does_not_count_as_idle() {
  let dir = temp_dir("slow-login");
  let idle = Duration::from_secs(2);
  let server = TestServer::start(&app(&probe(&dir), "idle_timeout = \"2s\"")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  tokio::time::sleep(idle * 3 / 2).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("PID=", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  assert!(
    !terminal.wait_closed(idle / 2).await,
    "closed as idle at once: {}",
    terminal.raw_text()
  );
  assert!(terminal.wait_closed(WAIT).await);
  assert_eq!(terminal.exit_status, Some(124));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crashed_program_leaves_a_clear_message() {
  let dir = temp_dir("crash");
  let server = TestServer::start(&app(&crash_bin(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await, "{}", terminal.raw_text());
  let raw = terminal.raw_text();
  assert!(raw.contains("PID="), "{raw}");
  assert!(raw.contains("stopped with an error (code 139)"), "{raw}");
  // The terminal is put back (main screen, cursor shown) before the message.
  assert!(raw.contains("\x1b[?1049l\x1b[?25h"), "{raw}");
  assert!(
    terminal.text().contains("Please reconnect"),
    "{}",
    terminal.text()
  );
  assert_eq!(terminal.exit_status, Some(139));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_cannot_start_is_explained() {
  let dir = temp_dir("missing");
  let server = TestServer::start(&app(&dir.join("no-such-program"), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    terminal.raw_text().contains("could not be started"),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(1));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_connection_is_cut_off_at_the_login_deadline() {
  use tokio::io::AsyncReadExt;
  let dir = temp_dir("silent");
  let server = TestServer::start(&app(&probe(&dir), "auth_timeout = \"1s\"")).await;
  // A client that never sends its SSH version line: before, it was held forever (and
  // enough of them refused everyone else).
  let mut socket = tokio::net::TcpStream::connect(server.addr).await.unwrap();
  let opened = Instant::now();
  let mut seen = Vec::new();
  let read = tokio::time::timeout(WAIT, socket.read_to_end(&mut seen)).await;
  assert!(read.is_ok(), "still open after {:?}", opened.elapsed());
  assert!(String::from_utf8_lossy(&seen).starts_with("SSH-2.0-tty_gateway\r\n"));
  assert!(opened.elapsed() >= Duration::from_millis(900));

  // Logged in, but no shell asked for: cut off at the same deadline.
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let deadline = Instant::now() + WAIT;
  while !handle.is_closed() && Instant::now() < deadline {
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  assert!(
    handle.is_closed(),
    "a connection without a session stayed open"
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn logging_in_holds_no_session_place() {
  let dir = temp_dir("login-place");
  let server = TestServer::start(&app(
    &probe(&dir),
    "max_sessions = 1\nmax_sessions_per_ip = 1",
  ))
  .await;
  // Logged in, no shell yet: the one place is still free for someone else.
  let (waiting, _) = connect(server.addr, Login::Keyless).await;
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert_eq!(banner, None);
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("SIZE=", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  // Now it is taken: the first connection's shell is refused with the reason.
  let mut late = Terminal::open(&waiting, 80, 24).await;
  assert!(late.wait_closed(WAIT).await);
  assert!(
    late.raw_text().contains("The server is full"),
    "{}",
    late.raw_text()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_beyond_the_rate_are_dropped_before_the_handshake() {
  use tokio::io::AsyncReadExt;
  let dir = temp_dir("rate");
  let server = TestServer::start(&app(&probe(&dir), "connections_per_minute = 2")).await;
  for _ in 0..2 {
    let (handle, _) = connect(server.addr, Login::Keyless).await;
    let mut terminal = Terminal::open(&handle, 80, 24).await;
    assert!(terminal.wait_raw("SIZE=", WAIT).await);
  }
  let mut socket = tokio::net::TcpStream::connect(server.addr).await.unwrap();
  let mut seen = Vec::new();
  let read = tokio::time::timeout(WAIT, socket.read_to_end(&mut seen)).await;
  assert!(read.is_ok(), "the third connection was kept");
  assert!(seen.is_empty(), "{:?}", String::from_utf8_lossy(&seen));
  let _ = std::fs::remove_dir_all(dir);
}
