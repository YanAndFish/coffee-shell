//! The catalog: `GET /v1/products` and `GET /v1/products/{id}`.

use serde::{Deserialize, Serialize};

use crate::{Pack, Weight, encode_query_value};

/// Flavor axes, in the order of a flavor array.
pub const FLAVOR_AXES: [&str; 6] = ["酸度", "甜感", "醇厚", "苦度", "余韵", "香气"];

/// One product in a list (`GET /v1/products`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
  /// Stable id, e.g. `yirga`.
  pub id: String,
  /// Display name, e.g. `耶加雪菲 孔加`.
  pub name: String,
  /// Latin subtitle, e.g. `Ethiopia · Yirgacheffe Konga G1`.
  pub subtitle: String,
  /// Origin country or `拼配`.
  pub origin: String,
  /// Region, e.g. `耶加雪菲 · 孔加`.
  pub region: String,
  /// Processing method, e.g. `水洗`.
  pub process: String,
  /// Roast level, 1 (light) to 5 (dark).
  pub roast: u8,
  /// Badge such as `新产季`, or `None`.
  pub tag: Option<String>,
  /// Tasting notes.
  pub notes: Vec<String>,
  /// Flavor profile 0..=10 in the order of [`FLAVOR_AXES`].
  pub flavor: [u8; 6],
  /// Cupping score, e.g. `87.5`.
  pub score: f64,
  /// The preview's description.
  pub description: String,
  /// Base price of 250 g, integer yuan.
  pub price250: u32,
  /// Units in stock.
  pub stock: u32,
  /// What quick add and the detail page start with.
  pub default_weight: Weight,
}

/// One origin of the filter sidebar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginFacet {
  /// The origin, e.g. `埃塞俄比亚`.
  pub origin: String,
  /// How many products.
  pub count: u32,
}

/// The filter sidebar's counts, over the whole catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductFacets {
  /// The `全部` count.
  pub total: u32,
  /// Each origin, in order of first appearance.
  pub origins: Vec<OriginFacet>,
}

/// `GET /v1/products`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductList {
  /// The matching products, in the requested order.
  pub items: Vec<ProductSummary>,
  /// The sidebar counts (whatever the filters).
  pub facets: ProductFacets,
}

/// Roast band filter: 浅 (1–2), 中 (3), 深 (4–5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoastBand {
  /// 浅: levels 1–2.
  Light,
  /// 中: level 3.
  Medium,
  /// 深: levels 4–5.
  Dark,
}

impl RoastBand {
  /// The wire value.
  pub fn code(self) -> &'static str {
    match self {
      RoastBand::Light => "light",
      RoastBand::Medium => "medium",
      RoastBand::Dark => "dark",
    }
  }
}

/// List order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductSort {
  /// 推荐: display order.
  #[default]
  Recommended,
  /// 价格 ↑.
  PriceAsc,
  /// 价格 ↓.
  PriceDesc,
  /// 杯测分 (high to low).
  Score,
}

impl ProductSort {
  /// The wire value.
  pub fn code(self) -> &'static str {
    match self {
      ProductSort::Recommended => "recommended",
      ProductSort::PriceAsc => "price_asc",
      ProductSort::PriceDesc => "price_desc",
      ProductSort::Score => "score",
    }
  }
}

/// Query of `GET /v1/products` (all optional).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProductListQuery {
  /// Exact origin; `None` = all.
  pub origin: Option<String>,
  /// Roast band; `None` = all.
  pub roast: Option<RoastBand>,
  /// Order; `None` = recommended.
  pub sort: Option<ProductSort>,
  /// Search text (trimmed and lower-cased by the server).
  pub q: Option<String>,
}

impl ProductListQuery {
  /// `""` or `?origin=…&roast=…&sort=…&q=…` (percent-encoded), to append to `/products`.
  pub fn to_query_string(&self) -> String {
    let mut parts = Vec::new();
    if let Some(origin) = self.origin.as_deref().filter(|s| !s.is_empty()) {
      parts.push(format!("origin={}", encode_query_value(origin)));
    }
    if let Some(roast) = self.roast {
      parts.push(format!("roast={}", roast.code()));
    }
    if let Some(sort) = self.sort {
      parts.push(format!("sort={}", sort.code()));
    }
    if let Some(q) = self.q.as_deref().filter(|s| !s.trim().is_empty()) {
      parts.push(format!("q={}", encode_query_value(q)));
    }
    if parts.is_empty() {
      String::new()
    } else {
      format!("?{}", parts.join("&"))
    }
  }
}

/// Whether one weight can be bought.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightOption {
  /// The weight.
  pub weight: Weight,
  /// `false` when sold out (`缺货`).
  pub available: bool,
}

/// The price of one weight × pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCell {
  /// The weight.
  pub weight: Weight,
  /// The pack.
  pub pack: Pack,
  /// Integer yuan.
  pub unit_price: u32,
  /// `totals[n - 1]` = the total for quantity `n`, n = 1 … max quantity.
  pub totals: Vec<u32>,
}

/// `GET /v1/products/{id}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
  /// Stable id.
  pub id: String,
  /// Display name.
  pub name: String,
  /// Latin subtitle.
  pub subtitle: String,
  /// 产区.
  pub origin: String,
  /// Region.
  pub region: String,
  /// 处理法.
  pub process: String,
  /// 1–5.
  pub roast: u8,
  /// Badge or `None`.
  pub tag: Option<String>,
  /// Tasting notes.
  pub notes: Vec<String>,
  /// Flavor profile.
  pub flavor: [u8; 6],
  /// 杯测.
  pub score: f64,
  /// Description.
  pub description: String,
  /// Base price of 250 g.
  pub price250: u32,
  /// Units in stock.
  pub stock: u32,
  /// The weight the SKU panel starts with.
  pub default_weight: Weight,
  /// SKU code, e.g. `PNK`.
  pub code: String,
  /// 庄园.
  pub farm: String,
  /// 海拔 as displayed.
  pub altitude: String,
  /// 品种.
  pub variety: String,
  /// 产季.
  pub harvest: String,
  /// 冲煮建议.
  pub brew: String,
  /// min(9, stock); 0 disables buying.
  pub max_quantity: u8,
  /// The four weights with availability.
  pub weights: Vec<WeightOption>,
  /// Eight cells: weight × pack.
  pub prices: Vec<PriceCell>,
}

impl ProductDetail {
  /// The price cell of `weight` × `pack`.
  pub fn price(&self, weight: Weight, pack: Pack) -> Option<&PriceCell> {
    self
      .prices
      .iter()
      .find(|cell| cell.weight == weight && cell.pack == pack)
  }
}
