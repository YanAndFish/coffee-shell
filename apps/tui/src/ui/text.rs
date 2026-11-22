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

/// `text` centred in `width` cells (the odd cell goes right); cut with `…` when wider.
pub fn center(text: &str, width: usize) -> String {
  let text = truncate(text, width);
  let spare = width.saturating_sub(display_width(&text));
  let left = spare / 2;
  let mut out: String = std::iter::repeat_n(' ', left).collect();
  out.push_str(&text);
  out.extend(std::iter::repeat_n(' ', spare - left));
  out
}

/// `text` cut to `width` cells (with `…`) and padded to exactly `width`.
pub fn fit(text: &str, width: usize) -> String {
  pad_end(&truncate(text, width), width)
}

/// `glyph` repeated to fill `width` cells (a wide glyph never overflows: the rest is spaces).
pub fn repeat(glyph: char, width: usize) -> String {
  let w = glyph.width().unwrap_or(1).max(1);
  let mut out: String = std::iter::repeat_n(glyph, width / w).collect();
  out.extend(std::iter::repeat_n(' ', width % w));
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn centring_and_fitting_count_cells() {
    assert_eq!(center("登录", 8), "  登录  ");
    assert_eq!(center("ok", 5), " ok  ");
    assert_eq!(display_width(&center("退出登录 LOGOUT", 9)), 9);
    assert_eq!(fit("耶加雪菲 孔加", 6), "耶加… ");
    assert_eq!(display_width(&fit("耶加雪菲 孔加", 6)), 6);
    assert_eq!(repeat('━', 4), "━━━━");
    assert_eq!(repeat('空', 5), "空空 ");
  }

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
