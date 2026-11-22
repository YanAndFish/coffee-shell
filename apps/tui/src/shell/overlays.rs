//! The shell's own overlays, as placeholders: the `?` help (`快捷键 KEYMAP`, from the
//! command table) and the logout confirmation (`退出登录 LOGOUT`, U4 / P-ui-1). Their state
//! is `UiStore` (`overlay`, `logout_choice`) and `SessionStore` (`key_remembered`,
//! `logging_out`); their keys are `commands::OVERLAYS`. The next wave draws them as the
//! design's dialogs.

use nsh_core::prelude::*;

use crate::{
  commands::{self, KeyContext, Scope},
  pages::placeholder::{self, Line},
  stores::{LogoutChoice, Stores},
  ui::Tone,
};

/// The help entries for the page on screen: every global key and the page's own, as
/// `key — what` (the command table is the source).
pub fn help_lines(stores: &Stores) -> Vec<Line> {
  let page = stores.page();
  let ctx = KeyContext::on(page);
  let mut lines = vec![Line::new("── 全局", Tone::Mute)];
  let mut page_lines = vec![Line::new("── 本页", Tone::Mute)];
  for command in commands::all() {
    let target = match command.scope {
      Scope::Pages | Scope::App if command.active(&ctx) => &mut lines,
      Scope::Page(scope) if scope == page => &mut page_lines,
      _ => continue,
    };
    target.push(Line::soft(format!(
      "{:<6} {}",
      command.key.glyph,
      command.text()
    )));
  }
  lines.extend(page_lines);
  lines.push(Line::soft(
    "鼠标：悬停高亮 · 单击选中 · 双击打开 · 底栏的快捷键提示可以直接点击",
  ));
  lines.push(Line::text("Esc 关闭").press("help.close"));
  lines
}

/// The help (placeholder).
pub fn help(stores: Stores) -> AnyElement {
  placeholder::panel(|| "快捷键 KEYMAP".to_owned(), move || help_lines(&stores)).into_element()
}

/// The logout confirmation's lines.
pub fn logout_lines(stores: &Stores) -> Vec<Line> {
  let note = if stores.session.key_remembered() {
    "同时解除当前公钥的免密登录。"
  } else {
    "购物车、地址本与订单都会保留在账号里。"
  };
  let mut lines = vec![Line::text("确定退出登录吗？"), Line::soft(note)];
  if stores.session.logging_out() {
    lines.push(Line::new("⠋ 正在退出…", Tone::Acc));
    return lines;
  }
  lines.push(Line::soft("再按 q 也可以确认"));
  let (cancel, confirm) = match stores.ui.logout_choice() {
    LogoutChoice::Confirm => ("  [ Esc 取消 ]", "▶ [ ⏎ 退出登录 ]"),
    LogoutChoice::Cancel => ("▶ [ Esc 取消 ]", "  [ ⏎ 退出登录 ]"),
  };
  lines.push(Line::text(cancel).press("logout.cancel"));
  lines.push(Line::text(confirm).press("logout.confirm"));
  lines
}

/// The logout confirmation (placeholder).
pub fn logout(stores: Stores) -> AnyElement {
  placeholder::panel(
    || "退出登录 LOGOUT".to_owned(),
    move || logout_lines(&stores),
  )
  .into_element()
}
