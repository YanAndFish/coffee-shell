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
//!   early ([`Session::enter`]). Keys typed meanwhile share the input with the answers:
//!   they are kept apart from them and delivered first, in order, by [`poll`]
//!   ([`split_replies`], [`decode_keys`]). No input reaches the app before it can take it
//!   ([`Startup`]).
//! - Frames are written whole, inside synchronized updates where supported ([`Frames`]).

use std::{
  cell::RefCell,
  collections::VecDeque,
  io::{self, Write},
  rc::Rc,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, Instant},
};

use nsh_platform_tui::{CursorStyle, RunEvent};
use ratatui_crossterm::crossterm::{
  cursor,
  event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
  execute,
  style::Print,
  terminal,
};
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
    // Before crossterm's reader exists, so the answers are ours to read. Whatever else
    // arrives with them was typed: it goes to the app first, in order.
    let mut asked = QUERY.to_vec();
    if ask_rgb {
      asked.extend_from_slice(QUERY_RGB);
    }
    asked.extend_from_slice(QUERY_DA1);
    let (replies, typed) = query(&asked, QUERY_TIMEOUT, &shutdown.flag())
      .map(|bytes| split_replies(&bytes))
      .unwrap_or_default();
    session.replies = replies;
    queue_typed(decode_keys(&typed));
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

/// Writes `query` and collects the input until DA1 arrives or `timeout` passes.
fn query(query: &[u8], timeout: Duration, shutdown: &Arc<AtomicBool>) -> io::Result<Vec<u8>> {
  let deadline = Instant::now() + timeout;
  Output::new(shutdown.clone()).write_all(query)?;
  collect(rustix::stdio::stdin(), deadline, shutdown)
}

/// Reads `input` until the answers are complete (DA1), `deadline` passes or a shutdown is
/// asked for: every byte read, the answers and whatever was typed meanwhile. Nothing after
/// the read that brought DA1 is taken.
fn collect(
  input: rustix::fd::BorrowedFd<'_>,
  deadline: Instant,
  shutdown: &AtomicBool,
) -> io::Result<Vec<u8>> {
  let mut bytes = Vec::new();
  let mut chunk = [0u8; 512];
  loop {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() || parse_replies(&bytes).complete || shutdown.load(Ordering::Relaxed) {
      return Ok(bytes);
    }
    let ready = wait_for(input, PollFlags::IN, left.min(Duration::from_millis(50)))?;
    if ready.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
      return Ok(bytes);
    }
    if !ready.contains(PollFlags::IN) {
      continue;
    }
    match rustix::io::read(input, &mut chunk) {
      Ok(0) => return Ok(bytes),
      Ok(read) => bytes.extend_from_slice(&chunk[..read]),
      Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
      Err(error) => return Err(error.into()),
    }
  }
}

/// The answers among `bytes` (as [`parse_replies`] reads them) and everything else, in
/// order: what was typed while startup waited. An answer cut off at the end is dropped.
pub fn split_replies(bytes: &[u8]) -> (Replies, Vec<u8>) {
  let mut typed = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    match reply_len(&bytes[i..]) {
      Some(len) => i += len,
      None => {
        typed.push(bytes[i]);
        i += 1;
      }
    }
  }
  (parse_replies(bytes), typed)
}

/// The length of the answer `bytes` starts with: an OSC 10/11 colour, a DCS string
/// (XTGETTCAP), `CSI ? … u` (kitty flags), `CSI ? … c` (DA1) or `CSI ? … $ y` (DECRQM);
/// the rest of `bytes` when such an answer is cut off.
fn reply_len(bytes: &[u8]) -> Option<usize> {
  let body = bytes.strip_prefix(b"\x1b")?;
  match body.first()? {
    b']' => {
      let text = &body[1..];
      if !(text.starts_with(b"10;") || text.starts_with(b"11;")) {
        return None;
      }
      Some(match text.iter().position(|&b| b == 0x07 || b == 0x1b) {
        Some(end) if text[end] == 0x07 => 3 + end,
        Some(end) if text.get(end + 1) == Some(&b'\\') => 4 + end,
        // Ended by another escape: that one is not part of it.
        Some(end) if end + 1 < text.len() => 2 + end,
        _ => bytes.len(),
      })
    }
    b'P' => Some(
      body[1..]
        .windows(2)
        .position(|w| w == b"\x1b\\")
        .map_or(bytes.len(), |end| 4 + end),
    ),
    b'[' if body.get(1) == Some(&b'?') => {
      let params = &body[2..];
      match params.iter().position(|b| (0x40..=0x7e).contains(b)) {
        Some(end) if matches!(params[end], b'u' | b'c' | b'y') => Some(4 + end),
        Some(_) => None,
        None => params
          .iter()
          .all(|&b| b.is_ascii_digit() || b == b';' || b == b'$')
          .then_some(bytes.len()),
      }
    }
    _ => None,
  }
}

/// Keys typed during startup, in order, until [`poll`] delivers them.
static TYPED: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

fn queue_typed(events: Vec<Event>) {
  if let Ok(mut typed) = TYPED.lock() {
    typed.extend(events);
  }
}

/// The next key typed during startup, once.
fn take_typed() -> Option<Event> {
  TYPED.lock().ok()?.pop_front()
}

/// The keys in `bytes` as crossterm reads a terminal's legacy encoding (no kitty flags
/// yet at startup): characters, Enter, Tab, Backspace, control keys, Esc and Alt+key,
/// arrows, Home/End, the `~` keys, F1–F12 and Shift+Tab. A `\n` is Enter: typed before
/// raw mode, the terminal turned Enter into one. Unknown sequences are skipped.
pub fn decode_keys(bytes: &[u8]) -> Vec<Event> {
  let mut events = Vec::new();
  let mut rest = bytes;
  while !rest.is_empty() {
    let (key, used) = decode_key(rest);
    events.extend(key.map(Event::Key));
    rest = &rest[used.max(1)..];
  }
  events
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<KeyEvent> {
  Some(KeyEvent::new(code, modifiers))
}

/// The key `bytes` starts with and how many bytes it takes.
fn decode_key(bytes: &[u8]) -> (Option<KeyEvent>, usize) {
  let none = KeyModifiers::NONE;
  match bytes[0] {
    0x1b => decode_escape(bytes),
    b'\r' | b'\n' => (key(KeyCode::Enter, none), 1),
    b'\t' => (key(KeyCode::Tab, none), 1),
    0x7f => (key(KeyCode::Backspace, none), 1),
    0 => (key(KeyCode::Char(' '), KeyModifiers::CONTROL), 1),
    c @ 0x01..=0x1a => (
      key(
        KeyCode::Char(char::from(c - 1 + b'a')),
        KeyModifiers::CONTROL,
      ),
      1,
    ),
    c @ 0x1c..=0x1f => (
      key(
        KeyCode::Char(char::from(c - 0x1c + b'4')),
        KeyModifiers::CONTROL,
      ),
      1,
    ),
    first => {
      let len = match first {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
      };
      let Some(c) = bytes
        .get(..len)
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.chars().next())
      else {
        return (None, 1);
      };
      let modifiers = if c.is_uppercase() {
        KeyModifiers::SHIFT
      } else {
        none
      };
      (key(KeyCode::Char(c), modifiers), len)
    }
  }
}

/// Esc, Alt+key, SS3 (`ESC O x`) and CSI keys.
fn decode_escape(bytes: &[u8]) -> (Option<KeyEvent>, usize) {
  let none = KeyModifiers::NONE;
  match bytes.get(1) {
    None | Some(0x1b) => (key(KeyCode::Esc, none), 1),
    Some(b'O') if bytes.len() > 2 => {
      let code = match bytes[2] {
        b'A' => Some(KeyCode::Up),
        b'B' => Some(KeyCode::Down),
        b'C' => Some(KeyCode::Right),
        b'D' => Some(KeyCode::Left),
        b'H' => Some(KeyCode::Home),
        b'F' => Some(KeyCode::End),
        f @ b'P'..=b'S' => Some(KeyCode::F(1 + f - b'P')),
        _ => None,
      };
      (code.and_then(|code| key(code, none)), 3)
    }
    Some(b'[') => {
      let params = &bytes[2..];
      let Some(end) = params.iter().position(|b| (0x40..=0x7e).contains(b)) else {
        return (None, bytes.len());
      };
      let text = std::str::from_utf8(&params[..end]).unwrap_or_default();
      let mut numbers = text.split(';').map(|n| n.parse::<u8>().unwrap_or(1));
      let first = numbers.next().unwrap_or(1);
      let mask = numbers.next().unwrap_or(1).saturating_sub(1);
      let mut modifiers = none;
      for (bit, modifier) in [
        (1, KeyModifiers::SHIFT),
        (2, KeyModifiers::ALT),
        (4, KeyModifiers::CONTROL),
        (8, KeyModifiers::SUPER),
      ] {
        if mask & bit != 0 {
          modifiers |= modifier;
        }
      }
      let code = match params[end] {
        b'A' => Some(KeyCode::Up),
        b'B' => Some(KeyCode::Down),
        b'C' => Some(KeyCode::Right),
        b'D' => Some(KeyCode::Left),
        b'H' => Some(KeyCode::Home),
        b'F' => Some(KeyCode::End),
        f @ b'P'..=b'S' => Some(KeyCode::F(1 + f - b'P')),
        b'Z' => {
          modifiers |= KeyModifiers::SHIFT;
          Some(KeyCode::BackTab)
        }
        b'~' => match first {
          1 | 7 => Some(KeyCode::Home),
          2 => Some(KeyCode::Insert),
          3 => Some(KeyCode::Delete),
          4 | 8 => Some(KeyCode::End),
          5 => Some(KeyCode::PageUp),
          6 => Some(KeyCode::PageDown),
          n @ 11..=15 => Some(KeyCode::F(n - 10)),
          n @ 17..=21 => Some(KeyCode::F(n - 11)),
          n @ 23..=24 => Some(KeyCode::F(n - 12)),
          _ => None,
        },
        _ => None,
      };
      (code.and_then(|code| key(code, modifiers)), 3 + end)
    }
    Some(_) => {
      let (inner, used) = decode_key(&bytes[1..]);
      let alt = inner.map(|mut event| {
        event.modifiers |= KeyModifiers::ALT;
        event
      });
      (alt, 1 + used)
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
///
/// It also tells when the app can take input ([`Startup`]).
pub struct Waker {
  read: std::os::unix::net::UnixStream,
  write: Arc<std::os::unix::net::UnixStream>,
  startup: std::cell::Cell<Startup>,
}

/// How long the engine must stay quiet after its startup work before input reaches the
/// app.
const APP_QUIET: Duration = Duration::from_millis(100);
/// The longest input waits for the app (an engine that never settles, e.g. an animation
/// from the first frame).
const APP_READY_WAIT: Duration = Duration::from_secs(2);

/// Whether the app can take input yet.
///
/// nsh runs the app on an engine thread and applies its batches (the tree, then the key
/// bindings the shell registers) only when the host draws; a key handled before the
/// bindings arrived matches nothing and is dropped (`AppView::handle` answers `Ignored`).
/// The first batches arrive over the first tens of milliseconds, in several wake-ups, so
/// input stays queued (in the terminal, or from the startup queries) until the engine
/// woke at least once and then stayed quiet for [`APP_QUIET`], or [`APP_READY_WAIT`]
/// passed since the run began. Then it is read in order, and from then on at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Startup {
  started: Instant,
  last_wake: Option<Instant>,
  ready: bool,
}

impl Startup {
  pub fn new(started: Instant) -> Self {
    Self {
      started,
      last_wake: None,
      ready: false,
    }
  }

  /// The engine woke the loop at `now`.
  pub fn woke(&mut self, now: Instant) {
    self.last_wake = Some(now);
  }

  /// Whether input may reach the app at `now` (once true, it stays true).
  pub fn ready(&mut self, now: Instant) -> bool {
    self.ready = self.ready
      || now.saturating_duration_since(self.started) >= APP_READY_WAIT
      || self
        .last_wake
        .is_some_and(|woke| now.saturating_duration_since(woke) >= APP_QUIET);
    self.ready
  }

  /// How long to wait at most at `now` before [`Self::ready`] may change.
  fn wait(&self, now: Instant) -> Duration {
    let limit = APP_READY_WAIT.saturating_sub(now.saturating_duration_since(self.started));
    self.last_wake.map_or(limit, |woke| {
      limit.min(APP_QUIET.saturating_sub(now.saturating_duration_since(woke)))
    })
  }
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
      startup: std::cell::Cell::new(Startup::new(Instant::now())),
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

  /// [`Startup::ready`] now.
  fn input_ready(&self) -> bool {
    let mut startup = self.startup.get();
    let ready = startup.ready(Instant::now());
    self.startup.set(startup);
    ready
  }
}

/// Before the app can take input: waits up to `timeout` for the engine only, leaving
/// input queued; a wake-up (the engine's work, to draw) or the app becoming ready is a
/// redraw, so the loop comes straight back.
fn wait_for_app(timeout: Duration, shutdown: &Shutdown, waker: &Waker) -> io::Result<RunEvent> {
  let mut fds = [PollFd::new(&waker.read, PollFlags::IN)];
  let wait = timeout.min(waker.startup.get().wait(Instant::now()));
  let wait = Timespec::try_from(wait).unwrap_or_default();
  match rustix::event::poll(&mut fds, Some(&wait)) {
    Ok(_) | Err(rustix::io::Errno::INTR) => {}
    Err(error) => return Err(error.into()),
  }
  if shutdown.requested() || disconnected()? {
    return Ok(RunEvent::Exit);
  }
  if waker.drain() {
    let mut startup = waker.startup.get();
    startup.woke(Instant::now());
    waker.startup.set(startup);
    return Ok(RunEvent::Redraw);
  }
  Ok(if waker.input_ready() {
    RunEvent::Redraw
  } else {
    RunEvent::Idle
  })
}

/// Waits up to `timeout` for input, a wake-up or a resize: a shutdown or a hung-up
/// terminal is `Exit`. Input waits until the app can take it ([`Startup`]).
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
  if !waker.input_ready() {
    return wait_for_app(timeout, shutdown, waker);
  }
  // Keys typed while startup waited for the terminal were read then: they come first.
  if let Some(event) = take_typed() {
    return Ok(convert(event));
  }
  // Events crossterm already parsed (one read can carry several) come next.
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

  fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
  }

  fn chars(text: &str) -> Vec<Event> {
    text
      .chars()
      .map(|c| press(KeyCode::Char(c), KeyModifiers::NONE))
      .collect()
  }

  /// Every answer of the startup queries, with keys typed before, between and after them.
  const INTERLEAVED: &[u8] = b"g\x1b]10;rgb:ecec/e4e4/d6d6\x1b\\h\x1b]11;rgb:14/11/0e\x07\x1b[A\
    \x1b[?1u\x1bP1+r524742\x1b\\\r\x1b[?2026;2$y\x1bP0+r5463\x1b\\\x1b[?62;22cij";

  #[test]
  fn keys_typed_during_the_queries_are_kept_apart_from_the_answers() {
    let (replies, typed) = split_replies(INTERLEAVED);
    assert_eq!(replies, parse_replies(INTERLEAVED));
    assert_eq!(replies.foreground, Some([0xec, 0xe4, 0xd6]));
    assert!(replies.keyboard && replies.synchronized && replies.truecolor && replies.complete);
    assert_eq!(typed, b"gh\x1b[A\rij");
    let mut expected = chars("gh");
    expected.push(press(KeyCode::Up, KeyModifiers::NONE));
    expected.push(press(KeyCode::Enter, KeyModifiers::NONE));
    expected.extend(chars("ij"));
    assert_eq!(decode_keys(&typed), expected);
    // No answers: everything was typed (a user's Esc and arrows included).
    let keys = b"\x1b\x1b[1;5C\x1bOP\x1b[Z";
    assert_eq!(split_replies(keys).1, keys);
    // An answer cut off at the end (the wait timed out) is not taken for keys.
    for cut in [&b"q\x1b[?62;2"[..], b"q\x1bP1+r52", b"q\x1b]11;rgb:14/11"] {
      assert_eq!(split_replies(cut).1, b"q", "{cut:?}");
    }
    // An unrelated OSC or CSI is not an answer.
    assert_eq!(split_replies(b"\x1b[5~").1, b"\x1b[5~");
  }

  #[test]
  fn typed_bytes_decode_as_crossterm_reads_them() {
    let ctrl = KeyModifiers::CONTROL;
    let none = KeyModifiers::NONE;
    let events =
      decode_keys("a中G\t\x7f\x03\n\x1b\x1bx\x1b[1;5C\x1bOP\x1b[15~\x1b[Z\x1b[3~".as_bytes());
    assert_eq!(
      events,
      vec![
        press(KeyCode::Char('a'), none),
        press(KeyCode::Char('中'), none),
        press(KeyCode::Char('G'), KeyModifiers::SHIFT),
        press(KeyCode::Tab, none),
        press(KeyCode::Backspace, none),
        press(KeyCode::Char('c'), ctrl),
        press(KeyCode::Enter, none),
        press(KeyCode::Esc, none),
        press(KeyCode::Char('x'), KeyModifiers::ALT),
        press(KeyCode::Right, ctrl),
        press(KeyCode::F(1), none),
        press(KeyCode::F(5), none),
        press(KeyCode::BackTab, KeyModifiers::SHIFT),
        press(KeyCode::Delete, none),
      ]
    );
    assert_eq!(decode_keys(b"\x1b"), vec![press(KeyCode::Esc, none)]);
    // Unknown sequences and broken UTF-8 are skipped, not misread.
    assert_eq!(decode_keys(b"\x1b[99~\xffz"), chars("z"));
  }

  /// The regression of "the first keys after connecting are lost": keys arriving before,
  /// between and right after the answers are all read once, in order; a key arriving after
  /// the wait ended stays for the normal reader.
  #[test]
  fn startup_keeps_keys_typed_during_and_right_after_the_queries() {
    use std::{io::Read, os::fd::AsFd, os::unix::net::UnixStream};

    let (app, mut terminal) = UnixStream::pair().unwrap();
    let flag = AtomicBool::new(false);
    // Typed before the queries even went out.
    terminal.write_all(b"g").unwrap();
    let mut writer = terminal.try_clone().unwrap();
    let answers = std::thread::spawn(move || {
      std::thread::sleep(Duration::from_millis(30));
      writer.write_all(b"\x1b]11;rgb:14/11/0e\x1b\\h").unwrap();
      std::thread::sleep(Duration::from_millis(30));
      // DA1 and two keys typed right after it, in one read.
      writer.write_all(b"\x1b[?62;22cij").unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let bytes = collect(app.as_fd(), deadline, &flag).unwrap();
    answers.join().unwrap();
    terminal.write_all(b"k").unwrap();
    let (replies, typed) = split_replies(&bytes);
    assert!(replies.complete);
    assert_eq!(replies.background, Some([0x14, 0x11, 0x0e]));
    assert_eq!(typed, b"ghij");
    let mut later = [0u8; 8];
    let read = (&app).read(&mut later).unwrap();
    assert_eq!(&later[..read], b"k");

    // A terminal that never answers: the wait ends at its deadline, the keys stay.
    let (app, mut terminal) = UnixStream::pair().unwrap();
    terminal.write_all(b"xy\r").unwrap();
    let bytes = collect(
      app.as_fd(),
      Instant::now() + Duration::from_millis(100),
      &flag,
    )
    .unwrap();
    let (replies, typed) = split_replies(&bytes);
    assert!(!replies.complete);
    assert_eq!(decode_keys(&typed).len(), 3);

    // What startup kept reaches the run loop once, in order, before anything read later.
    queue_typed(decode_keys(b"gh"));
    assert_eq!(
      take_typed(),
      Some(press(KeyCode::Char('g'), KeyModifiers::NONE))
    );
    assert_eq!(
      take_typed(),
      Some(press(KeyCode::Char('h'), KeyModifiers::NONE))
    );
    assert_eq!(take_typed(), None);
  }

  /// Input waits for the app: not before the engine's first wake-up, not while it keeps
  /// waking, and never longer than `APP_READY_WAIT`.
  #[test]
  fn input_waits_until_the_app_settled() {
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    let mut startup = Startup::new(start);
    assert!(!startup.ready(at(0)));
    assert!(
      !startup.ready(at(500)),
      "no wake-up yet: the app has no bindings"
    );
    assert_eq!(
      startup.wait(at(500)),
      APP_READY_WAIT - Duration::from_millis(500)
    );
    startup.woke(at(17));
    startup.woke(at(51));
    assert!(!startup.ready(at(60)), "still busy");
    assert_eq!(startup.wait(at(60)), APP_QUIET - Duration::from_millis(9));
    assert!(startup.ready(at(51) + APP_QUIET));
    // Once ready, always ready (a later wake-up does not hold input back).
    startup.woke(at(200));
    assert!(startup.ready(at(201)));
    // An engine that never settles: input goes through at the limit.
    let mut busy = Startup::new(start);
    for ms in (0..2000).step_by(20) {
      busy.woke(at(ms));
      assert!(!busy.ready(at(ms + 1)), "{ms}");
    }
    assert!(busy.ready(start + APP_READY_WAIT));
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
