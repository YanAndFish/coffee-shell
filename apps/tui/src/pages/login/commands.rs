//! The login page's keys (keymap.md, "登录 / 注册"; footers "登录（输入框里）", "登录（无焦点）",
//! "注册"). No `1`–`4` `/` `q` here: the global keys are the app pages' only.

use crate::{
  commands::{Command, Key, Scope, Typing, nothing},
  router::Page,
  stores::{LoginMode, Stores, actions},
};

const PAGE: Scope = Scope::Page(Page::Login);

fn login_mode(stores: &Stores) -> bool {
  stores.session.mode() == LoginMode::Login
}

fn register_mode(stores: &Stores) -> bool {
  stores.session.mode() == LoginMode::Register
}

fn submit(stores: &Stores, _: &str) {
  actions::submit_auth(stores);
}

fn toggle_mode(stores: &Stores, _: &str) {
  let next = match nsh_core::prelude::untracked(|| stores.session.mode()) {
    LoginMode::Login => LoginMode::Register,
    LoginMode::Register => LoginMode::Login,
  };
  stores.session.set_mode(next);
}

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new("login.tab", Key::hint("Tab"), "Tab 切换", PAGE, nothing)
    .typing(Typing::Also)
    .footer(10),
  Command::new(
    "login.submit",
    Key::new("⏎", &["enter"]),
    "⏎ 登录",
    PAGE,
    submit,
  )
  .typing(Typing::Also)
  .when(login_mode)
  .footer(20),
  Command::new(
    "login.register_submit",
    Key::new("⏎", &["enter"]),
    "⏎ 注册",
    PAGE,
    submit,
  )
  .typing(Typing::Also)
  .when(register_mode)
  .footer(20),
  Command::new(
    "login.leave",
    Key::new("Esc", &["escape"]),
    "Esc 离开输入",
    PAGE,
    |stores, _| stores.ui.leave_field(),
  )
  .typing(Typing::Only)
  .footer(30),
  Command::new(
    "login.back",
    Key::new("Esc", &["escape"]),
    "Esc 返回登录",
    PAGE,
    |stores, _| stores.session.set_mode(LoginMode::Login),
  )
  .when(register_mode)
  .footer_full(40)
  .footer_s(30),
  Command::new(
    "login.guest",
    Key::new("g", &["g"]),
    "g 访客浏览",
    PAGE,
    |stores, _| actions::continue_as_guest(stores),
  )
  .short("g 访客")
  .footer_full(30)
  .footer_s(40),
  Command::new(
    "login.mode",
    Key::hint("→"),
    "→ 注册新账号",
    PAGE,
    toggle_mode,
  ),
  Command::new(
    "login.remember",
    Key::hint("space"),
    "space 记住我",
    PAGE,
    |stores, _| stores.session.toggle_remember(),
  ),
];
