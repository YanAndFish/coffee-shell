//! 购物车 CART (04-cart.md): the lines, selection, undo, coupon, order address, amounts and
//! checkout, then the receipt.
//!
//! Store: [`crate::stores::CartStore`] — `phase`, `failure`, `cart` (every amount, as the
//! server priced it), `lines`, `item_count`, `selected_count`, `cursor` / `is_cursor` /
//! `select_line`, `undo`, `coupon_input` / `set_coupon_input` / `coupon_feedback`,
//! `checkout_step` / `submitting`, `receipt`. Actions (through the command table):
//! `actions::toggle_line`, `toggle_all`, `step_line`, `remove_line`, `undo_remove`,
//! `apply_coupon`, `checkout`, `continue_shopping`. The coupon field reports its focus as
//! [`COUPON_FIELD`]. A guest's cart is quoted, never computed.
//!
//! Layouts (responsive.md, 04): XL cart 1fr + checkout 45 columns, L + 40, M + 36 (no unit
//! price column; the spec on the subtitle's row); S stacked, the cart's rows two lines tall
//! and the compact checkout block under it. The receipt replaces the whole content area.

mod checkout;
pub mod commands;
mod copy;
mod lines;
mod receipt;

use nsh_core::{element::Screen, prelude::*};

use super::PageModule;
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::Stores,
  ui::{
    self, Bp, CartForm, Layout,
    layout::{MAX_CONTENT_COLS, cols},
  },
};

/// `购物车 CART` (the receipt too).
pub const MODE: Mode = Mode::new("购物车", "CART");

/// The coupon field's name in the focus tracking.
pub const COUPON_FIELD: &str = "cart.coupon";

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Cart,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

/// What the layout depends on: the width class, short or not, and the content's columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
  /// The width class.
  pub bp: Bp,
  /// Fewer than 32 rows.
  pub short: bool,
  /// The terminal's columns.
  pub cols: u16,
}

impl Shape {
  /// The shape of a `cols` × `rows` terminal.
  pub fn of(cols: u16, rows: u16) -> Self {
    let viewport = ui::Viewport::new(cols, rows);
    Self {
      bp: viewport.bp(),
      short: viewport.short(),
      cols,
    }
  }

  /// The columns the pages get (the shell keeps a column each side, 200 at most).
  pub fn content_cols(self) -> u16 {
    self.cols.min(MAX_CONTENT_COLS).saturating_sub(2)
  }

  /// The checkout column's width: XL 45, L 40, M 36.
  pub fn checkout_cols(self) -> u16 {
    match self.bp {
      Bp::Xl => 45,
      Bp::L => 40,
      _ => 36,
    }
  }

  /// The checkout column's content width: inside its line and two blank cells each side
  /// (XL 39).
  pub fn checkout_inner(self) -> u16 {
    self.checkout_cols().saturating_sub(6)
  }

  /// The cells of the cart rows' name column in the wide form (two columns only): what is
  /// left of the cart panel after its frame and padding (4), the row's padding (2) and the
  /// fixed columns (cursor 2, box 5, spec 21, unit 9, stepper 13, total 12, delete 3).
  pub fn wide_name_cells(self) -> u16 {
    self
      .content_cols()
      .saturating_sub(self.checkout_cols() + 2 + 4 + 2 + 65)
  }

  /// How much of a cart row shows: S two-line rows; the wide form while its name column
  /// keeps at least 24 cells (XL, and L from 139 columns); else the medium form (no unit
  /// price column, the SKU under the name).
  pub fn form(self) -> CartForm {
    if self.bp <= Bp::S {
      CartForm::Small
    } else if self.wide_name_cells() >= 24 {
      CartForm::Wide
    } else {
      CartForm::Medium
    }
  }

  /// The cells of the name column in [`Self::form`].
  pub fn name_cells(self) -> u16 {
    match self.form() {
      CartForm::Wide => self.wide_name_cells(),
      // No spec (21) and unit (9) columns.
      CartForm::Medium => self
        .content_cols()
        .saturating_sub(self.checkout_cols() + 2 + 4 + 2 + 35),
      // One column: the panel, its padding, the row's, cursor, box, stepper, total, delete.
      CartForm::Small => self
        .content_cols()
        .saturating_sub(4 + 2 + 2 + 5 + 13 + 12 + 3),
    }
  }

  /// The free-shipping bar of the compact block (S): 40 cells, less when the row is narrow.
  pub fn compact_bar(self) -> u16 {
    40.min(self.content_cols().saturating_sub(36))
  }

  /// The receipt card's width: min(77, columns − 2).
  pub fn receipt_cols(self) -> u16 {
    77.min(self.cols.saturating_sub(2))
  }

  /// The parts that change the build: the width class and shortness always, the columns
  /// only where a length follows them (S, the receipt below 79 columns).
  fn key(self) -> (Bp, bool, CartForm, u16) {
    let cols = if self.bp <= Bp::S || self.form() == CartForm::Medium {
      self.cols
    } else {
      0
    };
    (self.bp, self.short, self.form(), cols)
  }
}

fn shop(stores: Stores, shape: Shape) -> View {
  if shape.bp <= Bp::S {
    view()
      .flex_1()
      .min_h(px(0.0))
      .child(lines::panel(stores, shape))
      .child(
        view()
          .flex_none()
          .h(ui::layout::rows(checkout::COMPACT_ROWS))
          .child(checkout::compact(stores, shape)),
      )
  } else {
    view()
      .flex_1()
      .min_h(px(0.0))
      .flex_row()
      .child(lines::panel(stores, shape))
      .child(ui::layout::hspace(2))
      .child(
        view()
          .flex_none()
          .w(cols(shape.checkout_cols()))
          .child(checkout::panel(stores, shape)),
      )
  }
}

/// The cart page.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  let cart = stores.cart;
  // The page's work: the cart (member) or its quote (guest), once.
  effect(move || untracked(|| cart.ensure_loaded()));
  let shape = computed(move || {
    let viewport = layout.viewport();
    Shape::of(viewport.cols, viewport.rows)
  });
  let key = computed(move || shape.get().key());
  let receipt = computed(move || cart.receipt());
  ui::page(
    "购物车 CART",
    dynamic(move || {
      key.get();
      let order = receipt.get();
      untracked(|| {
        let shape = shape.get();
        match order {
          Some(order) => receipt::view_of(stores, shape, order),
          None => shop(stores, shape),
        }
      })
    }),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_columns_follow_responsive_md() {
    let xl = Shape::of(160, 50);
    assert_eq!((xl.checkout_cols(), xl.checkout_inner()), (45, 39));
    assert_eq!(xl.receipt_cols(), 77);
    assert_eq!(Shape::of(130, 40).checkout_cols(), 40);
    assert_eq!(Shape::of(100, 30).checkout_cols(), 36);
    let s = Shape::of(80, 24);
    assert!(s.short && s.bp == Bp::S);
    assert_eq!(s.compact_bar(), 40);
    assert_eq!(s.receipt_cols(), 77);
    assert_eq!(Shape::of(78, 24).receipt_cols(), 76);
    assert_eq!(Shape::of(160, 50).key(), Shape::of(170, 50).key());
    assert_eq!(xl.form(), CartForm::Wide);
    assert_eq!(xl.name_cells(), 40);
    assert_eq!(Shape::of(130, 40).form(), CartForm::Medium);
    assert_eq!(Shape::of(139, 40).form(), CartForm::Wide);
    assert_eq!(Shape::of(100, 30).form(), CartForm::Medium);
    assert_eq!(Shape::of(100, 30).name_cells(), 19);
    assert_eq!(Shape::of(80, 24).form(), CartForm::Small);
    assert_ne!(Shape::of(80, 24).key(), Shape::of(90, 24).key());
  }
}
