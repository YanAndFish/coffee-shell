//! Tag: small labels and badges (`components/Tag`), and the solid "pill" they share with
//! primary buttons, selected chips and the mode block.
//!
//! ```text
//! [新产季]  #茉莉  ▐店长推荐▌  ▐默认▌  [家]  [本单寄往]  ▐3▌
//! ```
//!
//! A pill is `▐` and `▌` in the fill's colour around text on the fill (`ink`, bold): the half
//! blocks round its ends. At 16 colours nsh draws the fill as a reversed band of its hue, and
//! in monochrome as reverse video, so it reads at every depth.

use nsh_core::prelude::*;

use super::{
  label::{label, strong},
  text::display_width,
  theme::{Fill, Tone},
};

/// The tone of a pill's half blocks: the fill's own colour.
fn end_tone(fill: Fill) -> Tone {
  match fill {
    Fill::Acc2 => Tone::Acc2,
    Fill::Danger => Tone::Danger,
    _ => Tone::Acc,
  }
}

/// A solid pill on `fill` holding `content` (already in the fill's text tone).
pub fn pill(fill: Fill, content: impl IntoElement) -> View {
  let ends = end_tone(fill);
  view()
    .flex_none()
    .flex_row()
    .child(label("▐", ends))
    .child(view().flex_row().bg(fill.color()).child(content))
    .child(label("▌", ends))
}

/// A solid pill with `text` (bold, in the fill's text tone), one blank cell inside each end
/// when `padded` (`▐ 选购 BROWSE ▌`) or none (`▐默认▌`).
pub fn pill_text(fill: Fill, text: impl IntoReactive<SharedString>, padded: bool) -> View {
  let text = text.into_reactive();
  let shown = if padded {
    text.map(|text| SharedString::from(format!(" {text} ")))
  } else {
    text
  };
  pill(fill, strong(shown, fill.on()))
}

/// A tag's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TagKind {
  /// `[新产季]`: mint brackets (`acc2-line`), mint text.
  Outline,
  /// `#茉莉`: a flavour note, mint.
  Note,
  /// `▐店长推荐▌`: solid mint.
  Solid,
  /// `▐默认▌`: solid amber (the default address).
  Default,
  /// `[家]`: neutral brackets (`line-strong`), `soft` text.
  Neutral,
}

/// The cells `text` takes as a tag of `kind`.
pub fn tag_width(kind: TagKind, text: &str) -> usize {
  display_width(text)
    + match kind {
      TagKind::Note => 1,
      _ => 2,
    }
}

/// A tag of `kind` saying `text` (2–4 characters read best).
pub fn tag(kind: TagKind, text: impl Into<SharedString>) -> View {
  let text = text.into();
  let bracketed = |bracket: Tone, inner: Tone| {
    view()
      .flex_none()
      .flex_row()
      .child(label("[", bracket))
      .child(label(text.clone(), inner))
      .child(label("]", bracket))
  };
  match kind {
    TagKind::Outline => bracketed(Tone::Acc2Line, Tone::Acc2),
    TagKind::Neutral => bracketed(Tone::LineStrong, Tone::Soft),
    TagKind::Note => view()
      .flex_none()
      .child(label(format!("#{text}"), Tone::Acc2)),
    TagKind::Solid => pill_text(Fill::Acc2, text, false),
    TagKind::Default => pill_text(Fill::Acc, text, false),
  }
}

/// The count badge `▐3▌` (`acc` fill, `ink` bold) of `count()`; nothing at 0.
pub fn count_badge(count: impl Fn() -> u32 + Copy + 'static) -> AnyElement {
  dynamic(move || {
    let n = count();
    (n > 0).then(|| pill_text(Fill::Acc, n.to_string(), false))
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tags_take_their_brackets() {
    assert_eq!(tag_width(TagKind::Outline, "新产季"), 8);
    assert_eq!(tag_width(TagKind::Note, "茉莉"), 5);
    assert_eq!(tag_width(TagKind::Default, "默认"), 6);
    assert_eq!(tag_width(TagKind::Neutral, "家"), 4);
  }
}
