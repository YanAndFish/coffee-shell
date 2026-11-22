//! EmptyState: a centred glyph, one sentence and the one way out (`components/EmptyState`):
//!
//! ```text
//!        ∅
//!  没有匹配的豆子
//!   Esc 清除筛选
//! ```

use nsh_core::prelude::*;

use super::{action, theme};

/// The way out of an empty state: a key, its label and what pressing it does.
pub struct EmptyAction {
  /// The key, `1`.
  pub key: &'static str,
  /// What it does, `去挑豆子`.
  pub label: &'static str,
  /// Runs it (as the key does).
  pub run: Box<dyn Fn()>,
}

/// An empty state: the decorative `glyph` (`∅`), the `message` (`soft`), an optional
/// `detail` (`mute`) and an optional way out.
pub fn empty_state(
  glyph: &'static str,
  message: impl IntoReactive<SharedString>,
  detail: Option<SharedString>,
  way_out: Option<EmptyAction>,
) -> impl IntoElement {
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .gap_4()
    .child(text(glyph).text_color(theme::dim))
    .child(text(message).text_color(theme::soft))
    .child(detail.map(|detail| text(detail).text_color(theme::mute)))
    .child(way_out.map(|way_out| action(way_out.key, way_out.label, way_out.run)))
}
