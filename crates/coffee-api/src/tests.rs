//! Every contract fixture deserializes into its DTO and serializes back to the same JSON;
//! a fixture this crate has no DTO for fails.

use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::*;

const HEALTH: &str = include_str!("../../../packages/contract/fixtures/v1/health.json");
const PRODUCTS: &str = include_str!("../../../packages/contract/fixtures/v1/products.json");
const ERROR: &str = include_str!("../../../packages/contract/fixtures/v1/error.json");

fn fixtures_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/contract/fixtures")
}

/// Every fixture as (`v1/…` path, text), sorted.
fn all_fixtures() -> Vec<(String, String)> {
  fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
      let path = entry.unwrap().path();
      if path.is_dir() {
        walk(&path, root, out);
      } else if path.extension().is_some_and(|e| e == "json") {
        let rel = path
          .strip_prefix(root)
          .unwrap()
          .to_string_lossy()
          .replace('\\', "/");
        out.push((rel, std::fs::read_to_string(&path).unwrap()));
      }
    }
  }
  let root = fixtures_root();
  let mut out = Vec::new();
  walk(&root, &root, &mut out);
  out.sort();
  out
}

/// Numbers compared as f64 (`88` and `88.0` are the same score).
fn normalize(value: Value) -> Value {
  match value {
    Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
    Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
    Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
    other => other,
  }
}

fn round_trip<T: DeserializeOwned + Serialize>(text: &str) -> Result<(), String> {
  let typed: T = serde_json::from_str(text).map_err(|e| format!("decode: {e}"))?;
  let back = normalize(serde_json::to_value(&typed).map_err(|e| format!("encode: {e}"))?);
  let original = normalize(serde_json::from_str(text).unwrap());
  if back == original {
    Ok(())
  } else {
    Err(format!("round trip differs:\n{back}\n!=\n{original}"))
  }
}

/// The DTO of each fixture path.
fn check(path: &str, text: &str) -> Result<(), String> {
  let file = path.rsplit('/').next().unwrap_or(path);
  match path {
    "v1/health.json" => round_trip::<Health>(text),
    "v1/products.json" => round_trip::<ProductList>(text),
    "v1/error.json" => round_trip::<ApiError>(text),
    _ if path.starts_with("v1/errors/") => round_trip::<ApiError>(text),
    _ if path.starts_with("v1/auth/") => match file {
      "logout.json" => round_trip::<OkResponse>(text),
      "me.json" => round_trip::<Me>(text),
      _ => round_trip::<AuthSession>(text),
    },
    _ if path.starts_with("v1/catalog/products-") => round_trip::<ProductList>(text),
    _ if path.starts_with("v1/catalog/product-") => round_trip::<ProductDetail>(text),
    "v1/cart/merge.json" => round_trip::<CartMergeResult>(text),
    _ if path.starts_with("v1/cart/") => round_trip::<Cart>(text),
    _ if path.starts_with("v1/addresses/saved-") || path == "v1/addresses/updated.json" => {
      round_trip::<AddressSaved>(text)
    }
    _ if path.starts_with("v1/addresses/") => round_trip::<AddressList>(text),
    "v1/orders/placed.json" => round_trip::<PlacedOrder>(text),
    _ if path.starts_with("v1/orders/list") => round_trip::<OrderList>(text),
    _ if path.starts_with("v1/orders/order") => round_trip::<Order>(text),
    _ if path.starts_with("v1/profile/settings") => round_trip::<Settings>(text),
    _ if path.starts_with("v1/profile/profile") => round_trip::<Profile>(text),
    _ => Err("no DTO mapped for this fixture: add one in crates/coffee-api".into()),
  }
}

#[test]
fn every_fixture_round_trips_through_its_dto() {
  let fixtures = all_fixtures();
  assert!(
    fixtures.len() > 70,
    "found only {} fixtures",
    fixtures.len()
  );
  let failures: Vec<String> = fixtures
    .iter()
    .filter_map(|(path, text)| check(path, text).err().map(|e| format!("{path}: {e}")))
    .collect();
  assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn error_codes_match_the_contract() {
  let fixtures: Vec<_> = all_fixtures()
    .into_iter()
    .filter(|(path, _)| path.starts_with("v1/errors/"))
    .collect();
  assert_eq!(fixtures.len(), ErrorCode::ALL.len());
  for (path, text) in fixtures {
    let error: ApiError = serde_json::from_str(&text).unwrap();
    let code = error
      .error
      .error_code()
      .unwrap_or_else(|| panic!("{path}: unknown code"));
    assert_eq!(format!("v1/errors/{}.json", code.as_str()), path);
    assert_eq!(code.message(), error.error.message, "{path}");
  }
  assert_eq!(ErrorCode::parse("nope"), None);
  assert_eq!(ErrorCode::OutOfStock.status(), 409);
}

#[test]
fn prefix_is_v1() {
  assert_eq!(API_PREFIX, "/v1");
}

#[test]
fn health_round_trips() {
  let health: Health = serde_json::from_str(HEALTH).unwrap();
  assert!(health.ok && health.demo);
}

#[test]
fn products_deserialize() {
  let list: ProductList = serde_json::from_str(PRODUCTS).unwrap();
  assert_eq!(list.items.len(), 8);
  assert_eq!(list.items[0].id, "yirga");
  assert_eq!(list.items[0].flavor, [8, 7, 4, 2, 7, 9]);
  assert_eq!(list.items[0].default_weight, Weight::G250);
  assert!(list.items.iter().all(|p| (1..=5).contains(&p.roast)));
  assert!(list.items.iter().all(|p| p.flavor.iter().all(|&v| v <= 10)));
  assert_eq!(list.items[2].tag, None);
  assert_eq!(list.facets.total, 8);
  assert_eq!(list.facets.origins.len(), 8);
}

#[test]
fn error_deserializes() {
  let error: ApiError = serde_json::from_str(ERROR).unwrap();
  assert_eq!(error.error.code, "not_found");
  assert_eq!(error.error.message, "资源不存在");
  assert_eq!(error.error.details, None);
  assert_eq!(error.error.error_code(), Some(ErrorCode::NotFound));
}

#[test]
fn details_decode() {
  let text = include_str!("../../../packages/contract/fixtures/v1/errors/out_of_stock.json");
  let error: ApiError = serde_json::from_str(text).unwrap();
  let details = error.error.details.unwrap();
  assert_eq!(details.product_name.as_deref(), Some("波奎特 瑰夏"));
  assert_eq!(details.available, Some(6));
}

#[test]
fn product_detail_prices() {
  let text = include_str!("../../../packages/contract/fixtures/v1/catalog/product-geisha.json");
  let detail: ProductDetail = serde_json::from_str(text).unwrap();
  assert_eq!(detail.max_quantity, 6);
  let cell = detail.price(Weight::G100, Pack::Gift).unwrap();
  assert_eq!(cell.unit_price, 141);
  assert_eq!(cell.totals.len(), 6);
  assert!(!detail.weights[2].available);
  assert_eq!(
    sku_code(&detail.code, Weight::G100, Grind::WholeBean, Pack::Gift),
    "RT-GSH-100G-WB-GIFT"
  );
}

#[test]
fn requests_serialize_as_the_contract_expects() {
  let add = AddCartLineRequest {
    product_id: "pinkb".into(),
    weight: Weight::G250,
    grind: Grind::PourOver,
    pack: Pack::Standard,
    quantity: 1,
    index: None,
  };
  assert_eq!(
    serde_json::to_value(&add).unwrap(),
    serde_json::json!({"productId":"pinkb","weight":"250g","grind":"PO","pack":"STD","quantity":1})
  );
  let update = UpdateCartLineRequest {
    selected: Some(false),
    ..Default::default()
  };
  assert_eq!(
    serde_json::to_value(&update).unwrap(),
    serde_json::json!({"selected": false})
  );
  let address = AddressInput {
    recipient: "林晓".into(),
    region: "上海市 徐汇区".into(),
    detail: "天平路 88 弄".into(),
    phone: None,
    tag: AddressTag::Work,
    is_default: true,
  };
  assert_eq!(
    serde_json::to_value(&address).unwrap(),
    serde_json::json!({"recipient":"林晓","region":"上海市 徐汇区","detail":"天平路 88 弄","tag":"work","isDefault":true})
  );
  let order = PlaceOrderRequest {
    idempotency_key: "checkout-1".into(),
    expected_total: Some(426),
  };
  assert_eq!(
    serde_json::to_value(&order).unwrap(),
    serde_json::json!({"idempotencyKey":"checkout-1","expectedTotal":426})
  );
  let login = LoginRequest {
    username: "林晓".into(),
    password: "espresso42".into(),
    remember: true,
  };
  assert_eq!(
    serde_json::to_value(&login).unwrap(),
    serde_json::json!({"username":"林晓","password":"espresso42","remember":true})
  );
  let patch = SettingsPatch {
    mouse_tracking: Some(false),
    ..Default::default()
  };
  assert_eq!(
    serde_json::to_value(patch).unwrap(),
    serde_json::json!({"mouseTracking": false})
  );
}

#[test]
fn product_list_query_strings() {
  assert_eq!(ProductListQuery::default().to_query_string(), "");
  let query = ProductListQuery {
    origin: Some("巴拿马".into()),
    roast: Some(RoastBand::Light),
    sort: Some(ProductSort::PriceAsc),
    q: Some("茉莉 tea".into()),
  };
  assert_eq!(
    query.to_query_string(),
    "?origin=%E5%B7%B4%E6%8B%BF%E9%A9%AC&roast=light&sort=price_asc&q=%E8%8C%89%E8%8E%89%20tea"
  );
}

#[test]
fn labels() {
  assert_eq!(Grind::WholeBean.label(), "整豆");
  assert_eq!(Pack::Gift.label(), "礼盒装");
  assert_eq!(Weight::Kg1.label(), "1kg");
  assert_eq!(AddressTag::School.label(), "学校");
  assert_eq!(OrderStatus::Shipping.label(), "运输中");
  assert_eq!(roast_name(2), "中浅烘");
}
