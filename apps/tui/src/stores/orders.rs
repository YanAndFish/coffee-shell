//! Orders (06-profile.md, `最近订单 ORDERS`): the member's recent orders, newest first, and
//! one order by number. Read-only; a checkout makes the list stale ([`OrdersStore::invalidate`]).

use coffee_api::{Order, OrderList, OrderSummary};
use nsh_core::prelude::*;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// How many orders the profile asks for.
pub const ORDER_LIMIT: u8 = 20;

fn no_items(list: &OrderList) -> bool {
  list.items.is_empty()
}

/// The orders' state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct OrdersStore {
  session: SessionStore,
  list: Remote<OrderList>,
  order: Remote<Order>,
}

impl OrdersStore {
  /// Nothing loaded, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      list: Remote::new(no_items),
      order: Remote::new(|_| false),
    }
  }

  /// The list's state (tracked).
  pub fn state(&self) -> RemoteState<OrderList> {
    self.list.state()
  }

  /// The list's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.list.phase()
  }

  /// The failure of the last load (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.list.failure()
  }

  /// The orders, newest first (tracked).
  pub fn items(&self) -> Vec<OrderSummary> {
    self
      .list
      .with(|list| list.map(|l| l.items.clone()).unwrap_or_default())
  }

  /// How many orders the member has in all (tracked).
  pub fn total(&self) -> u32 {
    self.list.with(|list| list.map_or(0, |l| l.total))
  }

  /// Loads the list. Await it in tests.
  pub async fn fetch(self) {
    let client = self.session.client();
    let revision = self.list.begin();
    let result = client.orders(Some(ORDER_LIMIT)).await;
    self.list.finish(revision, result);
  }

  /// Loads the list in a task of the root's owner.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Loads the list if nothing was asked yet (or it went stale).
  pub fn ensure_loaded(&self) {
    if self.list.is_idle() {
      self.load();
    }
  }

  /// The list is stale (a new order): the next [`Self::ensure_loaded`] reloads it.
  pub fn invalidate(&self) {
    self.list.reset();
  }

  /// One order's state (tracked).
  pub fn order_state(&self) -> RemoteState<Order> {
    self.order.state()
  }

  /// Loads order `number`. Await it in tests.
  pub async fn fetch_order(self, number: String) {
    let client = self.session.client();
    let revision = self.order.begin();
    let result = client.order(&number).await;
    self.order.finish(revision, result);
  }

  /// Loads order `number` in a task of the root's owner.
  pub fn open_order(&self, number: &str) {
    let store = *self;
    let number = number.to_owned();
    self.session.spawn(store.fetch_order(number));
  }

  /// Forgets everything (logout).
  pub fn reset(&self) {
    batch(|| {
      self.list.reset();
      self.order.reset();
    });
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, ErrorCode, FixtureSet, Transport, now_or_never};

  #[test]
  fn orders_load_and_need_a_member() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, (session, orders)) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(
        Transport::Fixture(FixtureSet::Contract),
        None,
      ));
      (session, OrdersStore::new(session))
    });
    now_or_never(orders.fetch());
    assert!(
      orders
        .failure()
        .is_some_and(|f| f.is(ErrorCode::LoginRequired))
    );
    let auth = serde_json::from_str(include_str!(
      "../../../../packages/contract/fixtures/v1/auth/login.json"
    ))
    .unwrap();
    session.signed_in(auth);
    now_or_never(orders.fetch());
    assert_eq!(orders.phase(), Phase::Ready);
    assert_eq!(orders.items()[0].number, "RT2611220417");
    assert_eq!(orders.total(), 4);
    now_or_never(orders.fetch_order("RT2611060117".into()));
    assert_eq!(orders.order_state().phase(), Phase::Ready);
    orders.invalidate();
    assert_eq!(orders.phase(), Phase::Idle);
    owner.dispose();
  }
}
