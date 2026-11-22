//! Serde DTOs of the ROAST/TTY backend's `/v1` HTTP API.
//!
//! Hand-written to match `packages/contract/fixtures/v1/*.json` exactly; the tests below
//! deserialize every fixture. JSON keys are camelCase and money is integer yuan.
//! This crate must not depend on nsh.

use serde::{Deserialize, Serialize};

/// The API version prefix every backend route lives under.
pub const API_PREFIX: &str = "/v1";

/// `GET /v1/health`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
  /// Always `true` when the backend answers.
  pub ok: bool,
  /// The service name, `roast-tty`.
  pub service: String,
  /// The backend's version.
  pub version: String,
  /// Always `true`: this is a demo shop.
  pub demo: bool,
}

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
  /// Processing method, e.g. `水洗`.
  pub process: String,
  /// Roast level, 1 (light) to 5 (dark).
  pub roast: u8,
  /// Badge such as `新产季`, or `None`.
  pub tag: Option<String>,
  /// Tasting notes.
  pub notes: Vec<String>,
  /// Flavor profile 0..=10 in the order 酸度, 甜感, 醇厚, 苦度, 余韵, 香气.
  pub flavor: [u8; 6],
  /// Cupping score, e.g. `87.5`.
  pub score: f64,
  /// Base price of 250 g, integer yuan.
  pub price250: u32,
  /// Units in stock.
  pub stock: u32,
}

/// `GET /v1/products`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductList {
  /// The products, in display order.
  pub items: Vec<ProductSummary>,
}

/// The envelope of every non-2xx response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
  /// What went wrong.
  pub error: ApiErrorBody,
}

/// The body of an [`ApiError`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorBody {
  /// Machine-readable code, e.g. `not_found`.
  pub code: String,
  /// Human-readable message (Chinese), e.g. `资源不存在`.
  pub message: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  const HEALTH: &str = include_str!("../../../packages/contract/fixtures/v1/health.json");
  const PRODUCTS: &str = include_str!("../../../packages/contract/fixtures/v1/products.json");
  const ERROR: &str = include_str!("../../../packages/contract/fixtures/v1/error.json");

  #[test]
  fn prefix_is_v1() {
    assert_eq!(API_PREFIX, "/v1");
  }

  #[test]
  fn health_round_trips() {
    let health: Health = serde_json::from_str(HEALTH).unwrap();
    assert!(health.ok && health.demo);
    let original: serde_json::Value = serde_json::from_str(HEALTH).unwrap();
    assert_eq!(serde_json::to_value(&health).unwrap(), original);
  }

  #[test]
  fn products_deserialize() {
    let list: ProductList = serde_json::from_str(PRODUCTS).unwrap();
    assert_eq!(list.items.len(), 8);
    assert_eq!(list.items[0].id, "yirga");
    assert_eq!(list.items[0].flavor, [8, 7, 4, 2, 7, 9]);
    assert!(list.items.iter().all(|p| (1..=5).contains(&p.roast)));
    assert!(list.items.iter().all(|p| p.flavor.iter().all(|&v| v <= 10)));
    assert_eq!(list.items[2].tag, None);
  }

  #[test]
  fn error_deserializes() {
    let error: ApiError = serde_json::from_str(ERROR).unwrap();
    assert_eq!(error.error.code, "not_found");
    assert_eq!(error.error.message, "资源不存在");
  }
}
