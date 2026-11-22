//! Orders: `/v1/orders`.

use serde::{Deserialize, Serialize};

use crate::{Cart, Grind, Pack, Weight};

/// Order status, derived by the server from the time of reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
  /// `● 烘焙中` (first 24 h).
  Roasting,
  /// `● 运输中` (until the ETA).
  Shipping,
  /// `● 已签收`.
  Delivered,
}

impl OrderStatus {
  /// The display label.
  pub fn label(self) -> &'static str {
    match self {
      OrderStatus::Roasting => "烘焙中",
      OrderStatus::Shipping => "运输中",
      OrderStatus::Delivered => "已签收",
    }
  }
}

/// One purchased line (a snapshot).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderItem {
  /// SKU code.
  pub sku: String,
  /// The product.
  pub product_id: String,
  /// Name at checkout.
  pub name: String,
  /// Weight.
  pub weight: Weight,
  /// Grind.
  pub grind: Grind,
  /// Pack.
  pub pack: Pack,
  /// 1–9.
  pub quantity: u8,
  /// Integer yuan.
  pub unit_price: u32,
  /// Integer yuan.
  pub line_total: u32,
}

/// The shipping address (a snapshot).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAddress {
  /// 收货人.
  pub recipient: String,
  /// Full phone, or `None`.
  pub phone: Option<String>,
  /// Masked phone, or `None`.
  pub phone_masked: Option<String>,
  /// 所在地区.
  pub region: String,
  /// 详细地址.
  pub detail: String,
}

/// One order: the receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
  /// `RT2611220417`.
  pub number: String,
  /// UTC instant.
  pub created_at: String,
  /// Shop-local date (`11-18` in lists).
  pub order_date: String,
  /// UTC instant.
  pub eta_at: String,
  /// Shop-local date: `预计 {M} 月 {D} 日 送达`.
  pub eta_date: String,
  /// Status now.
  pub status: OrderStatus,
  /// The lines.
  pub items: Vec<OrderItem>,
  /// Integer yuan.
  pub subtotal: u32,
  /// Integer yuan.
  pub discount: u32,
  /// Integer yuan.
  pub shipping: u32,
  /// 实付.
  pub total: u32,
  /// The coupon used, if any.
  pub coupon_code: Option<String>,
  /// 寄往.
  pub address: OrderAddress,
}

/// A line of an order-list row: `{name} {weight} ×{quantity}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderSummaryLine {
  /// The product.
  pub product_id: String,
  /// Name at checkout.
  pub name: String,
  /// Weight.
  pub weight: Weight,
  /// 1–9.
  pub quantity: u8,
}

/// One row of `最近订单 ORDERS`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderSummary {
  /// Order number.
  pub number: String,
  /// UTC instant.
  pub created_at: String,
  /// Shop-local date.
  pub order_date: String,
  /// Status now.
  pub status: OrderStatus,
  /// 金额.
  pub total: u32,
  /// Σ quantity.
  pub item_count: u32,
  /// The lines.
  pub lines: Vec<OrderSummaryLine>,
}

/// `GET /v1/orders`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderList {
  /// Newest first.
  pub items: Vec<OrderSummary>,
  /// All orders of the member.
  pub total: u32,
}

/// Body of `POST /v1/orders`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderRequest {
  /// 8–64 of `[A-Za-z0-9_-]`, one per checkout attempt (reuse it on retry).
  pub idempotency_key: String,
  /// The total the member saw; a different server total is `409 price_changed`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub expected_total: Option<u32>,
}

/// Result of `POST /v1/orders`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedOrder {
  /// The receipt.
  pub order: Order,
  /// The cart afterwards.
  pub cart: Cart,
}
