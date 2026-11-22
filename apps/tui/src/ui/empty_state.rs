//! EmptyState: a centred graphic, one sentence and the one way out (`components/EmptyState`):
//!
//! ```text
//!   ┌────────────┐                       ∅
//!   │            │─╮
//!   │   空  杯   │ │             没有匹配的豆子
//!   │            │─╯
//!   └────────────┘              [ Esc 清除筛选 ]
//!    购物车是空的
//!
//!  ▐ 1 去挑豆子 ▌
//! ```
//!
//! The graphic is `dim` and hidden from screen readers; the sentence `soft`; the way out a
//! [`Button`] (primary or default); a blank row between each, the whole centred in its parent.

use nsh_core::prelude::*;

use super::{
  art::{EMPTY_SET, empty_cup},
  button::{Button, ButtonKind},
  label::label,
  layout,
  theme::Tone,
};

/// The way out of an empty state: a key, its label and what pressing it does.
pub struct EmptyAction {
  /// The key, `1`.
  pub key: &'static str,
  /// What it does, `去挑豆子`.
  pub label: &'static str,
  /// Runs it (as the key does).
  pub run: Box<dyn Fn()>,
}

/// The graphic of an empty state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmptyArt {
  /// The empty cup (18×5): an empty cart.
  Cup,
  /// One glyph: `∅` for no results.
  Glyph(&'static str),
}

/// An empty state: `art`, the `message` (`soft`), an optional `detail` (`mute`) and an
/// optional way out.
///
/// ```ignore
/// empty_state_with(EmptyArt::Cup, "购物车是空的", None,
///   Some(Button::new(ButtonKind::Primary, "1", "去挑豆子").on_press(move || go(0))))
/// ```
pub fn empty_state_with(
  art: EmptyArt,
  message: impl IntoReactive<SharedString>,
  detail: Option<SharedString>,
  way_out: Option<Button>,
) -> View {
  let graphic = match art {
    EmptyArt::Cup => empty_cup(),
    EmptyArt::Glyph(glyph) => view()
      .flex_none()
      .accessibility_elements_hidden(true)
      .child(label(glyph, Tone::Dim)),
  };
  let has_way_out = way_out.is_some();
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .child(graphic)
    .child(layout::vspace(1))
    .child(
      view()
        .flex_none()
        .items_center()
        .child(label(message, Tone::Soft))
        .child(detail.map(|detail| label(detail, Tone::Mute))),
    )
    .child(has_way_out.then(|| layout::vspace(1)))
    .child(way_out)
}

/// The phase-1 form: the decorative `glyph` (`∅`), the `message`, an optional `detail` and
/// an optional way out (drawn as a default button `[ key label ]`).
pub fn empty_state(
  glyph: &'static str,
  message: impl IntoReactive<SharedString>,
  detail: Option<SharedString>,
  way_out: Option<EmptyAction>,
) -> impl IntoElement {
  empty_state_with(
    EmptyArt::Glyph(glyph),
    message,
    detail,
    way_out.map(|way_out| {
      let run = way_out.run;
      Button::new(ButtonKind::Default, way_out.key, way_out.label).on_press(run)
    }),
  )
}

/// The "no results" glyph, for [`EmptyArt::Glyph`].
pub const NO_RESULTS: &str = EMPTY_SET;
