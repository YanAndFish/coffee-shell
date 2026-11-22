//! The list page's layout decisions as plain functions (02-list.md, responsive.md "02 选购"):
//! which form a terminal size gets, the table's columns, what the list panel shows for the
//! list's phase, and the page's text that is built from the query or a product. No views
//! here, so every decision is unit-tested.

use coffee_api::{ProductSummary, roast_name};

use crate::{
  stores::{
    Phase,
    catalog::{CatalogQuery, roast_label, sort_label},
  },
  ui::{Align, Bp, Column, Viewport, Width},
};

/// The page's layout for a width class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
  /// ≥160 columns: filter 27 | list | preview 42.
  Xl,
  /// 130–159: filter 25 | list | preview 36.
  L,
  /// 100–129: list (with the filter chip row) | preview 36.
  M,
  /// 80–99: the list alone, the filter summary on its first row (`f` opens the dialog).
  S,
}

impl Form {
  /// The form of width class `bp` (the too-small notice is the shell's: S stands in).
  pub fn at(bp: Bp) -> Self {
    match bp {
      Bp::Xl => Form::Xl,
      Bp::L => Form::L,
      Bp::M => Form::M,
      Bp::S | Bp::TooSmall => Form::S,
    }
  }

  /// The filter sidebar's width in cells, border included (none at M and S).
  ///
  /// L is 25, not the design's 22: inside the panel's border and side padding, `▐全部▌ [浅]
  /// [中] [深]` takes 21 cells, so 22 would cut it.
  pub fn filter_cols(self) -> Option<u16> {
    match self {
      Form::Xl => Some(27),
      Form::L => Some(25),
      Form::M | Form::S => None,
    }
  }

  /// The preview panel's width in cells, border included (none at S).
  pub fn preview_cols(self) -> Option<u16> {
    match self {
      Form::Xl => Some(42),
      Form::L | Form::M => Some(36),
      Form::S => None,
    }
  }

  /// The preview's content width: the panel less its border, its padding and one blank
  /// cell on each side.
  pub fn preview_content_cols(self) -> usize {
    self
      .preview_cols()
      .map_or(0, |cols| usize::from(cols).saturating_sub(6))
  }

  /// The width of one sort chip in the sidebar's 2×2 grid.
  pub fn sort_chip_cols(self) -> u16 {
    match self {
      Form::Xl => 11,
      _ => 10,
    }
  }

  /// The gap between the panels, in cells.
  pub fn gap_cols(self) -> u16 {
    match self {
      Form::Xl => 2,
      _ => 1,
    }
  }

  /// Whether the table has its 处理法 column (XL and L; at M and S the process follows the
  /// subtitle on the cursor row's second line).
  pub fn has_process(self) -> bool {
    matches!(self, Form::Xl | Form::L)
  }

  /// The table's columns (the cursor column comes on top).
  pub fn columns(self) -> Vec<Column> {
    let name = column("名称", Width::Flex, Align::Start);
    match self {
      Form::Xl => vec![
        name,
        column("产区", Width::Cells(12), Align::Start),
        column("处理法", Width::Cells(10), Align::Start),
        column("烘焙", Width::Cells(8), Align::Start),
        column("价格 /250g", Width::Cells(10), Align::End),
      ],
      Form::L => vec![
        name,
        column("产区", Width::Cells(10), Align::Start),
        column("处理法", Width::Cells(8), Align::Start),
        column("烘焙", Width::Cells(5), Align::Start),
        column("¥/250g", Width::Cells(6), Align::End),
      ],
      Form::M => vec![
        name,
        column("产区", Width::Cells(10), Align::Start),
        column("烘焙", Width::Cells(5), Align::Start),
        column("¥/250g", Width::Cells(6), Align::End),
      ],
      Form::S => vec![
        name,
        column("产区", Width::Cells(11), Align::Start),
        column("烘焙", Width::Cells(6), Align::Start),
        column("¥/250g", Width::Cells(8), Align::End),
      ],
    }
  }
}

fn column(title: &'static str, width: Width, align: Align) -> Column {
  Column {
    title,
    width,
    align,
  }
}

/// The page's shape now: its form and whether the terminal is short (<32 rows: one line per
/// bean with the cursor row's second line, no blank rows at the panels' tops, meters for the
/// radar, a 3-line description).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
  /// The width class's layout.
  pub form: Form,
  /// Fewer than 32 rows.
  pub short: bool,
}

impl Shape {
  /// The shape of `viewport`.
  pub fn of(viewport: Viewport) -> Self {
    Self {
      form: Form::at(viewport.bp()),
      short: viewport.short(),
    }
  }

  /// Whether the preview draws the braille radar (else six meters, bar length 10).
  pub fn radar(self) -> bool {
    crate::ui::radar_fits(self.short, self.form.preview_content_cols())
  }

  /// The most lines of the preview's description (0: as many as the panel holds).
  pub fn description_lines(self) -> u32 {
    if self.short { 3 } else { 0 }
  }
}

/// What the list panel shows under its head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
  /// Nothing to show yet: `⠹ 正在载入商品…`.
  Loading,
  /// The load failed: `✗ 商品加载失败` and `⏎ 重试`.
  Failed,
  /// Nothing matches: `∅ 没有匹配的豆子` and `Esc 清除筛选`.
  Empty,
  /// The rows (also while a newer query loads: the last list stays).
  Rows,
}

/// What the list panel shows in `phase` with `count` products at hand (`has_data`: some list
/// arrived before).
pub fn body(phase: Phase, has_data: bool, count: usize) -> Body {
  match phase {
    Phase::Failed => Body::Failed,
    Phase::Empty => Body::Empty,
    Phase::Ready => Body::Rows,
    Phase::Idle | Phase::Loading if !has_data => Body::Loading,
    Phase::Idle | Phase::Loading if count == 0 => Body::Empty,
    Phase::Idle | Phase::Loading => Body::Rows,
  }
}

/// The list panel's legend: `商品列表 · {n} 款 · {排序}` (`载入中` before the first list,
/// `加载失败` after a failure).
pub fn legend(body: Body, count: usize, sort: &str) -> String {
  match body {
    Body::Loading => "商品列表 · 载入中".into(),
    Body::Failed => "商品列表 · 加载失败".into(),
    Body::Empty | Body::Rows => format!("商品列表 · {count} 款 · {sort}"),
  }
}

/// The S summary row: `产区 全部 · 烘焙度 全部 · 排序 推荐`.
pub fn summary(query: &CatalogQuery) -> String {
  format!(
    "产区 {} · 烘焙度 {} · 排序 {}",
    query.origin_label(),
    roast_label(query.roast),
    sort_label(query.sort)
  )
}

/// The M chip row's three labels (each after its key cap): `产区：全部` `烘焙：全部`
/// `排序：推荐`.
pub fn chip_labels(query: &CatalogQuery) -> [String; 3] {
  [
    format!("产区：{}", query.origin_label()),
    format!("烘焙：{}", roast_label(query.roast)),
    format!("排序：{}", sort_label(query.sort)),
  ]
}

/// The preview's summary: `{烘焙度名} · {处理法} · 杯测 {score}`.
pub fn preview_summary(product: &ProductSummary) -> String {
  format!(
    "{} · {} · 杯测 {}",
    roast_name(product.roast),
    product.process,
    product.score
  )
}

/// The second line of a bean in a short list (the cursor row only): the subtitle, and the
/// process when the table has no column for it.
pub fn second_line(product: &ProductSummary, form: Form) -> String {
  if form.has_process() {
    product.subtitle.clone()
  } else {
    format!("{} · {}", product.subtitle, product.process)
  }
}

/// A row's screen-reader label: `{商品名}，{产区}，{烘焙度名}，¥{价格} 每 250 克`.
pub fn row_label(product: &ProductSummary) -> String {
  format!(
    "{}，{}，{}，¥{} 每 250 克",
    product.name,
    product.origin,
    roast_name(product.roast),
    product.price250
  )
}

/// A bean's flavour values for the radar, 0–10 in the radar's axis order.
pub fn flavour(product: &ProductSummary) -> [f32; 6] {
  product.flavor.map(f32::from)
}

#[cfg(test)]
mod tests {
  use coffee_api::{ProductList, ProductSort, RoastBand};

  use super::*;
  use crate::ui::text::display_width;

  const PRODUCTS: &str = include_str!("../../../../../packages/contract/fixtures/v1/products.json");

  fn products() -> Vec<ProductSummary> {
    serde_json::from_str::<ProductList>(PRODUCTS).unwrap().items
  }

  #[test]
  fn forms_follow_the_breakpoints() {
    let shape = |cols, rows| Shape::of(Viewport::new(cols, rows));
    assert_eq!(
      shape(160, 50),
      Shape {
        form: Form::Xl,
        short: false
      }
    );
    assert_eq!(shape(130, 40).form, Form::L);
    assert_eq!(
      shape(100, 30),
      Shape {
        form: Form::M,
        short: true
      }
    );
    assert_eq!(shape(80, 24).form, Form::S);
    assert!(shape(160, 50).radar());
    assert!(
      shape(130, 40).radar(),
      "36 − 6 = 30 cells hold the 25-cell radar"
    );
    assert!(!shape(100, 30).radar(), "short: meters");
    assert!(!shape(160, 31).radar());
    assert_eq!(shape(100, 30).description_lines(), 3);
    assert_eq!(shape(160, 50).description_lines(), 0);
  }

  #[test]
  fn the_design_widths() {
    assert_eq!(Form::Xl.filter_cols(), Some(27));
    assert_eq!(Form::Xl.preview_cols(), Some(42));
    assert_eq!(Form::Xl.preview_content_cols(), 36);
    assert_eq!(Form::M.filter_cols(), None);
    assert_eq!(Form::S.preview_cols(), None);
    // The roast row and the sort grid fit the sidebar's content (border and padding off).
    for form in [Form::Xl, Form::L] {
      let content = usize::from(form.filter_cols().unwrap()) - 4;
      assert!(
        display_width("▐全部▌ [浅] [中] [深]") <= content,
        "{form:?}"
      );
      assert!(usize::from(form.sort_chip_cols()) * 2 < content, "{form:?}");
    }
    // Fixed columns: their heads and the widest values fit.
    for form in [Form::Xl, Form::L, Form::M, Form::S] {
      for column in form.columns() {
        if let Width::Cells(cells) = column.width {
          assert!(
            display_width(column.title) <= usize::from(cells),
            "{form:?} {}",
            column.title
          );
        }
      }
    }
    let titles = |form: Form| -> Vec<&str> { form.columns().iter().map(|c| c.title).collect() };
    assert_eq!(
      titles(Form::Xl),
      ["名称", "产区", "处理法", "烘焙", "价格 /250g"]
    );
    assert_eq!(titles(Form::S), ["名称", "产区", "烘焙", "¥/250g"]);
    assert!(!Form::M.has_process() && Form::L.has_process());
  }

  #[test]
  fn the_body_follows_the_phase() {
    assert_eq!(body(Phase::Idle, false, 0), Body::Loading);
    assert_eq!(body(Phase::Loading, false, 0), Body::Loading);
    assert_eq!(
      body(Phase::Loading, true, 8),
      Body::Rows,
      "the last list stays"
    );
    assert_eq!(body(Phase::Loading, true, 0), Body::Empty);
    assert_eq!(body(Phase::Ready, true, 8), Body::Rows);
    assert_eq!(body(Phase::Empty, true, 0), Body::Empty);
    assert_eq!(body(Phase::Failed, true, 8), Body::Failed);
    assert_eq!(legend(Body::Loading, 0, "推荐"), "商品列表 · 载入中");
    assert_eq!(legend(Body::Rows, 8, "推荐"), "商品列表 · 8 款 · 推荐");
    assert_eq!(legend(Body::Empty, 0, "杯测分"), "商品列表 · 0 款 · 杯测分");
    assert_eq!(legend(Body::Failed, 0, "推荐"), "商品列表 · 加载失败");
  }

  #[test]
  fn text_from_the_query_and_the_bean() {
    let mut query = CatalogQuery::default();
    assert_eq!(summary(&query), "产区 全部 · 烘焙度 全部 · 排序 推荐");
    query.origin = Some("肯尼亚".into());
    query.roast = Some(RoastBand::Dark);
    query.sort = ProductSort::PriceAsc;
    assert_eq!(summary(&query), "产区 肯尼亚 · 烘焙度 深 · 排序 价格 ↑");
    assert_eq!(
      chip_labels(&query),
      ["产区：肯尼亚", "烘焙：深", "排序：价格 ↑"].map(String::from)
    );
    let beans = products();
    let yirga = &beans[0];
    assert_eq!(preview_summary(yirga), "浅烘 · 水洗 · 杯测 87.5");
    assert_eq!(preview_summary(&beans[1]), "中浅烘 · 水洗 · 杯测 88");
    assert_eq!(
      second_line(yirga, Form::S),
      "Ethiopia · Yirgacheffe Konga G1 · 水洗"
    );
    assert_eq!(
      second_line(yirga, Form::Xl),
      "Ethiopia · Yirgacheffe Konga G1"
    );
    assert_eq!(
      row_label(yirga),
      "耶加雪菲 孔加，埃塞俄比亚，浅烘，¥98 每 250 克"
    );
    assert_eq!(flavour(yirga).len(), 6);
  }
}
