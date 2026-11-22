//! Toast, terminal form (`components/Toast`): one line above the status bar, its kind's glyph
//! first (`✓` / `i` / `!`), the title in bold, the next step after ` · `. One at a time.
//! nsh's own toasts would take the status bar's row, so the app draws its own.

use nsh_core::prelude::*;

use super::theme;

/// A toast's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
  /// Done (`✓`, `acc2`).
  Ok,
  /// For your information (`i`, `acc`).
  Info,
  /// Something needs you (`!`, `danger`).
  Warn,
}

impl ToastKind {
  /// Its glyph.
  pub fn glyph(self) -> &'static str {
    match self {
      ToastKind::Ok => "✓",
      ToastKind::Info => "i",
      ToastKind::Warn => "!",
    }
  }

  fn color(self) -> AdaptiveColor {
    match self {
      ToastKind::Ok => theme::acc2(),
      ToastKind::Info => theme::acc(),
      ToastKind::Warn => theme::danger(),
    }
  }
}

/// The toast line for `kind`, `title` and `sub`.
pub fn toast_line(kind: ToastKind, title: SharedString, sub: SharedString) -> impl IntoElement {
  view()
    .flex_none()
    .flex_row()
    .justify_end()
    .gap_2()
    .px_2()
    .label("通知")
    .child(
      text(kind.glyph())
        .text_color(move || kind.color())
        .font_weight(FontWeight::BOLD),
    )
    .child(
      text(title)
        .text_color(theme::fg)
        .font_weight(FontWeight::BOLD),
    )
    .child(text(format!("· {sub}")).text_color(theme::mute))
}
