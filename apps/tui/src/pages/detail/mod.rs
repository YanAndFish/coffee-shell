//! 详情 DETAIL (03-detail.md): one bean's profile, flavour and the SKU panel, over the list.
//!
//! Store: [`crate::stores::ProductStore`] — `id`, `phase`, `failure`, `detail`,
//! `selection`, `move_focus` / `focus` / `step` / `step_quantity` / `select_weight` /
//! `select_grind` / `select_pack` / `set_quantity`, `can_buy`, `unit_price`, `total` (the
//! server's `PriceCell.totals`), `sku`. Actions: `actions::add_selection(stores, buy_now)`,
//! `actions::back`. The route carries the product id; the page opens it on entry.
//!
//! Layouts (responsive.md, "03 详情"), one per [`Shape`]:
//!
//! - **XL** (≥160): the information panel (no legend, 1fr) and `规格 SKU` (50) side by side;
//!   dossier, radar and meters in three columns; the brewing inset at the panel's foot.
//! - **L** (130–159): the same two columns, SKU 46; the meters under the radar.
//! - **M** (100–129): stacked and scrolled, information first, then the SKU panel with one row
//!   per group; the purchase row (total and buttons) pinned under the scrolled content.
//! - **S** (80–99): stacked and scrolled, buying first: back and title rows, the SKU panel
//!   (total, buttons and SKU code inside), then the information panel.
//!
//! On a short terminal (<32 rows) the radar becomes meters and the description keeps three
//! rows. At M and S a change of the selection scrolls its group into view. Only the
//! information column (XL, L) or the whole content (M, S) scrolls: one scroll view a page.

pub mod commands;
mod info;
mod sku;

use std::rc::Rc;

use coffee_api::ProductDetail;
use nsh_core::{RevealOptions, ScrollAlignment, element::Screen, prelude::*};

use super::PageModule;
use crate::{
  commands::{Mode, press},
  router::{Page, Route},
  stores::{Phase, SkuGroup, Stores},
  ui::{self, Bp, Layout, layout},
};

/// `详情 DETAIL`.
pub const MODE: Mode = Mode::new("详情", "DETAIL");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Detail,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

/// The SKU panel's width at XL (03-detail.md: 50 cells, 44 inside).
pub const SKU_COLS_XL: u16 = 50;
/// The SKU panel's width at L (46 cells, 40 inside).
pub const SKU_COLS_L: u16 = 46;

/// The page's layout family, from the width class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
  /// XL: two columns, three-column profile.
  Wide,
  /// L: two columns, the meters under the radar.
  Narrow,
  /// M: stacked, information first, the purchase row pinned.
  Stacked,
  /// S: stacked, buying first.
  Single,
}

impl Shape {
  /// The shape of width class `bp`.
  pub fn of(bp: Bp) -> Self {
    match bp {
      Bp::Xl => Shape::Wide,
      Bp::L => Shape::Narrow,
      Bp::M => Shape::Stacked,
      Bp::S | Bp::TooSmall => Shape::Single,
    }
  }

  /// Whether the SKU panel stands beside the information (XL, L).
  pub fn two_columns(self) -> bool {
    matches!(self, Shape::Wide | Shape::Narrow)
  }

  /// The SKU panel's width beside the information.
  pub fn sku_cols(self) -> u16 {
    match self {
      Shape::Narrow => SKU_COLS_L,
      _ => SKU_COLS_XL,
    }
  }

  /// The SKU panel's inside: its width less the border and two blank cells a side (44 at
  /// XL, 40 at L).
  pub fn sku_inner(self) -> u16 {
    self.sku_cols() - 6
  }
}

/// What the body shows: it is rebuilt only when one of these changes (a selection change
/// only updates bindings).
#[derive(Clone, Debug, PartialEq, Eq)]
struct BodyKey {
  shape: Shape,
  short: bool,
  state: State,
  id: Option<SharedString>,
}

/// The detail's state as the body draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
  Loading,
  Ready,
  Failed,
}

impl State {
  fn of(phase: Phase) -> Self {
    match phase {
      Phase::Idle | Phase::Loading => State::Loading,
      Phase::Ready => State::Ready,
      Phase::Failed | Phase::Empty => State::Failed,
    }
  }
}

/// The detail page of the route's product.
pub fn render(route: &Route) -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  let id = match route {
    Route::Detail(id) => id.clone(),
    _ => SharedString::default(),
  };
  let product = stores.product;
  // The page's work: show its product, unless the list (`actions::open_product`) already
  // opened it. `ProductStore::open` alone would ask again here: its load task has not begun
  // yet in this commit, so the detail still reads idle, and the first answer is dropped.
  effect(move || {
    untracked(|| {
      if product.id().as_deref() != Some(id.as_str()) || product.phase() == Phase::Failed {
        product.open(&id);
      }
    })
  });
  let key = computed(move || BodyKey {
    shape: Shape::of(layout.bp()),
    short: layout.short(),
    state: State::of(product.phase()),
    id: product.id(),
  });
  ui::page(
    "详情 DETAIL",
    view().flex_1().min_h(px(0.0)).child(dynamic(move || {
      let key = key.get();
      untracked(move || body(stores, &key))
    })),
  )
}

fn body(stores: Stores, key: &BodyKey) -> AnyElement {
  let product = stores.product;
  let detail = match key.state {
    State::Ready => product.detail().map(Rc::new),
    _ => None,
  };
  match detail {
    Some(detail) => ready(stores, key, detail),
    None => not_ready(stores, key),
  }
}

/// Loading or failed: the message centred in the information panel; at XL and L the SKU
/// panel stands beside it with only its legend.
fn not_ready(stores: Stores, key: &BodyKey) -> AnyElement {
  let message = match key.state {
    State::Failed => view()
      .flex_1()
      .child(ui::error_state(
        "商品加载失败",
        move || {
          SharedString::from(
            stores
              .product
              .failure()
              .map(|failure| failure.detail())
              .unwrap_or_default(),
          )
        },
        "⏎",
        "重试",
        move || press(&stores, "detail.retry"),
      ))
      .into_element(),
    _ => view()
      .flex_1()
      .child(ui::loading("正在载入…"))
      .into_element(),
  };
  let info = ui::panel_plain(message);
  if key.shape.two_columns() {
    columns(
      key.shape,
      info,
      view()
        .flex_none()
        .w(layout::cols(key.shape.sku_cols()))
        .child(ui::panel("规格 SKU", view().flex_1())),
    )
  } else {
    info.into_element()
  }
}

/// The two columns of XL and L: `info` (1fr) and the SKU panel, 2 cells apart at XL, 1 at L.
fn columns(shape: Shape, info: View, sku: View) -> AnyElement {
  let gap = if shape == Shape::Wide { 2 } else { 1 };
  view()
    .flex_1()
    .flex_row()
    .min_h(px(0.0))
    .gap(layout::gap_cols(gap))
    .child(info)
    .child(sku)
    .into_element()
}

fn ready(stores: Stores, key: &BodyKey, detail: Rc<ProductDetail>) -> AnyElement {
  match key.shape {
    Shape::Wide | Shape::Narrow => two_columns(stores, key, detail),
    Shape::Stacked | Shape::Single => stacked(stores, key, detail),
  }
}

/// XL and L: the information panel (its content scrolls when the terminal is too short for
/// it; the brewing inset stays at its foot) beside the SKU panel.
fn two_columns(stores: Stores, key: &BodyKey, detail: Rc<ProductDetail>) -> AnyElement {
  let (shape, short) = (key.shape, key.short);
  // The padding goes inside the scroll view, so its bar (when the terminal is too short)
  // stands against the panel's line, not inside the text's margin.
  let info = ui::panel_plain(
    view()
      .flex_1()
      .min_h(px(0.0))
      .child(layout::vspace(1))
      .child(
        scroll().flex_1().min_h(px(0.0)).child(
          view()
            .flex_none()
            .px(layout::pad_cols(2))
            .child(info::back_row(stores, &detail))
            .child(layout::vspace(1))
            .child(info::title_block(&detail))
            .child(layout::vspace(1))
            .child(info::description(&detail, shape, short))
            .child(layout::vspace(1))
            .child(info::profile(&detail, shape, short))
            .child(layout::vspace(1)),
        ),
      )
      .child(
        view()
          .flex_none()
          .px(layout::pad_cols(2))
          .child(info::brew(&detail)),
      )
      .child(layout::vspace(1)),
  );
  let sku = view()
    .flex_none()
    .w(layout::cols(shape.sku_cols()))
    .child(ui::panel(
      "规格 SKU",
      sku::stacked(stores, detail, shape.sku_inner()),
    ));
  columns(shape, info, sku)
}

/// A box as tall as its content, for the scrolled column: nsh's bordered panel
/// ([`ui::panel`]) grows to share the scroll view's height instead.
fn boxed(legend: Option<&'static str>, content: impl IntoElement) -> View {
  ui::frame(
    || ui::FrameStyle::Round,
    || ui::Tone::Line,
    legend.map(ui::Legend::new),
    None,
    1,
    content,
  )
}

/// M and S: one scrolled column (the scroll bar right of it); a change of
/// the selection scrolls its group into view. M pins the purchase row under it.
fn stacked(stores: Stores, key: &BodyKey, detail: Rc<ProductDetail>) -> AnyElement {
  let (shape, short) = (key.shape, key.short);
  let refs: [ElementRef; 4] = std::array::from_fn(|_| element_ref());
  reveal_focus(stores, refs);

  let purchase = if shape == Shape::Single {
    sku::Purchase::InPanel
  } else {
    sku::Purchase::Pinned
  };
  let sku_panel = boxed(
    Some("规格 SKU"),
    sku::inline(stores, detail.clone(), refs, purchase),
  );
  let info_panel = boxed(
    None,
    view()
      .flex_none()
      .child(info::description(&detail, shape, short))
      .child(layout::vspace(1))
      .child(info::profile(&detail, shape, short))
      .child(layout::vspace(1))
      .child(info::brew(&detail)),
  );
  let heading = view()
    .flex_none()
    .px(layout::pad_cols(1))
    .child(info::back_row(stores, &detail))
    .child(info::title_row(&detail));
  let column = match shape {
    Shape::Single => view()
      .flex_none()
      .child(heading)
      .child(sku_panel)
      .child(info_panel),
    _ => view()
      .flex_none()
      .child(heading)
      .child(info_panel)
      .child(sku_panel),
  };
  // nsh keeps the scroll bar a column of its own, right of the content: the panels reach
  // it (03-detail.md's S mock, `│┃`), no blank column between.
  let scrolled = scroll().flex_1().min_h(px(0.0)).child(
    view()
      .flex_none()
      .flex_row()
      .child(view().flex_1().min_w(px(0.0)).child(column)),
  );
  view()
    .flex_1()
    .min_h(px(0.0))
    .child(scrolled)
    .child((shape == Shape::Stacked).then(|| sku::purchase_row(stores)))
    .into_element()
}

/// Scrolls the focused group's row into view whenever the selection changes (not on the
/// first run: the page opens at its top).
fn reveal_focus(stores: Stores, refs: [ElementRef; 4]) {
  let product = stores.product;
  let selection = computed(move || product.selection());
  let first = Rc::new(std::cell::Cell::new(true));
  effect(move || {
    let focus = selection.get().focus;
    if first.replace(false) {
      return;
    }
    let index = SkuGroup::ALL
      .iter()
      .position(|group| *group == focus)
      .unwrap_or(0);
    refs[index].reveal(RevealOptions::default().vertical(ScrollAlignment::Nearest));
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn shapes_follow_the_breakpoints() {
    assert_eq!(Shape::of(Bp::Xl), Shape::Wide);
    assert_eq!(Shape::of(Bp::L), Shape::Narrow);
    assert_eq!(Shape::of(Bp::M), Shape::Stacked);
    assert_eq!(Shape::of(Bp::S), Shape::Single);
    assert!(Shape::Wide.two_columns() && Shape::Narrow.two_columns());
    assert!(!Shape::Stacked.two_columns() && !Shape::Single.two_columns());
  }

  #[test]
  fn the_buttons_fill_the_sku_panel() {
    // Inside = width − 2 border cells − 2 × 2 padding cells.
    assert_eq!(Shape::Wide.sku_inner(), 44);
    assert_eq!(Shape::Narrow.sku_inner(), 40);
    for shape in [Shape::Wide, Shape::Narrow] {
      let (add, buy) = sku::button_widths(shape.sku_inner());
      assert_eq!(add + 1 + buy, shape.sku_inner(), "{shape:?}");
    }
    assert_eq!(
      State::of(Phase::Idle),
      State::Loading,
      "an unopened page shows the loading line"
    );
    assert_eq!(State::of(Phase::Empty), State::Failed);
  }
}
