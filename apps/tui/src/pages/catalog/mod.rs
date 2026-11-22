//! 选购 BROWSE (02-list.md): the bean list with its filters, sort, search and preview, and
//! the S-width filter dialog (`筛选 FILTER`).
//!
//! | width | layout |
//! |---|---|
//! | XL ≥160 | `筛选 FILTER` 27 · `商品列表` 1fr · `预览 PREVIEW` 42, two cells apart |
//! | L 130–159 | filter 25 · list · preview 36, a cell apart |
//! | M 100–129 | list (the filter chip row on top) · preview 36 |
//! | S 80–99 | the list alone, the filter summary on its first row; `f` opens the dialog |
//!
//! Below 32 rows each bean takes one line (the cursor row a second: subtitle · process),
//! the radar becomes six meters and the description three lines.
//!
//! Store: [`crate::stores::CatalogStore`]; actions `open_product`, `quick_add`, `search`.
//! The keys are this page's [`commands::COMMANDS`]; every control here presses the same
//! commands. The search box reports its focus as [`crate::stores::actions::SEARCH_FIELD`].

pub mod commands;
mod filters;
pub mod form;
mod list;
mod preview;

use nsh_core::{element::Screen, prelude::*};

use self::form::Shape;
use super::{Dialog, PageModule};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{Overlay, Stores},
  ui::{self, Layout, layout},
};

pub use list::SEARCH_PLACEHOLDER;

/// `选购 BROWSE`.
pub const MODE: Mode = Mode::new("选购", "BROWSE");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Catalog,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[Dialog {
    overlay: Overlay::FilterDialog,
    view: filters::dialog,
  }],
};

/// A fixed-width column of `cols` cells holding `content` (a panel filling its height).
fn column(cols: u16, content: View) -> View {
  view().flex_none().w(layout::cols(cols)).child(content)
}

/// The page's panels in `shape`.
fn panels(stores: Stores, shape: Shape) -> View {
  let form = shape.form;
  let gap = layout::hspace(form.gap_cols());
  let mut row = view().flex_1().min_h(px(0.0)).flex_row();
  if let Some(cols) = form.filter_cols() {
    row = row
      .child(column(cols, filters::sidebar(stores, form, shape.short)))
      .child(layout::hspace(form.gap_cols()));
  }
  row = row.child(list::list_panel(stores, shape));
  if let Some(cols) = form.preview_cols() {
    row = row
      .child(gap)
      .child(column(cols, preview::preview_panel(stores, shape)));
  }
  row
}

/// The list page.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  let catalog = stores.catalog;
  // The page's work: ask for the beans once it shows (the store keeps them after).
  effect(move || untracked(|| catalog.ensure_loaded()));
  // One layout per shape; the state lives in the stores, so a resize keeps it.
  let shape = computed(move || Shape::of(layout.viewport()));
  ui::page("选购 BROWSE", dynamic(move || panels(stores, shape.get())))
}
