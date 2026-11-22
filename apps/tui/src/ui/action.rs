//! An action written the design's way, key first (`⏎ 查看详情`, `t 重试`): a key cap and its
//! label, pressable with the mouse. The key itself is bound by the command table.

use nsh_core::{element::AccessibilityRole, prelude::*};

use super::{kbd, theme};

/// The action `label` of key `key`; a press runs `on_press`, as the key does.
pub fn action(
  key: impl Into<SharedString>,
  label: impl Into<SharedString>,
  on_press: impl Fn() + 'static,
) -> impl IntoElement {
  view()
    .role(AccessibilityRole::Button)
    .focusable(false)
    .flex_row()
    .items_center()
    .gap_2()
    .on_press(on_press)
    .child(kbd(key.into()))
    .child(text(label.into()).text_color(theme::fg))
}
