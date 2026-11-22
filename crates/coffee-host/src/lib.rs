//! `coffee-host`: runs the `coffee-app` root in the terminal this process was started in.
//!
//! One process serves one terminal: locally any terminal, behind `coffee-sshd` the PTY of
//! one SSH session. The host:
//!
//! - enters raw mode and the alternate screen, asks the terminal what it supports
//!   (kitty keyboard, synchronized output, its colours) and restores everything on the
//!   way out ([`terminal`]);
//! - leaves cleanly on SIGHUP, SIGTERM and SIGINT, and when the terminal hangs up (the
//!   SSH side closed the PTY), so no session outlives its client;
//! - redraws on every resize (crossterm's SIGWINCH);
//! - picks the colour depth from `COFFEE_COLORS`, then `NO_COLOR`, `COLORTERM`, `TERM`
//!   ([`color_depth`]);
//! - pins the design's exact colours over nsh's ANSI meanings in 24-bit and 256 colours
//!   ([`palette`]);
//! - turns Ctrl+C into press-twice-to-quit ([`interrupt`]);
//! - copies through OSC 52 ([`clipboard`]).

use std::{
  cell::{Cell, RefCell},
  env, io,
  rc::Rc,
  sync::Arc,
  time::{Duration, Instant},
};

use coffee_app::SessionEnv;
use nsh_platform_tui::{
  AppOptions, AppView, Appearance, ColorDepth, CursorStyle, Input, Modifiers, MouseButton,
  MouseInput, MouseKind, Root, RunEvent, TerminalCapabilities,
};
use ratatui_core::{backend::Backend, style::Color, terminal::Terminal};
use ratatui_crossterm::{
  CrosstermBackend,
  crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};

pub mod clipboard;
pub mod interrupt;
pub mod palette;
pub mod terminal;

use interrupt::{Interrupt, Press};
use palette::{Palette, Pin};

/// Reads the session settings `coffee-sshd` passes in the environment.
///
/// `COFFEE_API_URL` (default `http://127.0.0.1:3000`), `COFFEE_INTERNAL_KEY`,
/// `COFFEE_SSH_FINGERPRINT`, `COFFEE_CLIENT_IP`, `COFFEE_COLORS`.
pub fn session_env_from_process() -> SessionEnv {
  let var = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
  SessionEnv {
    api_url: var("COFFEE_API_URL").unwrap_or_else(|| "http://127.0.0.1:3000".to_owned()),
    internal_key: var("COFFEE_INTERNAL_KEY"),
    ssh_fingerprint: var("COFFEE_SSH_FINGERPRINT"),
    client_ip: var("COFFEE_CLIENT_IP"),
    color_mode: var("COFFEE_COLORS"),
  }
}

/// The colour depth: `color_mode` when given (`truecolor`, `256`, `16`, `mono`), else
/// what `NO_COLOR`, `COLORTERM` and `TERM` in this process's environment say.
pub fn color_depth(color_mode: Option<&str>) -> ColorDepth {
  color_depth_from(color_mode, |name| env::var(name).ok())
}

/// [`color_depth`] over the environment `var` reads.
pub fn color_depth_from(
  color_mode: Option<&str>,
  var: impl Fn(&str) -> Option<String>,
) -> ColorDepth {
  match color_mode.map(str::to_ascii_lowercase).as_deref() {
    Some("truecolor" | "24bit") => return ColorDepth::TrueColor,
    Some("256") => return ColorDepth::Ansi256,
    Some("16") => return ColorDepth::Ansi16,
    Some("mono" | "none") => return ColorDepth::Monochrome,
    _ => {}
  }
  let lower = |name: &str| var(name).map(|value| value.to_ascii_lowercase());
  if var("NO_COLOR").is_some_and(|value| !value.is_empty()) {
    ColorDepth::Monochrome
  } else if lower("COLORTERM").is_some_and(|c| c == "truecolor" || c == "24bit") {
    ColorDepth::TrueColor
  } else if lower("TERM").is_some_and(|term| term.contains("256color")) {
    ColorDepth::Ansi256
  } else if lower("TERM").is_some_and(|term| term == "dumb") {
    ColorDepth::Monochrome
  } else {
    ColorDepth::Ansi16
  }
}

/// The app's root as nsh's generated runners mount it from `nsh.json`: its theme
/// (`coffee_app::THEME_JSON`), id (`coffee_app::APP_ID`) and dark appearance.
pub fn app_root() -> io::Result<Root> {
  let theme = nsh_base::Theme::from_json(coffee_app::THEME_JSON)
    .map_err(|error| io::Error::other(format!("theme.json: {error}")))?;
  Ok(
    Root::of::<coffee_app::App>()
      .tokens(theme.into_tokens())
      .app_id(coffee_app::APP_ID)
      .appearance(Appearance::Dark),
  )
}

/// The capabilities the surface is drawn for.
///
/// With a pin, nsh gets no terminal colours: it draws "bandless" (fills as the standard
/// gray, accents as ANSI meanings, exact RGB for opaque accent fills), which the pin
/// then maps to the theme. Without one (16 colours, mono) the terminal's own answers
/// are passed on, as nsh's development host does.
pub fn capabilities(
  depth: ColorDepth,
  pinned: bool,
  replies: &terminal::Replies,
) -> TerminalCapabilities {
  let mut capabilities = TerminalCapabilities::default();
  capabilities.colors = depth;
  capabilities.hyperlinks = !env::var("TERM").is_ok_and(|term| term.starts_with("linux"));
  if !pinned {
    capabilities.background_color = replies.background;
    capabilities.foreground_color = replies.foreground;
  }
  capabilities
}

/// Configures `coffee_app` with `env` and runs its root until the app ends, the user
/// presses Ctrl+C twice, a signal arrives or the terminal hangs up.
pub fn run(env: SessionEnv) -> io::Result<()> {
  let depth = color_depth(env.color_mode.as_deref());
  let pin = Pin::new(Palette::app().map_err(io::Error::other)?, depth);
  coffee_app::configure(env);
  let root = app_root()?;

  let shutdown = terminal::Shutdown::install()?;
  let session = terminal::Session::enter(&shutdown)?;

  let mut options = AppOptions::default();
  options.capabilities = capabilities(depth, pin.is_some(), &session.replies());
  options.capabilities.command_key = session.keyboard();
  options.ui_sets = vec![nsh_base::registry(), coffee_app::registry()];
  let clipboard = Arc::new(clipboard::TerminalClipboard::default());
  options.clipboard = Some(clipboard.clone());
  let mut view = AppView::new(options);
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
    frames,
    pin,
    accent: if depth == ColorDepth::Monochrome {
      Color::Reset
    } else {
      Color::Yellow
    },
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

/// The run loop's state: nsh's `run_app` loop with the pin, the Ctrl+C hint, the
/// clipboard and the cursor shape between frames.
struct Host<'a> {
  session: &'a terminal::Session,
  shutdown: &'a terminal::Shutdown,
  frames: terminal::Frames,
  pin: Option<Pin>,
  accent: Color,
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
    let hint = self.interrupt.armed(Instant::now());
    let (pin, accent) = (self.pin, self.accent);
    terminal.draw(|frame| {
      let area = frame.area();
      view.render(frame, area);
      if hint {
        interrupt::draw_hint(frame.buffer_mut(), area, accent);
      }
      if let Some(pin) = pin {
        pin.buffer(frame.buffer_mut());
      }
    })?;
    self.frames.commit()
  }

  fn run<B: Backend<Error = io::Error>>(
    &mut self,
    terminal: &mut Terminal<B>,
    view: &mut AppView,
  ) -> io::Result<()> {
    const POLL_INTERVAL: Duration = Duration::from_millis(16);
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
      match terminal::poll(wait, self.shutdown)? {
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
fn key_name(code: KeyCode) -> String {
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
  fn the_root_takes_the_theme() {
    assert!(app_root().is_ok());
  }

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
  fn pinned_surfaces_get_no_terminal_colours() {
    let replies = terminal::Replies {
      background: Some([255, 255, 255]),
      foreground: Some([0, 0, 0]),
      ..terminal::Replies::default()
    };
    let pinned = capabilities(ColorDepth::TrueColor, true, &replies);
    assert_eq!(pinned.background_color, None);
    let plain = capabilities(ColorDepth::Ansi16, false, &replies);
    assert_eq!(plain.background_color, Some([255, 255, 255]));
  }
}
