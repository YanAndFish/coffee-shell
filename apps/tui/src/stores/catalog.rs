//! The catalog: the bean list as the backend filtered, sorted and searched it (P-api-9), the
//! sidebar facets, the query, and the cursor.
//!
//! The list is a [`Remote`]: `idle → loading → ready | empty | failed`, the last list kept
//! while reloading, stale answers dropped (typing a search sends one request per key; only
//! the last answer counts). Any change of the query reloads and puts the cursor back on the
//! first row (02-list.md, "行为"). The cursor is a product id, not an index, so a reload
//! with the same query keeps it on its bean.

use coffee_api::{
  ProductFacets, ProductList, ProductListQuery, ProductSort, ProductSummary, RoastBand,
};
use nsh_core::prelude::*;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// Roast bands in the sidebar's order (`全部` is `None`).
pub const ROAST_BANDS: [RoastBand; 3] = [RoastBand::Light, RoastBand::Medium, RoastBand::Dark];

/// Sort orders in the sidebar's order.
pub const SORTS: [ProductSort; 4] = [
  ProductSort::Recommended,
  ProductSort::PriceAsc,
  ProductSort::PriceDesc,
  ProductSort::Score,
];

/// A roast band's label: `浅` `中` `深` (`全部` for none).
pub fn roast_label(band: Option<RoastBand>) -> &'static str {
  match band {
    None => "全部",
    Some(RoastBand::Light) => "浅",
    Some(RoastBand::Medium) => "中",
    Some(RoastBand::Dark) => "深",
  }
}

/// A sort's label: `推荐` `价格 ↑` `价格 ↓` `杯测分`.
pub fn sort_label(sort: ProductSort) -> &'static str {
  match sort {
    ProductSort::Recommended => "推荐",
    ProductSort::PriceAsc => "价格 ↑",
    ProductSort::PriceDesc => "价格 ↓",
    ProductSort::Score => "杯测分",
  }
}

/// A group of the S-width filter dialog (`筛选 FILTER`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FilterGroup {
  /// `产区`.
  #[default]
  Origin,
  /// `烘焙度`.
  Roast,
  /// `排序`.
  Sort,
}

impl FilterGroup {
  /// The groups in order (`Tab` moves through them).
  pub const ALL: [FilterGroup; 3] = [FilterGroup::Origin, FilterGroup::Roast, FilterGroup::Sort];

  /// The group's title.
  pub fn label(self) -> &'static str {
    match self {
      FilterGroup::Origin => "产区",
      FilterGroup::Roast => "烘焙度",
      FilterGroup::Sort => "排序",
    }
  }
}

/// What the list shows: the filters, the order and the search text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogQuery {
  /// Exact origin (`埃塞俄比亚`); `None` = `全部`.
  pub origin: Option<String>,
  /// Roast band; `None` = `全部`.
  pub roast: Option<RoastBand>,
  /// Order.
  pub sort: ProductSort,
  /// Search text as typed.
  pub search: String,
}

impl CatalogQuery {
  /// The request's query.
  pub fn to_api(&self) -> ProductListQuery {
    ProductListQuery {
      origin: self.origin.clone(),
      roast: self.roast,
      sort: (self.sort != ProductSort::Recommended).then_some(self.sort),
      q: (!self.search.trim().is_empty()).then(|| self.search.trim().to_owned()),
    }
  }

  /// Whether an origin, a band or a search narrows the list (Esc clears them).
  pub fn is_filtered(&self) -> bool {
    self.origin.is_some() || self.roast.is_some() || !self.search.trim().is_empty()
  }

  /// The origin's label: the origin, or `全部`.
  pub fn origin_label(&self) -> &str {
    self.origin.as_deref().unwrap_or("全部")
  }
}

/// The next of `options` after `current` (`None` = `全部` before the first), wrapping to
/// `None` after the last.
pub fn cycle<T: Clone + PartialEq>(options: &[T], current: Option<&T>) -> Option<T> {
  match current.and_then(|c| options.iter().position(|o| o == c)) {
    None => options.first().cloned(),
    Some(i) => options.get(i + 1).cloned(),
  }
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

fn no_items(list: &ProductList) -> bool {
  list.items.is_empty()
}

/// The catalog's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct CatalogStore {
  session: SessionStore,
  query: Signal<CatalogQuery>,
  list: Remote<ProductList>,
  cursor: Signal<Option<SharedString>>,
  /// Cursor moves typed before the first list arrived: they apply to it.
  typed_ahead: Signal<isize>,
  filter_focus: Signal<(FilterGroup, usize)>,
}

impl CatalogStore {
  /// An idle catalog reaching the backend through `session`, owned by the current owner.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      query: signal(CatalogQuery::default()),
      list: Remote::new(no_items),
      cursor: signal(None),
      typed_ahead: signal(0),
      filter_focus: signal((FilterGroup::Origin, 0)),
    }
  }

  /// The list's state (tracked).
  pub fn state(&self) -> RemoteState<ProductList> {
    self.list.state()
  }

  /// The list's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.list.phase()
  }

  /// The failure, when failed (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.list.failure()
  }

  /// The products shown, in order (tracked).
  pub fn products(&self) -> Vec<ProductSummary> {
    self
      .list
      .with(|list| list.map(|l| l.items.clone()).unwrap_or_default())
  }

  /// How many products are shown (tracked).
  pub fn count(&self) -> usize {
    self.list.with(|list| list.map_or(0, |l| l.items.len()))
  }

  /// The product `id`, when shown (tracked).
  pub fn product(&self, id: &str) -> Option<ProductSummary> {
    self
      .list
      .with(|list| list.and_then(|l| l.items.iter().find(|p| p.id == id).cloned()))
  }

  /// The sidebar counts (whatever the filters), once loaded (tracked).
  pub fn facets(&self) -> Option<ProductFacets> {
    self.list.with(|list| list.map(|l| l.facets.clone()))
  }

  /// The origins of the sidebar, in its order (tracked).
  pub fn origins(&self) -> Vec<String> {
    self
      .facets()
      .map(|f| f.origins.into_iter().map(|o| o.origin).collect())
      .unwrap_or_default()
  }

  /// The query (tracked).
  pub fn query(&self) -> CatalogQuery {
    self.query.get()
  }

  // --- The cursor ---------------------------------------------------------------------

  /// The id under the cursor: the selected product, else the first (tracked).
  pub fn cursor(&self) -> Option<SharedString> {
    let selected = self.cursor.get();
    self
      .list
      .with(|list| list.and_then(|l| step(&l.items, selected.as_deref(), 0)))
  }

  /// The product under the cursor (tracked).
  pub fn current(&self) -> Option<ProductSummary> {
    self.cursor().and_then(|id| self.product(&id))
  }

  /// Whether the cursor is on `id` (tracked).
  pub fn is_cursor(&self, id: &str) -> bool {
    self.cursor().is_some_and(|cursor| cursor == id)
  }

  /// Moves the cursor `delta` rows, stopping at the ends. Before the first list arrived
  /// (still loading) the moves wait and apply to it, so keys typed ahead (`g ↓ ↓ ⏎`) land
  /// where they would have.
  pub fn move_cursor(&self, delta: isize) {
    if self.list.with_untracked(|list| list.is_none()) {
      if self.list.phase_untracked().pending() {
        self
          .typed_ahead
          .set(self.typed_ahead.get_untracked() + delta);
      }
      return;
    }
    let selected = self.cursor.get_untracked();
    let next = self
      .list
      .with_untracked(|list| list.and_then(|l| step(&l.items, selected.as_deref(), delta)));
    if next.is_some() && next != selected {
      self.cursor.set(next);
    }
  }

  /// Puts the cursor on `id`.
  pub fn select(&self, id: &str) {
    let id = SharedString::from(id.to_owned());
    if self.cursor.with_untracked(|s| s.as_ref() != Some(&id)) {
      self.cursor.set(Some(id));
    }
  }

  // --- The query ----------------------------------------------------------------------

  fn change(&self, f: impl FnOnce(&mut CatalogQuery)) {
    let mut query = self.query.get_untracked();
    f(&mut query);
    if query != self.query.get_untracked() {
      batch(|| {
        self.query.set(query);
        self.cursor.set(None);
      });
      self.load();
    }
  }

  /// Shows origin `origin` only (`None` = `全部`).
  pub fn set_origin(&self, origin: Option<String>) {
    self.change(|q| q.origin = origin);
  }

  /// Shows roast band `band` only (`None` = `全部`).
  pub fn set_roast(&self, band: Option<RoastBand>) {
    self.change(|q| q.roast = band);
  }

  /// Orders by `sort`.
  pub fn set_sort(&self, sort: ProductSort) {
    self.change(|q| q.sort = sort);
  }

  /// Searches for `text` (as typed; the server trims and lower-cases).
  pub fn set_search(&self, text: &str) {
    self.change(|q| q.search = text.to_owned());
  }

  /// `f`: the next origin (`全部` → each facet origin → `全部`).
  pub fn cycle_origin(&self) {
    let origins = untracked(|| self.origins());
    let current = self.query.with_untracked(|q| q.origin.clone());
    self.set_origin(cycle(&origins, current.as_ref()));
  }

  /// `r`: the next roast band (`全部` → 浅 → 中 → 深 → `全部`).
  pub fn cycle_roast(&self) {
    let current = self.query.with_untracked(|q| q.roast);
    self.set_roast(cycle(&ROAST_BANDS, current.as_ref()));
  }

  /// `s`: the next order (推荐 → 价格 ↑ → 价格 ↓ → 杯测分 → 推荐).
  pub fn cycle_sort(&self) {
    let current = self.query.with_untracked(|q| q.sort);
    self.set_sort(cycle(&SORTS, Some(&current)).unwrap_or_default());
  }

  /// Esc: clears origin, band and search; keeps the order (源稿 `setFilter`).
  pub fn clear_filters(&self) {
    self.change(|q| {
      q.origin = None;
      q.roast = None;
      q.search.clear();
    });
  }

  // --- The S-width filter dialog ------------------------------------------------------

  /// The labels of `group`'s options, `全部` first (tracked: the origins come with the
  /// facets).
  pub fn filter_options(&self, group: FilterGroup) -> Vec<String> {
    match group {
      FilterGroup::Origin => std::iter::once("全部".to_owned())
        .chain(self.origins())
        .collect(),
      FilterGroup::Roast => std::iter::once(None)
        .chain(ROAST_BANDS.map(Some))
        .map(|band| roast_label(band).to_owned())
        .collect(),
      FilterGroup::Sort => SORTS.map(|s| sort_label(s).to_owned()).to_vec(),
    }
  }

  /// The index of `group`'s option the query has now (tracked).
  pub fn filter_selected(&self, group: FilterGroup) -> usize {
    let query = self.query.get();
    match group {
      FilterGroup::Origin => query
        .origin
        .and_then(|o| self.origins().iter().position(|x| *x == o))
        .map_or(0, |i| i + 1),
      FilterGroup::Roast => query
        .roast
        .and_then(|b| ROAST_BANDS.iter().position(|x| *x == b))
        .map_or(0, |i| i + 1),
      FilterGroup::Sort => SORTS.iter().position(|s| *s == query.sort).unwrap_or(0),
    }
  }

  /// The dialog's focus: the group and the option in it (tracked).
  pub fn filter_focus(&self) -> (FilterGroup, usize) {
    self.filter_focus.get()
  }

  /// Opening the dialog: the focus on the origin group's current option.
  pub fn open_filter(&self) {
    let index = untracked(|| self.filter_selected(FilterGroup::Origin));
    self.filter_focus.set((FilterGroup::Origin, index));
  }

  /// `↑` `↓`: the previous / next option of the focused group (no wrap).
  pub fn filter_move(&self, delta: isize) {
    let (group, index) = self.filter_focus.get_untracked();
    let count = untracked(|| self.filter_options(group).len());
    let next = index
      .saturating_add_signed(delta)
      .min(count.saturating_sub(1));
    self.filter_focus.set((group, next));
  }

  /// `Tab`: the next group (wrapping), on its current option.
  pub fn filter_next_group(&self) {
    let (group, _) = self.filter_focus.get_untracked();
    let next = FilterGroup::ALL[(group as usize + 1) % FilterGroup::ALL.len()];
    self.filter_jump(next);
  }

  /// `f` `r` `s`: jumps to `group`, on its current option.
  pub fn filter_jump(&self, group: FilterGroup) {
    let index = untracked(|| self.filter_selected(group));
    self.filter_focus.set((group, index));
  }

  /// `⏎` / `space`: applies the focused option (at once, as the design says).
  pub fn filter_choose(&self) {
    let (group, index) = self.filter_focus.get_untracked();
    match group {
      FilterGroup::Origin => {
        let origin = index
          .checked_sub(1)
          .and_then(|i| untracked(|| self.origins()).get(i).cloned());
        self.set_origin(origin);
      }
      FilterGroup::Roast => {
        self.set_roast(
          index
            .checked_sub(1)
            .and_then(|i| ROAST_BANDS.get(i).copied()),
        );
      }
      FilterGroup::Sort => self.set_sort(SORTS.get(index).copied().unwrap_or_default()),
    }
  }

  // --- Loading ------------------------------------------------------------------------

  /// Starts a load: the revision its answer must carry.
  pub fn begin(&self) -> u64 {
    self.list.begin()
  }

  /// Applies the answer of load `revision`; an older load's answer is dropped. Whether it
  /// applied.
  pub fn finish(&self, revision: u64, result: Result<ProductList, ApiFailure>) -> bool {
    batch(|| {
      let applied = self.list.finish(revision, result);
      let ahead = self.typed_ahead.get_untracked();
      if applied && ahead != 0 {
        self.typed_ahead.set(0);
        self.move_cursor(ahead);
      }
      applied
    })
  }

  /// Loads the list for the current query: what [`Self::load`] runs (await it in tests).
  pub async fn fetch(self) {
    let query = self.query.with_untracked(CatalogQuery::to_api);
    let client = self.session.client();
    let revision = self.begin();
    let result = client.list_products(&query).await;
    self.finish(revision, result);
  }

  /// Loads the list in a task of the root's owner (a page going away does not cancel
  /// it); a newer load supersedes one under way.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Loads the list if nothing was asked yet.
  pub fn ensure_loaded(&self) {
    if self.list.is_idle() {
      self.load();
    }
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  const PRODUCTS: &str = include_str!("../../../../packages/contract/fixtures/v1/products.json");

  fn list() -> ProductList {
    serde_json::from_str(PRODUCTS).unwrap()
  }

  fn with_store(set: FixtureSet, test: impl FnOnce(CatalogStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, store) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(Transport::Fixture(set), None));
      CatalogStore::new(session)
    });
    test(store);
    owner.dispose();
  }

  #[test]
  fn a_load_goes_through_loading_to_ready() {
    with_store(FixtureSet::Contract, |store| {
      assert_eq!(store.phase(), Phase::Idle);
      let revision = store.begin();
      assert_eq!(store.phase(), Phase::Loading);
      assert!(store.finish(revision, Ok(list())));
      assert_eq!(store.phase(), Phase::Ready);
      assert_eq!(store.count(), 8);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
      assert_eq!(store.origins().len(), 8);
      assert_eq!(
        store.current().map(|p| p.name),
        Some("耶加雪菲 孔加".into())
      );
    });
  }

  #[test]
  fn empty_and_failed_and_retry() {
    with_store(FixtureSet::Empty, |store| {
      now_or_never(store.fetch());
      assert_eq!(store.phase(), Phase::Empty);
      assert_eq!(store.cursor(), None);
      assert_eq!(
        store.facets().map(|f| f.total),
        Some(8),
        "facets count everything"
      );
    });
    with_store(FixtureSet::Down, |store| {
      now_or_never(store.fetch());
      assert_eq!(store.phase(), Phase::Failed);
      assert!(store.failure().is_some_and(|f| f.is_network()));
    });
  }

  #[test]
  fn a_stale_answer_is_dropped() {
    with_store(FixtureSet::Contract, |store| {
      let old = store.begin();
      let new = store.begin();
      assert!(store.finish(new, Ok(list())));
      assert!(!store.finish(old, Err(ApiFailure::Network("late".into()))));
      assert_eq!(store.phase(), Phase::Ready);
    });
  }

  #[test]
  fn the_cursor_moves_and_stops_at_the_ends() {
    with_store(FixtureSet::Contract, |store| {
      now_or_never(store.fetch());
      store.move_cursor(1);
      assert_eq!(store.cursor().as_deref(), Some("pinkb"));
      store.move_cursor(-1);
      store.move_cursor(-1);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
      store.move_cursor(100);
      assert_eq!(store.cursor().as_deref(), Some("night"));
      store.select("geisha");
      assert!(store.is_cursor("geisha"));
      // A reload with the same query keeps the cursor on its bean.
      now_or_never(store.fetch());
      assert_eq!(store.cursor().as_deref(), Some("geisha"));
    });
  }

  #[test]
  fn moves_typed_before_the_first_list_apply_to_it() {
    with_store(FixtureSet::Contract, |store| {
      // `g ↓ ↓`: the list is still on its way.
      let revision = store.begin();
      store.move_cursor(1);
      store.move_cursor(1);
      assert_eq!(store.cursor(), None);
      let list =
        crate::api::now_or_never(store.session.client().list_products(&Default::default()));
      assert!(store.finish(revision, list));
      assert_eq!(store.cursor().as_deref(), Some("nyeri"), "the third bean");
      // Nothing is left over for the next list.
      now_or_never(store.fetch());
      assert_eq!(store.cursor().as_deref(), Some("nyeri"));
    });
  }

  #[test]
  fn the_query_cycles_filters_and_resets_the_cursor() {
    with_store(FixtureSet::Contract, |store| {
      now_or_never(store.fetch());
      store.select("geisha");
      // Changing the query asks for a new list (spawned; here, fetched by hand).
      store.query.set(CatalogQuery {
        roast: Some(RoastBand::Light),
        sort: ProductSort::PriceAsc,
        ..CatalogQuery::default()
      });
      store.cursor.set(None);
      now_or_never(store.fetch());
      let ids: Vec<String> = store.products().into_iter().map(|p| p.id).collect();
      assert_eq!(ids, ["yirga", "nyeri", "pinkb", "geisha"]);
      assert_eq!(store.cursor().as_deref(), Some("yirga"));
    });
    assert_eq!(cycle(&ROAST_BANDS, None), Some(RoastBand::Light));
    assert_eq!(
      cycle(&ROAST_BANDS, Some(&RoastBand::Dark)),
      None,
      "after the last comes 全部"
    );
    assert_eq!(
      cycle(&SORTS, Some(&ProductSort::Score)),
      None,
      "the sort wraps through None → Recommended"
    );
    let query = CatalogQuery {
      origin: Some("肯尼亚".into()),
      sort: ProductSort::Score,
      search: "  ".into(),
      ..CatalogQuery::default()
    };
    assert!(query.is_filtered());
    assert_eq!(
      query.to_api(),
      ProductListQuery {
        origin: Some("肯尼亚".into()),
        roast: None,
        sort: Some(ProductSort::Score),
        q: None,
      }
    );
    assert_eq!(roast_label(None), "全部");
    assert_eq!(sort_label(ProductSort::PriceDesc), "价格 ↓");
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
