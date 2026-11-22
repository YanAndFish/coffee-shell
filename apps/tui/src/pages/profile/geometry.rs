//! The profile's cell geometry per terminal size (responsive.md "06 我的", 06-profile.md
//! "区域"): pure numbers, so the layouts can be checked without a terminal.
//!
//! The shell gives the page the content box (at most 200 columns, one column in from each
//! side) and one blank row on top; the page adds the rest of the design's margins.

use crate::ui::{Bp, Viewport, layout};

/// How the account panel is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccountForm {
  /// The 6×3 avatar block beside name and key, the member meter, the three stat tiles,
  /// blank rows between (XL, L, M with room).
  Full,
  /// The `▐林▌` pill line, the key line, the meter and the stat tiles (short terminals).
  Compact,
  /// The pill line with the key, the meter, the stats in one line (S, or very short).
  Inline,
}

/// The cells of the profile page for one terminal size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Geometry {
  /// The width class.
  pub bp: Bp,
  /// One scrolling column (S) instead of two.
  pub one_column: bool,
  /// Blank columns the page adds on each side (the shell already keeps one).
  pub margin: u16,
  /// Blank rows under the content.
  pub bottom: u16,
  /// The left column's width (account, settings, logout); 0 in one column.
  pub left: u16,
  /// The columns between the two columns.
  pub gap: u16,
  /// The right column's width (taste, orders); in one column, the only column.
  pub right: u16,
  /// Blank cells between a panel's side line and its content.
  pub pad: u16,
  /// Blank rows inside the panels (above and below their content, between settings).
  pub roomy: bool,
  /// The account panel's form.
  pub account: AccountForm,
  /// The taste radar (else six flavour meters).
  pub radar: bool,
  /// Order columns: number, date, amount, status (the items take the rest).
  pub order_widths: [u16; 4],
}

/// The radar block's width, the blank before it and the blank after it.
const RADAR_COLS: u16 = 25;
/// A flavour meter row: label 4 + 1, bar 10, value 1 + 5.
const METER_COLS: u16 = 21;
/// Blank cells between the chart and the taste text.
const TASTE_GAP: u16 = 3;

impl Geometry {
  /// The geometry of a `viewport` terminal.
  pub fn at(viewport: Viewport) -> Self {
    let bp = viewport.bp();
    let short = viewport.short();
    // The shell's content box: at most 200 columns, one column in from each side.
    let content = viewport
      .cols
      .min(layout::MAX_CONTENT_COLS)
      .saturating_sub(2);
    // Rows under the header (5 rows, 3 when short) and above the status bar, less the
    // shell's blank row on top.
    let header = if short { 3 } else { 5 };
    let body_rows = viewport.rows.saturating_sub(header + 1 + 1);
    let (margin, bottom, left, gap, pad) = match bp {
      Bp::Xl => (1, 1, 47, 2, 2),
      Bp::L | Bp::M => (0, 0, 40, 1, 2),
      Bp::S | Bp::TooSmall => (0, 0, 0, 0, 1),
    };
    let one_column = left == 0;
    let right = if one_column {
      // The scroll bar takes the last column of the one scrolling column.
      content.saturating_sub(1)
    } else {
      content.saturating_sub(2 * margin + left + gap)
    };
    let body_rows = body_rows.saturating_sub(bottom);
    // The left column: account 16 + 1 + settings 9 + 1 + logout 1 in full; account 11
    // (tiles) or 7 (one line) + 1 + settings 7 + 1 + logout 1 when tight.
    let account = if one_column {
      AccountForm::Inline
    } else if !short && body_rows >= 28 {
      AccountForm::Full
    } else if body_rows >= 21 {
      AccountForm::Compact
    } else {
      AccountForm::Inline
    };
    let order_widths = match bp {
      Bp::Xl | Bp::L => [18, 7, 12, 10],
      _ => [12, 5, 8, 8],
    };
    Self {
      bp,
      one_column,
      margin,
      bottom,
      left,
      gap,
      right,
      pad,
      roomy: account == AccountForm::Full,
      account,
      radar: !one_column && !short,
      order_widths,
    }
  }

  /// The content width of a panel `width` columns wide.
  pub fn inner(&self, width: u16) -> u16 {
    width.saturating_sub(2 + 2 * self.pad)
  }

  /// The content width of the left column's panels (the only column's in one column).
  pub fn left_inner(&self) -> u16 {
    if self.one_column {
      self.inner(self.right)
    } else {
      self.inner(self.left)
    }
  }

  /// The content width of the right column's panels.
  pub fn right_inner(&self) -> u16 {
    self.inner(self.right)
  }

  /// The width of the logout button: its column.
  pub fn button(&self) -> u16 {
    if self.one_column {
      self.right
    } else {
      self.left
    }
  }

  /// The width of the order items column: what the other columns and the four gaps
  /// between them leave. (The order list's scroll bar takes the panel's padding column.)
  pub fn order_items(&self) -> u16 {
    let fixed: u16 = self.order_widths.iter().sum();
    self.right_inner().saturating_sub(fixed + 4)
  }

  /// The width of the taste text column beside the radar or the meters.
  pub fn taste_text(&self) -> u16 {
    let chart = if self.radar {
      1 + RADAR_COLS
    } else {
      METER_COLS
    };
    self.right_inner().saturating_sub(chart + TASTE_GAP)
  }

  /// The width of a taste line's label (`偏好`): 8 beside the radar, 6 beside meters.
  pub fn taste_label(&self) -> u16 {
    if self.radar { 8 } else { 6 }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn at(cols: u16, rows: u16) -> Geometry {
    Geometry::at(Viewport::new(cols, rows))
  }

  #[test]
  fn xl_is_the_design() {
    let g = at(160, 50);
    assert_eq!((g.left, g.gap, g.right), (47, 2, 107));
    // 06-profile.md: account content 5–45 (41), right content 54–154 (101).
    assert_eq!(g.left_inner(), 41);
    assert_eq!(g.right_inner(), 101);
    assert_eq!(g.account, AccountForm::Full);
    assert!(g.radar && g.roomy);
    // Number 19, date 8, items 50, amount to 143, status to 154 (the gaps inside).
    assert_eq!(g.order_items(), 50);
    // Text from column 83 (radar 55–79): 101 − 26 − 3.
    assert_eq!(g.taste_text(), 72);
  }

  #[test]
  fn l_and_m_narrow_the_left_column() {
    let l = at(130, 40);
    assert_eq!((l.left, l.right), (40, 87));
    assert_eq!(l.left_inner(), 34);
    assert_eq!(l.account, AccountForm::Full);
    assert!(l.radar);
    let m = at(100, 30);
    assert_eq!((m.left, m.right), (40, 57));
    assert_eq!(m.order_widths, [12, 5, 8, 8]);
    assert_eq!(m.account, AccountForm::Compact);
    assert!(!m.radar, "below 32 rows the radar becomes meters");
    assert_eq!(at(100, 24).account, AccountForm::Inline);
    assert_eq!(at(100, 40).account, AccountForm::Full);
  }

  #[test]
  fn s_is_one_scrolling_column() {
    let s = at(80, 24);
    assert!(s.one_column && !s.radar);
    assert_eq!(s.right, 77);
    assert_eq!(s.left_inner(), 73);
    assert_eq!(s.account, AccountForm::Inline);
    assert_eq!(s.button(), 77);
    assert_eq!(s.order_items(), 73 - 33 - 4);
  }

  #[test]
  fn wide_terminals_stop_at_200_columns() {
    assert_eq!(at(260, 60).right, at(200, 60).right);
  }
}
