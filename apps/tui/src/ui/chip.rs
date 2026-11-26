//! Chip: one option of a mutually exclusive set (`components/Chip`). One row, options one
//! cell apart.
//!
//! ```text
//! ▐全部▌ [浅] [中] [深]                      ← roast (segmented)
//! ▐  推荐   ▌ [ 价格 ↑  ]                     ← sort (2×2 grid, 11 cells each)
//! [ 100g ] ▐ 250g ▌ [ 500g ] [ 1kg  ]         ← SKU weight, a note under each
//!   ¥53      ¥118     ¥224     缺货
//! 标签  ▐ 家 ▌ [ 公司 ] [ 学校 ]              ← pick (one cell of padding)
//! ```
//!
//! Unselected: `[text]`, brackets `line`, text `soft`. Selected: the solid amber pill
//! `▐text▌` (`ink` bold; reverse video in monochrome). An unavailable SKU option is
//! `disabled` and struck through, its note says `缺货`, and presses skip it. The source's
//! `.t-filter` list is [`super::toggle::radio`].
//!
//! A chip's box is exactly [`chip_width`] cells: nsh (0.6.1, still 0.7.1) lays out the bare row of its
//! pieces one cell wider than it draws, which doubled the one-cell gap between options.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  label::{label, toned},
  layout,
  tag::pill_text,
  text::{center, display_width},
  theme::{Fill, Tone},
};

/// How a chip's text sits inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChipPad {
  /// `[text]`: segmented controls.
  Tight,
  /// `[ text ]`: picks and SKU options.
  Padded,
  /// Stretched to this many cells, the text centred (sort grid: 11; SKU weights: 8).
  Width(u16),
}

/// The inner text of a chip (between its ends) for `pad`.
pub fn chip_inner(text: &str, pad: ChipPad) -> String {
  match pad {
    ChipPad::Tight => text.to_owned(),
    ChipPad::Padded => format!(" {text} "),
    ChipPad::Width(cells) => center(text, usize::from(cells).saturating_sub(2)),
  }
}

/// The cells a chip takes.
pub fn chip_width(text: &str, pad: ChipPad) -> usize {
  display_width(&chip_inner(text, pad)) + 2
}

/// One chip saying `text`, selected while `selected()`; `available()` false makes it a
/// struck-through `disabled` option that presses skip. A press runs `on_press`.
pub fn chip(
  text_content: impl Into<SharedString>,
  pad: ChipPad,
  selected: impl Fn() -> bool + Copy + 'static,
  available: impl Fn() -> bool + Copy + 'static,
  on_press: impl Fn() + 'static,
) -> View {
  let text_content: SharedString = text_content.into();
  let shown = chip_inner(&text_content, pad);
  let width = chip_width(&text_content, pad) as u16;
  let chip = pressable()
    .flex_none()
    .label(text_content)
    .on_press(move || {
      if available() {
        on_press();
      }
    })
    .child(dynamic(move || {
      if selected() && available() {
        pill_text(Fill::Acc, shown.clone(), false).into_element()
      } else {
        let text_tone = move || {
          if available() {
            Tone::Soft
          } else {
            Tone::Disabled
          }
        };
        view()
          .flex_row()
          .child(label("[", Tone::Line))
          .child(
            toned(shown.clone(), text_tone).style_when(move || !available(), |s| s.line_through()),
          )
          .child(label("]", Tone::Line))
          .into_element()
      }
    }));
  // The pressable is laid out a cell wider than it draws: the box around it holds the
  // width the chip draws.
  view()
    .flex_none()
    .w(layout::cols(width))
    .overflow_hidden()
    .child(chip)
}

/// A segmented row of `options`, the `selected()` one solid; a press on option `i` runs
/// `on_select(i)`.
pub fn segmented(
  options: &[&'static str],
  pad: ChipPad,
  selected: impl Fn() -> usize + Copy + 'static,
  on_select: impl Fn(usize) + Copy + 'static,
) -> View {
  view()
    .flex_none()
    .flex_row()
    .gap(layout::gap_cols(1))
    .children(options.iter().enumerate().map(move |(index, option)| {
      chip(
        *option,
        pad,
        move || selected() == index,
        || true,
        move || on_select(index),
      )
    }))
}

/// One SKU option: an 8-cell chip and its note centred under it (`¥118`, `acc` when
/// selected, else `mute`; `缺货` when unavailable).
pub fn sku_option(
  text_content: impl Into<SharedString>,
  note: impl Into<SharedString>,
  selected: impl Fn() -> bool + Copy + 'static,
  available: bool,
  on_press: impl Fn() + 'static,
) -> View {
  let note: SharedString = if available {
    note.into()
  } else {
    SharedString::from("缺货")
  };
  let width = 8;
  view()
    .flex_none()
    .w(layout::cols(width))
    .child(chip(
      text_content,
      ChipPad::Width(width),
      selected,
      move || available,
      on_press,
    ))
    .child(toned(center(&note, usize::from(width)), move || {
      if !available {
        Tone::Disabled
      } else if selected() {
        Tone::Acc
      } else {
        Tone::Mute
      }
    }))
}

/// A SKU group's name row: `› 重量` with `← → 切换` at its end while the keyboard is on the
/// group (`focused()`), else the name in `mute`.
pub fn group_label(
  name: impl Into<SharedString>,
  hint: impl Into<SharedString>,
  focused: impl Fn() -> bool + Copy + 'static,
) -> View {
  let name: SharedString = name.into();
  let hint: SharedString = hint.into();
  view()
    .flex_none()
    .flex_row()
    .child(toned(
      move || if focused() { "› " } else { "  " },
      || Tone::Acc,
    ))
    .child(
      toned(name, move || if focused() { Tone::Fg } else { Tone::Mute })
        .style_when(focused, |s| s.font_weight(FontWeight::BOLD)),
    )
    .child(view().flex_1())
    .child(when(focused, move || label(hint.clone(), Tone::Mute)))
}

/// A section head of the filter column: its name (`mute`) with its key cap at the end.
pub fn section_head(name: impl Into<SharedString>, key: impl Into<SharedString>) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(label(name.into(), Tone::Mute))
    .child(view().flex_1())
    .child(super::kbd(key.into()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn chips_take_the_design_widths() {
    assert_eq!(chip_width("浅", ChipPad::Tight), 4);
    assert_eq!(chip_width("家", ChipPad::Padded), 6);
    assert_eq!(chip_width("推荐", ChipPad::Width(11)), 11);
    assert_eq!(chip_inner("推荐", ChipPad::Width(11)), "  推荐   ");
    assert_eq!(chip_inner("250g", ChipPad::Width(8)), " 250g ");
    assert_eq!(chip_inner("1kg", ChipPad::Width(8)), " 1kg  ");
    assert_eq!(chip_width("价格 ↑", ChipPad::Width(11)), 11);
  }
}
