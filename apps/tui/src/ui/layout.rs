//! Responsive breakpoints in terminal cells (the plan's "响应式断点"; the design is 160×50).
//!
//! nsh lays the app out in points and maps one cell to 8 × 16 points by default
//! (`nsh_platform_tui::CellSize`; `nsh.json` `tui.pointsPerCell` changes it, and this app
//! does not). [`Viewport::current`] turns the window's points back into cells, so it
//! follows a live resize: the terminal host redraws on `SIGWINCH` and the window signal
//! changes with it.

use nsh_core::prelude::*;

/// Points per cell, across (nsh's default).
pub const CELL_WIDTH: f32 = 8.0;
/// Points per cell, down (nsh's default).
pub const CELL_HEIGHT: f32 = 16.0;

/// The smallest terminal the shop lays out in.
pub const MIN_COLS: u16 = 80;
/// The smallest terminal height the shop lays out in.
pub const MIN_ROWS: u16 = 24;
/// Below this many rows the layout is "short": one-line rows, no illustrations.
pub const SHORT_ROWS: u16 = 32;
/// The widest the content gets, in columns (wider terminals centre it).
pub const MAX_CONTENT_COLS: u16 = 200;

/// Points of `cells` columns, for widths.
pub fn cols(cells: u16) -> Points {
  px(f32::from(cells) * CELL_WIDTH)
}

/// Points of `cells` rows, for heights.
pub fn rows(cells: u16) -> Points {
  px(f32::from(cells) * CELL_HEIGHT)
}

/// A gap of `cells` columns between the children of a row, as nsh's terminal layout reads
/// gaps (`nsh_core`'s `terminal_layout`: a column from above 0, two from 24 points; it has
/// no wider gap: use [`hspace`] for more).
pub fn gap_cols(cells: u16) -> Points {
  match cells {
    0 => px(0.0),
    1 => px(CELL_WIDTH),
    _ => px(3.0 * CELL_WIDTH),
  }
}

/// Horizontal padding of `cells` columns as nsh's terminal layout reads it (`terminal_layout`:
/// a column from 8 points, two from 24; at most one on a box of text or a control).
pub fn pad_cols(cells: u16) -> Points {
  gap_cols(cells)
}

/// A blank of exactly `cells` columns in a row. nsh's terminal layout turns padding into at
/// most two columns (one on a box of text) and keeps an empty box's explicit size, so
/// exact insets are spacers.
pub fn hspace(cells: u16) -> View {
  view().flex_none().w(cols(cells))
}

/// A blank of exactly `count` rows in a column (vertical padding under 24 points and gaps
/// under 16 are no rows in nsh's terminal layout, and none is more than one).
pub fn vspace(count: u16) -> View {
  view().flex_none().h(rows(count))
}

/// A width class of the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bp {
  /// Below 80×24: only the "terminal too small" notice.
  TooSmall,
  /// 80–99 columns: one column, no preview, 4 footer hints.
  S,
  /// 100–129 columns: list with preview.
  M,
  /// 130–159 columns: three narrow columns.
  L,
  /// 160 columns and more: the design's layout.
  Xl,
}

impl Bp {
  /// How many page hints the status bar shows (the `?` help hint comes on top).
  pub fn footer_hints(self) -> usize {
    match self {
      Bp::TooSmall => 0,
      Bp::S => 4,
      Bp::M => 5,
      Bp::L | Bp::Xl => 7,
    }
  }
}

/// The terminal's size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Viewport {
  /// Columns.
  pub cols: u16,
  /// Rows.
  pub rows: u16,
}

impl Viewport {
  /// A viewport of `cols` × `rows` cells.
  pub const fn new(cols: u16, rows: u16) -> Self {
    Self { cols, rows }
  }

  /// The cells a surface of `width` × `height` points covers (whole cells: a terminal's
  /// window is always whole cells; a headless window may not be).
  pub fn from_points(width: f32, height: f32) -> Self {
    let cells =
      |points: f32, per: f32| (points / per).floor().clamp(0.0, f32::from(u16::MAX)) as u16;
    Self::new(cells(width, CELL_WIDTH), cells(height, CELL_HEIGHT))
  }

  /// The window's size now (reactive: a binding reading it follows resizes).
  pub fn current() -> Self {
    let size = window();
    Self::from_points(size.width.0, size.height.0)
  }

  /// Its width class.
  pub fn bp(self) -> Bp {
    if self.cols < MIN_COLS || self.rows < MIN_ROWS {
      Bp::TooSmall
    } else if self.cols < 100 {
      Bp::S
    } else if self.cols < 130 {
      Bp::M
    } else if self.cols < 160 {
      Bp::L
    } else {
      Bp::Xl
    }
  }

  /// Fewer than 32 rows: one-line rows and the one-line wordmark.
  pub fn short(self) -> bool {
    self.rows < SHORT_ROWS
  }

  /// It is too small to lay the shop out.
  pub fn too_small(self) -> bool {
    self.bp() == Bp::TooSmall
  }
}

/// The terminal's size as the whole app reads it: one cached value, provided by the root
/// ([`Layout::provide`]) so every binding reads the same memo.
#[derive(Clone, Copy)]
pub struct Layout {
  viewport: Memo<Viewport>,
}

impl Layout {
  /// Makes the app's layout and provides it to what the current owner makes.
  pub fn provide() -> Self {
    let layout = Self {
      viewport: computed(Viewport::current),
    };
    provide_context(layout);
    layout
  }

  /// The layout the root provided.
  ///
  /// # Panics
  ///
  /// Outside the app's root (no layout was provided).
  pub fn get() -> Self {
    use_context::<Self>().expect("the root provides the layout")
  }

  /// The viewport (tracked).
  pub fn viewport(&self) -> Viewport {
    self.viewport.get()
  }

  /// The width class (tracked).
  pub fn bp(&self) -> Bp {
    self.viewport.get().bp()
  }

  /// Fewer than 32 rows (tracked).
  pub fn short(&self) -> bool {
    self.viewport.get().short()
  }

  /// Below 80×24 (tracked).
  pub fn too_small(&self) -> bool {
    self.viewport.get().too_small()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn breakpoints_follow_the_plan() {
    let bp = |cols, rows| Viewport::new(cols, rows).bp();
    assert_eq!(bp(79, 50), Bp::TooSmall);
    assert_eq!(bp(160, 23), Bp::TooSmall);
    assert_eq!(bp(60, 20), Bp::TooSmall);
    assert_eq!(bp(80, 24), Bp::S);
    assert_eq!(bp(99, 30), Bp::S);
    assert_eq!(bp(100, 30), Bp::M);
    assert_eq!(bp(129, 40), Bp::M);
    assert_eq!(bp(130, 40), Bp::L);
    assert_eq!(bp(159, 40), Bp::L);
    assert_eq!(bp(160, 50), Bp::Xl);
    assert_eq!(bp(300, 90), Bp::Xl);
  }

  #[test]
  fn short_is_below_32_rows() {
    assert!(Viewport::new(80, 24).short());
    assert!(Viewport::new(160, 31).short());
    assert!(!Viewport::new(160, 32).short());
    assert!(!Viewport::new(160, 50).short());
  }

  #[test]
  fn points_map_to_whole_cells() {
    assert_eq!(Viewport::from_points(1280.0, 800.0), Viewport::new(160, 50));
    assert_eq!(Viewport::from_points(640.0, 384.0), Viewport::new(80, 24));
    // A headless default phone window: 390 × 844 points.
    assert_eq!(Viewport::from_points(390.0, 844.0), Viewport::new(48, 52));
    assert_eq!(Viewport::from_points(-5.0, f32::NAN), Viewport::new(0, 0));
  }

  #[test]
  fn hints_shrink_with_the_width() {
    assert_eq!(Bp::S.footer_hints(), 4);
    assert!(Bp::M.footer_hints() > Bp::S.footer_hints());
    assert_eq!(Bp::Xl.footer_hints(), 7);
  }
}
