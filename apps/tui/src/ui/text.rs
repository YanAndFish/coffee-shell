//! Text measured in terminal cells: CJK and other wide characters take two cells, so a
//! string's width is never its `len()` (bytes) nor its `chars().count()`.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The cells `text` takes on one line.
pub fn display_width(text: &str) -> usize {
  UnicodeWidthStr::width(text)
}

/// `text` cut to at most `max` cells, ending in `…` (one cell) when it was cut. Wide
/// characters are never split.
pub fn truncate(text: &str, max: usize) -> String {
  if display_width(text) <= max {
    return text.to_owned();
  }
  if max == 0 {
    return String::new();
  }
  let mut out = String::new();
  let mut used = 0;
  for c in text.chars() {
    let w = c.width().unwrap_or(0);
    if used + w > max - 1 {
      break;
    }
    out.push(c);
    used += w;
  }
  out.push('…');
  out
}

/// `text` padded with spaces to `width` cells (unchanged when it is wider).
pub fn pad_end(text: &str, width: usize) -> String {
  let have = display_width(text);
  let mut out = text.to_owned();
  out.extend(std::iter::repeat_n(' ', width.saturating_sub(have)));
  out
}

/// `text` padded with spaces on the left to `width` cells (unchanged when it is wider).
pub fn pad_start(text: &str, width: usize) -> String {
  let have = display_width(text);
  let mut out: String = std::iter::repeat_n(' ', width.saturating_sub(have)).collect();
  out.push_str(text);
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cjk_takes_two_cells() {
    assert_eq!(display_width("耶加雪菲 孔加"), 13);
    assert_eq!(display_width("◆ROAST/TTY"), 10);
    assert_eq!(display_width("演示环境 · 不是真实商店"), 23);
    assert_eq!(display_width("¥98"), 3);
    assert_eq!(display_width("●●○○○"), 5);
    assert_eq!(display_width("终端太小"), 8);
  }

  #[test]
  fn truncation_keeps_wide_characters_whole() {
    assert_eq!(truncate("耶加雪菲 孔加", 13), "耶加雪菲 孔加");
    assert_eq!(truncate("耶加雪菲 孔加", 8), "耶加雪…");
    // 7 cells: three wide characters would leave no room for the ellipsis.
    assert_eq!(truncate("耶加雪菲", 6), "耶加…");
    assert_eq!(truncate("耶加雪菲", 7), "耶加雪…");
    assert_eq!(truncate("abc", 0), "");
    assert_eq!(truncate("abcdef", 4), "abc…");
    assert!(display_width(&truncate("「夜班」意式拼配", 9)) <= 9);
  }

  #[test]
  fn padding_counts_cells() {
    assert_eq!(pad_end("选购", 6), "选购  ");
    assert_eq!(pad_start("¥98", 5), "  ¥98");
    assert_eq!(pad_end("太长的文字", 4), "太长的文字");
    assert_eq!(display_width(&pad_end("埃塞俄比亚", 12)), 12);
  }
}
