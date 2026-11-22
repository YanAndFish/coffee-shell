//! Loading: a spinner glyph and the step under way (`components/Meter`, "加载状态写法"), as
//! `⠹ 正在加载商品…`. The glyph stands still: an endless animation would keep a headless
//! test from ever going quiet, and the text alone says what is happening.

use nsh_core::prelude::*;

use super::theme;

/// The spinner's resting glyph.
pub const SPINNER: &str = "⠹";

/// A loading line saying `message`.
pub fn loading(message: impl IntoReactive<SharedString>) -> impl IntoElement {
  view().flex_1().items_center().justify_center().child(
    view()
      .flex_row()
      .gap_2()
      .label("加载中")
      .child(
        text(SPINNER)
          .text_color(theme::acc)
          .font_weight(FontWeight::BOLD),
      )
      .child(text(message).text_color(theme::soft)),
  )
}
