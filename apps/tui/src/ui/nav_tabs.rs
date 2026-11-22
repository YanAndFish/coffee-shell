//! NavTabs: the header's main navigation (`components/NavTabs`): `1 选购 · 2 购物车 · 3 地址本
//! · 4 我的`, each behind its digit's key cap, the cart's count as a badge.
//!
//! ```text
//!   ◆ROAST/TTY              ‹1› 选购   ‹2› 购物车 ▐3▌  ‹3› 地址本     ‹4› 我的
//! ───────────────────────━━━━━━━━━━━━━━────────────────────────────────────────────
//! ```
//!
//! Wide (XL, L): each tab is 14 cells, its content centred. Tight (M, S): tabs two cells
//! apart, the badge right after the label. The current tab is bold `fg` and underlined (the
//! design's amber bar, which [`nav_rule`] also draws on the header's bottom line); the others
//! `mute`: the state never rests on colour alone. The badge `▐3▌` is the cart's item count,
//! not drawn at 0. No sliding animation.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  kbd::{kbd, kbd_width},
  label::{label, toned},
  layout::{self, Bp},
  tag::count_badge,
  text::display_width,
  theme::Tone,
};

/// One tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavTab {
  /// Its key, `1`–`4`.
  pub key: &'static str,
  /// Its label, `选购`.
  pub label: &'static str,
}

/// The width of a wide tab.
pub const TAB_CELLS: u16 = 14;
/// The cells between two tight tabs.
pub const TIGHT_GAP: u16 = 2;

/// Whether tabs are wide at `bp` (XL, L).
pub fn wide_tabs(bp: Bp) -> bool {
  bp >= Bp::L
}

/// The cells of a tab's content: key cap, a blank, the label, and the badge (` ▐3▌` wide,
/// `▐3▌` tight) when `badge` is above 0.
pub fn tab_content_width(tab: &NavTab, badge: u32, wide: bool) -> usize {
  let badge_cells = if badge > 0 {
    badge.to_string().len() + 2 + usize::from(wide)
  } else {
    0
  };
  kbd_width(tab.key) + 1 + display_width(tab.label) + badge_cells
}

/// The column span (start, cells) of tab `index` from the first tab's column, for the
/// underline: the whole 14 cells when wide, the label's width when tight.
pub fn tab_span(tabs: &[NavTab], badges: &[u32], index: usize, wide: bool) -> (usize, usize) {
  let badge = |i: usize| badges.get(i).copied().unwrap_or(0);
  if wide {
    (index * usize::from(TAB_CELLS), usize::from(TAB_CELLS))
  } else {
    let start: usize = (0..index)
      .map(|i| tab_content_width(&tabs[i], badge(i), false) + usize::from(TIGHT_GAP))
      .sum();
    (
      start + kbd_width(tabs[index].key) + 1,
      display_width(tabs[index].label),
    )
  }
}

/// The tabs; `current()` is the index of the current one (none outside the tabs, as on the
/// login page), and a press on tab `i` runs `on_select(i)`. Tight, without badges (phase 1).
pub fn nav_tabs(
  tabs: &'static [NavTab],
  current: impl Fn() -> Option<usize> + Copy + 'static,
  on_select: impl Fn(usize) + Copy + 'static,
) -> impl IntoElement {
  nav_tabs_with(tabs, current, |_| 0, || false, on_select)
}

/// The tabs with badges (`badge(i)`, tracked; 0: none) and the wide or tight layout
/// (`wide()`, tracked: see [`wide_tabs`]).
pub fn nav_tabs_with(
  tabs: &'static [NavTab],
  current: impl Fn() -> Option<usize> + Copy + 'static,
  badge: impl Fn(usize) -> u32 + Copy + 'static,
  wide: impl Fn() -> bool + Copy + 'static,
  on_select: impl Fn(usize) + Copy + 'static,
) -> impl IntoElement {
  view()
    .flex_row()
    .flex_none()
    .items_center()
    .style_when(move || !wide(), |s| s.gap(layout::gap_cols(TIGHT_GAP)))
    .label("主导航")
    .child(tabs.iter().enumerate().map(move |(index, tab)| {
      let selected = move || current() == Some(index);
      pressable()
        .flex_row()
        .items_center()
        .style_when(wide, |s| s.w(layout::cols(TAB_CELLS)).justify_center())
        .on_press(move || on_select(index))
        .child(kbd(tab.key))
        .child(text(" "))
        .child(
          toned(
            tab.label,
            move || if selected() { Tone::Fg } else { Tone::Mute },
          )
          .style_when(selected, |s| s.font_weight(FontWeight::BOLD).underline()),
        )
        .child(dynamic(move || {
          (wide() && badge(index) > 0).then(|| text(" "))
        }))
        .child(count_badge(move || badge(index)))
    }))
}

/// The header's bottom line: `─` across, `━` (`acc`) under the current tab. `start` is the
/// column of the first tab, `badges` the counts the tabs show.
pub fn nav_rule(
  tabs: &'static [NavTab],
  current: impl Fn() -> Option<usize> + Copy + 'static,
  badges: impl Fn() -> Vec<u32> + Copy + 'static,
  start: u16,
  wide: impl Fn() -> bool + Copy + 'static,
) -> View {
  let span = move || current().map(|i| tab_span(tabs, &badges(), i, wide()));
  view()
    .flex_none()
    .w_full()
    .h(layout::rows(1))
    .overflow_hidden()
    .child(
      view().absolute().top(px(0.0)).left(px(0.0)).child(
        text("")
          .child(label(
            move || {
              let before = span().map_or(240, |(at, _)| usize::from(start) + at);
              SharedString::from("─".repeat(before))
            },
            Tone::Line,
          ))
          .child(label(
            move || SharedString::from("━".repeat(span().map_or(0, |(_, cells)| cells))),
            Tone::Acc,
          ))
          .child(label("─".repeat(240), Tone::Line))
          .number_of_lines(1)
          .text_overflow(nsh_core::EllipsizeMode::Clip),
      ),
    )
}

#[cfg(test)]
mod tests {
  use super::*;

  const TABS: [NavTab; 4] = [
    NavTab {
      key: "1",
      label: "选购",
    },
    NavTab {
      key: "2",
      label: "购物车",
    },
    NavTab {
      key: "3",
      label: "地址本",
    },
    NavTab {
      key: "4",
      label: "我的",
    },
  ];

  #[test]
  fn wide_tabs_are_14_cells_and_hold_the_badge() {
    // `‹2› 购物车 ▐3▌` fills its 14 cells.
    assert_eq!(tab_content_width(&TABS[1], 3, true), 14);
    assert_eq!(tab_content_width(&TABS[0], 3, true), 8 + 4);
    assert_eq!(tab_span(&TABS, &[0, 3, 0, 0], 2, true), (28, 14));
    assert!(wide_tabs(Bp::Xl) && wide_tabs(Bp::L) && !wide_tabs(Bp::M));
  }

  #[test]
  fn tight_tabs_underline_their_label() {
    // `‹1› 选购  ‹2› 购物车▐3▌  ‹3› 地址本`
    assert_eq!(tab_span(&TABS, &[0, 3, 0, 0], 0, false), (4, 4));
    assert_eq!(tab_span(&TABS, &[0, 3, 0, 0], 1, false), (8 + 2 + 4, 6));
    assert_eq!(
      tab_span(&TABS, &[0, 3, 0, 0], 2, false),
      (8 + 2 + 13 + 2 + 4, 6)
    );
  }
}
