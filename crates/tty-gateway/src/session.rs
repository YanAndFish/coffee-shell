//! One session: takes its place, starts the upstream, pumps the upstream's output to the
//! SSH channel and the client's input to the upstream until either side ends (or the
//! idle or time limit, or a shutdown), then ends the other.
//!
//! Nothing here blocks a russh handler: the connection hands input over through a
//! [`Link`] that never waits (an unbounded queue with a byte budget, the latest size in
//! a `watch`, hang-ups in another), and every call on the connection's [`Handle`] on the
//! way out has a timeout (its queue is bounded and is not drained while the client's
//! window is full).

use std::{
  future::Future,
  io,
  net::IpAddr,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use bytes::Bytes;
use russh::{ChannelId, server::Handle};
use tokio::{
  sync::{mpsc, watch},
  task::JoinHandle,
};

use crate::{
  config::Route,
  limits::{Caps, Limits, Refusal},
  upstream::{Input, Launch},
};

/// How long a session's program has to leave after SIGHUP before its group is killed.
pub const KILL_GRACE: Duration = Duration::from_millis(1000);
/// Client input queued for a program that does not read it, at most.
pub const INPUT_BUDGET: usize = 1024 * 1024;
/// The exit status of a session the server ended by shutting down (128 + SIGTERM): a client
/// can tell it from its program leaving on its own.
pub const SHUTDOWN_STATUS: u32 = 143;
/// How long one call on the connection's handle may take on the way out.
pub const HANDLE_TIMEOUT: Duration = Duration::from_millis(250);
/// Tries at starting a session's upstream, and the pause between them.
const SPAWN_ATTEMPTS: u32 = 3;
const SPAWN_RETRY: Duration = Duration::from_millis(150);

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

/// Why the client side ended a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hangup {
  /// The channel was closed, or the connection is gone.
  Closed,
  /// The client sent EOF.
  Eof,
  /// The client sent more than [`INPUT_BUDGET`] the program did not take.
  Overflow,
}

/// The connection's end of a session: never waits.
#[derive(Debug)]
pub struct Link {
  input: mpsc::UnboundedSender<Vec<u8>>,
  queued: Arc<AtomicUsize>,
  size: watch::Sender<(u16, u16)>,
  hangup: watch::Sender<Option<Hangup>>,
}

/// The session's end.
#[derive(Debug)]
pub struct LinkEnd {
  input: mpsc::UnboundedReceiver<Vec<u8>>,
  queued: Arc<AtomicUsize>,
  size: watch::Receiver<(u16, u16)>,
  hangup: watch::Receiver<Option<Hangup>>,
}

/// A new link for a terminal of `cols`×`rows`.
pub fn link(cols: u16, rows: u16) -> (Link, LinkEnd) {
  let (input, input_rx) = mpsc::unbounded_channel();
  let queued = Arc::new(AtomicUsize::new(0));
  let (size, size_rx) = watch::channel((cols, rows));
  let (hangup, hangup_rx) = watch::channel(None);
  (
    Link {
      input,
      queued: queued.clone(),
      size,
      hangup,
    },
    LinkEnd {
      input: input_rx,
      queued,
      size: size_rx,
      hangup: hangup_rx,
    },
  )
}

impl Link {
  /// Queues client input; `false` when it is over the budget (the session is then
  /// ended, and nothing more is taken).
  pub fn input(&self, bytes: &[u8]) -> bool {
    if self.hangup.borrow().is_some() {
      return true;
    }
    let before = self.queued.fetch_add(bytes.len(), Ordering::AcqRel);
    if before + bytes.len() > INPUT_BUDGET {
      self.hang_up(Hangup::Overflow);
      return false;
    }
    let _ = self.input.send(bytes.to_vec());
    true
  }

  /// The terminal's new size (the latest one wins).
  pub fn resize(&self, cols: u16, rows: u16) {
    self.size.send_replace((cols, rows));
  }

  /// Ends the session from the client's side (the first reason counts).
  pub fn hang_up(&self, why: Hangup) {
    self.hangup.send_if_modified(|current| {
      if current.is_some() {
        return false;
      }
      *current = Some(why);
      true
    });
  }
}

/// Marks a connection as having a session until dropped.
#[derive(Debug)]
pub struct Alive(pub Arc<AtomicBool>);

impl Alive {
  pub fn new(flag: Arc<AtomicBool>) -> Self {
    flag.store(true, Ordering::Release);
    Self(flag)
  }
}

impl Drop for Alive {
  fn drop(&mut self) {
    self.0.store(false, Ordering::Release);
  }
}

/// 会话任务所需的路由、终端设置和连接状态。
pub struct SessionTask {
  /// 用于日志的连接编号。
  pub conn: u64,
  /// 本次登录选中的路由。
  pub route: Arc<Route>,
  /// 会话的全部环境变量。
  pub env: Vec<(String, String)>,
  /// 删除光标左侧字符的按键值。`None` 使用系统默认值，255 表示禁用。
  pub erase: Option<u8>,
  /// 接收客户端输入、终端尺寸和断开通知。
  pub link: LinkEnd,
  /// 向 SSH 连接发送数据的句柄。
  pub handle: Handle,
  /// 本次会话使用的 SSH 通道。
  pub channel: ChannelId,
  /// 客户端最后输入或调整终端尺寸的时间，使用 `clock` 的毫秒数。
  pub last_input: Arc<AtomicU64>,
  /// 从服务启动时开始计时的时钟。
  pub clock: Clock,
  /// 所有会话共用的数量限制。
  pub limits: Arc<Limits>,
  /// 客户端地址，用于按地址限制会话数量。
  pub ip: IpAddr,
  /// 当前配置允许的总会话数和每个地址的会话数。
  pub caps: Caps,
  /// 服务关闭时变为 `true`。
  pub shutdown: watch::Receiver<bool>,
  /// 服务关闭后等待会话程序退出的时间。
  pub shutdown_grace: Duration,
  /// 标记连接上的会话正在运行，释放时清除标记。
  pub alive: Alive,
}

/// How a session ended.
pub enum End {
  Exited(io::Result<u32>),
  Hangup(Hangup),
  Idle,
  TimeLimit,
  Shutdown,
}

/// Resolves once `shutdown` is `true`; never if the server is gone without one.
async fn shut(shutdown: &mut watch::Receiver<bool>) {
  if shutdown.wait_for(|down| *down).await.is_err() {
    std::future::pending::<()>().await;
  }
}

/// Resolves once the client hangs up (the connection gone counts as closed).
async fn hung_up(hangup: &mut watch::Receiver<Option<Hangup>>) -> Hangup {
  match hangup.wait_for(Option::is_some).await {
    Ok(why) => why.unwrap_or(Hangup::Closed),
    Err(_) => Hangup::Closed,
  }
}

/// `call` within [`HANDLE_TIMEOUT`].
pub async fn within<T>(call: impl Future<Output = T>) -> Option<T> {
  tokio::time::timeout(HANDLE_TIMEOUT, call).await.ok()
}

/// What the client is told when its session is refused, in the route's words.
pub fn refusal_message(route: &Route, refusal: Refusal) -> String {
  let messages = &route.messages;
  match refusal {
    Refusal::Full { max } => messages.full(max),
    Refusal::PerIp { max } => messages.full_per_ip(max),
    Refusal::Route { max } => messages.full_route(max),
  }
}

/// Runs one session from its place to its end.
pub async fn run_session(task: SessionTask) {
  let SessionTask {
    conn,
    route,
    env,
    erase,
    link,
    handle,
    channel,
    last_input,
    clock,
    limits,
    ip,
    caps,
    mut shutdown,
    shutdown_grace,
    alive,
  } = task;
  let _alive = alive;
  let LinkEnd {
    input,
    queued,
    mut size,
    mut hangup,
  } = link;
  let route_name = route.name.as_str();

  // The place is taken only now (and held by this task: freed on every way out).
  let _slot = match limits.acquire(ip, route_name, caps, route.limits.max_sessions) {
    Ok(slot) => slot,
    Err(refusal) => {
      tracing::warn!(
        conn,
        route = %route_name,
        refused = refusal.reason(),
        "session refused"
      );
      tell_and_close(&handle, channel, &refusal_message(&route, refusal), 1).await;
      return;
    }
  };

  // A start can fail for a moment (no process or PTY to spare under load, the program
  // being replaced): try again shortly before giving up on the session.
  let mut attempt = 1;
  let mut running = loop {
    let (cols, rows) = *size.borrow_and_update();
    let launch = Launch {
      env: env.clone(),
      cols,
      rows,
      erase,
    };
    let starting = route.clone();
    let started = tokio::task::spawn_blocking(move || starting.upstream.start(&launch))
      .await
      .unwrap_or_else(|error| Err(io::Error::other(error)));
    match started {
      Ok(running) => break running,
      Err(error) if attempt < SPAWN_ATTEMPTS => {
        tracing::warn!(
          conn,
          route = %route_name,
          attempt,
          %error,
          "could not start the session program; trying again"
        );
        tokio::select! {
          () = tokio::time::sleep(SPAWN_RETRY) => {}
          _ = hung_up(&mut hangup) => return,
          () = shut(&mut shutdown) => return,
        }
        attempt += 1;
      }
      Err(error) => {
        tracing::error!(
          conn,
          route = %route_name,
          upstream = %route.upstream.describe(),
          attempt,
          %error,
          "could not start the session program"
        );
        tell_and_close(&handle, channel, &route.messages.unavailable, 1).await;
        return;
      }
    }
  };
  let started = Instant::now();
  let pid = running.pid();
  tracing::info!(
    conn,
    route = %route_name,
    pid = pid.unwrap_or_default(),
    active = limits.active(),
    "session program started"
  );
  let Some((mut reader, writer)) = running.split() else {
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
  // never holds up anything else.
  let feeder = tokio::spawn(feed(writer, input, queued, size));

  let idle = route.limits.idle_timeout;
  let time_limit = route
    .limits
    .max_session_time
    .map(|limit| tokio::time::Instant::now() + limit);
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
      why = hung_up(&mut hangup) => break End::Hangup(why),
      () = shut(&mut shutdown) => break End::Shutdown,
      () = async {
        match time_limit {
          Some(at) => tokio::time::sleep_until(at).await,
          None => std::future::pending().await,
        }
      } => break End::TimeLimit,
      () = tokio::time::sleep(check) => {
        if let (Some(idle), Some(quiet)) = (idle, quiet()) && quiet >= idle {
          break End::Idle;
        }
      }
    }
  };
  feeder.abort();
  let seconds = || started.elapsed().as_secs();

  match end {
    End::Exited(status) => {
      // Whatever the program left in its group goes too (signalled at once: see
      // `Running::hang_up`).
      running.hang_up();
      // The last frame (the restored screen) may still be in the PTY.
      if tokio::time::timeout(Duration::from_millis(500), &mut pump)
        .await
        .is_err()
      {
        pump.abort();
      }
      let code = status.unwrap_or(255);
      if code == 0 {
        tracing::info!(
          conn,
          route = %route_name,
          ?pid,
          code,
          seconds = seconds(),
          "session ended: program exited"
        );
      } else {
        // It crashed or gave up: it may have left the client's terminal in its modes,
        // and a bare close would leave the user guessing.
        tracing::warn!(
          conn,
          route = %route_name,
          ?pid,
          code,
          seconds = seconds(),
          "session ended: program failed"
        );
        let notice = format!("{TERMINAL_RESET}\r\n{}\r\n", route.messages.crashed(code));
        within(handle.data(channel, Bytes::from(notice.into_bytes()))).await;
      }
      close(&handle, channel, code).await;
      running.stop(KILL_GRACE).await;
    }
    End::Hangup(why) => {
      let how = running.stop(KILL_GRACE).await;
      if why == Hangup::Eof {
        // The client still reads: what the program wrote last goes out first.
        let _ = tokio::time::timeout(Duration::from_millis(300), &mut pump).await;
      }
      pump.abort();
      match why {
        Hangup::Closed => {
          tracing::info!(
            conn,
            route = %route_name,
            ?pid,
            how,
            seconds = seconds(),
            "session ended: client left"
          );
        }
        Hangup::Eof => {
          tracing::info!(
            conn,
            route = %route_name,
            ?pid,
            how,
            seconds = seconds(),
            "session ended: client sent EOF"
          );
          let code = running.wait().await.unwrap_or(255);
          close(&handle, channel, code).await;
        }
        Hangup::Overflow => {
          tracing::warn!(
            conn,
            route = %route_name,
            ?pid,
            how,
            budget = INPUT_BUDGET,
            "session ended: input the program did not read"
          );
          disconnect(&handle, "input overflow").await;
        }
      }
    }
    End::Idle | End::TimeLimit => {
      let how = running.stop(KILL_GRACE).await;
      let _ = tokio::time::timeout(Duration::from_millis(300), &mut pump).await;
      pump.abort();
      let message = if matches!(end, End::Idle) {
        let minutes = idle.map_or(0, |idle| idle.as_secs() / 60);
        tracing::info!(conn, route = %route_name, ?pid, how, "session ended: idle");
        route.messages.idle(minutes)
      } else {
        tracing::info!(
          conn,
          route = %route_name,
          ?pid,
          how,
          seconds = seconds(),
          "session ended: time limit"
        );
        route.messages.time_limit.clone()
      };
      tell_and_close(&handle, channel, &message, 124).await;
      disconnect(&handle, "session over").await;
    }
    End::Shutdown => {
      pump.abort();
      let notice = format!("{TERMINAL_RESET}\r\n{}\r\n", route.messages.shutting_down);
      within(handle.data(channel, Bytes::from(notice.into_bytes()))).await;
      let how = running.stop(shutdown_grace).await;
      tracing::info!(
        conn,
        route = %route_name,
        ?pid,
        how,
        seconds = seconds(),
        "session ended: shutting down"
      );
      close(&handle, channel, SHUTDOWN_STATUS).await;
      disconnect(&handle, "shutting down").await;
    }
  }
}

/// Writes queued input to the upstream, and resizes it; a write that does not complete
/// (a program that does not read) never holds up a resize.
async fn feed(
  mut writer: Input,
  mut input: mpsc::UnboundedReceiver<Vec<u8>>,
  queued: Arc<AtomicUsize>,
  mut size: watch::Receiver<(u16, u16)>,
) {
  enum Step {
    Data(Option<Vec<u8>>),
    Wrote(io::Result<usize>),
    Resize(bool),
  }
  let mut pending: Option<(Vec<u8>, usize)> = None;
  let mut sizes = true;
  loop {
    let step = match &pending {
      None => tokio::select! {
        bytes = input.recv() => Step::Data(bytes),
        changed = size.changed(), if sizes => Step::Resize(changed.is_ok()),
      },
      Some((bytes, at)) => tokio::select! {
        wrote = writer.write(&bytes[*at..]) => Step::Wrote(wrote),
        changed = size.changed(), if sizes => Step::Resize(changed.is_ok()),
      },
    };
    match step {
      Step::Data(None) | Step::Wrote(Ok(0) | Err(_)) => return,
      Step::Data(Some(bytes)) => {
        if bytes.is_empty() {
          continue;
        }
        pending = Some((bytes, 0));
      }
      Step::Wrote(Ok(wrote)) => {
        queued.fetch_sub(wrote, Ordering::AcqRel);
        if let Some((bytes, at)) = &mut pending {
          *at += wrote;
          if *at >= bytes.len() {
            pending = None;
          }
        }
      }
      Step::Resize(true) => {
        let (cols, rows) = *size.borrow_and_update();
        let _ = writer.resize(cols, rows);
      }
      Step::Resize(false) => sizes = false,
    }
  }
}

/// Sends the exit status, EOF and the close, each within [`HANDLE_TIMEOUT`].
async fn close(handle: &Handle, channel: ChannelId, status: u32) {
  within(handle.exit_status_request(channel, status)).await;
  within(handle.eof(channel)).await;
  within(handle.close(channel)).await;
}

/// Ends the connection, within [`HANDLE_TIMEOUT`].
pub async fn disconnect(handle: &Handle, why: &str) {
  within(handle.disconnect(russh::Disconnect::ByApplication, why.into(), String::new())).await;
}

/// Writes `message` on the channel, then ends it with `status`.
pub async fn tell_and_close(handle: &Handle, channel: ChannelId, message: &str, status: u32) {
  let text = format!("\r\n{message}\r\n");
  within(handle.data(channel, Bytes::from(text.into_bytes()))).await;
  close(handle, channel, status).await;
}
