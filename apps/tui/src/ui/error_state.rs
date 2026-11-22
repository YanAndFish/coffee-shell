//! An error in place of content: `✗ ` and what went wrong (bold `danger`), the detail
//! (`mute`), and the retry action, per the design's copy rule "错误以 `✗ ` 开头写清原因".

use nsh_core::prelude::*;

use super::{action, theme};

/// The error `title` ("加载商品失败"), its `detail` (the cause, for people) and the retry
/// action `key` / `label`, which runs `retry`.
pub fn error_state(
  title: impl IntoReactive<SharedString>,
  detail: impl IntoReactive<SharedString>,
  key: &'static str,
  label: &'static str,
  retry: impl Fn() + 'static,
) -> impl IntoElement {
  let title = title.into_reactive();
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .gap_4()
    .child(
      text(title.map(|title| SharedString::from(format!("✗ {title}"))))
        .text_color(theme::danger)
        .font_weight(FontWeight::BOLD),
    )
    .child(text(detail).text_color(theme::mute))
    .child(action(key, label, retry))
}
