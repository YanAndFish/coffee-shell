//! 我的 PROFILE (06-profile.md): account, 记住我, settings, taste profile, recent orders,
//! logout. Guests see a card asking them to log in (their `m` / `o` still toggle the
//! settings, for this session only).
//!
//! Layouts (responsive.md "06 我的", cells in [`geometry`]): XL left 47 + right 1fr; L and M
//! left 40 + right 1fr, the order list scrolling inside its panel; S one scrolling column
//! (account, settings, taste, orders, logout). Below 32 rows the radar becomes flavour
//! meters and the avatar a `▐林▌` pill.
//!
//! Stores: [`crate::stores::ProfileStore`] (`state`, `profile`, the Tab focus ring),
//! [`crate::stores::OrdersStore`] (`phase`, `failure`, `items`), [`crate::stores::SettingsStore`]
//! (`animations`, `mouse_tracking`), [`crate::stores::SessionStore`] (`is_member`). Actions:
//! `actions::toggle_setting`, `actions::ask_logout`, `actions::go_login`,
//! `actions::open_product` (a recommendation), all through the page's commands.

mod account;
pub mod commands;
pub mod geometry;
mod orders;
mod settings;
mod taste;

use nsh_core::{element::Screen, prelude::*};

use self::geometry::Geometry;
use super::PageModule;
use crate::{
  commands::{Mode, press},
  router::{Page, Route},
  stores::Stores,
  ui::{
    self, Button, ButtonKind, Layout, Legend, Tone, card, label,
    layout::{self, hspace, vspace},
    strong,
  },
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

/// A panel titled `legend` as tall as its content, the content `geo.pad` cells in from each
/// side line (`ui::panel` keeps one; the rest are blank columns here).
fn section(legend: &'static str, geo: Geometry, content: impl IntoElement) -> View {
  let extra = geo.pad.saturating_sub(1);
  view().flex_none().child(ui::panel(
    legend,
    view()
      .flex_row()
      .child(hspace(extra))
      .child(view().flex_1().min_w(px(0.0)).child(content))
      .child(hspace(extra)),
  ))
}

/// The guest card: `你正在以访客身份浏览`, why to log in, `▐ ⏎ 去登录 ▌`; centred.
fn guest(stores: Stores, geo: Geometry) -> View {
  let width = 57.min(geo.right);
  view().flex_1().items_center().justify_center().child(card(
    Legend::new("我的 PROFILE"),
    width,
    Some(11),
    view()
      .child(strong("你正在以访客身份浏览", Tone::Fg))
      .child(vspace(1))
      .child(label(
        "登录后可以查看订单、管理地址本，并生成你的口味画像。",
        Tone::Soft,
      ))
      .child(vspace(1))
      .child(
        view().flex_row().child(
          Button::new(ButtonKind::Primary, "⏎", "去登录")
            .on_press(move || press(&stores, "profile.login")),
        ),
      ),
  ))
}

/// Two columns: account, settings and logout on the left; taste and the orders (filling
/// the rest, scrolling inside) on the right.
fn two_columns(stores: Stores, geo: Geometry) -> View {
  let left = view()
    .flex_none()
    .w(layout::cols(geo.left))
    .child(account::account_panel(stores, geo))
    .child(vspace(1))
    .child(settings::settings_panel(stores, geo))
    .child(vspace(1))
    .child(view().flex_row().child(account::logout_button(stores, geo)));
  let right = view()
    .flex_1()
    .min_w(px(0.0))
    .child(taste::taste_panel(stores, geo))
    .child(orders::orders_panel(stores, geo));
  view()
    .flex_1()
    .min_h(px(0.0))
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(hspace(geo.margin))
        .child(left)
        .child(hspace(geo.gap))
        .child(right)
        .child(hspace(geo.margin)),
    )
    .child(vspace(geo.bottom))
}

/// One scrolling column: account, settings, taste, orders, logout (nsh keeps the scroll
/// view's last column for its bar).
fn one_column(stores: Stores, geo: Geometry) -> View {
  view().flex_1().min_h(px(0.0)).child(
    scroll().flex_1().child(
      view()
        .flex_none()
        .w(layout::cols(geo.right))
        .child(account::account_panel(stores, geo))
        .child(settings::settings_panel(stores, geo))
        .child(taste::taste_panel(stores, geo))
        .child(orders::orders_panel(stores, geo))
        .child(view().flex_row().child(account::logout_button(stores, geo))),
    ),
  )
}

/// The profile page.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let session = stores.session;
  let profile = stores.profile;
  let orders = stores.orders;
  let layout = Layout::get();
  // The page's work: a member's profile and orders, once (again after a checkout).
  effect(move || {
    if session.is_member() {
      untracked(|| {
        profile.ensure_loaded();
        orders.ensure_loaded();
      });
    }
  });
  let geometry = computed(move || Geometry::at(layout.viewport()));
  ui::page(
    "我的 PROFILE",
    view().flex_1().min_h(px(0.0)).child(dynamic(move || {
      let geo = geometry.get();
      if !session.is_member() {
        guest(stores, geo)
      } else if geo.one_column {
        one_column(stores, geo)
      } else {
        two_columns(stores, geo)
      }
    })),
  )
}
