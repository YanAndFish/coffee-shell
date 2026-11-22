//! Input: text entry (`components/Input`), three forms on nsh's own text field (which owns
//! the editing, the caret and the terminal's cursor).
//!
//! ```text
//! 用户名                                 ╭─ 优惠码 ───────────────╮
//! ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓     │ 试试 TTY10             │
//! ┃ › 林晓                          ┃     ╰────────────────────────╯
//! ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
//! ✗ 请输入用户名                         ╭───────────────────────────────────╮
//!                                        │ ‹/› 搜索豆子、产区或风味…         │
//!                                        ╰───────────────────────────────────╯
//! ```
//!
//! - A rounded `line` frame three rows tall; **focused** it turns heavy `┏━┓` and `acc`
//!   (glyph and colour). The content starts one cell in.
//! - [`InputForm::Prompt`] puts `› ` before the text (`mute`, bold `acc` while focused).
//!   [`InputForm::Search`] puts the `‹/›` cap before it and `✕` (pressable: clears) after it
//!   while it holds text. [`InputForm::Plain`] has neither.
//! - The label sits on the row above (`mute`), or on the top line (`╭─ 标签 ─`, saving two
//!   rows) with [`LabelPlace::OnFrame`] (short layouts, S).
//! - `masked`: every character shows as `•` (nsh's secure field). `uppercase`: what is typed
//!   is upper-cased (the coupon field). The placeholder is `placeholder`-toned.
//! - The error line under the group: `✗ 原因` (bold `danger`).
//!
//! Focus: `focused()` is the caller's focus ring (the page's or the dialog's); when it turns
//! to this field the field takes nsh's text focus, and when it turns away the field gives it
//! up — a focus nsh gave (Tab, a click) counts as asked, so a ring that follows the field's
//! own moves never asks for it again (three quick Tabs would otherwise end where the second
//! request lands). A field whose ring is on it
//! when it appears takes the focus as it mounts (`auto_focus`: an `ElementRef::focus()`
//! before the mount does nothing, so no page needs a mount delay). What the frame shows is
//! the field's real focus, and `on_focus_change(bool)` reports it (a click into the field,
//! Esc or Tab handled by the field itself), so the caller keeps its ring in step.
//! [`InputSpec::element`] lends the field's handle to the caller (to blur it from a key
//! handler at once, `UiStore::field_focused(name, Some(handle))`). The command table's keys
//! still reach the app while a field has the focus, except those the field types.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  error_state::error_line,
  frame::{FrameStyle, Legend, frame},
  kbd::kbd,
  label::{label, toned},
  layout,
  theme::Tone,
};

/// What leads the text inside an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputForm {
  /// `› 林晓`: the login fields.
  Prompt,
  /// Plain text: dialog fields, the coupon.
  Plain,
  /// `‹/› 搜索…` and a `✕` to clear: the header's search.
  Search,
}

/// Where an input's label goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LabelPlace {
  /// On the row above the frame.
  Above,
  /// On the frame's top line: `╭─ 标签 ─`.
  OnFrame,
}

/// What an input shows and how it behaves.
#[derive(Clone)]
pub struct InputSpec {
  /// Its form.
  pub form: InputForm,
  /// Its label (none for the search field).
  pub label: Option<SharedString>,
  /// Where the label goes.
  pub place: LabelPlace,
  /// A note at the label row's right end (`mute`): "2–16 个字符".
  pub hint: Option<SharedString>,
  /// The text shown while it is empty.
  pub placeholder: SharedString,
  /// Every character as `•`.
  pub masked: bool,
  /// What is typed is upper-cased.
  pub uppercase: bool,
  /// The field's handle, when the caller wants one (to blur it from a key handler in the
  /// same commit); the input makes its own otherwise.
  pub element: Option<ElementRef>,
  /// While this says so the field is disabled: Tab skips it, a click does not focus it and
  /// nothing types into it (a submit under way, when only `?` works: keymap.md K2 / K6).
  pub disabled: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

impl InputSpec {
  /// A field of `form` labelled `label` (above), no placeholder.
  pub fn new(form: InputForm, label: Option<&'static str>) -> Self {
    Self {
      form,
      label: label.map(SharedString::from),
      place: LabelPlace::Above,
      hint: None,
      placeholder: SharedString::default(),
      masked: false,
      uppercase: false,
      element: None,
      disabled: None,
    }
  }
}

/// An input group: the label, the framed field and the error line. `value()` is the text,
/// `on_change` gets every edit (upper-cased first when the spec says so), `on_submit` runs on
/// ⏎ inside it; `focused()` is the caller's focus ring, `on_focus_change` hears the field's
/// real focus come and go;
/// `error()` is the current error (none: no line).
pub fn input(
  spec: InputSpec,
  value: impl Fn() -> String + Copy + 'static,
  focused: impl Fn() -> bool + Copy + 'static,
  error: impl Fn() -> Option<SharedString> + 'static,
  on_change: impl Fn(String) + Copy + 'static,
  on_focus_change: impl Fn(bool) + 'static,
  on_submit: impl Fn() + 'static,
) -> View {
  let field_ref = spec.element.unwrap_or_else(element_ref);
  let has_focus = signal(false);
  let on_focus_change = std::rc::Rc::new(on_focus_change);
  let on_blur_change = on_focus_change.clone();
  let focused_by_caller = focused;
  let focused = move || has_focus.get();
  // Disabled (a submit under way), the field gives the focus up; when it is enabled again
  // with the ring still on it, it takes the focus back.
  let disabled = spec.disabled.clone();
  let is_disabled = move || disabled.as_ref().is_some_and(|disabled| disabled());
  let wanted = {
    let is_disabled = is_disabled.clone();
    move || focused_by_caller() && !is_disabled()
  };
  // The ring's focus when the field appears: it takes it as it mounts.
  let at_mount = untracked(&wanted);
  // What the field last asked of nsh, or nsh gave it (Tab, a click): a focus nsh gave counts
  // as asked, so the ring following it asks for nothing (a second request would land after
  // the next Tab and pull the focus back). Requests follow the ring's changes only: blurring
  // every field that is not the focused one would take the focus off the one that is.
  let asked = std::rc::Rc::new(std::cell::Cell::new(at_mount));
  let (on_focus_asked, on_blur_asked) = (asked.clone(), asked.clone());
  effect(move || {
    let now = wanted();
    if now && !asked.get() {
      field_ref.focus();
    } else if !now && asked.get() {
      field_ref.blur();
    }
    asked.set(now);
  });
  let uppercase = spec.uppercase;
  let name = spec
    .label
    .clone()
    .unwrap_or_else(|| spec.placeholder.clone());
  let field = nsh_core::element::input()
    .element_ref(&field_ref)
    .auto_focus(at_mount)
    .label(name)
    .flex_1()
    .min_w(px(0.0))
    .h(layout::rows(1))
    .value(value)
    .placeholder(spec.placeholder.to_string())
    .text_color(move || Tone::Fg.color())
    .masked(spec.masked)
    .disabled(is_disabled)
    .on_change(move |text: &str| {
      on_change(if uppercase {
        text.to_uppercase()
      } else {
        text.to_owned()
      })
    })
    .on_submit(move |_: &str| on_submit())
    .on_focus(move || {
      on_focus_asked.set(true);
      has_focus.set(true);
      on_focus_change(true);
    })
    .on_blur(move || {
      on_blur_asked.set(false);
      has_focus.set(false);
      on_blur_change(false);
    });
  let lead = match spec.form {
    InputForm::Prompt => Some(
      toned("› ", move || if focused() { Tone::Acc } else { Tone::Mute })
        .style_when(focused, |s| s.font_weight(FontWeight::BOLD))
        .into_element(),
    ),
    InputForm::Search => Some(
      view()
        .flex_row()
        .child(kbd("/"))
        .child(text(" "))
        .into_element(),
    ),
    InputForm::Plain => None,
  };
  let clear = (spec.form == InputForm::Search).then(|| {
    when(
      move || !value().is_empty(),
      move || {
        pressable()
          .label("清除")
          .on_press(move || on_change(String::new()))
          .child(label(" ✕", Tone::Mute))
      },
    )
  });
  let row = view().flex_row().child(lead).child(field).child(clear);
  let style = move || {
    if focused() {
      FrameStyle::Heavy
    } else {
      FrameStyle::Round
    }
  };
  let tone = move || if focused() { Tone::Acc } else { Tone::Line };
  let legend = match (spec.place, &spec.label) {
    (LabelPlace::OnFrame, Some(text)) => Some(Legend {
      text: text.clone().into_reactive(),
      tone: Tone::Mute,
    }),
    _ => None,
  };
  let above = match (spec.place, spec.label.clone()) {
    (LabelPlace::Above, Some(text)) => Some(
      view()
        .flex_row()
        .child(label(text, Tone::Mute))
        .child(view().flex_1())
        .child(spec.hint.clone().map(|hint| label(hint, Tone::Mute))),
    ),
    _ => None,
  };
  view()
    .flex_none()
    .child(above)
    .child(frame(style, tone, legend, Some(3), 1, row))
    .child(error_line(error))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_spec_starts_plain() {
    let spec = InputSpec::new(InputForm::Prompt, Some("用户名"));
    assert_eq!(spec.place, LabelPlace::Above);
    assert!(!spec.masked && !spec.uppercase);
    assert_eq!(spec.label.as_deref(), Some("用户名"));
  }
}
