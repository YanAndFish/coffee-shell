//! End to end: a russh client with a `vt100` screen against `coffee-sshd` on an
//! ephemeral port, with the real `coffee-tui` or a size-reporting stand-in as the
//! session program.

mod support;

use std::time::{Duration, Instant};

use support::{
  Login, Terminal, TestServer, alive, connect, crash_bin, field, probe_bin, stub_backend, temp_dir,
  tui_bin,
};

const WAIT: Duration = Duration::from_secs(10);

/// The pids of this process's children running `name` (the server runs in-process,
/// so session programs are its children).
fn children_named(name: &str) -> Vec<u32> {
  let output = std::process::Command::new("ps")
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

#[tokio::test(flavor = "multi_thread")]
async fn the_real_tui_shows_the_brand_and_the_demo_notice_at_160x50() {
  let server = TestServer::start(tui_bin(), |_| {}).await;
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(
    banner
      .as_deref()
      .is_some_and(|b| b.contains("演示环境 · 不是真实商店")),
    "banner: {banner:?}"
  );
  let mut terminal = Terminal::open(&handle, 160, 50).await;
  assert!(
    terminal
      .wait_for(WAIT, |t| t.text().contains("◆ROAST/TTY")
        && t.text().contains("演示环境"))
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

  // Live resize: the app lays itself out again at the new size.
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
async fn keyless_and_key_logins_need_no_prompt_and_the_fingerprint_reaches_the_child() {
  let dir = temp_dir("logins");
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.internal_key = Some("internal-secret".into());
  })
  .await;

  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(banner.is_some_and(|b| b.contains("演示环境")));
  let mut keyless = Terminal::open(&handle, 80, 24).await;
  assert!(
    keyless.wait_raw("SIZE=", WAIT).await,
    "{}",
    keyless.raw_text()
  );
  let raw = keyless.raw_text();
  assert_eq!(field(&raw, "FP").as_deref(), Some("none"));
  assert_eq!(field(&raw, "IP").as_deref(), Some("127.0.0.1"));
  assert_eq!(field(&raw, "API").as_deref(), Some("http://127.0.0.1:9"));
  assert_eq!(field(&raw, "KEY").as_deref(), Some("internal-secret"));
  assert_eq!(field(&raw, "TERM").as_deref(), Some("xterm-256color"));
  assert!(field(&raw, "LANG").is_some_and(|lang| lang.contains("UTF-8")));

  let (login, fingerprint) = Login::new_key();
  assert!(fingerprint.starts_with("SHA256:"));
  let (handle, _) = connect(server.addr, login).await;
  let mut keyed = Terminal::open(&handle, 80, 24).await;
  assert!(keyed.wait_raw("SIZE=", WAIT).await);
  assert_eq!(field(&keyed.raw_text(), "FP"), Some(fingerprint));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn resizes_reach_the_child() {
  let dir = temp_dir("resize");
  let server = TestServer::start(probe_bin(&dir), |_| {}).await;
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
  let server = TestServer::start(probe_bin(&dir), |_| {}).await;
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
async fn a_disconnect_ends_the_real_tui_within_two_seconds() {
  // A name of its own, so other tests' sessions running alongside are not counted.
  let dir = temp_dir("real-disconnect");
  let bin = dir.join("coffee-tui-disconnect-test");
  std::os::unix::fs::symlink(tui_bin(), &bin).unwrap();
  let server = TestServer::start(bin, |_| {}).await;
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

#[tokio::test(flavor = "multi_thread")]
async fn exec_sftp_and_forwarding_are_refused() {
  let dir = temp_dir("refused");
  let server = TestServer::start(probe_bin(&dir), |_| {}).await;
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
  let server = TestServer::start(probe_bin(&dir), |_| {}).await;
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
  let server = TestServer::start(probe_bin(&dir), |config| config.max_sessions = 2).await;
  let mut open = Vec::new();
  for _ in 0..2 {
    let (handle, _) = connect(server.addr, Login::Keyless).await;
    let mut terminal = Terminal::open(&handle, 80, 24).await;
    assert!(terminal.wait_raw("SIZE=", WAIT).await);
    open.push((handle, terminal));
  }
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(banner.is_some_and(|b| b.contains("已满")));
  let mut third = Terminal::open(&handle, 80, 24).await;
  assert!(third.wait_closed(WAIT).await);
  assert!(third.raw_text().contains("已满"), "{}", third.raw_text());
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

  let per_ip = TestServer::start(probe_bin(&dir), |config| config.max_sessions_per_ip = 1).await;
  let (first, _) = connect(per_ip.addr, Login::Keyless).await;
  let mut first = (Terminal::open(&first, 80, 24).await, first);
  assert!(first.0.wait_raw("SIZE=", WAIT).await);
  let (handle, _) = connect(per_ip.addr, Login::Keyless).await;
  let mut second = Terminal::open(&handle, 80, 24).await;
  assert!(second.wait_closed(WAIT).await);
  assert!(second.raw_text().contains("上限"), "{}", second.raw_text());
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_idle_session_is_closed() {
  let dir = temp_dir("idle");
  let idle = Duration::from_secs(1);
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.idle_timeout = Some(idle);
  })
  .await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  // The idle clock starts at the program's first output (the `PID=` line), so the
  // session is still there to show it however long connecting and starting took.
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
    terminal.raw_text().contains("空闲"),
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
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.idle_timeout = Some(idle);
  })
  .await;
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
  let server = TestServer::start(crash_bin(&dir), |_| {}).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await, "{}", terminal.raw_text());
  let raw = terminal.raw_text();
  assert!(raw.contains("PID="), "{raw}");
  assert!(raw.contains("出错了，会话已结束（代码 139）"), "{raw}");
  // The terminal is put back (main screen, cursor shown) before the message.
  assert!(raw.contains("\x1b[?1049l\x1b[?25h"), "{raw}");
  assert!(
    terminal.text().contains("请重新连接"),
    "{}",
    terminal.text()
  );
  assert_eq!(terminal.exit_status, Some(139));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_cannot_start_is_explained() {
  let dir = temp_dir("missing");
  let server = TestServer::start(dir.join("no-such-program"), |_| {}).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    terminal.raw_text().contains("暂时无法启动"),
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
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.auth_timeout = Duration::from_secs(1);
  })
  .await;
  // A client that never sends its SSH version line: before, it was held forever (and
  // enough of them refused everyone else).
  let mut socket = tokio::net::TcpStream::connect(server.addr).await.unwrap();
  let opened = Instant::now();
  let mut seen = Vec::new();
  let read = tokio::time::timeout(WAIT, socket.read_to_end(&mut seen)).await;
  assert!(read.is_ok(), "still open after {:?}", opened.elapsed());
  assert!(String::from_utf8_lossy(&seen).starts_with("SSH-2.0-ROAST_TTY"));
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
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.max_sessions = 1;
    config.max_sessions_per_ip = 1;
  })
  .await;
  // Logged in, no shell yet: the one place is still free for someone else.
  let (waiting, _) = connect(server.addr, Login::Keyless).await;
  let (handle, banner) = connect(server.addr, Login::Keyless).await;
  assert!(banner.is_some_and(|b| !b.contains("上限") && !b.contains("已满")));
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("SIZE=", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  // Now it is taken: the first connection's shell is refused with the reason.
  let mut late = Terminal::open(&waiting, 80, 24).await;
  assert!(late.wait_closed(WAIT).await);
  assert!(late.raw_text().contains("已满"), "{}", late.raw_text());
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_beyond_the_rate_are_dropped_before_the_handshake() {
  use tokio::io::AsyncReadExt;
  let dir = temp_dir("rate");
  let server = TestServer::start(probe_bin(&dir), |config| {
    config.connections_per_minute = Some(2);
  })
  .await;
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

/// The SGR colour sequences in `raw`: whether any is 24-bit (`38;2;`/`48;2;`) and any is
/// from the 256-colour table (`38;5;`/`48;5;`).
fn color_kinds(raw: &str) -> (bool, bool) {
  let truecolor = raw.contains("38;2;") || raw.contains("48;2;");
  let indexed = raw.contains("38;5;") || raw.contains("48;5;");
  (truecolor, indexed)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_truecolor_term_brand_gets_24_bit_colour_without_colorterm() {
  let server = TestServer::start(tui_bin(), |_| {}).await;
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
  let server = TestServer::start(tui_bin(), |_| {}).await;
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
  let server = TestServer::start(tui_bin(), |config| config.api_url = api).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 160, 50).await;
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
  let server = TestServer::start(tui_bin(), |config| config.api_url = api).await;
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
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

/// One process's resident memory (KiB) and CPU time (seconds), from `ps`.
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
/// of the real `coffee-tui`. Against a running `coffee-sshd` when `COFFEE_SPIKE_ADDR`
/// and `COFFEE_SPIKE_PID` are set, else an in-process server (`COFFEE_TUI_BIN` picks
/// the program, e.g. a release build).
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
      let bin = std::env::var("COFFEE_TUI_BIN").map_or_else(|_| tui_bin(), Into::into);
      let server = TestServer::start(bin, |config| {
        config.max_sessions = 64;
        config.max_sessions_per_ip = 64;
      })
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
      assert!(terminal.wait_text("◆ROAST/TTY", WAIT).await);
      let frame = start.elapsed();
      if let Some(content) = &content {
        assert!(
          terminal.wait_text(content, WAIT).await,
          "{}",
          terminal.text()
        );
      }
      (frame, start.elapsed(), handle, terminal)
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
  println!("| sessions | {SESSIONS} concurrent, 160×50, keyless |");
  println!(
    "| startup (connect → first frame) | p50 {:?} · p95 {:?} · max {:?} |",
    startup[SESSIONS / 2],
    startup[SESSIONS * 95 / 100],
    startup[SESSIONS - 1]
  );
  if let Some(content) = &content {
    println!(
      "| connect → {content:?} on screen | p50 {:?} · p95 {:?} · max {:?} |",
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
      "coffee-sshd"
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
