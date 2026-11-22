//! The brand column beside the login card (01-login.md, "左栏"): the block wordmark, the
//! tagline, the steaming cup and the selling points at L / XL with 32 rows or more; the
//! one-row wordmark, the tagline and the points elsewhere; at S one centred row.
//!
//! ```text
//! ████   ███   ███   ████ █████     █ █████ █████ █   █        ◆ROAST/TTY
//! …                                                           一家开在终端里的精品咖啡烘焙店。
//! 一家开在终端里的精品咖啡烘焙店。                               每周二、五小批量烘焙，48 小时内
//! 每周二、五小批量烘焙，48 小时内发货。键盘飞快，鼠标也一样好用。   发货。键盘飞快，鼠标也一样好用。
//!     (   )  (                                                ✓ 8 款当季单品与意式拼配
//!   ┌────────────┐       ✓ 8 款当季单品与意式拼配               …
//! ```

use nsh_core::prelude::*;

use crate::{
  stores::Stores,
  ui::{self, Tone, layout},
};

/// `一家开在终端里的精品咖啡烘焙店。`
pub const TAGLINE: &str = "一家开在终端里的精品咖啡烘焙店。";
/// The second line of the tagline.
pub const SUBLINE: &str = "每周二、五小批量烘焙，48 小时内发货。键盘飞快，鼠标也一样好用。";
/// The selling points, each after `✓ `.
pub const POINTS: [&str; 4] = [
  "8 款当季单品与意式拼配",
  "重量 · 研磨 · 包装 自由组合",
  "风味雷达，一眼看懂酸甜醇苦",
  "购物车与地址本云端同步",
];
/// The width the subline wraps at beside the compact card (M: "按 41 列折行").
pub const COMPACT_WIDTH: u16 = 41;
/// The cells between the cup and the selling points.
const CUP_GAP: u16 = 5;

fn point(text_content: &'static str) -> Text {
  text("")
    .child(ui::label("✓ ", Tone::Acc2))
    .child(ui::label(text_content, Tone::Soft))
}

fn taglines() -> View {
  view()
    .flex_none()
    .child(ui::strong(TAGLINE, Tone::Fg))
    .child(ui::label(SUBLINE, Tone::Mute))
}

/// The tall brand block (21 rows): the block wordmark, two blank rows, the taglines, two
/// blank rows, the steam over the cup with the points level with its rows 0, 2, 4 and 6.
pub fn tall(stores: Stores) -> View {
  let settings = stores.settings;
  let points = view()
    .flex_none()
    .children(POINTS.iter().enumerate().map(|(i, p)| {
      view()
        .flex_none()
        .child((i > 0).then(|| layout::vspace(1)))
        .child(point(p))
    }));
  view()
    .flex_none()
    .child(ui::wordmark_block())
    .child(layout::vspace(2))
    .child(taglines())
    .child(layout::vspace(2))
    .child(ui::steam(move || settings.animations()))
    .child(
      view()
        .flex_none()
        .flex_row()
        .child(ui::cup())
        .child(layout::hspace(CUP_GAP))
        .child(points),
    )
}

/// The short brand block: the one-row wordmark, the taglines wrapped at
/// [`COMPACT_WIDTH`], the points; no cup, no steam.
pub fn compact() -> View {
  view()
    .flex_none()
    .w(layout::cols(COMPACT_WIDTH))
    .child(ui::wordmark_line())
    .child(layout::vspace(1))
    .child(taglines())
    .child(layout::vspace(1))
    .children(POINTS.iter().map(|p| point(p)))
}

/// The S row: `◆ROAST/TTY · 一家开在终端里的精品咖啡烘焙店。`, centred.
pub fn line() -> View {
  view()
    .flex_none()
    .flex_row()
    .justify_center()
    .child(ui::wordmark_line())
    .child(ui::label(format!(" · {TAGLINE}"), Tone::Soft))
}
