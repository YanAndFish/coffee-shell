//! The login page's keys (keymap.md, "登录 / 注册"; footers "登录（输入框里）", "登录（无焦点）",
//! "注册"). No `1`–`4` `/` `q` here: the global keys are the app pages' only.
//!
//! The card's focus ring is `SessionStore::form_focus`: Tab and Shift+Tab move it (nsh's own
//! traversal, which the page reports), `⏎` and `space` press the control it is on (a button
//! presses, `记住我` toggles); in a field, or with no focus, `⏎` submits.

use nsh_core::prelude::untracked;

use crate::{
  commands::{Command, Key, Scope, Typing, nothing},
  router::Page,
  stores::{LoginMode, Stores, actions, session::FormControl},
};

const PAGE: Scope = Scope::Page(Page::Login);

fn login_mode(stores: &Stores) -> bool {
  stores.session.mode() == LoginMode::Login
}

fn register_mode(stores: &Stores) -> bool {
  stores.session.mode() == LoginMode::Register
}

/// The focus is on a control that is not a text field (tracked).
fn on_control(stores: &Stores) -> bool {
  stores
    .session
    .form_focus()
    .is_some_and(|control| !control.is_field())
}

/// Presses `control` as a click on it does.
pub fn press_control(stores: &Stores, control: FormControl) {
  match control {
    FormControl::Remember => stores.session.toggle_remember(),
    FormControl::Guest => actions::continue_as_guest(stores),
    FormControl::SwitchMode => toggle_mode(stores, ""),
    FormControl::Submit | FormControl::Username | FormControl::Password => {
      actions::submit_auth(stores)
    }
  }
}

/// `⏎`: the control with the focus, else (a field, no focus) the submit.
fn enter(stores: &Stores, _: &str) {
  match stores.session.form_focus_untracked() {
    Some(control) => press_control(stores, control),
    None => actions::submit_auth(stores),
  }
}

/// `space` on a control (in a field it types a space).
fn space(stores: &Stores, _: &str) {
  if let Some(control) = stores.session.form_focus_untracked()
    && !control.is_field()
  {
    press_control(stores, control);
  }
}

fn toggle_mode(stores: &Stores, _: &str) {
  let next = match untracked(|| stores.session.mode()) {
    LoginMode::Login => LoginMode::Register,
    LoginMode::Register => LoginMode::Login,
  };
  stores.session.set_mode(next);
}

/// `Esc` in a field: no focus (the field gives nsh's text focus up).
fn leave(stores: &Stores, _: &str) {
  stores.session.set_form_focus(None);
  stores.ui.leave_field();
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
    enter,
  )
  .typing(Typing::Also)
  .when(login_mode)
  .footer(20),
  Command::new(
    "login.register_submit",
    Key::new("⏎", &["enter"]),
    "⏎ 注册",
    PAGE,
    enter,
  )
  .typing(Typing::Also)
  .when(register_mode)
  .footer(20),
  Command::new(
    "login.leave",
    Key::new("Esc", &["escape"]),
    "Esc 离开输入",
    PAGE,
    leave,
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
    Key::new("space", &["space"]),
    "space 记住我",
    PAGE,
    space,
  )
  .when(on_control),
];
