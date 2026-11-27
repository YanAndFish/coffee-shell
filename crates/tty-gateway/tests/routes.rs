//! Routing: the SSH user name picks the route; each route has its own program,
//! environment, caps and messages. Also `tty-gateway --check`.

use std::{
  path::{Path, PathBuf},
  process::Command,
  time::Duration,
};

use tty_gateway_testkit::{
  Login, Terminal, TestServer, connect_as, field, probe_bin, quote_path, temp_dir,
};

const WAIT: Duration = Duration::from_secs(10);

const VARS: &[&str] = &[
  "ROUTE",
  "WHO",
  "FP",
  "SECRET_A",
  "SECRET",
  "APP_FINGERPRINT",
  "APP_OTHER",
  "LANG",
];

/// Two routes: `a` (users alice, al; at most `a_max` sessions) and `b` (the default).
fn two_routes(dir: &Path, a_max: usize) -> String {
  let a = probe_bin(dir, "a.sh", VARS);
  let b = probe_bin(dir, "b.sh", VARS);
  format!(
    r#"
[server]
default_route = "b"

[routes.a]
users = ["alice", "al"]
command = [{a}]
max_sessions = {a_max}
accept_env = ["LANG", "LC_*", "APP_*"]
[routes.a.env]
ROUTE = "${{route}}"
WHO = "${{user}}"
SECRET_A = "${{env:SECRET}}"
APP_FINGERPRINT = "${{fingerprint}}"

[routes.b]
command = [{b}]
[routes.b.env]
ROUTE = "${{route}}"
WHO = "${{user}}"
"#,
    a = quote_path(&a),
    b = quote_path(&b),
  )
}

/// Logs in as `user`, opens a shell and returns its output once the probe is up.
async fn session_of(server: &TestServer, user: &str, login: Login) -> String {
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
async fn user_names_pick_the_route_and_others_get_the_default() {
  let dir = temp_dir("routes");
  let server = TestServer::start(&two_routes(&dir, 8)).await;
  for (user, probe) in [
    ("alice", "a.sh"),
    ("al", "a.sh"),
    ("b", "b.sh"),
    ("nobody", "b.sh"),
    ("a", "b.sh"),
  ] {
    let raw = session_of(&server, user, Login::Keyless).await;
    assert_eq!(field(&raw, "PROBE").as_deref(), Some(probe), "{user}");
    let route = &probe[..1];
    assert_eq!(field(&raw, "ROUTE").as_deref(), Some(route), "{user}");
    assert_eq!(field(&raw, "WHO").as_deref(), Some(user), "{user}");
  }
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_reaches_only_the_route_that_names_it() {
  let dir = temp_dir("isolation");
  let server = TestServer::start_with_env(&two_routes(&dir, 8), &[("SECRET", "topsecret")]).await;
  let a = session_of(&server, "alice", Login::Keyless).await;
  assert_eq!(field(&a, "SECRET_A").as_deref(), Some("topsecret"));
  // Nothing of the gateway's environment is passed on unless a route says so.
  assert_eq!(field(&a, "SECRET").as_deref(), Some("none"));
  let b = session_of(&server, "nobody", Login::Keyless).await;
  assert_eq!(field(&b, "SECRET_A").as_deref(), Some("none"));
  assert_eq!(field(&b, "SECRET").as_deref(), Some("none"));

  // Unset in the gateway: the entry is left out, not set empty.
  let unset = TestServer::start(&two_routes(&dir, 8)).await;
  let a = session_of(&unset, "alice", Login::Keyless).await;
  assert_eq!(field(&a, "SECRET_A").as_deref(), Some("none"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fingerprint_is_there_only_after_a_key() {
  let dir = temp_dir("fingerprint");
  let server = TestServer::start(&two_routes(&dir, 8)).await;
  let keyless = session_of(&server, "alice", Login::Keyless).await;
  assert_eq!(field(&keyless, "APP_FINGERPRINT").as_deref(), Some("none"));
  let (login, fingerprint) = Login::new_key();
  let keyed = session_of(&server, "alice", login).await;
  assert_eq!(field(&keyed, "APP_FINGERPRINT"), Some(fingerprint));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_cannot_forge_a_route_entry() {
  let dir = temp_dir("forge");
  let server = TestServer::start(&two_routes(&dir, 8)).await;
  // Route `a` accepts `APP_*` from clients, but `APP_FINGERPRINT` is its own: absent
  // for a keyless login, and never the client's.
  let (handle, _) = connect_as(server.addr, "alice", Login::Keyless).await;
  let mut terminal = Terminal::open_with_env(
    &handle,
    "xterm-256color",
    80,
    24,
    false,
    &[
      ("APP_FINGERPRINT", "SHA256:forged"),
      ("APP_OTHER", "fine"),
      ("LANG", "en_US.UTF-8"),
      ("ROUTE", "evil"),
    ],
  )
  .await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  let raw = terminal.raw_text();
  assert_eq!(field(&raw, "APP_FINGERPRINT").as_deref(), Some("none"));
  assert_eq!(field(&raw, "APP_OTHER").as_deref(), Some("fine"));
  assert_eq!(field(&raw, "LANG").as_deref(), Some("en_US.UTF-8"));
  assert_eq!(field(&raw, "ROUTE").as_deref(), Some("a"));

  // Route `b` accepts only the defaults.
  let (handle, _) = connect_as(server.addr, "b", Login::Keyless).await;
  let mut terminal = Terminal::open_with_env(
    &handle,
    "xterm-256color",
    80,
    24,
    false,
    &[("APP_OTHER", "fine")],
  )
  .await;
  assert!(terminal.wait_raw("SIZE=", WAIT).await);
  assert_eq!(
    field(&terminal.raw_text(), "APP_OTHER").as_deref(),
    Some("none")
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_route_cap_refuses_only_its_route() {
  let dir = temp_dir("route-cap");
  let server = TestServer::start(&two_routes(&dir, 1)).await;
  let (first, _) = connect_as(server.addr, "alice", Login::Keyless).await;
  let mut first = Terminal::open(&first, 80, 24).await;
  assert!(first.wait_raw("SIZE=", WAIT).await);

  let (handle, _) = connect_as(server.addr, "al", Login::Keyless).await;
  let mut second = Terminal::open(&handle, 80, 24).await;
  assert!(second.wait_closed(WAIT).await);
  assert!(
    second
      .raw_text()
      .contains("This service is full (1 sessions)"),
    "{}",
    second.raw_text()
  );
  assert_eq!(second.exit_status, Some(1));

  // The other route is not affected.
  let other = session_of(&server, "b", Login::Keyless).await;
  assert_eq!(field(&other, "PROBE").as_deref(), Some("b.sh"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn odd_user_names_are_routed_but_never_passed_on() {
  let dir = temp_dir("users");
  let server = TestServer::start(&two_routes(&dir, 8)).await;
  for user in ["a b", "$(reboot)", "x;y", "ünï", &"u".repeat(65)] {
    let raw = session_of(&server, user, Login::Keyless).await;
    assert_eq!(field(&raw, "PROBE").as_deref(), Some("b.sh"), "{user:?}");
    assert_eq!(field(&raw, "WHO").as_deref(), Some("none"), "{user:?}");
  }
  let raw = session_of(&server, "j.doe+x@example-1", Login::Keyless).await;
  assert_eq!(field(&raw, "WHO").as_deref(), Some("j.doe+x@example-1"));
  let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_connections_take_the_new_configuration() {
  let dir = temp_dir("reload");
  let server = TestServer::start(&two_routes(&dir, 8)).await;
  let (handle, _) = connect_as(server.addr, "nobody", Login::Keyless).await;
  let mut before = Terminal::open(&handle, 80, 24).await;
  assert!(before.wait_raw("SIZE=", WAIT).await);
  let swapped = two_routes(&dir, 8).replace("default_route = \"b\"", "default_route = \"a\"");
  server.reload(&swapped).unwrap();
  let raw = session_of(&server, "nobody", Login::Keyless).await;
  assert_eq!(field(&raw, "PROBE").as_deref(), Some("a.sh"));
  // The open session is untouched.
  before.resize(100, 30).await;
  assert!(before.wait_raw("SIZE=30 100", WAIT).await);
  assert!(server.reload("[routes]").is_err());
  let _ = std::fs::remove_dir_all(dir);
}

fn gateway() -> Command {
  Command::new(env!("CARGO_BIN_EXE_tty-gateway"))
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
  let path = dir.join(name);
  std::fs::write(&path, text).unwrap();
  path
}

#[test]
fn check_summarises_a_good_file_without_values() {
  let dir = temp_dir("check-good");
  let good = write(
    &dir,
    "good.toml",
    &format!(
      "{}\n[routes.c]\ncommand = [\"no-such-program-anywhere\", \"--flag\"]\n",
      two_routes(&dir, 2)
    ),
  );
  let output = gateway()
    .args(["--check", "--config"])
    .arg(&good)
    .env("SECRET", "topsecret")
    .env_remove("TTY_GATEWAY_ADDR")
    .output()
    .unwrap();
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "{stdout}\n{stderr}");
  assert!(stdout.contains("listen: 127.0.0.1:2222"), "{stdout}");
  assert!(stdout.contains("route a\n  users: alice, al"), "{stdout}");
  assert!(stdout.contains("route b (default)"), "{stdout}");
  assert!(stdout.contains("max_sessions 2"), "{stdout}");
  assert!(
    stdout.contains("env: APP_FINGERPRINT, ROUTE, SECRET_A, WHO"),
    "{stdout}"
  );
  assert!(
    stdout.contains("no-such-program-anywhere --flag"),
    "{stdout}"
  );
  assert!(!stdout.contains("topsecret") && !stderr.contains("topsecret"));
  // A program that is not there yet is a warning, not an error.
  assert!(stderr.contains("warning: routes.c.command"), "{stderr}");

  // The environment can name the file, and override the address.
  let output = gateway()
    .arg("--check")
    .env("TTY_GATEWAY_CONFIG", &good)
    .env("TTY_GATEWAY_ADDR", "0.0.0.0:2022")
    .output()
    .unwrap();
  assert!(output.status.success());
  assert!(String::from_utf8_lossy(&output.stdout).contains("listen: 0.0.0.0:2022"));
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn check_names_the_key_of_a_broken_file() {
  let dir = temp_dir("check-bad");
  let probe = quote_path(&probe_bin(&dir, "p.sh", &[]));
  for (text, key) in [
    (
      format!("[routes.a]\ncommand = [{probe}]\nidle_timeout = \"soon\"\n"),
      "routes.a.idle_timeout",
    ),
    (
      format!("[routes.a]\ncommand = [{probe}]\ncolour = true\n"),
      "colour",
    ),
    (
      format!(
        "[routes.a]\ncommand = [{probe}]\n[routes.b]\ncommand = [{probe}]\nusers = [\"a\"]\n"
      ),
      "users",
    ),
  ] {
    let bad = write(&dir, "bad.toml", &text);
    let output = gateway()
      .arg("--check")
      .arg(format!("--config={}", bad.display()))
      .output()
      .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{text}: {stderr}");
    assert!(stderr.contains(key), "{text}: {stderr}");
  }
  // No file at all, or no configuration named: a usage error.
  let output = gateway()
    .args(["--check", "--config"])
    .arg(dir.join("missing.toml"))
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let output = gateway().env_remove("TTY_GATEWAY_CONFIG").output().unwrap();
  assert_eq!(output.status.code(), Some(2));
  assert!(String::from_utf8_lossy(&output.stderr).contains("--config"));
  let output = gateway().arg("--version").output().unwrap();
  assert!(String::from_utf8_lossy(&output.stdout).starts_with("tty-gateway "));
  let _ = std::fs::remove_dir_all(dir);
}
