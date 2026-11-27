//! The user's actions: what a command (or a press) does when it reaches across stores —
//! starting the async work under the root's owner, then turning the outcome into toasts,
//! navigation and the other stores' updates. Commands call these; pages call them for
//! presses. Copy: `docs/design/copy.md` ("Toast").
//!
//! Every member request that answers `401 session_expired` (or `login_required`) ends the
//! member session here ([`session_expired`]): back to the login page, `登录已失效`.

use coffee_api::{ErrorCode, Grind, GuestCartLine, Pack};

use super::{
  Stores,
  cart::CheckoutCheck,
  session::LoginMode,
  settings::Setting,
  ui::{LogoutChoice, Overlay},
};
use crate::{
  api::{ApiFailure, NETWORK_MESSAGE},
  router::{Page, Route},
};

/// The search box's field name (`/` asks it to take the focus).
pub const SEARCH_FIELD: &str = "catalog.search";

/// `{商品名} · {重量} · {研磨} × {数量}`: a cart line for a toast.
pub fn line_summary(name: &str, line: &GuestCartLine) -> String {
  format!(
    "{name} · {} · {} × {}",
    line.weight.label(),
    line.grind.label(),
    line.quantity
  )
}

/// Ends the member session after `failure` if it says the session is gone: back to the
/// login page (coming back where the user was), `登录已失效`. Whether it did.
pub fn session_expired(stores: &Stores, failure: &ApiFailure) -> bool {
  if !failure.ends_session() || !stores.session.is_member_untracked() {
    return false;
  }
  let here = stores.router.top_untracked();
  sign_out_locally(stores);
  stores.session.set_return_to(Some(here));
  stores.router.replace(Route::Login);
  stores.toasts.warn("登录已失效", "请重新登录，购物车已保留");
  true
}

/// A request failed: the session-expiry flow, or `操作没有完成` with the reason.
pub fn failed(stores: &Stores, failure: &ApiFailure) {
  if session_expired(stores, failure) {
    return;
  }
  let sub = if failure.is_network() {
    NETWORK_MESSAGE.to_owned()
  } else {
    failure.message()
  };
  stores.toasts.warn("操作没有完成", sub);
}

fn sign_out_locally(stores: &Stores) {
  stores.session.signed_out();
  stores.cart.reset();
  stores.addresses.reset();
  stores.orders.reset();
  stores.profile.reset();
  stores.ui.reset();
}

// --- Navigation -------------------------------------------------------------------------

/// `1`–`4`: tab `index` (leaving the cart hides its receipt).
pub fn go_tab(stores: &Stores, index: usize) {
  if stores.router.top_untracked().page() == Page::Cart && index != 1 {
    stores.cart.clear_receipt();
  }
  stores.router.go_tab(index);
}

/// `/`: the catalog, with the focus in the search box.
pub fn search(stores: &Stores) {
  go_tab(stores, 0);
  stores.ui.request_focus(SEARCH_FIELD);
}

/// Opens product `id`'s detail page.
pub fn open_product(stores: &Stores, id: &str) {
  stores.product.open(id);
  stores.router.go(Route::Detail(id.to_owned().into()));
}

/// Esc / Backspace on the detail page: back to the list (its filters and cursor kept).
pub fn back(stores: &Stores) {
  stores.router.pop();
}

/// `⏎ 去登录` (guests on the address book and the profile, `登录 →`): the login page,
/// coming back here after.
pub fn go_login(stores: &Stores) {
  let here = stores.router.top_untracked();
  stores.session.set_return_to(Some(here));
  stores.router.replace(Route::Login);
}

// --- Session ----------------------------------------------------------------------------

/// On connect: with a key, try to resume the remembered session (01-login.md). While the
/// splash is up it only records the outcome ([`leave_splash`] then picks the page); on the
/// login page (the splash skipped) a resumed session replaces it with the catalog, and a key
/// that is not remembered leaves it there.
pub fn start(stores: &Stores) {
  if !stores.session.has_key() {
    return;
  }
  stores.session.set_resuming(true);
  let stores = *stores;
  stores.spawn(async move {
    match stores.session.client().resume().await {
      Ok(auth) => {
        let name = auth.user.username.clone();
        stores.session.signed_in(auth);
        if stores.router.top_untracked() != Route::Splash {
          stores.router.replace(Route::Catalog);
          welcome_back(&stores, &name);
        }
        sync_member(stores).await;
      }
      Err(failure) => stores.session.resume_failed(failure),
    }
  });
}

/// `欢迎回来，{用户名}` / `已自动恢复登录`.
fn welcome_back(stores: &Stores, name: &str) {
  stores
    .toasts
    .ok(format!("欢迎回来，{name}"), "已自动恢复登录");
}

/// The splash is done (its work settled, or `⏎ 跳过`, or its time ran out): a resumed
/// member goes to the catalog with the welcome toast, everyone else to the login page (a
/// resume still under way then moves on from there). Only from the splash: later calls do
/// nothing.
pub fn leave_splash(stores: &Stores) {
  if stores.router.top_untracked() != Route::Splash {
    return;
  }
  match nsh_core::prelude::untracked(|| stores.session.member()) {
    Some(member) => {
      stores.router.replace(Route::Catalog);
      welcome_back(stores, &member.user.username);
    }
    None => stores.router.replace(Route::Login),
  }
}

/// What a sign-in did with the cart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CartSync {
  /// Merged (the units merged; 0 when the guest had none and the member cart loaded).
  Merged(u32),
  /// The merge (or the member cart's load) failed: the guest lines are kept, and the cart
  /// says `✗ 购物车同步失败` with `⏎ 重试`, which merges them.
  Failed,
}

/// After any sign-in: merge the guest cart (or load the member cart), load addresses and
/// settings; the profile and orders reload on their next visit. What became of the cart, or
/// `None` when the session ended on the way (the expiry flow already went to the login
/// page: the caller must not navigate or greet).
async fn sync_member(stores: Stores) -> Option<CartSync> {
  let cart = match stores.cart.merge_guest().await {
    Ok(merged) => CartSync::Merged(merged),
    Err(failure) if session_expired(&stores, &failure) => return None,
    Err(_) => CartSync::Failed,
  };
  stores.addresses.fetch().await;
  stores.settings.fetch().await;
  stores.orders.invalidate();
  stores.profile.invalidate();
  if !stores.session.is_member_untracked() || stores.session.expired_untracked() {
    // A later load saw the session end: the shell's expiry flow goes to the login page.
    return None;
  }
  Some(cart)
}

/// The sign-in toast's line when the cart did not sync (the guest lines are kept).
const CART_SYNC_FAILED: &str = "购物车同步失败 · 在购物车按 ⏎ 重试";

/// `g`: browse as a guest (`以访客身份浏览` / `结算前需要登录`).
pub fn continue_as_guest(stores: &Stores) {
  stores.session.set_return_to(None);
  stores.router.replace(Route::Catalog);
  stores.toasts.info("以访客身份浏览", "结算前需要登录");
}

/// `⏎` on the login page: login or register with the form (local validation first), then
/// the member sync, the welcome toast, and the page the user came from (the cart, after a
/// gated checkout) or the catalog.
pub fn submit_auth(stores: &Stores) {
  let Some(revision) = stores.session.begin_auth() else {
    return;
  };
  let mode = untracked_mode(stores);
  let stores = *stores;
  stores.spawn(async move {
    let client = stores.session.client();
    let result = match mode {
      LoginMode::Login => client.login(&stores.session.login_request()).await,
      LoginMode::Register => client.register(&stores.session.register_request()).await,
    };
    let auth = match result {
      Ok(auth) => auth,
      Err(failure) => {
        stores.session.auth_failed(revision, &failure);
        return;
      }
    };
    stores.session.auth_step(revision, super::AuthStep::Verify);
    let name = auth.user.username.clone();
    stores.session.signed_in(auth);
    stores.session.auth_step(revision, super::AuthStep::Sync);
    let Some(cart) = sync_member(stores).await else {
      // Signed in, then expired at once: the login page says `登录已失效`; stay there.
      return;
    };
    stores.session.auth_done(revision);
    let next = stores.session.take_return_to().unwrap_or(Route::Catalog);
    let next = if next == Route::Login {
      Route::Catalog
    } else {
      next
    };
    stores.router.go(next);
    match (mode, cart) {
      // Signed in, but the cart is not the account's yet: say so, never `已同步`.
      (LoginMode::Register, CartSync::Failed) => {
        stores
          .toasts
          .warn(format!("注册成功，欢迎 {name}"), CART_SYNC_FAILED);
      }
      (LoginMode::Login, CartSync::Failed) => {
        stores
          .toasts
          .warn(format!("欢迎回来，{name}"), CART_SYNC_FAILED);
      }
      (LoginMode::Register, CartSync::Merged(_)) => {
        stores.toasts.ok(
          format!("注册成功，欢迎 {name}"),
          "已为你登录 · 购物车已同步",
        );
      }
      (LoginMode::Login, CartSync::Merged(merged)) if merged > 0 => {
        stores.toasts.ok(
          format!("欢迎回来，{name}"),
          format!("已合并访客购物车 {merged} 件"),
        );
      }
      (LoginMode::Login, CartSync::Merged(_)) => {
        let items = nsh_core::prelude::untracked(|| stores.cart.item_count());
        let addresses = nsh_core::prelude::untracked(|| stores.addresses.count());
        stores.toasts.ok(
          format!("欢迎回来，{name}"),
          format!("已同步 {items} 件购物车商品与 {addresses} 个地址"),
        );
      }
    }
  });
}

fn untracked_mode(stores: &Stores) -> LoginMode {
  nsh_core::prelude::untracked(|| stores.session.mode())
}

/// `q`: opens the logout confirmation (members only; nothing for a guest, K3).
pub fn ask_logout(stores: &Stores) {
  if stores.session.is_member_untracked() {
    stores.ui.open(Overlay::Logout);
  }
}

/// `q` / `⏎` in the logout confirmation: `⏎` on `取消` cancels; else log out. The local
/// session ends whatever the server answers; back to the login page, `已退出登录`.
pub fn confirm_logout(stores: &Stores, by_enter: bool) {
  if by_enter && nsh_core::prelude::untracked(|| stores.ui.logout_choice()) == LogoutChoice::Cancel
  {
    stores.ui.close_if(Overlay::Logout);
    return;
  }
  if stores.session.logging_out_untracked() {
    return;
  }
  stores.session.set_logging_out(true);
  let stores = *stores;
  stores.spawn(async move {
    let _ = stores.session.client().logout().await;
    sign_out_locally(&stores);
    stores.settings.reset();
    stores.session.set_return_to(None);
    stores.router.replace(Route::Login);
    stores.toasts.info("已退出登录", "按 ⏎ 重新登录");
  });
}

// --- Cart -------------------------------------------------------------------------------

/// Adds `line` (named `name`) to the cart: `已加入购物车`, or with `buy_now` (`b`) straight
/// to the cart without a toast. `库存不足` puts the detail's quantity back to what is left.
pub fn add_to_cart(stores: &Stores, line: GuestCartLine, name: String, buy_now: bool) {
  let stores = *stores;
  stores.spawn(async move {
    match stores.cart.add(line.clone()).await {
      Ok(_) if buy_now => go_tab(&stores, 1),
      Ok(_) => {
        stores.toasts.ok("已加入购物车", line_summary(&name, &line));
      }
      Err(failure) if failure.is(ErrorCode::OutOfStock) => {
        let details = failure.details().cloned().unwrap_or_default();
        let available = details.available.unwrap_or(0);
        let product = details.product_name.unwrap_or(name);
        stores
          .product
          .set_quantity(u8::try_from(available).unwrap_or(9));
        stores
          .toasts
          .warn("库存不足", format!("{product} 仅剩 {available} 份"));
      }
      Err(failure) => failed(&stores, &failure),
    }
  });
}

/// `a` on the list: the product under the cursor, its default weight (250g, or the first in
/// stock), 整豆, 标准袋, × 1.
pub fn quick_add(stores: &Stores) {
  let Some(product) = nsh_core::prelude::untracked(|| stores.catalog.current()) else {
    return;
  };
  let line = GuestCartLine {
    product_id: product.id.clone(),
    weight: product.default_weight,
    grind: Grind::WholeBean,
    pack: Pack::Standard,
    quantity: 1,
    selected: true,
  };
  add_to_cart(stores, line, product.name, false);
}

/// `a` / `b` on the detail page: the SKU panel's selection.
pub fn add_selection(stores: &Stores, buy_now: bool) {
  if !stores.product.can_buy_untracked() {
    return;
  }
  let Some(line) = stores.product.line() else {
    return;
  };
  let name = nsh_core::prelude::untracked(|| stores.product.detail())
    .map(|d| d.name)
    .unwrap_or_default();
  add_to_cart(stores, line, name, buy_now);
}

/// Runs a cart change; failures become toasts.
fn cart_change<F, T>(stores: &Stores, change: impl FnOnce(super::CartStore) -> F + 'static)
where
  F: Future<Output = Result<T, ApiFailure>> + 'static,
{
  let stores = *stores;
  stores.spawn(async move {
    if let Err(failure) = change(stores.cart).await {
      failed(&stores, &failure);
    }
  });
}

fn cursor_sku(stores: &Stores) -> Option<String> {
  nsh_core::prelude::untracked(|| stores.cart.cursor())
}

/// `space`: selects or unselects the line under the cursor.
pub fn toggle_line(stores: &Stores) {
  if let Some(sku) = cursor_sku(stores) {
    cart_change(stores, move |cart| cart.toggle_selected(sku));
  }
}

/// `a` in the cart: select all / none.
pub fn toggle_all(stores: &Stores) {
  cart_change(stores, |cart| cart.toggle_all());
}

/// `←` `→` `+` `−`: the line under the cursor ±1.
pub fn step_line(stores: &Stores, delta: i8) {
  if let Some(sku) = cursor_sku(stores) {
    cart_change(stores, move |cart| cart.step_quantity(sku, delta));
  }
}

/// `x` / `d`: removes the line under the cursor (`已从购物车移除` / `{商品名} · 按 u 撤销`).
pub fn remove_line(stores: &Stores) {
  let Some(line) = nsh_core::prelude::untracked(|| stores.cart.cursor_line()) else {
    return;
  };
  let stores = *stores;
  stores.spawn(async move {
    match stores.cart.remove(line.sku.clone()).await {
      Ok(_) => {
        stores
          .toasts
          .warn("已从购物车移除", format!("{} · 按 u 撤销", line.name));
      }
      Err(failure) => failed(&stores, &failure),
    }
  });
}

/// `u`: restores the last deleted line (`已恢复` / `{商品名}`).
pub fn undo_remove(stores: &Stores) {
  let stores = *stores;
  stores.spawn(async move {
    match stores.cart.restore().await {
      Ok(Some(name)) => {
        stores.toasts.ok("已恢复", name);
      }
      Ok(None) => {}
      Err(failure) => failed(&stores, &failure),
    }
  });
}

/// `⏎` in the coupon field: applies it (the field shows `✓ …` / `✗ …`).
pub fn apply_coupon(stores: &Stores) {
  let stores = *stores;
  stores.spawn(async move {
    if let Err(failure) = stores.cart.apply_coupon().await
      && !failure.is(ErrorCode::CouponInvalid)
    {
      failed(&stores, &failure);
    }
  });
}

/// `c` / `⏎`: checkout, after the checks in order (04-cart.md): a guest goes to log in and
/// comes back here; nothing selected or no address says so; else the order is placed and
/// the receipt shows.
pub fn checkout(stores: &Stores) {
  match stores.cart.checkout_check() {
    CheckoutCheck::Guest => {
      stores.toasts.warn("结算前请先登录", "访客可以浏览和加购");
      stores.session.set_return_to(Some(Route::Cart));
      stores.router.replace(Route::Login);
    }
    CheckoutCheck::NothingSelected => {
      stores
        .toasts
        .warn("还没有勾选商品", "用 space 勾选要结算的豆子");
    }
    CheckoutCheck::NoAddress => {
      stores
        .toasts
        .warn("请先选择收货地址", "在地址本里按 ⏎ 指定");
      go_tab(stores, 2);
    }
    CheckoutCheck::Ready => {
      if stores.cart.begin_checkout().is_none() {
        return;
      }
      let stores = *stores;
      stores.spawn(async move {
        match stores.cart.place_order().await {
          Ok(placed) => {
            stores.orders.invalidate();
            stores.profile.invalidate();
            stores.toasts.ok(
              "订单已提交",
              format!("{} · 下一个烘焙日出炉", placed.order.number),
            );
          }
          Err(failure) if session_expired(&stores, &failure) => {}
          Err(failure) => {
            stores.toasts.warn(
              "下单失败",
              format!("{}，购物车没有变化", order_failure(&failure)),
            );
          }
        }
      });
    }
  }
}

/// The reason of a failed checkout (copy.md: `{商品名} 库存不足`, `{商品名} {重量} 暂时缺货`).
pub fn order_failure(failure: &ApiFailure) -> String {
  let details = failure.details();
  let product = details.and_then(|d| d.product_name.clone());
  match (failure.code(), product) {
    (Some(ErrorCode::OutOfStock), Some(product)) => format!("{product} 库存不足"),
    (Some(ErrorCode::SoldOut), Some(product)) => {
      let weight = details
        .and_then(|d| d.weight)
        .map(|w| format!(" {}", w.label()))
        .unwrap_or_default();
      format!("{product}{weight} 暂时缺货")
    }
    _ if failure.is_network() => "连接服务器失败".to_owned(),
    _ => failure.message(),
  }
}

/// `⏎` / `Esc` on the receipt: back to the beans.
pub fn continue_shopping(stores: &Stores) {
  stores.cart.clear_receipt();
  stores.router.go_tab(0);
}

// --- Addresses --------------------------------------------------------------------------

/// `⏎ 用于本单`: address `id` for this order (`本单寄往这里`), then the cart.
pub fn use_address(stores: &Stores, id: u32) {
  let stores = *stores;
  stores.spawn(async move {
    match stores.cart.set_address(id).await {
      Ok(cart) => {
        if let Some(address) = cart.address {
          stores.toasts.ok(
            "本单寄往这里",
            format!("{} · {}", address.recipient, address.region),
          );
        }
        stores.addresses.fetch().await;
        go_tab(&stores, 1);
      }
      Err(failure) => failed(&stores, &failure),
    }
  });
}

/// `space`: address `id` becomes the default (`已设为默认地址` / `{地区}`).
pub fn set_default_address(stores: &Stores, id: u32) {
  let stores = *stores;
  stores.spawn(async move {
    match stores.addresses.set_default(id).await {
      Ok(book) => {
        if let Some(address) = book.items.iter().find(|a| a.id == id) {
          stores.toasts.ok("已设为默认地址", address.region.clone());
        }
      }
      Err(failure) => failed(&stores, &failure),
    }
    stores.addresses.refresh_after_changes().await;
  });
}

/// `d` / `x`: deletes address `id` (`已删除地址` / `{姓名} · {地区}`); the cart's order
/// address may have moved, so it reloads.
pub fn delete_address(stores: &Stores, id: u32) {
  let Some(address) = nsh_core::prelude::untracked(|| stores.addresses.address(id)) else {
    return;
  };
  let stores = *stores;
  stores.spawn(async move {
    let refresh_cart = match stores.addresses.delete(id).await {
      Ok(_) => {
        stores.toasts.warn(
          "已删除地址",
          format!("{} · {}", address.recipient, address.region),
        );
        true
      }
      Err(failure) => {
        failed(&stores, &failure);
        false
      }
    };
    if stores.addresses.refresh_after_changes().await && refresh_cart {
      stores.cart.fetch().await;
    }
  });
}

/// `n`: the new address dialog.
pub fn new_address(stores: &Stores) {
  stores.addresses.open_new();
  stores.ui.open(Overlay::AddressDialog);
}

/// `e`: the edit dialog of address `id`.
pub fn edit_address(stores: &Stores, id: u32) {
  stores.addresses.open_edit(id);
  stores.ui.open(Overlay::AddressDialog);
}

/// 关闭地址弹窗。已经发出的保存请求会在后台完成。
pub fn cancel_address(stores: &Stores) {
  stores.addresses.close_dialog();
  stores.ui.close_if(Overlay::AddressDialog);
}

/// `⏎` in the address dialog: saves (`已新增地址` / `地址已更新` + `{地区} {详细地址}`); a
/// field error stays in the dialog.
pub fn save_address(stores: &Stores) {
  stores.spawn(save_address_draft(*stores));
}

/// 保存地址并刷新相关数据。仅当前草稿的成功结果会关闭弹窗、显示提示。
async fn save_address_draft(stores: Stores) {
  let refresh_cart = match stores.addresses.save().await {
    Ok(Some(saved)) => {
      if saved.current_dialog {
        stores.ui.close_if(Overlay::AddressDialog);
        let title = if saved.created {
          "已新增地址"
        } else {
          "地址已更新"
        };
        stores.toasts.ok(
          title,
          format!("{} {}", saved.address.region, saved.address.detail),
        );
      }
      stores.profile.invalidate();
      true
    }
    Ok(None) => return,
    Err(failure) => {
      session_expired(&stores, &failure);
      false
    }
  };
  if stores.addresses.refresh_after_changes().await && refresh_cart {
    stores.cart.fetch().await;
  }
}

#[cfg(test)]
mod address_saves {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::{
    api::{ApiClient, Script, Stepped, Transport},
    stores::{AddressCursor, AddressField, Phase, SessionStore, addresses::DialogStop},
    ui::Viewport,
  };

  const LOGIN: &str = include_str!("../../../../packages/contract/fixtures/v1/auth/login.json");
  const BOOK: &str = include_str!("../../../../packages/contract/fixtures/v1/addresses/list.json");
  const SAVED: &str =
    include_str!("../../../../packages/contract/fixtures/v1/addresses/saved-no-phone.json");
  const UPDATED: &str =
    include_str!("../../../../packages/contract/fixtures/v1/addresses/updated.json");
  const CART: &str = include_str!("../../../../packages/contract/fixtures/v1/cart/cart.json");
  const PROFILE: &str =
    include_str!("../../../../packages/contract/fixtures/v1/profile/profile.json");
  const PHONE_ERROR: &str =
    include_str!("../../../../packages/contract/fixtures/v1/errors/phone_invalid.json");
  const EXPIRED: &str =
    include_str!("../../../../packages/contract/fixtures/v1/errors/session_expired.json");

  /// 创建已加载地址本、购物车和个人资料的会员，测试自行决定请求的响应顺序。
  fn with_script(test: impl FnOnce(Script, Stores)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let script = Script::default();
    let (owner, stores) = create_root(|| {
      let client = ApiClient::new(Transport::Script(script.clone()), None);
      let session = SessionStore::with_client(client);
      session.signed_in(serde_json::from_str(LOGIN).unwrap());
      Stores::new(session, || Viewport::new(130, 40))
    });
    let load = Stepped::start(stores.addresses.fetch());
    script.answer(0, 200, BOOK);
    load.finish();
    let load = Stepped::start(stores.cart.fetch());
    script.answer(0, 200, CART);
    load.finish();
    let load = Stepped::start(stores.profile.fetch());
    script.answer(0, 200, PROFILE);
    load.finish();
    stores.router.replace(Route::Addresses);
    test(script, stores);
    owner.dispose();
  }

  #[test]
  fn 旧草稿保存成功时保留后来打开的草稿() {
    for editing in [false, true] {
      with_script(|script, stores| {
        new_address(&stores);
        let mut save = Stepped::start(save_address_draft(stores));
        cancel_address(&stores);
        stores.addresses.select(AddressCursor::Card(2));
        if editing {
          edit_address(&stores, 2);
        } else {
          new_address(&stores);
        }
        stores
          .addresses
          .edit(|input| input.recipient = "第二份草稿".into());
        stores
          .addresses
          .focus_dialog(DialogStop::Field(AddressField::Detail));
        let draft = stores.addresses.dialog();
        let cart = stores.cart.cart();

        script.answer(0, 200, SAVED);
        assert!(save.poll().is_none(), "成功结果会刷新购物车");
        assert_eq!(stores.addresses.count(), 4);
        assert_eq!(stores.addresses.cursor(), AddressCursor::Card(2));
        assert_eq!(stores.addresses.dialog(), draft);
        assert_eq!(
          stores.addresses.dialog_focus(),
          DialogStop::Field(AddressField::Detail)
        );
        assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
        assert!(stores.toasts.queue().is_empty());
        assert_eq!(stores.profile.phase(), Phase::Idle);
        assert_eq!(stores.cart.cart(), cart);
        assert_eq!(script.calls(), ["GET /cart"]);
        script.answer(0, 200, CART);
        save.finish();
      });
    }
  }

  #[test]
  fn 旧草稿保存失败时保留新草稿的保存状态() {
    for network in [false, true] {
      with_script(|script, stores| {
        new_address(&stores);
        let old_save = Stepped::start(save_address_draft(stores));
        cancel_address(&stores);
        edit_address(&stores, 2);
        let current_save = Stepped::start(save_address_draft(stores));
        stores.addresses.focus_dialog(DialogStop::Save);
        let draft = stores.addresses.dialog();
        assert!(stores.addresses.saving());
        if network {
          script.drop_connection(0);
        } else {
          script.answer(0, 422, PHONE_ERROR);
        }
        old_save.finish();
        assert_eq!(stores.addresses.dialog(), draft);
        assert_eq!(stores.addresses.dialog_focus(), DialogStop::Save);
        assert!(stores.busy());
        assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
        assert!(stores.toasts.queue().is_empty());
        assert_eq!(stores.profile.phase(), Phase::Ready);
        assert_eq!(script.calls(), ["PUT /addresses/2"]);

        script.answer(0, 422, PHONE_ERROR);
        current_save.finish();
        assert!(!stores.addresses.saving());
        assert_eq!(
          stores.addresses.dialog_focus(),
          DialogStop::Field(AddressField::Phone)
        );
        assert_eq!(
          stores.addresses.dialog().unwrap().error.unwrap().message,
          "手机号应为 11 位数字"
        );
      });
    }
  }

  #[test]
  fn 后一次保存先成功时旧响应不会覆盖地址本和光标() {
    with_script(|script, stores| {
      new_address(&stores);
      let old_save = Stepped::start(save_address_draft(stores));
      cancel_address(&stores);
      edit_address(&stores, 2);
      let mut current_save = Stepped::start(save_address_draft(stores));
      script.answer(1, 200, UPDATED);
      assert!(current_save.poll().is_none());
      script.answer(1, 200, CART);
      current_save.finish();
      let book = stores.addresses.state();
      let toasts = stores.toasts.queue();
      assert_eq!(toasts.len(), 1);
      assert_eq!(toasts[0].title.as_ref(), "地址已更新");
      assert_eq!(stores.ui.overlay(), None);
      assert_eq!(stores.addresses.dialog(), None);

      let mut saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
      let updated: serde_json::Value = serde_json::from_str(UPDATED).unwrap();
      saved["book"]["items"][1] = updated["address"].clone();
      script.answer(0, 200, saved.to_string());
      let mut old_save = old_save;
      assert!(old_save.poll().is_none());
      assert_eq!(stores.addresses.state().data(), book.data());
      assert_eq!(stores.addresses.cursor(), AddressCursor::Card(2));
      assert_eq!(stores.addresses.dialog(), None);
      assert_eq!(stores.toasts.queue(), toasts);
      assert_eq!(script.calls(), ["GET /addresses"]);
      script.answer(0, 200, saved["book"].to_string());
      assert!(old_save.poll().is_none());
      script.answer(0, 200, CART);
      old_save.finish();
    });
  }

  #[test]
  fn 后一次保存先失败时旧保存成功后刷新地址本() {
    for refresh_fails in [false, true] {
      with_script(|script, stores| {
        new_address(&stores);
        let mut old_save = Stepped::start(save_address_draft(stores));
        cancel_address(&stores);
        edit_address(&stores, 2);
        let current_save = Stepped::start(save_address_draft(stores));

        script.answer(1, 422, PHONE_ERROR);
        current_save.finish();
        let draft = stores.addresses.dialog();
        let focus = stores.addresses.dialog_focus();
        script.answer(0, 200, SAVED);
        assert!(old_save.poll().is_none(), "保存冲突后需要补读地址本");
        assert_eq!(script.calls(), ["GET /addresses"]);
        let saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
        if refresh_fails {
          script.drop_connection(0);
          assert!(old_save.poll().is_none());
          assert_eq!(stores.addresses.phase(), Phase::Failed);
          assert!(stores.addresses.failure().unwrap().is_network());
          assert_eq!(stores.addresses.count(), 3);
          let retry = Stepped::start(stores.addresses.fetch());
          script.answer(1, 200, saved["book"].to_string());
          retry.finish();
        } else {
          script.answer(0, 200, saved["book"].to_string());
          assert!(old_save.poll().is_none());
        }
        script.answer(0, 200, CART);
        old_save.finish();

        assert_eq!(stores.addresses.count(), 4);
        assert_eq!(stores.addresses.cursor(), AddressCursor::Card(1));
        assert_eq!(stores.addresses.dialog(), draft);
        assert_eq!(stores.addresses.dialog_focus(), focus);
        assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
        assert!(stores.toasts.queue().is_empty());
        assert!(script.calls().is_empty());
      });
    }
  }

  #[test]
  fn 旧保存成功后等其他地址修改结束再补读地址本() {
    for deleting in [false, true] {
      with_script(|script, stores| {
        new_address(&stores);
        let mut save = Stepped::start(save_address_draft(stores));
        cancel_address(&stores);
        let mut change = Stepped::start(async move {
          if deleting {
            stores.addresses.delete(1).await.unwrap();
          } else {
            stores.addresses.set_default(3).await.unwrap();
          }
          stores.addresses.refresh_after_changes().await;
        });
        let changed = if deleting {
          include_str!("../../../../packages/contract/fixtures/v1/addresses/deleted-default.json")
        } else {
          include_str!("../../../../packages/contract/fixtures/v1/addresses/default-moved.json")
        };
        let mut expected: serde_json::Value = serde_json::from_str(changed).unwrap();
        let mut saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
        expected["items"]
          .as_array_mut()
          .unwrap()
          .push(saved["address"].clone());
        saved["book"] = expected.clone();
        script.answer(0, 200, saved.to_string());
        assert!(save.poll().is_none());
        assert!(!script.calls().iter().any(|call| call == "GET /addresses"));
        script.answer(1, 200, CART);
        save.finish();

        script.answer(0, 200, changed);
        assert!(change.poll().is_none());
        assert_eq!(script.calls(), ["GET /addresses"]);
        script.answer(0, 200, expected.to_string());
        change.finish();
        let expected: coffee_api::AddressList = serde_json::from_value(expected).unwrap();
        assert_eq!(stores.addresses.state().data(), Some(&expected));
        assert!(script.calls().is_empty());
      });
    }
  }

  #[test]
  fn 补读地址本期间的新保存会丢弃旧补读结果() {
    for success in [false, true] {
      with_script(|script, stores| {
        new_address(&stores);
        let mut old_save = Stepped::start(save_address_draft(stores));
        cancel_address(&stores);
        edit_address(&stores, 2);
        let current_save = Stepped::start(save_address_draft(stores));
        script.answer(1, 422, PHONE_ERROR);
        current_save.finish();
        script.answer(0, 200, SAVED);
        assert!(old_save.poll().is_none());
        let mut next_save = Stepped::start(save_address_draft(stores));
        script.answer(
          1,
          if success { 200 } else { 422 },
          if success { UPDATED } else { PHONE_ERROR },
        );
        assert!(next_save.poll().is_none());
        new_address(&stores);
        stores
          .addresses
          .edit(|input| input.recipient = "刚打开的草稿".into());
        let draft = stores.addresses.dialog();
        let book = stores.addresses.state();
        script.answer(0, 200, BOOK);
        assert!(old_save.poll().is_none());
        assert_eq!(stores.addresses.state(), book);
        assert_eq!(stores.addresses.dialog(), draft);

        let mut saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
        if success {
          let updated: serde_json::Value = serde_json::from_str(UPDATED).unwrap();
          saved["book"]["items"][1] = updated["address"].clone();
        }
        script.answer(0, 200, saved["book"].to_string());
        if success {
          assert!(next_save.poll().is_none());
          script.answer(1, 200, CART);
          next_save.finish();
        } else {
          next_save.finish();
        }
        script.answer(0, 200, CART);
        old_save.finish();
        assert_eq!(stores.addresses.count(), 4);
        assert_eq!(stores.addresses.dialog(), draft);
        assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
        assert!(script.calls().is_empty());
      });
    }
  }

  #[test]
  fn 退出登录前的地址请求不会改变新会话() {
    for refreshing in [false, true] {
      for success in [false, true] {
        with_script(|script, stores| {
          new_address(&stores);
          let mut save = Stepped::start(save_address_draft(stores));
          if refreshing {
            cancel_address(&stores);
            edit_address(&stores, 2);
            let current_save = Stepped::start(save_address_draft(stores));
            script.answer(1, 422, PHONE_ERROR);
            current_save.finish();
            script.answer(0, 200, SAVED);
            assert!(save.poll().is_none());
            assert_eq!(script.calls(), ["GET /addresses"]);
          }
          sign_out_locally(&stores);
          stores
            .session
            .signed_in(serde_json::from_str(LOGIN).unwrap());
          new_address(&stores);
          stores
            .addresses
            .edit(|input| input.recipient = "另一次登录的草稿".into());
          let draft = stores.addresses.dialog();
          if success {
            script.answer(0, 200, if refreshing { BOOK } else { SAVED });
          } else {
            script.answer(0, 401, EXPIRED);
          }
          save.finish();
          assert!(stores.session.is_member());
          assert_eq!(stores.addresses.phase(), Phase::Idle);
          assert_eq!(stores.addresses.dialog(), draft);
          assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
          assert!(stores.toasts.queue().is_empty());
          assert!(script.calls().is_empty());
          assert_eq!(stores.cart.phase(), Phase::Idle);
          assert_eq!(stores.profile.phase(), Phase::Idle);
        });
      }
    }
  }
}

// --- Settings ---------------------------------------------------------------------------

/// `m` / `o`: toggles a setting, with its toast.
pub fn toggle_setting(stores: &Stores, setting: Setting) {
  let stores = *stores;
  stores.spawn(async move {
    match stores.settings.toggle(setting).await {
      Ok(on) => {
        let (title, sub) = match (setting, on) {
          (Setting::Animations, true) => ("界面动画已开启", "蒸汽、盘旋字符与进度条恢复动画"),
          (Setting::Animations, false) => ("界面动画已关闭", "所有画面即时切换"),
          (Setting::MouseTracking, true) => ("鼠标追踪已开启", "悬停即移动高亮"),
          (Setting::MouseTracking, false) => ("鼠标追踪已关闭", "点击仍然可用"),
        };
        stores.toasts.info(title, sub);
      }
      Err(failure) => failed(&stores, &failure),
    }
  });
}

#[cfg(test)]
mod tests {
  use coffee_api::Weight;

  use super::*;

  #[test]
  fn copy_of_lines_and_order_failures() {
    let line = GuestCartLine {
      product_id: "yirga".into(),
      weight: Weight::G250,
      grind: Grind::WholeBean,
      pack: Pack::Standard,
      quantity: 1,
      selected: true,
    };
    assert_eq!(
      line_summary("耶加雪菲 孔加", &line),
      "耶加雪菲 孔加 · 250g · 整豆 × 1"
    );
    let stock = crate::api::decode::<coffee_api::Cart>(
      409,
      include_bytes!("../../../../packages/contract/fixtures/v1/errors/out_of_stock.json"),
    )
    .unwrap_err();
    assert_eq!(order_failure(&stock), "波奎特 瑰夏 库存不足");
    let sold = crate::api::decode::<coffee_api::Cart>(
      409,
      include_bytes!("../../../../packages/contract/fixtures/v1/errors/sold_out.json"),
    )
    .unwrap_err();
    assert_eq!(order_failure(&sold), "慧兰 粉红波旁 1kg 暂时缺货");
    assert_eq!(
      order_failure(&ApiFailure::of(ErrorCode::AddressRequired)),
      "请先选择收货地址"
    );
    assert_eq!(
      order_failure(&ApiFailure::Network("x".into())),
      "连接服务器失败"
    );
  }
}

/// The flows across stores, on the fixture transport with tasks run in place.
#[cfg(test)]
mod flows {
  use nsh_core::prelude::untracked;

  use super::*;
  use crate::{
    api::FixtureSet,
    commands::{self, KeyContext},
    stores::{Phase, testing::with_stores},
  };

  const KEY: &str = "SHA256:fixture-key";

  fn titles(stores: &Stores) -> Vec<String> {
    stores
      .toasts
      .queue()
      .into_iter()
      .map(|t| format!("{} / {}", t.title, t.sub))
      .collect()
  }

  fn last_toast(stores: &Stores) -> String {
    titles(stores).pop().unwrap_or_default()
  }

  /// Presses `stroke` as the shell's binding does.
  fn key(stores: &Stores, stroke: &str) -> Option<&'static str> {
    let command = untracked(|| {
      let ctx = commands::context(stores);
      commands::resolve(&ctx, stroke, |c| (c.enabled)(stores))
    })?;
    (command.run)(stores, stroke);
    Some(command.id)
  }

  fn fill_login(stores: &Stores, username: &str, password: &str) {
    stores.session.set_username(username);
    stores.session.set_password(password);
  }

  #[test]
  fn guest_browse_add_gated_checkout_login_and_the_merged_cart() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      // A new connection without a key starts on the login page.
      start(&stores);
      assert_eq!(stores.router.top(), Route::Login);
      assert_eq!(key(&stores, "g"), Some("login.guest"));
      assert_eq!(stores.router.top(), Route::Catalog);
      assert_eq!(last_toast(&stores), "以访客身份浏览 / 结算前需要登录");

      stores.catalog.ensure_loaded();
      assert_eq!(stores.catalog.phase(), Phase::Ready);
      assert_eq!(stores.cart.item_count(), 0);
      assert_eq!(key(&stores, "a"), Some("catalog.add"));
      assert_eq!(
        last_toast(&stores),
        "已加入购物车 / 耶加雪菲 孔加 · 250g · 整豆 × 1"
      );
      assert_eq!(
        stores.cart.item_count(),
        2,
        "the badge is the quote's count"
      );

      assert_eq!(key(&stores, "2"), Some("nav.cart"));
      assert_eq!(key(&stores, "c"), Some("cart.checkout"));
      assert_eq!(last_toast(&stores), "结算前请先登录 / 访客可以浏览和加购");
      assert_eq!(stores.router.top(), Route::Login, "the login gate");
      assert_eq!(stores.session.return_to(), Some(Route::Cart));

      fill_login(&stores, "林晓", "secret1");
      assert_eq!(key(&stores, "enter"), Some("login.submit"));
      assert!(stores.session.is_member());
      assert_eq!(stores.router.top(), Route::Cart, "back to the cart (K7)");
      assert_eq!(stores.cart.item_count(), 3, "the merged member cart");
      assert_eq!(stores.cart.guest().lines.len(), 0);
      assert_eq!(
        last_toast(&stores),
        "欢迎回来，林晓 / 已合并访客购物车 1 件"
      );
      assert_eq!(stores.addresses.count(), 3, "addresses synced at login");

      // Now checkout goes through: the receipt and its keys.
      assert_eq!(key(&stores, "c"), Some("cart.checkout"));
      assert_eq!(
        stores.cart.receipt().map(|o| o.number),
        Some("RT2611220417".into())
      );
      assert_eq!(
        last_toast(&stores),
        "订单已提交 / RT2611220417 · 下一个烘焙日出炉"
      );
      assert_eq!(key(&stores, "enter"), Some("cart.continue"));
      assert_eq!(stores.router.top(), Route::Catalog);
      assert!(!stores.cart.has_receipt());
    });
  }

  #[test]
  fn a_merge_that_fails_at_login_says_so_and_keeps_the_guest_lines() {
    // `fixture:member-error`: signing in and the quote work, every member call answers 500.
    with_stores(FixtureSet::MemberError, None, (100, 30), |stores| {
      start(&stores);
      assert_eq!(key(&stores, "g"), Some("login.guest"));
      stores.catalog.ensure_loaded();
      assert_eq!(key(&stores, "a"), Some("catalog.add"));
      assert_eq!(stores.cart.item_count(), 2);
      assert_eq!(key(&stores, "2"), Some("nav.cart"));
      assert_eq!(key(&stores, "c"), Some("cart.checkout"));
      assert_eq!(stores.router.top(), Route::Login, "the login gate");

      fill_login(&stores, "林晓", "secret1");
      assert_eq!(key(&stores, "enter"), Some("login.submit"));
      assert!(stores.session.is_member());
      assert_eq!(stores.router.top(), Route::Cart);
      assert_eq!(
        last_toast(&stores),
        "欢迎回来，林晓 / 购物车同步失败 · 在购物车按 ⏎ 重试",
        "never `已同步` or `已合并`"
      );
      assert_eq!(
        stores.cart.cart(),
        None,
        "no guest lines shown as the account's"
      );
      assert_eq!(
        stores.cart.guest().lines.len(),
        1,
        "the guest lines are kept"
      );
      assert!(stores.cart.merge_pending());
      // `c` cannot check out a cart that is not there; `⏎` retries the merge.
      assert_eq!(key(&stores, "c"), None);
      assert_eq!(key(&stores, "enter"), Some("cart.retry"));
      assert_eq!(stores.cart.phase(), Phase::Failed);
      assert_eq!(stores.cart.guest().lines.len(), 1);
    });
  }

  #[test]
  fn a_login_without_a_guest_cart_reports_the_sync() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      fill_login(&stores, "林晓", "secret1");
      submit_auth(&stores);
      assert_eq!(stores.router.top(), Route::Catalog);
      assert_eq!(
        last_toast(&stores),
        "欢迎回来，林晓 / 已同步 3 件购物车商品与 3 个地址"
      );
    });
  }

  #[test]
  fn a_wrong_password_stays_on_the_form() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      fill_login(&stores, "林晓", "wrongpass");
      submit_auth(&stores);
      assert!(!stores.session.is_member());
      assert_eq!(stores.router.top(), Route::Login);
      let draft = untracked(|| stores.session.draft());
      assert_eq!(draft.error.as_deref(), Some("用户名或密码不正确"));
      assert_eq!(draft.password, "");
      // Local validation first: no request.
      fill_login(&stores, "", "");
      submit_auth(&stores);
      assert_eq!(
        untracked(|| stores.session.draft()).error.as_deref(),
        Some("请输入用户名")
      );
    });
  }

  #[test]
  fn registering_signs_in() {
    with_stores(FixtureSet::Contract, Some(KEY), (100, 30), |stores| {
      stores.session.set_mode(LoginMode::Register);
      assert_eq!(commands::mode(&stores).en, "SIGN UP");
      fill_login(&stores, "bean_lover", "secret1");
      assert_eq!(key(&stores, "enter"), Some("login.register_submit"));
      assert_eq!(stores.session.user().as_deref(), Some("bean_lover"));
      assert_eq!(
        last_toast(&stores),
        "注册成功，欢迎 bean_lover / 已为你登录 · 购物车已同步"
      );
    });
  }

  #[test]
  fn a_remembered_key_resumes_on_connect() {
    with_stores(FixtureSet::Member, Some(KEY), (100, 30), |stores| {
      start(&stores);
      assert!(stores.session.is_member());
      assert!(!stores.session.resuming());
      assert_eq!(stores.router.top(), Route::Catalog);
      assert_eq!(last_toast(&stores), "欢迎回来，林晓 / 已自动恢复登录");
      assert_eq!(stores.cart.item_count(), 3);
      assert!(stores.session.key_remembered());
    });
    // A key nobody remembered: the login page, no toast.
    with_stores(FixtureSet::Contract, Some(KEY), (100, 30), |stores| {
      start(&stores);
      assert!(!stores.session.is_member());
      assert!(!stores.session.resuming());
      assert_eq!(stores.router.top(), Route::Login);
      assert!(titles(&stores).is_empty());
    });
    // No key: no request at all.
    with_stores(FixtureSet::Member, None, (100, 30), |stores| {
      start(&stores);
      assert!(!stores.session.is_member());
    });
  }

  #[test]
  fn logout_asks_first_and_q_q_confirms() {
    with_stores(FixtureSet::Member, Some(KEY), (100, 30), |stores| {
      // A guest's q does nothing (K3).
      stores.router.replace(Route::Catalog);
      assert_eq!(key(&stores, "q"), None);
      start(&stores);
      assert_eq!(key(&stores, "q"), Some("app.logout"));
      assert_eq!(stores.ui.overlay(), Some(Overlay::Logout));
      assert_eq!(commands::mode(&stores).en, "CONFIRM");
      // ⏎ on 取消 cancels.
      assert_eq!(key(&stores, "tab"), Some("logout.switch"));
      assert_eq!(key(&stores, "enter"), Some("logout.enter"));
      assert_eq!(stores.ui.overlay(), None);
      assert!(stores.session.is_member());
      // Esc cancels.
      key(&stores, "q");
      assert_eq!(key(&stores, "escape"), Some("logout.cancel"));
      assert!(stores.session.is_member());
      // q q logs out.
      key(&stores, "q");
      assert_eq!(key(&stores, "q"), Some("logout.confirm"));
      assert!(!stores.session.is_member());
      assert_eq!(stores.ui.overlay(), None);
      assert_eq!(stores.router.top(), Route::Login);
      assert_eq!(
        stores.cart.item_count(),
        0,
        "the member cart stays in the account"
      );
      assert_eq!(last_toast(&stores), "已退出登录 / 按 ⏎ 重新登录");
    });
  }

  #[test]
  fn an_expired_session_goes_back_to_login_and_returns() {
    with_stores(FixtureSet::Member, Some(KEY), (100, 30), |stores| {
      start(&stores);
      actions_go_cart(&stores);
      let expired = ApiFailure::of(ErrorCode::SessionExpired);
      assert!(session_expired(&stores, &expired));
      assert!(!stores.session.is_member());
      assert_eq!(stores.router.top(), Route::Login);
      assert_eq!(stores.session.return_to(), Some(Route::Cart));
      assert_eq!(last_toast(&stores), "登录已失效 / 请重新登录，购物车已保留");
      // A guest's failure is only a toast.
      assert!(!session_expired(&stores, &expired));
      failed(&stores, &ApiFailure::Network("x".into()));
      assert_eq!(
        last_toast(&stores),
        "操作没有完成 / 连接服务器失败，请稍后重试"
      );
    });
  }

  #[test]
  fn a_session_that_expires_during_the_login_sync_stays_on_the_login_page() {
    // fixture:expired signs in, then every member call answers `401 session_expired`.
    with_stores(FixtureSet::Expired, None, (100, 30), |stores| {
      start(&stores);
      assert_eq!(stores.router.top(), Route::Login);
      fill_login(&stores, "林晓", "secret1");
      assert_eq!(key(&stores, "enter"), Some("login.submit"));
      assert!(!stores.session.is_member());
      assert_eq!(stores.router.top(), Route::Login, "not the catalog");
      assert!(!stores.session.submitting(), "the form is usable again");
      assert_eq!(last_toast(&stores), "登录已失效 / 请重新登录，购物车已保留");
      assert!(
        !titles(&stores).iter().any(|t| t.starts_with("欢迎回来")),
        "no welcome: {:?}",
        titles(&stores)
      );
    });
  }

  fn actions_go_cart(stores: &Stores) {
    go_tab(stores, 1);
    stores.cart.ensure_loaded();
  }

  #[test]
  fn cart_keys_change_the_member_cart_with_undo() {
    with_stores(FixtureSet::Member, Some(KEY), (100, 30), |stores| {
      start(&stores);
      actions_go_cart(&stores);
      assert_eq!(key(&stores, "space"), Some("cart.toggle"));
      assert_eq!(stores.cart.selected_count(), 2);
      assert_eq!(key(&stores, "a"), Some("cart.all"));
      assert_eq!(stores.cart.selected_count(), 3);
      assert_eq!(key(&stores, "x"), Some("cart.remove"));
      assert_eq!(
        last_toast(&stores),
        "已从购物车移除 / 慧兰 粉红波旁 · 按 u 撤销"
      );
      assert_eq!(stores.cart.lines().len(), 2);
      assert_eq!(key(&stores, "u"), Some("cart.undo"));
      assert_eq!(last_toast(&stores), "已恢复 / 慧兰 粉红波旁");
      assert_eq!(key(&stores, "u"), None, "nothing left to undo");
      // Esc back to the beans.
      assert_eq!(key(&stores, "escape"), Some("cart.back"));
      assert_eq!(stores.router.top(), Route::Catalog);
    });
  }

  #[test]
  fn checkout_checks_selection_and_address() {
    with_stores(FixtureSet::Empty, Some(KEY), (100, 30), |stores| {
      fill_login(&stores, "林晓", "secret1");
      submit_auth(&stores);
      go_tab(&stores, 1);
      checkout(&stores);
      assert_eq!(
        last_toast(&stores),
        "还没有勾选商品 / 用 space 勾选要结算的豆子"
      );
      assert!(!stores.cart.submitting());
    });
  }

  #[test]
  fn the_detail_page_adds_its_selection_and_b_buys_now() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      continue_as_guest(&stores);
      open_product(&stores, "geisha");
      assert_eq!(stores.router.top(), Route::Detail("geisha".into()));
      assert_eq!(stores.product.phase(), Phase::Ready);
      assert_eq!(key(&stores, "shift+="), Some("detail.quantity"));
      assert_eq!(key(&stores, "a"), Some("detail.add"));
      assert_eq!(
        last_toast(&stores),
        "已加入购物车 / 波奎特 瑰夏 · 250g · 整豆 × 2"
      );
      assert_eq!(key(&stores, "b"), Some("detail.buy"));
      assert_eq!(stores.router.top(), Route::Cart, "b goes to the cart");
      // Back from a detail keeps the list.
      open_product(&stores, "pinkb");
      assert_eq!(key(&stores, "backspace"), Some("detail.back"));
      assert_eq!(stores.router.top(), Route::Catalog);
    });
  }

  #[test]
  fn addresses_use_new_save_and_guest_card() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      continue_as_guest(&stores);
      go_tab(&stores, 2);
      assert_eq!(key(&stores, "enter"), Some("addresses.login"));
      assert_eq!(stores.router.top(), Route::Login);
      fill_login(&stores, "林晓", "secret1");
      submit_auth(&stores);
      assert_eq!(
        stores.router.top(),
        Route::Addresses,
        "back where the guest was"
      );
      assert_eq!(key(&stores, "right"), Some("addresses.move"));
      assert_eq!(key(&stores, "enter"), Some("addresses.use"));
      assert_eq!(last_toast(&stores), "本单寄往这里 / 林晓 · 上海市 静安区");
      assert_eq!(stores.router.top(), Route::Cart);
      go_tab(&stores, 2);
      assert_eq!(key(&stores, "n"), Some("addresses.new"));
      assert_eq!(stores.ui.overlay(), Some(Overlay::AddressDialog));
      assert_eq!(commands::mode(&stores).en, "INSERT");
      assert_eq!(key(&stores, "enter"), Some("dialog.save"));
      let draft = untracked(|| stores.addresses.dialog()).unwrap();
      assert_eq!(
        draft.error.map(|e| e.message).as_deref(),
        Some("请填写收货人")
      );
      stores.addresses.edit(|input| {
        input.recipient = "周一".into();
        input.region = "上海市".into();
        input.detail = "某路 1 号".into();
      });
      assert_eq!(key(&stores, "enter"), Some("dialog.save"));
      assert_eq!(stores.ui.overlay(), None);
      assert!(last_toast(&stores).starts_with("已新增地址 / "));
      assert_eq!(key(&stores, "escape"), Some("addresses.back"));
      assert_eq!(stores.router.top(), Route::Cart);
    });
  }

  #[test]
  fn settings_toggle_with_their_toasts() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      continue_as_guest(&stores);
      go_tab(&stores, 3);
      assert_eq!(key(&stores, "m"), Some("profile.animations"));
      assert!(!stores.settings.animations());
      assert_eq!(last_toast(&stores), "界面动画已关闭 / 所有画面即时切换");
      assert_eq!(key(&stores, "o"), Some("profile.mouse"));
      assert_eq!(last_toast(&stores), "鼠标追踪已关闭 / 点击仍然可用");
    });
  }

  #[test]
  fn f_cycles_the_origin_or_opens_the_filter_dialog_at_s() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      continue_as_guest(&stores);
      stores.catalog.ensure_loaded();
      key(&stores, "f");
      assert_eq!(stores.catalog.query().origin.as_deref(), Some("埃塞俄比亚"));
      assert_eq!(stores.catalog.count(), 1, "the server filtered the list");
      assert_eq!(key(&stores, "escape"), Some("catalog.clear"));
      assert_eq!(stores.catalog.count(), 8);
    });
    with_stores(FixtureSet::Contract, None, (80, 24), |stores| {
      continue_as_guest(&stores);
      stores.catalog.ensure_loaded();
      assert_eq!(key(&stores, "f"), Some("catalog.origin"));
      assert_eq!(stores.ui.overlay(), Some(Overlay::FilterDialog));
      assert_eq!(key(&stores, "r"), Some("filter.jump"));
      assert_eq!(key(&stores, "j"), Some("filter.move"));
      assert_eq!(key(&stores, "enter"), Some("filter.choose"));
      assert_eq!(
        stores.catalog.query().roast.map(|r| r.code()),
        Some("light")
      );
      assert_eq!(stores.catalog.count(), 4);
      assert_eq!(key(&stores, "escape"), Some("filter.close"));
      assert_eq!(stores.ui.overlay(), None);
    });
  }

  #[test]
  fn the_search_key_focuses_the_search_box() {
    with_stores(FixtureSet::Contract, None, (100, 30), |stores| {
      continue_as_guest(&stores);
      go_tab(&stores, 1);
      assert_eq!(key(&stores, "/"), Some("app.search"));
      assert_eq!(stores.router.top(), Route::Catalog);
      assert!(stores.ui.focus_requested(SEARCH_FIELD));
      // While the search box has the focus, letters are typed, Esc leaves it.
      stores.ui.field_focused(SEARCH_FIELD, None);
      assert_eq!(key(&stores, "a"), None);
      let ctx = untracked(|| commands::context(&stores));
      assert_eq!(
        ctx,
        KeyContext {
          typing: true,
          ..KeyContext::on(Page::Catalog)
        }
      );
      stores.catalog.set_search("茉莉");
      assert_eq!(stores.catalog.count(), 2);
      assert_eq!(key(&stores, "escape"), Some("catalog.leave_search"));
      assert!(!stores.ui.typing());
    });
  }
}
