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

/// `view` (a row or a card) with the design's hover (components.md: "悬停（仅在“鼠标追踪”开启
/// 时）"): while `tracking()` says the 鼠标追踪 setting is on, the mouse moving over it runs
/// `on_hover` (the cart and the address book move their cursor there). Only a move of the
/// pointer counts: a card that comes under a resting pointer (a dialog closing, the list
/// scrolling to the keys' cursor) does not take the cursor from the keys. The hover band itself
/// follows the same setting app-wide ([`super::theme::set_hover_band`]).
pub fn track_hover(
  view: View,
  tracking: impl Fn() -> bool + 'static,
  on_hover: impl Fn() + 'static,
) -> View {
  view.on_mouse_move(move |event: &MouseEvent| {
    let at = (event.page_position.x.0, event.page_position.y.0);
    // The root heard the pointer here before ([`follow_pointer`]): the row came under a
    // pointer at rest (a dialog closed, a toast went, the list scrolled to the keys' cursor).
    let moved = POINTER.with(|last| last.get() != Some(at));
    if moved && untracked(&tracking) {
      on_hover();
    }
  })
}

/// The screen's root view, hearing where the pointer is after every row and card did (a
/// move bubbles from the row up to the root): what tells [`track_hover`] a real move from a
/// row arriving under a pointer at rest.
pub fn follow_pointer(root: View) -> View {
  root.on_mouse_move(|event: &MouseEvent| {
    let at = (event.page_position.x.0, event.page_position.y.0);
    POINTER.with(|last| last.set(Some(at)));
  })
}

thread_local! {
  /// Where the pointer was at the last move the root heard (in the root view).
  static POINTER: std::cell::Cell<Option<(f32, f32)>> = const { std::cell::Cell::new(None) };
}
