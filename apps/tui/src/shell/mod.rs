//! The app shell: the window frame (header, the page stack or an open dialog, the toast
//! line, the status bar), the key bindings of the command table, the session's start, and
//! the too-small guard. No page logic.
//!
//! Outlets (data wiring only; the overlays' look is drawn by the next wave):
//!
//! - the **dialog outlet** ([`outlets::dialog`]): while an overlay is open
//!   (`UiStore::overlay`), it shows in place of the page stack — the help and the logout
//!   confirmation from [`overlays`], the address and filter dialogs from their pages
//!   (`pages::dialog`). The pages stay mounted underneath and keep their state.
//! - the **toast outlet** ([`outlets::toast`]): `ToastStore::current`, dismissed after
//!   `ToastStore::duration` (shorter while another waits).
//!
//! Below 80×24 only the guard shows, but the frame stays mounted (hidden), so the pages
//! keep their state and enlarging the terminal brings them back as they were. Keys do
//! nothing meanwhile (nsh's own Ctrl+C still quits).

mod footer;
mod header;
pub mod outlets;
pub mod overlays;
mod too_small;

use nsh_core::{keymap::When, prelude::*};

use crate::{
  commands, pages,
  router::Route,
  stores::{Stores, actions},
  ui::{self, Layout, layout},
};

/// The demo notice the status bar always shows.
pub const DEMO_NOTICE: &str = "演示环境 · 不是真实商店";

/// Binds every keystroke of the command table once: a key runs the first command active
/// in the context now and enabled; it does nothing while the terminal is too small.
fn bind_keys(stores: Stores, layout: Layout) {
  for binding in commands::bindings() {
    // A held key repeats (holding `j` walks the list). Terminals without the kitty
    // protocol report no releases, so without `repeat` a second press of the same key
    // would read as a repeat of the first. A stroke some command takes while typing (`⏎`,
    // `Esc`) fires in text fields too; the others are the field's.
    let options = KeyOptions {
      repeat: true,
      when: if binding.in_text {
        When::Always
      } else {
        When::Auto
      },
      pass_through: binding.pass_through,
      ..KeyOptions::default()
    };
    on_key_with(binding.stroke, options, move |stroke| {
      if layout.too_small() {
        return;
      }
      let command = untracked(|| {
        let ctx = commands::context(&stores);
        commands::resolve(&ctx, stroke, |command| (command.enabled)(&stores))
      });
      if let Some(command) = command {
        // One update for all a command changes (the route, a toast, an overlay).
        batch(|| (command.run)(&stores, stroke));
      }
    });
  }
}

/// Dismisses each toast after its duration (the next one then shows, with its own clock).
/// A toast that others queue behind gets a new, shorter clock: the effect runs again and
/// its earlier timer goes with the earlier run.
fn expire_toasts(stores: Stores) {
  effect(move || {
    if let Some(toast) = stores.toasts.current() {
      let duration = stores.toasts.duration();
      set_timeout(duration, move || stores.toasts.dismiss(toast.seq));
    }
  });
}

/// The shell. Call it once, in the root, after the stores and the layout are provided.
pub fn shell() -> impl IntoElement {
  let stores = Stores::get();
  let layout = Layout::get();
  bind_keys(stores, layout);
  expire_toasts(stores);
  // The connection's start: resume a remembered key's session (once).
  effect(move || untracked(|| actions::start(&stores)));

  let pages = ui::page_stack().child(each(
    move || stores.router.routes(),
    |route: &Route| route.key(),
    move |route, depth| {
      // The user went back natively: the route (with those above it) goes.
      pages::page(&route.get_untracked())
        .on_dismiss(move || stores.router.truncate(depth.get_untracked()))
    },
  ));

  let content = view()
    .flex_1()
    .w_full()
    .max_w(layout::cols(layout::MAX_CONTENT_COLS))
    .px(layout::cols(1))
    .child(
      view()
        .flex_1()
        .style_when(move || stores.ui.overlay().is_some(), |s| s.hidden())
        .child(pages),
    )
    .child(outlets::dialog(stores));

  let frame = view()
    .flex_1()
    .style_when(move || layout.too_small(), |s| s.hidden())
    .child(header::header(stores, layout))
    .child(view().flex_1().min_h(px(0.0)).items_center().child(content))
    .child(outlets::toast(stores))
    .child(footer::footer(stores, layout));

  ui::surface_root().child(frame).child(when(
    move || layout.too_small(),
    move || too_small::too_small(layout),
  ))
}
