//! Reloads and shutdowns: in-process through the test kit, and through the real binary
//! with signals.

use std::{
  io::{BufRead, BufReader},
  net::SocketAddr,
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};

use tty_gateway_testkit::{
  Login, Terminal, TestServer, alive, connect, field, probe_bin, quote_path, script_bin, temp_dir,
};

const WAIT: Duration = Duration::from_secs(10);
/// What `serve` may take past the grace (the bound is a second; this is scheduling).
const SLACK: Duration = Duration::from_millis(300);

/// One route `app` running `program` with `TAG = tag`, a 1 s shutdown grace.
fn app(program: &Path, tag: &str) -> String {
  format!(
    "[server]\nshutdown_grace = \"1s\"\n\n[routes.app]\ncommand = [{}]\nenv = {{ TAG = \"{tag}\" }}\n",
    quote_path(program)
  )
}

/// A program that leaves a grandchild in its group and keeps running.
fn family_bin(dir: &Path) -> PathBuf {
  script_bin(
    dir,
    "family.sh",
    "sleep 300 &\necho \"GRANDCHILD=$!\"\necho \"PID=$$\"\necho READY\nwhile :; do sleep 0.05; done\n",
  )
}

async fn gone_within(pids: &[u32], limit: Duration) -> bool {
  let start = Instant::now();
  while pids.iter().any(|pid| alive(*pid)) && start.elapsed() < limit {
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  !pids.iter().any(|pid| alive(*pid))
}

async fn session(
  addr: SocketAddr,
) -> (russh::client::Handle<tty_gateway_testkit::Client>, Terminal) {
  let (handle, _) = connect(addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(
    terminal
      .wait_for(WAIT, |t| {
        let raw = t.raw_text();
        raw.contains("SIZE=") || raw.contains("READY")
      })
      .await,
    "{}",
    terminal.raw_text()
  );
  (handle, terminal)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reload_changes_new_sessions_and_leaves_open_ones_alone() {
  let dir = temp_dir("reload-program");
  let (a, b) = (
    probe_bin(&dir, "a.sh", &["TAG"]),
    probe_bin(&dir, "b.sh", &["TAG"]),
  );
  let server = TestServer::start(&app(&a, "one")).await;
  let (_old_handle, mut old) = session(server.addr).await;
  assert_eq!(field(&old.raw_text(), "TAG").as_deref(), Some("one"));
  let old_pid: u32 = field(&old.raw_text(), "PID").unwrap().parse().unwrap();

  server.reload(&app(&b, "two")).unwrap();
  let (_handle, new) = session(server.addr).await;
  assert_eq!(field(&new.raw_text(), "PROBE").as_deref(), Some("b.sh"));
  assert_eq!(field(&new.raw_text(), "TAG").as_deref(), Some("two"));

  // The open session still runs the old program.
  assert!(alive(old_pid));
  old.resize(100, 30).await;
  assert!(old.wait_raw("SIZE=30 100", WAIT).await);
  assert_eq!(field(&old.raw_text(), "PROBE").as_deref(), Some("a.sh"));

  // A file that does not load changes nothing.
  assert!(server.reload("[routes.app]\ncommand = []\n").is_err());
  let (_handle, again) = session(server.addr).await;
  assert_eq!(field(&again.raw_text(), "PROBE").as_deref(), Some("b.sh"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shutdown_tells_every_client_and_takes_every_program_down() {
  let dir = temp_dir("shutdown");
  let server = TestServer::start(&app(&family_bin(&dir), "x")).await;
  let mut sessions = Vec::new();
  let mut pids = Vec::new();
  for _ in 0..2 {
    let (handle, terminal) = session(server.addr).await;
    for key in ["PID", "GRANDCHILD"] {
      pids.push(
        field(&terminal.raw_text(), key)
          .unwrap()
          .parse::<u32>()
          .unwrap(),
      );
    }
    sessions.push((handle, terminal));
  }
  // Someone still logging in is dropped too.
  let (_waiting, _) = connect(server.addr, Login::Keyless).await;
  let took = server.shutdown().await;
  assert!(
    took <= Duration::from_secs(2) + SLACK,
    "the drain took {took:?}"
  );
  for (_, terminal) in &mut sessions {
    assert!(terminal.wait_closed(WAIT).await);
    assert!(
      terminal.raw_text().contains("The server is shutting down"),
      "{}",
      terminal.raw_text()
    );
    // The terminal is put back before the notice, whatever the program did on its way
    // out (this one is a shell script: a hang-up kills it and it restores nothing).
    let raw = terminal.raw_text();
    let reset = raw
      .rfind("\x1b[?1049l\x1b[?25h\x1b[0m")
      .expect("no terminal reset");
    let notice = raw.rfind("The server is shutting down").expect("no notice");
    assert!(reset < notice, "{raw}");
    assert_eq!(terminal.exit_status, Some(143));
  }
  assert!(
    gone_within(&pids, Duration::from_millis(500)).await,
    "still running: {pids:?}"
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_never_reads_does_not_hold_up_the_shutdown() {
  let dir = temp_dir("shutdown-stuck");
  let yes = script_bin(&dir, "yes.sh", "echo \"PID=$$\"\nexec yes flood\n");
  let server = TestServer::start(&app(&yes, "x")).await;
  let (handle, _) = connect(server.addr, Login::Keyless).await;
  // A shell whose output nobody reads: the window fills, the server's writes wait.
  let channel = handle.channel_open_session().await.unwrap();
  channel
    .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
    .await
    .unwrap();
  channel.request_shell(true).await.unwrap();
  tokio::time::sleep(Duration::from_millis(1500)).await;
  let took = server.shutdown().await;
  assert!(
    took <= Duration::from_secs(2) + SLACK,
    "the drain took {took:?}"
  );
  drop(channel);
  drop(handle);
  let _ = std::fs::remove_dir_all(dir);
}

/// The real binary, its log lines collected.
struct Gateway {
  child: Child,
  log: Arc<Mutex<Vec<String>>>,
  addr: SocketAddr,
}

impl Gateway {
  fn start(config: &Path, dir: &Path) -> Self {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tty-gateway"))
      .arg("--config")
      .arg(config)
      .env("TTY_GATEWAY_LOG", "info")
      .env("TTY_GATEWAY_ADDR", "127.0.0.1:0")
      .env("TTY_GATEWAY_HOST_KEY", dir.join("host_key"))
      .stdout(Stdio::null())
      .stderr(Stdio::piped())
      .spawn()
      .unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let lines = BufReader::new(child.stderr.take().unwrap());
    let collected = log.clone();
    std::thread::spawn(move || {
      for line in lines.lines().map_while(Result::ok) {
        collected.lock().unwrap().push(line);
      }
    });
    let mut gateway = Self {
      child,
      log,
      addr: "127.0.0.1:1".parse().unwrap(),
    };
    let line = gateway.wait_log("listening", 0);
    let addr = line
      .split_whitespace()
      .find_map(|part| part.strip_prefix("addr="))
      .unwrap();
    gateway.addr = addr.parse().unwrap();
    gateway
  }

  /// Waits for the `nth` (from 0) log line containing `needle`.
  fn wait_log(&self, needle: &str, nth: usize) -> String {
    let start = Instant::now();
    loop {
      if let Some(line) = self
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.contains(needle))
        .nth(nth)
      {
        return line.clone();
      }
      assert!(
        start.elapsed() < WAIT,
        "no {needle:?} in the log:\n{}",
        self.log.lock().unwrap().join("\n")
      );
      std::thread::sleep(Duration::from_millis(20));
    }
  }

  fn signal(&self, signal: rustix::process::Signal) {
    let pid = rustix::process::Pid::from_raw(i32::try_from(self.child.id()).unwrap()).unwrap();
    rustix::process::kill_process(pid, signal).unwrap();
  }
}

impl Drop for Gateway {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// Every log line names the route the same way, `route=<name>` unquoted (operators and
/// scripts/e2e grep for it), on the connection's lines and the session task's alike.
#[tokio::test(flavor = "multi_thread")]
async fn log_lines_name_the_route_unquoted() {
  let dir = temp_dir("log-route");
  let missing = dir.join("no-such-program");
  let config = dir.join("gateway.toml");
  std::fs::write(&config, app(&missing, "x")).unwrap();
  let gateway = Gateway::start(&config, &dir);
  let (handle, _) = connect(gateway.addr, Login::Keyless).await;
  let mut terminal = Terminal::open(&handle, 80, 24).await;
  assert!(terminal.wait_closed(WAIT).await);
  for needle in [
    "session started",
    "could not start the session program; trying again",
  ] {
    let line = gateway.wait_log(needle, 0);
    assert!(line.contains(" route=app"), "{line}");
  }
  let line = gateway.wait_log("ERROR could not start the session program", 0);
  assert!(line.contains(" route=app "), "{line}");
  assert!(!line.contains("route=\"app\""), "{line}");
  assert!(line.contains(&missing.display().to_string()), "{line}");
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_binary_reloads_on_sighup_and_drains_on_sigterm() {
  use rustix::process::Signal;
  let dir = temp_dir("binary");
  let (a, b) = (
    probe_bin(&dir, "a.sh", &["TAG"]),
    probe_bin(&dir, "b.sh", &["TAG"]),
  );
  let config = dir.join("gateway.toml");
  std::fs::write(&config, app(&a, "one")).unwrap();
  let mut gateway = Gateway::start(&config, &dir);

  let (_first_handle, mut first) = session(gateway.addr).await;
  assert_eq!(field(&first.raw_text(), "PROBE").as_deref(), Some("a.sh"));
  let mut pids = vec![
    field(&first.raw_text(), "PID")
      .unwrap()
      .parse::<u32>()
      .unwrap(),
  ];

  // A good file: new connections get it.
  std::fs::write(&config, app(&b, "two")).unwrap();
  gateway.signal(Signal::HUP);
  let line = gateway.wait_log("reloaded", 0);
  assert!(line.contains("changed app"), "{line}");
  let (_handle, second) = session(gateway.addr).await;
  assert_eq!(field(&second.raw_text(), "PROBE").as_deref(), Some("b.sh"));
  assert_eq!(field(&second.raw_text(), "TAG").as_deref(), Some("two"));
  pids.push(field(&second.raw_text(), "PID").unwrap().parse().unwrap());

  // A broken file: the running configuration stays.
  std::fs::write(&config, "[routes.app]\ncommand = \"not a list\"\n").unwrap();
  gateway.signal(Signal::HUP);
  gateway.wait_log("reload failed", 0);
  let (_handle, third) = session(gateway.addr).await;
  assert_eq!(field(&third.raw_text(), "PROBE").as_deref(), Some("b.sh"));
  pids.push(field(&third.raw_text(), "PID").unwrap().parse().unwrap());

  // The first session never noticed.
  first.resize(100, 30).await;
  assert!(first.wait_raw("SIZE=30 100", WAIT).await);

  // SIGTERM: everyone is told, everything stops, exit 0 within the grace and a second.
  let sent = Instant::now();
  gateway.signal(Signal::TERM);
  let status = loop {
    if let Some(status) = gateway.child.try_wait().unwrap() {
      break status;
    }
    assert!(sent.elapsed() < WAIT, "still running");
    tokio::time::sleep(Duration::from_millis(20)).await;
  };
  let took = sent.elapsed();
  assert!(status.success(), "{status:?}");
  assert!(took <= Duration::from_secs(2) + SLACK, "took {took:?}");
  assert!(first.wait_closed(WAIT).await);
  assert!(
    first.raw_text().contains("The server is shutting down"),
    "{}",
    first.raw_text()
  );
  assert!(
    gone_within(&pids, Duration::from_millis(500)).await,
    "still running: {pids:?}"
  );
  gateway.wait_log("stopped", 0);
  let _ = std::fs::remove_dir_all(dir);
}
