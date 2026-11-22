//! `coffee-host`: runs the `coffee-app` root in the terminal this process was started in.
//!
//! Minimal bootstrap host: crossterm raw mode + alternate screen + mouse + bracketed paste,
//! an `nsh_platform_tui::AppView` with the app's root, and nsh's `run_app` loop. Signal
//! handling, terminal colour queries (OSC 10/11), the kitty keyboard protocol and
//! synchronized output are not done yet; `nsh-tui-host` (nsh's own development host) is the
//! reference for them.

use std::{env, io, time::Duration};

use coffee_app::SessionEnv;
use nsh_platform_tui::{
  AppOptions, AppView, Appearance, ColorDepth, Input, Modifiers, MouseButton, MouseInput,
  MouseKind, Root, RunEvent, TerminalCapabilities, run_app,
};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::{
  CrosstermBackend,
  crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
  },
};

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

/// The color depth: `color_mode` when given (`truecolor`, `256`, `16`, `mono`), else what
/// `NO_COLOR`, `COLORTERM` and `TERM` say.
pub fn color_depth(color_mode: Option<&str>) -> ColorDepth {
  let lower = |name: &str| env::var(name).ok().map(|value| value.to_ascii_lowercase());
  match color_mode.map(str::to_ascii_lowercase).as_deref() {
    Some("truecolor" | "24bit") => return ColorDepth::TrueColor,
    Some("256") => return ColorDepth::Ansi256,
    Some("16") => return ColorDepth::Ansi16,
    Some("mono" | "none") => return ColorDepth::Monochrome,
    _ => {}
  }
  if env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
    ColorDepth::Monochrome
  } else if lower("COLORTERM").is_some_and(|c| c == "truecolor" || c == "24bit") {
    ColorDepth::TrueColor
  } else if lower("TERM").is_some_and(|term| term.contains("256color")) {
    ColorDepth::Ansi256
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

/// Configures `coffee_app` with `env` and runs its root until the app or the terminal ends.
pub fn run(env: SessionEnv) -> io::Result<()> {
  let colors = color_depth(env.color_mode.as_deref());
  coffee_app::configure(env);
  let root = app_root()?;

  let mut capabilities = TerminalCapabilities::default();
  capabilities.colors = colors;
  let mut options = AppOptions::default();
  options.capabilities = capabilities;
  options.ui_sets = vec![nsh_base::registry(), coffee_app::registry()];
  let mut view = AppView::new(options);

  let _guard = TerminalGuard::enter()?;
  let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
  // Prime the area so the first layout has the terminal's size, as nsh-tui-host does.
  terminal.draw(|frame| view.render(frame, frame.area()))?;
  view.load(root)?;
  run_app(&mut terminal, &mut view, poll)
}

/// Raw mode, the alternate screen, mouse capture and bracketed paste, undone on drop.
struct TerminalGuard;

impl TerminalGuard {
  fn enter() -> io::Result<Self> {
    terminal::enable_raw_mode()?;
    let guard = TerminalGuard;
    execute!(
      io::stdout(),
      EnterAlternateScreen,
      event::EnableMouseCapture,
      event::EnableBracketedPaste,
      event::EnableFocusChange
    )?;
    Ok(guard)
  }
}

impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let _ = execute!(
      io::stdout(),
      event::DisableFocusChange,
      event::DisableBracketedPaste,
      event::DisableMouseCapture,
      LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
  }
}

fn poll(timeout: Duration) -> io::Result<RunEvent> {
  if event::poll(timeout)? {
    Ok(convert(event::read()?))
  } else {
    Ok(RunEvent::Idle)
  }
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
    assert!(matches!(color_depth(Some("mono")), ColorDepth::Monochrome));
    assert!(matches!(
      color_depth(Some("TrueColor")),
      ColorDepth::TrueColor
    ));
    assert!(matches!(color_depth(Some("256")), ColorDepth::Ansi256));
    assert!(matches!(color_depth(Some("16")), ColorDepth::Ansi16));
  }

  #[test]
  fn keys_get_dom_names() {
    assert_eq!(key_name(KeyCode::Left), "ArrowLeft");
    assert_eq!(key_name(KeyCode::Char('q')), "q");
    assert!(matches!(convert(Event::Resize(80, 24)), RunEvent::Redraw));
  }
}
