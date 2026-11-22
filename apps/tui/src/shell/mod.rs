//! The app shell: the window frame (header, the page stack, the toast line, the status bar),
//! the key bindings of the command table, and the too-small guard. No page logic.
//!
//! Below 80×24 only the guard shows, but the frame stays mounted (hidden), so the pages
//! keep their state and enlarging the terminal brings them back as they were. Keys do
//! nothing meanwhile (nsh's own Ctrl+C still quits).

mod footer;
mod header;
mod too_small;

use nsh_core::prelude::*;

use crate::{
  commands, pages,
  router::Route,
  stores::{Stores, toast::TOAST_DURATION},
  ui::{self, Layout, layout},
};

/// The demo notice the status bar always shows.
pub const DEMO_NOTICE: &str = "演示环境 · 不是真实商店";

/// Binds every keystroke of the command table: a key runs the first command in scope on
/// the page on top and enabled; it does nothing while the terminal is too small.
fn bind_keys(stores: Stores, layout: Layout) {
  // A held key repeats (holding `j` walks the list). Terminals without the kitty protocol
  // report no releases, so without `repeat` a second press of the same key would read as a
  // repeat of the first.
  let options = KeyOptions {
    repeat: true,
    ..KeyOptions::default()
  };
  for stroke in commands::strokes() {
    on_key_with(stroke, options, move |stroke| {
      if layout.too_small() {
        return;
      }
      let page = stores.router.top_untracked().page();
      if let Some(command) = commands::resolve(page, stroke, |command| (command.enabled)(&stores)) {
        (command.run)(&stores, stroke);
      }
    });
  }
}

/// Dismisses each toast after [`TOAST_DURATION`] (a newer toast restarts the clock).
fn expire_toasts(stores: Stores) {
  effect(move || {
    if let Some(toast) = stores.toasts.current() {
      set_timeout(TOAST_DURATION, move || stores.toasts.dismiss(toast.seq));
    }
  });
}

/// The shell. Call it once, in the root, after the stores and the layout are provided.
pub fn shell() -> impl IntoElement {
  let stores = Stores::get();
  let layout = Layout::get();
  bind_keys(stores, layout);
  expire_toasts(stores);

  let pages = ui::page_stack().child(each(
    move || stores.router.routes(),
    |route: &Route| route.key(),
    move |route, depth| {
      // The user went back natively: the route (with those above it) goes.
      pages::page(&route.get_untracked())
        .on_dismiss(move || stores.router.truncate(depth.get_untracked()))
    },
  ));

  let frame = view()
    .flex_1()
    .style_when(move || layout.too_small(), |s| s.hidden())
    .child(header::header(stores, layout))
    .child(
      view().flex_1().min_h(px(0.0)).items_center().child(
        view()
          .flex_1()
          .w_full()
          .max_w(layout::cols(layout::MAX_CONTENT_COLS))
          .px(layout::cols(1))
          .child(pages),
      ),
    )
    .child(dynamic(move || {
      stores
        .toasts
        .current()
        .map(|toast| ui::toast_line(toast.kind, toast.title, toast.sub))
    }))
    .child(footer::footer(stores, layout));

  ui::surface_root().child(frame).child(when(
    move || layout.too_small(),
    move || too_small::too_small(layout),
  ))
}
