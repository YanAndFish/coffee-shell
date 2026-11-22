//! The catalog: the bean list as the backend gave it, and the cursor over it.
//!
//! `idle → loading → ready | empty | failed`, and `failed → loading` again on retry. Each
//! load takes a revision; an answer for an older revision is dropped, so a slow first
//! answer cannot overwrite a newer one. The cursor is a product id, not an index, so it
//! stays on its bean when the list reloads.

use std::rc::Rc;

use coffee_api::{ProductList, ProductSummary};
use nsh_core::prelude::*;

use crate::api::{ApiClient, ApiFailure};

/// Where the catalog stands.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogState {
  /// Nothing asked yet.
  Idle,
  /// A request is under way.
  Loading,
  /// The products, in display order (never empty).
  Ready(Rc<[ProductSummary]>),
  /// The backend has no products.
  Empty,
  /// The request failed.
  Failed(ApiFailure),
}

impl CatalogState {
  /// The state an answer leads to.
  pub fn settle(result: Result<ProductList, ApiFailure>) -> Self {
    match result {
      Ok(list) if list.items.is_empty() => CatalogState::Empty,
      Ok(list) => CatalogState::Ready(list.items.into()),
      Err(failure) => CatalogState::Failed(failure),
    }
  }

  /// The products shown (none unless ready).
  pub fn products(&self) -> &[ProductSummary] {
    match self {
      CatalogState::Ready(products) => products,
      _ => &[],
    }
  }
}

/// The phase of a [`CatalogState`], for code that branches on it without the data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CatalogPhase {
  /// [`CatalogState::Idle`] or [`CatalogState::Loading`].
  Loading,
  /// [`CatalogState::Ready`].
  Ready,
  /// [`CatalogState::Empty`].
  Empty,
  /// [`CatalogState::Failed`].
  Failed,
}

/// The id `delta` rows from `current` in `products`, clamped to the ends. No cursor yet,
/// or one on a product that is gone, counts as the first row.
pub fn step(
  products: &[ProductSummary],
  current: Option<&str>,
  delta: isize,
) -> Option<SharedString> {
  let last = products.len().checked_sub(1)?;
  let at = current
    .and_then(|id| products.iter().position(|p| p.id == id))
    .unwrap_or(0);
  let index = at.saturating_add_signed(delta).min(last);
  Some(products[index].id.clone().into())
}

/// The catalog's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct CatalogStore {
  state: Signal<CatalogState>,
  selected: Signal<Option<SharedString>>,
  revision: Signal<u64>,
  /// The owner loads run under: the store's, so a load outlives the page that asked.
  owner: Owner,
}

impl CatalogStore {
  /// An idle catalog, owned by the current owner (the root).
  pub fn new() -> Self {
    Self {
      state: signal(CatalogState::Idle),
      selected: signal(None),
      revision: signal(0),
      owner: Owner::current(),
    }
  }

  /// The state (tracked).
  pub fn state(&self) -> CatalogState {
    self.state.get()
  }

  /// The phase (tracked; changes only when the phase does).
  pub fn phase(&self) -> CatalogPhase {
    self.state.with(|state| match state {
      CatalogState::Idle | CatalogState::Loading => CatalogPhase::Loading,
      CatalogState::Ready(_) => CatalogPhase::Ready,
      CatalogState::Empty => CatalogPhase::Empty,
      CatalogState::Failed(_) => CatalogPhase::Failed,
    })
  }

  /// The failure, when failed (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.state.with(|state| match state {
      CatalogState::Failed(failure) => Some(failure.clone()),
      _ => None,
    })
  }

  /// The products shown (tracked).
  pub fn products(&self) -> Rc<[ProductSummary]> {
    self.state.with(|state| match state {
      CatalogState::Ready(products) => products.clone(),
      _ => Rc::from([]),
    })
  }

  /// How many products are shown (tracked).
  pub fn count(&self) -> usize {
    self.state.with(|state| state.products().len())
  }

  /// The product `id`, when shown (tracked).
  pub fn product(&self, id: &str) -> Option<ProductSummary> {
    self
      .state
      .with(|state| state.products().iter().find(|p| p.id == id).cloned())
  }

  /// The id under the cursor: the selected product, else the first (tracked).
  pub fn cursor(&self) -> Option<SharedString> {
    let selected = self.selected.get();
    self
      .state
      .with(|state| step(state.products(), selected.as_deref(), 0))
  }

  /// Whether the cursor is on `id` (tracked).
  pub fn is_cursor(&self, id: &str) -> bool {
    self.cursor().is_some_and(|cursor| cursor == id)
  }

  /// Moves the cursor `delta` rows, stopping at the ends.
  pub fn move_cursor(&self, delta: isize) {
    let selected = self.selected.get_untracked();
    let next = self
      .state
      .with_untracked(|state| step(state.products(), selected.as_deref(), delta));
    if next.is_some() && next != selected {
      self.selected.set(next);
    }
  }

  /// Puts the cursor on `id`.
  pub fn select(&self, id: &str) {
    let id = SharedString::from(id.to_owned());
    if self.selected.with_untracked(|s| s.as_ref() != Some(&id)) {
      self.selected.set(Some(id));
    }
  }

  /// Starts a load: the state is loading, and the revision its answer must carry.
  pub fn begin(&self) -> u64 {
    let revision = self.revision.get_untracked() + 1;
    batch(|| {
      self.revision.set(revision);
      self.state.set(CatalogState::Loading);
    });
    revision
  }

  /// Applies the answer of load `revision`; an answer of an older load is dropped.
  /// Whether it applied.
  pub fn finish(&self, revision: u64, result: Result<ProductList, ApiFailure>) -> bool {
    if revision != self.revision.get_untracked() {
      return false;
    }
    self.state.set(CatalogState::settle(result));
    true
  }

  /// Loads the products over `client` in a task of the store's owner (a page going away
  /// does not cancel it), unless a load is already under way.
  pub fn load(&self, client: ApiClient) {
    if self
      .state
      .with_untracked(|state| matches!(state, CatalogState::Loading))
    {
      return;
    }
    let store = *self;
    let revision = self.begin();
    self.owner.run(|| {
      spawn(async move {
        let result = client.products().await;
        store.finish(revision, result);
      })
    });
  }

  /// Loads the products if nothing was asked yet.
  pub fn ensure_loaded(&self, client: ApiClient) {
    if self
      .state
      .with_untracked(|state| matches!(state, CatalogState::Idle))
    {
      self.load(client);
    }
  }
}

impl Default for CatalogStore {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use coffee_api::ProductList;
  use nsh_core::element::{Runtime, create_root};

  use super::*;

  const PRODUCTS: &str = include_str!("../../../../packages/contract/fixtures/v1/products.json");

  fn list() -> ProductList {
    serde_json::from_str(PRODUCTS).unwrap()
  }

  fn with_store(test: impl FnOnce(CatalogStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, store) = create_root(CatalogStore::new);
    test(store);
    owner.dispose();
  }

  #[test]
  fn answers_settle_into_states() {
    assert!(matches!(CatalogState::settle(Ok(list())), CatalogState::Ready(p) if p.len() == 8));
    assert_eq!(
      CatalogState::settle(Ok(ProductList { items: vec![] })),
      CatalogState::Empty
    );
    let failure = ApiFailure::Network("refused".into());
    assert_eq!(
      CatalogState::settle(Err(failure.clone())),
      CatalogState::Failed(failure)
    );
  }

  #[test]
  fn a_load_goes_through_loading() {
    with_store(|store| {
      assert_eq!(store.state(), CatalogState::Idle);
      assert_eq!(store.phase(), CatalogPhase::Loading);
      let revision = store.begin();
      assert_eq!(store.state(), CatalogState::Loading);
      assert!(store.finish(revision, Ok(list())));
      assert_eq!(store.phase(), CatalogPhase::Ready);
      assert_eq!(store.count(), 8);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
    });
  }

  #[test]
  fn a_failure_then_a_retry() {
    with_store(|store| {
      let first = store.begin();
      store.finish(first, Err(ApiFailure::Network("refused".into())));
      assert_eq!(store.phase(), CatalogPhase::Failed);
      assert!(store.failure().is_some());
      let retry = store.begin();
      assert_eq!(store.phase(), CatalogPhase::Loading);
      store.finish(retry, Ok(ProductList { items: vec![] }));
      assert_eq!(store.phase(), CatalogPhase::Empty);
      assert_eq!(store.cursor(), None);
    });
  }

  #[test]
  fn a_stale_answer_is_dropped() {
    with_store(|store| {
      let old = store.begin();
      let new = store.begin();
      assert!(store.finish(new, Ok(list())));
      assert!(!store.finish(old, Err(ApiFailure::Network("late".into()))));
      assert_eq!(store.phase(), CatalogPhase::Ready);
    });
  }

  #[test]
  fn the_cursor_moves_and_stops_at_the_ends() {
    with_store(|store| {
      let revision = store.begin();
      store.finish(revision, Ok(list()));
      // The first move goes from the first row, where the cursor starts.
      store.move_cursor(1);
      assert_eq!(store.cursor().as_deref(), Some("pinkb"));
      store.move_cursor(-1);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
      store.move_cursor(-1);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
      store.move_cursor(1);
      assert_eq!(store.cursor().as_deref(), Some("pinkb"));
      store.move_cursor(100);
      assert_eq!(store.cursor().as_deref(), Some("night"));
      assert!(store.is_cursor("night"));
      store.select("geisha");
      assert_eq!(store.cursor().as_deref(), Some("geisha"));
      // A reload keeps the cursor on its bean.
      let revision = store.begin();
      store.finish(revision, Ok(list()));
      assert_eq!(store.cursor().as_deref(), Some("geisha"));
    });
  }

  #[test]
  fn step_clamps_and_falls_back_to_the_first() {
    let products = list().items;
    assert_eq!(step(&[], None, 1), None);
    assert_eq!(step(&products, None, 0).as_deref(), Some("yirga"));
    assert_eq!(step(&products, None, 3).as_deref(), Some("geisha"));
    assert_eq!(step(&products, Some("gone"), 1).as_deref(), Some("pinkb"));
    assert_eq!(step(&products, Some("pinkb"), -5).as_deref(), Some("yirga"));
  }
}
