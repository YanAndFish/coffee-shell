//! The SKU dimensions: weight × grind × pack (codes on the wire, labels for people).

use serde::{Deserialize, Serialize};

/// Weight options, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Weight {
  /// `100g`.
  #[serde(rename = "100g")]
  G100,
  /// `250g`.
  #[serde(rename = "250g")]
  G250,
  /// `500g`.
  #[serde(rename = "500g")]
  G500,
  /// `1kg`.
  #[serde(rename = "1kg")]
  Kg1,
}

impl Weight {
  /// All weights in display order.
  pub const ALL: [Weight; 4] = [Weight::G100, Weight::G250, Weight::G500, Weight::Kg1];

  /// The wire code and display label, e.g. `250g`.
  pub fn label(self) -> &'static str {
    match self {
      Weight::G100 => "100g",
      Weight::G250 => "250g",
      Weight::G500 => "500g",
      Weight::Kg1 => "1kg",
    }
  }

  /// The SKU-code part, e.g. `250G`.
  pub fn sku_part(self) -> &'static str {
    match self {
      Weight::G100 => "100G",
      Weight::G250 => "250G",
      Weight::G500 => "500G",
      Weight::Kg1 => "1KG",
    }
  }
}

/// Grind options, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Grind {
  /// `WB` 整豆.
  #[serde(rename = "WB")]
  WholeBean,
  /// `PO` 手冲.
  #[serde(rename = "PO")]
  PourOver,
  /// `ES` 意式.
  #[serde(rename = "ES")]
  Espresso,
  /// `FP` 法压.
  #[serde(rename = "FP")]
  FrenchPress,
  /// `CB` 冷萃.
  #[serde(rename = "CB")]
  ColdBrew,
}

impl Grind {
  /// All grinds in display order.
  pub const ALL: [Grind; 5] = [
    Grind::WholeBean,
    Grind::PourOver,
    Grind::Espresso,
    Grind::FrenchPress,
    Grind::ColdBrew,
  ];

  /// The wire code, e.g. `WB`.
  pub fn code(self) -> &'static str {
    match self {
      Grind::WholeBean => "WB",
      Grind::PourOver => "PO",
      Grind::Espresso => "ES",
      Grind::FrenchPress => "FP",
      Grind::ColdBrew => "CB",
    }
  }

  /// The display label, e.g. `整豆`.
  pub fn label(self) -> &'static str {
    match self {
      Grind::WholeBean => "整豆",
      Grind::PourOver => "手冲",
      Grind::Espresso => "意式",
      Grind::FrenchPress => "法压",
      Grind::ColdBrew => "冷萃",
    }
  }
}

/// Pack options, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pack {
  /// `STD` 标准袋.
  #[serde(rename = "STD")]
  Standard,
  /// `GIFT` 礼盒装 (`+¥20` is copy; prices come from the server).
  #[serde(rename = "GIFT")]
  Gift,
}

impl Pack {
  /// Both packs in display order.
  pub const ALL: [Pack; 2] = [Pack::Standard, Pack::Gift];

  /// The wire code, e.g. `STD`.
  pub fn code(self) -> &'static str {
    match self {
      Pack::Standard => "STD",
      Pack::Gift => "GIFT",
    }
  }

  /// The display label, e.g. `标准袋`.
  pub fn label(self) -> &'static str {
    match self {
      Pack::Standard => "标准袋",
      Pack::Gift => "礼盒装",
    }
  }
}

/// `RT-{product code}-{WEIGHT}-{grind}-{pack}`, e.g. `RT-PNK-250G-WB-STD`.
pub fn sku_code(product_code: &str, weight: Weight, grind: Grind, pack: Pack) -> String {
  format!(
    "RT-{product_code}-{}-{}-{}",
    weight.sku_part(),
    grind.code(),
    pack.code()
  )
}

/// Roast level names, `roast_name(1)` = `浅烘` … `roast_name(5)` = `深烘`.
pub fn roast_name(level: u8) -> &'static str {
  match level {
    1 => "浅烘",
    2 => "中浅烘",
    3 => "中烘",
    4 => "中深烘",
    _ => "深烘",
  }
}
