//! The shell's outlets: where toasts and dialogs show. Plain hooks: they place what the
//! stores say; the toast's and the dialogs' look belong to `ui` (next wave).

use nsh_core::prelude::*;

use super::overlays;
use crate::{
  pages,
  stores::{Overlay, Stores, ToastKind},
  ui,
};

/// The `ui` kind of a toast kind.
pub fn toast_kind(kind: ToastKind) -> ui::ToastKind {
  match kind {
    ToastKind::Ok => ui::ToastKind::Ok,
    ToastKind::Info => ui::ToastKind::Info,
    ToastKind::Warn => ui::ToastKind::Warn,
  }
}

/// The toast line: the toast showing, if any.
pub fn toast(stores: Stores) -> impl IntoElement {
  dynamic(move || {
    stores
      .toasts
      .current()
      .map(|toast| ui::toast_line(toast_kind(toast.kind), toast.title, toast.sub))
  })
}

/// The dialog of the overlay open (none when closed): the shell's own (help, logout
/// confirmation) or a page's (address, filter).
pub fn dialog(stores: Stores) -> impl IntoElement {
  dynamic(move || {
    stores.ui.overlay().and_then(|overlay| match overlay {
      Overlay::Help => Some(overlays::help(stores)),
      Overlay::Logout => Some(overlays::logout(stores)),
      other => pages::dialog(other),
    })
  })
}
