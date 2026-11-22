//! Leader: key ┈┈┈ value, for the bean profile and the order receipt (`components/Leader`).
//!
//! ```text
//! 产区 ┈┈┈┈┈┈┈┈┈┈ 慧兰 · 皮塔利托
//! 慧兰 粉红波旁 250g · 手冲 · 标准袋 × 1 ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ ¥118.00
//! ```
//!
//! One row `width` cells wide: the key (`mute`; on a receipt the name bold `fg` and the spec
//! `mute`), a blank, at least one `┈` (`line-strong`), a blank, the value (`fg`). When it does
//! not fit, the key is cut (`…`), never the value.

use nsh_core::prelude::*;

use super::{
  label::{label, strong},
  text::{display_width, truncate},
  theme::Tone,
};

/// The parts of a leader row `width` cells wide: the key (cut to fit), the dots, the value.
pub fn leader_parts(key: &str, value: &str, width: usize) -> (String, String) {
  let value_cells = display_width(value);
  // key + " " + at least one dot + " " + value
  let room = width.saturating_sub(value_cells + 3);
  let key = truncate(key, room);
  let dots = width
    .saturating_sub(display_width(&key) + value_cells + 2)
    .max(1);
  (key, "┈".repeat(dots))
}

/// A leader row: `key` ┈┈┈ `value`, `width` cells.
pub fn leader(key: &str, value: &str, width: usize) -> Text {
  let (key, dots) = leader_parts(key, value, width);
  text("")
    .child(label(format!("{key} "), Tone::Mute))
    .child(label(dots, Tone::LineStrong))
    .child(label(format!(" {value}"), Tone::Fg))
}

/// A receipt line: the item `name` (bold `fg`) and its `spec` (`mute`) as the key, the
/// amount as the value, `width` cells.
pub fn receipt_leader(name: &str, spec: &str, amount: &str, width: usize) -> Text {
  let whole = format!("{name} {spec}");
  let (key, dots) = leader_parts(&whole, amount, width);
  // The name keeps its weight as far as it survived the cut.
  let name_cells = display_width(name).min(display_width(&key));
  let (name_part, spec_part) = split_at_cells(&key, name_cells);
  text("")
    .child(strong(name_part, Tone::Fg))
    .child(label(format!("{spec_part} "), Tone::Mute))
    .child(label(dots, Tone::LineStrong))
    .child(strong(format!(" {amount}"), Tone::Fg))
}

/// `text` split after `cells` cells.
fn split_at_cells(text: &str, cells: usize) -> (String, String) {
  let mut used = 0;
  for (at, c) in text.char_indices() {
    let w = display_width(c.encode_utf8(&mut [0; 4]));
    if used + w > cells {
      return (text[..at].to_owned(), text[at..].to_owned());
    }
    used += w;
  }
  (text.to_owned(), String::new())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_dots_fill_the_width() {
    let (key, dots) = leader_parts("产区", "慧兰 · 皮塔利托", 31);
    assert_eq!(key, "产区");
    assert_eq!(display_width(&format!("{key} {dots} 慧兰 · 皮塔利托")), 31);
    assert!(dots.chars().all(|c| c == '┈'));
  }

  #[test]
  fn a_long_key_is_cut_never_the_value() {
    let (key, dots) = leader_parts("慧兰 粉红波旁 250g · 手冲 · 标准袋 × 1", "¥118.00", 20);
    assert!(key.ends_with('…'), "{key}");
    assert_eq!(dots, "┈");
    assert_eq!(display_width(&format!("{key} {dots} ¥118.00")), 20);
    assert_eq!(
      split_at_cells("慧兰 粉红", 4),
      ("慧兰".to_owned(), " 粉红".to_owned())
    );
  }
}
