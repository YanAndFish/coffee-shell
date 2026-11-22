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
  ui::{self, Layout},
};

/// The `ui` kind of a toast kind.
pub fn toast_kind(kind: ToastKind) -> ui::ToastKind {
  match kind {
    ToastKind::Ok => ui::ToastKind::Ok,
    ToastKind::Info => ui::ToastKind::Info,
    ToastKind::Warn => ui::ToastKind::Warn,
  }
}

/// The toast overlay: the toast showing, if any.
pub fn toast(stores: Stores, layout: Layout) -> AnyElement {
  ui::toast(move || {
    if layout.too_small() {
      return None;
    }
    stores
      .toasts
      .current()
      .map(|toast| (toast_kind(toast.kind), toast.title, toast.sub))
  })
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
