//! The profile's keys (keymap.md, "我的"; footers "我的（会员）" and "我的（访客）"). `q` is the
//! global logout key; its hint here runs the same action. Tab walks the page's focus ring
//! (`动画开关 → 鼠标开关 → 退出登录 → 推荐按钮 → 订单列表`) and `⏎` does what the focused
//! control does; PageUp / PageDown scroll the order list natively.

use crate::{
  commands::{Command, Key, Scope, search_hint, tabs_hint},
  router::Page,
  stores::{Phase, Setting, Stores, actions, profile::ProfileFocus},
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

fn focused(stores: &Stores) -> bool {
  member(stores) && stores.profile.focus().is_some()
}

fn retry(stores: &Stores, _: &str) {
  stores.profile.load();
  stores.orders.load();
}

/// `⏎` on the focused control: what a click on it does.
fn activate(stores: &Stores, _: &str) {
  match stores.profile.focus_untracked() {
    Some(ProfileFocus::Animations) => actions::toggle_setting(stores, Setting::Animations),
    Some(ProfileFocus::MouseTracking) => actions::toggle_setting(stores, Setting::MouseTracking),
    Some(ProfileFocus::Logout) => actions::ask_logout(stores),
    Some(ProfileFocus::Recommendation(index)) => open_recommendation(stores, index),
    Some(ProfileFocus::Orders) | None => {}
  }
}

/// Opens recommendation `index` of the profile (`按画像推荐`).
pub fn open_recommendation(stores: &Stores, index: usize) {
  let id = nsh_core::prelude::untracked(|| {
    stores
      .profile
      .profile()
      .and_then(|profile| profile.recommendations.get(index).map(|r| r.id.clone()))
  });
  if let Some(id) = id {
    actions::open_product(stores, &id);
  }
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
    "profile.activate",
    Key::new("⏎", &["enter"]),
    "⏎ 确定",
    PAGE,
    activate,
  )
  .when(focused),
  Command::new(
    "profile.focus",
    Key::new("Tab", &["tab", "shift+tab"]),
    "Tab 切换焦点",
    PAGE,
    |stores, stroke| stores.profile.move_focus(stroke != "shift+tab"),
  )
  .when(member),
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

#[cfg(test)]
mod tests {
  use coffee_api::AuthSession;

  use super::*;
  use crate::{
    api::FixtureSet,
    commands::{KeyContext, resolve},
    stores::testing::with_stores,
  };

  fn sign_in(stores: &Stores) {
    let auth: AuthSession = serde_json::from_str(include_str!(
      "../../../../../packages/contract/fixtures/v1/auth/login.json"
    ))
    .unwrap();
    stores.session.signed_in(auth);
  }

  fn enter(stores: &Stores) -> Option<&'static str> {
    let ctx = KeyContext::on(Page::Profile);
    resolve(&ctx, "enter", |command| (command.enabled)(stores)).map(|c| c.id)
  }

  #[test]
  fn a_failed_load_offers_the_retry() {
    with_stores(FixtureSet::Down, None, (160, 50), |stores| {
      sign_in(&stores);
      stores.profile.load();
      stores.orders.load();
      assert_eq!(stores.profile.phase(), Phase::Failed);
      assert_eq!(stores.orders.phase(), Phase::Failed);
      assert_eq!(enter(&stores), Some("profile.retry"));
      // The retry asks again (and fails again on a backend that is down).
      retry(&stores, "enter");
      assert_eq!(stores.orders.phase(), Phase::Failed);
    });
  }

  #[test]
  fn enter_follows_the_state() {
    with_stores(
      FixtureSet::Contract,
      Some("SHA256:k"),
      (160, 50),
      |stores| {
        assert_eq!(
          enter(&stores),
          Some("profile.login"),
          "a guest goes to log in"
        );
        sign_in(&stores);
        stores.profile.load();
        stores.orders.load();
        assert_eq!(stores.profile.phase(), Phase::Ready);
        assert_eq!(enter(&stores), None, "nothing focused");
        // Tab, Tab: 鼠标追踪; ⏎ toggles it.
        stores.profile.move_focus(true);
        stores.profile.move_focus(true);
        assert_eq!(enter(&stores), Some("profile.activate"));
        activate(&stores, "enter");
        assert!(!stores.settings.mouse_tracking());
      },
    );
  }
}
