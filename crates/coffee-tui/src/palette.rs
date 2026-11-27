//! The palette pin: the design's exact colours in place of the terminal's theme.
//!
//! nsh (0.6.1 and 0.7.1) draws an app's colours as the terminal's ANSI meanings
//! (`nsh_platform_tui`'s `palette.rs`): a saturated colour becomes the accent of its
//! hue band (`Yellow` for amber, `Green` for mint, `Red` for coral, `Cyan`, `Blue`,
//! `Magenta`), near-neutral text the default foreground (`Reset`), secondary text `Reset` +
//! `DIM`, and, without surfaces (no OSC 11 background given to it, the "bandless" look
//! this host asks for), a fill on a box under three rows the standard gray (`DarkGray`) when
//! it is neutral and its exact RGB (or xterm index) when it is opaque and saturated. The
//! user's terminal theme would then decide what amber is, and every brown of the design would
//! be one "dim".
//!
//! The app sends each design token on a **carrier** that survives that mapping
//! (`coffee_app::ui::theme`: [`INKS`] for text, [`FILLS`] for fills): a channel (an ANSI
//! meaning), optionally *marked* with the italic attribute the design never uses, or an
//! exact carrier colour for a fill. [`Pin`] walks the rendered ratatui buffer and puts the
//! tokens back (`apps/tui/theme.json`, read through `coffee_app::THEME_JSON`):
//!
//! | nsh draws | true colour / 256 colours | 16 colours | monochrome |
//! |---|---|---|---|
//! | background `Reset` | `background` | kept | kept |
//! | background `DarkGray` (a raised fill) | `card` | kept | kept |
//! | a carrier fill (`carrier-kbd`, `carrier-selection`) | `kbd`, `selection` | none, or reverse video | (nsh reverses it) |
//! | the bandless hover / press bands | `hover` (`background` while the app hides it: 鼠标追踪 off) / 18 % of `foreground` over `background` | kept | kept |
//! | text `Reset` | `foreground`; `primary-foreground` on an accent fill | kept | kept |
//! | text `Reset` + `DIM` | `muted-foreground`, without `DIM` | kept | kept |
//! | text in a channel, plain or marked | its token (`INKS`), unmarked | its 16-colour role, unmarked | unmarked; `DIM` when marked |
//! | nsh's own focus mark (plain `Blue`) | `primary`; a reversed focus band becomes a `selection` band | `Yellow` | kept |
//!
//! Since nsh 0.7, at 24-bit colour nsh repaints every ANSI `Blue` it drew (its own accent, and
//! our `Blue` channel's carriers alike) in its brand indigo ([`NSH_BRAND_ACCENT`]) before the
//! frame reaches the host. The pin first reads that indigo as the `Blue` it stands for
//! ([`Pin::unbrand`]), so the table above holds unchanged.
//!
//! At 24-bit colour the values are exact; at 256 colours each is the nearest entry of the
//! xterm cube and gray ramp (indices 16–255, which terminal themes leave alone). Every other
//! colour (pictures, fills nsh already drew as RGB or as an index) passes through.

use coffee_app::ui::theme::{Ansi16, Channel, FILLS, Fill16, HOVER_TOKEN, INKS, hover_band};
use tty_host::{
  ColorDepth, FrameFilter,
  ratatui_core::{
    buffer::{Buffer, Cell},
    style::{Color, Modifier},
  },
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
  /// `hover`: the band under a hovered row or button.
  pub hover: Rgb,
  /// Each text tone's value, in the order of [`INKS`].
  pub inks: [Rgb; INKS.len()],
  /// Each carrier fill's value and carrier, in the order of [`FILLS`].
  pub fills: [(Rgb, Rgb); FILLS.len()],
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
    let mut inks = [[0; 3]; INKS.len()];
    for (slot, spec) in inks.iter_mut().zip(INKS) {
      *slot = color(spec.token)?;
    }
    let mut fills = [([0; 3], [0; 3]); FILLS.len()];
    for (slot, spec) in fills.iter_mut().zip(FILLS) {
      *slot = (color(spec.token)?, color(spec.carrier)?);
    }
    Ok(Self {
      background: color("background")?,
      foreground: color("foreground")?,
      muted_foreground: color("muted-foreground")?,
      card: color("card")?,
      primary: color("primary")?,
      primary_foreground: color("primary-foreground")?,
      success: color("success")?,
      destructive: color("destructive")?,
      hover: color(HOVER_TOKEN)?,
      inks,
      fills,
    })
  }

  /// The palette of `coffee_app::THEME_JSON`.
  pub fn app() -> Result<Self, String> {
    Self::from_theme_json(coffee_app::THEME_JSON)
  }

  /// The value of the token named `name` among the text tones, if it is one.
  pub fn ink(&self, name: &str) -> Option<Rgb> {
    INKS
      .iter()
      .position(|spec| spec.token == name)
      .map(|at| self.inks[at])
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

/// The share of white over black nsh (0.6.1, unchanged in 0.7.1) gives a hovered control without surfaces
/// (`palette::HOVER_SHARE`), and a pressed one (`PRESS_SHARE`).
const HOVER_SHARE: f32 = 0.10;
const PRESS_SHARE: f32 = 0.18;

/// The indigo nsh (0.7) paints its ANSI `Blue` accent in at 24-bit colour, for a dark and a
/// light page (`nsh_platform_tui`'s `palette::BRAND_ACCENT`), and the text it puts on a fill of
/// it (`palette::brand_accent`). No token or carrier of the app is either colour (carriers
/// are xterm cube entries, tokens browns), so each stands for `Blue` alone.
pub const NSH_BRAND_ACCENT: [(Rgb, Rgb); 2] = [
  ([0x8f, 0x9b, 0xff], [0x11, 0x11, 0x1b]),
  ([0x4f, 0x46, 0xe5], [0xff, 0xff, 0xff]),
];

/// Whether `color` is nsh's true-colour brand accent; with the text colour it sets on it.
fn brand_accent(color: Color) -> Option<Color> {
  NSH_BRAND_ACCENT
    .iter()
    .find_map(|&([r, g, b], [fr, fg, fb])| {
      (color == Color::Rgb(r, g, b)).then_some(Color::Rgb(fr, fg, fb))
    })
}

/// The ANSI meaning of a foreground nsh drew: which channel it is.
fn channel(color: Color, dim: bool) -> Option<Channel> {
  Some(match color {
    Color::Reset if dim => Channel::Muted,
    Color::Reset => Channel::Default,
    Color::Yellow | Color::LightYellow => Channel::Yellow,
    Color::Green | Color::LightGreen => Channel::Green,
    Color::Red | Color::LightRed => Channel::Red,
    Color::Cyan | Color::LightCyan => Channel::Cyan,
    Color::Blue | Color::LightBlue => Channel::Blue,
    Color::Magenta | Color::LightMagenta => Channel::Magenta,
    _ => return None,
  })
}

/// The 16-colour terminal colour of a role.
fn ansi16(role: Ansi16) -> Color {
  match role {
    Ansi16::Default => Color::Reset,
    Ansi16::Gray => Color::DarkGray,
    Ansi16::Yellow => Color::Yellow,
    Ansi16::Green => Color::Green,
    Ansi16::Red => Color::Red,
  }
}

/// The theme's colours at one colour depth, ready to replace nsh's meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
  depth: ColorDepth,
  background: Color,
  foreground: Color,
  muted: Color,
  card: Color,
  primary: Color,
  ink: Color,
  success: Color,
  destructive: Color,
  selection: Color,
  /// Each text tone at this depth (in the order of [`INKS`]).
  inks: [Color; INKS.len()],
  /// Each carrier fill as nsh draws it at this depth, and its token (in the order of
  /// [`FILLS`]).
  fills: [(Color, Color); FILLS.len()],
  /// nsh's bandless hover and press bands, and what they become.
  bands: [(Color, Color); 2],
  /// Fills whose text reads in the dark ink rather than the foreground.
  light_fills: [Color; 3],
}

impl Pin {
  /// The pin of `palette` at `depth`. At 16 colours and without colours only the carriers
  /// change (into the 16-colour roles, or attributes): the rest is the terminal's.
  pub fn new(palette: Palette, depth: ColorDepth) -> Self {
    let exact = matches!(depth, ColorDepth::TrueColor | ColorDepth::Ansi256);
    let at = |rgb: Rgb| -> Color {
      match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb[0], rgb[1], rgb[2]),
        ColorDepth::Ansi256 => Color::Indexed(nearest_256(rgb)),
        _ => Color::Reset,
      }
    };
    let band = |share: f32| (at([(255.0 * share).round() as u8; 3]), share);
    let (hover, press) = (band(HOVER_SHARE), band(PRESS_SHARE));
    let mut inks = [Color::Reset; INKS.len()];
    for ((slot, spec), rgb) in inks.iter_mut().zip(INKS).zip(palette.inks) {
      *slot = if exact { at(rgb) } else { ansi16(spec.ansi16) };
    }
    // nsh draws a carrier fill as itself at the depth, and at 16 colours as its hue's accent.
    let mut fills = [(Color::Reset, Color::Reset); FILLS.len()];
    for (slot, (token, carrier)) in fills.iter_mut().zip(palette.fills) {
      let drawn = if exact {
        at(carrier)
      } else {
        hue_band(carrier)
      };
      *slot = (drawn, at(token));
    }
    let selection = FILLS
      .iter()
      .zip(palette.fills)
      .find(|(spec, _)| spec.token == "selection")
      .map_or(palette.card, |(_, (token, _))| token);
    let accent = |rgb: Rgb, sixteen: Color| if exact { at(rgb) } else { sixteen };
    Self {
      depth,
      background: at(palette.background),
      foreground: at(palette.foreground),
      muted: at(palette.muted_foreground),
      card: at(palette.card),
      primary: accent(palette.primary, Color::Yellow),
      ink: at(palette.primary_foreground),
      success: accent(palette.success, Color::Green),
      destructive: accent(palette.destructive, Color::Red),
      selection: at(selection),
      inks,
      fills,
      bands: [
        (hover.0, at(palette.hover)),
        (
          press.0,
          at(mix(palette.background, palette.foreground, press.1)),
        ),
      ],
      light_fills: [
        at(palette.primary),
        at(palette.success),
        at(palette.destructive),
      ],
    }
  }

  /// The theme colour standing for nsh's accent `color` as a fill, if it is one.
  fn accent_fill(&self, color: Color) -> Option<Color> {
    match color {
      Color::Yellow | Color::LightYellow | Color::Blue | Color::LightBlue => Some(self.primary),
      Color::Green | Color::LightGreen => Some(self.success),
      Color::Red | Color::LightRed => Some(self.destructive),
      _ => None,
    }
  }

  /// The tone of `channel`, plain or marked.
  fn ink_of(&self, channel: Channel, marked: bool) -> Option<(usize, Color)> {
    INKS
      .iter()
      .position(|spec| spec.channel == channel && spec.marked == marked)
      .map(|at| (at, self.inks[at]))
  }

  /// Pins one cell's colours.
  pub fn cell(&self, cell: &mut Cell) {
    match self.depth {
      ColorDepth::TrueColor | ColorDepth::Ansi256 => self.exact_cell(cell),
      ColorDepth::Ansi16 => self.sixteen_cell(cell),
      _ => self.mono_cell(cell),
    }
  }

  /// Undoes nsh's true-colour brand accent: its indigo is the `Blue` nsh drew (a fill of
  /// it with the text nsh set on it: a `Blue` fill under the default text, as without it).
  fn unbrand(cell: &mut Cell) {
    if let Some(on) = brand_accent(cell.bg) {
      cell.bg = Color::Blue;
      if cell.fg == on {
        cell.fg = Color::Reset;
      }
    }
    if brand_accent(cell.fg).is_some() {
      cell.fg = Color::Blue;
    }
    if brand_accent(cell.underline_color).is_some() {
      cell.underline_color = Color::Blue;
    }
  }

  fn exact_cell(&self, cell: &mut Cell) {
    if self.depth == ColorDepth::TrueColor {
      Self::unbrand(cell);
    }
    let marked = cell.modifier.contains(Modifier::ITALIC);
    // nsh's focus band (a focused field's row: reversed in its blue accent) reads as a
    // second primary button; the design's focus is the field's heavy amber frame, so the
    // band becomes the cursor's selection band under plain text.
    if !marked
      && matches!(cell.fg, Color::Blue | Color::LightBlue)
      && cell.modifier.contains(Modifier::REVERSED)
    {
      cell.modifier.remove(Modifier::REVERSED);
      cell.fg = self.foreground;
      cell.bg = self.selection;
      return;
    }
    let bg = match cell.bg {
      Color::Reset => self.background,
      Color::DarkGray => self.card,
      // nsh's hover band, while the app hides it (鼠标追踪 off): the ground it covers.
      other if other == self.bands[0].0 && !hover_band() => self.background,
      other => self.accent_fill(other).unwrap_or_else(|| {
        self
          .fills
          .iter()
          .chain(&self.bands)
          .find(|(from, _)| *from == other)
          .map_or(other, |(_, to)| *to)
      }),
    };
    let dim = cell.modifier.contains(Modifier::DIM);
    let fg = match channel(cell.fg, dim) {
      Some(channel) if marked => match self.ink_of(channel, true) {
        Some((_, color)) => {
          cell.modifier.remove(Modifier::ITALIC | Modifier::DIM);
          color
        }
        None => cell.fg,
      },
      Some(Channel::Muted) => {
        cell.modifier.remove(Modifier::DIM);
        self.muted
      }
      // The default text on an accent fill (a primary button's label): nsh leaves it
      // to the terminal's foreground, the design draws it in the dark ink.
      Some(Channel::Default)
        if !cell.modifier.contains(Modifier::REVERSED) && self.light_fills.contains(&bg) =>
      {
        self.ink
      }
      Some(Channel::Default) => self.foreground,
      // nsh's own focus marks: the brand accent.
      Some(Channel::Blue) => self.primary,
      Some(channel) => self
        .ink_of(channel, false)
        .map_or(cell.fg, |(_, color)| color),
      None => cell.fg,
    };
    // 实心标签的右边缘左半格有色。用底色画这半格，宽字符末尾就能沿用标签底色。
    if cell.symbol() == "▌" && self.light_fills.contains(&fg) {
      cell.set_symbol("▐");
      cell.fg = bg;
      cell.bg = fg;
    } else {
      cell.fg = fg;
      cell.bg = bg;
    }
  }

  fn sixteen_cell(&self, cell: &mut Cell) {
    let marked = cell.modifier.contains(Modifier::ITALIC);
    if let Some((_, spec)) = self
      .fills
      .iter()
      .zip(FILLS)
      .find(|((from, _), _)| *from == cell.bg)
    {
      cell.bg = Color::Reset;
      if spec.ansi16 == Fill16::Reverse {
        cell.modifier.insert(Modifier::REVERSED);
      }
    }
    if matches!(cell.bg, Color::Blue | Color::LightBlue) {
      cell.bg = Color::Yellow;
    }
    let dim = cell.modifier.contains(Modifier::DIM);
    let Some(channel) = channel(cell.fg, dim) else {
      return;
    };
    if marked {
      if let Some((at, color)) = self.ink_of(channel, true) {
        cell.modifier.remove(Modifier::ITALIC);
        if INKS[at].channel != Channel::Muted {
          cell.modifier.remove(Modifier::DIM);
          cell.fg = color;
        }
      }
      return;
    }
    match channel {
      // A small box of a carrier fill that nsh drew as a reversed band of the carrier's hue
      // (a key cap): reverse video in the terminal's own colours.
      Channel::Cyan | Channel::Magenta if cell.modifier.contains(Modifier::REVERSED) => {
        cell.fg = Color::Reset;
      }
      Channel::Blue => cell.fg = Color::Yellow,
      Channel::Cyan | Channel::Magenta => {
        if let Some((_, color)) = self.ink_of(channel, false) {
          cell.fg = color;
        }
      }
      _ => {}
    }
  }

  fn mono_cell(&self, cell: &mut Cell) {
    // Colours are gone (nsh strips them); a marked tone still says it is one of the
    // design's faint lines, outlines or disabled texts, which are never bold (nsh makes its
    // blue accent bold in monochrome, and the blue channel carries `line-soft`).
    if cell.modifier.contains(Modifier::ITALIC) {
      cell.modifier.remove(Modifier::ITALIC | Modifier::BOLD);
      cell.modifier.insert(Modifier::DIM);
    }
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

/// The pin is the host's frame filter at every depth: it owns the colours even at 16
/// colours and in monochrome (nsh draws bandless there too, and the pin maps the
/// carriers into roles and attributes), so the terminal's own colours never reach nsh.
impl FrameFilter for Pin {
  fn apply(&mut self, buffer: &mut Buffer) {
    self.buffer(buffer);
  }

  fn owns_colors(&self) -> bool {
    true
  }
}

/// The accent nsh (0.6.1 and 0.7.1) reads a saturated colour as (its `HUE_BANDS`: hue upper bounds in
/// degrees).
pub fn hue_band(rgb: Rgb) -> Color {
  const BANDS: [(f32, Color); 6] = [
    (15.0, Color::Red),
    (75.0, Color::Yellow),
    (165.0, Color::Green),
    (200.0, Color::Cyan),
    (265.0, Color::Blue),
    (335.0, Color::Magenta),
  ];
  let [r, g, b] = rgb.map(f32::from);
  let (max, min) = (r.max(g).max(b), r.min(g).min(b));
  if max <= min {
    return Color::Reset;
  }
  let chroma = max - min;
  let sector = if max == r {
    ((g - b) / chroma).rem_euclid(6.0)
  } else if max == g {
    (b - r) / chroma + 2.0
  } else {
    (r - g) / chroma + 4.0
  };
  let hue = sector * 60.0;
  BANDS
    .iter()
    .find(|(bound, _)| hue < *bound)
    .map_or(Color::Red, |(_, color)| *color)
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
  use coffee_app::ui::theme::Tone;

  use super::*;

  fn palette() -> Palette {
    Palette::app().unwrap()
  }

  fn pin(depth: ColorDepth) -> Pin {
    Pin::new(palette(), depth)
  }

  fn pinned(pin: &Pin, fg: Color, bg: Color, modifier: Modifier) -> Cell {
    let mut cell = Cell::new("x");
    cell.fg = fg;
    cell.bg = bg;
    cell.modifier = modifier;
    pin.cell(&mut cell);
    cell
  }

  fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
  }

  /// What nsh draws a tone's carrier as: its channel's meaning, italic when marked.
  fn carrier(tone: Tone) -> (Color, Modifier) {
    let spec = tone.spec();
    let fg = match spec.channel {
      Channel::Default | Channel::Muted => Color::Reset,
      Channel::Yellow => Color::Yellow,
      Channel::Green => Color::Green,
      Channel::Red => Color::Red,
      Channel::Cyan => Color::Cyan,
      Channel::Blue => Color::Blue,
      Channel::Magenta => Color::Magenta,
    };
    let mut modifier = Modifier::empty();
    if spec.channel == Channel::Muted {
      modifier |= Modifier::DIM;
    }
    if spec.marked {
      modifier |= Modifier::ITALIC;
    }
    (fg, modifier)
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
    assert_eq!(palette.hover, [0x24, 0x20, 0x1c]);
    assert_eq!(palette.ink("line-strong"), Some([0x4a, 0x40, 0x36]));
    assert_eq!(palette.ink("dim"), Some([0x5a, 0x4d, 0x3f]));
    assert_eq!(palette.ink("disabled"), Some([0x7d, 0x72, 0x64]));
    assert_eq!(palette.ink("success-border"), Some([0x3e, 0x5a, 0x48]));
    assert_eq!(palette.ink("destructive-border"), Some([0x6b, 0x44, 0x36]));
    assert_eq!(palette.ink("border"), Some([0x3b, 0x33, 0x2a]));
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
  fn true_color_restores_every_tone_exactly() {
    let pin = pin(ColorDepth::TrueColor);
    for (spec, [r, g, b]) in INKS.iter().zip(palette().inks) {
      if spec.tone == Tone::Ink {
        continue;
      }
      let (fg, modifier) = carrier(spec.tone);
      let cell = pinned(&pin, fg, Color::Reset, modifier | Modifier::BOLD);
      assert_eq!(cell.fg, Color::Rgb(r, g, b), "{:?}", spec.tone);
      assert_eq!(cell.bg, rgb(0x14110e), "{:?}", spec.tone);
      assert_eq!(
        cell.modifier,
        Modifier::BOLD,
        "{:?}: no mark, no DIM",
        spec.tone
      );
    }
  }

  #[test]
  fn carrier_fills_and_bands_become_their_tokens() {
    let pin = pin(ColorDepth::TrueColor);
    let kbd = pinned(&pin, Color::Reset, rgb(0x008787), Modifier::BOLD);
    assert_eq!((kbd.fg, kbd.bg), (rgb(0xece4d6), rgb(0x221d18)));
    let selection = pinned(&pin, Color::Yellow, rgb(0x870087), Modifier::BOLD);
    assert_eq!((selection.fg, selection.bg), (rgb(0xf0b45a), rgb(0x30271a)));
    let card = pinned(&pin, Color::Reset, Color::DarkGray, Modifier::empty());
    assert_eq!((card.fg, card.bg), (rgb(0xece4d6), rgb(0x1b1713)));
    // nsh's bandless hover band is the design's `hover`.
    let hover = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(26, 26, 26),
      Modifier::empty(),
    );
    assert_eq!(hover.bg, rgb(0x24201c));
    // With 鼠标追踪 off the app hides it: the background shows through.
    coffee_app::ui::theme::set_hover_band(false);
    let hidden = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(26, 26, 26),
      Modifier::empty(),
    );
    coffee_app::ui::theme::set_hover_band(true);
    assert_eq!(hidden.bg, rgb(0x14110e));
    let press = pinned(
      &pin,
      Color::Reset,
      Color::Rgb(46, 46, 46),
      Modifier::empty(),
    );
    let [r, g, b] = mix(palette().background, palette().foreground, PRESS_SHARE);
    assert_eq!(press.bg, Color::Rgb(r, g, b));
  }

  #[test]
  fn a_label_on_an_accent_fill_takes_the_dark_ink() {
    let pin = pin(ColorDepth::TrueColor);
    let button = pinned(&pin, Color::Reset, rgb(0xf0b45a), Modifier::empty());
    assert_eq!((button.fg, button.bg), (rgb(0x1a140d), rgb(0xf0b45a)));
    let mint = pinned(&pin, Color::Reset, rgb(0x8cc7a1), Modifier::empty());
    assert_eq!(mint.fg, rgb(0x1a140d));
    // Reversed text keeps the foreground: the terminal swaps the pinned pair.
    let selected = pinned(&pin, Color::Reset, Color::Reset, Modifier::REVERSED);
    assert_eq!(selected.fg, rgb(0xece4d6));
    assert_eq!(selected.modifier, Modifier::REVERSED);
  }

  #[test]
  fn nsh_focus_marks_take_the_brand_accent() {
    let pin = pin(ColorDepth::TrueColor);
    let mark = pinned(&pin, Color::Blue, Color::Reset, Modifier::BOLD);
    assert_eq!(mark.fg, rgb(0xf0b45a));
    // A focused field's reversed blue band becomes plain text on the selection band.
    let band = pinned(
      &pin,
      Color::Blue,
      Color::Reset,
      Modifier::REVERSED | Modifier::UNDERLINED,
    );
    assert_eq!((band.fg, band.bg), (rgb(0xece4d6), rgb(0x30271a)));
    assert_eq!(band.modifier, Modifier::UNDERLINED);
  }

  #[test]
  fn nsh_brand_indigo_is_read_as_its_blue() {
    let pin = pin(ColorDepth::TrueColor);
    let (indigo, on) = (rgb(0x8f9bff), rgb(0x11111b));
    // `line-soft` rides on marked blue: nsh 0.7 paints it indigo at 24-bit colour.
    let line = pinned(&pin, indigo, Color::Reset, Modifier::ITALIC);
    assert_eq!(line.fg, exact(palette().ink("line-soft").unwrap()));
    let mark = pinned(&pin, indigo, Color::Reset, Modifier::BOLD);
    assert_eq!(mark.fg, rgb(0xf0b45a));
    let band = pinned(&pin, indigo, Color::Reset, Modifier::REVERSED);
    assert_eq!((band.fg, band.bg), (rgb(0xece4d6), rgb(0x30271a)));
    // An accent fill and nsh's text on it: the primary fill under the dark ink.
    let chip = pinned(&pin, on, indigo, Modifier::empty());
    assert_eq!((chip.fg, chip.bg), (rgb(0x1a140d), rgb(0xf0b45a)));
    // At 256 colours nsh keeps ANSI blue: the same colour is just a colour there.
    let other = pinned(&pin_256(), indigo, Color::Reset, Modifier::empty());
    assert_eq!(other.fg, indigo);
  }

  fn exact([r, g, b]: Rgb) -> Color {
    Color::Rgb(r, g, b)
  }

  fn pin_256() -> Pin {
    pin(ColorDepth::Ansi256)
  }

  #[test]
  fn other_colors_pass_through() {
    let pin = pin(ColorDepth::TrueColor);
    for color in [Color::Rgb(1, 2, 3), Color::Indexed(42), Color::Gray] {
      let cell = pinned(&pin, color, color, Modifier::empty());
      assert_eq!((cell.fg, cell.bg), (color, color));
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
    let pin = pin(ColorDepth::Ansi256);
    let amber = pinned(&pin, Color::Yellow, Color::Reset, Modifier::empty());
    // #f0b45a → (255, 175, 95), xterm 215; #14110e → (18, 18, 18), xterm 233.
    assert_eq!(
      (amber.fg, amber.bg),
      (Color::Indexed(215), Color::Indexed(233))
    );
    assert_eq!(xterm_rgb(215), [255, 175, 95]);
    for (spec, value) in INKS.iter().zip(palette().inks) {
      if spec.tone == Tone::Ink {
        continue;
      }
      let (fg, modifier) = carrier(spec.tone);
      let cell = pinned(&pin, fg, Color::Reset, modifier);
      assert_eq!(
        cell.fg,
        Color::Indexed(nearest_256(value)),
        "{:?}",
        spec.tone
      );
      assert!(
        matches!(cell.fg, Color::Indexed(16..=255)),
        "{:?}",
        spec.tone
      );
    }
    // A carrier fill: nsh draws the exact cube entry of the carrier.
    let carrier_kbd = Color::Indexed(nearest_256([0, 135, 135]));
    let kbd = pinned(&pin, Color::Reset, carrier_kbd, Modifier::empty());
    assert_eq!(kbd.bg, Color::Indexed(nearest_256([0x22, 0x1d, 0x18])));
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
  fn sixteen_colors_map_the_carriers_to_their_roles() {
    let pin = pin(ColorDepth::Ansi16);
    for spec in INKS {
      if matches!(spec.tone, Tone::Ink | Tone::Mute | Tone::Placeholder) {
        continue;
      }
      let (fg, modifier) = carrier(spec.tone);
      let cell = pinned(&pin, fg, Color::Reset, modifier);
      assert_eq!(cell.fg, ansi16(spec.ansi16), "{:?}", spec.tone);
      assert!(!cell.modifier.contains(Modifier::ITALIC), "{:?}", spec.tone);
    }
    // Muted tones keep nsh's DIM; the placeholder loses only its mark.
    let placeholder = pinned(
      &pin,
      Color::Reset,
      Color::Reset,
      Modifier::DIM | Modifier::ITALIC,
    );
    assert_eq!(
      (placeholder.fg, placeholder.modifier),
      (Color::Reset, Modifier::DIM)
    );
    // A key cap nsh drew as a reversed band of the carrier's hue: plain reverse video.
    let cap = pinned(&pin, Color::Cyan, Color::Reset, Modifier::REVERSED);
    assert_eq!((cap.fg, cap.modifier), (Color::Reset, Modifier::REVERSED));
    // The selection band behind hued text: no fill.
    let row = pinned(&pin, Color::Yellow, Color::Magenta, Modifier::BOLD);
    assert_eq!((row.fg, row.bg), (Color::Yellow, Color::Reset));
    // A key cap's fill behind hued text: reverse video.
    let kbd = pinned(&pin, Color::Yellow, Color::Cyan, Modifier::empty());
    assert_eq!(kbd.bg, Color::Reset);
    assert!(kbd.modifier.contains(Modifier::REVERSED));
    // nsh's focus blue is the amber.
    let focus = pinned(&pin, Color::Blue, Color::Reset, Modifier::empty());
    assert_eq!(focus.fg, Color::Yellow);
    // The standard meanings stay the terminal's.
    let amber = pinned(&pin, Color::Yellow, Color::Reset, Modifier::empty());
    assert_eq!((amber.fg, amber.bg), (Color::Yellow, Color::Reset));
  }

  #[test]
  fn monochrome_turns_marks_into_dim() {
    let pin = pin(ColorDepth::Monochrome);
    let line = pinned(
      &pin,
      Color::Reset,
      Color::Reset,
      Modifier::ITALIC | Modifier::CROSSED_OUT,
    );
    assert_eq!(line.modifier, Modifier::DIM | Modifier::CROSSED_OUT);
    let bold = pinned(
      &pin,
      Color::Reset,
      Color::Reset,
      Modifier::ITALIC | Modifier::BOLD,
    );
    assert_eq!(bold.modifier, Modifier::DIM, "a marked tone is never bold");
    let plain = pinned(&pin, Color::Reset, Color::Reset, Modifier::REVERSED);
    assert_eq!(plain.modifier, Modifier::REVERSED);
  }

  #[test]
  fn hue_bands_follow_nsh() {
    assert_eq!(hue_band([0, 135, 135]), Color::Cyan);
    assert_eq!(hue_band([135, 0, 135]), Color::Magenta);
    assert_eq!(hue_band([0xf0, 0xb4, 0x5a]), Color::Yellow);
    assert_eq!(hue_band([0xf0, 0x8a, 0x6c]), Color::Red);
    assert_eq!(hue_band([0x8c, 0xc7, 0xa1]), Color::Green);
    assert_eq!(hue_band([0, 0x87, 0xff]), Color::Blue);
    assert_eq!(hue_band([9, 9, 9]), Color::Reset);
  }

  #[test]
  fn a_whole_buffer_is_pinned() {
    use tty_host::ratatui_core::layout::Rect;
    let pin = pin(ColorDepth::TrueColor);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
    pin.buffer(&mut buffer);
    assert!(buffer.content.iter().all(|cell| cell.bg == rgb(0x14110e)));
  }
}
