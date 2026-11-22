//! Window: the whole-screen frame (`components/Window`). The terminal is the window, so there
//! is no outer frame (D2): a system row on top, the header, the content with its margins, and
//! the status bar at the bottom. This module draws the system row and gives the margins.
//!
//! ```text
//!  roast@tty: ~/shop — 160×50        ! 演示环境 · 不是真实商店        ● 公钥已识别 · 21:04
//! ```
//!
//! The system row (row 0, `surface`): left `roast@tty: ~/shop — {cols}×{rows}` (`mute`, live,
//! left out under 100 columns); centred the demo notice `! 演示环境 · 不是真实商店` (bold
//! `acc`; bold carries it in monochrome); right `● 公钥已识别` (`acc2`) or `○ 未提供公钥`
//! (`mute`) and ` · HH:MM` (`mute`), one cell from each edge. It stays visible over dialogs.

use nsh_core::prelude::*;

use super::{
  label::{label, strong},
  layout::{self, Bp, Layout, Viewport},
  theme::{Fill, Tone},
};

/// The demo notice of the system row.
pub const DEMO_NOTICE: &str = "! 演示环境 · 不是真实商店";

/// The system row's left text for `viewport`: `roast@tty: ~/shop — 160×50`.
pub fn title_text(viewport: Viewport) -> String {
  format!("roast@tty: ~/shop — {}×{}", viewport.cols, viewport.rows)
}

/// The content's margins at `bp`: (columns each side, blank rows above and below).
pub fn content_margins(bp: Bp) -> (u16, u16) {
  match bp {
    Bp::Xl => (2, 1),
    Bp::L | Bp::M => (1, 1),
    Bp::S | Bp::TooSmall => (0, 0),
  }
}

/// The system row. `has_key()`: the SSH client offered a public key; `clock()`: the server's
/// time, `HH:MM`.
pub fn system_row(
  has_key: impl Fn() -> bool + Copy + 'static,
  clock: impl Fn() -> SharedString + 'static,
) -> View {
  let layout = Layout::get();
  view()
    .flex_none()
    .flex_row()
    .h(layout::rows(1))
    .px(layout::cols(1))
    .bg(Fill::Surface.color())
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .basis(px(0.0))
        .child(dynamic(move || {
          let viewport = layout.viewport();
          (viewport.cols >= 100).then(|| label(title_text(viewport), Tone::Mute))
        })),
    )
    .child(strong(DEMO_NOTICE, Tone::Acc).flex_none())
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .basis(px(0.0))
        .flex_row()
        .justify_end()
        .child(dynamic(move || {
          if has_key() {
            label("● 公钥已识别", Tone::Acc2)
          } else {
            label("○ 未提供公钥", Tone::Mute)
          }
        }))
        .child(label(
          move || SharedString::from(format!(" · {}", clock())),
          Tone::Mute,
        )),
    )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn the_row_names_the_size() {
    assert_eq!(
      title_text(Viewport::new(160, 50)),
      "roast@tty: ~/shop — 160×50"
    );
    assert_eq!(display_width(DEMO_NOTICE), 25);
    assert_eq!(content_margins(Bp::Xl), (2, 1));
    assert_eq!(content_margins(Bp::S), (0, 0));
  }
}
