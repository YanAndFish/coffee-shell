//! Toggle: check boxes, switches and radio rows (`components/Toggle`). The state is always a
//! glyph (`[✓]`/`[ ]`, `■ 开`/`□ 关`, `◉`/`○`) plus an attribute, never colour alone.
//!
//! ```text
//! [✓] 记住我 30 天          界面动画 m                       [ ■ 开 ]
//! [ ] 设为默认              鼠标追踪 o                       [ □ 关 ]
//! ◉ 全部                8
//! ○ 埃塞俄比亚          1
//! ```
//!
//! Every toggle takes its state as a closure, and a press runs the caller's handler (the
//! command the key also runs): none of them takes the keyboard focus.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  label::{label, toned},
  layout,
  theme::{Fill, Tone},
};

/// The glyph of a check box: `[✓]` or `[ ]`.
pub fn check_glyph(checked: bool) -> &'static str {
  if checked { "[✓]" } else { "[ ]" }
}

/// The text of a switch: `[ ■ 开 ]` or `[ □ 关 ]` (8 cells).
pub fn switch_glyph(on: bool) -> (&'static str, &'static str) {
  if on { ("■", "开") } else { ("□", "关") }
}

/// The glyph of a radio row: `◉` or `○`.
pub fn radio_glyph(selected: bool) -> &'static str {
  if selected { "◉" } else { "○" }
}

/// A check box `[✓] text` while `checked()`; `disabled()` dims and strikes the whole row
/// (give the reason on the next line, see [`toggle_note`]). A press runs `on_toggle` unless
/// disabled; `focused()` shows the keyboard focus (`›` to the left, bold underlined text).
pub fn checkbox(
  text_content: impl Into<SharedString>,
  checked: impl Fn() -> bool + Copy + 'static,
  disabled: impl Fn() -> bool + Copy + 'static,
  focused: impl Fn() -> bool + Copy + 'static,
  on_toggle: impl Fn() + 'static,
) -> View {
  let box_tone = move || {
    if disabled() {
      Tone::Disabled
    } else if checked() {
      Tone::Acc
    } else {
      Tone::Mute
    }
  };
  let text_tone = move || {
    if disabled() {
      Tone::Disabled
    } else {
      Tone::Soft
    }
  };
  let text_content: SharedString = text_content.into();
  pressable()
    .flex_none()
    .flex_row()
    .label(text_content.clone())
    .on_press(move || {
      if !disabled() {
        on_toggle();
      }
    })
    .child(focus_mark(focused))
    .child(toned(move || check_glyph(checked()), box_tone).style_when(
      move || checked() && !disabled(),
      |s| s.font_weight(FontWeight::BOLD),
    ))
    .child(text(" "))
    .child(
      toned(text_content, text_tone)
        .style_when(disabled, |s| s.line_through())
        .style_when(focused, |s| s.font_weight(FontWeight::BOLD).underline()),
    )
}

/// The note under a toggle (`mute`), indented to its text: the reason a check box is
/// disabled, what a switch does.
pub fn toggle_note(note: impl IntoReactive<SharedString>, indent: u16) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(layout::hspace(indent))
    .child(label(note, Tone::Mute))
}

/// The switch itself, `[ ■ 开 ]` while `on()` else `[ □ 关 ]` (8 cells): on is `acc` and
/// bold, off `line` brackets and `mute` text.
pub fn switch(on: impl Fn() -> bool + Copy + 'static, on_toggle: impl Fn() + 'static) -> View {
  let bracket = move || if on() { Tone::Acc } else { Tone::Line };
  let inner = move || if on() { Tone::Acc } else { Tone::Mute };
  pressable()
    .flex_none()
    .flex_row()
    .label("开关")
    .on_press(on_toggle)
    .child(toned("[ ", bracket))
    .child(
      toned(
        move || {
          let (glyph, word) = switch_glyph(on());
          SharedString::from(format!("{glyph} {word}"))
        },
        inner,
      )
      .style_when(on, |s| s.font_weight(FontWeight::BOLD)),
    )
    .child(toned(" ]", bracket))
}

/// A setting row: `title key` (`fg` + `mute` key letter) with the switch right-aligned and
/// `note` (`mute`) on the next line.
pub fn switch_row(
  title: impl Into<SharedString>,
  key: impl Into<SharedString>,
  note: impl Into<SharedString>,
  on: impl Fn() -> bool + Copy + 'static,
  on_toggle: impl Fn() + 'static,
) -> View {
  view()
    .flex_none()
    .child(
      view()
        .flex_row()
        .child(
          text("")
            .child(label(title.into(), Tone::Fg))
            .child(label(format!(" {}", key.into()), Tone::Mute)),
        )
        .child(view().flex_1())
        .child(switch(on, on_toggle)),
    )
    .child(label(note.into(), Tone::Mute))
}

/// A radio row `◉ 全部                8`: the glyph and name, and an optional `count`
/// right-aligned (`mute`). The selected row is `acc` and bold on the `selection` band.
pub fn radio(
  text_content: impl Into<SharedString>,
  count: Option<SharedString>,
  selected: impl Fn() -> bool + Copy + 'static,
  on_select: impl Fn() + 'static,
) -> View {
  let tone = move || if selected() { Tone::Acc } else { Tone::Soft };
  let text_content: SharedString = text_content.into();
  // The fill is on a box inside the pressable one: nsh paints the text of a pressable
  // box with a saturated fill in that fill's accent.
  let band = view().flex_1().flex_row().bg(move || {
    if selected() {
      Fill::Selection.color()
    } else {
      AdaptiveColor::TRANSPARENT
    }
  });
  pressable()
    .flex_none()
    .flex_row()
    .label(text_content.clone())
    .on_press(on_select)
    .child(
      band
        .child(
          toned(
            {
              let name = text_content;
              move || SharedString::from(format!("{} {name}", radio_glyph(selected())))
            },
            tone,
          )
          .style_when(selected, |s| s.font_weight(FontWeight::BOLD)),
        )
        .child(view().flex_1())
        .child(count.map(|count| label(count, Tone::Mute))),
    )
}

/// A focus mark: `›` in the cell left of a control while `focused()`.
pub(crate) fn focus_mark(focused: impl Fn() -> bool + 'static) -> View {
  view()
    .absolute()
    .top(px(0.0))
    .left(px(-layout::CELL_WIDTH))
    .style_when(move || !focused(), |s| s.hidden())
    .child(label("›", Tone::Acc).font_weight(FontWeight::BOLD))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn glyphs_carry_the_state() {
    assert_eq!(check_glyph(true), "[✓]");
    assert_eq!(check_glyph(false), "[ ]");
    let (glyph, word) = switch_glyph(true);
    assert_eq!(display_width(&format!("[ {glyph} {word} ]")), 8);
    assert_eq!(switch_glyph(false), ("□", "关"));
    assert_ne!(radio_glyph(true), radio_glyph(false));
  }
}
