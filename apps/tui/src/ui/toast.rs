//! Toast: a short notice, our own overlay at the bottom right (`components/Toast`). One at a
//! time, a newer one replaces it; the caller dismisses it after 2.6 s or on Esc.
//!
//! ```text
//! ╭───────────────────────────────────────────╮
//! │ ▐✓▌ 已加入购物车                          │
//! │     耶加雪菲 孔加 · 250g · 整豆 × 1       │
//! ╰───────────────────────────────────────────╯
//! ```
//!
//! 45×4 (narrower terminals: columns − 2), its right edge two cells from the screen's, its
//! bottom row just above the blank row over the status bar. Row one: the icon pill and the
//! title (bold); row two, four cells in: the subtitle (`mute`, one line, cut with `…`).
//! Kinds: ok `▐✓▌` (`acc2` fill, `acc2-line` frame), info `▐i▌` (`acc` fill, `dim` frame),
//! warn `▐!▌` (`danger` fill, `danger-line` frame); monochrome shows each pill reversed and
//! tells them apart by the glyph. nsh's own toast would take the status bar's row.

use nsh_core::prelude::*;

use super::{
  frame::{FrameStyle, frame},
  label::{label, strong},
  layout::{self, Layout},
  tag::pill_text,
  text::truncate,
  theme::{self, Fill, Tone},
};

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

  /// Its icon's fill.
  pub fn fill(self) -> Fill {
    match self {
      ToastKind::Ok => Fill::Acc2,
      ToastKind::Info => Fill::Acc,
      ToastKind::Warn => Fill::Danger,
    }
  }

  /// Its frame's tone.
  pub fn frame_tone(self) -> Tone {
    match self {
      ToastKind::Ok => Tone::Acc2Line,
      ToastKind::Info => Tone::Dim,
      ToastKind::Warn => Tone::DangerLine,
    }
  }

  fn tone(self) -> Tone {
    match self {
      ToastKind::Ok => Tone::Acc2,
      ToastKind::Info => Tone::Acc,
      ToastKind::Warn => Tone::Danger,
    }
  }
}

/// The toast's width in a terminal `cols` wide: 45, or `cols − 2` when narrower.
pub fn toast_width(cols: u16) -> u16 {
  45.min(cols.saturating_sub(2))
}

/// The toast's height.
pub const TOAST_ROWS: u16 = 4;

/// The toast box itself (`width` cells, 4 rows), where the caller puts it.
pub fn toast_box(kind: ToastKind, title: SharedString, sub: SharedString, width: u16) -> View {
  let inner = usize::from(width.saturating_sub(4));
  let title = truncate(&title, inner.saturating_sub(4));
  let sub = truncate(&sub, inner.saturating_sub(4));
  view()
    .flex_none()
    .w(layout::cols(width))
    .label("通知")
    .child(frame(
      move || FrameStyle::Round,
      move || kind.frame_tone(),
      None,
      Some(TOAST_ROWS),
      1,
      view()
        .flex_1()
        .child(
          view()
            .flex_row()
            .child(pill_text(kind.fill(), kind.glyph(), false))
            .child(text(" "))
            .child(strong(title, Tone::Fg)),
        )
        .child(
          view()
            .flex_row()
            .child(layout::hspace(4))
            .child(label(sub, Tone::Mute)),
        ),
    ))
}

/// The toast overlay of `current()` (none: nothing), at the bottom right of the screen,
/// over the content and never over the status bar. Its box clears what is behind it. It
/// takes no presses (`pointerEvents: none`): a notice is never in the way of the mouse, and
/// the fields and hints under it stay pressable.
pub fn toast(
  current: impl Fn() -> Option<(ToastKind, SharedString, SharedString)> + 'static,
) -> AnyElement {
  let layout = Layout::get();
  dynamic(move || {
    current().map(|(kind, title, sub)| {
      let viewport = layout.viewport();
      let width = toast_width(viewport.cols);
      let right = if viewport.cols >= 100 { 2 } else { 1 };
      overlay()
        .absolute()
        .inset(px(0.0))
        .pointer_events(PointerEvents::None)
        .child(
          view()
            .absolute()
            .right(layout::cols(right))
            .bottom(layout::rows(2))
            .w(layout::cols(width))
            .h(layout::rows(TOAST_ROWS))
            // An overlay's box with a fill of its own clears what is behind it; as the
            // terminal's summary line, nsh leaves its own frame off it.
            .role(AccessibilityRole::Summary)
            .bg(theme::bg)
            .child(toast_box(kind, title, sub, width)),
        )
    })
  })
}

/// The phase-1 toast line for `kind`, `title` and `sub`: one row, right-aligned, its glyph
/// first.
pub fn toast_line(kind: ToastKind, title: SharedString, sub: SharedString) -> impl IntoElement {
  view()
    .flex_none()
    .flex_row()
    .justify_end()
    .gap_2()
    .px_2()
    .label("通知")
    .child(strong(kind.glyph(), kind.tone()))
    .child(strong(title, Tone::Fg))
    .child(label(format!("· {sub}"), Tone::Mute))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_toast_fits_the_terminal() {
    assert_eq!(toast_width(160), 45);
    assert_eq!(toast_width(80), 45);
    assert_eq!(toast_width(46), 44);
    assert_eq!(ToastKind::Warn.glyph(), "!");
    assert_ne!(ToastKind::Ok.fill(), ToastKind::Warn.fill());
  }
}
