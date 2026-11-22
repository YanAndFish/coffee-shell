//! Text in one of the design's tones: the only way pages colour text.

use nsh_core::prelude::*;

use super::theme::Tone;

/// `content` in `tone`.
pub fn label(content: impl IntoReactive<SharedString>, tone: Tone) -> Text {
  text(content).text_color(move || tone.color())
}

/// `content` in `tone`, bold (weight 700: the terminal's bold).
pub fn strong(content: impl IntoReactive<SharedString>, tone: Tone) -> Text {
  label(content, tone).font_weight(FontWeight::BOLD)
}

/// `content` in the tone `tone()` gives now (tracked: a cursor row's name turns `acc`).
pub fn toned(content: impl IntoReactive<SharedString>, tone: impl Fn() -> Tone + 'static) -> Text {
  text(content).text_color(move || tone().color())
}
