//! The too-small guard: below 80×24 only this shows, with the current and the required
//! size; the shop waits behind it, unchanged.

use nsh_core::prelude::*;

use crate::ui::{
  Layout, Tone, label,
  layout::{MIN_COLS, MIN_ROWS},
  strong,
};

/// The guard's message for a terminal of `cols` × `rows`.
pub fn message(cols: u16, rows: u16) -> String {
  format!("当前 {cols}×{rows} · 至少需要 {MIN_COLS}×{MIN_ROWS}")
}

/// The guard, following the live size.
pub fn too_small(layout: Layout) -> impl IntoElement {
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .gap_4()
    .child(strong("终端太小", Tone::Danger))
    .child(label(
      move || {
        let viewport = layout.viewport();
        SharedString::from(message(viewport.cols, viewport.rows))
      },
      Tone::Soft,
    ))
    .child(label("放大窗口即可回到商店", Tone::Mute))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn the_message_names_both_sizes_and_fits_the_smallest_terminals() {
    let text = message(60, 20);
    assert_eq!(text, "当前 60×20 · 至少需要 80×24");
    assert!(display_width(&text) <= 40);
  }
}
