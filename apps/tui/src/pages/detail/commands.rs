//! The detail page's keys (keymap.md, "详情"; footer "详情": `↑↓ 规格行` `←→ 选择` `+ 数量`
//! `a 加入` `b 购买` `Esc 返回`, S `←→ 选择` `a 加入` `b 购买` `Esc 返回`).

use crate::{
  commands::{Command, Key, Scope},
  router::Page,
  stores::{Phase, Stores, actions},
};

const PAGE: Scope = Scope::Page(Page::Detail);

fn loaded(stores: &Stores) -> bool {
  stores.product.detail().is_some()
}

fn buyable(stores: &Stores) -> bool {
  stores.product.can_buy()
}

fn loading(stores: &Stores) -> bool {
  stores.product.detail().is_none() && stores.product.phase().pending()
}

fn failed(stores: &Stores) -> bool {
  stores.product.phase() == Phase::Failed
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new(
    "detail.retry",
    Key::new("⏎", &["enter"]),
    "⏎ 重试",
    PAGE,
    |stores, _| stores.product.load(),
  )
  .when(failed)
  .footer(5),
  Command::new(
    "detail.rows",
    Key::new("↑↓", &["down", "j", "up", "k"]),
    "↑↓ 规格行",
    PAGE,
    |stores, stroke| {
      stores
        .product
        .move_focus(if matches!(stroke, "up" | "k") { -1 } else { 1 })
    },
  )
  .when(loaded)
  .footer_full(10),
  Command::new(
    "detail.choose",
    Key::new("←→", &["right", "l", "left", "h"]),
    "←→ 选择",
    PAGE,
    |stores, stroke| {
      stores.product.step(if matches!(stroke, "left" | "h") {
        -1
      } else {
        1
      })
    },
  )
  .when(loaded)
  .footer(20),
  Command::new(
    "detail.quantity",
    Key::new("+", &["shift+=", "=", "-", "shift+-"]),
    "+ 数量",
    PAGE,
    |stores, stroke| {
      stores
        .product
        .step_quantity(if matches!(stroke, "-" | "shift+-") {
          -1
        } else {
          1
        })
    },
  )
  .when(loaded)
  .footer_full(30),
  Command::new(
    "detail.add",
    Key::new("a", &["a"]),
    "a 加入",
    PAGE,
    |stores, _| actions::add_selection(stores, false),
  )
  .when(buyable)
  .footer(40),
  Command::new(
    "detail.buy",
    Key::new("b", &["b"]),
    "b 购买",
    PAGE,
    |stores, _| actions::add_selection(stores, true),
  )
  .when(buyable)
  .footer(50),
  // `a` / `b` before the product arrived: held, run once it is there (the page's effect),
  // so a quick `⏎ a` from the list is not lost. No hint: nothing can be bought yet.
  Command::new(
    "detail.ahead",
    Key::new("a", &["a", "b"]),
    "a 加入",
    PAGE,
    |stores, stroke| stores.product.type_ahead(stroke == "b"),
  )
  .when(loading),
  Command::new(
    "detail.back",
    Key::new("Esc", &["escape", "backspace"]),
    "Esc 返回",
    PAGE,
    |stores, _| actions::back(stores),
  )
  .footer(60),
];
