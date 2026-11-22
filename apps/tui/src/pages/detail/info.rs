//! The information side of the detail page (03-detail.md, "信息面板"): the back button and
//! breadcrumb, the title with its tag and English subtitle, the description, the bean's
//! dossier (豆子档案) as leaders, the flavour block (风味指标: radar and / or meters) and
//! the brewing inset (冲煮建议 · 风味).

use coffee_api::{ProductDetail, roast_name};
use nsh_core::prelude::*;

use super::Shape;
use crate::{
  commands::press,
  stores::Stores,
  ui::{
    self, Button, ButtonKind, TagKind, Tone, heading, inset, label, layout, leader, meter, radar,
    radar_label, strong,
  },
};

/// The dossier's leader width at XL and L (03-detail.md: 31 cells).
pub const DOSSIER_COLS: usize = 31;
/// The dossier's leader width when it sits beside the meters (S, M: 36 cells).
pub const DOSSIER_SIDE_COLS: usize = 36;
/// A meter row with a 10-cell bar: label 4, blank, bar 10, value 6.
pub const METER_COLS: u16 = 21;
/// The radar block's width.
pub const RADAR_COLS: u16 = 25;
/// The description's measure at XL (700 px of the design: 87 cells).
pub const DESCRIPTION_COLS: u16 = 87;
/// The flavour axes, clockwise from the top (the radar's and the meters' order).
pub const AXES: [&str; 6] = ["酸度", "甜感", "醇厚", "苦度", "余韵", "香气"];

/// The dossier rows: `产区` `庄园` `海拔` `品种` `处理法` `产季` `烘焙度` `杯测`. The roast
/// level is its name and its pips (`中浅烘 ●●○○○`), the score `{score} 分`.
pub fn dossier(detail: &ProductDetail) -> Vec<(&'static str, String)> {
  vec![
    ("产区", detail.region.clone()),
    ("庄园", detail.farm.clone()),
    ("海拔", detail.altitude.clone()),
    ("品种", detail.variety.clone()),
    ("处理法", detail.process.clone()),
    ("产季", detail.harvest.clone()),
    (
      "烘焙度",
      format!("{} {}", roast_name(detail.roast), ui::pips(detail.roast, 5)),
    ),
    ("杯测", format!("{} 分", detail.score)),
  ]
}

/// The breadcrumb's muted part: `选购 / {产区} / ` (the product's name follows in `fg`).
pub fn breadcrumb(detail: &ProductDetail) -> String {
  format!("选购 / {} / ", detail.origin)
}

/// The tasting notes joined for the inset: `荔枝 · 蔓越莓 · 蜂蜜 · 可可`.
pub fn notes_line(notes: &[String]) -> String {
  notes.join(" · ")
}

/// The flavour values as the radar and meters read them.
pub fn flavor_values(detail: &ProductDetail) -> [f32; 6] {
  detail.flavor.map(f32::from)
}

/// `[ ← Esc 返回 ]  选购 / 哥伦比亚 / 慧兰 粉红波旁`.
pub fn back_row(stores: Stores, detail: &ProductDetail) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(
      Button::new(ButtonKind::Default, "", "← Esc 返回")
        .on_press(move || press(&stores, "detail.back")),
    )
    .child(layout::hspace(2))
    .child(
      text("")
        .child(label(breadcrumb(detail), Tone::Mute))
        .child(label(detail.name.clone(), Tone::Fg))
        .truncate(),
    )
}

fn tag_of(detail: &ProductDetail) -> Option<View> {
  detail
    .tag
    .as_ref()
    .filter(|tag| !tag.is_empty())
    .map(|tag| ui::tag(TagKind::Solid, tag.clone()))
}

/// The title (bold) and its solid tag; the subtitle on the next row (XL, L).
pub fn title_block(detail: &ProductDetail) -> View {
  let tag = tag_of(detail);
  view()
    .flex_none()
    .child(
      view()
        .flex_row()
        .child(strong(detail.name.clone(), Tone::Fg))
        .child(tag.map(|tag| view().flex_row().child(layout::hspace(2)).child(tag))),
    )
    .child(label(detail.subtitle.clone(), Tone::Mute).truncate())
}

/// The title, tag and subtitle on one row (S, M).
pub fn title_row(detail: &ProductDetail) -> View {
  let tag = tag_of(detail);
  view()
    .flex_none()
    .flex_row()
    .child(strong(detail.name.clone(), Tone::Fg))
    .child(tag.map(|tag| view().flex_row().child(layout::hspace(1)).child(tag)))
    .child(layout::hspace(2))
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .child(label(detail.subtitle.clone(), Tone::Mute).truncate()),
    )
}

/// The description (`soft`), at most three rows on a short terminal.
pub fn description(detail: &ProductDetail, shape: Shape, short: bool) -> View {
  let body = label(detail.description.clone(), Tone::Soft);
  let body = if short {
    body.number_of_lines(3u32)
  } else {
    body
  };
  let block = view().flex_none().child(body);
  if shape == Shape::Wide {
    // The panel's inside is at least 96 cells at XL.
    block.w(layout::cols(DESCRIPTION_COLS))
  } else {
    block
  }
}

/// The dossier: `── 豆子档案` and its leaders, `width` cells.
fn dossier_block(detail: &ProductDetail, width: usize) -> View {
  view()
    .flex_none()
    .w(layout::cols(width as u16))
    .child(heading("豆子档案"))
    .children(
      dossier(detail)
        .into_iter()
        .map(move |(key, value)| leader(key, &value, width)),
    )
}

/// Six meter rows `length` cells long, a blank row between two when `spaced`.
fn meters(values: [f32; 6], length: usize, spaced: bool) -> View {
  view().flex_none().children((0..6).map(move |i| {
    let row = meter(AXES[i], move || values[i], length, Tone::Acc);
    if spaced && i > 0 {
      view().child(layout::vspace(1)).child(row).into_element()
    } else {
      row.into_element()
    }
  }))
}

/// The radar block (25×11, the bean's polygon in `acc`).
fn radar_block(name: &str, values: [f32; 6]) -> AnyElement {
  radar(
    move || values,
    Tone::Acc,
    || true,
    radar_label(name, values),
  )
}

/// The dossier and flavour block for `shape`:
///
/// - XL: dossier (31) · 4 · radar (25) · 2 · meters (20, a blank row between two);
/// - L, and M / S on a tall terminal: dossier (31) · 3 · the radar with the meters (10) under it;
/// - short (fewer than 32 rows) or S / M: meters (10) and dossier (36) side by side, the meters
///   first (03-detail.md, S); at XL and L short the radar's place takes the meters.
pub fn profile(detail: &ProductDetail, shape: Shape, short: bool) -> View {
  let values = flavor_values(detail);
  let flavor_head = || heading("风味指标");
  match (shape, short) {
    (Shape::Wide, false) => view()
      .flex_none()
      .flex_row()
      .child(dossier_block(detail, DOSSIER_COLS))
      .child(layout::hspace(4))
      .child(
        view()
          .flex_none()
          .w(layout::cols(RADAR_COLS))
          .child(flavor_head())
          .child(radar_block(&detail.name, values)),
      )
      .child(layout::hspace(2))
      .child(
        view()
          .flex_none()
          .child(layout::vspace(1))
          .child(meters(values, 20, true)),
      ),
    (Shape::Wide, true) => view()
      .flex_none()
      .flex_row()
      .child(dossier_block(detail, DOSSIER_COLS))
      .child(layout::hspace(4))
      .child(
        view()
          .flex_none()
          .child(flavor_head())
          .child(meters(values, 20, false)),
      ),
    (_, false) => view()
      .flex_none()
      .flex_row()
      .child(dossier_block(detail, DOSSIER_COLS))
      .child(layout::hspace(3))
      .child(
        view()
          .flex_none()
          .w(layout::cols(RADAR_COLS))
          .child(flavor_head())
          .child(radar_block(&detail.name, values))
          .child(layout::vspace(1))
          .child(meters(values, 10, false)),
      ),
    (Shape::Narrow, true) => view()
      .flex_none()
      .flex_row()
      .child(dossier_block(detail, DOSSIER_COLS))
      .child(layout::hspace(3))
      .child(
        view()
          .flex_none()
          .child(flavor_head())
          .child(meters(values, 10, false)),
      ),
    (_, true) => view()
      .flex_none()
      .flex_row()
      .child(
        // 36 cells as the design has it, giving way (down to the meters' 21) before the
        // dossier does on a narrow terminal.
        view()
          .flex_none()
          .flex_shrink(1.0)
          .w(layout::cols(DOSSIER_SIDE_COLS as u16))
          .min_w(layout::cols(METER_COLS))
          .child(flavor_head())
          .child(meters(values, 10, false)),
      )
      .child(layout::hspace(1))
      .child(dossier_block(detail, DOSSIER_SIDE_COLS)),
  }
}

/// The brewing inset: `冲煮建议  {brew}` left, `风味  {notes}` (`acc2`) right.
pub fn brew(detail: &ProductDetail) -> View {
  inset(
    view()
      .flex_row()
      .child(
        text("")
          .child(label("冲煮建议  ", Tone::Mute))
          .child(label(detail.brew.clone(), Tone::Fg))
          .truncate(),
      )
      .child(view().flex_1().min_w(layout::cols(2)))
      .child(
        text("")
          .child(label("风味  ", Tone::Mute))
          .child(label(notes_line(&detail.notes), Tone::Acc2))
          .truncate(),
      ),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  fn pinkb() -> ProductDetail {
    serde_json::from_str(include_str!(
      "../../../../../packages/contract/fixtures/v1/catalog/product-pinkb.json"
    ))
    .unwrap()
  }

  #[test]
  fn the_dossier_reads_as_the_design() {
    let rows = dossier(&pinkb());
    let keys: Vec<_> = rows.iter().map(|(k, _)| *k).collect();
    assert_eq!(
      keys,
      [
        "产区",
        "庄园",
        "海拔",
        "品种",
        "处理法",
        "产季",
        "烘焙度",
        "杯测"
      ]
    );
    assert_eq!(rows[0].1, "慧兰 · 皮塔利托");
    assert_eq!(rows[2].1, "1,700–1,850 m");
    assert_eq!(rows[6].1, "中浅烘 ●●○○○");
    assert_eq!(rows[7].1, "88 分");
    for (key, value) in &rows {
      let (shown, dots) = ui::leader::leader_parts(key, value, DOSSIER_COLS);
      assert_eq!(shown, *key, "no key is cut at 31 cells");
      assert_eq!(
        display_width(&format!("{shown} {dots} {value}")),
        DOSSIER_COLS
      );
    }
  }

  #[test]
  fn breadcrumb_and_notes() {
    let detail = pinkb();
    assert_eq!(breadcrumb(&detail), "选购 / 哥伦比亚 / ");
    assert_eq!(notes_line(&detail.notes), "荔枝 · 蔓越莓 · 蜂蜜 · 可可");
    assert_eq!(flavor_values(&detail), [7.0, 8.0, 5.0, 2.0, 8.0, 8.0]);
  }
}
