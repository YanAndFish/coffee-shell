//! The address book's keys (keymap.md, "地址本"; footers "地址本", "地址本（访客）") and the
//! address dialog's ("地址对话框"). While saving only `Esc` works in the dialog.

use coffee_api::AddressTag;

use crate::{
  commands::{Command, Key, Scope, Typing, search_hint, tabs_hint},
  router::Page,
  stores::{AddressCursor, Overlay, Phase, Stores, actions},
  ui::Bp,
};

const PAGE: Scope = Scope::Page(Page::Addresses);
const DIALOG: Scope = Scope::Overlay(Overlay::AddressDialog);

fn member(stores: &Stores) -> bool {
  stores.session.is_member()
}

fn guest(stores: &Stores) -> bool {
  !stores.session.is_member()
}

fn on_card(stores: &Stores) -> bool {
  member(stores) && matches!(stores.addresses.cursor(), AddressCursor::Card(_))
}

fn failed(stores: &Stores) -> bool {
  member(stores) && stores.addresses.phase() == Phase::Failed
}

fn cursor_id(stores: &Stores) -> Option<u32> {
  match nsh_core::prelude::untracked(|| stores.addresses.cursor()) {
    AddressCursor::Card(id) => Some(id),
    AddressCursor::New => None,
  }
}

/// `←` `→` move by one card, `↑` `↓` by a row of the grid (two columns from M, one at S).
fn step(stores: &Stores, stroke: &str) {
  let columns = if nsh_core::prelude::untracked(|| stores.bp()) <= Bp::S {
    1
  } else {
    2
  };
  let delta = match stroke {
    "left" | "h" => -1,
    "right" | "l" => 1,
    "up" | "k" => -columns,
    _ => columns,
  };
  stores.addresses.move_cursor(delta);
}

fn use_here(stores: &Stores, _: &str) {
  match cursor_id(stores) {
    Some(id) => actions::use_address(stores, id),
    None => actions::new_address(stores),
  }
}

fn cycle_tag(stores: &Stores, stroke: &str) {
  stores.addresses.edit(|input| {
    let at = AddressTag::ALL
      .iter()
      .position(|t| *t == input.tag)
      .unwrap_or(0);
    let next = if stroke == "left" {
      at.saturating_sub(1)
    } else {
      (at + 1).min(AddressTag::ALL.len() - 1)
    };
    input.tag = AddressTag::ALL[next];
  });
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  // A guest.
  Command::new(
    "addresses.login",
    Key::new("⏎", &["enter"]),
    "⏎ 去登录",
    PAGE,
    |stores, _| actions::go_login(stores),
  )
  .when(guest)
  .footer(10),
  tabs_hint("addresses.tabs", Page::Addresses)
    .when(guest)
    .footer(20),
  search_hint("addresses.search", Page::Addresses)
    .when(guest)
    .footer(30),
  // A member.
  Command::new(
    "addresses.retry",
    Key::new("⏎", &["enter"]),
    "⏎ 重试",
    PAGE,
    |stores, _| stores.addresses.load(),
  )
  .when(failed)
  .footer(5),
  Command::new(
    "addresses.move",
    Key::new("←↑↓→", &["left", "h", "right", "l", "up", "k", "down", "j"]),
    "←↑↓→ 移动",
    PAGE,
    step,
  )
  .short("↑↓ 移动")
  .when(member)
  .footer(10),
  Command::new(
    "addresses.use",
    Key::new("⏎", &["enter"]),
    "⏎ 用于本单",
    PAGE,
    use_here,
  )
  .when(member)
  .footer(20),
  Command::new(
    "addresses.new",
    Key::new("n", &["n"]),
    "n 新增",
    PAGE,
    |stores, _| actions::new_address(stores),
  )
  .when(member)
  .footer(30),
  Command::new(
    "addresses.edit",
    Key::new("e", &["e"]),
    "e 编辑",
    PAGE,
    |stores, _| {
      if let Some(id) = cursor_id(stores) {
        actions::edit_address(stores, id);
      }
    },
  )
  .when(on_card)
  .footer_full(40),
  Command::new(
    "addresses.default",
    Key::new("space", &["space"]),
    "space 默认",
    PAGE,
    |stores, _| {
      if let Some(id) = cursor_id(stores) {
        actions::set_default_address(stores, id);
      }
    },
  )
  .when(on_card)
  .footer_full(50),
  Command::new(
    "addresses.delete",
    Key::new("d", &["d", "x"]),
    "d 删除",
    PAGE,
    |stores, _| {
      if let Some(id) = cursor_id(stores) {
        actions::delete_address(stores, id);
      }
    },
  )
  .when(on_card)
  .footer(60),
  Command::new(
    "addresses.back",
    Key::new("Esc", &["escape"]),
    "Esc 回到购物车",
    PAGE,
    |stores, _| actions::go_tab(stores, 1),
  ),
  // The dialog.
  Command::new(
    "dialog.next",
    Key::hint("Tab"),
    "Tab 下一项",
    DIALOG,
    crate::commands::nothing,
  )
  .typing(Typing::Also)
  .footer(10),
  Command::new(
    "dialog.tag",
    Key::new("←→", &["left", "right"]),
    "←→ 标签",
    DIALOG,
    cycle_tag,
  )
  .footer_full(20),
  Command::new(
    "dialog.default",
    Key::new("space", &["space"]),
    "space 设为默认",
    DIALOG,
    |stores, _| {
      stores
        .addresses
        .edit(|input| input.is_default = !input.is_default)
    },
  )
  .short("space 默认")
  .footer(30),
  Command::new(
    "dialog.save",
    Key::new("⏎", &["enter"]),
    "⏎ 保存",
    DIALOG,
    |stores, _| actions::save_address(stores),
  )
  .typing(Typing::Also)
  .footer(40),
  Command::new(
    "dialog.cancel",
    Key::new("Esc", &["escape"]),
    "Esc 取消",
    DIALOG,
    |stores, _| actions::cancel_address(stores),
  )
  .typing(Typing::Also)
  .while_busy()
  .footer(50),
];
