//! The app shell: the window frame around the pages, the key bindings of the command table,
//! the session's start, and the too-small page. No page logic.
//!
//! ```text
//! row 0        system row: roast@tty: ~/shop — 160×50 · ! 演示环境 · 不是真实商店 · 21:04
//! rows 1–4     header (app pages; 2 rows at S and under 32 rows; the login page has none)
//! …            the page stack, inside the content margins (at most 200 columns, centred)
//! last row     status bar: ▐ 选购 BROWSE ▌ hints …  ● 鼠标   购物车 ¥426.00   ‹?› 帮助
//! ```
//!
//! - **System row** (`ui::system_row`): the demo notice on every screen and the clock
//!   ([`clock`]); no SSH key indicator (only the login card's 记住我 speaks of the key).
//! - **Header** ([`header`]): wordmark, tabs with the cart badge and the underline of the
//!   current tab, the search entry, who is shopping (not on the splash and the login page).
//! - **Content margins**: columns per `ui::content_margins` (XL 2, L/M 1, S 0) and the blank
//!   row under the content at XL; the row above it is `ui::page`'s own. The login page has
//!   none: its two columns are laid out on the whole width.
//! - **Status bar** ([`footer`]): `ui::mode_line` with the command table's hints.
//! - **Outlets** ([`outlets`]): the dialog of the overlay open (over the page, which stays
//!   drawn and mounted) and the toast (bottom right).
//! - **Too small** ([`too_small`]): below 80×24 only that page shows; the frame stays
//!   mounted (hidden), so the pages keep their state and enlarging the terminal brings them
//!   back as they were. Keys do nothing meanwhile (the host's Ctrl+C still quits).
//!
//! Keys: every keystroke of the command table is bound once ([`bind_keys`]); a key runs the
//! first command active in the context now and enabled (keymap.md's order: overlay → text
//! field → submitting → page). `Esc`, `⏎`, `Tab` and `Shift+Tab` are bound
//! `When::Always`, so they reach the table from a text field too (nsh leaves every
//! non-character key to a focused field otherwise); letters and digits are bound
//! `When::Auto` and type into a field. Clicks on a status-bar hint or a tab run the same
//! command.

pub mod clock;
pub mod footer;
pub mod header;
pub mod outlets;
pub mod overlays;
pub mod too_small;

use nsh_core::{keymap::When, prelude::*};

use crate::{
  api::{ApiFailure, ErrorCode},
  commands, pages,
  router::{Page, Route},
  stores::{Stores, actions},
  ui::{self, Layout, layout},
};

/// The demo notice the system row always shows.
pub const DEMO_NOTICE: &str = ui::DEMO_NOTICE;

/// Keystrokes bound `When::Always`: they reach the command table from a text field too.
pub const ALWAYS: &[&str] = &["escape", "enter", "tab", "shift+tab"];

/// The blank rows under the content at `bp` (`ui::bottom_margin`). The row above the
/// content is the page's own (`ui::page`).
pub use ui::bottom_margin;

/// Binds every keystroke of the command table once: a key runs the first command active
/// in the context now and enabled; it does nothing while the terminal is too small.
fn bind_keys(stores: Stores, layout: Layout) {
  for binding in commands::bindings() {
    // A held key repeats (holding `j` walks the list). Terminals without the kitty
    // protocol report no releases, so without `repeat` a second press of the same key
    // would read as a repeat of the first.
    let options = KeyOptions {
      repeat: true,
      when: if binding.in_text || ALWAYS.contains(&binding.stroke) {
        When::Always
      } else {
        When::Auto
      },
      pass_through: binding.pass_through,
      ..KeyOptions::default()
    };
    on_key_with(binding.stroke, options, move |stroke| {
      // Any key wakes what idles (the login page's steam), handled or not.
      stores.ui.touch();
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

/// The focus trap of the dialogs without fields (help, logout, the S filter): Tab goes on
/// to nsh (it moves the focus), which may land in a text field of the page under the
/// dialog; that field gives the keys straight back, so the dialog's keys keep working.
fn trap_focus(stores: Stores) {
  effect(move || {
    let open = stores.ui.overlay();
    if open.is_some_and(|overlay| !overlay.has_fields()) && stores.ui.typing() {
      untracked(|| stores.ui.leave_field());
    }
  });
}

/// The shell. Call it once, in the root, after the stores and the layout are provided.
pub fn shell() -> impl IntoElement {
  let stores = Stores::get();
  let layout = Layout::get();
  bind_keys(stores, layout);
  expire_toasts(stores);
  trap_focus(stores);
  // nsh's hover band shows only while 鼠标追踪 is on (components.md, "悬停").
  effect(move || ui::theme::set_hover_band(stores.settings.mouse_tracking()));
  // The connection's start: resume a remembered key's session (once). The splash, when it
  // is up, waits for it.
  effect(move || untracked(|| actions::start(&stores)));
  // A store saw the member session end: back to the login page (`登录已失效`).
  effect(move || {
    if stores.session.expired() {
      untracked(|| {
        let failure = ApiFailure::of(ErrorCode::SessionExpired);
        batch(|| actions::session_expired(&stores, &failure));
      });
    }
  });
  let time = clock::clock();
  // The splash and the login page are laid out on the whole width between the system row
  // and the status bar (01-login.md: brand column 95 + card column 65 at 160): no header,
  // no margins there.
  let bare = computed(move || matches!(stores.page(), Page::Splash | Page::Login));
  let margins = computed(move || {
    if bare.get() {
      return (0, 0);
    }
    let viewport = layout.viewport();
    let bp = viewport.bp();
    (
      ui::content_margins(bp).0,
      bottom_margin(bp, viewport.short()),
    )
  });

  let pages = ui::page_stack().child(each(
    move || stores.router.routes(),
    |route: &Route| route.key(),
    move |route, depth| {
      // The user went back natively: the route (with those above it) goes.
      pages::page(&route.get_untracked())
        .on_dismiss(move || stores.router.truncate(depth.get_untracked()))
    },
  ));
  let side = move || layout::hspace(margins.get().0);

  let content = view()
    .flex_1()
    .min_h(px(0.0))
    .w_full()
    .max_w(layout::cols(layout::MAX_CONTENT_COLS))
    .flex_row()
    .child(dynamic(side))
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .child(pages)
        .child(outlets::dialog(stores)),
    )
    .child(dynamic(side));

  let frame = view()
    .flex_1()
    .style_when(move || layout.too_small(), |s| s.hidden())
    .child(ui::system_row(move || time.get()))
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .items_center()
        .child(when(
          move || !bare.get(),
          move || {
            view()
              .flex_none()
              .w_full()
              .max_w(layout::cols(layout::MAX_CONTENT_COLS))
              .child(header::header(stores, layout))
          },
        ))
        .child(content)
        .child(dynamic(move || layout::vspace(margins.get().1))),
    )
    .child(footer::footer(stores, layout));

  ui::follow_pointer(ui::surface_root())
    .child(frame)
    .child(outlets::toast(stores, layout))
    .child(when(
      move || layout.too_small(),
      move || too_small::too_small(layout),
    ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::Bp;

  #[test]
  fn the_keys_of_fields_and_dialogs_are_bound_always() {
    let bindings = commands::bindings();
    for stroke in ["escape", "enter", "tab"] {
      assert!(
        bindings.iter().any(|b| b.stroke == stroke),
        "{stroke} is bound"
      );
    }
    assert_eq!(bottom_margin(Bp::Xl, false), 1);
    assert_eq!(bottom_margin(Bp::Xl, true), 0);
    assert_eq!(bottom_margin(Bp::M, false), 0);
  }
}
