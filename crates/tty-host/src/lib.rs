//! `tty-host`: runs an nsh 0.7.1 Rust-mode root in the terminal this process was started
//! in.
//!
//! One process serves one terminal: locally any terminal, behind an SSH gateway the PTY of
//! one SSH session. The host knows nothing of the app it runs; the caller builds the root
//! and passes it with its UI sets in [`HostOptions`]. The host:
//!
//! - enters raw mode and the alternate screen, asks the terminal what it supports
//!   (kitty keyboard, synchronized output, its colours) and restores everything on the
//!   way out ([`terminal`]);
//! - leaves cleanly on SIGHUP, SIGTERM and SIGINT, and when the terminal hangs up (the
//!   SSH side closed the PTY), so no session outlives its client;
//! - redraws on every resize (crossterm's SIGWINCH);
//! - picks the colour depth from the caller's forced mode, then `NO_COLOR`, `COLORTERM`,
//!   the terminal's brand in `TERM`, and for a `256color` terminal its answer to XTGETTCAP
//!   `RGB` ([`color`]);
//! - hands every rendered frame to the caller's [`FrameFilter`] before the diff (e.g. to
//!   pin exact colours over nsh's ANSI meanings);
//! - turns Ctrl+C into press-twice-to-quit ([`interrupt`]);
//! - copies through OSC 52 ([`clipboard`]).

use std::{
  cell::{Cell, RefCell},
  env,
  io::{self, Write as _},
  rc::Rc,
  sync::Arc,
  time::{Duration, Instant},
};

pub use nsh_core::element::Registry;
pub use nsh_platform_tui::{self, AppError, Appearance, ColorDepth, Root, TerminalCapabilities};
use nsh_platform_tui::{
  AppOptions, AppView, CursorStyle, Input, Modifiers, MouseButton, MouseInput, MouseKind, RunEvent,
};
pub use ratatui_core::{self, buffer::Buffer};
use ratatui_core::{backend::Backend, style::Color, terminal::Terminal};
use ratatui_crossterm::{
  CrosstermBackend,
  crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};

pub mod clipboard;
pub mod color;
pub mod interrupt;
pub mod terminal;

use interrupt::{Interrupt, Press};

/// Rewrites each rendered frame in place, after nsh and the host drew it and before
/// ratatui diffs it against the previous one.
pub trait FrameFilter {
  /// Changes `buffer`, the whole frame.
  fn apply(&mut self, buffer: &mut Buffer);

  /// Whether the filter decides the colours: nsh then gets no terminal colours (OSC 10/11
  /// answers) and draws its "bandless" look, which the filter maps. When `false`, the
  /// terminal's answers are passed on, as nsh's development host does.
  fn owns_colors(&self) -> bool {
    true
  }
}

/// Makes the [`FrameFilter`] once the terminal answered: the colour depth is known only
/// then.
pub type FrameFilterFactory = Box<dyn FnOnce(ColorDepth) -> Box<dyn FrameFilter>>;

/// What [`run`] runs and how.
#[non_exhaustive]
pub struct HostOptions {
  /// This program's name, used in messages ("`<program>` needs a terminal …").
  pub program: String,
  /// The nsh root, built by the caller before the terminal is entered.
  pub root: Root,
  /// Every UI set the root uses (e.g. `nsh_base::registry()` and the app's own); by
  /// default nsh's base set alone.
  pub ui_sets: Vec<Registry>,
  /// A forced colour depth: `truecolor`/`24bit`, `256`, `16`, `mono`/`none` (any case);
  /// `None` (or anything else) detects it ([`color`]).
  pub color_mode: Option<String>,
  /// The text shown on the bottom row after the first Ctrl+C.
  pub interrupt_hint: String,
  /// Makes the filter every frame goes through; `None` leaves frames as nsh drew them.
  pub frame_filter: Option<FrameFilterFactory>,
  /// The terminal belongs to someone who must not see the program's internals (a remote
  /// SSH user): after a panic the restored terminal gets one neutral line instead of the
  /// panic's message, source location and backtrace, and errors the app reported but
  /// survived are not printed at the end. `false` (a developer's terminal): the panic is
  /// reported as usual and such errors are listed once the terminal is restored.
  pub quiet_panics: bool,
}

impl HostOptions {
  /// `root` as `program`, with nsh's base UI set, a detected colour depth, the English
  /// [`interrupt::DEFAULT_HINT`] and no frame filter.
  pub fn new(program: impl Into<String>, root: Root) -> Self {
    Self {
      program: program.into(),
      root,
      ui_sets: AppOptions::default().ui_sets,
      color_mode: None,
      interrupt_hint: interrupt::DEFAULT_HINT.to_owned(),
      frame_filter: None,
      quiet_panics: false,
    }
  }

  /// Replaces the UI sets.
  pub fn ui_sets(mut self, ui_sets: Vec<Registry>) -> Self {
    self.ui_sets = ui_sets;
    self
  }

  /// Forces a colour depth (see [`Self::color_mode`]).
  pub fn color_mode(mut self, color_mode: Option<String>) -> Self {
    self.color_mode = color_mode;
    self
  }

  /// Replaces the Ctrl+C hint.
  pub fn interrupt_hint(mut self, hint: impl Into<String>) -> Self {
    self.interrupt_hint = hint.into();
    self
  }

  /// Puts every frame through the filter `factory` makes once the depth is known.
  pub fn frame_filter(
    mut self,
    factory: impl FnOnce(ColorDepth) -> Box<dyn FrameFilter> + 'static,
  ) -> Self {
    self.frame_filter = Some(Box::new(factory));
    self
  }

  /// Hides panics and internal errors from the terminal (see [`Self::quiet_panics`]).
  pub fn quiet_panics(mut self, quiet: bool) -> Self {
    self.quiet_panics = quiet;
    self
  }
}

/// The error [`run`] returns when a thread panicked during the session.
#[derive(Debug)]
pub struct Panicked;

impl std::fmt::Display for Panicked {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("a thread panicked; the session ended")
  }
}

impl std::error::Error for Panicked {}

/// Whether `error` is [`run`]'s [`Panicked`]: the panic hook reported it already, so a
/// caller need not print anything more.
pub fn is_panic(error: &io::Error) -> bool {
  error.get_ref().is_some_and(|inner| inner.is::<Panicked>())
}

/// The one line a quiet session prints after a panic.
fn panic_line(program: &str) -> String {
  format!("{program}: the session ended after an internal error")
}

/// What the app reported through `on_app_error` during the run.
#[derive(Debug, Default)]
struct Reports {
  /// The first fatal error (the app did not load, or its VM stopped): it ends the run.
  fatal: Option<String>,
  /// How many errors the app kept running after (an exception escaping a callback, an
  /// asset that did not load, a development update), and the last one.
  survived: usize,
  last: Option<String>,
}

impl Reports {
  fn record(&mut self, error: &AppError) {
    self.note(error.is_fatal(), error.to_string());
  }

  fn note(&mut self, fatal: bool, text: String) {
    if fatal {
      self.fatal.get_or_insert(text);
    } else {
      self.survived += 1;
      self.last = Some(text);
    }
  }

  /// The line listing the errors the app survived, for a developer's terminal.
  fn survived_line(&self, program: &str) -> Option<String> {
    let last = self.last.as_deref()?;
    Some(format!(
      "{program}: the app reported {} error(s) it kept running after; the last: {last}",
      self.survived
    ))
  }
}

/// How the run ends. In order: a panic is an error ([`Panicked`]) whatever else
/// happened (its hook requests the shutdown, and the writes it stops fail); then a fatal
/// app error; then the loop's own error, unless the session `ended` (a signal, a hang-up),
/// when a failed write is the end of the session, not an error. Errors the app survived
/// never turn a normal end into a failure.
fn outcome(
  result: io::Result<()>,
  panicked: bool,
  ended: bool,
  reports: &Reports,
) -> io::Result<()> {
  if panicked {
    return Err(io::Error::other(Panicked));
  }
  if let Some(fatal) = &reports.fatal {
    return Err(io::Error::other(fatal.clone()));
  }
  match result {
    Err(error) if !ended => Err(error),
    _ => Ok(()),
  }
}

/// The colour depth before the terminal is asked: `color_mode` when given (`truecolor`,
/// `256`, `16`, `mono`), else what `NO_COLOR`, `COLORTERM` and `TERM` (its brand, then its
/// colour count) in this process's environment say. See [`color`] for the whole table;
/// [`run`] may still raise 256 colours to 24-bit when the terminal reports `RGB`.
pub fn color_depth(color_mode: Option<&str>) -> ColorDepth {
  color_depth_from(color_mode, |name| env::var(name).ok())
}

/// [`color_depth`] over the environment `var` reads.
pub fn color_depth_from(
  color_mode: Option<&str>,
  var: impl Fn(&str) -> Option<String>,
) -> ColorDepth {
  color::Choice::from_env(color_mode, var).depth
}

/// The capabilities the surface is drawn for.
///
/// When a filter owns the colours, nsh gets no terminal colours: it draws "bandless"
/// (fills as the standard gray, accents as ANSI meanings, exact RGB for opaque accent
/// fills), which the filter then maps. Otherwise the terminal's own answers are passed on,
/// as nsh's development host does.
pub fn capabilities(
  depth: ColorDepth,
  owns_colors: bool,
  replies: &terminal::Replies,
) -> TerminalCapabilities {
  let mut capabilities = TerminalCapabilities::default();
  capabilities.colors = depth;
  capabilities.hyperlinks = !env::var("TERM").is_ok_and(|term| term.starts_with("linux"));
  if !owns_colors {
    capabilities.background_color = replies.background;
    capabilities.foreground_color = replies.foreground;
  }
  capabilities
}

/// Runs `options.root` until the app ends, the user presses Ctrl+C twice, a signal
/// arrives or the terminal hangs up (all `Ok`), a thread panics (`Err` of [`Panicked`],
/// see [`is_panic`]) or the app reports a fatal error (`Err` with its text). The terminal
/// is restored in every case.
pub fn run(options: HostOptions) -> io::Result<()> {
  let HostOptions {
    program,
    root,
    ui_sets,
    color_mode,
    interrupt_hint,
    frame_filter,
    quiet_panics,
  } = options;
  let choice = color::Choice::from_env(color_mode.as_deref(), |name| env::var(name).ok());

  let shutdown = terminal::Shutdown::install()?;
  let report = if quiet_panics {
    terminal::PanicReport::Line(panic_line(&program))
  } else {
    terminal::PanicReport::Full
  };
  let session = terminal::Session::enter(&shutdown, choice.asks_terminal(), &program, report)?;
  let depth = choice.answered(session.replies().truecolor).depth;
  let filter = frame_filter.map(|factory| factory(depth));

  let mut app_options = AppOptions::default();
  app_options.capabilities = capabilities(
    depth,
    filter.as_ref().is_some_and(|filter| filter.owns_colors()),
    &session.replies(),
  );
  app_options.capabilities.command_key = session.keyboard();
  app_options.ui_sets = ui_sets;
  let clipboard = Arc::new(clipboard::TerminalClipboard::default());
  app_options.clipboard = Some(clipboard.clone());
  let waker = terminal::Waker::new()?;
  app_options.wake = Some(waker.callback());
  let mut view = AppView::new(app_options);
  let reports = Rc::new(RefCell::new(Reports::default()));
  let reported = reports.clone();
  view.on_app_error(move |error| reported.borrow_mut().record(error));
  let cursor = Rc::new(Cell::new(CursorStyle::default()));
  let changed = cursor.clone();
  view.on_cursor_style_changed(move |style| changed.set(style));

  let (frames, writer) = terminal::Frames::new(
    terminal::Output::new(shutdown.flag()),
    session.synchronized(),
  );
  let mut terminal = Terminal::new(CrosstermBackend::new(io::BufWriter::new(writer)))?;
  let mut host = Host {
    session: &session,
    shutdown: &shutdown,
    waker: &waker,
    frames,
    filter,
    accent: if depth == ColorDepth::Monochrome {
      Color::Reset
    } else {
      Color::Yellow
    },
    hint: interrupt_hint,
    interrupt: Interrupt::default(),
    clipboard,
    cursor,
    shown: CursorStyle::default(),
    reports: reports.clone(),
  };
  // Prime the area so the first layout has the terminal's size, as nsh-tui-host does.
  host.draw(&mut terminal, &mut view, false)?;
  view.load(root)?;
  let result = host.run(&mut terminal, &mut view);
  let ended = shutdown.requested() || terminal::disconnected().unwrap_or(true);
  view.stop();
  // Nothing may write once the loop ended but the session's own bounded restore.
  shutdown.request();
  drop(terminal);
  drop(host);
  let panicked = session.panicked();
  drop(session);
  let reports = reports.borrow();
  let result = outcome(result, panicked, ended, &reports);
  if !quiet_panics
    && result.is_ok()
    && let Some(line) = reports.survived_line(&program)
  {
    // The terminal is the shell's again; best effort, as the restore.
    let _ = writeln!(io::stderr(), "{line}");
  }
  result
}

/// The run loop's state: nsh's `run_app` loop with the frame filter, the Ctrl+C hint,
/// the clipboard and the cursor shape between frames.
struct Host<'a> {
  session: &'a terminal::Session,
  shutdown: &'a terminal::Shutdown,
  waker: &'a terminal::Waker,
  frames: terminal::Frames,
  filter: Option<Box<dyn FrameFilter>>,
  accent: Color,
  hint: String,
  interrupt: Interrupt,
  clipboard: Arc<clipboard::TerminalClipboard>,
  cursor: Rc<Cell<CursorStyle>>,
  shown: CursorStyle,
  reports: Rc<RefCell<Reports>>,
}

impl Host<'_> {
  fn draw<B: Backend<Error = io::Error>>(
    &mut self,
    terminal: &mut Terminal<B>,
    view: &mut AppView,
    repaint: bool,
  ) -> io::Result<()> {
    if repaint {
      terminal.clear()?;
    }
    let hint = self
      .interrupt
      .armed(Instant::now())
      .then_some(self.hint.as_str());
    let accent = self.accent;
    let filter = &mut self.filter;
    terminal.draw(|frame| {
      let area = frame.area();
      view.render(frame, area);
      if let Some(hint) = hint {
        interrupt::draw_hint(frame.buffer_mut(), area, accent, hint);
      }
      if let Some(filter) = filter {
        filter.apply(frame.buffer_mut());
      }
    })?;
    self.frames.commit()
  }

  fn run<B: Backend<Error = io::Error>>(
    &mut self,
    terminal: &mut Terminal<B>,
    view: &mut AppView,
  ) -> io::Result<()> {
    // The longest wait without a wake-up: signals and hang-ups are seen within it.
    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    let mut redraw = true;
    let mut repaint = false;
    loop {
      // A panic restored the terminal already (and stops every write): draw nothing more.
      if self.session.panicked() {
        return Err(io::Error::other(Panicked));
      }
      if self.reports.borrow().fatal.is_some() {
        return Ok(());
      }
      if self.interrupt.expire(Instant::now()) {
        redraw = true;
      }
      if redraw
        || view
          .next_frame_deadline()
          .is_some_and(|due| due <= Instant::now())
      {
        self.draw(terminal, view, std::mem::take(&mut repaint))?;
        redraw = false;
      }
      if view.exit_requested() || self.shutdown.requested() {
        return Ok(());
      }
      self.clipboard.flush(self.frames.output())?;
      if self.cursor.get() != self.shown {
        self.shown = self.cursor.get();
        terminal::set_cursor_style(self.frames.output(), self.shown)?;
      }
      let mut wait = view.next_frame_deadline().map_or(POLL_INTERVAL, |due| {
        due
          .saturating_duration_since(Instant::now())
          .min(POLL_INTERVAL)
      });
      if let Some(until) = self.interrupt.deadline() {
        wait = wait.min(until.saturating_duration_since(Instant::now()));
      }
      let waiting = Instant::now();
      match terminal::poll(wait, self.shutdown, self.waker)? {
        RunEvent::Input(input) if is_ctrl_c(&input) => {
          if is_press(&input) && self.interrupt.press(Instant::now()) == Press::Quit {
            return Ok(());
          }
          redraw = true;
        }
        RunEvent::Input(input) => {
          if is_press(&input) && self.interrupt.disarm() {
            redraw = true;
          }
          match &input {
            Input::Focus(true) => {
              self.session.reassert(self.frames.output())?;
              repaint = true;
            }
            Input::Focus(false) => self.session.focus_lost(self.frames.output())?,
            _ => {}
          }
          view.handle(input);
        }
        RunEvent::Redraw => redraw = true,
        RunEvent::Exit => return Ok(()),
        _ => {
          // An early return from the poll: honour the rest of the wait unless engine
          // work arrived, rather than polling again in a tight loop (as run_app does).
          let remaining = wait.saturating_sub(waiting.elapsed());
          let remaining = view.next_frame_deadline().map_or(remaining, |due| {
            remaining.min(due.saturating_duration_since(Instant::now()))
          });
          if !remaining.is_zero() {
            std::thread::sleep(remaining);
          }
        }
      }
    }
  }
}

/// Whether `input` is Ctrl+C (press, repeat or release).
fn is_ctrl_c(input: &Input) -> bool {
  let (key, modifiers) = match input {
    Input::Key { key, modifiers, .. } | Input::KeyRepeat { key, modifiers } => (key, modifiers),
    _ => return false,
  };
  key == "c" && modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.cmd
}

/// Whether `input` is a key going down (not a repeat or release).
fn is_press(input: &Input) -> bool {
  matches!(input, Input::Key { release: false, .. })
}

fn modifiers(keys: KeyModifiers) -> Modifiers {
  Modifiers {
    ctrl: keys.contains(KeyModifiers::CONTROL),
    alt: keys.contains(KeyModifiers::ALT),
    shift: keys.contains(KeyModifiers::SHIFT),
    cmd: keys.contains(KeyModifiers::SUPER),
    // A terminal does not say whether a character came from AltGr (as nsh-tui-host).
    alt_graph: false,
  }
}

fn button(button: event::MouseButton) -> MouseButton {
  match button {
    event::MouseButton::Left => MouseButton::Left,
    event::MouseButton::Right => MouseButton::Right,
    event::MouseButton::Middle => MouseButton::Middle,
  }
}

/// Converts a crossterm event into the surface's input; a resize is a redraw.
pub fn convert(event: Event) -> RunEvent {
  match event {
    Event::Resize(..) => RunEvent::Redraw,
    Event::FocusGained => RunEvent::Input(Input::Focus(true)),
    Event::FocusLost => RunEvent::Input(Input::Focus(false)),
    Event::Paste(text) => RunEvent::Input(Input::Paste(text)),
    Event::Mouse(mouse) => {
      let mut input = MouseInput::default();
      input.column = mouse.column;
      input.row = mouse.row;
      input.modifiers = modifiers(mouse.modifiers);
      input.kind = match mouse.kind {
        event::MouseEventKind::Down(b) => MouseKind::Down(button(b)),
        event::MouseEventKind::Up(b) => MouseKind::Up(button(b)),
        event::MouseEventKind::Moved | event::MouseEventKind::Drag(_) => MouseKind::Move,
        event::MouseEventKind::ScrollUp => MouseKind::Wheel { x: 0, y: -1 },
        event::MouseEventKind::ScrollDown => MouseKind::Wheel { x: 0, y: 1 },
        event::MouseEventKind::ScrollLeft => MouseKind::Wheel { x: -1, y: 0 },
        event::MouseEventKind::ScrollRight => MouseKind::Wheel { x: 1, y: 0 },
      };
      RunEvent::Input(Input::Mouse(input))
    }
    Event::Key(key) => {
      let name = key_name(key.code);
      let mut modifiers = modifiers(key.modifiers);
      if key.code == KeyCode::BackTab {
        modifiers.shift = true;
      }
      if key.kind == KeyEventKind::Repeat {
        RunEvent::Input(Input::KeyRepeat {
          key: name,
          modifiers,
        })
      } else {
        RunEvent::Input(Input::Key {
          key: name,
          modifiers,
          release: key.kind == KeyEventKind::Release,
        })
      }
    }
  }
}

/// A key's name as nsh's surface names it: the character, or the DOM key name.
pub fn key_name(code: KeyCode) -> String {
  match code {
    KeyCode::Char(c) => c.to_string(),
    KeyCode::Enter => "Enter".into(),
    KeyCode::Esc => "Escape".into(),
    KeyCode::Tab | KeyCode::BackTab => "Tab".into(),
    KeyCode::Backspace => "Backspace".into(),
    KeyCode::Delete => "Delete".into(),
    KeyCode::Insert => "Insert".into(),
    KeyCode::Home => "Home".into(),
    KeyCode::End => "End".into(),
    KeyCode::PageUp => "PageUp".into(),
    KeyCode::PageDown => "PageDown".into(),
    KeyCode::Left => "ArrowLeft".into(),
    KeyCode::Right => "ArrowRight".into(),
    KeyCode::Up => "ArrowUp".into(),
    KeyCode::Down => "ArrowDown".into(),
    KeyCode::F(number) => format!("F{number}"),
    _ => "Unidentified".into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn forced_color_modes_win() {
    let everything = |name: &str| match name {
      "COLORTERM" => Some("truecolor".to_owned()),
      "NO_COLOR" => Some("1".to_owned()),
      _ => None,
    };
    for (mode, depth) in [
      ("mono", ColorDepth::Monochrome),
      ("TrueColor", ColorDepth::TrueColor),
      ("256", ColorDepth::Ansi256),
      ("16", ColorDepth::Ansi16),
    ] {
      assert_eq!(color_depth_from(Some(mode), everything), depth);
    }
  }

  #[test]
  fn the_environment_decides_otherwise() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
      move |name: &str| {
        pairs
          .iter()
          .find(|(key, _)| *key == name)
          .map(|(_, value)| (*value).to_owned())
      }
    };
    assert_eq!(
      color_depth_from(None, env(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")])),
      ColorDepth::Monochrome
    );
    assert_eq!(
      color_depth_from(None, env(&[("COLORTERM", "24bit"), ("TERM", "xterm")])),
      ColorDepth::TrueColor
    );
    assert_eq!(
      color_depth_from(None, env(&[("TERM", "xterm-256color")])),
      ColorDepth::Ansi256
    );
    assert_eq!(
      color_depth_from(None, env(&[("TERM", "xterm")])),
      ColorDepth::Ansi16
    );
    assert_eq!(
      color_depth_from(None, env(&[("TERM", "dumb")])),
      ColorDepth::Monochrome
    );
    assert_eq!(
      color_depth_from(Some("bogus"), env(&[("TERM", "xterm-256color")])),
      ColorDepth::Ansi256
    );
  }

  #[test]
  fn keys_get_dom_names() {
    assert_eq!(key_name(KeyCode::Left), "ArrowLeft");
    assert_eq!(key_name(KeyCode::Char('q')), "q");
    assert!(matches!(convert(Event::Resize(80, 24)), RunEvent::Redraw));
  }

  #[test]
  fn ctrl_c_is_recognised_in_every_phase() {
    let ctrl = Modifiers {
      ctrl: true,
      ..Modifiers::default()
    };
    let press = Input::Key {
      key: "c".into(),
      modifiers: ctrl,
      release: false,
    };
    assert!(is_ctrl_c(&press) && is_press(&press));
    let release = Input::Key {
      key: "c".into(),
      modifiers: ctrl,
      release: true,
    };
    assert!(is_ctrl_c(&release) && !is_press(&release));
    let plain = Input::Key {
      key: "c".into(),
      modifiers: Modifiers::default(),
      release: false,
    };
    assert!(!is_ctrl_c(&plain));
  }

  #[test]
  fn surfaces_whose_colours_a_filter_owns_get_no_terminal_colours() {
    let replies = terminal::Replies {
      background: Some([255, 255, 255]),
      foreground: Some([0, 0, 0]),
      ..terminal::Replies::default()
    };
    let owned = capabilities(ColorDepth::TrueColor, true, &replies);
    assert_eq!(owned.background_color, None);
    let plain = capabilities(ColorDepth::Ansi16, false, &replies);
    assert_eq!(plain.background_color, Some([255, 255, 255]));
  }

  fn failed_write() -> io::Result<()> {
    Err(io::Error::new(
      io::ErrorKind::BrokenPipe,
      "the terminal hung up",
    ))
  }

  #[test]
  fn a_panic_wins_over_everything() {
    let mut reports = Reports::default();
    reports.note(true, "load failed".into());
    // The hook requested the shutdown, so the session looks ended and its writes failed:
    // still an error, and a recognisable one.
    for ended in [false, true] {
      for result in [Ok(()), failed_write()] {
        let error = outcome(result, true, ended, &reports).unwrap_err();
        assert!(is_panic(&error), "{error}");
      }
    }
    assert!(!is_panic(&failed_write().unwrap_err()));
  }

  #[test]
  fn a_fatal_app_error_fails_the_run() {
    let mut reports = Reports::default();
    reports.note(true, "first".into());
    reports.note(true, "second".into());
    let error = outcome(Ok(()), false, true, &reports).unwrap_err();
    assert_eq!(error.to_string(), "first");
    assert!(!is_panic(&error));
  }

  #[test]
  fn errors_the_app_survived_leave_a_normal_end_normal() {
    let mut reports = Reports::default();
    assert_eq!(reports.survived_line("probe"), None);
    reports.note(false, "Error: one".into());
    reports.note(false, "Error: two".into());
    assert!(outcome(Ok(()), false, false, &reports).is_ok());
    assert!(outcome(failed_write(), false, true, &reports).is_ok());
    let line = reports.survived_line("probe").unwrap();
    assert!(
      line.starts_with("probe: ") && line.contains("2 error(s)") && line.ends_with("Error: two")
    );
  }

  #[test]
  fn a_failed_write_is_an_error_only_while_the_session_lasts() {
    let reports = Reports::default();
    assert!(outcome(Ok(()), false, false, &reports).is_ok());
    assert!(outcome(failed_write(), false, true, &reports).is_ok());
    let error = outcome(failed_write(), false, false, &reports).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
  }

  #[test]
  fn the_quiet_panic_line_is_neutral() {
    assert_eq!(
      panic_line("probe"),
      "probe: the session ended after an internal error"
    );
  }

  mod probe {
    use nsh_core::prelude::*;

    #[component]
    pub fn Probe() -> impl IntoElement {
      text("probe")
    }
  }

  #[test]
  fn options_start_neutral() {
    struct Probe;
    impl FrameFilter for Probe {
      fn apply(&mut self, _: &mut Buffer) {}
    }
    let options = HostOptions::new("probe", Root::of::<probe::Probe>());
    assert_eq!(options.program, "probe");
    assert_eq!(options.interrupt_hint, interrupt::DEFAULT_HINT);
    assert!(options.color_mode.is_none() && options.frame_filter.is_none());
    assert!(!options.quiet_panics);
    assert!(!options.ui_sets.is_empty(), "nsh's base set");
    let options = options
      .color_mode(Some("16".into()))
      .interrupt_hint(" bye ")
      .quiet_panics(true)
      .frame_filter(|_| Box::new(Probe));
    assert!(options.quiet_panics);
    assert_eq!(options.color_mode.as_deref(), Some("16"));
    assert_eq!(options.interrupt_hint, " bye ");
    let filter = (options.frame_filter.unwrap())(ColorDepth::Ansi16);
    assert!(filter.owns_colors(), "a filter owns the colours by default");
  }
}
