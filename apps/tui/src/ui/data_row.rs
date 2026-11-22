//! DataRow: table-style lists with a `▶` cursor (`components/DataRow`).
//!
//! The first column (two cells) holds `▶` on the current row only; the current row also
//! stands on the `acc` tint band, so the cursor reads without colour (the glyph) and with
//! it (the band, and the page's accent on the name). Columns are fixed cell widths or one
//! flexible column, a one-cell gap apart; a page drops columns as the terminal narrows (see
//! `pages::catalog`).

use nsh_core::{element::PressEvent, prelude::*};

use super::{layout, theme};

/// How wide a column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
  /// What is left: the name column.
  Flex,
  /// A fixed number of cells.
  Cells(u16),
}

/// Where a column's content sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
  /// Left (text).
  Start,
  /// Right (amounts).
  End,
}

/// A column of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
  /// Its head, `名称`.
  pub title: &'static str,
  /// Its width.
  pub width: Width,
  /// Its alignment.
  pub align: Align,
}

/// The cells of the cursor column.
const CURSOR_COLS: u16 = 2;

fn cell(column: Column, content: impl IntoElement) -> View {
  let cell = match column.width {
    Width::Flex => view().flex_1().min_w(px(0.0)),
    Width::Cells(n) => view().flex_none().w(layout::cols(n)),
  };
  match column.align {
    Align::Start => cell.items_start(),
    Align::End => cell.items_end(),
  }
  .child(content)
}

fn row_frame() -> View {
  view().flex_row().gap_2().px_2()
}

/// The head row of `columns`, in `mute`. (A lone border line is not drawn by nsh's terminal
/// backend, which keeps borders only on cards, so the head has no rule under it.)
pub fn table_head(columns: &[Column]) -> View {
  row_frame()
    .flex_none()
    .child(view().flex_none().w(layout::cols(CURSOR_COLS)))
    .child(
      columns
        .iter()
        .map(|&column| cell(column, text(column.title).text_color(theme::mute)))
        .collect::<Vec<_>>(),
    )
}

/// A row of `columns` holding `cells` (one element per column, in order). `current()`
/// says whether the cursor is on it. A click runs `on_press(clicks)`: 1 for a click, 2 for
/// a double click.
pub fn data_row(
  columns: &[Column],
  current: impl Fn() -> bool + Clone + 'static,
  cells: Vec<AnyElement>,
  on_press: impl Fn(u32) + 'static,
) -> View {
  let band = current.clone();
  row_frame()
    .flex_none()
    .focusable(false)
    .on_press(move |event: &PressEvent| on_press(event.clicks))
    .bg(move || {
      if band() {
        theme::acc_tint()
      } else {
        AdaptiveColor::TRANSPARENT
      }
    })
    .child(
      view().flex_none().w(layout::cols(CURSOR_COLS)).child(
        text(move || if current() { "▶" } else { "" })
          .text_color(theme::acc)
          .font_weight(FontWeight::BOLD),
      ),
    )
    .child(
      columns
        .iter()
        .zip(cells)
        .map(|(&column, content)| cell(column, content))
        .collect::<Vec<_>>(),
    )
}
