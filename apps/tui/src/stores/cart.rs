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
//! it runs ([`Remote::begin_quiet`]); a newer one supersedes an older answer, and only the
//! latest answer touches the guest lines; a failed one changes nothing (a guest change is
//! rolled back). Quantity steps go one at a time, each from the quantity last answered.

use std::rc::Rc;

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

/// 删除记录在发送恢复请求后暂时不能再次撤销。
#[derive(Clone, PartialEq, Eq)]
enum Undo {
  /// 可以通过购物车的撤销键恢复这条记录。
  Ready(Removed),
  /// 请求持有同一条记录，用来识别等待期间发生的另一次删除。
  Restoring(Rc<Removed>),
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

/// 生成购物车合并或下单的请求标识，格式为 8–64 个 `[A-Za-z0-9_-]` 字符。
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
  /// 待合并访客购物车的请求标识，失败后重试时复用。
  merge_key: Signal<Option<String>>,
  cursor: Signal<Option<String>>,
  undo: Signal<Option<Undo>>,
  coupon_input: Signal<String>,
  coupon_feedback: Signal<Option<CouponFeedback>>,
  checkout: Signal<Option<CheckoutStep>>,
  attempt: Signal<(u64, Option<String>)>,
  receipt: Signal<Option<Order>>,
  steps: Signal<Vec<(String, i8)>>,
  stepping: Signal<bool>,
}

impl CartStore {
  /// An empty guest cart reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      cart: Remote::new(no_lines),
      guest: signal(GuestCart::default()),
      merge_key: signal(None),
      cursor: signal(None),
      undo: signal(None),
      coupon_input: signal(String::new()),
      coupon_feedback: signal(None),
      checkout: signal(None),
      attempt: signal((0, None)),
      receipt: signal(None),
      steps: signal(Vec::new()),
      stepping: signal(false),
    }
  }

  fn member(&self) -> bool {
    self.session.is_member_untracked()
  }

  /// Whether a member still has guest lines the account has not taken: the merge after
  /// login failed, and the next load (`⏎ 重试`) or change merges them first (tracked).
  pub fn merge_pending(&self) -> bool {
    self.session.is_member() && self.guest.with(|guest| !guest.lines.is_empty())
  }

  fn merge_pending_untracked(&self) -> bool {
    self.member() && self.guest.with_untracked(|guest| !guest.lines.is_empty())
  }

  /// Before a member change: merges the guest lines a failed merge left (the change would
  /// answer a cart without them); its failure stops the change.
  async fn settle_merge(self) -> Result<(), ApiFailure> {
    if self.merge_pending_untracked() {
      self.merge_guest().await?;
    }
    Ok(())
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
    self.undo.with(|undo| match undo {
      Some(Undo::Ready(removed)) => Some(removed.clone()),
      _ => None,
    })
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

  /// Loads the cart: the member cart (merging the guest lines a failed merge left), or the
  /// guest quote. Await it in tests.
  pub async fn fetch(self) {
    if self.merge_pending_untracked() {
      if let Err(failure) = self.merge_guest().await {
        self.session.report(&failure);
      }
      return;
    }
    let client = self.session.client();
    let revision = self.cart.begin();
    let result = if self.member() {
      client.cart().await
    } else {
      client.quote_cart(&self.quote_request(None)).await
    };
    if let Err(failure) = &result {
      self.session.report(failure);
    }
    // Only the latest answer speaks for the guest lines: a newer change (an add while this
    // quote was out) already holds lines this older quote has never seen.
    if self.cart.is_current(revision)
      && let Ok(cart) = &result
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
      self.settle_merge().await?;
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

  /// `←` `→`: line `sku`'s quantity ±1. Steps go one request at a time: a step pressed
  /// while one is out waits for its answer, then goes from the quantity the server answered
  /// (two quick `→` on 1 make 3). Each press is its own step, clamped to 1 … the line's
  /// most, in the order pressed (`→ → ←` on 8 of 9 ends on 8); one that changes nothing
  /// sends nothing. A failed request drops the steps still waiting (they counted on it).
  pub async fn step_quantity(self, sku: String, delta: i8) -> Result<(), ApiFailure> {
    self.steps.update(|steps| steps.push((sku, delta)));
    if self.stepping.get_untracked() {
      // The step under way sends it.
      return Ok(());
    }
    self.stepping.set(true);
    let mut outcome = Ok(());
    while let Some((sku, delta)) = self.steps.with_untracked(|steps| steps.first().cloned()) {
      self.steps.update(|steps| {
        steps.remove(0);
      });
      let Some((_, line)) = self.line(&sku) else {
        continue;
      };
      let most = line.max_quantity.clamp(1, MAX_LINE_QUANTITY);
      let next = line.quantity.saturating_add_signed(delta).clamp(1, most);
      if next == line.quantity {
        continue;
      }
      if let Err(failure) = self.set_quantity(sku, next).await {
        self.steps.set(Vec::new());
        outcome = Err(failure);
        break;
      }
    }
    self.stepping.set(false);
    outcome
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
        self.undo.set(Some(Undo::Ready(Removed {
          line: as_guest_line(&line),
          index: u32::try_from(index).unwrap_or(u32::MAX),
          name: line.name.clone(),
        })));
        self.cursor.set(cursor);
      });
    }
    result
  }

  /// 恢复最近删除的商品并返回商品名。没有可撤销的记录时返回 None。
  pub async fn restore(self) -> Result<Option<String>, ApiFailure> {
    let Some(Undo::Ready(removed)) = self.undo.get_untracked() else {
      return Ok(None);
    };
    let removed = Rc::new(removed);
    self.undo.set(Some(Undo::Restoring(removed.clone())));
    let result = self.restore_line(&removed).await;
    let current = self.undo.with_untracked(
      |undo| matches!(undo, Some(Undo::Restoring(pending)) if Rc::ptr_eq(pending, &removed)),
    );
    if current {
      self
        .undo
        .set(result.is_err().then(|| Undo::Ready((*removed).clone())));
    }
    result.map(|_| Some(removed.name.clone()))
  }

  /// 把删除的商品按当时的位置放回会员或访客购物车。
  async fn restore_line(self, removed: &Removed) -> Result<Cart, ApiFailure> {
    if self.member() {
      self.settle_merge().await?;
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
    }
  }

  /// Applies the coupon field's code (empty removes the coupon). A bad code leaves the cart
  /// as it was and says `✗ 优惠码无效或已过期`.
  pub async fn apply_coupon(self) -> Result<Cart, ApiFailure> {
    let code = self.coupon_input.get_untracked().trim().to_owned();
    let result = if self.member() {
      let settled = self.settle_merge().await;
      let client = self.session.client();
      let revision = self.cart.begin_quiet();
      let result = match settled {
        Ok(()) => client.apply_coupon(&code).await,
        Err(failure) => Err(failure),
      };
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
    self.settle_merge().await?;
    let client = self.session.client();
    let revision = self.cart.begin_quiet();
    let result = client.set_cart_address(address_id).await;
    self.apply(revision, result)
  }

  /// 登录后合并访客购物车，返回实际加入的商品件数。访客购物车为空时加载会员购物车。
  /// 同一次合并失败后复用请求标识，供服务端识别重试。
  /// 等待和失败时清空展示的数据，保留访客商品直到合并成功。
  pub async fn merge_guest(self) -> Result<u32, ApiFailure> {
    let guest = self.guest.get_untracked();
    let client = self.session.client();
    let revision = self.cart.begin_fresh();
    if guest.lines.is_empty() {
      let result = client.cart().await;
      let outcome = result.as_ref().map(|_| 0).map_err(Clone::clone);
      self.cart.finish(revision, result);
      return outcome;
    }
    let request = CartMergeRequest {
      idempotency_key: Some(self.merge_key.get_untracked().unwrap_or_else(|| {
        let key = idempotency_key(1);
        self.merge_key.set(Some(key.clone()));
        key
      })),
      lines: guest.lines,
      coupon_code: guest.coupon,
    };
    match client.merge_cart(&request).await {
      Ok(merged) => {
        batch(|| {
          if self.cart.finish(revision, Ok(merged.cart)) {
            self.guest.set(GuestCart::default());
            self.merge_key.set(None);
            self.undo.set(None);
          }
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
  /// (a network failure keeps the attempt's key, so a retry cannot order twice), except
  /// that `price_changed` reloads it first: the next `c` sends the total now shown.
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
      Err(failure) => {
        if failure.is(ErrorCode::PriceChanged) {
          // The total the member confirmed is not the server's any more: show the new one
          // before they press `c` again (a retry would send the old total once more).
          self.fetch().await;
        }
        batch(|| {
          self.checkout.set(None);
          if !failure.is_network() {
            self.attempt.set((attempt + 1, None));
          }
        });
      }
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
      self.merge_key.set(None);
      self.cursor.set(None);
      self.undo.set(None);
      self.coupon_input.set(String::new());
      self.coupon_feedback.set(None);
      self.checkout.set(None);
      self.receipt.set(None);
      self.steps.set(Vec::new());
    });
  }
}

#[cfg(test)]
mod tests {
  use coffee_api::{AuthSession, Grind, LoginRequest, Pack, Weight};
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Script, Stepped, Transport, now_or_never};

  const CART: &str = include_str!("../../../../packages/contract/fixtures/v1/cart/cart.json");
  const CART_EMPTY: &str =
    include_str!("../../../../packages/contract/fixtures/v1/cart/cart-empty.json");
  const GUEST_QUOTE: &str =
    include_str!("../../../../packages/contract/fixtures/v1/cart/guest-quote.json");
  const LOGIN: &str = include_str!("../../../../packages/contract/fixtures/v1/auth/login.json");
  const MERGED: &str = include_str!("../../../../packages/contract/fixtures/v1/cart/merge.json");
  const ADDRESS_SET: &str =
    include_str!("../../../../packages/contract/fixtures/v1/cart/address-set.json");
  const INTERNAL_ERROR: &str =
    include_str!("../../../../packages/contract/fixtures/v1/errors/internal_error.json");
  const PRICE_CHANGED: &str =
    include_str!("../../../../packages/contract/fixtures/v1/errors/price_changed.json");
  const PINK: &str = "RT-PNK-250G-PO-STD";

  /// Stores over a [`Script`]: every answer comes when the test gives it.
  fn with_script(test: impl FnOnce(Script, SessionStore, CartStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let script = Script::default();
    let transport = Transport::Script(script.clone());
    let (owner, (session, cart)) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(transport, None));
      (session, CartStore::new(session))
    });
    test(script, session, cart);
    owner.dispose();
  }

  /// The member cart of the fixtures with line `sku` at `quantity` and the total `total`.
  fn member_cart(sku: &str, quantity: u8, total: u32) -> String {
    let mut cart: Cart = serde_json::from_str(CART).unwrap();
    for line in &mut cart.lines {
      if line.sku == sku {
        line.quantity = quantity;
      }
    }
    cart.totals.total = total;
    serde_json::to_string(&cart).unwrap()
  }

  /// Reloads the member cart with the pink line at `quantity`.
  fn cart_with_quantity(script: &Script, cart: CartStore, quantity: u8) {
    let mut load = Stepped::start(cart.fetch());
    script.answer(0, 200, member_cart(PINK, quantity, 100));
    assert!(load.poll().is_some());
  }

  fn sign_in_scripted(script: &Script, session: SessionStore, cart: CartStore) {
    session.signed_in(serde_json::from_str(LOGIN).unwrap());
    let mut load = Stepped::start(cart.fetch());
    script.answer(0, 200, member_cart(PINK, 1, 426));
    assert!(load.poll().is_some());
  }

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
      assert_eq!(
        now_or_never(cart.restore()).unwrap().as_deref(),
        Some("耶加雪菲 孔加")
      );
      assert_eq!(cart.undo(), None);
      assert_eq!(cart.item_count(), 2);
    });
  }

  #[test]
  fn repeated_undo_sends_one_restore_and_failure_allows_retry() {
    with_script(|script, session, cart| {
      sign_in_scripted(&script, session, cart);
      let delete = Stepped::start(cart.remove(PINK.into()));
      script.answer(0, 200, CART_EMPTY);
      delete.finish().unwrap();
      let removed = cart.undo().unwrap();

      let first = Stepped::start(cart.restore());
      assert_eq!(cart.undo(), None);
      assert_eq!(now_or_never(cart.restore()).unwrap(), None);
      assert_eq!(script.calls(), ["POST /cart/lines"]);
      let body = script.body(0);
      script.answer(0, 500, INTERNAL_ERROR);
      assert!(first.finish().is_err());
      assert_eq!(cart.undo(), Some(removed.clone()));

      let retry = Stepped::start(cart.restore());
      assert_eq!(script.body(0), body);
      script.answer(0, 200, member_cart(PINK, 1, 426));
      assert_eq!(retry.finish().unwrap(), Some(removed.name));
      assert_eq!(cart.undo(), None);
      assert!(script.calls().is_empty());
    });
  }

  #[test]
  fn an_older_restore_leaves_a_new_deletion_available() {
    with_script(|script, session, cart| {
      sign_in_scripted(&script, session, cart);
      let delete = Stepped::start(cart.remove(PINK.into()));
      script.answer(
        0,
        200,
        include_str!("../../../../packages/contract/fixtures/v1/cart/removed.json"),
      );
      delete.finish().unwrap();
      let restore = Stepped::start(cart.restore());
      let sku = cart.cursor().unwrap();
      let next_delete = Stepped::start(cart.remove(sku));
      script.answer(1, 200, CART_EMPTY);
      next_delete.finish().unwrap();
      let newer = cart.undo().unwrap();
      script.answer(0, 200, CART);
      restore.finish().unwrap();
      assert_eq!(cart.undo(), Some(newer));
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
  fn an_older_quote_landing_late_leaves_the_guest_lines_alone() {
    with_script(|script, _, cart| {
      // The cart page's load (no lines yet), then `a` on the list before it answered.
      let mut load = Stepped::start(cart.fetch());
      let add = Stepped::start(cart.add(line("yirga", 2)));
      assert_eq!(script.calls(), ["POST /cart/quote", "POST /cart/quote"]);
      script.answer(1, 200, GUEST_QUOTE);
      add.finish().unwrap();
      script.answer(0, 200, CART_EMPTY);
      assert!(load.poll().is_some());
      assert_eq!(cart.guest().lines, vec![line("yirga", 2)]);
      assert_eq!(cart.item_count(), 2, "the screen still shows the add");
      // The next add quotes both lines, not only the new one.
      let _next = Stepped::start(cart.add(line("pinkb", 1)));
      let lines = script.body(0)["lines"].as_array().unwrap().len();
      assert_eq!(lines, 2);
    });
  }

  #[test]
  fn quick_quantity_steps_go_one_at_a_time_from_the_latest_answer() {
    with_script(|script, session, cart| {
      sign_in_scripted(&script, session, cart);
      let mut first = Stepped::start(cart.step_quantity(PINK.into(), 1));
      // The second `→` before the first answer: it waits for it.
      now_or_never(cart.step_quantity(PINK.into(), 1)).unwrap();
      assert_eq!(script.calls(), [format!("PATCH /cart/lines/{PINK}")]);
      assert_eq!(script.body(0)["quantity"], 2);
      script.answer(0, 200, member_cart(PINK, 2, 530));
      assert!(first.poll().is_none(), "the waiting step goes out now");
      assert_eq!(script.body(0)["quantity"], 3);
      script.answer(0, 200, member_cart(PINK, 3, 634));
      first.finish().unwrap();
      assert_eq!(cart.cursor_line().map(|l| l.quantity), Some(3));
      // A failed step drops the ones counting on it.
      let first = Stepped::start(cart.step_quantity(PINK.into(), 1));
      now_or_never(cart.step_quantity(PINK.into(), 1)).unwrap();
      script.drop_connection(0);
      assert!(first.finish().unwrap_err().is_network());
      assert!(script.calls().is_empty());
      assert_eq!(cart.cursor_line().map(|l| l.quantity), Some(3));
      // At the line's most (9): `→` out, then `→` `←` pressed. Each press clamps on its own:
      // the second `→` changes nothing, the `←` goes back to 8.
      cart_with_quantity(&script, cart, 8);
      let mut first = Stepped::start(cart.step_quantity(PINK.into(), 1));
      now_or_never(cart.step_quantity(PINK.into(), 1)).unwrap();
      now_or_never(cart.step_quantity(PINK.into(), -1)).unwrap();
      assert_eq!(script.body(0)["quantity"], 9);
      script.answer(0, 200, member_cart(PINK, 9, 900));
      assert!(first.poll().is_none(), "the `←` goes out");
      assert_eq!(script.body(0)["quantity"], 8);
      script.answer(0, 200, member_cart(PINK, 8, 800));
      first.finish().unwrap();
      assert!(script.calls().is_empty());
      assert_eq!(cart.cursor_line().map(|l| l.quantity), Some(8));
      cart_with_quantity(&script, cart, 3);
      // `→` then `←` before the answer: nothing more to send.
      let mut first = Stepped::start(cart.step_quantity(PINK.into(), 1));
      now_or_never(cart.step_quantity(PINK.into(), -1)).unwrap();
      script.answer(0, 200, member_cart(PINK, 4, 738));
      assert!(first.poll().is_none(), "back to 3");
      script.answer(0, 200, member_cart(PINK, 3, 634));
      first.finish().unwrap();
      assert!(script.calls().is_empty());
    });
  }

  #[test]
  fn a_changed_price_reloads_the_cart_so_the_retry_sends_the_new_total() {
    with_script(|script, session, cart| {
      sign_in_scripted(&script, session, cart);
      cart.begin_checkout();
      let mut order = Stepped::start(cart.place_order());
      assert_eq!(script.calls(), ["POST /orders"]);
      assert_eq!(script.body(0)["expectedTotal"], 426);
      script.answer(0, 409, PRICE_CHANGED);
      assert!(order.poll().is_none(), "the cart reloads first");
      assert!(cart.submitting(), "no second `c` while it reloads");
      assert_eq!(script.calls(), ["GET /cart"]);
      script.answer(0, 200, member_cart(PINK, 1, 436));
      assert!(order.finish().unwrap_err().is(ErrorCode::PriceChanged));
      assert!(!cart.submitting());
      assert_eq!(cart.total(), 436, "the new total shows");
      cart.begin_checkout();
      let _retry = Stepped::start(cart.place_order());
      assert_eq!(script.body(0)["expectedTotal"], 436);
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

  #[test]
  fn a_failed_merge_keeps_the_guest_lines_and_the_next_load_or_change_merges_them() {
    with_script(|script, session, cart| {
      let add = Stepped::start(cart.add(line("yirga", 2)));
      script.answer(0, 200, GUEST_QUOTE);
      add.finish().unwrap();
      assert_eq!(cart.item_count(), 2);
      session.signed_in(serde_json::from_str(LOGIN).unwrap());

      // The merge after login fails: no cart shows (not the guest's quote), the lines stay.
      let merge = Stepped::start(cart.merge_guest());
      assert_eq!(script.calls(), ["POST /cart/merge"]);
      let first_key = script.body(0)["idempotencyKey"]
        .as_str()
        .unwrap()
        .to_owned();
      script.answer(0, 500, INTERNAL_ERROR);
      assert!(merge.finish().is_err());
      assert_eq!(cart.phase(), Phase::Failed);
      assert_eq!(
        cart.cart(),
        None,
        "the guest's quote is not the member cart"
      );
      assert_eq!(cart.item_count(), 0);
      assert_eq!(cart.guest().lines, vec![line("yirga", 2)]);
      assert!(cart.merge_pending());
      assert_eq!(cart.checkout_check(), CheckoutCheck::NothingSelected);

      // A change first merges (here 本单寄往 from the address book), and stops if that fails.
      let address = Stepped::start(cart.set_address(1));
      assert_eq!(script.calls(), ["POST /cart/merge"]);
      assert_eq!(script.body(0)["idempotencyKey"], first_key);
      script.answer(0, 500, INTERNAL_ERROR);
      assert!(address.finish().is_err());
      assert!(script.calls().is_empty(), "no address without the merge");
      assert_eq!(cart.cart(), None);
      assert!(cart.merge_pending());

      // `⏎ 重试` (a load) merges them; then the change goes through on the merged cart.
      let mut load = Stepped::start(cart.fetch());
      assert_eq!(script.calls(), ["POST /cart/merge"]);
      assert_eq!(script.body(0)["idempotencyKey"], first_key);
      assert_eq!(script.body(0)["lines"].as_array().unwrap().len(), 1);
      script.answer(0, 200, MERGED);
      assert!(load.poll().is_some());
      assert!(!cart.merge_pending());
      assert_eq!(cart.guest(), GuestCart::default());
      assert_eq!(cart.merge_key.get_untracked(), None);
      assert_eq!(cart.phase(), Phase::Ready);
      assert_eq!(cart.item_count(), 3, "the merged member cart");
      let address = Stepped::start(cart.set_address(1));
      assert_eq!(script.calls(), ["PUT /cart/address"]);
      script.answer(0, 200, ADDRESS_SET);
      address.finish().unwrap();
      assert!(cart.cart().is_some_and(|c| c.address.is_some()));
    });
  }

  #[test]
  fn 合并响应丢失后重试使用相同的请求内容和标识() {
    with_script(|script, session, cart| {
      let add = Stepped::start(cart.add(line("yirga", 2)));
      script.answer(0, 200, GUEST_QUOTE);
      add.finish().unwrap();
      session.signed_in(serde_json::from_str(LOGIN).unwrap());

      let first = Stepped::start(cart.merge_guest());
      let body = script.body(0);
      script.drop_connection(0);
      assert!(first.finish().unwrap_err().is_network());
      let retry = Stepped::start(cart.merge_guest());
      assert_eq!(script.body(0), body);
      script.answer(0, 200, MERGED);
      assert_eq!(retry.finish().unwrap(), 1);
      assert_eq!(cart.merge_key.get_untracked(), None);
    });
  }

  #[test]
  fn 并发合并使用相同的请求标识() {
    with_script(|script, session, cart| {
      cart.guest.set(GuestCart {
        lines: vec![line("yirga", 2)],
        coupon: None,
      });
      session.signed_in(serde_json::from_str(LOGIN).unwrap());
      let first = Stepped::start(cart.merge_guest());
      let second = Stepped::start(cart.merge_guest());
      let first_key = script.body(0)["idempotencyKey"].clone();
      assert_eq!(script.body(1)["idempotencyKey"], first_key);
      script.answer(0, 200, MERGED);
      first.finish().unwrap();
      assert!(cart.merge_pending());
      script.answer(0, 200, MERGED);
      second.finish().unwrap();
      assert!(!cart.merge_pending());
    });
  }

  #[test]
  fn 清空购物车后使用新的合并请求标识() {
    with_script(|script, session, cart| {
      cart.guest.set(GuestCart {
        lines: vec![line("yirga", 2)],
        coupon: None,
      });
      session.signed_in(serde_json::from_str(LOGIN).unwrap());
      let first = Stepped::start(cart.merge_guest());
      let first_key = script.body(0)["idempotencyKey"].clone();
      script.drop_connection(0);
      first.finish().unwrap_err();
      cart.reset();
      cart.guest.set(GuestCart {
        lines: vec![line("yirga", 2)],
        coupon: None,
      });
      let next = Stepped::start(cart.merge_guest());
      assert_ne!(script.body(0)["idempotencyKey"], first_key);
      script.drop_connection(0);
      next.finish().unwrap_err();
      cart.reset();
      assert_eq!(cart.merge_key.get_untracked(), None);
    });
  }
}
