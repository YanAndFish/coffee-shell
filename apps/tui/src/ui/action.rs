//! An action written the design's way, key first (`⏎ 查看详情`, `t 重试`): a key cap and its
//! label, pressable with the mouse. The key itself is bound by the command table.

use nsh_core::{element::AccessibilityRole, prelude::*};

use super::{kbd, label::label, theme::Tone};

/// A view the mouse can press, which never takes the keyboard focus: every action has a key
/// in the command table, and a focused control would swallow Enter and Space.
pub fn pressable() -> View {
  view().role(AccessibilityRole::Button).focusable(false)
}

/// The action `label` of key `key`; a press runs `on_press`, as the key does.
///
/// ```ignore
/// ui::action("t", "重试", move || commands::press(&stores, "catalog.retry"))
/// ```
pub fn action(
  key: impl Into<SharedString>,
  label_text: impl Into<SharedString>,
  on_press: impl Fn() + 'static,
) -> impl IntoElement {
  pressable()
    .flex_row()
    .items_center()
    .gap_2()
    .on_press(on_press)
    .child(kbd(key.into()))
    .child(label(label_text.into(), Tone::Fg))
}
