//! Meter: segmented bars for a 0–10 value or a progress (`components/Meter`).
//!
//! ```text
//! 酸度 ▉▉▉▉▉▉▉▉▉▉▉▉▉▉░░░░░░  7/10        ← flavour meter (20 cells, 2 per point)
//! ▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉░░░░░░░░░░░░░░░        ← member level (64 %)
//! ⠹ 校验凭据                                  42%   ← progress block, row 1
//! ▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉▉░░░░░░░░░░░░░░░░░░░░░░░░░░        ← progress block, row 2
//! ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━         ← thin bar (free shipping)
//! ```
//!
//! Filled segments are `▉` (a 7/8 block, its own gap between segments) in the meter's
//! tone; empty ones `░` in `line-soft`. Filled cells = round(value ÷ max × length). The
//! thin bar is `━` (`acc2`) then `─` (`line-soft`). Lengths are cells, given by the caller
//! (the layout knows its column).

use nsh_core::prelude::*;

use super::{
  label::{label, toned},
  loading::spinner,
  text::{display_width, pad_start},
  theme::Tone,
};

/// The filled cells of a bar `length` long showing `value` of `max` (clamped).
pub fn filled_cells(value: f32, max: f32, length: usize) -> usize {
  if max <= 0.0 || !value.is_finite() {
    return 0;
  }
  let share = (value / max).clamp(0.0, 1.0);
  ((share * length as f32).round() as usize).min(length)
}

/// The two runs of a segmented bar: filled `▉`s and empty `░`s.
pub fn bar_runs(value: f32, max: f32, length: usize) -> (String, String) {
  let filled = filled_cells(value, max, length);
  ("▉".repeat(filled), "░".repeat(length - filled))
}

/// A segmented bar `length` cells long showing `value()` of `max`, filled in `tone`.
pub fn bar(value: impl Fn() -> f32 + Copy + 'static, max: f32, length: usize, tone: Tone) -> Text {
  text("")
    .child(label(
      move || SharedString::from(bar_runs(value(), max, length).0),
      tone,
    ))
    .child(label(
      move || SharedString::from(bar_runs(value(), max, length).1),
      Tone::LineSoft,
    ))
}

/// A flavour meter row: `酸度 ▉▉▉▉▉▉▉░░░  7/10`: the label (4 cells, `mute`), the bar
/// `length` long (20, or 10 on small or short layouts), the value right-aligned in 5 cells.
pub fn meter(
  name: impl Into<SharedString>,
  value: impl Fn() -> f32 + Copy + 'static,
  length: usize,
  tone: Tone,
) -> Text {
  let name: SharedString = name.into();
  text("")
    .child(label(format!("{} ", pad_label(&name)), Tone::Mute))
    .child(bar(value, 10.0, length, tone))
    .child(label(
      move || {
        SharedString::from(format!(
          " {}",
          pad_start(&format!("{}/10", value().round()), 5)
        ))
      },
      Tone::Soft,
    ))
}

fn pad_label(name: &str) -> String {
  super::text::pad_end(name, 4.max(display_width(name)))
}

/// The thin bar: `━` (`acc2`) for `value` of `max`, then `─` (`line-soft`), `length` cells.
pub fn thin_bar(value: impl Fn() -> f32 + Copy + 'static, max: f32, length: usize) -> Text {
  text("")
    .child(label(
      move || SharedString::from("━".repeat(filled_cells(value(), max, length))),
      Tone::Acc2,
    ))
    .child(label(
      move || SharedString::from("─".repeat(length - filled_cells(value(), max, length))),
      Tone::LineSoft,
    ))
}

/// The progress block that replaces a submitting button (two rows, `length` cells wide): the
/// spinner and the step on the left, the percentage right (`mute`); then the bar in `tone`
/// (`Acc` for login, `Acc2` for checkout). `animate()` turns the spinner (off: `⠋`).
pub fn progress(
  step: impl IntoReactive<SharedString>,
  percent: impl Fn() -> f32 + Copy + 'static,
  length: usize,
  tone: Tone,
  animate: impl Fn() -> bool + 'static,
) -> View {
  view()
    .flex_none()
    .label("进度")
    .child(
      view()
        .flex_row()
        .child(spinner(animate))
        // A blank cell, not a space of text: nsh clears braille next to a text's box.
        .child(super::layout::hspace(1))
        .child(label(step, Tone::Soft))
        .child(view().flex_1())
        .child(toned(
          move || SharedString::from(format!("{}%", percent().round())),
          || Tone::Mute,
        )),
    )
    .child(bar(percent, 100.0, length, tone))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cells_round_the_share() {
    assert_eq!(filled_cells(7.0, 10.0, 20), 14);
    assert_eq!(filled_cells(7.0, 10.0, 10), 7);
    assert_eq!(filled_cells(64.0, 100.0, 41), 26);
    assert_eq!(filled_cells(42.0, 100.0, 45), 19);
    assert_eq!(filled_cells(12.0, 10.0, 10), 10, "clamped");
    assert_eq!(filled_cells(-1.0, 10.0, 10), 0);
    assert_eq!(filled_cells(5.0, 0.0, 10), 0);
    assert_eq!(filled_cells(f32::NAN, 10.0, 10), 0);
  }

  #[test]
  fn runs_fill_the_whole_length() {
    let (on, off) = bar_runs(7.0, 10.0, 20);
    assert_eq!(on.chars().count() + off.chars().count(), 20);
    assert_eq!(display_width(&format!("{on}{off}")), 20);
    assert_eq!(
      format!("酸度 {on}{off} {}", pad_start("7/10", 5)),
      "酸度 ▉▉▉▉▉▉▉▉▉▉▉▉▉▉░░░░░░  7/10"
    );
  }
}
