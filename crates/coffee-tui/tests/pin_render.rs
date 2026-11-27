//! The palette pin over what nsh (0.7.1) really draws: probe components in the app's theme,
//! rendered headless at each colour depth, then pinned as `coffee-tui` pins it.
//!
//! [`Swatches`] draws the theme's own colours the way nsh sees them; [`Tokens`] draws every
//! design token through the `ui` layer's carriers (`coffee_app::ui::theme`), so these tests
//! say whether each token survives nsh and the pin: exactly at 24-bit colour, as its nearest
//! xterm entry at 256 colours, as its 16-colour role, and as an attribute in monochrome. If an
//! nsh upgrade changes how it maps colours, these are the tests that say so.

use std::time::{Duration, Instant};

use coffee_app::ui::{
  self,
  theme::{Ansi16, INKS, Tone},
};
use coffee_tui::palette::{Palette, Pin, nearest_256};
use nsh_base::theme;
use nsh_core::prelude::*;
use tty_host::{
  FrameFilter, capabilities,
  nsh_platform_tui::{AppOptions, AppView, Appearance, ColorDepth, Root, RunEvent, run_app},
  ratatui_core::{
    backend::TestBackend,
    buffer::{Buffer, Cell},
    style::{Color, Modifier},
    terminal::Terminal,
  },
  terminal::Replies,
};

/// One row per token: text in it, or a fill of it with a label.
#[component]
fn Swatches() -> impl IntoElement {
  view()
    .flex_1()
    .bg(move || theme().background)
    .text_color(move || theme().foreground)
    .child(text("FOREGROUND"))
    .child(text("MUTED").text_color(move || theme().muted_foreground))
    .child(text("PRIMARY").text_color(move || theme().primary))
    .child(text("SUCCESS").text_color(move || theme().color("success").unwrap()))
    .child(text("DESTRUCTIVE").text_color(move || theme().destructive))
    .child(
      view()
        .bg(move || theme().primary)
        .child(text("BUTTON").text_color(move || theme().primary_foreground)),
    )
    .child(view().bg(move || theme().card).child(text("CARD")))
}

const ROWS: [&str; 7] = [
  "FOREGROUND",
  "MUTED",
  "PRIMARY",
  "SUCCESS",
  "DESTRUCTIVE",
  "BUTTON",
  "CARD",
];

/// The tones drawn by [`Tokens`], one row each, named by their token.
const TONES: [Tone; 15] = [
  Tone::Fg,
  Tone::Soft,
  Tone::Mute,
  Tone::Placeholder,
  Tone::Disabled,
  Tone::Dim,
  Tone::Line,
  Tone::LineStrong,
  Tone::LineSoft,
  Tone::LineRow,
  Tone::Acc,
  Tone::Acc2,
  Tone::Acc2Line,
  Tone::Danger,
  Tone::DangerLine,
];

/// Every token through the `ui` layer: a row per tone (`label` applies the carrier and its
/// mark), then a key cap, the cursor band (`▶` in amber on it, as a data row draws it) and a
/// primary pill.
#[component]
fn Tokens() -> impl IntoElement {
  view()
    .flex_1()
    .bg(ui::theme::bg)
    .text_color(ui::theme::fg)
    .children(
      TONES
        .iter()
        .map(|&tone| ui::label(tone.spec().token.to_uppercase(), tone)),
    )
    .child(view().flex_row().child(ui::kbd("KBD")))
    .child(
      view()
        .flex_row()
        .bg(ui::Fill::Selection.color())
        .child(ui::strong("▶", Tone::Acc))
        .child(ui::label(" SELECTION", Tone::Fg)),
    )
    .child(
      view()
        .flex_row()
        .child(ui::pill_text(ui::Fill::Acc, "PILL", true)),
    )
}

fn render_root(root: Root, depth: ColorDepth, cols: u16, rows: u16) -> Buffer {
  let mut pin = Pin::new(Palette::app().unwrap(), depth);
  let mut options = AppOptions::default();
  options.capabilities = capabilities(depth, pin.owns_colors(), &Replies::default());
  options.ui_sets = vec![nsh_base::registry(), coffee_app::registry()];
  let mut view = AppView::new(options);
  let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
  terminal.draw(|f| view.render(f, f.area())).unwrap();
  view.load(root).unwrap();
  let end = Instant::now() + Duration::from_millis(600);
  run_app(&mut terminal, &mut view, |wait| {
    std::thread::sleep(wait);
    Ok(if Instant::now() > end {
      RunEvent::Exit
    } else {
      RunEvent::Redraw
    })
  })
  .unwrap();
  let mut buffer = terminal.backend().buffer().clone();
  pin.apply(&mut buffer);
  buffer
}

fn themed<C: Component>() -> Root
where
  C::Props: Default,
{
  let theme = nsh_base::Theme::from_json(coffee_app::THEME_JSON).unwrap();
  Root::of::<C>()
    .tokens(theme.into_tokens())
    .appearance(Appearance::Dark)
}

fn render(depth: ColorDepth) -> Buffer {
  render_root(themed::<Swatches>(), depth, 24, 8)
}

fn render_tokens(depth: ColorDepth) -> Buffer {
  render_root(themed::<Tokens>(), depth, 32, 20)
}

/// The first cell of the row that shows `label`.
fn row<'a>(buffer: &'a Buffer, label: &str) -> &'a Cell {
  let y = ROWS.iter().position(|row| *row == label).unwrap() as u16;
  let line: String = (0..label.len() as u16)
    .map(|x| buffer[(x, y)].symbol().to_owned())
    .collect();
  assert_eq!(line, label, "row {y}");
  &buffer[(0, y)]
}

/// The first cell showing `text` (searched row by row).
fn find<'a>(buffer: &'a Buffer, text: &str) -> &'a Cell {
  let area = buffer.area;
  for y in 0..area.height {
    let line: String = (0..area.width)
      .map(|x| buffer[(x, y)].symbol().to_owned())
      .collect();
    if let Some(at) = line.find(text) {
      let x = line[..at].chars().count() as u16;
      return &buffer[(x, y)];
    }
  }
  panic!("{text:?} is not on screen");
}

/// The cell of a tone's row.
fn tone_cell(buffer: &Buffer, tone: Tone) -> &Cell {
  find(buffer, &tone.spec().token.to_uppercase())
}

fn rgb(hex: u32) -> Color {
  Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn exact(rgb: [u8; 3]) -> Color {
  Color::Rgb(rgb[0], rgb[1], rgb[2])
}

#[test]
fn true_color_shows_the_design_tokens_exactly() {
  let buffer = render(ColorDepth::TrueColor);
  let background = rgb(0x14110e);
  for (label, fg) in [
    ("FOREGROUND", 0xece4d6),
    ("MUTED", 0xa39684),
    ("PRIMARY", 0xf0b45a),
    ("SUCCESS", 0x8cc7a1),
    ("DESTRUCTIVE", 0xf08a6c),
  ] {
    let cell = row(&buffer, label);
    assert_eq!((cell.fg, cell.bg), (rgb(fg), background), "{label}");
    assert!(!cell.modifier.contains(Modifier::DIM), "{label}");
  }
  let button = row(&buffer, "BUTTON");
  assert_eq!((button.fg, button.bg), (rgb(0x1a140d), rgb(0xf0b45a)));
  let card = row(&buffer, "CARD");
  assert_eq!((card.fg, card.bg), (rgb(0xece4d6), rgb(0x1b1713)));
}

#[test]
fn ansi256_shows_the_nearest_entries() {
  let buffer = render(ColorDepth::Ansi256);
  for label in ROWS {
    let cell = row(&buffer, label);
    for color in [cell.fg, cell.bg] {
      assert!(
        matches!(color, Color::Indexed(16..=255)),
        "{label}: {color:?} is a themeable colour"
      );
    }
  }
  assert_eq!(row(&buffer, "PRIMARY").fg, Color::Indexed(215));
  assert_eq!(row(&buffer, "SUCCESS").fg, Color::Indexed(115));
}

#[test]
fn sixteen_colors_and_mono_are_left_to_the_terminal() {
  let ansi = render(ColorDepth::Ansi16);
  assert_eq!(row(&ansi, "PRIMARY").fg, Color::Yellow);
  assert_eq!(row(&ansi, "FOREGROUND").bg, Color::Reset);
  let mono = render(ColorDepth::Monochrome);
  for label in ROWS {
    let cell = row(&mono, label);
    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "{label}");
  }
  assert!(row(&mono, "MUTED").modifier.contains(Modifier::DIM));
}

#[test]
fn every_ui_token_is_exact_in_true_color() {
  let buffer = render_tokens(ColorDepth::TrueColor);
  let palette = Palette::app().unwrap();
  let background = rgb(0x14110e);
  for tone in TONES {
    let value = palette.ink(tone.spec().token).unwrap();
    let cell = tone_cell(&buffer, tone);
    assert_eq!((cell.fg, cell.bg), (exact(value), background), "{tone:?}");
    assert!(
      !cell.modifier.intersects(Modifier::DIM | Modifier::ITALIC),
      "{tone:?}: {:?}",
      cell.modifier
    );
  }
  // The key cap: `kbd` under bold `fg`.
  let kbd = find(&buffer, "KBD");
  assert_eq!((kbd.fg, kbd.bg), (rgb(0xece4d6), rgb(0x221d18)));
  assert!(kbd.modifier.contains(Modifier::BOLD));
  // The cursor band: `selection` under the amber `▶`.
  let cursor = find(&buffer, "▶");
  assert_eq!((cursor.fg, cursor.bg), (rgb(0xf0b45a), rgb(0x30271a)));
  assert_eq!(find(&buffer, "SELECTION").bg, rgb(0x30271a));
  // The pill: `ink` on `acc`, its ends amber on the window.
  let pill = find(&buffer, "PILL");
  assert_eq!((pill.fg, pill.bg), (rgb(0x1a140d), rgb(0xf0b45a)));
  assert_eq!(find(&buffer, "▐").fg, rgb(0xf0b45a));
}

#[test]
fn every_ui_token_is_its_nearest_xterm_entry_at_256_colors() {
  let buffer = render_tokens(ColorDepth::Ansi256);
  let palette = Palette::app().unwrap();
  for tone in TONES {
    let value = palette.ink(tone.spec().token).unwrap();
    let cell = tone_cell(&buffer, tone);
    assert_eq!(cell.fg, Color::Indexed(nearest_256(value)), "{tone:?}");
    assert_eq!(cell.bg, Color::Indexed(233), "{tone:?}");
    assert!(!cell.modifier.contains(Modifier::ITALIC), "{tone:?}");
  }
  let kbd = find(&buffer, "KBD");
  assert_eq!(kbd.bg, Color::Indexed(nearest_256([0x22, 0x1d, 0x18])));
  let cursor = find(&buffer, "▶");
  assert_eq!(
    (cursor.fg, cursor.bg),
    (
      Color::Indexed(215),
      Color::Indexed(nearest_256([0x30, 0x27, 0x1a]))
    )
  );
}

#[test]
fn every_ui_token_takes_its_role_at_16_colors() {
  let buffer = render_tokens(ColorDepth::Ansi16);
  for tone in TONES {
    let spec = tone.spec();
    let cell = tone_cell(&buffer, tone);
    assert!(!cell.modifier.contains(Modifier::ITALIC), "{tone:?}");
    // The muted tones are nsh's own dim default text.
    if matches!(tone, Tone::Mute | Tone::Placeholder) {
      assert_eq!(cell.fg, Color::Reset, "{tone:?}");
      assert!(cell.modifier.contains(Modifier::DIM), "{tone:?}");
      continue;
    }
    let role = match spec.ansi16 {
      Ansi16::Default => Color::Reset,
      Ansi16::Gray => Color::DarkGray,
      Ansi16::Yellow => Color::Yellow,
      Ansi16::Green => Color::Green,
      Ansi16::Red => Color::Red,
    };
    assert_eq!(cell.fg, role, "{tone:?}");
  }
  // The key cap is reverse video in the terminal's own colours.
  let kbd = find(&buffer, "KBD");
  assert_eq!((kbd.fg, kbd.bg), (Color::Reset, Color::Reset));
  assert!(kbd.modifier.contains(Modifier::REVERSED));
  // The cursor band is no fill: the amber `▶` carries it.
  let cursor = find(&buffer, "▶");
  assert_eq!((cursor.fg, cursor.bg), (Color::Yellow, Color::Reset));
  // The pill is a reversed yellow band.
  let pill = find(&buffer, "PILL");
  assert_eq!(pill.fg, Color::Yellow);
  assert!(pill.modifier.contains(Modifier::REVERSED));
}

#[test]
fn every_ui_token_keeps_its_attribute_in_monochrome() {
  let buffer = render_tokens(ColorDepth::Monochrome);
  for tone in TONES {
    let spec = tone.spec();
    let cell = tone_cell(&buffer, tone);
    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "{tone:?}");
    assert!(!cell.modifier.contains(Modifier::ITALIC), "{tone:?}");
    let dim = spec.marked || tone == Tone::Mute;
    assert_eq!(cell.modifier.contains(Modifier::DIM), dim, "{tone:?}");
  }
  // Fills are reverse video: the key cap, the cursor band, the pill.
  for text in ["KBD", "▶", "SELECTION", "PILL"] {
    let cell = find(&buffer, text);
    assert!(cell.modifier.contains(Modifier::REVERSED), "{text}");
  }
  // Every marked tone is one the design dims in monochrome, or an outline.
  for spec in INKS {
    if spec.marked {
      assert!(
        spec.mono_dim || matches!(spec.tone, Tone::Acc2Line | Tone::DangerLine),
        "{:?}",
        spec.tone
      );
    }
  }
}

/// 在同一行渲染详情页的标签和选项，检查中文末尾的背景色。
#[component]
fn DetailPills() -> impl IntoElement {
  view()
    .flex_row()
    .child(ui::tag(ui::TagKind::Solid, "新产季"))
    .child(ui::chip(
      "手冲",
      ui::ChipPad::Tight,
      || true,
      || true,
      || {},
    ))
}

#[test]
fn filled_labels_cover_the_final_chinese_character() {
  for depth in [ColorDepth::TrueColor, ColorDepth::Ansi256] {
    let buffer = render_root(themed::<DetailPills>(), depth, 40, 2);
    for (x, last) in [(5, "季"), (11, "冲")] {
      let glyph = &buffer[(x, 0)];
      let cap = &buffer[(x + 2, 0)];
      assert_eq!(glyph.symbol(), last);
      assert_eq!(cap.bg, glyph.bg, "{last} 的右边缘应延续文字底色");
      assert_eq!(cap.fg, buffer[(14, 0)].bg);
      assert_eq!(cap.symbol(), "▐");
    }
  }
}
