//! 登录 · 注册 (01-login.md): the first page of a new connection (unless its key is
//! remembered), and where a gated checkout or `⏎ 去登录` sends a guest.
//!
//! Store: [`crate::stores::SessionStore`] — the form (`draft`, `set_username`,
//! `set_password`, `toggle_remember`, `set_mode`), the submit's progress (`step`,
//! `submitting`), `resuming`, `has_key`. Actions: `actions::submit_auth`,
//! `actions::continue_as_guest`. Fields report their focus as [`USERNAME_FIELD`] /
//! [`PASSWORD_FIELD`].
//!
//! The body is a placeholder (plain lines and two bare fields); the login screen replaces it.

pub mod commands;

use nsh_core::{element::Screen, prelude::*};

use super::{
  PageModule,
  placeholder::{self, Line},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{LoginMode, Stores},
  ui::{self, Tone},
};

/// `登录 LOGIN`.
pub const MODE: Mode = Mode::new("登录", "LOGIN");
/// `注册 SIGN UP` (register mode).
pub const REGISTER_MODE: Mode = Mode::new("注册", "SIGN UP");

/// The username field's name in the focus tracking.
pub const USERNAME_FIELD: &str = "login.username";
/// The password field's name in the focus tracking.
pub const PASSWORD_FIELD: &str = "login.password";

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Login,
  mode,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

fn mode(stores: &Stores) -> Mode {
  match stores.session.mode() {
    LoginMode::Login => MODE,
    LoginMode::Register => REGISTER_MODE,
  }
}

fn lines(stores: Stores) -> Vec<Line> {
  let session = stores.session;
  let draft = session.draft();
  let register = draft.mode == LoginMode::Register;
  let mut lines = vec![Line::new("! 演示环境 · 请勿使用真实密码", Tone::Acc)];
  if session.resuming() {
    lines.push(Line::soft("⠋ 正在通过 SSH 公钥恢复登录…"));
  }
  lines.push(if session.has_key() {
    let mark = if draft.remember { "[✓]" } else { "[ ]" };
    Line::text(format!(
      "{mark} 记住我 30 天 · 绑定当前 SSH 公钥，下次连接免密登录"
    ))
    .press("login.remember")
  } else {
    Line::soft("[ ] 记住我 30 天 · 当前连接没有 SSH 公钥，无法记住登录")
  });
  if let Some(error) = draft.error {
    lines.push(Line::error(format!("✗ {error}")));
  }
  match session.step() {
    Some(step) => lines.push(Line::new(
      format!("⠋ {} · {}%", step.label(), step.percent()),
      Tone::Acc,
    )),
    None if register => lines.push(Line::text("⏎ 注册并登录").press("login.register_submit")),
    None => lines.push(Line::text("⏎ 登录").press("login.submit")),
  }
  lines.push(Line::text("g 访客浏览").press("login.guest"));
  lines.push(if register {
    Line::soft("← 已有账号，去登录").press("login.mode")
  } else {
    Line::soft("注册新账号 →").press("login.mode")
  });
  lines.push(Line::soft("Tab 切换输入 · ⏎ 提交 · ? 查看全部快捷键"));
  lines
}

/// The login page (placeholder body).
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let session = stores.session;
  let legend = move || {
    SharedString::from(match session.mode() {
      LoginMode::Login => "登录 · SIGN IN",
      LoginMode::Register => "注册 · SIGN UP",
    })
  };
  ui::page(
    "登录 LOGIN",
    ui::panel(
      legend,
      view()
        .flex_1()
        .child(placeholder::field(
          USERNAME_FIELD,
          "用户名",
          move || session.draft().username,
          move |text| session.set_username(text),
          false,
        ))
        .child(placeholder::field(
          PASSWORD_FIELD,
          "密码",
          move || session.draft().password,
          move |text| session.set_password(text),
          true,
        ))
        .child(placeholder::lines(move || lines(stores))),
    ),
  )
}
