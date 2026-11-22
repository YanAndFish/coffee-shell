//! The design's semantic colours (`docs/design/raw/design-system/tokens.json`), read from the
//! theme (`apps/tui/theme.json`). The only place that reads theme tokens.
//!
//! Each accessor reads `theme()`, so pass it as a binding (`.text_color(theme::acc)`) to
//! follow theme changes. nsh's terminal backend draws these as ANSI meanings; the host pins
//! the exact colours. Never let a state depend on colour alone: pair it with a glyph or an
//! attribute (bold, underline, dim).

use nsh_base::theme;
use nsh_core::prelude::*;

/// A text or glyph colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
  /// Primary text (`fg`).
  Fg,
  /// Secondary text: descriptions, options (`soft`).
  Soft,
  /// Labels, table heads, units (`mute`).
  Mute,
  /// The brand accent: cursor, prices, legends, selection (`acc`).
  Acc,
  /// Good news: success, attribute tags (`acc2`).
  Acc2,
  /// Errors and destructive actions (`danger`).
  Danger,
  /// Decorative glyphs only, never readable text (`dim`).
  Dim,
  /// Text on an `acc` fill (`ink`).
  Ink,
}

impl Tone {
  /// The colour of this role in the current theme.
  pub fn color(self) -> AdaptiveColor {
    match self {
      Tone::Fg => fg(),
      Tone::Soft => soft(),
      Tone::Mute => mute(),
      Tone::Acc => acc(),
      Tone::Acc2 => acc2(),
      Tone::Danger => danger(),
      Tone::Dim => dim(),
      Tone::Ink => ink(),
    }
  }
}

/// An app token, or `fallback` when the theme lacks it.
fn token(name: &str, fallback: AdaptiveColor) -> AdaptiveColor {
  theme().color(name).unwrap_or(fallback)
}

/// The window and input background (`bg`).
pub fn bg() -> AdaptiveColor {
  theme().background
}

/// Panels, bars and dialogs (`surface`).
pub fn surface() -> AdaptiveColor {
  theme().card
}

/// Borders and separators (`line`).
pub fn line() -> AdaptiveColor {
  theme().border
}

/// Key caps' borders, leader lines (`line-strong`).
pub fn line_strong() -> AdaptiveColor {
  token("line-strong", theme().border)
}

/// A key cap's fill (`kbd-bg`).
pub fn kbd_bg() -> AdaptiveColor {
  token("kbd-bg", theme().card)
}

/// Primary text (`fg`).
pub fn fg() -> AdaptiveColor {
  theme().foreground
}

/// Secondary text (`soft`).
pub fn soft() -> AdaptiveColor {
  token("soft", theme().foreground)
}

/// Tertiary text: labels, heads, units (`mute`).
pub fn mute() -> AdaptiveColor {
  theme().muted_foreground
}

/// Decorative glyphs (`dim`).
pub fn dim() -> AdaptiveColor {
  token("dim", theme().muted_foreground)
}

/// The brand accent, amber (`acc`).
pub fn acc() -> AdaptiveColor {
  theme().primary
}

/// The cursor row's band: `acc` at 10 %, as the design's `acc-tint-10`.
pub fn acc_tint() -> AdaptiveColor {
  theme().primary.opacity(0.1)
}

/// Text on `acc` (`ink`).
pub fn ink() -> AdaptiveColor {
  theme().primary_foreground
}

/// Good news, mint (`acc2`).
pub fn acc2() -> AdaptiveColor {
  token("success", theme().primary)
}

/// Errors (`danger`).
pub fn danger() -> AdaptiveColor {
  theme().destructive
}

#[cfg(test)]
mod tests {
  #[test]
  fn the_theme_has_every_app_token() {
    let theme = nsh_base::Theme::from_json(crate::THEME_JSON).unwrap();
    for name in ["soft", "success", "kbd-bg", "line-strong", "dim"] {
      assert!(theme.color(name).is_some(), "theme.json lacks {name}");
    }
  }
}
