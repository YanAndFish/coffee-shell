//! The cart (04-cart.md): a member's cart through the API, a guest's held in memory and
//! priced by `POST /v1/cart/quote` (P-cart-7), merged into the account right after login
//! (P-cart-8), and checkout with its receipt.
//!
//! **The TUI never computes money**: every amount shown is a field of the last [`Cart`] the
//! server answered (line totals, item counts, subtotal, discount, shipping, free-shipping
//! progress). After each guest change the guest lines are re-quoted, and the quote's lines
//! become the guest lines (the server merges equal SKUs and caps quantities).
//!
//! The async operations are `async fn`s returning the outcome; [`super::actions`] spawns them
//! and turns outcomes into toasts and navigation. A mutation keeps the cart on screen while
//! it runs ([`Remote::begin_quiet`]); a newer one supersedes an older answer; a failed one
//! changes nothing (a guest change is rolled back).

use coffee_api::{
  AppliedCoupon, Cart, CartLine, CartMergeRequest, CartQuoteRequest, ErrorCode, GuestCartLine,
  Order, PlaceOrderRequest, PlacedOrder, UpdateCartLineRequest,
};
use nsh_core::prelude::*;

use super::{
  product::add_request,
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// The most units of one SKU a line holds (U7).
pub const MAX_LINE_QUANTITY: u8 = 9;

/// The guest's cart, as the TUI holds it until login.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuestCart {
  /// The lines, in order.
  pub lines: Vec<GuestCartLine>,
  /// The coupon typed, sent with every quote.
  pub coupon: Option<String>,
}

/// The last deleted line, for `u 撤销` (only the latest is kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed {
  /// The line as it was.
  pub line: GuestCartLine,
  /// Where it was (it goes back there).
  pub index: u32,
  /// Its product's name, for `已删除 {商品名} · u 撤销`.
  pub name: String,
}

/// What the coupon row says under the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CouponFeedback {
  /// `✓ 已使用 TTY10 · 全单 9 折`.
  Applied(AppliedCoupon),
  /// `✗ 优惠码无效或已过期`.
  Invalid(String),
}

/// The checkout's progress (`校验库存` → `锁定烘焙批次` → `生成订单` → `完成`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckoutStep {
  /// `校验库存`: the local checks passed.
  Verify,
  /// `锁定烘焙批次`: the order request is out.
  Lock,
  /// `生成订单`: the server answered.
  Create,
  /// `完成`.
  Done,
}

impl CheckoutStep {
  /// The step's copy.
  pub fn label(self) -> &'static str {
    match self {
      CheckoutStep::Verify => "校验库存",
      CheckoutStep::Lock => "锁定烘焙批次",
      CheckoutStep::Create => "生成订单",
      CheckoutStep::Done => "完成",
    }
  }
}

/// What `c` finds before placing an order (04-cart.md, "c / ⏎ 结算，依次检查").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckoutCheck {
  /// A guest: log in first (and come back here).
  Guest,
  /// Nothing selected.
  NothingSelected,
  /// No order address.
  NoAddress,
  /// Ready to place the order.
  Ready,
}

/// The checks of `cart` for a member (`is_member`) or a guest.
pub fn checkout_check(is_member: bool, cart: Option<&Cart>) -> CheckoutCheck {
  if !is_member {
    return CheckoutCheck::Guest;
  }
  match cart {
    Some(cart) if cart.totals.selected_count == 0 => CheckoutCheck::NothingSelected,
    None => CheckoutCheck::NothingSelected,
    Some(cart) if cart.address.is_none() => CheckoutCheck::NoAddress,
    Some(_) => CheckoutCheck::Ready,
  }
}

/// The guest line a cart line is.
pub fn as_guest_line(line: &CartLine) -> GuestCartLine {
  GuestCartLine {
    product_id: line.product_id.clone(),
    weight: line.weight,
    grind: line.grind,
    pack: line.pack,
    quantity: line.quantity,
    selected: line.selected,
  }
}

/// Whether `a` and `b` are the same SKU.
pub fn same_sku(a: &GuestCartLine, b: &GuestCartLine) -> bool {
  a.product_id == b.product_id && a.weight == b.weight && a.grind == b.grind && a.pack == b.pack
}

/// `lines` with `line` added: an equal SKU adds up (at most 9) and is selected again
/// (P-cart-2), else it goes at the end.
pub fn add_line(mut lines: Vec<GuestCartLine>, line: GuestCartLine) -> Vec<GuestCartLine> {
  match lines.iter_mut().find(|l| same_sku(l, &line)) {
    Some(existing) => {
      existing.quantity = existing
        .quantity
        .saturating_add(line.quantity)
        .min(MAX_LINE_QUANTITY);
      existing.selected = true;
    }
    None => lines.push(line),
  }
  lines
}

/// The id of a new checkout attempt (8–64 of `[A-Za-z0-9_-]`, P-order-2).
pub fn idempotency_key(attempt: u64) -> String {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |d| d.as_nanos());
  format!("tty-{nanos:x}-{attempt}")
}

fn no_lines(cart: &Cart) -> bool {
  cart.lines.is_empty()
}

/// The cart's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct CartStore {
  session: SessionStore,
  cart: Remote<Cart>,
  guest: Signal<GuestCart>,
  cursor: Signal<Option<String>>,
  undo: Signal<Option<Removed>>,
  coupon_input: Signal<String>,
  coupon_feedback: Signal<Option<CouponFeedback>>,
  checkout: Signal<Option<CheckoutStep>>,
  attempt: Signal<(u64, Option<String>)>,
  receipt: Signal<Option<Order>>,
}

impl CartStore {
  /// An empty guest cart reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      cart: Remote::new(no_lines),
      guest: signal(GuestCart::default()),
      cursor: signal(None),
      undo: signal(None),
      coupon_input: signal(String::new()),
      coupon_feedback: signal(None),
      checkout: signal(None),
      attempt: signal((0, None)),
      receipt: signal(None),
    }
  }

  fn member(&self) -> bool {
    self.session.is_member_untracked()
  }

  // --- Reading ------------------------------------------------------------------------

  /// The cart's state (tracked): the member cart or the guest quote.
  pub fn state(&self) -> RemoteState<Cart> {
    self.cart.state()
  }

  /// The cart's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.cart.phase()
  }

  /// The failure of the last load, when failed (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.cart.failure()
  }

  /// The cart as the server priced it (tracked).
  pub fn cart(&self) -> Option<Cart> {
    self.cart.data()
  }

  /// The lines (tracked).
  pub fn lines(&self) -> Vec<CartLine> {
    self
      .cart
      .with(|cart| cart.map(|c| c.lines.clone()).unwrap_or_default())
  }

  /// Units in the cart: the nav badge and `购物车 · {n} 件` (tracked; 0 before any answer).
  pub fn item_count(&self) -> u32 {
    self
      .cart
      .with(|cart| cart.map_or(0, |c| c.totals.item_count))
  }

  /// Selected units: `c 结算 {n} 件` (tracked).
  pub fn selected_count(&self) -> u32 {
    self
      .cart
      .with(|cart| cart.map_or(0, |c| c.totals.selected_count))
  }

  /// `合计`, as the server computed it (tracked; 0 before any answer).
  pub fn total(&self) -> u32 {
    self.cart.with(|cart| cart.map_or(0, |c| c.totals.total))
  }

  /// The guest's lines as held in memory (tracked).
  pub fn guest(&self) -> GuestCart {
    self.guest.get()
  }

  /// The SKU under the cursor: the chosen line, else the first (tracked).
  pub fn cursor(&self) -> Option<String> {
    let chosen = self.cursor.get();
    self.cart.with(|cart| {
      let lines = &cart?.lines;
      chosen
        .filter(|sku| lines.iter().any(|l| &l.sku == sku))
        .or_else(|| lines.first().map(|l| l.sku.clone()))
    })
  }

  /// The line under the cursor (tracked).
  pub fn cursor_line(&self) -> Option<CartLine> {
    let sku = self.cursor()?;
    self.lines().into_iter().find(|l| l.sku == sku)
  }

  /// Whether the cursor is on `sku` (tracked).
  pub fn is_cursor(&self, sku: &str) -> bool {
    self.cursor().as_deref() == Some(sku)
  }

  /// Moves the cursor `delta` lines, stopping at the ends.
  pub fn move_cursor(&self, delta: isize) {
    let current = untracked(|| self.cursor());
    let next = self.cart.with_untracked(|cart| {
      let lines = &cart?.lines;
      let last = lines.len().checked_sub(1)?;
      let at = current
        .and_then(|sku| lines.iter().position(|l| l.sku == sku))
        .unwrap_or(0);
      Some(lines[at.saturating_add_signed(delta).min(last)].sku.clone())
    });
    if next.is_some() {
      self.cursor.set(next);
    }
  }

  /// Puts the cursor on `sku`.
  pub fn select_line(&self, sku: &str) {
    self.cursor.set(Some(sku.to_owned()));
  }

  /// The last deleted line, while it can be restored (tracked).
  pub fn undo(&self) -> Option<Removed> {
    self.undo.get()
  }

  /// The coupon field's text (tracked).
  pub fn coupon_input(&self) -> String {
    self.coupon_input.get()
  }

  /// Sets the coupon field's text (clears the feedback line).
  pub fn set_coupon_input(&self, text: &str) {
    batch(|| {
      self.coupon_input.set(text.to_owned());
      self.coupon_feedback.set(None);
    });
  }

  /// The line under the coupon field (tracked).
  pub fn coupon_feedback(&self) -> Option<CouponFeedback> {
    self.coupon_feedback.get()
  }

  /// The checkout's progress; `None` when not checking out (tracked).
  pub fn checkout_step(&self) -> Option<CheckoutStep> {
    self.checkout.get()
  }

  /// Whether a checkout is under way (tracked): only `?` works meanwhile.
  pub fn submitting(&self) -> bool {
    self.checkout.with(Option::is_some)
  }

  /// The placed order, while its receipt shows (tracked).
  pub fn receipt(&self) -> Option<Order> {
    self.receipt.get()
  }

  /// Whether the receipt shows (tracked).
  pub fn has_receipt(&self) -> bool {
    self.receipt.with(Option::is_some)
  }

  /// Hides the receipt (leaving the cart page).
  pub fn clear_receipt(&self) {
    if self.receipt.with_untracked(Option::is_some) {
      self.receipt.set(None);
    }
  }

  /// What `c` would find now (untracked).
  pub fn checkout_check(&self) -> CheckoutCheck {
    let member = self.member();
    self
      .cart
      .with_untracked(|cart| checkout_check(member, cart))
  }

  // --- Loading ------------------------------------------------------------------------

  /// Loads the cart: the member cart, or the guest quote. Await it in tests.
  pub async fn fetch(self) {
    let client = self.session.client();
    let revision = self.cart.begin();
    let result = if self.member() {
      client.cart().await
    } else {
      client.quote_cart(&self.quote_request(None)).await
    };
    if let Ok(cart) = &result
      && !self.member()
    {
      self.adopt_quote(cart);
    }
    self.cart.finish(revision, result);
  }

  /// Loads the cart in a task of the root's owner.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Loads the cart if nothing was asked yet.
  pub fn ensure_loaded(&self) {
    if self.cart.is_idle() {
      self.load();
    }
  }

  fn quote_request(&self, coupon: Option<Option<String>>) -> CartQuoteRequest {
    self.guest.with_untracked(|guest| CartQuoteRequest {
      lines: guest.lines.clone(),
      coupon_code: coupon.unwrap_or_else(|| guest.coupon.clone()),
    })
  }

  /// The quote's lines become the guest lines (the server's merging and caps win).
  fn adopt_quote(&self, cart: &Cart) {
    let lines = cart.lines.iter().map(as_guest_line).collect();
    self.guest.update(|guest| {
      guest.lines = lines;
      if cart.coupon.is_none() {
        guest.coupon = None;
      }
    });
  }

  /// Re-quotes the guest cart with `next` lines: applied on success, rolled back on failure.
  async fn requote(self, next: Vec<GuestCartLine>) -> Result<Cart, ApiFailure> {
    let before = self.guest.get_untracked();
    self.guest.update(|guest| guest.lines = next);
    let client = self.session.client();
    let revision = self.cart.begin_quiet();
    let result = client.quote_cart(&self.quote_request(None)).await;
    match &result {
      Ok(cart) if self.cart.is_current(revision) => {
        self.adopt_quote(cart);
        self.cart.set(cart.clone());
      }
      Ok(_) => {}
      Err(_) => {
        if self.cart.is_current(revision) {
          self.guest.set(before);
        }
      }
    }
    result
  }

  /// Applies a member mutation's answer if it is still the latest.
  fn apply(&self, revision: u64, result: Result<Cart, ApiFailure>) -> Result<Cart, ApiFailure> {
    if let Ok(cart) = &result {
      self.cart.finish_quiet(revision, Ok(cart.clone()));
    }
    result
  }

  // --- Changing it --------------------------------------------------------------------

  /// Adds `line` (quick add, the detail page's `a` / `b`): an equal SKU adds up.
  pub async fn add(self, line: GuestCartLine) -> Result<Cart, ApiFailure> {
    if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = client.add_cart_line(&add_request(&line, None)).await;
      self.apply(revision, result)
    } else {
      let next = self
        .guest
        .with_untracked(|g| add_line(g.lines.clone(), line));
      self.requote(next).await
    }
  }

  fn line(&self, sku: &str) -> Option<(usize, CartLine)> {
    self.cart.with_untracked(|cart| {
      let lines = &cart?.lines;
      lines
        .iter()
        .position(|l| l.sku == sku)
        .map(|i| (i, lines[i].clone()))
    })
  }

  fn guest_lines_with(
    &self,
    sku: &str,
    f: impl FnOnce(&mut GuestCartLine),
  ) -> Option<Vec<GuestCartLine>> {
    let (_, line) = self.line(sku)?;
    let target = as_guest_line(&line);
    let mut lines = self.guest.get_untracked().lines;
    match lines.iter_mut().find(|l| same_sku(l, &target)) {
      Some(existing) => f(existing),
      None => {
        let mut added = target;
        f(&mut added);
        lines.push(added);
      }
    }
    Some(lines)
  }

  /// Sets line `sku`'s quantity (clamped to 1 … its maxQuantity).
  pub async fn set_quantity(self, sku: String, quantity: u8) -> Result<Cart, ApiFailure> {
    let max = self
      .line(&sku)
      .map_or(MAX_LINE_QUANTITY, |(_, l)| l.max_quantity.max(1));
    let quantity = quantity.clamp(1, max);
    if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let request = UpdateCartLineRequest {
        quantity: Some(quantity),
        selected: None,
      };
      let result = client.update_cart_line(&sku, &request).await;
      self.apply(revision, result)
    } else {
      match self.guest_lines_with(&sku, |l| l.quantity = quantity) {
        Some(next) => self.requote(next).await,
        None => Err(ApiFailure::of(ErrorCode::NotFound)),
      }
    }
  }

  /// `←` `→`: line `sku`'s quantity ±1.
  pub async fn step_quantity(self, sku: String, delta: i8) -> Result<Cart, ApiFailure> {
    let current = self.line(&sku).map_or(1, |(_, l)| l.quantity);
    let next = current.saturating_add_signed(delta).max(1);
    self.set_quantity(sku, next).await
  }

  /// `space`: selects or unselects line `sku`.
  pub async fn toggle_selected(self, sku: String) -> Result<Cart, ApiFailure> {
    let selected = !self.line(&sku).is_some_and(|(_, l)| l.selected);
    if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let request = UpdateCartLineRequest {
        quantity: None,
        selected: Some(selected),
      };
      let result = client.update_cart_line(&sku, &request).await;
      self.apply(revision, result)
    } else {
      match self.guest_lines_with(&sku, |l| l.selected = selected) {
        Some(next) => self.requote(next).await,
        None => Err(ApiFailure::of(ErrorCode::NotFound)),
      }
    }
  }

  /// `a`: selects every line, or none when all are selected.
  pub async fn toggle_all(self) -> Result<Cart, ApiFailure> {
    let all = self.cart.with_untracked(|cart| {
      cart.is_some_and(|c| !c.lines.is_empty() && c.lines.iter().all(|l| l.selected))
    });
    let selected = !all;
    if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = client.set_cart_selection(selected).await;
      self.apply(revision, result)
    } else {
      let mut next = self.guest.get_untracked().lines;
      for line in &mut next {
        line.selected = selected;
      }
      self.requote(next).await
    }
  }

  /// `x` / `d`: removes line `sku`, keeping it for `u 撤销`; the cursor stays where it was
  /// (or on the line above).
  pub async fn remove(self, sku: String) -> Result<Cart, ApiFailure> {
    let Some((index, line)) = self.line(&sku) else {
      return Err(ApiFailure::of(ErrorCode::NotFound));
    };
    let result = if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = client.remove_cart_line(&sku).await;
      self.apply(revision, result)
    } else {
      let target = as_guest_line(&line);
      let mut next = self.guest.get_untracked().lines;
      next.retain(|l| !same_sku(l, &target));
      self.requote(next).await
    };
    if let Ok(cart) = &result {
      let cursor = cart
        .lines
        .get(index)
        .or_else(|| index.checked_sub(1).and_then(|i| cart.lines.get(i)))
        .map(|l| l.sku.clone());
      batch(|| {
        self.undo.set(Some(Removed {
          line: as_guest_line(&line),
          index: u32::try_from(index).unwrap_or(u32::MAX),
          name: line.name.clone(),
        }));
        self.cursor.set(cursor);
      });
    }
    result
  }

  /// `u`: puts the last deleted line back where it was. The restored line, by name.
  pub async fn restore(self) -> Result<String, ApiFailure> {
    let Some(removed) = self.undo.get_untracked() else {
      return Err(ApiFailure::of(ErrorCode::NotFound));
    };
    let result = if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let request = add_request(&removed.line, Some(removed.index));
      let result = client.add_cart_line(&request).await;
      self.apply(revision, result)
    } else {
      let mut next = self.guest.get_untracked().lines;
      let at = usize::try_from(removed.index)
        .unwrap_or(usize::MAX)
        .min(next.len());
      next.insert(at, removed.line.clone());
      self.requote(next).await
    };
    result.map(|_| {
      self.undo.set(None);
      removed.name
    })
  }

  /// Applies the coupon field's code (empty removes the coupon). A bad code leaves the cart
  /// as it was and says `✗ 优惠码无效或已过期`.
  pub async fn apply_coupon(self) -> Result<Cart, ApiFailure> {
    let code = self.coupon_input.get_untracked().trim().to_owned();
    let result = if self.member() {
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = client.apply_coupon(&code).await;
      self.apply(revision, result)
    } else {
      let coupon = (!code.is_empty()).then(|| code.clone());
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = client
        .quote_cart(&self.quote_request(Some(coupon.clone())))
        .await;
      if let Ok(cart) = &result
        && self.cart.is_current(revision)
      {
        self.guest.update(|guest| guest.coupon = coupon);
        self.adopt_quote(cart);
        self.cart.set(cart.clone());
      }
      result
    };
    let feedback = match &result {
      Ok(cart) => cart.coupon.clone().map(CouponFeedback::Applied),
      Err(failure) if failure.is(ErrorCode::CouponInvalid) => {
        Some(CouponFeedback::Invalid(failure.message()))
      }
      Err(_) => None,
    };
    self.coupon_feedback.set(feedback);
    result
  }

  /// Sets 本单寄往 (members).
  pub async fn set_address(self, address_id: u32) -> Result<Cart, ApiFailure> {
    let client = self.session.client();
    let revision = self.cart.begin_quiet();
    let result = client.set_cart_address(address_id).await;
    self.apply(revision, result)
  }

  /// Right after login: merges the guest cart into the account (or loads the member cart
  /// when the guest had none). The units merged, for `已合并访客购物车 {n} 件` (0 = no
  /// merge toast).
  pub async fn merge_guest(self) -> Result<u32, ApiFailure> {
    let guest = self.guest.get_untracked();
    let client = self.session.client();
    let revision = self.cart.begin();
    if guest.lines.is_empty() {
      let result = client.cart().await;
      let outcome = result.as_ref().map(|_| 0).map_err(Clone::clone);
      self.cart.finish(revision, result);
      return outcome;
    }
    let request = CartMergeRequest {
      lines: guest.lines,
      coupon_code: guest.coupon,
    };
    match client.merge_cart(&request).await {
      Ok(merged) => {
        batch(|| {
          self.guest.set(GuestCart::default());
          self.cart.finish(revision, Ok(merged.cart));
          self.undo.set(None);
        });
        Ok(merged.merged_count)
      }
      Err(failure) => {
        self.cart.finish(revision, Err(failure.clone()));
        Err(failure)
      }
    }
  }

  /// Starts a checkout: the step is `校验库存`. `None` while one is under way.
  pub fn begin_checkout(&self) -> Option<()> {
    if self.checkout.with_untracked(Option::is_some) {
      return None;
    }
    self.checkout.set(Some(CheckoutStep::Verify));
    Some(())
  }

  /// Places the order (after [`Self::begin_checkout`] and the checks). On success the
  /// receipt shows and the cart is what the server left; on failure the cart is as it was
  /// (a network failure keeps the attempt's key, so a retry cannot order twice).
  pub async fn place_order(self) -> Result<PlacedOrder, ApiFailure> {
    let (attempt, key) = self.attempt.get_untracked();
    let key = key.unwrap_or_else(|| idempotency_key(attempt + 1));
    self.attempt.set((attempt + 1, Some(key.clone())));
    let expected_total = self
      .cart
      .with_untracked(|cart| cart.map(|c| c.totals.total));
    self.checkout.set(Some(CheckoutStep::Lock));
    let client = self.session.client();
    let request = PlaceOrderRequest {
      idempotency_key: key,
      expected_total,
    };
    let result = client.place_order(&request).await;
    match &result {
      Ok(placed) => {
        self.checkout.set(Some(CheckoutStep::Create));
        batch(|| {
          self.cart.set(placed.cart.clone());
          self.receipt.set(Some(placed.order.clone()));
          self.undo.set(None);
          self.coupon_input.set(String::new());
          self.coupon_feedback.set(None);
          self.attempt.set((attempt + 1, None));
          self.checkout.set(None);
        });
      }
      Err(failure) => batch(|| {
        self.checkout.set(None);
        if !failure.is_network() {
          self.attempt.set((attempt + 1, None));
        }
      }),
    }
    result
  }

  /// Ends a checkout that did not reach the server (a check failed).
  pub fn cancel_checkout(&self) {
    self.checkout.set(None);
  }

  /// Back to an empty guest cart (logout, expiry: the member's cart stays in the account).
  pub fn reset(&self) {
    batch(|| {
      self.cart.reset();
      self.guest.set(GuestCart::default());
      self.cursor.set(None);
      self.undo.set(None);
      self.coupon_input.set(String::new());
      self.coupon_feedback.set(None);
      self.checkout.set(None);
      self.receipt.set(None);
    });
  }
}

#[cfg(test)]
mod tests {
  use coffee_api::{AuthSession, Grind, LoginRequest, Pack, Weight};
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  fn with_cart(set: FixtureSet, test: impl FnOnce(SessionStore, CartStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, (session, cart)) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(Transport::Fixture(set), None));
      (session, CartStore::new(session))
    });
    test(session, cart);
    owner.dispose();
  }

  fn sign_in(session: SessionStore) {
    let request = LoginRequest {
      username: "林晓".into(),
      password: "secret1".into(),
      remember: false,
    };
    let auth: AuthSession = now_or_never(session.client().login(&request)).unwrap();
    session.signed_in(auth);
  }

  fn line(product: &str, quantity: u8) -> GuestCartLine {
    GuestCartLine {
      product_id: product.into(),
      weight: Weight::G250,
      grind: Grind::PourOver,
      pack: Pack::Standard,
      quantity,
      selected: true,
    }
  }

  #[test]
  fn equal_skus_add_up_to_nine_and_are_selected_again() {
    let mut unselected = line("yirga", 5);
    unselected.selected = false;
    let lines = add_line(vec![unselected], line("yirga", 7));
    assert_eq!(lines.len(), 1);
    assert_eq!((lines[0].quantity, lines[0].selected), (9, true));
    let lines = add_line(lines, line("pinkb", 1));
    assert_eq!(lines.len(), 2);
  }

  #[test]
  fn checkout_checks_in_order() {
    let cart: Cart = serde_json::from_str(include_str!(
      "../../../../packages/contract/fixtures/v1/cart/cart.json"
    ))
    .unwrap();
    assert_eq!(checkout_check(false, Some(&cart)), CheckoutCheck::Guest);
    assert_eq!(checkout_check(true, Some(&cart)), CheckoutCheck::Ready);
    let mut none = cart.clone();
    none.totals.selected_count = 0;
    assert_eq!(
      checkout_check(true, Some(&none)),
      CheckoutCheck::NothingSelected
    );
    let mut nowhere = cart;
    nowhere.address = None;
    assert_eq!(
      checkout_check(true, Some(&nowhere)),
      CheckoutCheck::NoAddress
    );
    let key = idempotency_key(3);
    assert!((8..=64).contains(&key.len()) && key.ends_with("-3"));
  }

  #[test]
  fn a_guest_cart_is_held_locally_and_priced_by_the_quote() {
    with_cart(FixtureSet::Contract, |_, cart| {
      assert_eq!(cart.item_count(), 0, "no answer yet: no badge");
      now_or_never(cart.fetch());
      assert_eq!(cart.phase(), Phase::Empty);
      let quoted = now_or_never(cart.add(line("yirga", 2))).unwrap();
      assert_eq!(quoted.totals.item_count, 2);
      assert_eq!(cart.item_count(), 2, "the badge follows the quote");
      assert_eq!(cart.total(), 208, "the server's total, not computed");
      assert_eq!(cart.guest().lines, vec![line("yirga", 2)]);
      assert_eq!(cart.cursor().as_deref(), Some("RT-YIR-250G-PO-STD"));
      // A failing quote rolls the guest lines back.
      let unknown = now_or_never(cart.add(line("nope", 1))).unwrap_err();
      assert!(unknown.is(ErrorCode::NotFound));
      assert_eq!(cart.guest().lines, vec![line("yirga", 2)]);
      assert_eq!(cart.item_count(), 2);
    });
  }

  #[test]
  fn guest_coupons_ride_on_the_quote() {
    with_cart(FixtureSet::Contract, |_, cart| {
      now_or_never(cart.add(line("yirga", 2))).unwrap();
      cart.set_coupon_input("nope");
      assert!(now_or_never(cart.apply_coupon()).is_err());
      assert_eq!(
        cart.coupon_feedback(),
        Some(CouponFeedback::Invalid("优惠码无效或已过期".into()))
      );
      assert_eq!(cart.guest().coupon, None);
      cart.set_coupon_input("");
      assert_eq!(cart.coupon_feedback(), None, "typing clears the feedback");
    });
  }

  #[test]
  fn remove_then_undo_for_a_guest() {
    with_cart(FixtureSet::Contract, |_, cart| {
      now_or_never(cart.add(line("yirga", 2))).unwrap();
      now_or_never(cart.remove("RT-YIR-250G-PO-STD".into())).unwrap();
      assert_eq!(cart.item_count(), 0);
      assert!(cart.guest().lines.is_empty());
      let removed = cart.undo().unwrap();
      assert_eq!((removed.name.as_str(), removed.index), ("耶加雪菲 孔加", 0));
      assert_eq!(now_or_never(cart.restore()).unwrap(), "耶加雪菲 孔加");
      assert_eq!(cart.undo(), None);
      assert_eq!(cart.item_count(), 2);
    });
  }

  #[test]
  fn the_guest_cart_merges_into_the_account_after_login() {
    with_cart(FixtureSet::Contract, |session, cart| {
      now_or_never(cart.add(line("yirga", 2))).unwrap();
      sign_in(session);
      let merged = now_or_never(cart.merge_guest()).unwrap();
      assert_eq!(merged, 1);
      assert_eq!(cart.guest(), GuestCart::default());
      assert_eq!(cart.item_count(), 3, "the member cart after the merge");
      // With nothing to merge, login loads the member cart.
      cart.reset();
      assert_eq!(now_or_never(cart.merge_guest()).unwrap(), 0);
      assert_eq!(cart.item_count(), 3);
    });
  }

  #[test]
  fn member_changes_go_through_the_api() {
    with_cart(FixtureSet::Contract, |session, cart| {
      sign_in(session);
      now_or_never(cart.fetch());
      assert_eq!(cart.phase(), Phase::Ready);
      assert_eq!(cart.lines().len(), 3);
      cart.move_cursor(1);
      assert_eq!(cart.cursor().as_deref(), Some("RT-NGT-500G-ES-STD"));
      now_or_never(cart.toggle_selected("RT-PNK-250G-PO-STD".into())).unwrap();
      assert_eq!(cart.selected_count(), 2);
      now_or_never(cart.toggle_all()).unwrap();
      assert_eq!(
        cart.selected_count(),
        3,
        "not all were selected: select all"
      );
      now_or_never(cart.toggle_all()).unwrap();
      assert_eq!(cart.selected_count(), 0, "all selected: select none");
      now_or_never(cart.fetch());
      now_or_never(cart.remove("RT-PNK-250G-PO-STD".into())).unwrap();
      assert_eq!(cart.lines().len(), 2);
      assert_eq!(cart.cursor().as_deref(), Some("RT-NGT-500G-ES-STD"));
      now_or_never(cart.restore()).unwrap();
      assert_eq!(cart.lines().len(), 3);
      cart.set_coupon_input(" tty10 ");
      now_or_never(cart.apply_coupon()).unwrap();
      assert!(
        matches!(cart.coupon_feedback(), Some(CouponFeedback::Applied(c)) if c.code == "TTY10")
      );
      now_or_never(cart.set_address(2)).unwrap();
      assert_eq!(cart.cart().and_then(|c| c.address).map(|a| a.id), Some(2));
      let expired = now_or_never(cart.set_quantity("RT-GONE".into(), 2)).unwrap_err();
      assert!(expired.is(ErrorCode::NotFound));
    });
  }

  #[test]
  fn checkout_shows_the_receipt_and_keeps_the_key_on_network_failure() {
    with_cart(FixtureSet::Contract, |session, cart| {
      sign_in(session);
      now_or_never(cart.fetch());
      assert_eq!(cart.checkout_check(), CheckoutCheck::Ready);
      assert_eq!(cart.begin_checkout(), Some(()));
      assert!(cart.submitting());
      assert_eq!(cart.begin_checkout(), None);
      let placed = now_or_never(cart.place_order()).unwrap();
      assert_eq!(placed.order.number, "RT2611220417");
      assert!(!cart.submitting());
      assert_eq!(
        cart.receipt().map(|o| o.number),
        Some("RT2611220417".into())
      );
      cart.clear_receipt();
      assert!(!cart.has_receipt());
    });
    with_cart(FixtureSet::Down, |session, cart| {
      let auth: AuthSession = serde_json::from_str(include_str!(
        "../../../../packages/contract/fixtures/v1/auth/login.json"
      ))
      .unwrap();
      session.signed_in(auth);
      cart.begin_checkout();
      assert!(now_or_never(cart.place_order()).unwrap_err().is_network());
      let first = cart.attempt.get_untracked().1.clone();
      assert!(first.is_some(), "the key is kept for the retry");
      cart.begin_checkout();
      let _ = now_or_never(cart.place_order());
      assert_eq!(cart.attempt.get_untracked().1, first);
    });
  }
}
