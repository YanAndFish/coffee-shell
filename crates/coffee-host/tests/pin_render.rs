//! The palette pin over what nsh 0.6.1 really draws: a probe component in the app's
//! theme, rendered headless at each colour depth, then pinned as `coffee-tui` pins it.
//!
//! If an nsh upgrade changes how it maps colours, this is the test that says so.

use std::time::{Duration, Instant};

use coffee_host::{
  capabilities,
  palette::{Palette, Pin},
  terminal::Replies,
};
use nsh_base::theme;
use nsh_core::prelude::*;
use nsh_platform_tui::{AppOptions, AppView, Appearance, ColorDepth, Root, RunEvent, run_app};
use ratatui_core::{
  backend::TestBackend,
  buffer::Buffer,
  style::{Color, Modifier},
  terminal::Terminal,
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

fn render(depth: ColorDepth) -> Buffer {
  let theme = nsh_base::Theme::from_json(coffee_app::THEME_JSON).unwrap();
  let root = Root::of::<Swatches>()
    .tokens(theme.into_tokens())
    .appearance(Appearance::Dark);
  let pin = Pin::new(Palette::app().unwrap(), depth);
  let mut options = AppOptions::default();
  options.capabilities = capabilities(depth, pin.is_some(), &Replies::default());
  options.ui_sets = vec![nsh_base::registry()];
  let mut view = AppView::new(options);
  let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
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
  if let Some(pin) = pin {
    pin.buffer(&mut buffer);
  }
  buffer
}

/// The first cell of the row that shows `label`.
fn row<'a>(buffer: &'a Buffer, label: &str) -> &'a ratatui_core::buffer::Cell {
  let y = ROWS.iter().position(|row| *row == label).unwrap() as u16;
  let line: String = (0..label.len() as u16)
    .map(|x| buffer[(x, y)].symbol().to_owned())
    .collect();
  assert_eq!(line, label, "row {y}");
  &buffer[(0, y)]
}

fn rgb(hex: u32) -> Color {
  Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
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
