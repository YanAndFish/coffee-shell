//! 购物车 CART (04-cart.md): the lines, selection, undo, coupon, order address, amounts and
//! checkout, then the receipt.
//!
//! Store: [`crate::stores::CartStore`] — `phase`, `failure`, `cart` (every amount, as the
//! server priced it), `lines`, `item_count`, `selected_count`, `total`, `cursor` /
//! `cursor_line` / `is_cursor` / `move_cursor` / `select_line`, `undo`, `coupon_input` /
//! `set_coupon_input` / `coupon_feedback`, `checkout_step` / `submitting`, `receipt`.
//! Actions: `actions::toggle_line`, `toggle_all`, `step_line`, `remove_line`,
//! `undo_remove`, `apply_coupon`, `checkout`, `continue_shopping`. The coupon field reports
//! its focus as [`COUPON_FIELD`]. A guest's cart is quoted, never computed.
//!
//! The body is a placeholder (plain lines and a bare coupon field); the cart screen
//! replaces it.

pub mod commands;

use coffee_api::{CartLineProblem, Order};
use nsh_core::{element::Screen, prelude::*};

use super::{
  PageModule, format,
  placeholder::{self, Line, phase_lines},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{CouponFeedback, Stores},
  ui::{self, Tone},
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

fn receipt_lines(order: &Order) -> Vec<Line> {
  let mut lines = vec![
    Line::new("订单回执 RECEIPT", Tone::Acc2),
    Line::text("✓ 订单已提交"),
    Line::soft(format!(
      "订单号 {} · 预计 {} 送达",
      order.number,
      format::month_day_zh(&order.eta_date)
    )),
  ];
  for item in &order.items {
    lines.push(Line::text(format!(
      "{} {} · {} · {} {} ┈ {}",
      item.name,
      item.weight.label(),
      item.grind.label(),
      item.pack.label(),
      format::times(item.quantity),
      format::yuan_cents(item.line_total)
    )));
  }
  lines.push(Line::new(
    format!("实付 {}", format::yuan_cents(order.total)),
    Tone::Acc,
  ));
  lines.push(Line::soft(format!(
    "寄往 {} · {} {}",
    order.address.recipient, order.address.region, order.address.detail
  )));
  lines.push(Line::new(
    "! 演示订单 · 没有真实扣款，也不会发货",
    Tone::Acc,
  ));
  lines.push(Line::text("⏎ 继续选购").press("cart.continue"));
  lines.push(Line::text("4 查看订单").press("cart.orders"));
  lines
}

fn lines(stores: Stores) -> Vec<Line> {
  let cart = stores.cart;
  if let Some(order) = cart.receipt() {
    return receipt_lines(&order);
  }
  let mut lines = phase_lines(
    cart.phase(),
    cart.failure(),
    "购物车",
    "购物车是空的 · 1 去挑豆子",
    None,
  );
  let Some(data) = cart.cart() else {
    return lines;
  };
  for line in &data.lines {
    let cursor = if cart.is_cursor(&line.sku) {
      "▶"
    } else {
      " "
    };
    let check = if line.selected { "[✓]" } else { "[ ]" };
    let problem = match line.problem {
      Some(CartLineProblem::SoldOut) => " · ! 该重量暂时缺货",
      Some(CartLineProblem::OutOfStock) => " · ! 库存不足",
      None => "",
    };
    lines.push(Line::new(
      format!(
        "{cursor} {check} {} · {} · {} · {} · {} {} · {}{problem}",
        line.name,
        line.weight.label(),
        line.grind.label(),
        line.pack.label(),
        format::yuan(line.unit_price),
        format::times(line.quantity),
        format::yuan_cents(line.line_total)
      ),
      if line.selected { Tone::Fg } else { Tone::Mute },
    ));
  }
  if let Some(removed) = cart.undo() {
    lines.push(Line::error(format!("已删除 {} · u 撤销", removed.name)).press("cart.undo"));
  }
  lines.push(if !stores.session.is_member() {
    Line::soft("收货地址：登录后选择收货地址")
  } else {
    match &data.address {
      Some(address) => Line::soft(format!(
        "收货地址：{} · {} {} · 3 更换 →",
        address.recipient, address.region, address.detail
      )),
      None => Line::error("还没有收货地址，去地址本添加一个"),
    }
  });
  match cart.coupon_feedback() {
    Some(CouponFeedback::Applied(coupon)) => lines.push(Line::new(
      format!("✓ 已使用 {} · {}", coupon.code, coupon.description),
      Tone::Acc2,
    )),
    Some(CouponFeedback::Invalid(message)) => lines.push(Line::error(format!("✗ {message}"))),
    None => {}
  }
  let totals = &data.totals;
  lines.push(Line::text(format!(
    "商品小计 {} · 优惠 {} · 运费 {} · 合计 {}",
    format::yuan_cents(totals.subtotal),
    format::discount(totals.discount),
    format::shipping(totals.shipping),
    format::yuan_cents(totals.total)
  )));
  lines.push(Line::soft(if totals.free_shipping.reached {
    format!("已满 ¥{} · 本单包邮", totals.free_shipping.threshold)
  } else {
    format!("再买 {} 即可包邮", format::yuan(totals.free_shipping.gap))
  }));
  lines.push(match cart.checkout_step() {
    Some(step) => Line::new(format!("⠋ {}", step.label()), Tone::Acc2),
    None => Line::text(format!("c 结算 {} 件", totals.selected_count)).press("cart.checkout"),
  });
  lines
}

/// The cart page (placeholder body).
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let cart = stores.cart;
  // The page's work: the cart (member) or its quote (guest), once.
  effect(move || untracked(|| cart.ensure_loaded()));
  ui::page(
    "购物车 CART",
    ui::panel(
      move || {
        SharedString::from(if cart.has_receipt() {
          "订单回执 RECEIPT".to_owned()
        } else {
          format!("购物车 · {} 件", cart.item_count())
        })
      },
      view()
        .flex_1()
        .child(placeholder::lines(move || lines(stores)))
        .child(placeholder::field(
          COUPON_FIELD,
          "优惠码",
          move || cart.coupon_input(),
          move |text| cart.set_coupon_input(text),
          false,
        )),
    ),
  )
}
