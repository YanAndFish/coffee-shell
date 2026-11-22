//! The header row: the wordmark, the tabs, and who is shopping. When the session serves
//! the contract fixtures instead of a backend, it says so (`◇ 样例数据`).

use nsh_core::prelude::*;

use crate::{
  router::TABS,
  stores::Stores,
  ui::{Bp, Layout, Tone, label, nav_tabs, wordmark},
};

/// The header of `stores`' session.
pub fn header(stores: Stores, layout: Layout) -> impl IntoElement {
  let router = stores.router;
  let session = stores.session;
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .gap_4()
    .px_2()
    .child(wordmark(move || router.go_tab(0)))
    .child(nav_tabs(
      &TABS,
      move || router.tab(),
      move |index| router.go_tab(index),
    ))
    .child(view().flex_1())
    .child(when(
      move || session.offline(),
      || label("◇ 样例数据", Tone::Mute),
    ))
    .child(label(
      move || {
        session.user().unwrap_or_else(|| {
          // At 80 columns the English half does not fit beside the tabs.
          SharedString::from(if layout.bp() <= Bp::S {
            "访客"
          } else {
            "访客 GUEST"
          })
        })
      },
      Tone::Soft,
    ))
}
