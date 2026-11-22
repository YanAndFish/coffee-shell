//! The list page's keys (keymap.md, "选购（列表）"; footer "选购": 7 hints, S `↑↓ 移动`
//! `⏎ 详情` `a 加入` `f 筛选`) and the S-width filter dialog's ("筛选对话框").

use crate::{
  commands::{Command, Key, Scope, Typing, search_hint},
  router::Page,
  stores::{Overlay, Phase, Stores, actions, catalog::FilterGroup},
  ui::Bp,
};

const PAGE: Scope = Scope::Page(Page::Catalog);
const DIALOG: Scope = Scope::Overlay(Overlay::FilterDialog);

fn ready(stores: &Stores) -> bool {
  stores.catalog.phase() == Phase::Ready
}

/// The list is there, or on its way (moves typed ahead apply when it arrives).
fn movable(stores: &Stores) -> bool {
  matches!(
    stores.catalog.phase(),
    Phase::Ready | Phase::Loading | Phase::Idle
  )
}

fn failed(stores: &Stores) -> bool {
  stores.catalog.phase() == Phase::Failed
}

fn filtered(stores: &Stores) -> bool {
  stores.catalog.query().is_filtered()
}

fn delta(stroke: &str) -> isize {
  match stroke {
    "up" | "k" => -1,
    _ => 1,
  }
}

fn open(stores: &Stores, _: &str) {
  if let Some(id) = nsh_core::prelude::untracked(|| stores.catalog.cursor()) {
    actions::open_product(stores, &id);
  }
}

fn origin(stores: &Stores, _: &str) {
  if nsh_core::prelude::untracked(|| stores.bp()) == Bp::S {
    stores.catalog.open_filter();
    stores.ui.open(Overlay::FilterDialog);
  } else {
    stores.catalog.cycle_origin();
  }
}

fn jump(stores: &Stores, stroke: &str) {
  let group = match stroke {
    "f" => FilterGroup::Origin,
    "r" => FilterGroup::Roast,
    _ => FilterGroup::Sort,
  };
  stores.catalog.filter_jump(group);
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new(
    "catalog.retry",
    Key::new("⏎", &["enter"]),
    "⏎ 重试",
    PAGE,
    |stores, _| stores.catalog.load(),
  )
  .when(failed)
  .footer(5),
  Command::new(
    "catalog.move",
    Key::new("↑↓", &["down", "j", "up", "k"]),
    "↑↓ 移动",
    PAGE,
    |stores, stroke| stores.catalog.move_cursor(delta(stroke)),
  )
  .when(movable)
  .footer(10),
  Command::new(
    "catalog.open",
    Key::new("⏎", &["enter", "l", "right"]),
    "⏎ 详情",
    PAGE,
    open,
  )
  .when(ready)
  .footer(20),
  Command::new(
    "catalog.add",
    Key::new("a", &["a"]),
    "a 加入",
    PAGE,
    |stores, _| actions::quick_add(stores),
  )
  .when(ready)
  .footer(30),
  Command::new(
    "catalog.origin",
    Key::new("f", &["f"]),
    "f 产区",
    PAGE,
    origin,
  )
  .short("f 筛选")
  .footer(40),
  Command::new(
    "catalog.roast",
    Key::new("r", &["r"]),
    "r 烘焙",
    PAGE,
    |stores, _| stores.catalog.cycle_roast(),
  )
  .footer_full(50),
  Command::new(
    "catalog.sort",
    Key::new("s", &["s"]),
    "s 排序",
    PAGE,
    |stores, _| stores.catalog.cycle_sort(),
  )
  .footer_full(60),
  search_hint("catalog.search", Page::Catalog).footer_full(70),
  Command::new(
    "catalog.clear",
    Key::new("Esc", &["escape"]),
    "Esc 清除筛选",
    PAGE,
    |stores, _| stores.catalog.clear_filters(),
  )
  .when(filtered),
  Command::new(
    "catalog.leave_search",
    Key::new("Esc", &["escape", "enter"]),
    "Esc 离开搜索",
    PAGE,
    |stores, _| stores.ui.leave_field(),
  )
  .typing(Typing::Only)
  .footer(10),
  // The filter dialog (S).
  Command::new(
    "filter.move",
    Key::new("↑↓", &["down", "j", "up", "k"]),
    "↑↓ 选择",
    DIALOG,
    |stores, stroke| stores.catalog.filter_move(delta(stroke)),
  )
  .footer(10),
  Command::new(
    "filter.group",
    Key::new("Tab", &["tab"]),
    "Tab 下一组",
    DIALOG,
    |stores, _| stores.catalog.filter_next_group(),
  )
  .footer(20),
  Command::new(
    "filter.choose",
    Key::new("⏎", &["enter", "space"]),
    "⏎ 选中",
    DIALOG,
    |stores, _| stores.catalog.filter_choose(),
  )
  .footer(30),
  Command::new(
    "filter.jump",
    Key::new("f", &["f", "r", "s"]),
    "f 产区组",
    DIALOG,
    jump,
  ),
  Command::new(
    "filter.close",
    Key::new("Esc", &["escape"]),
    "Esc 关闭",
    DIALOG,
    |stores, _| stores.ui.close_if(Overlay::FilterDialog),
  )
  .footer(40),
];
