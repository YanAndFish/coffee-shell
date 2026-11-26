//! `tty-host`: runs an nsh 0.6.1 Rust-mode root in the terminal this process was started
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
  env, io,
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
/// arrives or the terminal hangs up.
pub fn run(options: HostOptions) -> io::Result<()> {
  let HostOptions {
    program,
    root,
    ui_sets,
    color_mode,
    interrupt_hint,
    frame_filter,
  } = options;
  let choice = color::Choice::from_env(color_mode.as_deref(), |name| env::var(name).ok());

  let shutdown = terminal::Shutdown::install()?;
  let session = terminal::Session::enter(&shutdown, choice.asks_terminal(), &program)?;
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
  let failure = Rc::new(RefCell::new(None));
  let reported = failure.clone();
  view.on_app_error(move |error| {
    *reported.borrow_mut() = Some(error.to_string());
  });
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
  };
  // Prime the area so the first layout has the terminal's size, as nsh-tui-host does.
  host.draw(&mut terminal, &mut view, false)?;
  view.load(root)?;
  let result = host.run(&mut terminal, &mut view);
  // A write that failed because the terminal hung up or a signal arrived is the end of
  // the session, not an error.
  let ended = shutdown.requested() || terminal::disconnected().unwrap_or(true);
  let result = result.or_else(|error| if ended { Ok(()) } else { Err(error) });
  view.stop();
  // Nothing may write once the loop ended but the session's own bounded restore.
  shutdown.request();
  drop(terminal);
  drop(host);
  drop(session);
  let error = failure.borrow_mut().take();
  match (result, error) {
    (Err(error), _) => Err(error),
    (_, Some(error)) => Err(io::Error::other(error)),
    _ => Ok(()),
  }
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
    assert!(!options.ui_sets.is_empty(), "nsh's base set");
    let options = options
      .color_mode(Some("16".into()))
      .interrupt_hint(" bye ")
      .frame_filter(|_| Box::new(Probe));
    assert_eq!(options.color_mode.as_deref(), Some("16"));
    assert_eq!(options.interrupt_hint, " bye ");
    let filter = (options.frame_filter.unwrap())(ColorDepth::Ansi16);
    assert!(filter.owns_colors(), "a filter owns the colours by default");
  }
}
