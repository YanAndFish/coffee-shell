//! `coffee-tui`: runs the `coffee-app` root in the terminal this process was started in.
//!
//! The terminal work is `tty-host`'s (raw mode, startup queries, colour depth, signals,
//! hang-ups, Ctrl+C twice, OSC 52); this crate adds the shop:
//!
//! - the session settings `tty-gateway` passes in `COFFEE_*` variables
//!   ([`session_env_from_process`]), given to `coffee_app::configure` before the app loads;
//! - the root with the app's theme, id and dark appearance, or the component gallery
//!   ([`app_root`]);
//! - the palette pin, which puts the design's exact colours back over nsh's ANSI meanings
//!   ([`palette`], a [`tty_host::FrameFilter`]);
//! - the Chinese Ctrl+C hint ([`INTERRUPT_HINT`]).

use std::{env, io};

use coffee_app::SessionEnv;
use tty_host::{Appearance, HostOptions, Root};

pub mod palette;

use palette::{Palette, Pin};

/// This program's name, in the host's messages.
pub const PROGRAM: &str = "coffee-tui";

/// The hint shown after the first Ctrl+C.
pub const INTERRUPT_HINT: &str = " 再按一次 Ctrl+C 退出 ";

/// Reads the session settings `tty-gateway` passes in the environment.
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

/// The app's root as nsh's generated runners mount it from `nsh.json`: its theme
/// (`coffee_app::THEME_JSON`), id (`coffee_app::APP_ID`) and dark appearance. With
/// `COFFEE_UI_GALLERY=1` (not `0`) in the environment the root is the component gallery
/// (`coffee_app::UiGallery`) instead of the shop: the way to look at every `ui` component
/// through the palette pin and the colour depths.
pub fn app_root() -> io::Result<Root> {
  app_root_for(coffee_app::ui::gallery::requested())
}

/// [`app_root`] with the shop (`gallery == false`) or the component gallery.
pub fn app_root_for(gallery: bool) -> io::Result<Root> {
  let theme = nsh_base::Theme::from_json(coffee_app::THEME_JSON)
    .map_err(|error| io::Error::other(format!("theme.json: {error}")))?;
  let root = if gallery {
    Root::of::<coffee_app::UiGallery>()
  } else {
    Root::of::<coffee_app::App>()
  };
  Ok(
    root
      .tokens(theme.into_tokens())
      .app_id(coffee_app::APP_ID)
      .appearance(Appearance::Dark),
  )
}

/// What `tty-host` runs for the session `env`: [`app_root`] with nsh's base UI set and
/// the app's, the colour depth `env.color_mode` forces (`COFFEE_COLORS`), the Chinese
/// Ctrl+C hint and the palette pin at every depth.
pub fn host_options(env: &SessionEnv) -> io::Result<HostOptions> {
  let palette = Palette::app().map_err(io::Error::other)?;
  Ok(
    HostOptions::new(PROGRAM, app_root()?)
      .ui_sets(vec![nsh_base::registry(), coffee_app::registry()])
      .color_mode(env.color_mode.clone())
      .interrupt_hint(INTERRUPT_HINT)
      .frame_filter(move |depth| Box::new(Pin::new(palette, depth))),
  )
}

/// Configures `coffee_app` with `env` and runs its root until the app ends, the user
/// presses Ctrl+C twice, a signal arrives or the terminal hangs up.
pub fn run(env: SessionEnv) -> io::Result<()> {
  let options = host_options(&env)?;
  coffee_app::configure(env);
  tty_host::run(options)
}

#[cfg(test)]
mod tests {
  use tty_host::{
    interrupt::draw_hint,
    ratatui_core::{buffer::Buffer, layout::Rect, style::Color},
  };

  use super::*;

  #[test]
  fn the_root_takes_the_theme() {
    assert!(app_root().is_ok());
    assert!(app_root_for(true).is_ok(), "the gallery mounts as the root");
  }

  #[test]
  fn the_session_settings_reach_the_host() {
    let env = SessionEnv {
      color_mode: Some("mono".into()),
      ..SessionEnv::default()
    };
    let options = host_options(&env).unwrap();
    assert_eq!(options.program, PROGRAM);
    assert_eq!(options.color_mode.as_deref(), Some("mono"));
    assert_eq!(options.interrupt_hint, INTERRUPT_HINT);
    assert_eq!(options.ui_sets.len(), 2);
    let pin = (options.frame_filter.expect("the pin"))(tty_host::ColorDepth::Monochrome);
    assert!(pin.owns_colors(), "the pin owns the colours, mono included");
  }

  #[test]
  fn the_hint_sits_centred_on_the_last_row() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buffer = Buffer::empty(area);
    draw_hint(&mut buffer, area, Color::Yellow, INTERRUPT_HINT);
    let row: String = (0..40)
      .map(|x| buffer[(x, 4)].symbol().to_string())
      .collect();
    let squeezed: String = row.chars().filter(|c| *c != ' ').collect();
    assert!(squeezed.contains("再按一次Ctrl+C退出"), "{row:?}");
    assert!(
      buffer[(20, 4)]
        .modifier
        .contains(tty_host::ratatui_core::style::Modifier::REVERSED)
    );
    // A narrow terminal clips the hint instead of panicking.
    let tiny = Rect::new(0, 0, 6, 1);
    let mut buffer = Buffer::empty(tiny);
    draw_hint(&mut buffer, tiny, Color::Yellow, INTERRUPT_HINT);
  }
}
