//! One product's detail and the SKU panel's selection (03-detail.md).
//!
//! Opening a product loads `GET /v1/products/{id}` and starts the selection as the design
//! says: the product's default weight (250g, or the first weight in stock), 整豆, 标准袋,
//! quantity 1, focus on the weight row (D13). Sold-out weights are skipped; the quantity is
//! 1 … `maxQuantity` (min(9, stock)); a product with no stock cannot be bought (K9). Prices
//! are the server's: the unit price and total come from the detail's price cells
//! (`PriceCell.totals[quantity − 1]`, P-api-8); this store never multiplies.

use coffee_api::{
  AddCartLineRequest, Grind, GuestCartLine, Pack, PriceCell, ProductDetail, Weight, sku_code,
};
use nsh_core::prelude::*;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// A row of the SKU panel, in focus order (`↑↓` moves between them, no wrap).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SkuGroup {
  /// `重量`.
  #[default]
  Weight,
  /// `研磨`.
  Grind,
  /// `包装`.
  Pack,
  /// `数量`.
  Quantity,
}

impl SkuGroup {
  /// The rows in order.
  pub const ALL: [SkuGroup; 4] = [
    SkuGroup::Weight,
    SkuGroup::Grind,
    SkuGroup::Pack,
    SkuGroup::Quantity,
  ];

  /// The row's label.
  pub fn label(self) -> &'static str {
    match self {
      SkuGroup::Weight => "重量",
      SkuGroup::Grind => "研磨",
      SkuGroup::Pack => "包装",
      SkuGroup::Quantity => "数量",
    }
  }
}

/// What the SKU panel has chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SkuSelection {
  /// Weight (never a sold-out one when another is in stock).
  pub weight: Weight,
  /// Grind.
  pub grind: Grind,
  /// Pack.
  pub pack: Pack,
  /// 1 … max(1, maxQuantity).
  pub quantity: u8,
  /// The focused row.
  pub focus: SkuGroup,
}

impl Default for SkuSelection {
  fn default() -> Self {
    Self {
      weight: Weight::G250,
      grind: Grind::WholeBean,
      pack: Pack::Standard,
      quantity: 1,
      focus: SkuGroup::Weight,
    }
  }
}

/// Whether `weight` is in stock for `detail`.
pub fn weight_available(detail: &ProductDetail, weight: Weight) -> bool {
  detail
    .weights
    .iter()
    .any(|w| w.weight == weight && w.available)
}

/// The selection a detail page starts with.
pub fn initial_selection(detail: &ProductDetail) -> SkuSelection {
  let weight = if weight_available(detail, detail.default_weight) {
    detail.default_weight
  } else {
    Weight::ALL
      .into_iter()
      .find(|w| weight_available(detail, *w))
      .unwrap_or(detail.default_weight)
  };
  SkuSelection {
    weight,
    ..SkuSelection::default()
  }
}

/// The weight `delta` steps from `current`, skipping sold-out ones; `current` when there
/// is none that way (no wrap).
pub fn step_weight(detail: &ProductDetail, current: Weight, delta: isize) -> Weight {
  let at = Weight::ALL.iter().position(|w| *w == current).unwrap_or(0) as isize;
  let mut i = at + delta.signum();
  while (0..Weight::ALL.len() as isize).contains(&i) {
    let weight = Weight::ALL[i as usize];
    if weight_available(detail, weight) {
      return weight;
    }
    i += delta.signum();
  }
  current
}

/// The option `delta` steps from `current` in `options`, clamped to the ends.
pub fn step_option<T: Copy + PartialEq>(options: &[T], current: T, delta: isize) -> T {
  let at = options.iter().position(|o| *o == current).unwrap_or(0);
  let last = options.len().saturating_sub(1);
  options[at.saturating_add_signed(delta).min(last)]
}

/// The most of this product one line may hold (at least 1, so the stepper shows `1`).
pub fn max_quantity(detail: &ProductDetail) -> u8 {
  detail.max_quantity.max(1)
}

/// Whether `selection` can be added to the cart (K9: no stock → `a` `b` disabled).
pub fn can_buy(detail: &ProductDetail, selection: &SkuSelection) -> bool {
  detail.max_quantity > 0
    && weight_available(detail, selection.weight)
    && (1..=detail.max_quantity).contains(&selection.quantity)
}

/// The server's price cell of `selection`.
pub fn price_cell<'a>(
  detail: &'a ProductDetail,
  selection: &SkuSelection,
) -> Option<&'a PriceCell> {
  detail.price(selection.weight, selection.pack)
}

/// The server's total of `selection` (`totals[quantity − 1]`).
pub fn total(detail: &ProductDetail, selection: &SkuSelection) -> Option<u32> {
  price_cell(detail, selection)
    .and_then(|cell| {
      cell
        .totals
        .get(usize::from(selection.quantity).checked_sub(1)?)
    })
    .copied()
}

/// The cart line `selection` of `product_id` makes (a guest line, selected).
pub fn guest_line(product_id: &str, selection: &SkuSelection) -> GuestCartLine {
  GuestCartLine {
    product_id: product_id.to_owned(),
    weight: selection.weight,
    grind: selection.grind,
    pack: selection.pack,
    quantity: selection.quantity,
    selected: true,
  }
}

/// The member request adding `line` (at `index`, for an undo).
pub fn add_request(line: &GuestCartLine, index: Option<u32>) -> AddCartLineRequest {
  AddCartLineRequest {
    product_id: line.product_id.clone(),
    weight: line.weight,
    grind: line.grind,
    pack: line.pack,
    quantity: line.quantity,
    index,
  }
}

/// The detail page's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct ProductStore {
  session: SessionStore,
  id: Signal<Option<SharedString>>,
  detail: Remote<ProductDetail>,
  selection: Signal<SkuSelection>,
}

impl ProductStore {
  /// No product, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      id: signal(None),
      detail: Remote::new(|_| false),
      selection: signal(SkuSelection::default()),
    }
  }

  /// The product shown, by id (tracked).
  pub fn id(&self) -> Option<SharedString> {
    self.id.get()
  }

  /// The detail's state (tracked).
  pub fn state(&self) -> RemoteState<ProductDetail> {
    self.detail.state()
  }

  /// The detail's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.detail.phase()
  }

  /// The failure, when failed (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.detail.failure()
  }

  /// The detail, once loaded (tracked).
  pub fn detail(&self) -> Option<ProductDetail> {
    self.detail.data()
  }

  /// The selection (tracked).
  pub fn selection(&self) -> SkuSelection {
    self.selection.get()
  }

  fn with_detail<R>(&self, f: impl FnOnce(&ProductDetail, &SkuSelection) -> R) -> Option<R> {
    let selection = self.selection.get();
    self.detail.with(|detail| detail.map(|d| f(d, &selection)))
  }

  /// Whether the selection can be added now (tracked).
  pub fn can_buy(&self) -> bool {
    self.with_detail(can_buy).unwrap_or(false)
  }

  /// The selection's unit price (tracked).
  pub fn unit_price(&self) -> Option<u32> {
    self
      .with_detail(|d, s| price_cell(d, s).map(|c| c.unit_price))
      .flatten()
  }

  /// The selection's total, as the server priced it (tracked).
  pub fn total(&self) -> Option<u32> {
    self.with_detail(total).flatten()
  }

  /// The selection's SKU code, `RT-PNK-250G-WB-STD` (tracked).
  pub fn sku(&self) -> Option<String> {
    self.with_detail(|d, s| sku_code(&d.code, s.weight, s.grind, s.pack))
  }

  /// The cart line the selection makes (untracked: for the add command).
  pub fn line(&self) -> Option<GuestCartLine> {
    let selection = self.selection.get_untracked();
    self
      .detail
      .with_untracked(|detail| detail.map(|d| guest_line(&d.id, &selection)))
  }

  /// Whether the selection can be added now, untracked.
  pub fn can_buy_untracked(&self) -> bool {
    untracked(|| self.can_buy())
  }

  // --- Opening ------------------------------------------------------------------------

  /// Shows product `id`: loads it unless it is the one shown or already loading (the
  /// selection then stays). The load starts in this call (the state is loading at once),
  /// so a second call in the same commit is a no-op instead of a second request.
  pub fn open(&self, id: &str) {
    if self
      .id
      .with_untracked(|current| current.as_deref() == Some(id))
      && !self.detail.is_idle()
      && self.detail.phase_untracked() != Phase::Failed
    {
      return;
    }
    batch(|| {
      self.id.set(Some(SharedString::from(id.to_owned())));
      self.selection.set(SkuSelection::default());
      self.detail.reset();
    });
    self.load();
  }

  /// Loads the product shown: what [`Self::load`] runs (await it in tests).
  pub async fn fetch(self) {
    let revision = self.detail.begin();
    self.request(revision).await;
  }

  /// Asks for the product shown; the answer applies while `revision` is the latest.
  async fn request(self, revision: u64) {
    let Some(id) = self.id.get_untracked() else {
      return;
    };
    let client = self.session.client();
    let result = client.product(&id).await;
    let initial = result.as_ref().ok().map(initial_selection);
    if self.detail.finish(revision, result)
      && let Some(initial) = initial
    {
      self.selection.set(initial);
    }
  }

  /// Loads (or retries) the product shown, in a task of the root's owner. The state turns
  /// loading now, not when the task first runs.
  pub fn load(&self) {
    let store = *self;
    let revision = self.detail.begin();
    self.session.spawn(store.request(revision));
  }

  /// Opens product `id` for a test without spawning: sets it and fetches it.
  pub async fn open_now(self, id: &str) {
    batch(|| {
      self.id.set(Some(SharedString::from(id.to_owned())));
      self.selection.set(SkuSelection::default());
      self.detail.reset();
    });
    self.fetch().await;
  }

  // --- The SKU panel ------------------------------------------------------------------

  fn change(&self, f: impl FnOnce(&ProductDetail, &mut SkuSelection)) {
    let mut selection = self.selection.get_untracked();
    let changed = self.detail.with_untracked(|detail| {
      detail.map(|d| {
        f(d, &mut selection);
      })
    });
    if changed.is_some() {
      self.selection.set(selection);
    }
  }

  /// `↑` `↓`: the previous / next row (no wrap).
  pub fn move_focus(&self, delta: isize) {
    self.change(|_, s| s.focus = step_option(&SkuGroup::ALL, s.focus, delta));
  }

  /// Focuses row `group` (a click, Tab).
  pub fn focus(&self, group: SkuGroup) {
    self.change(|_, s| s.focus = group);
  }

  /// `←` `→`: the previous / next option of the focused row (weights skip sold-out ones);
  /// on the quantity row, −1 / +1.
  pub fn step(&self, delta: isize) {
    self.change(|d, s| match s.focus {
      SkuGroup::Weight => s.weight = step_weight(d, s.weight, delta),
      SkuGroup::Grind => s.grind = step_option(&Grind::ALL, s.grind, delta),
      SkuGroup::Pack => s.pack = step_option(&Pack::ALL, s.pack, delta),
      SkuGroup::Quantity => s.quantity = step_quantity(d, s.quantity, delta),
    });
  }

  /// `+` / `−`: the quantity ±1 within 1 … maxQuantity (whatever the focus).
  pub fn step_quantity(&self, delta: isize) {
    self.change(|d, s| s.quantity = step_quantity(d, s.quantity, delta));
  }

  /// Chooses `weight` (a click); a sold-out weight is ignored.
  pub fn select_weight(&self, weight: Weight) {
    self.change(|d, s| {
      if weight_available(d, weight) {
        s.weight = weight;
        s.focus = SkuGroup::Weight;
      }
    });
  }

  /// Chooses `grind`.
  pub fn select_grind(&self, grind: Grind) {
    self.change(|_, s| {
      s.grind = grind;
      s.focus = SkuGroup::Grind;
    });
  }

  /// Chooses `pack`.
  pub fn select_pack(&self, pack: Pack) {
    self.change(|_, s| {
      s.pack = pack;
      s.focus = SkuGroup::Pack;
    });
  }

  /// Sets the quantity, clamped to 1 … maxQuantity (the server said only `available` are
  /// left: 03-detail.md, "数量回落到可买上限").
  pub fn set_quantity(&self, quantity: u8) {
    self.change(|d, s| s.quantity = quantity.clamp(1, max_quantity(d)));
  }
}

/// `quantity` + `delta` within 1 … maxQuantity.
pub fn step_quantity(detail: &ProductDetail, quantity: u8, delta: isize) -> u8 {
  let max = max_quantity(detail);
  let next = isize::from(quantity) + delta;
  next.clamp(1, isize::from(max)) as u8
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  fn detail(name: &str) -> ProductDetail {
    let body = match name {
      "pinkb" => {
        include_str!("../../../../packages/contract/fixtures/v1/catalog/product-pinkb.json")
      }
      _ => include_str!("../../../../packages/contract/fixtures/v1/catalog/product-geisha.json"),
    };
    serde_json::from_str(body).unwrap()
  }

  #[test]
  fn the_selection_starts_as_the_design_says() {
    let pinkb = detail("pinkb");
    let start = initial_selection(&pinkb);
    assert_eq!(start.weight, Weight::G250);
    assert_eq!(start.grind, Grind::WholeBean);
    assert_eq!(start.pack, Pack::Standard);
    assert_eq!((start.quantity, start.focus), (1, SkuGroup::Weight));
    let mut sold_out_default = pinkb.clone();
    sold_out_default.default_weight = Weight::Kg1;
    assert_eq!(initial_selection(&sold_out_default).weight, Weight::G100);
  }

  #[test]
  fn weights_skip_sold_out_ones_and_stop_at_the_ends() {
    let geisha = detail("geisha");
    // Geisha: 500g and 1kg are sold out.
    assert_eq!(step_weight(&geisha, Weight::G250, 1), Weight::G250);
    assert_eq!(step_weight(&geisha, Weight::G250, -1), Weight::G100);
    assert_eq!(step_weight(&geisha, Weight::G100, -1), Weight::G100);
    let pinkb = detail("pinkb");
    assert_eq!(step_weight(&pinkb, Weight::G250, 1), Weight::G500);
    assert_eq!(step_weight(&pinkb, Weight::G500, 1), Weight::G500);
  }

  #[test]
  fn quantity_and_prices_come_from_the_server() {
    let geisha = detail("geisha");
    assert_eq!(max_quantity(&geisha), 6);
    assert_eq!(step_quantity(&geisha, 6, 1), 6);
    assert_eq!(step_quantity(&geisha, 1, -1), 1);
    let selection = SkuSelection {
      weight: Weight::G100,
      pack: Pack::Gift,
      quantity: 2,
      ..SkuSelection::default()
    };
    let cell = price_cell(&geisha, &selection).unwrap();
    assert_eq!(total(&geisha, &selection), Some(cell.totals[1]));
    assert!(can_buy(&geisha, &selection));
    let mut gone = geisha.clone();
    gone.max_quantity = 0;
    assert!(!can_buy(&gone, &selection), "K9: no stock, no buying");
    assert_eq!(
      step_option(&Grind::ALL, Grind::ColdBrew, 1),
      Grind::ColdBrew
    );
    assert_eq!(step_option(&Pack::ALL, Pack::Standard, -1), Pack::Standard);
  }

  #[test]
  fn the_store_loads_and_edits_the_selection() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, store) = create_root(|| {
      ProductStore::new(SessionStore::with_client(ApiClient::new(
        Transport::Fixture(FixtureSet::Contract),
        None,
      )))
    });
    now_or_never(store.open_now("geisha"));
    assert_eq!(store.phase(), Phase::Ready);
    assert_eq!(store.selection().weight, Weight::G250);
    store.move_focus(3);
    assert_eq!(store.selection().focus, SkuGroup::Quantity);
    store.step(1);
    store.step(1);
    assert_eq!(store.selection().quantity, 3);
    store.step_quantity(10);
    assert_eq!(store.selection().quantity, 6);
    store.move_focus(-3);
    store.step(1);
    assert_eq!(store.selection().weight, Weight::G250, "500g is sold out");
    store.select_pack(Pack::Gift);
    assert_eq!(store.sku().as_deref(), Some("RT-GSH-250G-WB-GIFT"));
    let expected = detail("geisha")
      .price(Weight::G250, Pack::Gift)
      .unwrap()
      .totals[5];
    assert_eq!(store.total(), Some(expected));
    store.set_quantity(2);
    let line = store.line().unwrap();
    assert_eq!((line.product_id.as_str(), line.quantity), ("geisha", 2));
    assert!(store.can_buy());
    now_or_never(store.open_now("no-such-bean"));
    assert_eq!(
      store.phase(),
      Phase::Failed,
      "an unknown product is not found"
    );
    assert!(!store.can_buy());
    owner.dispose();
  }
}
