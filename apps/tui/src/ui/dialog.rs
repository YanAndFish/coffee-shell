//! Dialog: a modal box over the page (`components/Dialog`): a double frame (`dim`), its title
//! on the top line, two blank rows, the content four cells in, the footer buttons
//! right-aligned. [`confirm`] is the small one of the logout confirmation (56×11).
//!
//! ```text
//! ╔═ 退出登录 LOGOUT ════════════════════════════════════╗
//! ║                                                      ║
//! ║                                                      ║
//! ║    确定退出登录吗？                                  ║
//! ║    同时解除当前公钥的免密登录。                      ║
//! ║                                                      ║
//! ║    再按 q 也可以确认                                 ║
//! ║                                                      ║
//! ║                      [ Esc 取消 ]  ▐ ⏎ 退出登录 ▌    ║
//! ║                                                      ║
//! ╚══════════════════════════════════════════════════════╝
//! ```
//!
//! It is an overlay of the root view, centred between the system row and the status bar,
//! so neither is covered and the command table keeps every key: the status bar shows the
//! dialog's mode, and the caller's commands (Esc, ⏎, q, Tab) act on it. Its box clears what
//! is behind it. The **focus trap** is [`DialogFocus`]: the dialog's own stops (fields and
//! buttons) in a ring that Tab / Shift+Tab / ← → turn and never leave; a field's real text
//! focus follows it ([`super::input::input`]), a button shows it with `›` and an underline.

use nsh_core::prelude::*;

use super::{
  button::{Button, ButtonKind},
  frame::{FrameStyle, Legend, frame},
  label::{label, strong},
  layout::{self, Layout, Viewport},
  theme::{self, Tone},
};

/// The focus ring of a dialog: `count` stops, one of them current. `Copy`; make it when the
/// dialog opens (with the stop that takes the focus first) and drop it when it closes.
#[derive(Clone, Copy)]
pub struct DialogFocus {
  at: Signal<usize>,
  count: usize,
}

impl DialogFocus {
  /// A ring of `count` stops, the focus on `first`.
  pub fn new(count: usize, first: usize) -> Self {
    Self {
      at: signal(first.min(count.saturating_sub(1))),
      count,
    }
  }

  /// The stop with the focus (tracked).
  pub fn current(&self) -> usize {
    self.at.get()
  }

  /// Whether stop `index` has the focus (tracked).
  pub fn is(&self, index: usize) -> bool {
    self.at.get() == index
  }

  /// The next stop (Tab, →), wrapping.
  pub fn next(&self) {
    let count = self.count.max(1);
    self.at.update(|at| *at = (*at + 1) % count);
  }

  /// The previous stop (Shift+Tab, ←), wrapping.
  pub fn prev(&self) {
    let count = self.count.max(1);
    self.at.update(|at| *at = (*at + count - 1) % count);
  }

  /// Puts the focus on `index`.
  pub fn set(&self, index: usize) {
    self.at.set(index.min(self.count.saturating_sub(1)));
  }
}

/// The dialog's size in a terminal: the design's `cols` × `rows`, narrowed to the screen's
/// columns − 4 and to the rows between the system row and the status bar.
pub fn dialog_size(cols: u16, rows: u16, viewport: Viewport) -> (u16, u16) {
  (
    cols.min(viewport.cols.saturating_sub(4)),
    rows.min(viewport.rows.saturating_sub(2)),
  )
}

/// The dialog box itself (`cols` × `rows`), where the caller puts it: the double frame with
/// `title`, `content` `pad` cells in from each side (the design's 4; 3 in the 70-column
/// address dialog) after two blank rows, and the right-aligned `footer` with a blank row
/// under it.
pub fn dialog_box(
  title: impl IntoReactive<SharedString>,
  cols: u16,
  rows: u16,
  pad: u16,
  content: impl IntoElement,
  footer: Vec<Button>,
) -> View {
  view()
    .flex_none()
    .w(layout::cols(cols))
    .h(layout::rows(rows))
    .child(frame(
      || FrameStyle::Double,
      || Tone::Dim,
      Some(Legend::new(title)),
      Some(rows),
      pad,
      view()
        .flex_1()
        .child(layout::vspace(2))
        .child(view().flex_1().min_h(px(0.0)).child(content))
        .child(
          view()
            .flex_none()
            .flex_row()
            .justify_end()
            .gap(layout::gap_cols(2))
            .children(footer),
        )
        .child(layout::vspace(1)),
    ))
}

/// The size and padding of a dialog: the design's `cols` × `rows` and its side padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogSize {
  /// Columns (narrowed to the terminal's − 4).
  pub cols: u16,
  /// Rows (narrowed to those between the system row and the status bar).
  pub rows: u16,
  /// Blank cells between each side line and the content.
  pub pad: u16,
}

/// A dialog overlay of `size` (narrowed to the terminal), centred between the system row and
/// the status bar, shown while `open()` holds. `title` goes on its top line; `content()` and
/// `footer()` are made each time it opens.
pub fn dialog(
  open: impl Fn() -> bool + 'static,
  title: impl Into<SharedString>,
  size: DialogSize,
  content: impl Fn() -> AnyElement + 'static,
  footer: impl Fn() -> Vec<Button> + 'static,
) -> AnyElement {
  let layout = Layout::get();
  let title: SharedString = title.into();
  dynamic(move || {
    open().then(|| {
      let viewport = layout.viewport();
      let (w, h) = dialog_size(size.cols, size.rows, viewport);
      overlay().absolute().inset(px(0.0)).child(
        view()
          .absolute()
          .top(layout::rows(1))
          .bottom(layout::rows(1))
          .left(px(0.0))
          .right(px(0.0))
          .items_center()
          .justify_center()
          .child(
            view()
              .flex_none()
              .w(layout::cols(w))
              .h(layout::rows(h))
              // A box of its own fill clears what is behind it; as the terminal's summary
              // line, nsh leaves its own frame off it (the double frame is the dialog's).
              .role(AccessibilityRole::Summary)
              .label(title.clone())
              .bg(theme::bg)
              .child(dialog_box(
                title.clone(),
                w,
                h,
                size.pad,
                content(),
                footer(),
              )),
          ),
      )
    })
  })
}

/// The text of a confirmation dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmText {
  /// Its title, "退出登录 LOGOUT".
  pub title: SharedString,
  /// The question (bold), "确定退出登录吗？".
  pub question: SharedString,
  /// What follows from it (`mute`).
  pub detail: SharedString,
  /// A note after a blank row (`mute`), "再按 q 也可以确认".
  pub note: SharedString,
  /// The primary button's label, "退出登录" (its key is ⏎).
  pub confirm: SharedString,
}

/// The logout confirmation (56×11) saying `words`, shown while `open()` holds: the buttons
/// `[ Esc 取消 ]` and `▐ ⏎ {confirm} ▌`; `focus` says which has the focus (0: cancel,
/// 1: confirm). A press runs `on_cancel` / `on_confirm` (the commands the keys run).
pub fn confirm(
  open: impl Fn() -> bool + 'static,
  words: ConfirmText,
  focus: DialogFocus,
  on_cancel: impl Fn() + Clone + 'static,
  on_confirm: impl Fn() + Clone + 'static,
) -> AnyElement {
  let body = words.clone();
  let confirm_label = words.confirm.clone();
  dialog(
    open,
    words.title,
    DialogSize {
      cols: CONFIRM_COLS,
      rows: CONFIRM_ROWS,
      pad: 4,
    },
    move || {
      view()
        .child(strong(body.question.clone(), Tone::Fg))
        .child(label(body.detail.clone(), Tone::Mute))
        .child(text(" "))
        .child(label(body.note.clone(), Tone::Mute))
        .into_element()
    },
    move || {
      vec![
        Button::new(ButtonKind::Default, "Esc", "取消")
          .focused(move || focus.is(0))
          .on_press(on_cancel.clone()),
        Button::new(ButtonKind::Primary, "⏎", confirm_label.clone())
          .focused(move || focus.is(1))
          .on_press(on_confirm.clone()),
      ]
    },
  )
}

/// The confirmation dialog's size (`06-profile.md`).
pub const CONFIRM_COLS: u16 = 56;
/// The confirmation dialog's height.
pub const CONFIRM_ROWS: u16 = 11;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dialogs_fit_between_the_bars() {
    assert_eq!(dialog_size(56, 11, Viewport::new(160, 50)), (56, 11));
    assert_eq!(dialog_size(112, 24, Viewport::new(100, 30)), (96, 24));
    assert_eq!(dialog_size(70, 23, Viewport::new(80, 24)), (70, 22));
    assert_eq!(dialog_size(56, 11, Viewport::new(80, 24)), (56, 11));
  }
}
