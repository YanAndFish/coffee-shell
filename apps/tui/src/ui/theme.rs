//! The design's semantic colours (`docs/design/tokens.md`), read from the theme
//! (`apps/tui/theme.json`). The only place that reads theme tokens.
//!
//! # Carriers
//!
//! nsh 0.6.1 draws an app's colours in a terminal as ANSI *meanings*, not values: a saturated
//! colour becomes the accent of its hue band (`Yellow`, `Green`, `Red`, `Cyan`, `Blue`,
//! `Magenta`), a neutral one the default foreground or the same plus `DIM`, and a fill is
//! drawn only on a box under three rows tall, as its exact RGB when it is saturated and the
//! standard gray otherwise. Nine shades of brown would all come out as "dim".
//!
//! So each design token travels through nsh on a **carrier** that survives that mapping, and
//! `coffee-tui`'s palette pin (`crates/coffee-tui/src/palette.rs`) swaps the carrier back for
//! the token's value at every colour depth:
//!
//! - a text tone ([`Tone`]) is a [`Channel`] (the ANSI meaning nsh picks) plus an optional
//!   *mark*: the italic attribute, which the design never uses for itself. The pin removes the
//!   mark, so no text is ever drawn italic. Marked tones survive monochrome too (attributes
//!   outlive colours there), which is where the pin gives them `DIM`.
//! - a fill ([`Fill`]) is an opaque, saturated `carrier-*` colour whose exact RGB (true
//!   colour) or xterm index (256 colours) nsh passes on unchanged.
//!
//! The tables [`INKS`] and [`FILLS`] are the contract between the two sides; the pin builds
//! itself from them and from `theme.json`. Draw colours only through this module (the
//! helpers in `ui::label` apply the mark): a raw `Tone::color()` without its mark would
//! reach the terminal as the wrong token.
//!
//! Never let a state depend on colour alone: pair it with a glyph or an attribute (bold,
//! underline, reverse, strike-through).

use nsh_base::theme;
use nsh_core::prelude::*;

/// The ANSI meaning nsh 0.6.1 gives a carrier colour (its `palette::tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
  /// The terminal's default foreground (`Reset`): neutral text far from the ground.
  Default,
  /// The default foreground with `DIM`: neutral text close to the ground.
  Muted,
  /// The yellow accent (hue 15°–75°).
  Yellow,
  /// The green accent (75°–165°).
  Green,
  /// The red accent (below 15° or from 335°).
  Red,
  /// The cyan accent (165°–200°).
  Cyan,
  /// The blue accent (200°–265°). nsh also draws its own focus marks in plain blue.
  Blue,
  /// The magenta accent (265°–335°).
  Magenta,
}

/// What a 16-colour terminal shows a text tone as (`docs/design/tokens.md`, "16 色").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ansi16 {
  /// The terminal's default foreground.
  Default,
  /// Bright black (SGR 90).
  Gray,
  /// Yellow (33).
  Yellow,
  /// Green (32).
  Green,
  /// Red (31).
  Red,
}

/// A text or line colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
  /// Primary text (`fg`).
  Fg,
  /// Secondary text: descriptions, options, chip labels (`soft`).
  Soft,
  /// Labels, table heads, units, hints (`mute`).
  Mute,
  /// An input's placeholder (`placeholder`).
  Placeholder,
  /// Unavailable options and actions (`disabled`; also the design's `steam`, which differs
  /// from it by under 2 % and has no channel of its own).
  Disabled,
  /// Decorative glyphs and the dialog frame, never readable text (`dim`).
  Dim,
  /// Panel, input and button lines (`line`).
  Line,
  /// Leader dots, receipt rules, neutral tag brackets, the radar's rim (`line-strong`).
  LineStrong,
  /// Empty meter segments (`line-soft`).
  LineSoft,
  /// The dotted rule between order rows (`line-row`).
  LineRow,
  /// The brand accent, amber: cursor, prices, legends, selection (`acc`).
  Acc,
  /// Good news, mint: success, attribute tags (`acc2`).
  Acc2,
  /// Mint outlines: tag and recommendation brackets, the success toast (`acc2-line`).
  Acc2Line,
  /// Errors and destructive actions (`danger`).
  Danger,
  /// Destructive outlines: logout brackets, the undo bar, the warning toast (`danger-line`).
  DangerLine,
  /// Text on an `acc`, `acc2` or `danger` fill (`ink`).
  Ink,
}

/// How one [`Tone`] reaches the terminal, and what each colour depth shows it as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InkSpec {
  /// The role.
  pub tone: Tone,
  /// Its value's key in `theme.json` (`cssVars.dark`).
  pub token: &'static str,
  /// The ANSI meaning nsh gives its carrier.
  pub channel: Channel,
  /// Whether the carrier is marked (italic), telling it apart from the channel's plain tone.
  pub marked: bool,
  /// What a 16-colour terminal shows.
  pub ansi16: Ansi16,
  /// Whether monochrome shows it dim.
  pub mono_dim: bool,
}

/// Every text tone's carrier. Each (channel, mark) pair appears once; plain `Blue` is left to
/// nsh's own focus marks, and `Ink` rides on the default foreground (the pin turns default
/// text on a light fill into `ink`).
pub const INKS: [InkSpec; 16] = [
  spec(
    Tone::Fg,
    "foreground",
    Channel::Default,
    false,
    Ansi16::Default,
    false,
  ),
  spec(
    Tone::Soft,
    "soft",
    Channel::Magenta,
    false,
    Ansi16::Default,
    false,
  ),
  spec(
    Tone::Mute,
    "muted-foreground",
    Channel::Muted,
    false,
    Ansi16::Gray,
    true,
  ),
  spec(
    Tone::Placeholder,
    "placeholder",
    Channel::Muted,
    true,
    Ansi16::Gray,
    true,
  ),
  spec(
    Tone::Disabled,
    "disabled",
    Channel::Yellow,
    true,
    Ansi16::Gray,
    true,
  ),
  spec(Tone::Dim, "dim", Channel::Magenta, true, Ansi16::Gray, true),
  spec(
    Tone::Line,
    "border",
    Channel::Cyan,
    false,
    Ansi16::Gray,
    false,
  ),
  spec(
    Tone::LineStrong,
    "line-strong",
    Channel::Cyan,
    true,
    Ansi16::Gray,
    true,
  ),
  spec(
    Tone::LineSoft,
    "line-soft",
    Channel::Blue,
    true,
    Ansi16::Gray,
    true,
  ),
  spec(
    Tone::LineRow,
    "line-row",
    Channel::Default,
    true,
    Ansi16::Gray,
    true,
  ),
  spec(
    Tone::Acc,
    "primary",
    Channel::Yellow,
    false,
    Ansi16::Yellow,
    false,
  ),
  spec(
    Tone::Acc2,
    "success",
    Channel::Green,
    false,
    Ansi16::Green,
    false,
  ),
  spec(
    Tone::Acc2Line,
    "success-border",
    Channel::Green,
    true,
    Ansi16::Green,
    false,
  ),
  spec(
    Tone::Danger,
    "destructive",
    Channel::Red,
    false,
    Ansi16::Red,
    false,
  ),
  spec(
    Tone::DangerLine,
    "destructive-border",
    Channel::Red,
    true,
    Ansi16::Red,
    false,
  ),
  spec(
    Tone::Ink,
    "primary-foreground",
    Channel::Default,
    false,
    Ansi16::Default,
    false,
  ),
];

const fn spec(
  tone: Tone,
  token: &'static str,
  channel: Channel,
  marked: bool,
  ansi16: Ansi16,
  mono_dim: bool,
) -> InkSpec {
  InkSpec {
    tone,
    token,
    channel,
    marked,
    ansi16,
    mono_dim,
  }
}

impl Tone {
  /// Its row of [`INKS`].
  pub fn spec(self) -> &'static InkSpec {
    INKS
      .iter()
      .find(|spec| spec.tone == self)
      .expect("every tone has a carrier")
  }

  /// The colour nsh is given for this role (its carrier). Pair it with [`Tone::marked`]:
  /// prefer `ui::label`, which does.
  pub fn color(self) -> AdaptiveColor {
    match self {
      // These read as themselves; their channel is their own meaning.
      Tone::Fg => fg(),
      Tone::Mute => mute(),
      Tone::Acc => acc(),
      Tone::Acc2 => acc2(),
      Tone::Danger => danger(),
      Tone::Ink => ink_color(),
      _ => channel_color(self.spec().channel),
    }
  }

  /// Whether the carrier is marked (drawn italic, which the pin removes).
  pub fn marked(self) -> bool {
    self.spec().marked
  }
}

/// A colour of nsh's `channel`.
fn channel_color(channel: Channel) -> AdaptiveColor {
  match channel {
    Channel::Default => fg(),
    Channel::Muted => mute(),
    Channel::Yellow => acc(),
    Channel::Green => acc2(),
    Channel::Red => danger(),
    Channel::Cyan => token("carrier-cyan", theme().border),
    Channel::Blue => token("carrier-blue", theme().border),
    Channel::Magenta => token("carrier-magenta", theme().foreground),
  }
}

/// A background role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fill {
  /// Panels, bars and dialogs (`surface`): drawn by nsh only on boxes under three rows tall
  /// (the status bar), as the standard gray the pin turns into `surface`.
  Surface,
  /// A key cap (`kbd`).
  Kbd,
  /// The cursor row's band (`selection`).
  Selection,
  /// The brand accent: the mode block, primary buttons, solid chips (`acc`).
  Acc,
  /// Mint: solid tags, the success toast's icon (`acc2`).
  Acc2,
  /// Coral: the warning toast's icon (`danger`).
  Danger,
}

/// What a 16-colour terminal does with a carrier fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fill16 {
  /// No fill: the glyph beside it carries the state (`▶`).
  Clear,
  /// Reverse video.
  Reverse,
}

/// How one carrier [`Fill`] reaches the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillSpec {
  /// The role.
  pub fill: Fill,
  /// Its value's key in `theme.json`.
  pub token: &'static str,
  /// Its carrier's key in `theme.json`: an opaque, saturated colour on the xterm cube, so
  /// nsh passes it on as exact RGB (true colour) or as that cube entry (256 colours).
  pub carrier: &'static str,
  /// What 16 colours show.
  pub ansi16: Fill16,
}

/// The fills that need a carrier (the others are nsh meanings the pin already knows:
/// `Surface` is the standard gray, `Acc`/`Acc2`/`Danger` their own exact RGB).
pub const FILLS: [FillSpec; 2] = [
  FillSpec {
    fill: Fill::Kbd,
    token: "kbd",
    carrier: "carrier-kbd",
    ansi16: Fill16::Reverse,
  },
  FillSpec {
    fill: Fill::Selection,
    token: "selection",
    carrier: "carrier-selection",
    ansi16: Fill16::Clear,
  },
];

/// The opacity of a carrier fill: opaque enough for nsh to draw it as its exact colour (a
/// "tone" fill, from 60 %), not opaque enough to be the ground its text is read against or a
/// pressable box's "stands out" fill (both only at 100 %). So text on a carrier band reads as
/// on the window, and a pressable row with a band keeps its text colours.
pub const CARRIER_OPACITY: f32 = 0.94;

/// The theme token nsh's own hover band is pinned to (no carrier: nsh draws the band).
pub const HOVER_TOKEN: &str = "hover";

/// Whether nsh's hover band shows (the 鼠标追踪 setting): one app per process (one per SSH
/// session), so a process-wide flag the host's pin reads for every frame.
static HOVER_BAND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Shows or hides nsh's hover band (components.md: rows and cards are banded on hover only
/// while 鼠标追踪 is on). nsh draws the band on every pressable view it hovers; the host pins
/// it to the background colour while this is off.
pub fn set_hover_band(shown: bool) {
  HOVER_BAND.store(shown, std::sync::atomic::Ordering::Relaxed);
}

/// Whether nsh's hover band shows now.
pub fn hover_band() -> bool {
  HOVER_BAND.load(std::sync::atomic::Ordering::Relaxed)
}

impl Fill {
  /// The colour nsh is given for this fill.
  pub fn color(self) -> AdaptiveColor {
    match self {
      Fill::Surface => surface(),
      Fill::Kbd => token("carrier-kbd", theme().card).opacity(CARRIER_OPACITY),
      Fill::Selection => token("carrier-selection", theme().card).opacity(CARRIER_OPACITY),
      Fill::Acc => acc(),
      Fill::Acc2 => acc2(),
      Fill::Danger => danger(),
    }
  }

  /// The text tone that reads on this fill.
  pub fn on(self) -> Tone {
    match self {
      Fill::Acc | Fill::Acc2 | Fill::Danger => Tone::Ink,
      Fill::Surface | Fill::Kbd | Fill::Selection => Tone::Fg,
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

/// Borders and separators (`line`), as nsh borders take it.
pub fn line() -> AdaptiveColor {
  Tone::Line.color()
}

/// Primary text (`fg`).
pub fn fg() -> AdaptiveColor {
  theme().foreground
}

/// Tertiary text: labels, heads, units (`mute`).
pub fn mute() -> AdaptiveColor {
  theme().muted_foreground
}

/// The brand accent, amber (`acc`).
pub fn acc() -> AdaptiveColor {
  theme().primary
}

/// Text on `acc` (`ink`).
fn ink_color() -> AdaptiveColor {
  theme().primary_foreground
}

/// Text on `acc` (`ink`).
pub fn ink() -> AdaptiveColor {
  ink_color()
}

/// Good news, mint (`acc2`).
pub fn acc2() -> AdaptiveColor {
  token("success", theme().primary)
}

/// Errors (`danger`).
pub fn danger() -> AdaptiveColor {
  theme().destructive
}

/// A key cap's fill (`kbd`).
pub fn kbd_bg() -> AdaptiveColor {
  Fill::Kbd.color()
}

/// The cursor row's band (`selection`).
pub fn selection() -> AdaptiveColor {
  Fill::Selection.color()
}

/// Secondary text (`soft`).
pub fn soft() -> AdaptiveColor {
  Tone::Soft.color()
}

/// Decorative glyphs (`dim`).
pub fn dim() -> AdaptiveColor {
  Tone::Dim.color()
}

/// Key caps' borders, leader lines (`line-strong`).
pub fn line_strong() -> AdaptiveColor {
  Tone::LineStrong.color()
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use super::*;

  fn theme() -> nsh_base::Theme {
    nsh_base::Theme::from_json(crate::THEME_JSON).unwrap()
  }

  #[test]
  fn the_theme_has_every_token_and_carrier() {
    let theme = theme();
    for spec in INKS {
      let known = matches!(
        spec.token,
        "foreground" | "muted-foreground" | "border" | "primary" | "primary-foreground"
      ) || matches!(spec.token, "destructive");
      assert!(
        known || theme.color(spec.token).is_some(),
        "theme.json lacks {}",
        spec.token
      );
    }
    for spec in FILLS {
      assert!(theme.color(spec.token).is_some(), "{}", spec.token);
      assert!(theme.color(spec.carrier).is_some(), "{}", spec.carrier);
    }
    for name in [
      "carrier-cyan",
      "carrier-magenta",
      "carrier-blue",
      HOVER_TOKEN,
      "steam",
    ] {
      assert!(theme.color(name).is_some(), "theme.json lacks {name}");
    }
  }

  #[test]
  fn every_tone_has_one_carrier_and_no_two_share_it() {
    let mut seen = HashSet::new();
    for spec in INKS {
      // `Ink` shares the default channel on purpose: the pin tells it by its fill.
      if spec.tone != Tone::Ink {
        assert!(seen.insert((spec.channel, spec.marked)), "{:?}", spec.tone);
      }
    }
    assert!(
      !seen.contains(&(Channel::Blue, false)),
      "plain blue is nsh's focus mark"
    );
    let tones: HashSet<Tone> = INKS.iter().map(|spec| spec.tone).collect();
    assert_eq!(tones.len(), INKS.len());
  }

  #[test]
  fn marked_tones_are_the_dim_and_outline_ones() {
    assert!(!Tone::Fg.marked() && !Tone::Acc.marked() && !Tone::Line.marked());
    for tone in [
      Tone::Disabled,
      Tone::Dim,
      Tone::LineStrong,
      Tone::Acc2Line,
      Tone::DangerLine,
      Tone::Placeholder,
    ] {
      assert!(tone.marked(), "{tone:?}");
    }
  }
}
