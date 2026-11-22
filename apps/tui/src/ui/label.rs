//! Text in one of the design's tones: the only way pages colour text.
//!
//! Each helper applies the tone's carrier (`ui::theme`, "Carriers"): its colour and, for a
//! marked tone, the italic mark the host's palette pin turns back into the token. A `Text`
//! from here is also a span: put it in `text("")` with `.child(..)` to mix tones on one line.
//!
//! ```ignore
//! text("").child(label("产区 ", Tone::Mute)).child(strong("¥98", Tone::Acc))
//! ```

use std::rc::Rc;

use nsh_core::prelude::*;

use super::theme::Tone;

/// `content` in `tone`.
pub fn label(content: impl IntoReactive<SharedString>, tone: Tone) -> Text {
  let text = text(content).text_color(move || tone.color());
  if tone.marked() { text.italic() } else { text }
}

/// `content` in `tone`, bold (weight 700: the terminal's bold).
pub fn strong(content: impl IntoReactive<SharedString>, tone: Tone) -> Text {
  label(content, tone).font_weight(FontWeight::BOLD)
}

/// `content` in the tone `tone()` gives now (tracked: a cursor row's name turns `acc`).
pub fn toned(content: impl IntoReactive<SharedString>, tone: impl Fn() -> Tone + 'static) -> Text {
  let tone = Rc::new(tone);
  let marked = tone.clone();
  text(content)
    .text_color(move || tone().color())
    .style_when(move || marked().marked(), |s| s.italic())
}

/// `content` in the tone `tone()` gives now, bold while `bold()` holds (tracked).
pub fn toned_strong(
  content: impl IntoReactive<SharedString>,
  tone: impl Fn() -> Tone + 'static,
  bold: impl Fn() -> bool + 'static,
) -> Text {
  toned(content, tone).style_when(bold, |s| s.font_weight(FontWeight::BOLD))
}
