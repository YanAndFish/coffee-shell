//! Radar: the six-axis flavour radar in braille dots (`components/Radar`).
//!
//! ```text
//!           酸度
//!           ⣠⠴⠚⠦⣄
//!       ⢀⡤⠖⠉⢀⡠⣤⢄⡀⠉⠲⢤⡀
//! 香气 ⡎⢡⡤⠒⠋⠁   ⠈⠙⠒⠤⣌⢱ 甜感
//!      ⡇⢸⠁         ⢰⠋⢸
//!      …
//!           苦度
//! ```
//!
//! The block is 25×11 cells; its middle 15×9 cells are a braille canvas (2×4 dots a cell),
//! 30×36 dots, centred on dot (14.5, 17.5) with an outer radius of 17 dots. Axes run
//! clockwise from the top: 酸度, 甜感, 醇厚, 苦度, 余韵, 香气 (axis i at i×60° − 90°). Only the
//! outer hexagon (`line-strong`) and the data polygon (`acc` for a bean, `acc2` for "my
//! taste") are drawn, each vertex a 2×2 dot block; a cell holding both takes the data's
//! colour. Below 32 rows or in a column under 25 cells the radar falls back to six
//! [`super::meter::meter`] rows (bar length 10).

use nsh_core::prelude::*;

use super::{label::label, meter::meter, text::center, theme::Tone};

/// The axes, clockwise from the top.
pub const AXES: [&str; 6] = ["酸度", "甜感", "醇厚", "苦度", "余韵", "香气"];

/// The block's size in cells.
pub const RADAR_COLS: usize = 25;
/// The block's height in cells.
pub const RADAR_ROWS: usize = 11;
/// The canvas's size in cells.
const CANVAS_COLS: usize = 15;
const CANVAS_ROWS: usize = 9;
/// The canvas's centre and outer radius, in dots.
const CENTRE: (f32, f32) = (14.5, 17.5);
const RADIUS: f32 = 17.0;

/// What a dot of the canvas belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ink {
  /// The outer hexagon.
  Rim,
  /// The data polygon.
  Data,
}

/// A braille canvas of `cols` × `rows` cells (dots: twice as wide, four times as tall).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Braille {
  cols: usize,
  rows: usize,
  dots: Vec<Option<Ink>>,
}

/// The bit of dot (`dx`, `dy`) of a braille cell (Unicode's dot numbering).
fn bit(dx: usize, dy: usize) -> u32 {
  match (dx, dy) {
    (0, 0) => 0x01,
    (0, 1) => 0x02,
    (0, 2) => 0x04,
    (1, 0) => 0x08,
    (1, 1) => 0x10,
    (1, 2) => 0x20,
    (0, 3) => 0x40,
    _ => 0x80,
  }
}

impl Braille {
  /// An empty canvas.
  pub fn new(cols: usize, rows: usize) -> Self {
    Self {
      cols,
      rows,
      dots: vec![None; cols * 2 * rows * 4],
    }
  }

  fn width(&self) -> usize {
    self.cols * 2
  }

  fn height(&self) -> usize {
    self.rows * 4
  }

  /// Sets the dot nearest to (`x`, `y`) (dots) to `ink`; data wins over the rim.
  pub fn set(&mut self, x: f32, y: f32, ink: Ink) {
    let (x, y) = (x.round(), y.round());
    if x < 0.0 || y < 0.0 || x >= self.width() as f32 || y >= self.height() as f32 {
      return;
    }
    let at = y as usize * self.width() + x as usize;
    if self.dots[at] != Some(Ink::Data) {
      self.dots[at] = Some(ink);
    }
  }

  /// Whether the dot at (`x`, `y`) is set.
  pub fn get(&self, x: usize, y: usize) -> Option<Ink> {
    self.dots.get(y * self.width() + x).copied().flatten()
  }

  /// A line from `a` to `b` (dots), one dot per step along its longer axis.
  pub fn line(&mut self, a: (f32, f32), b: (f32, f32), ink: Ink) {
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0) as usize;
    for step in 0..=steps {
      let t = step as f32 / steps as f32;
      self.set(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, ink);
    }
  }

  /// A closed polygon through `points`, each vertex a 2×2 block.
  pub fn polygon(&mut self, points: &[(f32, f32)], ink: Ink) {
    for (i, &a) in points.iter().enumerate() {
      let b = points[(i + 1) % points.len()];
      self.line(a, b, ink);
    }
    for &(x, y) in points {
      // The two dots on each side of the vertex, across and down.
      let (left, top) = ((x - 0.5).floor().max(0.0), (y - 0.5).floor().max(0.0));
      for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        self.set(left + dx, top + dy, ink);
      }
    }
  }

  /// Each cell as its braille character and the ink it shows (data over rim); `None` for a
  /// blank cell.
  pub fn cells(&self) -> Vec<Vec<Option<(char, Ink)>>> {
    (0..self.rows)
      .map(|row| {
        (0..self.cols)
          .map(|col| {
            let mut bits = 0;
            let mut ink = None;
            for dy in 0..4 {
              for dx in 0..2 {
                if let Some(dot) = self.get(col * 2 + dx, row * 4 + dy) {
                  bits |= bit(dx, dy);
                  if ink != Some(Ink::Data) {
                    ink = Some(dot);
                  }
                }
              }
            }
            ink.map(|ink| (char::from_u32(0x2800 + bits).unwrap_or(' '), ink))
          })
          .collect()
      })
      .collect()
  }
}

/// Vertex `i` of a hexagon of `radius` dots around the canvas centre.
pub fn vertex(i: usize, radius: f32) -> (f32, f32) {
  let angle = (i as f32 * 60.0 - 90.0).to_radians();
  (
    CENTRE.0 + radius * angle.cos(),
    CENTRE.1 + radius * angle.sin(),
  )
}

/// The canvas of `values` (0–10 each, clamped): the rim and the data polygon.
pub fn radar_canvas(values: [f32; 6]) -> Braille {
  let mut canvas = Braille::new(CANVAS_COLS, CANVAS_ROWS);
  let rim: Vec<(f32, f32)> = (0..6).map(|i| vertex(i, RADIUS)).collect();
  canvas.polygon(&rim, Ink::Rim);
  let data: Vec<(f32, f32)> = (0..6)
    .map(|i| vertex(i, RADIUS * values[i].clamp(0.0, 10.0) / 10.0))
    .collect();
  canvas.polygon(&data, Ink::Data);
  canvas
}

/// A run of one row: its text and its tone (`None`: blank or a label).
pub type Run = (String, Option<Tone>);

/// The 11 rows of the radar block, as runs: axis labels (`mute`), the rim (`line-strong`)
/// and the data (`data`), each row exactly [`RADAR_COLS`] cells.
pub fn radar_rows(values: [f32; 6], data: Tone) -> Vec<Vec<Run>> {
  let canvas = radar_canvas(values).cells();
  let mut rows = Vec::with_capacity(RADAR_ROWS);
  rows.push(vec![(center(AXES[0], RADAR_COLS), Some(Tone::Mute))]);
  for (index, cells) in canvas.iter().enumerate() {
    let row = index + 1;
    let (left, right) = match row {
      3 => (format!("{} ", AXES[5]), format!(" {}", AXES[1])),
      7 => (format!("{} ", AXES[4]), format!(" {}", AXES[2])),
      _ => ("     ".to_owned(), "     ".to_owned()),
    };
    let mut runs: Vec<Run> = vec![(left, Some(Tone::Mute))];
    for cell in cells {
      let (glyph, tone) = match cell {
        Some((glyph, Ink::Rim)) => (*glyph, Some(Tone::LineStrong)),
        Some((glyph, Ink::Data)) => (*glyph, Some(data)),
        None => (' ', None),
      };
      match runs.last_mut() {
        Some((text, last)) if *last == tone => text.push(glyph),
        _ => runs.push((glyph.to_string(), tone)),
      }
    }
    runs.push((right, Some(Tone::Mute)));
    rows.push(runs);
  }
  rows.push(vec![(center(AXES[3], RADAR_COLS), Some(Tone::Mute))]);
  rows
}

/// Whether a radar fits: at least 32 rows tall and a column of at least 25 cells.
pub fn radar_fits(short: bool, column_cells: usize) -> bool {
  !short && column_cells >= RADAR_COLS
}

/// The radar of `values()` (tracked) in `data` (`Acc`: the bean, `Acc2`: my taste), or the six
/// meter rows when `fits()` says it does not fit. `label_text` is its screen-reader label.
pub fn radar(
  values: impl Fn() -> [f32; 6] + Copy + 'static,
  data: Tone,
  fits: impl Fn() -> bool + Copy + 'static,
  label_text: impl Into<SharedString>,
) -> AnyElement {
  let label_text: SharedString = label_text.into();
  dynamic(move || {
    if fits() {
      let rows = radar_rows(values(), data);
      view()
        .flex_none()
        .label(label_text.clone())
        .children(rows.into_iter().map(|runs| {
          text("").children(runs.into_iter().map(|(run, tone)| match tone {
            Some(tone) => label(run, tone),
            None => text(run),
          }))
        }))
        .into_element()
    } else {
      view()
        .flex_none()
        .label(label_text.clone())
        .children((0..6).map(move |i| meter(AXES[i], move || values()[i], 10, data)))
        .into_element()
    }
  })
}

/// The screen-reader label "{name} 风味雷达：酸度 8，甜感 7，…".
pub fn radar_label(name: &str, values: [f32; 6]) -> String {
  let parts: Vec<String> = AXES
    .iter()
    .zip(values)
    .map(|(axis, value)| format!("{axis} {}", value.round()))
    .collect();
  format!("{name} 风味雷达：{}", parts.join("，"))
}

/// Keeps every row exactly as wide as the block.
pub fn row_width(runs: &[Run]) -> usize {
  runs
    .iter()
    .map(|(text, _)| super::text::display_width(text))
    .sum()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_block_is_25_by_11() {
    let rows = radar_rows([8.0, 7.0, 4.0, 2.0, 7.0, 8.0], Tone::Acc2);
    assert_eq!(rows.len(), RADAR_ROWS);
    for runs in &rows {
      assert_eq!(row_width(runs), RADAR_COLS, "{runs:?}");
    }
    let top: String = rows[0].iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(top.trim(), "酸度");
    let side: String = rows[3].iter().map(|(t, _)| t.as_str()).collect();
    assert!(
      side.starts_with("香气 ") && side.ends_with(" 甜感"),
      "{side}"
    );
    let low: String = rows[7].iter().map(|(t, _)| t.as_str()).collect();
    assert!(low.starts_with("余韵 ") && low.ends_with(" 醇厚"), "{low}");
  }

  #[test]
  fn vertices_follow_the_axes() {
    let top = vertex(0, RADIUS);
    assert!((top.0 - 14.5).abs() < 1e-4 && (top.1 - 0.5).abs() < 1e-4);
    let bottom = vertex(3, RADIUS);
    assert!((bottom.1 - 34.5).abs() < 1e-4);
    let right = vertex(1, RADIUS);
    assert!(right.0 > 28.0 && right.1 < CENTRE.1);
    // Every rim vertex lands on a dot of the canvas (the side ones round to its edges).
    for i in 0..6 {
      let (x, y) = vertex(i, RADIUS);
      let (x, y) = (x.round(), y.round());
      assert!(
        (0.0..30.0).contains(&x.abs()) && (0.0..36.0).contains(&y),
        "{i}"
      );
    }
  }

  #[test]
  fn data_wins_a_shared_cell_and_values_clamp() {
    let full = radar_canvas([10.0; 6]);
    // The data polygon at 10 is the rim: every rim cell shows data.
    assert!(
      full
        .cells()
        .iter()
        .flatten()
        .flatten()
        .all(|(_, ink)| *ink == Ink::Data)
    );
    assert_eq!(radar_canvas([15.0; 6]), full);
    let empty = radar_canvas([0.0; 6]);
    let data = empty
      .cells()
      .iter()
      .flatten()
      .flatten()
      .filter(|(_, ink)| *ink == Ink::Data)
      .count();
    assert!((1..=2).contains(&data), "a dot in the middle: {data}");
  }

  #[test]
  fn braille_bits_are_unicodes() {
    let mut canvas = Braille::new(1, 1);
    canvas.set(0.0, 0.0, Ink::Rim);
    canvas.set(1.0, 3.0, Ink::Rim);
    assert_eq!(canvas.cells()[0][0], Some(('⢁', Ink::Rim)));
  }

  #[test]
  fn the_fallback_and_label() {
    assert!(radar_fits(false, 25));
    assert!(!radar_fits(true, 60));
    assert!(!radar_fits(false, 24));
    assert_eq!(
      radar_label("耶加雪菲 孔加", [8.0, 7.0, 4.0, 2.0, 7.0, 9.0]),
      "耶加雪菲 孔加 风味雷达：酸度 8，甜感 7，醇厚 4，苦度 2，余韵 7，香气 9"
    );
  }
}
