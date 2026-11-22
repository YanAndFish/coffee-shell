//! An error in place of content: `✗ ` and what went wrong (bold `danger`), the detail
//! (`mute`), and the retry button, per the design's copy rule "错误以 `✗ ` 开头写清原因".
//! Also the one-row error line under an input group ([`error_line`]).
//!
//! ```text
//!   ✗ 订单加载失败
//!   连接服务器失败
//!
//!   [ ⏎ 重试 ]
//! ```

use nsh_core::prelude::*;

use super::{
  button::{Button, ButtonKind},
  label::{label, strong},
  layout,
  theme::Tone,
};

/// `message` as an error: `✗ message`.
pub fn error_text(message: &str) -> String {
  format!("✗ {message}")
}

/// The error `title` ("加载商品失败"), its `detail` (the cause, for people) and the retry
/// button `key` / `label`, which runs `retry`.
pub fn error_state(
  title: impl IntoReactive<SharedString>,
  detail: impl IntoReactive<SharedString>,
  key: &'static str,
  retry_label: &'static str,
  retry: impl Fn() + 'static,
) -> impl IntoElement {
  let title = title.into_reactive();
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .child(
      view()
        .flex_none()
        .items_center()
        .child(strong(
          title.map(|title| SharedString::from(error_text(&title))),
          Tone::Danger,
        ))
        .child(label(detail, Tone::Mute)),
    )
    .child(layout::vspace(1))
    .child(Button::new(ButtonKind::Default, key, retry_label).on_press(retry))
}

/// An error line `✗ 原因` (bold `danger`; bold carries it in monochrome), or nothing while
/// `error()` is `None`.
pub fn error_line(error: impl Fn() -> Option<SharedString> + 'static) -> AnyElement {
  dynamic(move || error().map(|error| strong(error_text(&error), Tone::Danger)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn errors_lead_with_the_cross() {
    assert_eq!(error_text("密码至少 6 位"), "✗ 密码至少 6 位");
  }
}
