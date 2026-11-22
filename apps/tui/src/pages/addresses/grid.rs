//! The card grid's geometry (05-address.md, responsive.md "05 地址本"): how many columns,
//! how wide and tall each card is, and which rows of the grid show when they do not all fit
//! (the cursor's row is always among them). Pure functions, so the sizes are tested here.

use std::ops::Range;

use crate::ui::{Bp, Button};

/// A full card's height (XL; L too, see [`shape`]).
pub const FULL_ROWS: u16 = 11;
/// A compact card under the cursor (S, M): head, `地区 · 详细地址`, actions.
pub const CURRENT_ROWS: u16 = 5;
/// Any other compact card: head and `地区 · 详细地址`.
pub const OTHER_ROWS: u16 = 4;
/// The compact add card: `+ 新增收货地址` on one row.
pub const ADD_ROWS: u16 = 3;
/// The widest card (XL: 77×11).
pub const MAX_CARD_COLS: u16 = 77;
/// Blank columns between the two columns of cards.
pub const COLUMN_GAP: u16 = 2;
/// The cells left of a card's actions and right of them: the side line, two blank cells and
/// the three-cell indent on the left; two blank cells and the side line on the right.
pub const ACTION_INSET: u16 = 9;

/// How the grid is laid out at a width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
  /// One column (S, M) or two (L, XL).
  pub columns: usize,
  /// Each card's width.
  pub card_cols: u16,
}

impl Shape {
  /// Two columns of full cards.
  pub fn is_grid(self) -> bool {
    self.columns == 2
  }
}

/// The grid at width class `bp` in `avail` columns: two columns of full cards from L
/// (`(avail − 2) ÷ 2` each, at most 77), one column of compact cards as wide as the
/// content at S and M.
///
/// The spec draws L's cards 9 rows tall; `ui::address_card` lays a card out in full from 11
/// rows only, so L keeps the 11-row card (it fits in 40 rows).
pub fn shape(bp: Bp, avail: u16) -> Shape {
  if bp >= Bp::L {
    Shape {
      columns: 2,
      card_cols: (avail.saturating_sub(COLUMN_GAP) / 2).min(MAX_CARD_COLS),
    }
  } else {
    Shape {
      columns: 1,
      card_cols: avail,
    }
  }
}

/// The height of a cell: a card (`current` or not) or the add card (`add`).
pub fn cell_rows(shape: Shape, current: bool, add: bool) -> u16 {
  match (shape.is_grid(), add, current) {
    (true, _, _) => FULL_ROWS,
    (false, true, _) => ADD_ROWS,
    (false, false, true) => CURRENT_ROWS,
    (false, false, false) => OTHER_ROWS,
  }
}

/// Blank rows between two rows of the grid.
pub fn row_gap(shape: Shape) -> u16 {
  u16::from(shape.is_grid())
}

/// The rows of the grid to draw: those from the previous first row `top` (moved as little
/// as possible) that fit in `avail` rows, always including `cursor`'s row. `heights` are the
/// rows' heights, `gap` the blank rows between two. `avail` 0 (not measured yet) shows all.
pub fn window(heights: &[u16], gap: u16, cursor: usize, avail: u16, top: usize) -> Range<usize> {
  let count = heights.len();
  if count == 0 {
    return 0..0;
  }
  if avail == 0 {
    return 0..count;
  }
  let cursor = cursor.min(count - 1);
  let span = |range: Range<usize>| -> u32 {
    let rows: u32 = heights[range.clone()].iter().map(|h| u32::from(*h)).sum();
    rows + u32::from(gap) * (range.len().saturating_sub(1) as u32)
  };
  let avail = u32::from(avail);
  // Scroll up to the cursor, or down until the cursor's row fits.
  let mut top = top.min(cursor);
  while top < cursor && span(top..cursor + 1) > avail {
    top += 1;
  }
  // Pull rows above back in while the rest leaves room (after a delete).
  while top > 0 && span(top - 1..count) <= avail {
    top -= 1;
  }
  let mut end = cursor + 1;
  while end < count && span(top..end + 1) <= avail {
    end += 1;
  }
  top..end
}

/// The labels of a card's four actions for a card `cols` wide: `space 设为默认` when the
/// four fit, else the footer's shorter `space 默认` (L's narrower cards).
pub fn default_label(cols: u16) -> &'static str {
  let room = usize::from(cols.saturating_sub(ACTION_INSET));
  if actions_width("设为默认") <= room {
    "设为默认"
  } else {
    "默认"
  }
}

/// The cells the four actions take with the default button saying `default`.
pub fn actions_width(default: &str) -> usize {
  let buttons = [
    Button::new(crate::ui::ButtonKind::Default, "⏎", "用于本单"),
    Button::new(crate::ui::ButtonKind::Default, "e", "编辑"),
    Button::new(crate::ui::ButtonKind::Default, "space", default.to_owned()),
    Button::new(crate::ui::ButtonKind::Danger, "d", "删除"),
  ];
  buttons.iter().map(Button::cells).sum::<usize>() + usize::from(COLUMN_GAP) * (buttons.len() - 1)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn two_columns_from_l_one_below() {
    // XL 160 with the 2-column margins: 77-column cards.
    assert_eq!(
      shape(Bp::Xl, 156),
      Shape {
        columns: 2,
        card_cols: 77
      }
    );
    // Wider terminals keep the design's 77.
    assert_eq!(shape(Bp::Xl, 196).card_cols, 77);
    // L 130 with 1-column margins: 63.
    assert_eq!(shape(Bp::L, 128).card_cols, 63);
    assert_eq!(
      shape(Bp::M, 98),
      Shape {
        columns: 1,
        card_cols: 98
      }
    );
    assert_eq!(shape(Bp::S, 80).card_cols, 80);
  }

  #[test]
  fn cells_are_as_tall_as_the_spec() {
    let grid = shape(Bp::Xl, 156);
    let list = shape(Bp::S, 80);
    assert_eq!(cell_rows(grid, false, true), 11);
    assert_eq!(cell_rows(list, true, false), 5);
    assert_eq!(cell_rows(list, false, false), 4);
    assert_eq!(cell_rows(list, false, true), 3);
    assert_eq!(row_gap(grid), 1);
    assert_eq!(row_gap(list), 0);
  }

  #[test]
  fn the_window_keeps_the_cursor_and_scrolls_little() {
    let rows = [11, 11, 11, 11];
    // Everything fits: all rows.
    assert_eq!(window(&rows, 1, 0, 60, 0), 0..4);
    // Two rows fit (23 of 30).
    assert_eq!(window(&rows, 1, 0, 30, 0), 0..2);
    assert_eq!(window(&rows, 1, 1, 30, 0), 0..2);
    // Down to row 2: the window moves by one.
    assert_eq!(window(&rows, 1, 2, 30, 0), 1..3);
    // Back up to row 1: no move.
    assert_eq!(window(&rows, 1, 1, 30, 1), 1..3);
    assert_eq!(window(&rows, 1, 0, 30, 1), 0..2);
    // Too small for even one row: the cursor's row alone.
    assert_eq!(window(&rows, 1, 3, 5, 0), 3..4);
    // Not measured yet: everything.
    assert_eq!(window(&rows, 1, 3, 0, 2), 0..4);
    // After a delete, rows above come back.
    assert_eq!(window(&[11, 11], 1, 1, 30, 1), 0..2);
    assert_eq!(window(&[], 1, 0, 30, 0), 0..0);
    // Compact rows of different heights.
    assert_eq!(window(&[5, 4, 4, 4, 3], 0, 4, 12, 0), 2..5);
  }

  #[test]
  fn actions_shorten_on_narrow_cards() {
    assert_eq!(actions_width("设为默认"), 58);
    assert_eq!(actions_width("默认"), 54);
    assert_eq!(default_label(77), "设为默认");
    assert_eq!(default_label(63), "默认");
  }
}
