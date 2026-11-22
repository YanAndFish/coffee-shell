//! The backend API: `coffee_api`'s DTOs (re-exported here, so `coffee_app::api::ProductList`
//! is the frozen path it always was) and the client of one session.
//!
//! Every call is one [`Call`]: an [`Endpoint`] (method, access, which session headers it
//! takes), a path under [`API_PREFIX`] and an optional JSON body. [`ApiClient::send`] sends it
//! over nsh's `fetch` ([`Transport::Live`]) or answers it from the committed contract
//! fixtures ([`Transport::Fixture`], see [`FixtureSet`] for the scenarios), and [`decode`]
//! turns both answers into `T` or an [`ApiFailure`] — one decoding path, so the fixtures
//! exercise exactly what the network does. The typed methods (one per `/v1` operation) are
//! in `endpoints.rs`.
//!
//! Headers, as `packages/contract/README.md` says: `X-Internal-Key` on every call when the
//! host has one; `Authorization: Bearer <token>` on member (`auth: user`) calls once signed
//! in; `X-SSH-Fingerprint` on the calls that use it (register, login, resume, logout, me,
//! profile); `X-Client-IP` on the `/auth/*` calls; `Content-Type: application/json` with a
//! body.

mod endpoints;
mod fixtures;

use std::fmt;

pub use coffee_api::*;
use nsh_core::{platform::HttpRequest, prelude::*};
use serde::{Serialize, de::DeserializeOwned};

pub use endpoints::ENDPOINTS;
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
  /// The transport a configured URL names: `fixture:` followed by a scenario name
  /// ([`FixtureSet::parse`]: `guest`, `member`, `empty`, `down`, `error`) picks the
  /// fixtures; anything else is a live base URL.
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
    /// The envelope's machine-readable code, `not_found` (see [`ApiFailure::code`]).
    code: String,
    /// The envelope's message for people, `资源不存在`.
    message: String,
    /// The facts some codes carry (`out_of_stock` → product and stock, …).
    details: Option<Box<ApiErrorDetails>>,
  },
  /// The body is not what the contract says.
  Decode(String),
}

/// The copy of a failed request without an answer (copy.md, "校验错误").
pub const NETWORK_MESSAGE: &str = "连接服务器失败，请稍后重试";

impl ApiFailure {
  /// The failure the error envelope of `code` describes (as the backend would answer it).
  pub fn of(code: ErrorCode) -> Self {
    ApiFailure::Status {
      status: code.status(),
      code: code.as_str().to_owned(),
      message: code.message().to_owned(),
      details: None,
    }
  }

  /// The contract's code, when the backend answered one this build knows.
  pub fn code(&self) -> Option<ErrorCode> {
    match self {
      ApiFailure::Status { code, .. } => ErrorCode::parse(code),
      _ => None,
    }
  }

  /// Whether the backend answered `code`.
  pub fn is(&self, code: ErrorCode) -> bool {
    self.code() == Some(code)
  }

  /// The envelope's details, if any.
  pub fn details(&self) -> Option<&ApiErrorDetails> {
    match self {
      ApiFailure::Status { details, .. } => details.as_deref(),
      _ => None,
    }
  }

  /// No answer at all (the connection failed).
  pub fn is_network(&self) -> bool {
    matches!(self, ApiFailure::Network(_))
  }

  /// The member session is gone (`401 session_expired`, or `login_required` for a call
  /// that needed one): the UI drops to the login page (states.md, "登录已失效").
  pub fn ends_session(&self) -> bool {
    self.is(ErrorCode::SessionExpired) || self.is(ErrorCode::LoginRequired)
  }

  /// What a form or a toast says: the envelope's one message (`用户名或密码不正确`), the
  /// network copy, or the generic title for anything else.
  pub fn message(&self) -> String {
    match self {
      ApiFailure::Status { message, .. } if !message.is_empty() => message.clone(),
      ApiFailure::Network(_) => NETWORK_MESSAGE.to_owned(),
      other => other.title().to_owned(),
    }
  }

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
        ..
      } if message.is_empty() => format!("HTTP {status} · {code}"),
      ApiFailure::Status {
        status,
        code,
        message,
        ..
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
      details: envelope.error.details.map(Box::new),
    }),
    Err(_) => Err(ApiFailure::Status {
      status,
      code: format!("http_{status}"),
      message: String::new(),
      details: None,
    }),
  }
}

/// An HTTP method the client uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
  /// `GET`.
  Get,
  /// `POST`.
  Post,
  /// `PUT`.
  Put,
  /// `PATCH`.
  Patch,
  /// `DELETE`.
  Delete,
}

impl Method {
  /// The wire name.
  pub fn as_str(self) -> &'static str {
    match self {
      Method::Get => "GET",
      Method::Post => "POST",
      Method::Put => "PUT",
      Method::Patch => "PATCH",
      Method::Delete => "DELETE",
    }
  }
}

/// Who may call an endpoint (the contract's `auth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Access {
  /// Anyone (`auth: none`).
  Public,
  /// A member: the call carries `Authorization: Bearer` (`auth: user`).
  Member,
}

/// One `/v1` operation as the contract defines it (the endpoint table of
/// `packages/contract/README.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Endpoint {
  /// The contract's `operationId`, `getCart`.
  pub operation: &'static str,
  /// Its method.
  pub method: Method,
  /// Its path template under `/v1`, `/cart/lines/{sku}`.
  pub path: &'static str,
  /// Who may call it.
  pub access: Access,
  /// It takes `X-SSH-Fingerprint` (optional or required in the contract).
  pub fingerprint: bool,
  /// It takes `X-Client-IP` (the `/auth/*` calls).
  pub client_ip: bool,
}

/// One request: an endpoint, its concrete path (`/cart/lines/RT-PNK-250G-WB-STD`, with any
/// query) and its JSON body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
  /// What it calls.
  pub endpoint: Endpoint,
  /// The path under [`API_PREFIX`], with the query string if any.
  pub path: String,
  /// The JSON body.
  pub body: Option<Vec<u8>>,
}

impl Call {
  /// A call of `endpoint` at `path` without a body.
  pub fn new(endpoint: Endpoint, path: impl Into<String>) -> Self {
    Self {
      endpoint,
      path: path.into(),
      body: None,
    }
  }

  /// The call with `body` serialized as JSON.
  pub fn json(mut self, body: &impl Serialize) -> Self {
    self.body = Some(serde_json::to_vec(body).expect("request bodies serialize"));
    self
  }

  /// The path without its query string.
  pub fn route(&self) -> &str {
    self.path.split('?').next().unwrap_or(&self.path)
  }
}

/// The backend client of one session: the transport and the session's headers. `Clone` and
/// cheap; a task takes a copy when it starts (so a request started before logout keeps the
/// token it was started with).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiClient {
  transport: Transport,
  internal_key: Option<String>,
  fingerprint: Option<String>,
  client_ip: Option<String>,
  token: Option<String>,
}

impl ApiClient {
  /// A client over `transport`, sending `internal_key` as [`INTERNAL_KEY_HEADER`] when
  /// there is one.
  pub fn new(transport: Transport, internal_key: Option<String>) -> Self {
    Self {
      transport,
      internal_key,
      fingerprint: None,
      client_ip: None,
      token: None,
    }
  }

  /// The client sending `fingerprint` as [`SSH_FINGERPRINT_HEADER`] where the contract
  /// takes it.
  pub fn with_fingerprint(mut self, fingerprint: Option<String>) -> Self {
    self.fingerprint = fingerprint.filter(|f| !f.trim().is_empty());
    self
  }

  /// The client sending `client_ip` as [`CLIENT_IP_HEADER`] on the auth calls.
  pub fn with_client_ip(mut self, client_ip: Option<String>) -> Self {
    self.client_ip = client_ip.filter(|ip| !ip.trim().is_empty());
    self
  }

  /// The client of a member session (`token`), or of a guest (`None`).
  pub fn with_token(mut self, token: Option<String>) -> Self {
    self.token = token;
    self
  }

  /// Its transport.
  pub fn transport(&self) -> &Transport {
    &self.transport
  }

  /// The connection's SSH key fingerprint, if it has one.
  pub fn fingerprint(&self) -> Option<&str> {
    self.fingerprint.as_deref()
  }

  /// Whether it carries a member session.
  pub fn signed_in(&self) -> bool {
    self.token.is_some()
  }

  /// The headers `call` carries (besides `Accept` and `Content-Type`).
  pub fn headers(&self, call: &Call) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(key) = &self.internal_key {
      headers.push((INTERNAL_KEY_HEADER.to_owned(), key.clone()));
    }
    if call.endpoint.access == Access::Member
      && let Some(token) = &self.token
    {
      headers.push((AUTHORIZATION_HEADER.to_owned(), format!("Bearer {token}")));
    }
    if call.endpoint.fingerprint
      && let Some(fingerprint) = &self.fingerprint
    {
      headers.push((SSH_FINGERPRINT_HEADER.to_owned(), fingerprint.clone()));
    }
    if call.endpoint.client_ip
      && let Some(ip) = &self.client_ip
    {
      headers.push((CLIENT_IP_HEADER.to_owned(), ip.clone()));
    }
    headers
  }

  /// The live request for `call` against `base_url`: what [`Self::send`] sends over
  /// [`Transport::Live`].
  pub fn http_request(&self, base_url: &str, call: &Call) -> HttpRequest {
    let mut request = HttpRequest::new(
      call.endpoint.method.as_str(),
      format!("{base_url}{API_PREFIX}{}", call.path),
    )
    .header("Accept", "application/json");
    for (name, value) in self.headers(call) {
      request = request.header(name, value);
    }
    if let Some(body) = &call.body {
      request = request
        .header("Content-Type", "application/json")
        .body(body.clone());
    }
    request
  }

  /// Sends `call` and decodes the answer. Run it in a task (`spawn`); the fixture
  /// transport answers without awaiting anything.
  pub async fn send<T: DeserializeOwned>(&self, call: Call) -> Result<T, ApiFailure> {
    match &self.transport {
      Transport::Live(base_url) => {
        let response = fetch(self.http_request(base_url, &call))
          .await
          .map_err(ApiFailure::Network)?;
        decode(response.status(), &response.body)
      }
      Transport::Fixture(set) => {
        let answer = set.answer(&call, self)?;
        decode(answer.status, answer.body.as_bytes())
      }
    }
  }
}

/// The contract's username rule (U2, `USERNAME_PATTERN` = `^[\p{Script=Han}\w]{2,16}$`):
/// 2–16 characters, each a Han character, an ASCII letter, a digit or `_`.
pub fn username_is_valid(username: &str) -> bool {
  const HAN: [(char, char); 6] = [
    ('\u{2e80}', '\u{2fdf}'),
    ('\u{3005}', '\u{3007}'),
    ('\u{3400}', '\u{4dbf}'),
    ('\u{4e00}', '\u{9fff}'),
    ('\u{f900}', '\u{faff}'),
    ('\u{20000}', '\u{3134f}'),
  ];
  let count = username.chars().count();
  (2..=16).contains(&count)
    && username.chars().all(|c| {
      c.is_ascii_alphanumeric()
        || c == '_'
        || HAN.iter().any(|(from, to)| (*from..=*to).contains(&c))
    })
}

/// Polls `future` to its end on this thread, for futures that never wait (the fixture
/// transport's calls). Tests and the fixture scenarios use it.
///
/// # Panics
///
/// When `future` waits (a live request).
pub fn now_or_never<F: Future>(future: F) -> F::Output {
  let mut future = std::pin::pin!(future);
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  match future.as_mut().poll(&mut context) {
    std::task::Poll::Ready(output) => output,
    std::task::Poll::Pending => panic!("now_or_never: the future waited"),
  }
}

#[cfg(test)]
mod tests;
