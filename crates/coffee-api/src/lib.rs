//! Serde DTOs of the ROAST/TTY backend's `/v1` HTTP API.
//!
//! Hand-written to match `packages/contract` (the zod schemas are the source of truth); the
//! tests deserialize every file in `packages/contract/fixtures/v1/` and serialize it back.
//! JSON keys are camelCase, money is integer yuan computed by the server (never by the TUI),
//! `*_at` fields are UTC instants and `*_date` / `*_on` fields are shop-local dates
//! (`YYYY-MM-DD`, Asia/Shanghai), both kept as strings. This crate must not depend on nsh.
//!
//! One module per contract area ([`auth`], [`catalog`], [`cart`], [`addresses`], [`orders`],
//! [`profile`]) plus [`sku`]; everything is re-exported at the root.

pub mod addresses;
pub mod auth;
pub mod cart;
pub mod catalog;
pub mod orders;
pub mod profile;
pub mod sku;

pub use addresses::*;
pub use auth::*;
pub use cart::*;
pub use catalog::*;
pub use orders::*;
pub use profile::*;
pub use sku::*;

use serde::{Deserialize, Serialize};

/// The API version prefix every backend route lives under.
pub const API_PREFIX: &str = "/v1";

/// The gateway ↔ backend secret header.
pub const INTERNAL_KEY_HEADER: &str = "X-Internal-Key";

/// The session's SSH key fingerprint (`SHA256:…`), from `COFFEE_SSH_FINGERPRINT`.
pub const SSH_FINGERPRINT_HEADER: &str = "X-SSH-Fingerprint";

/// The SSH client's address, from `COFFEE_CLIENT_IP` (rate limits and logs only).
pub const CLIENT_IP_HEADER: &str = "X-Client-IP";

/// `Authorization: Bearer <token>` carries the member session.
pub const AUTHORIZATION_HEADER: &str = "Authorization";

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

/// `{ "ok": true }`: a success with nothing else to say (`POST /v1/auth/logout`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkResponse {
  /// Always `true`.
  pub ok: bool,
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
  /// Machine-readable code, e.g. `not_found` (see [`ErrorCode`]); kept as a string so an
  /// unknown code still decodes.
  pub code: String,
  /// The code's one Chinese message (show it as is; forms prefix `✗ `).
  pub message: String,
  /// Facts some codes carry.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub details: Option<ApiErrorDetails>,
}

impl ApiErrorBody {
  /// The code as an [`ErrorCode`], `None` for a code this build does not know.
  pub fn error_code(&self) -> Option<ErrorCode> {
    ErrorCode::parse(&self.code)
  }
}

/// Structured facts of an error (`validation_failed` → `field`; `out_of_stock` →
/// `product_id`, `product_name`, `available`; `sold_out` → `product_id`, `product_name`,
/// `weight`; `rate_limited` → `retry_after`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorDetails {
  /// Dotted path of the offending input, e.g. `body.lines.0.weight`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub field: Option<String>,
  /// The product concerned.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub product_id: Option<String>,
  /// Its name, for `{商品名} 仅剩 {n} 份`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub product_name: Option<String>,
  /// The sold-out weight.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub weight: Option<Weight>,
  /// Units in stock.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub available: Option<u32>,
  /// Seconds before retrying.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub retry_after: Option<u32>,
}

macro_rules! error_codes {
  ($($variant:ident = $code:literal, $status:literal, $message:literal;)*) => {
    /// Every error code of the contract (`ERRORS` in `packages/contract/src/common.ts`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ErrorCode {
      $(
        #[doc = concat!("`", $code, "` (HTTP ", stringify!($status), "): ", $message)]
        $variant,
      )*
    }

    impl ErrorCode {
      /// Every code, in the contract's order.
      pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant),*];

      /// The wire code, e.g. `not_found`.
      pub fn as_str(self) -> &'static str {
        match self { $(ErrorCode::$variant => $code,)* }
      }

      /// The HTTP status the backend answers it with.
      pub fn status(self) -> u16 {
        match self { $(ErrorCode::$variant => $status,)* }
      }

      /// Its one Chinese message.
      pub fn message(self) -> &'static str {
        match self { $(ErrorCode::$variant => $message,)* }
      }

      /// The code named `code`, if known.
      pub fn parse(code: &str) -> Option<Self> {
        match code { $($code => Some(ErrorCode::$variant),)* _ => None }
      }
    }
  };
}

error_codes! {
  BadRequest = "bad_request", 400, "请求格式错误";
  ValidationFailed = "validation_failed", 400, "请求参数不合法";
  Unauthorized = "unauthorized", 401, "缺少或错误的内部密钥";
  NotFound = "not_found", 404, "资源不存在";
  MethodNotAllowed = "method_not_allowed", 405, "不支持该请求方法";
  RateLimited = "rate_limited", 429, "操作太频繁，请稍后再试";
  InternalError = "internal_error", 500, "服务器内部错误";
  NotImplemented = "not_implemented", 501, "功能尚未实现";
  LoginRequired = "login_required", 401, "请先登录";
  SessionExpired = "session_expired", 401, "登录已失效";
  UsernameRequired = "username_required", 422, "请输入用户名";
  UsernameInvalid = "username_invalid", 422, "用户名需为 2–16 个字符，可用中文、字母、数字和下划线";
  PasswordTooShort = "password_too_short", 422, "密码至少 6 位";
  PasswordTooLong = "password_too_long", 422, "密码最多 128 位";
  UsernameTaken = "username_taken", 409, "这个用户名已被注册";
  InvalidCredentials = "invalid_credentials", 401, "用户名或密码不正确";
  FingerprintRequired = "fingerprint_required", 400, "当前连接没有 SSH 公钥，无法记住登录";
  NotRemembered = "not_remembered", 404, "当前公钥没有记住的登录";
  QuantityInvalid = "quantity_invalid", 422, "数量应为 1–9";
  SoldOut = "sold_out", 409, "该重量暂时缺货";
  OutOfStock = "out_of_stock", 409, "库存不足";
  CartFull = "cart_full", 409, "购物车最多放 50 种规格";
  CouponInvalid = "coupon_invalid", 422, "优惠码无效或已过期";
  EmptySelection = "empty_selection", 409, "还没有勾选商品";
  AddressRequired = "address_required", 409, "请先选择收货地址";
  PriceChanged = "price_changed", 409, "价格有变化，请确认后再结算";
  RecipientRequired = "recipient_required", 422, "请填写收货人";
  RegionRequired = "region_required", 422, "请填写所在地区";
  DetailRequired = "detail_required", 422, "请填写详细地址";
  PhoneInvalid = "phone_invalid", 422, "手机号应为 11 位数字";
  TextTooLong = "text_too_long", 422, "内容太长了";
  AddressLimit = "address_limit", 409, "地址最多保存 20 条";
}

/// Percent-encodes `value` for a URL query (RFC 3986 unreserved characters stay).
pub fn encode_query_value(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for byte in value.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
      _ => out.push_str(&format!("%{byte:02X}")),
    }
  }
  out
}

#[cfg(test)]
mod tests;
