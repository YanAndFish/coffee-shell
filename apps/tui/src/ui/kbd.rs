//! Kbd: a key cap (`components/Kbd`).
//!
//! ```text
//! ‹a›  ‹⏎›  ‹↑↓›  ‹space›  ‹Esc›  ‹  1 – 4  ›
//! ```
//!
//! In cells the key sits between one blank cell on each side, the three on the `kbd` fill,
//! the key bold `fg`; 16 colours and monochrome show the cap in reverse video. The wide form
//! (the help dialog) is 11 cells with the key centred. Inside a primary button the key is
//! plain bold `ink` on the button's own fill ([`kbd_on_fill`]).

use nsh_core::prelude::*;

use super::{
  label::strong,
  layout,
  text::{center, display_width},
  theme::{Fill, Tone},
};

/// The width of the wide key cap of the help dialog.
pub const WIDE_CELLS: u16 = 11;

/// The cells the key cap of `key` takes.
pub fn kbd_width(key: &str) -> usize {
  display_width(key) + 2
}

/// The key cap of `key`: a lower-case letter, a digit, a symbol (`⏎`, `↑↓`) or a word
/// (`Esc`, `space`).
pub fn kbd(key: impl IntoReactive<SharedString>) -> View {
  view()
    .flex_none()
    .flex_row()
    .px_2()
    .bg(Fill::Kbd.color())
    .child(strong(key, Tone::Fg))
}

/// The help dialog's key cap: [`WIDE_CELLS`] cells, the key centred.
pub fn kbd_wide(key: &str) -> View {
  let inner = center(key, usize::from(WIDE_CELLS));
  view()
    .flex_none()
    .w(layout::cols(WIDE_CELLS))
    .bg(Fill::Kbd.color())
    .child(strong(inner, Tone::Fg))
}

/// A key inside a primary button: `‹⏎›` as text on the button's fill, bold, no cap fill.
pub fn kbd_on_fill(key: &str) -> Text {
  strong(format!(" {key} "), Tone::Ink)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn caps_are_the_key_and_a_cell_each_side() {
    assert_eq!(kbd_width("a"), 3);
    assert_eq!(kbd_width("space"), 7);
    assert_eq!(kbd_width("↑↓"), 4);
    assert_eq!(center("1 – 4", usize::from(WIDE_CELLS)), "   1 – 4   ");
  }
}
