//! Loading: the braille spinner and the loading line (`components/Meter`, "加载状态写法"):
//! `⠹ 正在加载商品…`.
//!
//! The spinner turns through `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, a frame every 100 ms, only while the caller's
//! `animate()` holds (the account's "界面动画" setting); otherwise it rests on `⠋`. The
//! [`loading`] line of phase 1 keeps its resting `⠹` and never animates, so a headless test
//! still goes quiet.

use std::time::Duration;

use nsh_core::prelude::*;

use super::{
  label::{label, strong},
  theme::Tone,
};

/// The spinner's frames, in order.
pub const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A frame of the spinner every this long.
pub const SPIN_PERIOD: Duration = Duration::from_millis(100);

/// The resting glyph of the phase-1 loading line.
pub const SPINNER: &str = "⠹";

/// The spinner's frame after `ticks` periods.
pub fn spin_frame(ticks: u64) -> &'static str {
  SPIN[(ticks % SPIN.len() as u64) as usize]
}

/// The spinner glyph (bold `acc`): turning while `animate()` holds, else `⠋`.
pub fn spinner(animate: impl Fn() -> bool + 'static) -> Text {
  let ticks = signal(0u64);
  let animate = computed(animate);
  effect(move || {
    if animate.get() {
      let timer = set_interval(SPIN_PERIOD, move || {
        ticks.update(|t| *t = t.wrapping_add(1))
      });
      cleanup(move || drop(timer));
    }
  });
  strong(
    move || {
      SharedString::from(if animate.get() {
        spin_frame(ticks.get())
      } else {
        SPIN[0]
      })
    },
    Tone::Acc,
  )
}

/// A loading line saying `message`, centred in its parent.
pub fn loading(message: impl IntoReactive<SharedString>) -> impl IntoElement {
  view().flex_1().items_center().justify_center().child(
    view()
      .flex_row()
      .gap_2()
      .label("加载中")
      .child(strong(SPINNER, Tone::Acc))
      .child(label(message, Tone::Soft)),
  )
}

/// A loading line with a live spinner: `⠹ 正在载入订单…`, centred in its parent.
pub fn loading_live(
  message: impl IntoReactive<SharedString>,
  animate: impl Fn() -> bool + 'static,
) -> View {
  view().flex_1().items_center().justify_center().child(
    view()
      .flex_row()
      .gap_2()
      .label("加载中")
      .child(spinner(animate))
      .child(label(message, Tone::Soft)),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_spinner_cycles_through_ten_frames() {
    assert_eq!(spin_frame(0), "⠋");
    assert_eq!(spin_frame(2), "⠹");
    assert_eq!(spin_frame(9), "⠏");
    assert_eq!(spin_frame(10), "⠋");
    assert_eq!(SPIN_PERIOD.as_millis(), 100);
  }
}
