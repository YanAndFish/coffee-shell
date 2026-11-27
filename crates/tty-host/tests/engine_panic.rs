//! A panic on nsh's engine thread, end to end on a real PTY: the host restores the
//! terminal, draws nothing more, prints what `quiet_panics` asks for and `run` returns
//! [`tty_host::Panicked`], so the process exits non-zero.
//!
//! The test binary runs itself as the program under test ([`child`], which does nothing
//! unless `TTY_HOST_PANIC_CHILD` is set) on a PTY driven by the repository's
//! `scripts/pty-run.py` (python3, standard library only), which records the raw output.

use std::{path::PathBuf, process::Command};

use nsh_core::prelude::*;
use tty_host::{HostOptions, Root};

const MESSAGE: &str = "probe engine panic";
/// The child's exit status when `run` returned [`tty_host::Panicked`].
const PANICKED: i32 = 3;

/// The root: its body runs on the engine thread and panics there.
#[component]
fn Boom() -> impl IntoElement {
  panic!("{MESSAGE}");
  #[allow(unreachable_code)]
  text("never")
}

/// The program under test, when the parent runs it.
#[test]
fn child() {
  let Ok(mode) = std::env::var("TTY_HOST_PANIC_CHILD") else {
    return;
  };
  let options = HostOptions::new("probe", Root::of::<Boom>()).quiet_panics(mode == "quiet");
  let code = match tty_host::run(options) {
    Ok(()) => 0,
    Err(error) if tty_host::is_panic(&error) => PANICKED,
    Err(_) => 4,
  };
  std::process::exit(code);
}

struct Run {
  raw: Vec<u8>,
  summary: String,
}

/// Runs [`child`] in `mode` on an 80x24 PTY until it exits (at most 20 s).
fn run_child(mode: &str) -> Run {
  let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/pty-run.py");
  let raw = std::env::temp_dir().join(format!(
    "tty-host-engine-panic-{mode}-{}.bin",
    std::process::id()
  ));
  let output = Command::new("python3")
    .arg(&script)
    .args(["--size", "80x24", "--settle", "0.2", "--timeout", "20"])
    .args(["--step", "exit=20", "--raw"])
    .arg(&raw)
    .args(["--env", &format!("TTY_HOST_PANIC_CHILD={mode}"), "--"])
    .arg(std::env::current_exe().unwrap())
    .args(["child", "--exact", "--nocapture"])
    .output()
    .expect("python3 runs scripts/pty-run.py");
  let summary = String::from_utf8_lossy(&output.stderr).into_owned();
  assert!(output.status.success(), "pty-run failed: {summary}");
  let bytes = std::fs::read(&raw).unwrap();
  let _ = std::fs::remove_file(&raw);
  Run {
    raw: bytes,
    summary,
  }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack.windows(needle.len()).position(|w| w == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// The terminal was restored after the panic and nothing was drawn on it afterwards: the
/// last thing the host wrote leaves the alternate screen, shows the cursor and turns the
/// input modes off; what follows is the hook's report alone.
fn restored(run: &Run) -> &[u8] {
  const LEAVE: &[u8] = b"\x1b[?1049l";
  let leave = rfind(&run.raw, LEAVE).expect("the alternate screen was left");
  assert!(
    rfind(&run.raw, b"\x1b[?1049h").is_some_and(|enter| enter < leave),
    "entered before it was left"
  );
  let restore = &run.raw[..leave];
  for mode in [&b"\x1b[?1000l"[..], b"\x1b[?2004l", b"\x1b[?1004l"] {
    assert!(rfind(restore, mode).is_some(), "{mode:?} turned off");
  }
  let after = &run.raw[leave + LEAVE.len()..];
  // No frame after the restore: no cursor positioning, no synchronized update, no colour.
  for drawn in [&b"\x1b[?2026h"[..], b"H\x1b[", b"\x1b[38;", b"\x1b[48;"] {
    assert!(
      find(after, drawn).is_none(),
      "drawn after the restore: {:?}",
      String::from_utf8_lossy(after)
    );
  }
  assert!(
    run
      .summary
      .contains(&format!("program exited with code {PANICKED}")),
    "{}",
    run.summary
  );
  after
}

#[test]
fn a_quiet_engine_panic_restores_the_terminal_and_prints_one_neutral_line() {
  let run = run_child("quiet");
  let after = String::from_utf8_lossy(restored(&run)).into_owned();
  assert!(
    after.contains("probe: the session ended after an internal error"),
    "{after:?}"
  );
  let everything = String::from_utf8_lossy(&run.raw);
  for secret in [MESSAGE, "panicked at", ".rs:", "backtrace"] {
    assert!(!everything.contains(secret), "{secret:?} in {everything:?}");
  }
}

#[test]
fn a_loud_engine_panic_restores_the_terminal_then_reports_it_in_full() {
  let run = run_child("full");
  let after = String::from_utf8_lossy(restored(&run)).into_owned();
  assert!(after.contains(MESSAGE), "{after:?}");
  assert!(after.contains("panicked at"), "{after:?}");
  // On the engine thread, not the one that called `run`.
  assert!(!after.contains("thread 'child'"), "{after:?}");
  assert!(!after.contains("thread 'main'"), "{after:?}");
}
