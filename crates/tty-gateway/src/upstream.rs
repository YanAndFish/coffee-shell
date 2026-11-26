//! Upstreams: what a session is connected to. For now a local program on a PTY; the
//! session driver only needs a byte stream, resizes and a way to stop it.

use std::{io, os::unix::process::ExitStatusExt, process::ExitStatus, time::Duration};

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

/// A started upstream.
pub struct Running {
  inner: Inner,
}

enum Inner {
  Command {
    child: tokio::process::Child,
    pty: Option<pty_process::Pty>,
    pid: Option<u32>,
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
  /// Starts the upstream for one session.
  pub fn start(&self, launch: &Launch) -> io::Result<Running> {
    match self {
      Upstream::Command(spec) => {
        let (child, pty) = spawn_child(spec, launch)?;
        let pid = child.id();
        Ok(Running {
          inner: Inner::Command {
            child,
            pty: Some(pty),
            pid,
          },
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
  /// 128 + the signal).
  pub async fn wait(&mut self) -> io::Result<u32> {
    match &mut self.inner {
      Inner::Command { child, .. } => child.wait().await.map(exit_code),
    }
  }

  /// Ends it: SIGHUP to the program's process group (it is a session leader on its own
  /// PTY), then SIGKILL if it is still there after `grace`. Says which it took.
  pub async fn stop(&mut self, grace: Duration) -> &'static str {
    match &mut self.inner {
      Inner::Command { child, pid, .. } => {
        let group = pid
          .and_then(|pid| i32::try_from(pid).ok())
          .and_then(rustix::process::Pid::from_raw);
        if let Some(group) = group {
          let _ = rustix::process::kill_process_group(group, rustix::process::Signal::HUP);
        }
        if tokio::time::timeout(grace, child.wait()).await.is_ok() {
          return "hangup";
        }
        if let Some(group) = group {
          let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
        "killed"
      }
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
  pub async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
    match &mut self.inner {
      InputInner::Pty(writer) => writer.write_all(bytes).await,
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
}
