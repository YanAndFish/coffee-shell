//! Programs and clients that misbehave: a program that never reads, a client that floods
//! it, grandchildren left behind, EOF, the session time limit.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use tty_gateway_testkit::{
  Login, Terminal, TestServer, alive, connect, field, probe_bin, quote_path, script_bin, temp_dir,
};

const WAIT: Duration = Duration::from_secs(10);

/// One route running `program`, with `limits` as the body of `[limits]`.
fn app(program: &Path, limits: &str) -> String {
  format!(
    "[limits]\n{limits}\n\n[routes.app]\ncommand = [{}]\n",
    quote_path(program)
  )
}

/// A program that never reads its input (raw mode, so the terminal does not discard
/// it either), and reports its size on SIGWINCH.
fn stall_bin(dir: &Path) -> PathBuf {
  script_bin(
    dir,
    "stall.sh",
    "stty raw -echo\necho \"PID=$$\"\ntrap 'echo \"SIZE=$(stty size)\"' WINCH\necho READY\nwhile :; do sleep 0.05; done\n",
  )
}

/// Waits (at most 3 s) until `pid` is gone; how long that took.
async fn gone_within(pid: u32) -> Duration {
  let start = Instant::now();
  while alive(pid) && start.elapsed() < Duration::from_secs(3) {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  start.elapsed()
}

/// Sends `total` bytes in 32 KiB pieces; stops at the first error (the server hung up).
async fn flood(terminal: &Terminal, total: usize) {
  let chunk = vec![b'x'; 32 * 1024];
  let mut sent = 0;
  while sent < total {
    let send = terminal.channel.data(&chunk[..]);
    match tokio::time::timeout(WAIT, send).await {
      Ok(Ok(())) => sent += chunk.len(),
      _ => return,
    }
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_never_reads_does_not_hold_up_the_close() {
  let dir = temp_dir("stall-close");
  let server = TestServer::start(&app(&stall_bin(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("READY", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  // Well within the budget, but far more than a terminal's input queue holds.
  flood(&terminal, 512 * 1024).await;
  tokio::time::sleep(Duration::from_millis(200)).await;
  assert!(alive(pid));
  drop(terminal);
  drop(handle);
  let took = gone_within(pid).await;
  assert!(!alive(pid), "child {pid} still running");
  assert!(took <= Duration::from_secs(2), "took {took:?}");
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn input_beyond_the_budget_ends_the_session_and_frees_its_place() {
  let dir = temp_dir("flood");
  let server = TestServer::start(&app(&stall_bin(&dir), "max_sessions = 1")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("READY", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  flood(&terminal, 3 * 1024 * 1024).await;
  assert!(
    terminal.wait_closed(WAIT).await,
    "the session is still open"
  );
  assert!(
    !terminal.raw_text().contains("x"),
    "nothing is echoed or told"
  );
  let took = gone_within(pid).await;
  assert!(!alive(pid), "child {pid} still running after {took:?}");
  drop(handle);

  // The one place is free again.
  let deadline = Instant::now() + WAIT;
  let mut admitted = false;
  while !admitted && Instant::now() < deadline {
    let (handle, _) = connect(server.addr, Login::Keyless).await;
    let mut next = Terminal::open(&handle, 80, 24).await;
    admitted = next.wait_raw("READY", Duration::from_secs(2)).await;
  }
  assert!(admitted);
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_program_leaves_in_its_group_goes_when_it_exits() {
  let dir = temp_dir("grandchild");
  // The grandchild ignores SIGHUP: the kernel's hang-up when the session leader exits,
  // and the gateway's, do not end it; only the group SIGKILL after the grace does.
  let program = script_bin(
    &dir,
    "parent.sh",
    "(trap '' HUP; exec sleep 300) &\necho \"GRANDCHILD=$!\"\nsleep 0.3\nexit 0\n",
  );
  let server = TestServer::start(&app(&program, "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await, "{}", terminal.raw_text());
  assert_eq!(terminal.exit_status, Some(0));
  let grandchild: u32 = field(&terminal.raw_text(), "GRANDCHILD")
    .unwrap()
    .parse()
    .unwrap();
  let took = gone_within(grandchild).await;
  assert!(!alive(grandchild), "grandchild {grandchild} still running");
  assert!(took <= Duration::from_secs(2), "took {took:?}");
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn eof_from_the_client_ends_the_session() {
  let dir = temp_dir("eof");
  let server = TestServer::start(&app(&probe_bin(&dir, "probe.sh", &[]), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  terminal.channel.eof().await.unwrap();
  assert!(terminal.wait_closed(WAIT).await);
  assert!(terminal.exit_status.is_some());
  assert!(!alive(pid) || gone_within(pid).await <= Duration::from_secs(2));
  assert!(!alive(pid));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_session_time_limit_closes_with_124() {
  let dir = temp_dir("time-limit");
  let server = TestServer::start(&app(
    &probe_bin(&dir, "probe.sh", &[]),
    "max_session_time = \"1s\"",
  ))
  .await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  let shown = Instant::now();
  let pid: u32 = field(&terminal.raw_text(), "PID").unwrap().parse().unwrap();
  // Typing does not extend it (it is not the idle timeout).
  terminal.send(b"a").await;
  assert!(terminal.wait_closed(WAIT).await);
  assert!(
    shown.elapsed() < Duration::from_secs(3),
    "{:?}",
    shown.elapsed()
  );
  assert!(
    terminal.raw_text().contains("reached its time limit"),
    "{}",
    terminal.raw_text()
  );
  assert_eq!(terminal.exit_status, Some(124));
  assert!(!alive(pid));

  // A route may set its own.
  let routed = TestServer::start(&format!(
    "{}max_session_time = \"0\"\n",
    app(
      &probe_bin(&dir, "probe.sh", &[]),
      "max_session_time = \"1s\""
    )
  ))
  .await;
  let (handle, _) = connect(routed.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  assert!(!terminal.wait_closed(Duration::from_secs(2)).await);
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn input_sent_right_after_the_shell_request_arrives() {
  let dir = temp_dir("early-input");
  let program = script_bin(
    &dir,
    "reader.sh",
    "read line\necho \"GOT=$line\"\nsleep 5\n",
  );
  let server = TestServer::start(&app(&program, "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  // Before the program can be up: queued, not lost.
  terminal.send(b"hello\r").await;
  assert!(
    terminal.wait_raw("GOT=hello", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resize_lands_while_input_is_stuck() {
  let dir = temp_dir("stall-resize");
  let server = TestServer::start(&app(&stall_bin(&dir), "")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal.wait_raw("READY", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  flood(&terminal, 256 * 1024).await;
  terminal.resize(100, 30).await;
  assert!(
    terminal.wait_raw("SIZE=30 100", WAIT).await,
    "{}",
    terminal.raw_text()
  );
  terminal.resize(90, 20).await;
  assert!(terminal.wait_raw("SIZE=20 90", WAIT).await);
  let _ = std::fs::remove_dir_all(dir);
}
