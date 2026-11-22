//! StatusBar: the bottom row, a vim/tmux mode line (`components/StatusBar`):
//!
//! ```text
//!  选购 BROWSE  ↑↓ 移动  ⏎ 详情  t 重试        演示环境 · 不是真实商店   ? 帮助
//! ```
//!
//! The mode block in bold `acc` on the key-cap band, then the page's key hints (each
//! pressable, as the key), then the persistent demo notice and the help hint.
//!
//! The design draws the mode as `ink` on an `acc` fill. nsh's terminal backend maps a dark
//! neutral text colour such as `ink` to the terminal's own foreground, which is light on a
//! dark terminal: light on amber does not read. So the block keeps the brand colour on the
//! text and stands on the neutral band instead. It owns the bottom row:
//! nsh's own toasts and search line would draw there, so the app never uses them.

use nsh_core::{element::AccessibilityRole, prelude::*};

use super::{kbd, text::display_width, theme};

/// The mode: `选购 BROWSE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
  /// Chinese name, `选购`.
  pub zh: &'static str,
  /// English name in capitals, `BROWSE`.
  pub en: &'static str,
}

impl Mode {
  /// The cells the mode block takes (with its one-cell padding on each side).
  pub fn width(&self) -> usize {
    display_width(self.zh) + 1 + display_width(self.en) + 2
  }
}

/// One key hint: `↑↓ 移动`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hint {
  /// The command it runs when pressed.
  pub id: &'static str,
  /// The key cap's text, `↑↓`.
  pub key: &'static str,
  /// What it does, `移动`.
  pub text: &'static str,
}

impl Hint {
  /// The cells the hint takes: key cap (key plus a cell each side), a gap, the text.
  pub fn width(&self) -> usize {
    display_width(self.key) + 2 + 1 + display_width(self.text)
  }
}

/// The cells between two hints.
pub const HINT_GAP: usize = 2;

/// The status bar: `mode()`, the hints `hints()` (already fitted to the width), the demo
/// `notice` and the `help` hint; a press on a hint runs `on_hint(id)`.
pub fn status_bar(
  mode: impl Fn() -> Mode + 'static,
  hints: impl Fn() -> Vec<Hint> + 'static,
  notice: &'static str,
  help: Hint,
  on_hint: impl Fn(&'static str) + Copy + 'static,
) -> impl IntoElement {
  let mode = computed(mode);
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .bg(theme::surface)
    .child(
      view().flex_none().px_2().bg(theme::kbd_bg).child(
        text(move || {
          let mode = mode.get();
          SharedString::from(format!("{} {}", mode.zh, mode.en))
        })
        .text_color(theme::acc)
        .font_weight(FontWeight::BOLD),
      ),
    )
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap_4()
        .px_2()
        .overflow_hidden()
        .child(each(
          hints,
          |hint| hint.id,
          move |hint, _| hint_button(hint.get_untracked(), on_hint),
        )),
    )
    .child(text(notice).flex_none().text_color(theme::soft))
    .child(view().flex_none().px_2().child(hint_button(help, on_hint)))
}

fn hint_button(hint: Hint, on_hint: impl Fn(&'static str) + 'static) -> impl IntoElement {
  view()
    .role(AccessibilityRole::Button)
    .focusable(false)
    .flex_none()
    .flex_row()
    .items_center()
    .gap_2()
    .label(SharedString::from(format!("{} {}", hint.key, hint.text)))
    .on_press(move || on_hint(hint.id))
    .child(kbd(hint.key))
    .child(text(hint.text).text_color(theme::mute))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn widths_count_cells() {
    let mode = Mode {
      zh: "选购",
      en: "BROWSE",
    };
    assert_eq!(mode.width(), 13);
    let hint = Hint {
      id: "x",
      key: "↑↓",
      text: "移动",
    };
    assert_eq!(hint.width(), 2 + 2 + 1 + 4);
  }
}
