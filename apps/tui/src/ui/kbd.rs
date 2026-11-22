//! Kbd: a key cap (`components/Kbd`). In cells: the key between one blank cell on each side
//! on the `kbd-bg` fill, so ` ⏎ ` reads as a key with or without colour.

use nsh_core::prelude::*;

use super::theme;

/// The key cap of `key`: a lower-case letter, a digit, a symbol (`⏎`, `↑↓`) or a word
/// (`Esc`, `space`).
pub fn kbd(key: impl IntoReactive<SharedString>) -> View {
  view()
    .flex_none()
    .px_2()
    .bg(theme::kbd_bg)
    .child(text(key).text_color(theme::fg))
}
