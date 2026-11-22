//! The filters in their three forms (02-list.md): the `筛选 FILTER` sidebar (XL, L), the chip
//! row at the list's top (M), the summary row (S), and the S-width filter dialog.
//!
//! Every control runs what its key runs: a radio or a chip sets the query through the
//! catalog store, a chip-row key cap or the summary's `‹f› 筛选` presses the command.

use coffee_api::RoastBand;
use nsh_core::prelude::*;

use super::form::{Form, chip_labels, summary};
use crate::{
  commands::press,
  stores::{
    Stores,
    catalog::{FilterGroup, ROAST_BANDS, SORTS},
    ui::Overlay,
  },
  ui::{
    self, Button, ButtonKind, ChipPad, DialogSize, Tone, action, chip, label, layout, panel, radio,
    section_head, segmented,
  },
};

/// The roast options, `全部` first.
const ROASTS: [&str; 4] = ["全部", "浅", "中", "深"];

/// The roast band of option `index` (`全部` is `None`).
fn roast_at(index: usize) -> Option<RoastBand> {
  index
    .checked_sub(1)
    .and_then(|i| ROAST_BANDS.get(i).copied())
}

/// The origin radios: `◉ 全部  8`, then each origin of the facets with its count; a press
/// shows that origin only. `mark(index)` draws a gutter before each (the dialog's focus).
fn origin_radios(stores: Stores, mark: Option<fn(Stores, usize) -> AnyElement>) -> AnyElement {
  let catalog = stores.catalog;
  dynamic(move || {
    let facets = catalog.facets();
    let mut options: Vec<(Option<String>, Option<SharedString>)> =
      vec![(None, facets.as_ref().map(|f| f.total.to_string().into()))];
    if let Some(facets) = facets {
      options.extend(
        facets
          .origins
          .into_iter()
          .map(|o| (Some(o.origin), Some(o.count.to_string().into()))),
      );
    }
    view()
      .flex_none()
      .children(
        options
          .into_iter()
          .enumerate()
          .map(move |(index, (origin, count))| {
            let name = origin.clone().unwrap_or_else(|| "全部".to_owned());
            let radio = radio(
              name,
              count,
              move || catalog.filter_selected(FilterGroup::Origin) == index,
              move || catalog.set_origin(origin.clone()),
            );
            view()
              .flex_none()
              .flex_row()
              .child(mark.map(|mark| mark(stores, index)))
              .child(view().flex_1().min_w(px(0.0)).child(radio))
          }),
      )
  })
}

/// The sort grid: two rows of two chips `width` cells each, a cell apart.
fn sort_grid(stores: Stores, width: u16) -> View {
  let catalog = stores.catalog;
  let selected = move || catalog.filter_selected(FilterGroup::Sort);
  let set = move |index: usize| catalog.set_sort(SORTS[index]);
  view()
    .flex_none()
    .child(segmented(
      &["推荐", "价格 ↑"],
      ChipPad::Width(width),
      selected,
      set,
    ))
    .child(segmented(
      &["价格 ↓", "杯测分"],
      ChipPad::Width(width),
      // No chip of this row is chosen while the sort is in the first.
      move || selected().checked_sub(2).unwrap_or(usize::MAX),
      move |index| set(index + 2),
    ))
}

/// The roast segmented control `▐全部▌ [浅] [中] [深]`.
fn roast_row(stores: Stores) -> View {
  let catalog = stores.catalog;
  segmented(
    &ROASTS,
    ChipPad::Tight,
    move || catalog.filter_selected(FilterGroup::Roast),
    move |index| catalog.set_roast(roast_at(index)),
  )
}

/// The `筛选 FILTER` sidebar (XL, L): 产区 radios with their counts, 烘焙度 and 排序 chips,
/// each section headed by its key cap.
pub fn sidebar(stores: Stores, form: Form, short: bool) -> View {
  panel(
    "筛选 FILTER",
    view()
      .flex_1()
      .child((!short).then(|| layout::vspace(1)))
      .child(section_head("产区", "f"))
      .child(origin_radios(stores, None))
      .child(layout::vspace(1))
      .child(section_head("烘焙度", "r"))
      .child(roast_row(stores))
      .child(layout::vspace(1))
      .child(section_head("排序", "s"))
      .child(sort_grid(stores, form.sort_chip_cols())),
  )
}

/// The M chip row: `‹f› 产区：全部   ‹r› 烘焙：全部   ‹s› 排序：推荐`; a press cycles as the
/// key does.
pub fn chip_row(stores: Stores) -> AnyElement {
  let catalog = stores.catalog;
  dynamic(move || {
    let [origin, roast, sort] = chip_labels(&catalog.query());
    view()
      .flex_none()
      .flex_row()
      .child(action("f", origin, move || {
        press(&stores, "catalog.origin")
      }))
      .child(layout::hspace(3))
      .child(action("r", roast, move || press(&stores, "catalog.roast")))
      .child(layout::hspace(3))
      .child(action("s", sort, move || press(&stores, "catalog.sort")))
  })
}

/// The S summary row: `产区 全部 · 烘焙度 全部 · 排序 推荐` and, at its end, `‹f› 筛选` (opens
/// the dialog).
pub fn summary_row(stores: Stores) -> View {
  let catalog = stores.catalog;
  view()
    .flex_none()
    .flex_row()
    .child(
      view().flex_1().min_w(px(0.0)).child(
        label(
          move || SharedString::from(summary(&catalog.query())),
          Tone::Soft,
        )
        .truncate(),
      ),
    )
    .child(action("f", "筛选", move || {
      press(&stores, "catalog.origin")
    }))
}

// --- The S-width dialog ----------------------------------------------------------------

/// The dialog's size in the design (02-list.md, "筛选对话框").
const DIALOG_COLS: u16 = 60;
const DIALOG_ROWS: u16 = 16;
/// Blank cells between the dialog's side lines and its content.
const DIALOG_PAD: u16 = 2;
/// The origin column's width; the chips take the rest.
const ORIGIN_COLS: u16 = 26;
/// A sort chip's width in the dialog.
const DIALOG_SORT_COLS: u16 = 12;

/// The focus gutter of the dialog's option `index` of `group`: `›` (bold `acc`) on the
/// focused option, else blank (two cells).
fn focus_mark(stores: Stores, group: FilterGroup, index: usize) -> View {
  let catalog = stores.catalog;
  view().flex_none().w(layout::cols(2)).child(
    ui::toned(
      move || {
        if catalog.filter_focus() == (group, index) {
          "›"
        } else {
          ""
        }
      },
      || Tone::Acc,
    )
    .font_weight(FontWeight::BOLD),
  )
}

fn origin_mark(stores: Stores, index: usize) -> AnyElement {
  focus_mark(stores, FilterGroup::Origin, index).into_element()
}

/// A chip group of the dialog: its options, the chosen one solid, the focused one led by
/// `›` inside its brackets (the chip has no focus look of its own), `per_row` a row.
fn chip_group(
  stores: Stores,
  group: FilterGroup,
  options: &'static [&'static str],
  pad: ChipPad,
  per_row: usize,
) -> AnyElement {
  let catalog = stores.catalog;
  dynamic(move || {
    let (focus_group, focus_index) = catalog.filter_focus();
    let rows = options
      .chunks(per_row)
      .enumerate()
      .map(move |(row, chunk)| {
        view()
          .flex_none()
          .flex_row()
          .gap(layout::gap_cols(1))
          .children(chunk.iter().enumerate().map(move |(at, option)| {
            let index = row * per_row + at;
            let focused = focus_group == group && focus_index == index;
            let text = if focused {
              format!("›{option}")
            } else {
              (*option).to_owned()
            };
            chip(
              text,
              pad,
              move || catalog.filter_selected(group) == index,
              || true,
              move || match group {
                FilterGroup::Roast => catalog.set_roast(roast_at(index)),
                _ => catalog.set_sort(SORTS[index]),
              },
            )
          }))
      });
    view().flex_none().children(rows.collect::<Vec<_>>())
  })
}

/// The dialog's content: the origin radios (left) and the roast and sort chips with the
/// help (right).
fn dialog_content(stores: Stores) -> View {
  let left = view()
    .flex_none()
    .w(layout::cols(ORIGIN_COLS))
    .child(
      view()
        .flex_row()
        .child(layout::hspace(2))
        .child(view().flex_1().child(section_head("产区", "f"))),
    )
    .child(origin_radios(stores, Some(origin_mark)));
  let right = view()
    .flex_1()
    .min_w(px(0.0))
    .child(section_head("烘焙度", "r"))
    .child(chip_group(
      stores,
      FilterGroup::Roast,
      &ROASTS,
      ChipPad::Tight,
      4,
    ))
    .child(layout::vspace(1))
    .child(section_head("排序", "s"))
    .child(chip_group(
      stores,
      FilterGroup::Sort,
      &["推荐", "价格 ↑", "价格 ↓", "杯测分"],
      ChipPad::Width(DIALOG_SORT_COLS),
      2,
    ))
    .child(layout::vspace(1))
    .child(label("↑↓ 选择 · Tab 下一组", Tone::Mute))
    .child(label("⏎ / space 选中，立即生效", Tone::Mute));
  view()
    .flex_1()
    .flex_row()
    .child(left)
    .child(layout::hspace(2))
    .child(right)
}

/// The S-width filter dialog (`筛选 FILTER`, 60×16, narrowed to the terminal), an overlay
/// centred between the system row and the status bar over the list (`ui::dialog`, as every
/// dialog). ⏎ / space choose at once; there is no apply button.
pub fn dialog() -> AnyElement {
  let stores = Stores::get();
  ui::dialog(
    move || stores.ui.is_open(Overlay::FilterDialog),
    "筛选 FILTER",
    DialogSize {
      cols: DIALOG_COLS,
      rows: DIALOG_ROWS,
      pad: DIALOG_PAD,
    },
    move || dialog_content(stores).into_element(),
    // The close button runs what Esc runs.
    move || {
      vec![
        Button::new(ButtonKind::Default, "Esc", "关闭")
          .on_press(move || press(&stores, "filter.close")),
      ]
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roast_options_map_to_bands() {
    assert_eq!(roast_at(0), None);
    assert_eq!(roast_at(1), Some(RoastBand::Light));
    assert_eq!(roast_at(3), Some(RoastBand::Dark));
    assert_eq!(roast_at(4), None);
    assert_eq!(ROASTS.len(), ROAST_BANDS.len() + 1);
  }

  #[test]
  fn the_dialog_columns_fit_its_content() {
    let content = DIALOG_COLS - 2 - 2 * DIALOG_PAD;
    let right = content - ORIGIN_COLS - 2;
    assert!(DIALOG_SORT_COLS * 2 < right);
    assert!(ui::text::display_width("▐全部▌ [浅] [中] [深]") < usize::from(right));
  }
}
