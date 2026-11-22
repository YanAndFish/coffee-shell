//! The palette pin: the design's exact colours in place of the terminal's theme.
//!
//! nsh 0.6.1 draws an app's colours as the terminal's ANSI meanings
//! (`nsh_platform_tui`'s `palette.rs`): a saturated colour becomes the accent of its
//! hue band (`Yellow` for amber, `Green` for mint, `Red` for coral), near-neutral text
//! the default foreground (`Reset`), secondary text `Reset` + `DIM`, and, without
//! surfaces (no OSC 11 background given to it, the "bandless" look this host asks
//! for), a raised or sunken fill the standard gray (`DarkGray`) and an opaque
//! saturated fill its exact RGB. The user's terminal theme then decides what amber is.
//!
//! [`Pin`] walks the rendered ratatui buffer and puts the theme's values back
//! (`apps/tui/theme.json`, read through `coffee_app::THEME_JSON`):
//!
//! | nsh draws | pinned to |
//! |---|---|
//! | background `Reset` | `background` |
//! | background `DarkGray` (a card, popover, border fill) | `card` |
//! | `Yellow` / `LightYellow` | `primary` |
//! | `Green` / `LightGreen` | `success` |
//! | `Red` / `LightRed` | `destructive` |
//! | text `Reset` + `DIM` | `muted-foreground`, without `DIM` |
//! | text `Reset` | `foreground`; `primary-foreground` on a light fill |
//! | the bandless hover / press bands (10 % / 18 % white over black) | the same mixes of `background` toward `foreground` |
//!
//! At 24-bit colour the values are exact; at 256 colours each is the nearest entry of
//! the xterm cube and gray ramp (indices 16–255, which terminal themes leave alone).
//! 16 colours and monochrome are left to the terminal: there the ANSI meanings are the
//! point. Every other colour (pictures, fills nsh already drew as RGB or as an index)
//! passes through.

use nsh_platform_tui::ColorDepth;
use ratatui_core::{
  buffer::{Buffer, Cell},
  style::{Color, Modifier},
};

/// An RGB triple.
pub type Rgb = [u8; 3];

/// The theme colours the pin uses, from the dark half of `theme.json`'s `cssVars`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
  pub background: Rgb,
  pub foreground: Rgb,
  pub muted_foreground: Rgb,
  pub card: Rgb,
  pub primary: Rgb,
  pub primary_foreground: Rgb,
  pub success: Rgb,
  pub destructive: Rgb,
}

impl Palette {
  /// Reads the palette from a shadcn `cssVars` theme file (`{"cssVars":{"dark":{…}}}`)
  /// whose colours are `#rrggbb`.
  pub fn from_theme_json(json: &str) -> Result<Self, String> {
    let value: serde_json::Value =
      serde_json::from_str(json).map_err(|error| format!("theme.json: {error}"))?;
    let dark = value
      .pointer("/cssVars/dark")
      .and_then(serde_json::Value::as_object)
      .ok_or("theme.json: no cssVars.dark")?;
    let color = |name: &str| -> Result<Rgb, String> {
      let text = dark
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("theme.json: no dark `{name}`"))?;
      parse_hex(text).ok_or_else(|| format!("theme.json: `{name}` is not #rrggbb: {text}"))
    };
    Ok(Self {
      background: color("background")?,
      foreground: color("foreground")?,
      muted_foreground: color("muted-foreground")?,
      card: color("card")?,
      primary: color("primary")?,
      primary_foreground: color("primary-foreground")?,
      success: color("success")?,
      destructive: color("destructive")?,
    })
  }

  /// The palette of `coffee_app::THEME_JSON`.
  pub fn app() -> Result<Self, String> {
    Self::from_theme_json(coffee_app::THEME_JSON)
  }
}

/// `#rrggbb` as RGB.
pub fn parse_hex(text: &str) -> Option<Rgb> {
  let hex = text.strip_prefix('#')?;
  if hex.len() != 6 || !hex.is_ascii() {
    return None;
  }
  let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
  Some([channel(0)?, channel(2)?, channel(4)?])
}

/// The share of white over black nsh 0.6.1 gives a hovered control without surfaces
/// (`palette::HOVER_SHARE`), and a pressed one (`PRESS_SHARE`).
const HOVER_SHARE: f32 = 0.10;
const PRESS_SHARE: f32 = 0.18;

/// The theme's colours at one colour depth, ready to replace nsh's meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
  background: Color,
  foreground: Color,
  muted: Color,
  card: Color,
  primary: Color,
  ink: Color,
  success: Color,
  destructive: Color,
  /// nsh's bandless hover and press bands, and what they become.
  bands: [(Color, Color); 2],
  /// Fills whose text reads in the dark ink rather than the foreground.
  light_fills: [Color; 3],
}

impl Pin {
  /// The pin of `palette` at `depth`; `None` at 16 colours and without colours, which
  /// stay the terminal's.
  pub fn new(palette: Palette, depth: ColorDepth) -> Option<Self> {
    let at = |rgb: Rgb| -> Option<Color> {
      match depth {
        ColorDepth::TrueColor => Some(Color::Rgb(rgb[0], rgb[1], rgb[2])),
        ColorDepth::Ansi256 => Some(Color::Indexed(nearest_256(rgb))),
        _ => None,
      }
    };
    let band = |share: f32| {
      let gray = (255.0 * share).round() as u8;
      Some((
        at([gray; 3])?,
        at(mix(palette.background, palette.foreground, share))?,
      ))
    };
    Some(Self {
      background: at(palette.background)?,
      foreground: at(palette.foreground)?,
      muted: at(palette.muted_foreground)?,
      card: at(palette.card)?,
      primary: at(palette.primary)?,
      ink: at(palette.primary_foreground)?,
      success: at(palette.success)?,
      destructive: at(palette.destructive)?,
      bands: [band(HOVER_SHARE)?, band(PRESS_SHARE)?],
      light_fills: [
        at(palette.primary)?,
        at(palette.success)?,
        at(palette.destructive)?,
      ],
    })
  }

  /// The theme colour standing for nsh's accent `color`, if it is one.
  fn accent(&self, color: Color) -> Option<Color> {
    match color {
      Color::Yellow | Color::LightYellow => Some(self.primary),
      Color::Green | Color::LightGreen => Some(self.success),
      Color::Red | Color::LightRed => Some(self.destructive),
      _ => None,
    }
  }

  /// Pins one cell's colours.
  pub fn cell(&self, cell: &mut Cell) {
    let bg = match cell.bg {
      Color::Reset => self.background,
      Color::DarkGray => self.card,
      other => self.accent(other).unwrap_or_else(|| {
        self
          .bands
          .iter()
          .find(|(from, _)| *from == other)
          .map_or(other, |(_, to)| *to)
      }),
    };
    let fg = match cell.fg {
      Color::Reset if cell.modifier.contains(Modifier::DIM) => {
        cell.modifier.remove(Modifier::DIM);
        self.muted
      }
      // The default text on an accent fill (a primary button's label): nsh leaves it
      // to the terminal's foreground, the design draws it in the dark ink.
      Color::Reset
        if !cell.modifier.contains(Modifier::REVERSED) && self.light_fills.contains(&bg) =>
      {
        self.ink
      }
      Color::Reset => self.foreground,
      other => self.accent(other).unwrap_or(other),
    };
    cell.bg = bg;
    cell.fg = fg;
  }

  /// Pins every cell of `buffer`.
  pub fn buffer(&self, buffer: &mut Buffer) {
    for cell in &mut buffer.content {
      self.cell(cell);
    }
  }

  /// The pinned primary colour, for the host's own overlays.
  pub fn primary(&self) -> Color {
    self.primary
  }

  /// The pinned dark ink drawn on the primary colour.
  pub fn ink(&self) -> Color {
    self.ink
  }
}

/// `from` moved toward `to` by `share`, per channel, rounded as nsh rounds its bands.
pub fn mix(from: Rgb, to: Rgb, share: f32) -> Rgb {
  std::array::from_fn(|i| {
    let (a, b) = (f32::from(from[i]), f32::from(to[i]));
    (a + (b - a) * share).round() as u8
  })
}

/// The RGB value of xterm colour `index` (16–255: the 6×6×6 cube, then the gray ramp).
pub fn xterm_rgb(index: u8) -> Rgb {
  const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
  match index {
    16..=231 => {
      let cube = index - 16;
      [
        LEVELS[usize::from(cube / 36)],
        LEVELS[usize::from(cube / 6 % 6)],
        LEVELS[usize::from(cube % 6)],
      ]
    }
    232..=255 => [8 + 10 * (index - 232); 3],
    // The 16 standard colours are the terminal theme's; these are xterm's defaults.
    _ => {
      const STANDARD: [Rgb; 16] = [
        [0, 0, 0],
        [205, 0, 0],
        [0, 205, 0],
        [205, 205, 0],
        [0, 0, 238],
        [205, 0, 205],
        [0, 205, 205],
        [229, 229, 229],
        [127, 127, 127],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [92, 92, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
      ];
      STANDARD[usize::from(index)]
    }
  }
}

/// The xterm colour (16–255) nearest to `rgb`, by the "redmean" weighted distance,
/// which follows perceived difference better than plain RGB distance.
pub fn nearest_256(rgb: Rgb) -> u8 {
  (16..=255u8)
    .min_by(|&a, &b| distance(rgb, xterm_rgb(a)).total_cmp(&distance(rgb, xterm_rgb(b))))
    .unwrap_or(16)
}

/// The "redmean" approximation of perceived colour distance (squared).
fn distance(a: Rgb, b: Rgb) -> f32 {
  let [ar, ag, ab] = a.map(f32::from);
  let [br, bg, bb] = b.map(f32::from);
  let mean = (ar + br) / 2.0;
  let (dr, dg, db) = (ar - br, ag - bg, ab - bb);
  (2.0 + mean / 256.0) * dr * dr + 4.0 * dg * dg + (2.0 + (255.0 - mean) / 256.0) * db * db
}

#[cfg(test)]
mod tests {
  use super::*;

  fn palette() -> Palette {
    Palette::app().unwrap()
  }

  fn cell(fg: Color, bg: Color, modifier: Modifier) -> Cell {
    let mut cell = Cell::new("x");
    cell.fg = fg;
    cell.bg = bg;
    cell.modifier = modifier;
    cell
  }

  fn pinned(pin: &Pin, fg: Color, bg: Color, modifier: Modifier) -> Cell {
    let mut cell = cell(fg, bg, modifier);
    pin.cell(&mut cell);
    cell
  }

  #[test]
  fn the_theme_reads_as_the_design() {
    let palette = palette();
    assert_eq!(palette.background, [0x14, 0x11, 0x0e]);
    assert_eq!(palette.foreground, [0xec, 0xe4, 0xd6]);
    assert_eq!(palette.muted_foreground, [0xa3, 0x96, 0x84]);
    assert_eq!(palette.card, [0x1b, 0x17, 0x13]);
    assert_eq!(palette.primary, [0xf0, 0xb4, 0x5a]);
    assert_eq!(palette.primary_foreground, [0x1a, 0x14, 0x0d]);
    assert_eq!(palette.success, [0x8c, 0xc7, 0xa1]);
    assert_eq!(palette.destructive, [0xf0, 0x8a, 0x6c]);
  }

  #[test]
  fn hex_parsing_is_strict() {
    assert_eq!(parse_hex("#f0b45a"), Some([240, 180, 90]));
    assert_eq!(parse_hex("f0b45a"), None);
    assert_eq!(parse_hex("#f0b45"), None);
    assert_eq!(parse_hex("#zzb45a"), None);
    assert!(Palette::from_theme_json(r#"{"cssVars":{"light":{}}}"#).is_err());
  }

  #[test]
  fn true_color_restores_the_exact_tokens() {
    let pin = Pin::new(palette(), ColorDepth::TrueColor).unwrap();
    let none = Modifier::empty();
    let text = pinned(&pin, Color::Reset, Color::Reset, none);
    assert_eq!(
      (text.fg, text.bg),
      (Color::Rgb(0xec, 0xe4, 0xd6), Color::Rgb(0x14, 0x11, 0x0e))
    );
    let amber = pinned(&pin, Color::Yellow, Color::Reset, none);
    assert_eq!(amber.fg, Color::Rgb(0xf0, 0xb4, 0x5a));
    let mint = pinned(&pin, Color::Green, Color::Reset, none);
    assert_eq!(mint.fg, Color::Rgb(0x8c, 0xc7, 0xa1));
    let coral = pinned(&pin, Color::LightRed, Color::Reset, none);
    assert_eq!(coral.fg, Color::Rgb(0xf0, 0x8a, 0x6c));
    let muted = pinned(
      &pin,
      Color::Reset,
      Color::Reset,
      Modifier::DIM | Modifier::BOLD,
    );
    assert_eq!(muted.fg, Color::Rgb(0xa3, 0x96, 0x84));
    assert_eq!(
      muted.modifier,
      Modifier::BOLD,
      "DIM gives way to the exact colour"
    );
    let card = pinned(&pin, Color::Reset, Color::DarkGray, none);
    assert_eq!(card.bg, Color::Rgb(0x1b, 0x17, 0x13));
    assert_eq!(card.fg, Color::Rgb(0xec, 0xe4, 0xd6));
  }

  #[test]
  fn a_label_on_an_amber_fill_takes_the_dark_ink() {
    let pin = Pin::new(palette(), ColorDepth::TrueColor).unwrap();
    // nsh draws a primary button's opaque fill as its exact RGB, its label as Reset.
    let button = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(240, 180, 90),
      Modifier::empty(),
    );
    assert_eq!(button.bg, Color::Rgb(240, 180, 90));
    assert_eq!(button.fg, Color::Rgb(0x1a, 0x14, 0x0d));
    // Reversed text keeps the foreground: the terminal swaps the pinned pair.
    let selected = pinned(&pin, Color::Reset, Color::Reset, Modifier::REVERSED);
    assert_eq!(selected.fg, Color::Rgb(0xec, 0xe4, 0xd6));
    assert_eq!(selected.modifier, Modifier::REVERSED);
  }

  #[test]
  fn hover_and_press_bands_follow_the_theme() {
    let pin = Pin::new(palette(), ColorDepth::TrueColor).unwrap();
    let palette = palette();
    let hover = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(26, 26, 26),
      Modifier::empty(),
    );
    let [r, g, b] = mix(palette.background, palette.foreground, HOVER_SHARE);
    assert_eq!(hover.bg, Color::Rgb(r, g, b));
    let press = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(46, 46, 46),
      Modifier::empty(),
    );
    let [r, g, b] = mix(palette.background, palette.foreground, PRESS_SHARE);
    assert_eq!(press.bg, Color::Rgb(r, g, b));
  }

  #[test]
  fn other_colors_pass_through() {
    let pin = Pin::new(palette(), ColorDepth::TrueColor).unwrap();
    for color in [
      Color::Rgb(1, 2, 3),
      Color::Indexed(42),
      Color::Blue,
      Color::Cyan,
      Color::Magenta,
    ] {
      let cell = pinned(&pin, color, color, Modifier::ITALIC);
      assert_eq!((cell.fg, cell.bg), (color, color));
      assert_eq!(cell.modifier, Modifier::ITALIC);
    }
    let dim_accent = pinned(&pin, Color::Yellow, Color::Reset, Modifier::DIM);
    assert_eq!(
      dim_accent.modifier,
      Modifier::DIM,
      "only the default text's DIM goes"
    );
  }

  #[test]
  fn ansi256_takes_the_nearest_cube_and_ramp_entries() {
    let pin = Pin::new(palette(), ColorDepth::Ansi256).unwrap();
    let amber = pinned(&pin, Color::Yellow, Color::Reset, Modifier::empty());
    // #f0b45a → (255, 175, 95), xterm 215.
    assert_eq!(amber.fg, Color::Indexed(215));
    assert_eq!(xterm_rgb(215), [255, 175, 95]);
    // #14110e → the gray ramp's (18, 18, 18), xterm 233.
    assert_eq!(amber.bg, Color::Indexed(233));
    let mint = pinned(&pin, Color::Green, Color::Reset, Modifier::empty());
    // #8cc7a1 → (135, 215, 175), xterm 115.
    assert_eq!(mint.fg, Color::Indexed(115));
    for (fg, bg) in [(amber.fg, amber.bg), (mint.fg, mint.bg)] {
      for color in [fg, bg] {
        assert!(matches!(color, Color::Indexed(16..=255)), "{color:?}");
      }
    }
  }

  #[test]
  fn nearest_256_is_the_true_minimum() {
    for rgb in [
      [0x14, 0x11, 0x0e],
      [0xec, 0xe4, 0xd6],
      [0xa3, 0x96, 0x84],
      [0xf0, 0x8a, 0x6c],
      [0, 0, 0],
      [255, 255, 255],
      [128, 128, 128],
    ] {
      let best = nearest_256(rgb);
      assert!((16..=255).contains(&best));
      let d = distance(rgb, xterm_rgb(best));
      assert!(
        (16..=255u8).all(|i| distance(rgb, xterm_rgb(i)) >= d),
        "{rgb:?}"
      );
    }
    assert_eq!(nearest_256([255, 255, 255]), 231);
    assert_eq!(nearest_256([0, 0, 0]), 16);
  }

  #[test]
  fn sixteen_colors_and_mono_stay_the_terminals() {
    assert!(Pin::new(palette(), ColorDepth::Ansi16).is_none());
    assert!(Pin::new(palette(), ColorDepth::Monochrome).is_none());
  }

  #[test]
  fn a_whole_buffer_is_pinned() {
    use ratatui_core::layout::Rect;
    let pin = Pin::new(palette(), ColorDepth::TrueColor).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
    pin.buffer(&mut buffer);
    assert!(
      buffer
        .content
        .iter()
        .all(|cell| cell.bg == Color::Rgb(0x14, 0x11, 0x0e))
    );
  }
}
