//! The catalog page (`选购 BROWSE`): the bean list in a panel, with the cursor, and its
//! loading, empty and error states. Columns drop as the terminal narrows; rows are two
//! lines (name, then the Latin subtitle) unless the terminal is short.
//!
//! Phase 2 adds the filter and preview panels beside the list.

use coffee_api::ProductSummary;
use nsh_core::{element::Screen, prelude::*};

use crate::{
  commands::press,
  stores::{CatalogPhase, CatalogStore, Stores},
  ui::{
    self, Align, Bp, Column, EmptyAction, Layout, Tone, Width, data_row, label, strong, table_head,
    toned,
  },
};

/// A column of the bean table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
  /// Name and tag, over the subtitle.
  Name,
  /// Origin country.
  Origin,
  /// Processing method.
  Process,
  /// Roast level as pips.
  Roast,
  /// Cupping score.
  Score,
  /// Price of 250 g.
  Price,
}

/// The columns at width class `bp`: the name, roast and price always; the origin from 80
/// columns, the process from 100, the score from 130.
pub fn columns(bp: Bp) -> Vec<(Field, Column)> {
  let column = |title, width, align| Column {
    title,
    width,
    align,
  };
  let mut columns = vec![(Field::Name, column("名称", Width::Flex, Align::Start))];
  if bp >= Bp::S {
    columns.push((
      Field::Origin,
      column("产区", Width::Cells(10), Align::Start),
    ));
  }
  if bp >= Bp::M {
    columns.push((
      Field::Process,
      column("处理法", Width::Cells(11), Align::Start),
    ));
  }
  columns.push((Field::Roast, column("烘焙", Width::Cells(5), Align::Start)));
  if bp >= Bp::L {
    columns.push((Field::Score, column("杯测", Width::Cells(4), Align::End)));
  }
  let price = if bp >= Bp::M {
    column("价格 /250g", Width::Cells(10), Align::End)
  } else {
    column("价格", Width::Cells(5), Align::End)
  };
  columns.push((Field::Price, price));
  columns
}

/// Roast level `roast` (1–5) as five pips: `●●○○○`.
pub fn roast_pips(roast: u8) -> String {
  let filled = usize::from(roast.min(5));
  "●".repeat(filled) + &"○".repeat(5 - filled)
}

/// A price in whole yuan: `¥98`.
pub fn price(yuan: u32) -> String {
  format!("¥{yuan}")
}

/// The panel's legend for `phase` with `count` products.
pub fn legend(phase: CatalogPhase, count: usize) -> String {
  match phase {
    CatalogPhase::Loading => "商品列表 · 加载中".into(),
    CatalogPhase::Ready => format!("商品列表 · {count} 款"),
    CatalogPhase::Empty => "商品列表 · 0 款".into(),
    CatalogPhase::Failed => "商品列表 · 加载失败".into(),
  }
}

/// The catalog page.
pub fn catalog_page() -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  let catalog = stores.catalog;
  let session = stores.session;
  // The page's work: ask for the beans once it shows (the store keeps them after).
  effect(move || catalog.ensure_loaded(session.client()));
  ui::page(
    "选购 BROWSE",
    ui::panel(
      move || SharedString::from(legend(catalog.phase(), catalog.count())),
      dynamic(move || match catalog.phase() {
        CatalogPhase::Loading => ui::loading("正在加载商品…").into_element(),
        CatalogPhase::Empty => ui::empty_state(
          "∅",
          "店里暂时没有豆子",
          Some("后端返回了空的商品列表".into()),
          Some(EmptyAction {
            key: "t",
            label: "重试",
            run: Box::new(move || press(&stores, "catalog.retry")),
          }),
        )
        .into_element(),
        CatalogPhase::Failed => {
          let failure = catalog.failure();
          ui::error_state(
            format!(
              "加载商品失败 · {}",
              failure.as_ref().map_or("", |f| f.title())
            ),
            failure.map(|f| f.detail()).unwrap_or_default(),
            "t",
            "重试",
            move || press(&stores, "catalog.retry"),
          )
          .into_element()
        }
        CatalogPhase::Ready => table(stores, layout).into_element(),
      }),
    ),
  )
}

/// The bean table, its columns following the width and its row height the height.
fn table(stores: Stores, layout: Layout) -> impl IntoElement {
  let catalog = stores.catalog;
  dynamic(move || {
    let columns = columns(layout.bp());
    let short = layout.short();
    let heads: Vec<Column> = columns.iter().map(|(_, column)| *column).collect();
    view().flex_1().child(table_head(&heads)).child(each(
      move || catalog.products().to_vec(),
      |product: &ProductSummary| product.id.clone(),
      move |product, _| {
        let product = product.get_untracked();
        row(stores, &columns, product, short)
      },
    ))
  })
}

/// One bean's row.
fn row(stores: Stores, columns: &[(Field, Column)], product: ProductSummary, short: bool) -> View {
  let catalog = stores.catalog;
  let id = SharedString::from(product.id.clone());
  let current = {
    let id = id.clone();
    move || catalog.is_cursor(&id)
  };
  let cells = columns
    .iter()
    .map(|(field, _)| cell(*field, &product, short, current.clone()))
    .collect();
  let heads: Vec<Column> = columns.iter().map(|(_, column)| *column).collect();
  data_row(&heads, current, cells, move |clicks| {
    open_or_select(stores, catalog, &id, clicks)
  })
}

/// A click puts the cursor on the row; a second click, or a double click, opens it.
fn open_or_select(stores: Stores, catalog: CatalogStore, id: &str, clicks: u32) {
  let on_it = untracked(|| catalog.is_cursor(id));
  catalog.select(id);
  if clicks >= 2 || on_it {
    press(&stores, "catalog.open");
  }
}

/// The content of `field` for `product`.
fn cell(
  field: Field,
  product: &ProductSummary,
  short: bool,
  current: impl Fn() -> bool + Clone + 'static,
) -> AnyElement {
  match field {
    Field::Name => {
      let name = {
        let tone = move || if current() { Tone::Acc } else { Tone::Fg };
        toned(product.name.clone(), tone)
          .font_weight(FontWeight::BOLD)
          .child(
            product
              .tag
              .as_ref()
              .map(|tag| label(format!(" [{tag}]"), Tone::Acc2).font_weight(FontWeight::NORMAL)),
          )
          .truncate()
      };
      view()
        .w_full()
        .child(name)
        .child((!short).then(|| label(product.subtitle.clone(), Tone::Mute).truncate()))
        .into_element()
    }
    Field::Origin => label(product.origin.clone(), Tone::Soft)
      .truncate()
      .into_element(),
    Field::Process => label(product.process.clone(), Tone::Mute)
      .truncate()
      .into_element(),
    Field::Roast => label(roast_pips(product.roast), Tone::Acc).into_element(),
    Field::Score => label(format!("{:.1}", product.score), Tone::Mute).into_element(),
    Field::Price => strong(price(product.price250), Tone::Fg).into_element(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  fn fields(bp: Bp) -> Vec<Field> {
    columns(bp).into_iter().map(|(field, _)| field).collect()
  }

  #[test]
  fn columns_drop_as_the_terminal_narrows() {
    use Field::*;
    assert_eq!(fields(Bp::S), vec![Name, Origin, Roast, Price]);
    assert_eq!(fields(Bp::M), vec![Name, Origin, Process, Roast, Price]);
    assert_eq!(
      fields(Bp::L),
      vec![Name, Origin, Process, Roast, Score, Price]
    );
    assert_eq!(fields(Bp::Xl), fields(Bp::L));
  }

  #[test]
  fn fixed_columns_leave_the_name_room_at_80_columns() {
    // 80 columns: the shell's side padding (2), the panel's border and padding (4), the
    // row's padding (2), the cursor column (2) and a one-cell gap before every column.
    let columns = columns(Bp::S);
    let fixed: usize = columns
      .iter()
      .map(|(_, column)| match column.width {
        Width::Cells(n) => usize::from(n),
        Width::Flex => 0,
      })
      .sum();
    let name = 80 - 2 - 4 - 2 - 2 - columns.len() - fixed;
    assert!(name >= 30, "the name column gets {name} cells");
    // The longest fixture name and subtitle fit whole.
    assert!(display_width("「夜班」意式拼配 [招牌]") <= name);
    assert!(display_width("Kenya · Nyeri AA Double Washed") <= name);
  }

  #[test]
  fn fixed_columns_fit_their_widest_fixture_values() {
    let products: coffee_api::ProductList = serde_json::from_str(include_str!(
      "../../../../packages/contract/fixtures/v1/products.json"
    ))
    .unwrap();
    for (field, column) in columns(Bp::Xl) {
      let Width::Cells(width) = column.width else {
        continue;
      };
      let width = usize::from(width);
      assert!(display_width(column.title) <= width, "{:?} head", field);
      for product in &products.items {
        let value = match field {
          Field::Origin => product.origin.clone(),
          Field::Process => product.process.clone(),
          Field::Roast => roast_pips(product.roast),
          Field::Score => format!("{:.1}", product.score),
          Field::Price => price(product.price250),
          Field::Name => unreachable!(),
        };
        assert!(display_width(&value) <= width, "{field:?}: {value}");
      }
    }
  }

  #[test]
  fn formats() {
    assert_eq!(roast_pips(1), "●○○○○");
    assert_eq!(roast_pips(4), "●●●●○");
    assert_eq!(roast_pips(9), "●●●●●");
    assert_eq!(price(98), "¥98");
    assert_eq!(legend(CatalogPhase::Ready, 8), "商品列表 · 8 款");
    assert_eq!(legend(CatalogPhase::Failed, 0), "商品列表 · 加载失败");
  }
}
