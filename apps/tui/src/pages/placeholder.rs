//! The plain bodies the pages ship with until their screen is built: a panel of text lines
//! that say what the page's stores hold, with pressable lines for the page's commands, and a
//! bare text field wired to the shell's focus tracking. No design here: each screen's worker
//! replaces its page's body with the real layout.

use nsh_core::{
  element::{Screen, input},
  prelude::*,
};

use crate::{
  commands::press,
  stores::{Phase, Stores},
  ui::{self, Tone, label},
};

/// One line of a placeholder body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
  /// What it says.
  pub text: String,
  /// Its tone.
  pub tone: Tone,
  /// The command a press on it runs (by id).
  pub command: Option<&'static str>,
}

impl Line {
  /// A line of `text` in `tone`.
  pub fn new(text: impl Into<String>, tone: Tone) -> Self {
    Self {
      text: text.into(),
      tone,
      command: None,
    }
  }

  /// A body line.
  pub fn text(text: impl Into<String>) -> Self {
    Self::new(text, Tone::Fg)
  }

  /// A secondary line.
  pub fn soft(text: impl Into<String>) -> Self {
    Self::new(text, Tone::Soft)
  }

  /// An error line (`✗ …`).
  pub fn error(text: impl Into<String>) -> Self {
    Self::new(text, Tone::Danger)
  }

  /// The line runs command `id` when pressed.
  pub fn press(mut self, id: &'static str) -> Self {
    self.command = Some(id);
    self
  }
}

/// The lines of a remote value's phase that is not ready: `⠋ 正在载入…`, `✗ {what}加载失败`
/// with the cause and `⏎ 重试`, or `∅ {empty}`; nothing when ready.
pub fn phase_lines(
  phase: Phase,
  failure: Option<crate::api::ApiFailure>,
  what: &str,
  empty: &str,
  retry: Option<&'static str>,
) -> Vec<Line> {
  match phase {
    Phase::Idle | Phase::Loading => vec![Line::soft("⠋ 正在载入…")],
    Phase::Failed => {
      let mut lines = vec![Line::error(format!("✗ {what}加载失败"))];
      if let Some(failure) = failure {
        lines.push(Line::soft(failure.detail()));
      }
      if let Some(retry) = retry {
        lines.push(Line::text("⏎ 重试").press(retry));
      }
      lines
    }
    Phase::Empty => vec![Line::soft(format!("∅ {empty}"))],
    Phase::Ready => Vec::new(),
  }
}

fn line_view(stores: Stores, line: Line) -> AnyElement {
  let text = label(line.text, line.tone).truncate();
  match line.command {
    Some(id) => view()
      .role(AccessibilityRole::Button)
      .focusable(false)
      .on_press(move || press(&stores, id))
      .child(text)
      .into_element(),
    None => text.into_element(),
  }
}

/// A page titled `title` (the stack's title, `选购 BROWSE`): a panel whose legend is
/// `legend()` and whose body is `lines()`, both following the stores.
pub fn screen(
  title: &'static str,
  legend: impl Fn() -> String + 'static,
  lines: impl Fn() -> Vec<Line> + 'static,
) -> Screen {
  ui::page(title, panel(legend, lines))
}

/// The panel of [`screen`], for a dialog or a part of a page.
pub fn panel(
  legend: impl Fn() -> String + 'static,
  lines: impl Fn() -> Vec<Line> + 'static,
) -> View {
  ui::panel(move || SharedString::from(legend()), self::lines(lines))
}

/// `lines()` as text lines, following the stores.
pub fn lines(lines: impl Fn() -> Vec<Line> + 'static) -> impl IntoElement {
  let stores = Stores::get();
  dynamic(move || {
    view()
      .flex_none()
      .children(lines().into_iter().map(move |line| line_view(stores, line)))
  })
}

/// A bare one-line text field named `field` (its name in `UiStore`'s focus tracking),
/// labelled `name`, showing `value()` and reporting edits to `on_change`; `masked` hides
/// what is typed (passwords). Its focus is reported to the shell, so the command table
/// knows a field is focused, and Esc leaves it; it takes the focus when
/// `UiStore::request_focus(field)` asks.
pub fn field(
  field: &'static str,
  name: &'static str,
  value: impl Fn() -> String + 'static,
  on_change: impl Fn(&str) + 'static,
  masked: bool,
) -> impl IntoElement {
  let stores = Stores::get();
  let ui_store = stores.ui;
  let handle = element_ref();
  // A field that goes away with the focus in it (its page left the stack) gives the keys
  // back: no blur is reported then.
  cleanup(move || ui_store.field_blurred(field));
  // A request is done once the focus arrived (a field shown for the request stays).
  effect(move || {
    if ui_store.focus_requested(field) {
      handle.focus();
    }
  });
  view()
    .flex_row()
    .gap_2()
    .child(label(name, Tone::Mute))
    .child(
      input()
        .flex_1()
        .label(name)
        .masked(masked)
        // Shown because the focus was asked for: focused as it appears (the ref's
        // `focus()` above reaches a field already on screen).
        .auto_focus(untracked(|| ui_store.focus_requested(field)))
        .value(value)
        .on_change(move |text: &str| on_change(text))
        .on_focus(move || {
          batch(|| {
            ui_store.field_focused(field, Some(handle));
            ui_store.focus_taken(field);
          })
        })
        .on_blur(move || ui_store.field_blurred(field))
        .element_ref(&handle),
    )
}
