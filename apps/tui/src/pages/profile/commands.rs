//! The profile's keys (keymap.md, "我的"; footers "我的（会员）" and "我的（访客）"). `q` is the
//! global logout key; its hint here runs the same action.

use crate::{
  commands::{Command, Key, Scope, search_hint, tabs_hint},
  router::Page,
  stores::{Phase, Setting, Stores, actions},
};

const PAGE: Scope = Scope::Page(Page::Profile);

fn member(stores: &Stores) -> bool {
  stores.session.is_member()
}

fn guest(stores: &Stores) -> bool {
  !stores.session.is_member()
}

fn failed(stores: &Stores) -> bool {
  member(stores)
    && (stores.profile.phase() == Phase::Failed || stores.orders.phase() == Phase::Failed)
}

fn retry(stores: &Stores, _: &str) {
  stores.profile.load();
  stores.orders.load();
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new(
    "profile.login",
    Key::new("⏎", &["enter"]),
    "⏎ 去登录",
    PAGE,
    |stores, _| actions::go_login(stores),
  )
  .when(guest)
  .footer(5),
  Command::new(
    "profile.retry",
    Key::new("⏎", &["enter"]),
    "⏎ 重试",
    PAGE,
    retry,
  )
  .when(failed),
  Command::new(
    "profile.animations",
    Key::new("m", &["m"]),
    "m 动画开关",
    PAGE,
    |stores, _| actions::toggle_setting(stores, Setting::Animations),
  )
  .short("m 动画")
  .footer(10),
  Command::new(
    "profile.mouse",
    Key::new("o", &["o"]),
    "o 鼠标追踪",
    PAGE,
    |stores, _| actions::toggle_setting(stores, Setting::MouseTracking),
  )
  .short("o 鼠标")
  .footer(20),
  Command::new(
    "profile.logout",
    Key::hint("q"),
    "q 退出登录",
    PAGE,
    |stores, _| actions::ask_logout(stores),
  )
  .short("q 退出")
  .when(member)
  .footer(30),
  tabs_hint("profile.tabs", Page::Profile).footer(40),
  search_hint("profile.search", Page::Profile).footer_full(50),
];
