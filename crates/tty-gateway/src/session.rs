//! One running session: pumps the upstream's output to the SSH channel and the client's
//! input to the upstream until either side ends, then ends the other.

use std::{
  io,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use bytes::Bytes;
use russh::{ChannelId, server::Handle};
use tokio::{
  sync::{mpsc, oneshot},
  task::JoinHandle,
};

use crate::{config::Route, limits::Slot, upstream::Running};

/// How long a session's program has to leave after SIGHUP before it is killed.
pub const KILL_GRACE: Duration = Duration::from_millis(1000);

/// Undoes what a session program that died may have left on the client's terminal: kitty
/// keys, mouse, paste and focus reports, alternate scroll, a synchronized update, the
/// alternate screen, a hidden cursor, colours.
pub const TERMINAL_RESET: &str = "\x1b[<u\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?2004l\x1b[?1004l\x1b[?1007l\x1b[?2026l\x1b[?1049l\x1b[?25h\x1b[0m";

/// Milliseconds since the server started: what the idle clock counts in.
#[derive(Clone, Copy, Debug)]
pub struct Clock(pub Instant);

impl Clock {
  pub fn millis(self) -> u64 {
    u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
  }
}

/// What a session task gets from its connection.
pub enum ToUpstream {
  Input(Vec<u8>),
  Resize { cols: u16, rows: u16 },
}

/// Everything a session task needs.
pub struct SessionTask {
  pub conn: u64,
  pub route: Arc<Route>,
  pub running: Running,
  pub from_connection: mpsc::Receiver<ToUpstream>,
  pub handle: Handle,
  pub channel: ChannelId,
  /// When the client last typed or resized, on `clock`.
  pub last_input: Arc<AtomicU64>,
  pub clock: Clock,
  pub slot: Slot,
}

/// How a session ended.
pub enum End {
  Exited(io::Result<u32>),
  Disconnected,
  Idle,
}

/// Pumps the upstream to the channel and the channel to the upstream until it ends or
/// the client goes; then stops it (see [`Running::stop`]) and closes the channel with its
/// exit status.
pub async fn run_session(task: SessionTask) {
  let SessionTask {
    conn,
    route,
    mut running,
    mut from_connection,
    handle,
    channel,
    last_input,
    clock,
    slot,
  } = task;
  let _slot = slot;
  let started = Instant::now();
  let pid = running.pid();
  let Some((mut reader, mut writer)) = running.split() else {
    return;
  };

  // The idle clock runs from the program's first output: the time a client takes to
  // connect, and the program to start, never counts against the session.
  let shown = Arc::new(AtomicBool::new(false));
  let output = handle.clone();
  let (first_output, idle_since) = (shown.clone(), last_input.clone());
  let mut pump: JoinHandle<()> = tokio::spawn(async move {
    let mut buffer = vec![0u8; 32 * 1024];
    loop {
      match reader.read(&mut buffer).await {
        Ok(0) | Err(_) => break,
        Ok(read) => {
          if !first_output.load(Ordering::Relaxed) {
            idle_since.store(clock.millis(), Ordering::Relaxed);
            first_output.store(true, Ordering::Release);
          }
          if output
            .data(channel, Bytes::copy_from_slice(&buffer[..read]))
            .await
            .is_err()
          {
            break;
          }
        }
      }
    }
  });

  // Input and resizes go to the upstream on their own task, so one that stops reading
  // can never keep a disconnect from being noticed.
  let (gone_tx, mut gone) = oneshot::channel::<()>();
  let feeder = tokio::spawn(async move {
    while let Some(message) = from_connection.recv().await {
      match message {
        ToUpstream::Input(bytes) => {
          if writer.write_all(&bytes).await.is_err() {
            break;
          }
        }
        ToUpstream::Resize { cols, rows } => {
          let _ = writer.resize(cols, rows);
        }
      }
    }
    let _ = gone_tx.send(());
  });

  let idle = route.limits.idle_timeout;
  // How long ago the last input (or the first output) was, once there was output.
  let quiet = || {
    shown.load(Ordering::Acquire).then(|| {
      let since = clock
        .millis()
        .saturating_sub(last_input.load(Ordering::Relaxed));
      Duration::from_millis(since)
    })
  };
  let end = loop {
    let check = match (idle, quiet()) {
      (Some(idle), Some(quiet)) => idle.saturating_sub(quiet),
      // Nothing shown yet: look again after a full period.
      (Some(idle), None) => idle,
      (None, _) => Duration::from_secs(3600),
    };
    tokio::select! {
      status = running.wait() => break End::Exited(status),
      _ = &mut gone => break End::Disconnected,
      () = tokio::time::sleep(check) => {
        if let (Some(idle), Some(quiet)) = (idle, quiet()) && quiet >= idle {
          break End::Idle;
        }
      }
    }
  };

  let route_name = route.name.as_str();
  match end {
    End::Exited(status) => {
      feeder.abort();
      // The last frame (the restored screen) may still be in the PTY.
      if tokio::time::timeout(Duration::from_millis(500), &mut pump)
        .await
        .is_err()
      {
        pump.abort();
      }
      let code = status.unwrap_or(255);
      let seconds = started.elapsed().as_secs();
      if code == 0 {
        tracing::info!(
          conn,
          route = route_name,
          ?pid,
          code,
          seconds,
          "session ended: program exited"
        );
      } else {
        // It crashed or gave up: it may have left the client's terminal in its modes,
        // and a bare close would leave the user guessing.
        tracing::warn!(
          conn,
          route = route_name,
          ?pid,
          code,
          seconds,
          "session ended: program failed"
        );
        let notice = format!("{TERMINAL_RESET}\r\n{}\r\n", route.messages.crashed(code));
        let _ = handle.data(channel, Bytes::from(notice.into_bytes())).await;
      }
      let _ = handle.exit_status_request(channel, code).await;
      let _ = handle.eof(channel).await;
      let _ = handle.close(channel).await;
    }
    End::Disconnected => {
      feeder.abort();
      let how = running.stop(KILL_GRACE).await;
      pump.abort();
      tracing::info!(
        conn,
        route = route_name,
        ?pid,
        how,
        seconds = started.elapsed().as_secs(),
        "session ended: client left"
      );
    }
    End::Idle => {
      feeder.abort();
      let how = running.stop(KILL_GRACE).await;
      let _ = tokio::time::timeout(Duration::from_millis(300), &mut pump).await;
      pump.abort();
      let minutes = idle.map_or(0, |idle| idle.as_secs() / 60);
      tracing::info!(conn, route = route_name, ?pid, how, "session ended: idle");
      tell_and_close(&handle, channel, &route.messages.idle(minutes), 124).await;
      let _ = handle
        .disconnect(
          russh::Disconnect::ByApplication,
          "idle".into(),
          String::new(),
        )
        .await;
    }
  }
}

/// Writes `message` on the channel, then ends it with `status`.
pub async fn tell_and_close(handle: &Handle, channel: ChannelId, message: &str, status: u32) {
  let text = format!("\r\n{message}\r\n");
  let _ = handle.data(channel, Bytes::from(text.into_bytes())).await;
  let _ = handle.exit_status_request(channel, status).await;
  let _ = handle.eof(channel).await;
  let _ = handle.close(channel).await;
}
