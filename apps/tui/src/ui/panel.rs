//! Panel: the basic container, its legend on the top border (`components/Panel`), and its
//! relatives: the centred card, the inset block, the section heading and the dashed rule.
//!
//! ```text
//! ╭─ 筛选 FILTER ───────────╮      ── 豆子档案
//! │                         │
//! │ 产区                ‹f› │      ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
//! │ ◉ 全部                8 │
//! ╰─────────────────────────╯
//! ```
//!
//! [`panel`] fills its parent's main axis and keeps nsh's own thin rounded border (`line`);
//! nsh clips a bordered view's children to inside its border, so the legend is a sibling laid
//! over the border row, two cells in, on the window colour that cuts the line. [`card`] and
//! [`inset`] are glyph frames ([`super::frame::frame`]) with the design's paddings. No panel draws a
//! shadow or a side bar.

use nsh_core::prelude::*;

use super::{
  frame::{FrameStyle, Legend, frame},
  label::strong,
  layout,
  theme::{self, Tone},
};

/// A panel titled `legend` ("中文 ENGLISH", or "商品列表 · 8 款") around `content`, filling its
/// parent's main axis. The box keeps a one-cell side padding, so nsh keeps its border.
pub fn panel(legend: impl IntoReactive<SharedString>, content: impl IntoElement) -> View {
  panel_toned(legend, Tone::Acc, content)
}

/// [`panel`] with its legend in `tone` (`Acc2` on a success receipt).
pub fn panel_toned(
  legend: impl IntoReactive<SharedString>,
  tone: Tone,
  content: impl IntoElement,
) -> View {
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
        .child(strong(legend, tone)),
    )
}

/// A panel without a legend (the detail page's left column): only its border.
pub fn panel_plain(content: impl IntoElement) -> View {
  view().flex_1().min_w(px(0.0)).child(
    view()
      .flex_1()
      .border_1()
      .border_color(theme::line)
      .rounded(px(8.0))
      .px(px(12.0))
      .child(content),
  )
}

/// A centred card (login, receipt, guest notice): a rounded frame `cols` wide and `rows` tall
/// (`None`: as tall as its content) with `legend`, its content four cells in from each side
/// and one blank row under the top line.
pub fn card(legend: Legend, cols: u16, rows: Option<u16>, content: impl IntoElement) -> View {
  view().flex_none().w(layout::cols(cols)).child(frame(
    || FrameStyle::Round,
    || Tone::Line,
    Some(legend),
    rows,
    4,
    view().flex_1().child(layout::vspace(1)).child(content),
  ))
}

/// An inset block (an address, brewing notes, a stat): an inner rounded frame (`line`), its
/// content one blank cell in from each line (`│ 收货地址 … │`).
pub fn inset(content: impl IntoElement) -> View {
  frame(
    || FrameStyle::Round,
    || Tone::Line,
    None,
    None,
    1,
    view().flex_1().child(content),
  )
}

/// A section heading inside a panel: `── 豆子档案`, bold `acc`, content right under it.
pub fn heading(title: impl Into<SharedString>) -> Text {
  strong(format!("── {}", title.into()), Tone::Acc)
}

/// The dashed rule before a total, `╌╌╌` across its parent: `line`, or `line-strong` on a
/// receipt (`strong`).
pub fn rule(strong_line: bool) -> View {
  let tone = if strong_line {
    Tone::LineStrong
  } else {
    Tone::Line
  };
  super::frame::line_run('╌', tone)
}
