//! The header row: the wordmark, the tabs, and the session's facts — `◇ 样例数据` when the
//! fixtures serve instead of a backend, the SSH key indicator (`● 公钥已识别` /
//! `○ 未提供公钥`), the cart badge (units in the cart, as the server counted them), and who
//! is shopping (`林晓 Lv.3` / `访客 GUEST`). Data wiring only: the system row and the
//! account chip of the design come with the next wave.

use nsh_core::prelude::*;

use crate::{
  router::TABS,
  stores::{Stores, actions},
  ui::{Bp, Layout, Tone, label, nav_tabs, wordmark},
};

/// The key indicator: `● 公钥已识别` / `○ 未提供公钥` (the glyph alone at S).
pub fn key_indicator(has_key: bool, bp: Bp) -> &'static str {
  match (has_key, bp <= Bp::S) {
    (true, false) => "● 公钥已识别",
    (false, false) => "○ 未提供公钥",
    (true, true) => "●",
    (false, true) => "○",
  }
}

/// The cart badge: `购物车 3` (`[3]` at S); nothing for an empty cart.
pub fn cart_badge(count: u32, bp: Bp) -> Option<String> {
  match (count, bp <= Bp::S) {
    (0, _) => None,
    (n, false) => Some(format!("购物车 {n}")),
    (n, true) => Some(format!("[{n}]")),
  }
}

/// Who is shopping: `林晓 Lv.3` / `访客 GUEST` (`林晓` / `访客` at S).
pub fn account(user: Option<(&str, u32)>, bp: Bp) -> String {
  match (user, bp <= Bp::S) {
    (Some((name, _)), true) => name.to_owned(),
    (Some((name, level)), false) => format!("{name} Lv.{level}"),
    (None, true) => "访客".to_owned(),
    (None, false) => "访客 GUEST".to_owned(),
  }
}

/// The header of `stores`' session.
pub fn header(stores: Stores, layout: Layout) -> impl IntoElement {
  let session = stores.session;
  let cart = stores.cart;
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .gap_4()
    .px_2()
    .child(wordmark(move || actions::go_tab(&stores, 0)))
    .child(nav_tabs(
      &TABS,
      move || stores.router.tab(),
      move |index| actions::go_tab(&stores, index),
    ))
    .child(view().flex_1())
    .child(when(
      move || session.offline() && layout.bp() > Bp::S,
      || label("◇ 样例数据", Tone::Mute),
    ))
    .child(label(
      move || SharedString::from(key_indicator(session.has_key(), layout.bp())),
      Tone::Mute,
    ))
    .child(dynamic(move || {
      cart_badge(cart.item_count(), layout.bp()).map(|badge| label(badge, Tone::Acc))
    }))
    .child(label(
      move || {
        let member = session.member();
        let user = member
          .as_ref()
          .map(|m| (m.user.username.as_str(), m.user.tier.level));
        SharedString::from(account(user, layout.bp()))
      },
      Tone::Soft,
    ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_session_facts_shrink_at_s() {
    assert_eq!(key_indicator(true, Bp::Xl), "● 公钥已识别");
    assert_eq!(key_indicator(false, Bp::M), "○ 未提供公钥");
    assert_eq!(key_indicator(false, Bp::S), "○");
    assert_eq!(cart_badge(0, Bp::Xl), None);
    assert_eq!(cart_badge(3, Bp::Xl).as_deref(), Some("购物车 3"));
    assert_eq!(cart_badge(3, Bp::S).as_deref(), Some("[3]"));
    assert_eq!(account(Some(("林晓", 3)), Bp::L), "林晓 Lv.3");
    assert_eq!(account(None, Bp::S), "访客");
    assert_eq!(account(None, Bp::M), "访客 GUEST");
  }
}
