//! NavTabs: the header's main navigation (`components/NavTabs`): `1 选购  2 购物车  3 地址本
//! 4 我的`, each behind its digit's key cap. The current tab is bold and underlined (the
//! design's 2px amber bar), the others `mute`: the state never rests on colour alone.

use nsh_core::{element::AccessibilityRole, prelude::*};

use super::{kbd, theme};

/// One tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavTab {
  /// Its key, `1`–`4`.
  pub key: &'static str,
  /// Its label, `选购`.
  pub label: &'static str,
}

/// The tabs; `current()` is the index of the current one (none on pages outside the tabs,
/// such as the login page), and a press on tab `i` runs `on_select(i)`.
pub fn nav_tabs(
  tabs: &'static [NavTab],
  current: impl Fn() -> Option<usize> + Copy + 'static,
  on_select: impl Fn(usize) + Copy + 'static,
) -> impl IntoElement {
  view()
    .flex_row()
    .flex_none()
    .items_center()
    .gap_4()
    .label("主导航")
    .child(tabs.iter().enumerate().map(move |(index, tab)| {
      let selected = move || current() == Some(index);
      view()
        .role(AccessibilityRole::Button)
        .focusable(false)
        .flex_row()
        .items_center()
        .gap_2()
        .on_press(move || on_select(index))
        .child(kbd(tab.key))
        .child(
          text(tab.label)
            .text_color(move || {
              if selected() {
                theme::fg()
              } else {
                theme::mute()
              }
            })
            .style_when(selected, |s| s.font_weight(FontWeight::BOLD).underline()),
        )
    }))
}
