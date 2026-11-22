//! The cart: member cart, guest quote and merge (`/v1/cart/*`).

use serde::{Deserialize, Serialize};

use crate::{Address, Grind, Pack, Weight};

/// Why a line cannot be checked out as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CartLineProblem {
  /// Its weight is sold out now.
  SoldOut,
  /// Its quantity is above the stock now.
  OutOfStock,
}

/// One cart line, priced by the server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartLine {
  /// The line's identity and `{sku}` path parameter.
  pub sku: String,
  /// The product.
  pub product_id: String,
  /// Its name.
  pub name: String,
  /// Its origin (`{产区} · {烘焙度名}`).
  pub origin: String,
  /// Its roast level.
  pub roast: u8,
  /// Weight.
  pub weight: Weight,
  /// Grind.
  pub grind: Grind,
  /// Pack.
  pub pack: Pack,
  /// 1–9.
  pub quantity: u8,
  /// Counted in the totals and checked out.
  pub selected: bool,
  /// `¥{单价}`.
  pub unit_price: u32,
  /// `¥{小计}`.
  pub line_total: u32,
  /// The stepper's upper bound.
  pub max_quantity: u8,
  /// A problem now, if any.
  pub problem: Option<CartLineProblem>,
}

/// A coupon in use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedCoupon {
  /// `TTY10`.
  pub code: String,
  /// `全单 9 折`.
  pub description: String,
}

/// Free shipping progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeShipping {
  /// `199`.
  pub threshold: u32,
  /// `已满 ¥199 · 本单包邮`.
  pub reached: bool,
  /// 0–100.
  pub progress: u32,
  /// `再买 ¥{gap} 即可包邮`.
  pub gap: u32,
}

/// The cart's amounts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartTotals {
  /// Σ quantity of all lines (`购物车 · {n} 件`, the nav badge).
  pub item_count: u32,
  /// Σ quantity of selected lines (`c 结算 {n} 件`).
  pub selected_count: u32,
  /// 商品小计.
  pub subtotal: u32,
  /// 优惠.
  pub discount: u32,
  /// 运费 (0 = `包邮`).
  pub shipping: u32,
  /// 合计.
  pub total: u32,
  /// Free shipping progress.
  pub free_shipping: FreeShipping,
}

/// A priced cart (member cart or guest quote).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cart {
  /// The lines, in order.
  pub lines: Vec<CartLine>,
  /// The coupon in use.
  pub coupon: Option<AppliedCoupon>,
  /// 本单寄往 (always `None` for guests).
  pub address: Option<Address>,
  /// Amounts.
  pub totals: CartTotals,
}

/// A line of a guest cart, held by the TUI until login.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestCartLine {
  /// The product.
  pub product_id: String,
  /// Weight.
  pub weight: Weight,
  /// Grind.
  pub grind: Grind,
  /// Pack.
  pub pack: Pack,
  /// 1–9.
  pub quantity: u8,
  /// Selected.
  pub selected: bool,
}

/// Body of `POST /v1/cart/quote`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartQuoteRequest {
  /// The guest lines (at most 50).
  pub lines: Vec<GuestCartLine>,
  /// The typed coupon, if any.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub coupon_code: Option<String>,
}

/// Body of `POST /v1/cart/merge`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartMergeRequest {
  /// The guest lines.
  pub lines: Vec<GuestCartLine>,
  /// The guest's coupon, applied when valid.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub coupon_code: Option<String>,
}

/// Result of `POST /v1/cart/merge`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartMergeResult {
  /// The member cart afterwards.
  pub cart: Cart,
  /// `已合并访客购物车 {n} 件` (0 = no merge toast).
  pub merged_count: u32,
  /// Guest lines dropped.
  pub skipped_count: u32,
}

/// Body of `POST /v1/cart/lines`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCartLineRequest {
  /// The product.
  pub product_id: String,
  /// Weight.
  pub weight: Weight,
  /// Grind.
  pub grind: Grind,
  /// Pack.
  pub pack: Pack,
  /// 1–9.
  pub quantity: u8,
  /// Position for a new line (undo of a delete); `None` = at the end.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub index: Option<u32>,
}

/// Body of `PATCH /v1/cart/lines/{sku}` (at least one field).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCartLineRequest {
  /// New quantity.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub quantity: Option<u8>,
  /// New selection.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub selected: Option<bool>,
}

/// Body of `PUT /v1/cart/selection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartSelectionRequest {
  /// Select (`true`) or unselect every line.
  pub selected: bool,
}

/// Body of `PUT /v1/cart/coupon`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyCouponRequest {
  /// As typed; empty removes the coupon.
  pub code: String,
}

/// Body of `PUT /v1/cart/address`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCartAddressRequest {
  /// One of the member's addresses.
  pub address_id: u32,
}
