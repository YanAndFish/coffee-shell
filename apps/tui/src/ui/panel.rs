//! Panel: the basic container, a box with its legend on the top border (`components/Panel`):
//!
//! ```text
//! ╭─ 商品列表 · 8 款 ───────╮
//! │ …                       │
//! ╰─────────────────────────╯
//! ```
//!
//! nsh clips a bordered view's children to inside its border, so the legend is a sibling
//! laid over the border row (absolute, drawn after the box), two cells in.

use nsh_core::prelude::*;

use super::{layout, theme};

/// A panel titled `legend` ("中文 ENGLISH", or "商品列表 · 8 款") around `content`, filling its
/// parent's main axis. The box keeps a one-cell side padding, so nsh keeps its border.
pub fn panel(legend: impl IntoReactive<SharedString>, content: impl IntoElement) -> View {
  view()
    .flex_1()
    .min_w(px(0.0))
    .child(
      view()
        .flex_1()
        .border_1()
        .border_color(theme::line)
        .rounded(px(8.0))
        .px(px(12.0))
        .child(content),
    )
    .child(
      // The window colour behind the legend cuts the border, a cell each side.
      view()
        .absolute()
        .top(px(0.0))
        .left(layout::cols(2))
        .px_2()
        .bg(theme::bg)
        .child(
          text(legend)
            .text_color(theme::acc)
            .font_weight(FontWeight::BOLD),
        ),
    )
}
