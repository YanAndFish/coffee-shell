//! 我的 PROFILE (06-profile.md): account, 记住我, settings, taste profile, recent orders,
//! logout. Guests see a card asking them to log in (and can still toggle the settings,
//! for this session only).
//!
//! Stores: [`crate::stores::ProfileStore`] (`phase`, `failure`, `profile`, `ensure_loaded`),
//! [`crate::stores::OrdersStore`] (`phase`, `failure`, `items`, `total`, `ensure_loaded`),
//! [`crate::stores::SettingsStore`] (`animations`, `mouse_tracking`),
//! [`crate::stores::SessionStore`] (`member`, `remembered`, `has_key`). Actions:
//! `actions::toggle_setting`, `actions::ask_logout`, `actions::go_login`,
//! `actions::open_product` (a recommendation).
//!
//! The body is a placeholder (plain lines); the profile screen replaces it.

pub mod commands;

use coffee_api::{FLAVOR_AXES, RememberState};
use nsh_core::{element::Screen, prelude::*};

use super::{
  PageModule, format,
  placeholder::{self, Line, phase_lines},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::Stores,
  ui::Tone,
};

/// `我的 PROFILE`.
pub const MODE: Mode = Mode::new("我的", "PROFILE");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Profile,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

/// `■ 开` / `□ 关`.
pub fn switch(on: bool) -> &'static str {
  if on { "■ 开" } else { "□ 关" }
}

fn setting_lines(stores: Stores) -> Vec<Line> {
  let settings = stores.settings;
  vec![
    Line::text(format!(
      "界面动画 {} · m · 蒸汽、加载指示与进度条",
      switch(settings.animations())
    ))
    .press("profile.animations"),
    Line::text(format!(
      "鼠标追踪 {} · o · 悬停即移动选择高亮",
      switch(settings.mouse_tracking())
    ))
    .press("profile.mouse"),
  ]
}

fn lines(stores: Stores) -> Vec<Line> {
  let session = stores.session;
  if !session.is_member() {
    let mut lines = vec![
      Line::text("你正在以访客身份浏览"),
      Line::soft("登录后可以查看订单、管理地址本，并生成你的口味画像。"),
      Line::text("⏎ 去登录").press("profile.login"),
    ];
    lines.extend(setting_lines(stores));
    return lines;
  }
  let profile = stores.profile;
  let mut lines = phase_lines(
    profile.phase(),
    profile.failure(),
    "个人信息",
    "",
    Some("profile.retry"),
  );
  if let Some(data) = profile.profile() {
    let tier = &data.user.tier;
    lines.push(Line::text(format!(
      "{} · {} Lv.{} · {} / {} 豆",
      data.user.username,
      tier.name,
      tier.level,
      format::thousands(tier.points),
      format::thousands(tier.next_level_points)
    )));
    lines.push(Line::soft(match data.remembered.state {
      RememberState::Remembered => format!(
        "● 已记住当前公钥 · 至 {}",
        data
          .remembered
          .expires_on
          .as_deref()
          .map(format::month_day)
          .unwrap_or_default()
      ),
      RememberState::NotRemembered => "○ 未记住当前公钥".to_owned(),
      RememberState::NoKey => "○ 本次连接没有公钥".to_owned(),
    }));
    lines.push(Line::soft(format!(
      "订单 {} · 地址 {} · 优惠券 {}",
      data.stats.orders, data.stats.addresses, data.stats.coupons
    )));
    match &data.taste {
      Some(taste) => {
        let flavor = FLAVOR_AXES
          .iter()
          .zip(taste.flavor)
          .map(|(axis, value)| format!("{axis} {value}"))
          .collect::<Vec<_>>()
          .join(" · ");
        lines.push(Line::text(format!("口味画像：{flavor}")));
        lines.push(Line::soft(format!(
          "偏好 {} · 常买 {} · 少碰 {}",
          taste.prefer, taste.often, taste.avoid
        )));
      }
      None => lines.push(Line::soft("下单后生成你的口味画像")),
    }
    for product in &data.recommendations {
      lines.push(Line::soft(format!("按画像推荐：{} →", product.name)));
    }
  }
  lines.extend(setting_lines(stores));
  let orders = stores.orders;
  lines.push(Line::new("── 最近订单 ORDERS", Tone::Mute));
  lines.extend(phase_lines(
    orders.phase(),
    orders.failure(),
    "订单",
    "还没有订单 · 1 去挑豆子",
    Some("profile.retry"),
  ));
  for order in orders.items() {
    let items = order
      .lines
      .iter()
      .map(|l| format!("{} {}", l.name, format::order_line(l.weight, l.quantity)))
      .collect::<Vec<_>>()
      .join("、");
    lines.push(Line::text(format!(
      "{} · {} · {} · {} · {}",
      order.number,
      format::month_day(&order.order_date),
      items,
      format::yuan_cents(order.total),
      format::order_status(order.status)
    )));
  }
  lines.push(Line::text("q 退出登录").press("profile.logout"));
  lines
}

/// The profile page (placeholder body).
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let session = stores.session;
  let profile = stores.profile;
  let orders = stores.orders;
  // The page's work: a member's profile and orders, once (again after a checkout).
  effect(move || {
    if session.is_member() {
      untracked(|| {
        profile.ensure_loaded();
        orders.ensure_loaded();
      });
    }
  });
  placeholder::screen(
    "我的 PROFILE",
    move || {
      if session.is_member() {
        "个人信息 ACCOUNT".to_owned()
      } else {
        "我的 PROFILE".to_owned()
      }
    },
    move || lines(stores),
  )
}
