//! The shell's outlets: where toasts and dialogs show, over the frame.
//!
//! - **Toast** ([`toast`]): `ToastStore::current` as `ui::toast` draws it, 45×4 at the bottom
//!   right above the status bar; the shell dismisses it after `ToastStore::duration` (shorter
//!   while another waits), then the next one shows. Hidden (its clock still running) while
//!   the terminal is too small.
//! - **Dialog** ([`dialog`]): the overlay open (`UiStore::overlay`): the shell's help and
//!   logout confirmation ([`super::overlays`]), or a page's dialog (`pages::dialog`: the
//!   address dialog, the S filter dialog). The page stays drawn and mounted underneath; the
//!   status bar shows the dialog's mode and hints. Focus: opening an overlay takes the keys
//!   from a focused field and closing it gives them back (`UiStore::open` / `close`); while
//!   a dialog without fields of its own is open, no field under it may take the keys (the
//!   shell blurs one that Tab reached, see `shell::trap_focus`). A page dialog with fields
//!   keeps its own ring (`ui::DialogFocus`), its keys in the command table.

use nsh_core::prelude::*;

use super::overlays;
use crate::{
  pages,
  stores::{Overlay, Stores, ToastKind},
  ui::{self, Layout, layout},
};

/// The `ui` kind of a toast kind.
pub fn toast_kind(kind: ToastKind) -> ui::ToastKind {
  match kind {
    ToastKind::Ok => ui::ToastKind::Ok,
    ToastKind::Info => ui::ToastKind::Info,
    ToastKind::Warn => ui::ToastKind::Warn,
  }
}

/// The toast overlay: the toast showing, if any, placed as `ui::toast` places it (45×4, its
/// right edge two cells from the screen's, one under 100 columns, its bottom row above the
/// blank row over the status bar), on the window colour so it clears what is behind it.
/// Unlike `ui::toast` it takes no presses (`pointerEvents: none` on the overlay): a notice is
/// never in the way of the mouse, and the frame's line runs, clipped on screen, would
/// otherwise take the presses meant for the fields and hints under them.
pub fn toast(stores: Stores, layout: Layout) -> AnyElement {
  dynamic(move || {
    if layout.too_small() {
      return None;
    }
    stores.toasts.current().map(|toast| {
      let viewport = layout.viewport();
      let width = ui::toast::toast_width(viewport.cols);
      let right = if viewport.cols >= 100 { 2 } else { 1 };
      overlay()
        .absolute()
        .inset(px(0.0))
        .pointer_events(PointerEvents::None)
        .child(
          ui::surface_root()
            .flex_none()
            .absolute()
            .right(layout::cols(right))
            .bottom(layout::rows(2))
            .w(layout::cols(width))
            .h(layout::rows(ui::toast::TOAST_ROWS))
            // As the terminal's summary line, nsh leaves its own frame off the box.
            .role(AccessibilityRole::Summary)
            .child(ui::toast_box(
              toast_kind(toast.kind),
              toast.title,
              toast.sub,
              width,
            )),
        )
    })
  })
  .into_element()
}

/// The dialog of the overlay open (none when closed): the shell's own (help, logout
/// confirmation) or a page's (address, filter).
pub fn dialog(stores: Stores) -> impl IntoElement {
  let open = computed(move || stores.ui.overlay());
  dynamic(move || {
    open.get().and_then(|overlay| {
      untracked(|| match overlay {
        Overlay::Help => Some(overlays::help(stores)),
        Overlay::Logout => Some(overlays::logout(stores)),
        other => pages::dialog(other),
      })
    })
  })
}
