//! The terminal this process runs in: modes, startup queries, output, lifetime.
//!
//! Adapted from nsh's own development host (`nsh-tui-host` 0.6.1, Apache-2.0:
//! `terminal.rs`, `terminal/unix.rs`, `shutdown.rs`), with `rustix` and `signal-hook` in
//! place of raw `libc` calls so this crate needs no `unsafe`:
//!
//! - SIGHUP, SIGTERM and SIGINT set a flag; the run loop sees it within one poll and
//!   leaves, restoring the terminal ([`Shutdown`]).
//! - stdin and stdout are non-blocking for the session, so a client that stops reading
//!   can never wedge a write: writes wait in bounded steps and give up once the
//!   terminal hung up or a shutdown was requested ([`Output`]).
//! - A hung-up terminal (the SSH side closed the PTY) ends the run ([`disconnected`]).
//! - Startup asks the terminal for its colours (OSC 10/11), the kitty keyboard
//!   protocol, synchronized output (DECRQM 2026), when it matters 24-bit colour
//!   (XTGETTCAP `RGB`/`Tc`), and DA1, which every terminal answers and so ends the wait
//!   early ([`Session::enter`]).
//! - Frames are written whole, inside synchronized updates where supported ([`Frames`]).

use std::{
  cell::RefCell,
  io::{self, Write},
  rc::Rc,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, Instant},
};

use nsh_platform_tui::{CursorStyle, RunEvent};
use ratatui_crossterm::crossterm::{cursor, event, execute, style::Print, terminal};
use rustix::{
  event::{PollFd, PollFlags, Timespec},
  fs::OFlags,
};

use crate::convert;

/// How long startup waits for the terminal to answer its queries; DA1 ends it sooner.
/// Over SSH the answers take a network round trip.
const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);
/// OSC 10, OSC 11, the kitty flags query and DECRQM 2026.
const QUERY: &[u8] = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b[?u\x1b[?2026$p";
/// XTGETTCAP for `RGB` and for `Tc` (names in hex), when the answer can raise the colour
/// depth ([`crate::color`]). One request each: xterm stops a request at its first
/// unknown name. A terminal that does not know XTGETTCAP skips the device control
/// strings; one that does answers before DA1, so the wait is no longer.
const QUERY_RGB: &[u8] = b"\x1bP+q524742\x1b\\\x1bP+q5463\x1b\\";
/// DA1, last: every terminal answers it, which ends the wait.
const QUERY_DA1: &[u8] = b"\x1b[c";
/// How long one blocked write waits before it checks the terminal again.
const OUTPUT_WAIT: Duration = Duration::from_millis(16);
const ENABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007h";
const DISABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007l";

/// Escape and modified keys as CSI-u, event kinds, alternate keys (as nsh-tui-host).
const KEYBOARD_FLAGS: event::KeyboardEnhancementFlags =
  event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
    .union(event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    .union(event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS);

/// SIGHUP, SIGTERM and SIGINT, as a flag the run loop polls.
///
/// The handlers stay for the rest of the process: a hung-up PTY can deliver another
/// SIGHUP while the terminal is being restored, which must not kill the process
/// half-way (the process ends right after the run anyway).
pub struct Shutdown {
  flag: Arc<AtomicBool>,
}

impl Shutdown {
  /// Installs the handlers.
  pub fn install() -> io::Result<Self> {
    let flag = Arc::new(AtomicBool::new(false));
    for signal in [
      signal_hook::consts::SIGHUP,
      signal_hook::consts::SIGTERM,
      signal_hook::consts::SIGINT,
    ] {
      signal_hook::flag::register(signal, flag.clone())?;
    }
    Ok(Self { flag })
  }

  /// Whether a signal asked the process to leave.
  pub fn requested(&self) -> bool {
    self.flag.load(Ordering::Relaxed)
  }

  /// The flag, for writers that must stop once a shutdown is under way.
  pub fn flag(&self) -> Arc<AtomicBool> {
    self.flag.clone()
  }

  /// Asks the run to end, as a signal does.
  pub fn request(&self) {
    self.flag.store(true, Ordering::Relaxed);
  }
}

/// The terminal's answers to the startup queries.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Replies {
  pub foreground: Option<[u8; 3]>,
  pub background: Option<[u8; 3]>,
  /// The terminal answered the kitty keyboard flags query.
  pub keyboard: bool,
  /// The terminal reported synchronized output (mode 2026) as known.
  pub synchronized: bool,
  /// The terminal reported 24-bit colour (XTGETTCAP `RGB` or `Tc` found).
  pub truecolor: bool,
  /// DA1 arrived: no further answer will come.
  pub complete: bool,
}

/// What restoration must undo; shared with the panic hook.
#[derive(Default)]
struct Modes {
  active: AtomicBool,
  keyboard: AtomicBool,
}

/// Raw mode, the alternate screen and the input modes, restored on drop.
pub struct Session {
  modes: Arc<Modes>,
  replies: Replies,
  synchronized: bool,
  keyboard: bool,
  flags: Vec<(rustix::fd::BorrowedFd<'static>, OFlags)>,
}

impl Session {
  /// Enters raw mode, queries the terminal (for 24-bit colour too when `ask_rgb`), then
  /// switches to the alternate screen with mouse capture, bracketed paste, focus reports
  /// and (when answered) kitty keys.
  pub fn enter(shutdown: &Shutdown, ask_rgb: bool) -> io::Result<Self> {
    let stdin = rustix::stdio::stdin();
    let stdout = rustix::stdio::stdout();
    if !rustix::termios::isatty(stdin) || !rustix::termios::isatty(stdout) {
      return Err(io::Error::new(
        io::ErrorKind::NotConnected,
        "coffee-tui needs a terminal: run it in one, or connect with `ssh -t`",
      ));
    }
    let mut session = Self {
      modes: Arc::new(Modes::default()),
      replies: Replies::default(),
      synchronized: false,
      keyboard: false,
      flags: Vec::new(),
    };
    // Capture both before changing either: stdin and stdout may share one open file.
    for fd in [stdin, stdout] {
      session.flags.push((fd, rustix::fs::fcntl_getfl(fd)?));
    }
    for &(fd, flags) in &session.flags {
      rustix::fs::fcntl_setfl(fd, flags | OFlags::NONBLOCK)?;
    }
    session.modes.active.store(true, Ordering::Release);
    let previous = std::panic::take_hook();
    let modes = session.modes.clone();
    std::panic::set_hook(Box::new(move |panic| {
      restore(&modes);
      previous(panic);
    }));
    terminal::enable_raw_mode()?;
    // Before crossterm's reader exists, so the answers are ours to read.
    let mut asked = QUERY.to_vec();
    if ask_rgb {
      asked.extend_from_slice(QUERY_RGB);
    }
    asked.extend_from_slice(QUERY_DA1);
    session.replies = query(&asked, QUERY_TIMEOUT, &shutdown.flag())
      .map(|bytes| parse_replies(&bytes))
      .unwrap_or_default();
    let var = |name: &str| std::env::var(name).ok();
    session.synchronized = var("TMUX").is_none_or(|v| v.is_empty()) || session.replies.synchronized;
    let mut output = Output::new(shutdown.flag());
    execute!(
      output,
      terminal::EnterAlternateScreen,
      Print(ENABLE_ALTERNATE_SCROLL),
      cursor::Hide
    )?;
    let _ = execute!(
      output,
      event::EnableBracketedPaste,
      event::EnableFocusChange,
      event::EnableMouseCapture
    );
    if keyboard_protocol(&session.replies, var) {
      execute!(output, event::PushKeyboardEnhancementFlags(KEYBOARD_FLAGS))?;
      session.modes.keyboard.store(true, Ordering::Release);
      session.keyboard = true;
    }
    Ok(session)
  }

  /// What the terminal answered at startup.
  pub fn replies(&self) -> Replies {
    self.replies
  }

  /// Whether frames go out as synchronized updates.
  pub fn synchronized(&self) -> bool {
    self.synchronized
  }

  /// Whether the kitty keyboard protocol is on (Cmd/Super is then reported).
  pub fn keyboard(&self) -> bool {
    self.keyboard
  }

  /// Turns the input modes on again when the terminal comes back to focus (a
  /// multiplexer may have reset them); the kitty flags are set, never stacked.
  pub fn reassert(&self, output: &mut Output) -> io::Result<()> {
    execute!(
      output,
      event::EnableBracketedPaste,
      event::EnableFocusChange,
      event::EnableMouseCapture
    )?;
    if self.keyboard {
      if self.modes.keyboard.swap(true, Ordering::AcqRel) {
        write!(output, "\x1b[={}u", KEYBOARD_FLAGS.bits())?;
      } else {
        execute!(output, event::PushKeyboardEnhancementFlags(KEYBOARD_FLAGS))?;
      }
    }
    output.flush()
  }

  /// The terminal lost the focus: give the kitty flags back until it returns.
  pub fn focus_lost(&self, output: &mut Output) -> io::Result<()> {
    if self.modes.keyboard.swap(false, Ordering::AcqRel) {
      execute!(output, event::PopKeyboardEnhancementFlags)?;
    }
    Ok(())
  }
}

impl Drop for Session {
  fn drop(&mut self) {
    restore(&self.modes);
    for &(fd, flags) in &self.flags {
      let _ = rustix::fs::fcntl_setfl(fd, flags);
    }
  }
}

/// Leaves every mode the session entered: best effort, never waiting on a full or
/// hung-up terminal.
fn restore(modes: &Modes) {
  if !modes.active.swap(false, Ordering::AcqRel) {
    return;
  }
  let mut bytes = Vec::new();
  if modes.keyboard.swap(false, Ordering::AcqRel) {
    let _ = execute!(bytes, event::PopKeyboardEnhancementFlags);
  }
  let _ = execute!(
    bytes,
    terminal::EndSynchronizedUpdate,
    event::DisableMouseCapture,
    event::DisableBracketedPaste,
    event::DisableFocusChange,
    Print(DISABLE_ALTERNATE_SCROLL),
    cursor::SetCursorStyle::DefaultUserShape,
    terminal::LeaveAlternateScreen,
    cursor::Show
  );
  let stdout = rustix::stdio::stdout();
  let mut rest = &bytes[..];
  while !rest.is_empty() {
    match rustix::io::write(stdout, rest) {
      Ok(0) => break,
      Ok(written) => rest = &rest[written..],
      Err(rustix::io::Errno::AGAIN) => {
        // A full PTY nobody reads: drop the output rather than wait on it.
        let _ = rustix::termios::tcflush(stdout, rustix::termios::QueueSelector::OFlush);
        break;
      }
      Err(_) => break,
    }
  }
  let _ = terminal::disable_raw_mode();
}

/// Whether the terminal went away: stdin or stdout hung up or errored.
pub fn disconnected() -> io::Result<bool> {
  let stdin = rustix::stdio::stdin();
  let stdout = rustix::stdio::stdout();
  if !rustix::termios::isatty(stdin) {
    return Ok(true);
  }
  #[cfg(target_os = "macos")]
  if input_at_eof()? {
    return Ok(true);
  }
  let mut fds = [
    PollFd::new(&stdin, PollFlags::empty()),
    PollFd::new(&stdout, PollFlags::empty()),
  ];
  match rustix::event::poll(&mut fds, Some(&Timespec::default())) {
    Ok(_) | Err(rustix::io::Errno::INTR) => {}
    Err(error) => return Err(error.into()),
  }
  Ok(fds.iter().any(|fd| {
    fd.revents()
      .intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL)
  }))
}

/// macOS's poll can leave out HUP for a PTY whose master closed. A readable raw-mode
/// input with nothing queued is at its end.
#[cfg(target_os = "macos")]
fn input_at_eof() -> io::Result<bool> {
  let stdin = rustix::stdio::stdin();
  let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
  match rustix::event::poll(&mut fds, Some(&Timespec::default())) {
    Ok(0) | Err(rustix::io::Errno::INTR) => return Ok(false),
    Ok(_) => {}
    Err(error) => return Err(error.into()),
  }
  if fds[0]
    .revents()
    .intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL)
  {
    return Ok(true);
  }
  Ok(rustix::io::ioctl_fionread(stdin)? == 0)
}

/// Waits up to `timeout` for `fd` to be ready for `events`; what it is ready for.
fn wait_for(
  fd: rustix::fd::BorrowedFd<'_>,
  events: PollFlags,
  timeout: Duration,
) -> io::Result<PollFlags> {
  let mut fds = [PollFd::new(&fd, events)];
  let timeout = Timespec::try_from(timeout).unwrap_or_default();
  match rustix::event::poll(&mut fds, Some(&timeout)) {
    Ok(_) => Ok(fds[0].revents()),
    Err(rustix::io::Errno::INTR) => Ok(PollFlags::empty()),
    Err(error) => Err(error.into()),
  }
}

/// The terminal's output: non-blocking writes that wait in bounded steps and give up
/// when the terminal hung up or a shutdown is under way.
pub struct Output {
  shutdown: Arc<AtomicBool>,
}

impl Output {
  pub fn new(shutdown: Arc<AtomicBool>) -> Self {
    Self { shutdown }
  }
}

impl Write for Output {
  fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    let stdout = rustix::stdio::stdout();
    loop {
      if self.shutdown.load(Ordering::Relaxed) {
        return Err(io::Error::new(
          io::ErrorKind::ConnectionAborted,
          "the terminal session is shutting down",
        ));
      }
      match rustix::io::write(stdout, bytes) {
        Ok(written) => return Ok(written),
        Err(rustix::io::Errno::INTR) => {}
        Err(rustix::io::Errno::AGAIN) => {
          let ready = wait_for(stdout, PollFlags::OUT, OUTPUT_WAIT)?;
          if ready.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
            return Err(io::Error::new(
              io::ErrorKind::BrokenPipe,
              "the terminal hung up",
            ));
          }
          if disconnected()? {
            return Err(io::Error::new(
              io::ErrorKind::BrokenPipe,
              "the terminal hung up",
            ));
          }
        }
        Err(error) => return Err(error.into()),
      }
    }
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

/// Writes `query` and collects the answers until DA1 arrives or `timeout` passes.
fn query(query: &[u8], timeout: Duration, shutdown: &Arc<AtomicBool>) -> io::Result<Vec<u8>> {
  let deadline = Instant::now() + timeout;
  Output::new(shutdown.clone()).write_all(query)?;
  let stdin = rustix::stdio::stdin();
  let mut replies = Vec::new();
  let mut chunk = [0u8; 512];
  loop {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() || parse_replies(&replies).complete || shutdown.load(Ordering::Relaxed) {
      return Ok(replies);
    }
    let ready = wait_for(stdin, PollFlags::IN, left.min(Duration::from_millis(50)))?;
    if ready.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
      return Ok(replies);
    }
    if !ready.contains(PollFlags::IN) {
      continue;
    }
    match rustix::io::read(stdin, &mut chunk) {
      Ok(0) => return Ok(replies),
      Ok(read) => replies.extend_from_slice(&chunk[..read]),
      Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
      Err(error) => return Err(error.into()),
    }
  }
}

/// Reads OSC 10/11 colours, the kitty flags answer, the mode 2026 report, XTGETTCAP
/// answers and DA1.
pub fn parse_replies(bytes: &[u8]) -> Replies {
  let mut replies = Replies::default();
  let mut i = 0;
  while i + 1 < bytes.len() {
    if bytes[i] != 0x1b {
      i += 1;
      continue;
    }
    match bytes[i + 1] {
      // DCS ... ST: `1+r<name>[=<value>]` is XTGETTCAP's "known" answer.
      b'P' => {
        let body = &bytes[i + 2..];
        // Only a whole answer counts.
        let Some(end) = body.windows(2).position(|w| w == b"\x1b\\") else {
          break;
        };
        if let Some(found) = body[..end].strip_prefix(b"1+r") {
          replies.truecolor |= found.split(|&b| b == b';').any(|capability| {
            let name = capability.split(|&b| b == b'=').next().unwrap_or_default();
            name.eq_ignore_ascii_case(b"524742") || name.eq_ignore_ascii_case(b"5463")
          });
        }
        i += 2 + end;
      }
      b']' => {
        let body = &bytes[i + 2..];
        let end = body
          .iter()
          .position(|&b| b == 0x07 || b == 0x1b)
          .unwrap_or(body.len());
        let text = String::from_utf8_lossy(&body[..end]);
        if let Some((code, value)) = text.split_once(';') {
          let color = parse_color(value);
          match code {
            "10" => replies.foreground = color.or(replies.foreground),
            "11" => replies.background = color.or(replies.background),
            _ => {}
          }
        }
        i += 2 + end;
      }
      b'[' => {
        let body = &bytes[i + 2..];
        let Some(end) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else {
          break;
        };
        if body.first() == Some(&b'?') {
          match body[end] {
            b'u' => replies.keyboard = true,
            b'c' => replies.complete = true,
            b'y' => {
              if let Some(value) = body[1..end].strip_prefix(b"2026;") {
                replies.synchronized |= matches!(value, b"1$" | b"2$" | b"3$");
              }
            }
            _ => {}
          }
        }
        i += 3 + end;
      }
      _ => i += 1,
    }
  }
  replies
}

/// An X11 colour answer (`rgb:RRRR/GGGG/BBBB`, one to four hex digits per channel).
fn parse_color(value: &str) -> Option<[u8; 3]> {
  let channels = value
    .strip_prefix("rgb:")
    .or_else(|| value.strip_prefix("rgba:"))?;
  let mut out = [0; 3];
  let mut parts = channels.split('/');
  for slot in &mut out {
    let part = parts.next()?;
    if part.is_empty() || part.len() > 4 {
      return None;
    }
    let value = u32::from_str_radix(part, 16).ok()?;
    let max = (1u32 << (4 * part.len())) - 1;
    *slot = u8::try_from((value * 255 + max / 2) / max).ok()?;
  }
  Some(out)
}

/// Whether to push the kitty flags: answered, and not a terminal known to overstate
/// them (Apple's Terminal, VS Code, VTE) or a tmux before 3.3.
fn keyboard_protocol(replies: &Replies, var: impl Fn(&str) -> Option<String>) -> bool {
  if !replies.keyboard || var("VTE_VERSION").is_some_and(|v| !v.is_empty()) {
    return false;
  }
  match var("TERM_PROGRAM").as_deref() {
    Some("Apple_Terminal" | "vscode") => false,
    Some("tmux") => var("TERM_PROGRAM_VERSION").is_some_and(|version| {
      let mut parts = version.split(|c: char| !c.is_ascii_digit());
      let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
      let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
      (major, minor) >= (3, 3)
    }),
    _ => true,
  }
}

/// Collects what ratatui writes for one frame and writes it whole between frames, as
/// one synchronized update, and not at all when it only repeats the cursor.
pub struct Frames {
  output: Output,
  pending: Rc<RefCell<Vec<u8>>>,
  last: Vec<u8>,
  synchronized: bool,
}

/// The buffer ratatui's backend writes a frame into.
pub struct FrameWriter(Rc<RefCell<Vec<u8>>>);

impl Write for FrameWriter {
  fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    self.0.borrow_mut().extend_from_slice(bytes);
    Ok(bytes.len())
  }

  // A frame ends at `Frames::commit`, not at ratatui's flushes within it.
  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl Frames {
  pub fn new(output: Output, synchronized: bool) -> (Self, FrameWriter) {
    let pending = Rc::new(RefCell::new(Vec::new()));
    let frames = Self {
      output,
      pending: pending.clone(),
      last: Vec::new(),
      synchronized,
    };
    (frames, FrameWriter(pending))
  }

  /// Writes the frame drawn since the last commit, if it changes anything.
  pub fn commit(&mut self) -> io::Result<()> {
    let frame = std::mem::take(&mut *self.pending.borrow_mut());
    if frame.is_empty() || (cursor_only(&frame) && self.last.ends_with(&frame)) {
      return Ok(());
    }
    if self.synchronized {
      execute!(self.output, terminal::BeginSynchronizedUpdate)?;
    }
    self.output.write_all(&frame)?;
    if self.synchronized {
      execute!(self.output, terminal::EndSynchronizedUpdate)?;
    }
    self.last = frame;
    Ok(())
  }

  /// Writes bytes between frames (a clipboard escape, a cursor shape).
  pub fn output(&mut self) -> &mut Output {
    &mut self.output
  }
}

/// Whether `bytes` are only ratatui's end-of-frame cursor commands.
fn cursor_only(bytes: &[u8]) -> bool {
  let mut rest = bytes;
  while !rest.is_empty() {
    let Some(body) = rest.strip_prefix(b"\x1b[") else {
      return false;
    };
    let Some(end) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else {
      return false;
    };
    let (params, last) = (&body[..end], body[end]);
    let cursor = match last {
      b'h' | b'l' => params == b"?25",
      b'H' => params.iter().all(|&b| b.is_ascii_digit() || b == b';'),
      _ => false,
    };
    if !cursor {
      return false;
    }
    rest = &body[end + 1..];
  }
  true
}

/// Writes the cursor shape a frame asks for: a steady bar while a field is edited.
pub fn set_cursor_style(output: &mut Output, style: CursorStyle) -> io::Result<()> {
  match style {
    CursorStyle::Bar => execute!(output, cursor::SetCursorStyle::SteadyBar),
    _ => execute!(output, cursor::SetCursorStyle::DefaultUserShape),
  }
}

/// Wakes the run loop from its wait: the engine has work (nsh's wake callback) or the
/// terminal was resized (SIGWINCH). A self-pipe the loop polls next to stdin.
pub struct Waker {
  read: std::os::unix::net::UnixStream,
  write: Arc<std::os::unix::net::UnixStream>,
}

impl Waker {
  pub fn new() -> io::Result<Self> {
    let (read, write) = std::os::unix::net::UnixStream::pair()?;
    read.set_nonblocking(true)?;
    write.set_nonblocking(true)?;
    signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, write.try_clone()?)?;
    Ok(Self {
      read,
      write: Arc::new(write),
    })
  }

  /// The callback nsh calls when the surface has pending work.
  pub fn callback(&self) -> nsh_platform_tui::WakeCallback {
    let write = self.write.clone();
    Arc::new(move || {
      // A full pipe already holds a wake-up.
      let _ = (&*write).write(&[1]);
    })
  }

  /// Empties the pipe; `true` when it held a wake-up.
  fn drain(&self) -> bool {
    let mut woke = false;
    let mut chunk = [0u8; 64];
    while let Ok(read) = std::io::Read::read(&mut &self.read, &mut chunk) {
      if read == 0 {
        break;
      }
      woke = true;
    }
    woke
  }
}

/// Waits up to `timeout` for input, a wake-up or a resize: a shutdown or a hung-up
/// terminal is `Exit`.
///
/// nsh's `run_app` polls crossterm every 16 ms because crossterm cannot be woken from
/// the engine thread; that costs about 0.5 % CPU per idle session. Here the wait is a
/// `poll` on stdin and the [`Waker`]'s pipe instead, so an idle session sleeps until
/// something happens (or `timeout`, which the run loop keeps at most 250 ms).
pub fn poll(timeout: Duration, shutdown: &Shutdown, waker: &Waker) -> io::Result<RunEvent> {
  // crossterm's poll-based reader skips buffered events at a zero timeout: give it a
  // small budget (as nsh-tui-host does).
  const DECODE_POLL: Duration = Duration::from_micros(50);
  if shutdown.requested() || disconnected()? {
    return Ok(RunEvent::Exit);
  }
  // Events crossterm already parsed (one read can carry several) come first.
  if event::poll(DECODE_POLL)? {
    return Ok(convert(event::read()?));
  }
  let stdin = rustix::stdio::stdin();
  let mut fds = [
    PollFd::new(&stdin, PollFlags::IN),
    PollFd::new(&waker.read, PollFlags::IN),
  ];
  let wait = Timespec::try_from(timeout).unwrap_or_default();
  match rustix::event::poll(&mut fds, Some(&wait)) {
    Ok(_) | Err(rustix::io::Errno::INTR) => {}
    Err(error) => return Err(error.into()),
  }
  let input = !fds[0].revents().is_empty();
  if shutdown.requested() || disconnected()? {
    return Ok(RunEvent::Exit);
  }
  let woke = waker.drain();
  if input && event::poll(DECODE_POLL)? {
    return Ok(convert(event::read()?));
  }
  // A resize or engine work: draw (a resize is picked up by the draw's autoresize).
  Ok(if woke {
    RunEvent::Redraw
  } else {
    RunEvent::Idle
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn replies_are_parsed() {
    let replies = parse_replies(
      b"\x1b]10;rgb:ecec/e4e4/d6d6\x1b\\\x1b]11;rgb:14/11/0e\x07\x1b[?1u\x1b[?2026;2$y\x1b[?62;22c",
    );
    assert_eq!(replies.foreground, Some([0xec, 0xe4, 0xd6]));
    assert_eq!(replies.background, Some([0x14, 0x11, 0x0e]));
    assert!(replies.keyboard && replies.synchronized && replies.complete);
    let none = parse_replies(b"\x1b[?1;2c");
    assert!(none.complete && !none.keyboard && none.background.is_none());
    assert!(!none.truecolor);
  }

  #[test]
  fn xtgettcap_answers_are_read() {
    // kitty-style: one answer per name, `Tc` known.
    let tc = parse_replies(b"\x1bP0+r524742\x1b\\\x1bP1+r5463\x1b\\\x1b[?62;22c");
    assert!(tc.truecolor && tc.complete);
    // A value, lower-case hex, several names in one answer.
    let rgb = parse_replies(b"\x1bP1+r4e6f=31;524742=382f382f38\x1b\\\x1b[?1c");
    assert!(rgb.truecolor);
    // "Unknown" answers, another capability, or none at all: no.
    for bytes in [
      &b"\x1bP0+r524742\x1b\\\x1bP0+r5463\x1b\\\x1b[?1c"[..],
      b"\x1bP1+r636f6c6f7273=323536\x1b\\\x1b[?1c",
      b"\x1b[?62;22c",
      // A cut-off answer (no ST) does not count, nor does it hide DA1 before it.
      b"\x1b[?62c\x1bP1+r524742",
    ] {
      let replies = parse_replies(bytes);
      assert!(!replies.truecolor, "{bytes:?}");
      assert!(replies.complete, "{bytes:?}");
    }
    // Its hex never reads as another answer (no stray DA1 or kitty flags).
    let inside = parse_replies(b"\x1bP1+r5463=5b3f75\x1b\\");
    assert!(inside.truecolor && !inside.complete && !inside.keyboard);
  }

  #[test]
  fn only_cursor_commands_are_recognised() {
    assert!(cursor_only(b"\x1b[?25l\x1b[3;4H"));
    assert!(!cursor_only(b"\x1b[3;4Hx"));
  }

  #[test]
  fn kitty_keys_are_skipped_where_terminals_overstate_them() {
    let replies = Replies {
      keyboard: true,
      ..Replies::default()
    };
    assert!(keyboard_protocol(&replies, |_| None));
    assert!(!keyboard_protocol(&replies, |name| {
      (name == "TERM_PROGRAM").then(|| "Apple_Terminal".to_owned())
    }));
    assert!(!keyboard_protocol(&Replies::default(), |_| None));
  }

  #[test]
  fn shutdown_requests_are_seen() {
    let shutdown = Shutdown::install().unwrap();
    assert!(!shutdown.requested());
    shutdown.request();
    assert!(shutdown.requested());
  }
}
