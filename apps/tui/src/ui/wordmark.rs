//! Wordmark: the brand (`components/Wordmark`), in three forms.
//!
//! ```text
//! ████   ███   ███   ████ █████     █ █████ █████ █   █      ← block (53×5): the login page
//! █   █ █   █ █   █ █       █      █    █     █    █ █
//! ████  █   █ █████  ███    █     █     █     █     █
//! █  █  █   █ █   █     █   █    █      █     █     █
//! █   █  ███  █   █ ████    █   █       █     █     █
//!
//! ◆ROAST/TTY                                                  ← one row (10×1)
//! ```
//!
//! The block's first 30 columns ("ROAST ") are bold `acc`, the last 23 ("/TTY") bold `fg`;
//! letters are 5×5, a column apart (the source's `GLYPH` / `LOGO`). The one-row form is `◆`
//! `acc`, `/` `mute`, the rest bold `fg`: in the header it is the button home ([`wordmark`],
//! the `1` key's command), elsewhere plain ([`wordmark_line`]); short layouts (under 32 rows)
//! use it in place of the block ([`wordmark_for`]).

use nsh_core::prelude::*;

use super::{
  action::pressable,
  label::{label, strong},
  theme::Tone,
};

/// The 5×5 letters of the block wordmark.
fn glyph(letter: char) -> [&'static str; 5] {
  match letter {
    'R' => ["████ ", "█   █", "████ ", "█  █ ", "█   █"],
    'O' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
    'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
    'S' => [" ████", "█    ", " ███ ", "    █", "████ "],
    'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
    '/' => ["    █", "   █ ", "  █  ", " █   ", "█    "],
    'Y' => ["█   █", " █ █ ", "  █  ", "  █  ", "  █  "],
    _ => ["     "; 5],
  }
}

/// The block wordmark's rows: each the "ROAST " part (30 cells) and the "/TTY" part (23).
pub fn block_rows() -> [(String, String); 5] {
  std::array::from_fn(|row| {
    let join = |word: &str| {
      word
        .chars()
        .map(|c| glyph(c)[row])
        .collect::<Vec<_>>()
        .join(" ")
    };
    (format!("{} ", join("ROAST")), join("/TTY"))
  })
}

/// The block wordmark (53×5).
pub fn wordmark_block() -> View {
  view()
    .flex_none()
    .label("ROAST/TTY")
    .children(block_rows().into_iter().map(|(roast, tty)| {
      text("")
        .child(strong(roast, Tone::Acc))
        .child(strong(tty, Tone::Fg))
    }))
}

/// The one-row wordmark `◆ROAST/TTY` (10 cells), not pressable.
pub fn wordmark_line() -> Text {
  text("")
    .child(label("◆", Tone::Acc))
    .child(strong("ROAST", Tone::Fg))
    .child(label("/", Tone::Mute))
    .child(strong("TTY", Tone::Fg))
}

/// The header wordmark; a press runs `on_press` (home).
pub fn wordmark(on_press: impl Fn() + 'static) -> impl IntoElement {
  pressable()
    .flex_none()
    .label("ROAST/TTY 首页")
    .on_press(on_press)
    .child(wordmark_line())
}

/// The login page's wordmark: the block, or the one-row form when `short` (under 32 rows).
pub fn wordmark_for(short: bool) -> AnyElement {
  if short {
    view().flex_none().child(wordmark_line()).into_element()
  } else {
    wordmark_block().into_element()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn the_block_is_53_by_5_in_two_parts() {
    let rows = block_rows();
    for (roast, tty) in &rows {
      assert_eq!(display_width(roast), 30);
      assert_eq!(display_width(tty), 23);
    }
    assert_eq!(
      format!("{}{}", rows[0].0, rows[0].1),
      "████   ███   ███   ████ █████     █ █████ █████ █   █"
    );
    assert_eq!(
      format!("{}{}", rows[4].0, rows[4].1).trim_end(),
      "█   █  ███  █   █ ████    █   █       █     █     █"
    );
    assert_eq!(display_width("◆ROAST/TTY"), 10);
  }
}
