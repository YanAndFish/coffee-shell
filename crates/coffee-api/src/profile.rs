//! The profile page and settings: `GET /v1/profile`, `/v1/settings`.

use serde::{Deserialize, Serialize};

use crate::{RememberStatus, User};

/// The three stat tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStats {
  /// 订单.
  pub orders: u32,
  /// 地址.
  pub addresses: u32,
  /// 优惠券.
  pub coupons: u32,
}

/// 我的口味画像.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Taste {
  /// Average flavor of what was bought.
  pub flavor: [u8; 6],
  /// `偏好`.
  pub prefer: String,
  /// `常买`.
  pub often: String,
  /// `少碰`.
  pub avoid: String,
}

/// A product link (`{商品名} →`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductRef {
  /// Product id.
  pub id: String,
  /// Its name.
  pub name: String,
}

/// `GET /v1/profile`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
  /// The member.
  pub user: User,
  /// 记住我 for this connection.
  pub remembered: RememberStatus,
  /// Stat tiles.
  pub stats: ProfileStats,
  /// `None` before the first order.
  pub taste: Option<Taste>,
  /// 0–2 products.
  pub recommendations: Vec<ProductRef>,
}

/// Per-account UI settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
  /// 界面动画 (`m`).
  pub animations: bool,
  /// 鼠标追踪 (`o`).
  pub mouse_tracking: bool,
}

/// Body of `PATCH /v1/settings` (at least one field).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
  /// New 界面动画.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub animations: Option<bool>,
  /// New 鼠标追踪.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mouse_tracking: Option<bool>,
}
