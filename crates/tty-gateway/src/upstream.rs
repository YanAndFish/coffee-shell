//! Upstreams: what a session is connected to. For now a local program on a PTY; the
//! session driver only needs a byte stream, resizes and a way to stop it.

use std::{
  io,
  os::unix::process::ExitStatusExt,
  process::ExitStatus,
  time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::config::{CommandSpec, Upstream};

/// What starting a session needs.
#[derive(Clone, Debug)]
pub struct Launch {
  /// The whole environment of the session (nothing else is inherited).
  pub env: Vec<(String, String)>,
  pub cols: u16,
  pub rows: u16,
}

/// A started upstream. Dropped before [`Running::stop`] finished (a cancelled or
/// panicking session task), it kills what is left of it.
pub struct Running {
  inner: Inner,
  /// Stopped for good: nothing left to kill.
  finished: bool,
}

enum Inner {
  Command {
    child: tokio::process::Child,
    pty: Option<pty_process::Pty>,
    pid: Option<u32>,
    /// The program's process group (it is a session leader: pgid == pid).
    group: Option<Pid>,
    hung_up: bool,
  },
}

/// What the upstream writes (to the client).
pub struct Output {
  inner: OutputInner,
}

enum OutputInner {
  Pty(pty_process::OwnedReadPty),
}

/// What the upstream reads (from the client), and its terminal size.
pub struct Input {
  inner: InputInner,
}

enum InputInner {
  Pty(pty_process::OwnedWritePty),
}

impl Upstream {
  /// Starts the upstream for one session. For a command this forks and execs: call it
  /// off the async threads (`spawn_blocking`).
  pub fn start(&self, launch: &Launch) -> io::Result<Running> {
    match self {
      Upstream::Command(spec) => {
        let (child, pty) = spawn_child(spec, launch)?;
        let pid = child.id();
        let group = pid
          .and_then(|pid| i32::try_from(pid).ok())
          .and_then(Pid::from_raw);
        Ok(Running {
          inner: Inner::Command {
            child,
            pty: Some(pty),
            pid,
            group,
            hung_up: false,
          },
          finished: false,
        })
      }
    }
  }

  /// What the logs call it: the program, for a command.
  pub fn describe(&self) -> String {
    match self {
      Upstream::Command(spec) => spec.program.display().to_string(),
    }
  }
}

/// Starts the program on a new PTY of the launch's size, as its session leader.
fn spawn_child(
  spec: &CommandSpec,
  launch: &Launch,
) -> io::Result<(tokio::process::Child, pty_process::Pty)> {
  let program = spec.resolve().ok_or_else(|| {
    io::Error::new(
      io::ErrorKind::NotFound,
      format!(
        "{} is neither next to the gateway nor on PATH",
        spec.program.display()
      ),
    )
  })?;
  let (master, slave) = pty_process::open().map_err(io::Error::other)?;
  master
    .resize(pty_process::Size::new(launch.rows, launch.cols))
    .map_err(io::Error::other)?;
  let child = pty_process::Command::new(&program)
    .args(&spec.args)
    .env_clear()
    .envs(launch.env.iter().cloned())
    .current_dir(&spec.cwd)
    .kill_on_drop(true)
    .spawn(slave)
    .map_err(io::Error::other)?;
  Ok((child, master))
}

/// Whether anything is left in process group `group`.
fn group_alive(group: Option<Pid>) -> bool {
  group.is_some_and(|group| test_kill_process_group(group).is_ok())
}

fn signal_group(group: Option<Pid>, signal: Signal) {
  if let Some(group) = group {
    let _ = kill_process_group(group, signal);
  }
}

impl Running {
  /// The process id, for logs (a command's).
  pub fn pid(&self) -> Option<u32> {
    match &self.inner {
      Inner::Command { pid, .. } => *pid,
    }
  }

  /// The two ends of the stream; `None` once taken.
  pub fn split(&mut self) -> Option<(Output, Input)> {
    match &mut self.inner {
      Inner::Command { pty, .. } => {
        let (reader, writer) = pty.take()?.into_split();
        Some((
          Output {
            inner: OutputInner::Pty(reader),
          },
          Input {
            inner: InputInner::Pty(writer),
          },
        ))
      }
    }
  }

  /// Waits until the upstream ends; its exit code as the client sees it (the code, or
  /// 128 + the signal). Call [`Running::hang_up`] right after: what the program started
  /// may still be running.
  pub async fn wait(&mut self) -> io::Result<u32> {
    match &mut self.inner {
      Inner::Command { child, .. } => child.wait().await.map(exit_code),
    }
  }

  /// SIGHUP to the program's process group, once (later calls do nothing).
  ///
  /// After the program itself has exited and was reaped, its pid names the group only
  /// while something of the group is left; sending this right after [`Running::wait`]
  /// keeps the window in which the number could be reused small. Not covered: a
  /// descendant that left the group (`setsid`, `setpgid`) escapes.
  pub fn hang_up(&mut self) {
    match &mut self.inner {
      Inner::Command { group, hung_up, .. } => {
        if !std::mem::replace(hung_up, true) {
          signal_group(*group, Signal::HUP);
        }
      }
    }
  }

  /// Ends it: SIGHUP to the process group, then SIGKILL to the group if the program or
  /// anything else of its group is still there after `grace`. Says which it took.
  pub async fn stop(&mut self, grace: Duration) -> &'static str {
    self.hang_up();
    let how = match &mut self.inner {
      Inner::Command { child, group, .. } => {
        let group = *group;
        let deadline = Instant::now() + grace;
        let leader_gone = tokio::time::timeout(grace, child.wait()).await.is_ok();
        while group_alive(group) && Instant::now() < deadline {
          tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if leader_gone && !group_alive(group) {
          "hangup"
        } else {
          signal_group(group, Signal::KILL);
          let _ = child.start_kill();
          let _ = child.wait().await;
          "killed"
        }
      }
    };
    self.finished = true;
    how
  }
}

impl Drop for Running {
  fn drop(&mut self) {
    if self.finished {
      return;
    }
    match &self.inner {
      // The program itself is killed by `kill_on_drop`; this takes the rest of its group.
      Inner::Command { group, .. } => signal_group(*group, Signal::KILL),
    }
  }
}

impl Output {
  /// Reads what the upstream wrote; 0 at its end.
  pub async fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
    match &mut self.inner {
      OutputInner::Pty(reader) => reader.read(buffer).await,
    }
  }
}

impl Input {
  /// Writes some of `bytes`; how many. Cancel-safe: dropped before it completes, it
  /// wrote nothing.
  pub async fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    match &mut self.inner {
      InputInner::Pty(writer) => writer.write(bytes).await,
    }
  }

  pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
    match &self.inner {
      InputInner::Pty(writer) => writer
        .resize(pty_process::Size::new(rows, cols))
        .map_err(io::Error::other),
    }
  }
}

/// The exit status the client sees: the code, or 128 + the signal.
pub fn exit_code(status: ExitStatus) -> u32 {
  match (status.code(), status.signal()) {
    (Some(code), _) => u32::try_from(code).unwrap_or(255),
    (None, Some(signal)) => 128 + u32::try_from(signal).unwrap_or(0),
    (None, None) => 255,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exit_codes_follow_the_shell_convention() {
    assert_eq!(exit_code(ExitStatus::from_raw(0)), 0);
    assert_eq!(exit_code(ExitStatus::from_raw(3 << 8)), 3);
    assert_eq!(exit_code(ExitStatus::from_raw(9)), 137);
  }

  /// What a cancelled or panicking session task leaves: its `Running` dropped mid-way.
  #[tokio::test(flavor = "multi_thread")]
  async fn dropping_a_running_upstream_kills_its_whole_group() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("tty-gateway-drop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("family.sh");
    let marker = dir.join("grandchild");
    std::fs::write(
      &script,
      format!(
        "#!/bin/sh\n(trap '' HUP; exec sleep 300) &\necho $! > {}\nwhile :; do sleep 0.05; done\n",
        marker.display()
      ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let upstream = Upstream::Command(CommandSpec {
      program: script,
      args: Vec::new(),
      cwd: dir.clone(),
      exe_dir: None,
    });
    let running = upstream
      .start(&Launch {
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        cols: 80,
        rows: 24,
      })
      .unwrap();
    let start = Instant::now();
    let grandchild = loop {
      if let Some(pid) = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
      {
        break Pid::from_raw(pid).unwrap();
      }
      assert!(start.elapsed() < Duration::from_secs(5), "no grandchild");
      tokio::time::sleep(Duration::from_millis(20)).await;
    };
    drop(running);
    let start = Instant::now();
    while rustix::process::test_kill_process(grandchild).is_ok()
      && start.elapsed() < Duration::from_secs(2)
    {
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Gone, or a zombie about to be reaped by init: `ps` says which.
    let stat = std::process::Command::new("ps")
      .args([
        "-o",
        "stat=",
        "-p",
        &grandchild.as_raw_nonzero().to_string(),
      ])
      .output()
      .unwrap();
    let stat = String::from_utf8_lossy(&stat.stdout);
    assert!(
      stat.trim().is_empty() || stat.trim().starts_with('Z'),
      "grandchild still running: {stat}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
  }
}
