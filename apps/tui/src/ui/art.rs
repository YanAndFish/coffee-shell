//! The design's character art: the login cup and its three steam frames, the empty cup of
//! an empty cart, and `∅` for "no results" (`raw/canvas/Main.dc.html` `CUP`, `STEAM`).
//! Decorative only: screen readers skip it.
//!
//! ```text
//!     (   )  (              ← steam (14×3), three frames, `dim` / `steam` / `mute` wisps
//!      )  (   )
//!     (   )  (
//!   ┌────────────┐            ← cup (18×7), `soft`; the `▓` row is the coffee
//!   │▓▓▓▓▓▓▓▓▓▓▓▓│─╮
//!   │            │ │
//!   │   ROAST    │─╯
//!   │      /TTY  │
//!   └────────────┘
//!  ════════════════
//! ```

use std::time::Duration;

use nsh_core::prelude::*;

use super::{label::label, theme::Tone};

/// The steam's frames (three rows each, 14 cells).
pub const STEAM: [[&str; 3]; 3] = [
  ["    (   )  (  ", "     )  (   ) ", "    (   )  (  "],
  ["     )  (   ) ", "    (   )  (  ", "     )  (   ) "],
  ["   (   )   (  ", "    )   (   ) ", "   (   )  (   "],
];

/// The tones of the three wisps, top to bottom (the design's `dim`, `steam`, `mute`).
pub const STEAM_TONES: [Tone; 3] = [Tone::Dim, Tone::Disabled, Tone::Mute];

/// A steam frame every this long.
pub const STEAM_PERIOD: Duration = Duration::from_millis(400);

/// The login cup (18×7).
pub const CUP: [&str; 7] = [
  "  ┌────────────┐  ",
  "  │▓▓▓▓▓▓▓▓▓▓▓▓│─╮",
  "  │            │ │",
  "  │   ROAST    │─╯",
  "  │      /TTY  │  ",
  "  └────────────┘  ",
  " ════════════════ ",
];

/// The empty cup of an empty cart (18×5).
pub const EMPTY_CUP: [&str; 5] = [
  "  ┌────────────┐  ",
  "  │            │─╮",
  "  │   空  杯   │ │",
  "  │            │─╯",
  "  └────────────┘  ",
];

/// The "no results" glyph.
pub const EMPTY_SET: &str = "∅";

/// Rows of art in `tone`, hidden from screen readers.
pub fn art(rows: &[&'static str], tone: Tone) -> View {
  view()
    .flex_none()
    .accessibility_elements_hidden(true)
    .children(rows.iter().map(move |row| label(*row, tone)))
}

/// The steam over the cup: the next of its three frames every [`STEAM_PERIOD`] while
/// `animate()` holds (the login page, 32 rows or more, animations on); else the first frame.
pub fn steam(animate: impl Fn() -> bool + 'static) -> View {
  let frame = signal(0usize);
  let animate = computed(animate);
  effect(move || {
    if animate.get() {
      let timer = set_interval(STEAM_PERIOD, move || frame.update(|f| *f = (*f + 1) % 3));
      cleanup(move || drop(timer));
    } else {
      frame.set(0);
    }
  });
  view()
    .flex_none()
    .accessibility_elements_hidden(true)
    .children((0..3).map(move |row| {
      label(
        move || SharedString::from(STEAM[frame.get()][row]),
        STEAM_TONES[row],
      )
    }))
}

/// The login cup.
pub fn cup() -> View {
  art(&CUP, Tone::Soft)
}

/// The empty cup.
pub fn empty_cup() -> View {
  art(&EMPTY_CUP, Tone::Dim)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn art_rows_are_even() {
    for frame in STEAM {
      for row in frame {
        assert_eq!(display_width(row), 14, "{row:?}");
      }
    }
    for row in CUP {
      assert_eq!(display_width(row), 18, "{row:?}");
    }
    for row in EMPTY_CUP {
      assert_eq!(display_width(row), 18, "{row:?}");
    }
  }
}
