//! The too-small page (01-login.md, "终端太小"): below 80×24 only this shows, with the
//! current and the required size; the shop waits behind it, unchanged, and comes back as it
//! was once the terminal is large enough.
//!
//! ```text
//!  ! 演示环境 · 不是真实商店
//!
//!                          ◆ROAST/TTY
//!
//!                           终端太小
//!                 当前 60×18 · 需要至少 80×24
//!              放大终端窗口后，会回到刚才的画面。
//!
//!                   Ctrl+C 连按两次断开连接
//! ```
//!
//! Row 0 is the demo notice (cut when narrower); the rest is centred in the rows below. When
//! they are too few, the Ctrl+C line goes first, then the "放大…" line, then the wordmark; the
//! title and the size line stay (cut when narrower). Only the host's Ctrl+C works here.

use nsh_core::prelude::*;

use crate::ui::{
  DEMO_NOTICE, Layout, Tone, label,
  layout::{MIN_COLS, MIN_ROWS},
  strong, wordmark_line,
};

/// The size line for a terminal of `cols` × `rows`.
pub fn message(cols: u16, rows: u16) -> String {
  format!("当前 {cols}×{rows} · 需要至少 {MIN_COLS}×{MIN_ROWS}")
}

/// The "放大…" line.
pub const ENLARGE: &str = "放大终端窗口后，会回到刚才的画面。";
/// The Ctrl+C line.
pub const CTRL_C: &str = "Ctrl+C 连按两次断开连接";

/// What the page keeps in `rows` rows (row 0, the notice, aside): (wordmark, enlarge line,
/// Ctrl+C line), each with the blank row before the next part.
pub fn kept(rows: u16) -> (bool, bool, bool) {
  let room = rows.saturating_sub(1);
  (room >= 4, room >= 5, room >= 7)
}

/// The page, following the live size.
pub fn too_small(layout: Layout) -> impl IntoElement {
  let parts = computed(move || kept(layout.viewport().rows));
  view()
    .flex_1()
    .label("终端太小")
    .child(
      view()
        .flex_none()
        .flex_row()
        .px(crate::ui::layout::cols(1))
        .child(strong(DEMO_NOTICE, Tone::Acc).number_of_lines(1)),
    )
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .items_center()
        .justify_center()
        .child(when(
          move || parts.get().0,
          || {
            view()
              .flex_none()
              .items_center()
              .child(wordmark_line())
              .child(text(" "))
          },
        ))
        .child(strong("终端太小", Tone::Acc))
        .child(
          label(
            move || {
              let viewport = layout.viewport();
              SharedString::from(message(viewport.cols, viewport.rows))
            },
            Tone::Soft,
          )
          .number_of_lines(1),
        )
        .child(when(move || parts.get().1, || label(ENLARGE, Tone::Soft)))
        .child(when(
          move || parts.get().2,
          || {
            view()
              .flex_none()
              .items_center()
              .child(text(" "))
              .child(label(CTRL_C, Tone::Mute))
          },
        )),
    )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn the_message_names_both_sizes() {
    let text = message(60, 18);
    assert_eq!(text, "当前 60×18 · 需要至少 80×24");
    assert!(display_width(&text) <= 40);
  }

  #[test]
  fn parts_give_way_from_the_bottom() {
    assert_eq!(kept(18), (true, true, true));
    assert_eq!(kept(8), (true, true, true));
    assert_eq!(kept(7), (true, true, false));
    assert_eq!(kept(5), (true, false, false));
    assert_eq!(kept(4), (false, false, false));
  }
}
