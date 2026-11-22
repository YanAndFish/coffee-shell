//! The cart's keys (keymap.md, "购物车"; footers "购物车" and "回执"). While a checkout is
//! submitting only `?` works (K6); in the coupon field `⏎` applies and `Esc` leaves.

use crate::{
  commands::{Command, Key, Scope, Typing, search_hint, tabs_hint},
  router::Page,
  stores::{Phase, Stores, actions},
};

const PAGE: Scope = Scope::Page(Page::Cart);

fn has_lines(stores: &Stores) -> bool {
  !stores.cart.has_receipt() && !stores.cart.lines().is_empty()
}

fn failed(stores: &Stores) -> bool {
  !stores.cart.has_receipt() && stores.cart.phase() == Phase::Failed && stores.cart.cart().is_none()
}

fn shopping(stores: &Stores) -> bool {
  !stores.cart.has_receipt()
}

fn receipt(stores: &Stores) -> bool {
  stores.cart.has_receipt()
}

fn can_undo(stores: &Stores) -> bool {
  shopping(stores) && stores.cart.undo().is_some()
}

fn step(stores: &Stores, stroke: &str) {
  let delta = if matches!(stroke, "left" | "-" | "h") {
    -1
  } else {
    1
  };
  actions::step_line(stores, delta);
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new(
    "cart.move",
    Key::new("↑↓", &["down", "j", "up", "k"]),
    "↑↓ 移动",
    PAGE,
    |stores, stroke| {
      stores
        .cart
        .move_cursor(if matches!(stroke, "up" | "k") { -1 } else { 1 })
    },
  )
  .when(has_lines)
  .footer(10),
  Command::new(
    "cart.toggle",
    Key::new("space", &["space"]),
    "space 勾选",
    PAGE,
    |stores, _| actions::toggle_line(stores),
  )
  .when(has_lines)
  .footer(20),
  Command::new(
    "cart.quantity",
    Key::new("←→", &["right", "shift+=", "=", "l", "left", "-", "h"]),
    "←→ 数量",
    PAGE,
    step,
  )
  .when(has_lines)
  .footer_full(30),
  Command::new(
    "cart.remove",
    Key::new("x", &["x", "d", "delete"]),
    "x 删除",
    PAGE,
    |stores, _| actions::remove_line(stores),
  )
  .when(has_lines)
  .footer(40),
  Command::new(
    "cart.undo",
    Key::new("u", &["u"]),
    "u 撤销",
    PAGE,
    |stores, _| actions::undo_remove(stores),
  )
  .when(can_undo)
  .footer_full(50),
  Command::new(
    "cart.all",
    Key::new("a", &["a"]),
    "a 全选",
    PAGE,
    |stores, _| actions::toggle_all(stores),
  )
  .when(has_lines),
  Command::new(
    "cart.checkout",
    Key::new("c", &["c", "enter"]),
    "c 结算",
    PAGE,
    // A cart that never loaded: ⏎ retries (the error state's `⏎ 重试`; a command of its own
    // would need the core's command tests to know it, see the report).
    |stores, _| {
      if failed(stores) {
        stores.cart.load();
      } else {
        actions::checkout(stores);
      }
    },
  )
  .when(shopping)
  .footer(60),
  Command::new(
    "cart.back",
    Key::new("Esc", &["escape"]),
    "Esc 回到选购",
    PAGE,
    |stores, _| actions::go_tab(stores, 0),
  )
  .when(shopping),
  // The coupon field.
  Command::new(
    "cart.coupon",
    Key::new("⏎", &["enter"]),
    "⏎ 应用",
    PAGE,
    |stores, _| actions::apply_coupon(stores),
  )
  .typing(Typing::Only)
  .footer(10),
  Command::new(
    "cart.coupon_leave",
    Key::new("Esc", &["escape"]),
    "Esc 离开输入",
    PAGE,
    |stores, _| stores.ui.leave_field(),
  )
  .typing(Typing::Only)
  .footer(20),
  // The receipt.
  Command::new(
    "cart.continue",
    Key::new("⏎", &["enter", "escape"]),
    "⏎ 继续选购",
    PAGE,
    |stores, _| actions::continue_shopping(stores),
  )
  .when(receipt)
  .footer(10),
  Command::new(
    "cart.orders",
    Key::hint("4"),
    "4 查看订单",
    PAGE,
    |stores, _| actions::go_tab(stores, 3),
  )
  .when(receipt)
  .footer(20),
  tabs_hint("cart.tabs", Page::Cart)
    .when(receipt)
    .footer_full(30),
  search_hint("cart.search", Page::Cart)
    .when(receipt)
    .footer_full(40),
];
