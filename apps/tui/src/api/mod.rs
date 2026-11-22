//! The backend API: `coffee_api`'s DTOs (re-exported here, so `coffee_app::api::ProductList`
//! is the frozen path it always was) and the client: every call goes through
//! [`ApiClient::request`], over nsh's `fetch` ([`Transport::Live`]) or the committed contract
//! fixtures ([`Transport::Fixture`]).
//!
//! A non-2xx answer carries the error envelope
//! (`{"error":{"code","message"}}`), decoded into [`ApiFailure::Status`]; a lost connection
//! is [`ApiFailure::Network`]; a body that is not the contract is [`ApiFailure::Decode`].
//! Decoding is one function ([`decode`]) for both transports, so the fixtures exercise the
//! same path as the network.

mod fixtures;

use std::fmt;

pub use coffee_api::*;
use nsh_core::{platform::HttpRequest, prelude::*};
use serde::de::DeserializeOwned;

pub use fixtures::FixtureSet;

/// Where requests go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transport {
  /// The backend at this base URL (`http://127.0.0.1:3000`), through nsh's `fetch`.
  Live(String),
  /// The contract fixtures (`packages/contract/fixtures/v1`), answered in process: what
  /// tests and `nsh run` use, deterministic and offline.
  Fixture(FixtureSet),
}

impl Transport {
  /// The transport a configured URL names: `fixture:` (or `fixture:contract`),
  /// `fixture:empty` and `fixture:down` pick the fixtures; anything else is a live base URL.
  pub fn from_url(url: &str) -> Self {
    match url.trim().strip_prefix("fixture:") {
      Some(set) => Transport::Fixture(FixtureSet::parse(set)),
      None => Transport::Live(url.trim().trim_end_matches('/').to_owned()),
    }
  }
}

/// Why a request has no usable answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiFailure {
  /// No answer: the connection failed or the host is not granted (nsh's message).
  Network(String),
  /// The backend answered `status` with its error envelope (or without one: then `code`
  /// is `http_<status>` and `message` is empty).
  Status {
    /// The HTTP status.
    status: u16,
    /// The envelope's machine-readable code, `not_found`.
    code: String,
    /// The envelope's message for people, `资源不存在`.
    message: String,
  },
  /// The body is not what the contract says.
  Decode(String),
}

impl ApiFailure {
  /// A one-line title for people.
  pub fn title(&self) -> &'static str {
    match self {
      ApiFailure::Network(_) => "连不上商店后端",
      ApiFailure::Status { .. } => "商店后端返回了错误",
      ApiFailure::Decode(_) => "商店后端的应答看不懂",
    }
  }

  /// The detail for people: the cause as precisely as we know it.
  pub fn detail(&self) -> String {
    match self {
      ApiFailure::Network(message) => format!("网络错误：{message}"),
      ApiFailure::Status {
        status,
        code,
        message,
      } if message.is_empty() => format!("HTTP {status} · {code}"),
      ApiFailure::Status {
        status,
        code,
        message,
      } => format!("{message}（HTTP {status} · {code}）"),
      ApiFailure::Decode(why) => format!("数据格式不符：{why}"),
    }
  }
}

impl fmt::Display for ApiFailure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}：{}", self.title(), self.detail())
  }
}

impl std::error::Error for ApiFailure {}

/// Turns an answer (`status`, `body`) into `T`, or the failure it reports.
pub fn decode<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, ApiFailure> {
  if (200..300).contains(&status) {
    return serde_json::from_slice(body).map_err(|error| ApiFailure::Decode(error.to_string()));
  }
  match serde_json::from_slice::<ApiError>(body) {
    Ok(envelope) => Err(ApiFailure::Status {
      status,
      code: envelope.error.code,
      message: envelope.error.message,
    }),
    Err(_) => Err(ApiFailure::Status {
      status,
      code: format!("http_{status}"),
      message: String::new(),
    }),
  }
}

/// An HTTP method the client uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
  /// `GET`.
  Get,
  /// `POST`.
  Post,
}

impl Method {
  fn as_str(self) -> &'static str {
    match self {
      Method::Get => "GET",
      Method::Post => "POST",
    }
  }
}

/// The backend client of one session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiClient {
  transport: Transport,
  internal_key: Option<String>,
}

impl ApiClient {
  /// A client over `transport`, sending `internal_key` as [`INTERNAL_KEY_HEADER`] when
  /// there is one.
  pub fn new(transport: Transport, internal_key: Option<String>) -> Self {
    Self {
      transport,
      internal_key,
    }
  }

  /// Its transport.
  pub fn transport(&self) -> &Transport {
    &self.transport
  }

  /// The live request for `method` `path` (`/products`, under [`API_PREFIX`]) with JSON
  /// `body`: what [`Self::request`] sends over [`Transport::Live`].
  pub fn http_request(
    &self,
    base_url: &str,
    method: Method,
    path: &str,
    body: Option<Vec<u8>>,
  ) -> HttpRequest {
    let mut request = HttpRequest::new(method.as_str(), format!("{base_url}{API_PREFIX}{path}"))
      .header("Accept", "application/json");
    if let Some(key) = &self.internal_key {
      request = request.header(INTERNAL_KEY_HEADER, key.clone());
    }
    if let Some(body) = body {
      request = request
        .header("Content-Type", "application/json")
        .body(body);
    }
    request
  }

  /// Sends `method` `path` (relative to [`API_PREFIX`]) with JSON `body` and decodes the
  /// answer. Run it in a task (`spawn`): dropping the task cancels the request.
  pub async fn request<T: DeserializeOwned>(
    &self,
    method: Method,
    path: &str,
    body: Option<Vec<u8>>,
  ) -> Result<T, ApiFailure> {
    match &self.transport {
      Transport::Live(base_url) => {
        let response = fetch(self.http_request(base_url, method, path, body))
          .await
          .map_err(ApiFailure::Network)?;
        decode(response.status(), &response.body)
      }
      Transport::Fixture(set) => {
        let answer = set.answer(method, path)?;
        decode(answer.status, answer.body.as_bytes())
      }
    }
  }

  /// `GET /v1/health`.
  pub async fn health(&self) -> Result<Health, ApiFailure> {
    self.request(Method::Get, "/health", None).await
  }

  /// `GET /v1/products`.
  pub async fn products(&self) -> Result<ProductList, ApiFailure> {
    self.request(Method::Get, "/products", None).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn urls_pick_the_transport() {
    assert_eq!(
      Transport::from_url("http://127.0.0.1:3000/"),
      Transport::Live("http://127.0.0.1:3000".into())
    );
    assert_eq!(
      Transport::from_url("fixture:"),
      Transport::Fixture(FixtureSet::Contract)
    );
    assert_eq!(
      Transport::from_url("fixture:empty"),
      Transport::Fixture(FixtureSet::Empty)
    );
    assert_eq!(
      Transport::from_url("fixture:down"),
      Transport::Fixture(FixtureSet::Down)
    );
  }

  #[test]
  fn success_decodes_the_contract() {
    let list: ProductList = decode(200, fixtures::PRODUCTS.as_bytes()).unwrap();
    assert_eq!(list.items.len(), 8);
    assert_eq!(list.items[0].name, "耶加雪菲 孔加");
    let health: Health = decode(200, fixtures::HEALTH.as_bytes()).unwrap();
    assert!(health.demo);
  }

  #[test]
  fn the_error_envelope_becomes_a_typed_failure() {
    let failure = decode::<ProductList>(404, fixtures::ERROR.as_bytes()).unwrap_err();
    assert_eq!(
      failure,
      ApiFailure::Status {
        status: 404,
        code: "not_found".into(),
        message: "资源不存在".into(),
      }
    );
    assert_eq!(failure.detail(), "资源不存在（HTTP 404 · not_found）");
  }

  #[test]
  fn a_status_without_envelope_keeps_the_status() {
    let failure = decode::<ProductList>(502, b"Bad Gateway").unwrap_err();
    assert_eq!(
      failure,
      ApiFailure::Status {
        status: 502,
        code: "http_502".into(),
        message: String::new(),
      }
    );
    assert_eq!(failure.detail(), "HTTP 502 · http_502");
  }

  #[test]
  fn a_body_off_contract_is_a_decode_failure() {
    let failure = decode::<ProductList>(200, br#"{"items":[{"id":1}]}"#).unwrap_err();
    assert!(matches!(failure, ApiFailure::Decode(_)));
    assert!(failure.to_string().starts_with("商店后端的应答看不懂"));
  }

  #[test]
  fn live_requests_carry_the_internal_key() {
    let client = ApiClient::new(
      Transport::Live("http://127.0.0.1:3000".into()),
      Some("s3cret".into()),
    );
    let request = client.http_request("http://127.0.0.1:3000", Method::Get, "/products", None);
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "http://127.0.0.1:3000/v1/products");
    assert!(
      request
        .headers
        .contains(&(INTERNAL_KEY_HEADER.to_owned(), "s3cret".to_owned()))
    );
    let anonymous = ApiClient::new(Transport::Live("http://h".into()), None);
    let request = anonymous.http_request("http://h", Method::Post, "/cart", Some(b"{}".to_vec()));
    assert!(
      !request
        .headers
        .iter()
        .any(|(name, _)| name == INTERNAL_KEY_HEADER)
    );
    assert_eq!(request.method, "POST");
  }
}
