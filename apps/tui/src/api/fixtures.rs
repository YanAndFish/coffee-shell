//! The fixture transport: every `/v1` endpoint answered in process from the committed
//! contract fixtures (`packages/contract/fixtures/v1/**`), so screens run and are tested
//! with no backend. Deterministic: the answer depends only on the scenario ([`FixtureSet`]),
//! the call (method, path, body) and the session headers the client would send (a token,
//! a key fingerprint).
//!
//! The fixtures are static examples, so a cart does not change with what was added: an add
//! answers `cart/added.json`, a delete `cart/removed.json`, and so on. The rules beyond "the
//! endpoint's fixture" are listed on [`FixtureSet`]; the product list is the one derived
//! answer (filtered, sorted and searched from `products.json` as the server does, P-api-9;
//! tests check it equals the committed list fixtures).

use std::borrow::Cow;

use coffee_api::*;

use super::{Access, ApiClient, ApiFailure, Call, Method, username_is_valid};

macro_rules! fixture_files {
  ($($name:ident = $path:literal;)*) => {
    $(
      #[doc = concat!("`fixtures/v1/", $path, "`.")]
      pub(crate) const $name: &str =
        include_str!(concat!("../../../../packages/contract/fixtures/v1/", $path));
    )*

    /// Every fixture file, by its path under `fixtures/v1/`.
    pub(crate) const FILES: &[(&str, &str)] = &[$(($path, $name)),*];
  };
}

fixture_files! {
  HEALTH = "health.json";
  PRODUCTS = "products.json";
  ERROR = "error.json";
  AUTH_LOGIN = "auth/login.json";
  AUTH_LOGIN_NO_KEY = "auth/login-no-key.json";
  AUTH_LOGOUT = "auth/logout.json";
  AUTH_ME = "auth/me.json";
  AUTH_REGISTER = "auth/register.json";
  AUTH_RESUME = "auth/resume.json";
  PRODUCT_ANTIGUA = "catalog/product-antigua.json";
  PRODUCT_GEISHA = "catalog/product-geisha.json";
  PRODUCT_MANDH = "catalog/product-mandh.json";
  PRODUCT_NIGHT = "catalog/product-night.json";
  PRODUCT_NYERI = "catalog/product-nyeri.json";
  PRODUCT_PINKB = "catalog/product-pinkb.json";
  PRODUCT_YIRGA = "catalog/product-yirga.json";
  PRODUCT_YUNNAN = "catalog/product-yunnan.json";
  PRODUCT_NO_STOCK = "catalog/detail-no-stock.json";
  PRODUCTS_EMPTY = "catalog/products-empty.json";
  PRODUCTS_LIGHT_BY_PRICE = "catalog/products-light-by-price.json";
  PRODUCTS_SEARCH = "catalog/products-search.json";
  CART = "cart/cart.json";
  CART_ADDED = "cart/added.json";
  CART_ADDRESS_SET = "cart/address-set.json";
  CART_COUPON = "cart/cart-coupon.json";
  CART_EMPTY = "cart/cart-empty.json";
  CART_NO_ADDRESS = "cart/cart-no-address.json";
  CART_PARTLY_SELECTED = "cart/cart-partly-selected.json";
  CART_PROBLEM = "cart/cart-problem.json";
  CART_COUPON_REMOVED = "cart/coupon-removed.json";
  CART_GUEST_QUOTE = "cart/guest-quote.json";
  CART_MERGE = "cart/merge.json";
  CART_NONE_SELECTED = "cart/none-selected.json";
  CART_REMOVED = "cart/removed.json";
  ADDRESSES_DEFAULT_MOVED = "addresses/default-moved.json";
  ADDRESSES_DELETED_DEFAULT = "addresses/deleted-default.json";
  ADDRESSES_EMPTY = "addresses/list-empty.json";
  ADDRESSES_ORDER_ELSEWHERE = "addresses/list-order-elsewhere.json";
  ADDRESSES = "addresses/list.json";
  ADDRESS_SAVED_FIRST = "addresses/saved-first.json";
  ADDRESS_SAVED_NO_PHONE = "addresses/saved-no-phone.json";
  ADDRESS_UPDATED = "addresses/updated.json";
  ORDERS_EMPTY = "orders/list-empty.json";
  ORDERS = "orders/list.json";
  ORDER_COUPON = "orders/order-coupon.json";
  ORDER = "orders/order.json";
  ORDER_PLACED = "orders/placed.json";
  PROFILE_NEW = "profile/profile-new.json";
  PROFILE_NO_KEY = "profile/profile-no-key.json";
  PROFILE_ORDERS_NO_KEY = "profile/profile-orders-no-key.json";
  PROFILE = "profile/profile.json";
  SETTINGS_ANIMATIONS_OFF = "profile/settings-animations-off.json";
  SETTINGS = "profile/settings.json";
  E_ADDRESS_LIMIT = "errors/address_limit.json";
  E_ADDRESS_REQUIRED = "errors/address_required.json";
  E_BAD_REQUEST = "errors/bad_request.json";
  E_CART_FULL = "errors/cart_full.json";
  E_COUPON_INVALID = "errors/coupon_invalid.json";
  E_DETAIL_REQUIRED = "errors/detail_required.json";
  E_EMPTY_SELECTION = "errors/empty_selection.json";
  E_FINGERPRINT_REQUIRED = "errors/fingerprint_required.json";
  E_INTERNAL_ERROR = "errors/internal_error.json";
  E_INVALID_CREDENTIALS = "errors/invalid_credentials.json";
  E_LOGIN_REQUIRED = "errors/login_required.json";
  E_METHOD_NOT_ALLOWED = "errors/method_not_allowed.json";
  E_NOT_FOUND = "errors/not_found.json";
  E_NOT_IMPLEMENTED = "errors/not_implemented.json";
  E_NOT_REMEMBERED = "errors/not_remembered.json";
  E_OUT_OF_STOCK = "errors/out_of_stock.json";
  E_PASSWORD_TOO_LONG = "errors/password_too_long.json";
  E_PASSWORD_TOO_SHORT = "errors/password_too_short.json";
  E_PHONE_INVALID = "errors/phone_invalid.json";
  E_PRICE_CHANGED = "errors/price_changed.json";
  E_QUANTITY_INVALID = "errors/quantity_invalid.json";
  E_RATE_LIMITED = "errors/rate_limited.json";
  E_RECIPIENT_REQUIRED = "errors/recipient_required.json";
  E_REGION_REQUIRED = "errors/region_required.json";
  E_SESSION_EXPIRED = "errors/session_expired.json";
  E_SOLD_OUT = "errors/sold_out.json";
  E_TEXT_TOO_LONG = "errors/text_too_long.json";
  E_UNAUTHORIZED = "errors/unauthorized.json";
  E_USERNAME_INVALID = "errors/username_invalid.json";
  E_USERNAME_REQUIRED = "errors/username_required.json";
  E_USERNAME_TAKEN = "errors/username_taken.json";
  E_VALIDATION_FAILED = "errors/validation_failed.json";
}

/// In every fixture scenario, logging in with this password answers
/// `401 invalid_credentials` (any other valid password signs in as `林晓`).
pub const WRONG_PASSWORD: &str = "wrongpass";

/// In every fixture scenario, registering this name (any letter case) answers
/// `409 username_taken`.
pub const TAKEN_USERNAME: &str = "林晓";

/// Which answers the fixture transport gives: a scenario. Answers are static: a change (an
/// add, a delete, a saved address) answers its fixture and is not remembered by later reads.
///
/// Common to all but [`FixtureSet::Down`] and [`FixtureSet::Error`]:
///
/// - `auth: user` endpoints without a token answer `401 login_required`.
/// - Every member is `林晓` (the contract checks every member fixture names her).
/// - Register validates as the server does (`username_required`, `username_invalid`,
///   `password_too_short`, `password_too_long`), [`TAKEN_USERNAME`] is `username_taken`,
///   else `auth/register.json` with the typed username. Login validates the same way,
///   [`WRONG_PASSWORD`] is `invalid_credentials`, else `auth/login.json`
///   (`auth/login-no-key.json` without a key).
/// - Resume without a key is `400 fingerprint_required`.
/// - The product list is filtered, sorted and searched from `products.json`; every listed
///   product has its detail (`catalog/product-<id>.json`); other ids are `404 not_found`.
/// - Cart: add → `added.json`; unselecting a line → `cart-partly-selected.json`, any other
///   line change → `cart.json`; delete → `removed.json`; unselect all → `none-selected.json`,
///   select all → `cart.json`; coupon `TTY10` (any case, trimmed) → `cart-coupon.json`, empty
///   → `coupon-removed.json`, anything else `422 coupon_invalid`; address 1–3 →
///   `address-set.json`; a guest quote → `guest-quote.json` (`cart-empty.json` for no lines);
///   merge → `merge.json`. Unknown products and SKUs are `404 not_found`, quantities outside
///   1–9 `422 quantity_invalid`.
/// - Addresses validate (`recipient_required`, `region_required`, `detail_required`,
///   `phone_invalid`) and answer their fixtures; ids 1–3 exist.
/// - Checkout → `orders/placed.json` (an idempotency key outside 8–64 characters is
///   `400 validation_failed`).
/// - The profile is `profile/profile.json`, `profile/profile-orders-no-key.json` without a
///   key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FixtureSet {
  /// `fixture:` / `fixture:guest` / `fixture:contract`: a new visitor. Its key (if the
  /// connection has one) is not remembered (`404 not_remembered`); logging in gives the
  /// member `林晓` and the full fixtures (three cart lines, three addresses, orders).
  #[default]
  Contract,
  /// `fixture:member`: the connection's key is remembered for `林晓`: resume signs in at
  /// once (only when the connection has a key, as on the real server).
  Member,
  /// `fixture:empty`: nothing anywhere: no products, an empty cart, no addresses, no
  /// orders, a new profile (`profile-new.json`, `profile-no-key.json` without a key);
  /// resume answers `not_remembered`.
  Empty,
  /// `fixture:new`: `林晓` as a new member: the whole catalog, one line in the cart and no
  /// address (`cart-no-address.json`, also the answer to an add; using an address is
  /// `404 not_found`, checkout `422 address_required`), no addresses (saving one answers
  /// `saved-first.json`), no orders, the new profile.
  New,
  /// `fixture:problem`: short stock and failing writes: `nyeri` is out of stock (list stock
  /// 0, `detail-no-stock.json`) and `geisha` down to 2 (its list stock, detail bound and
  /// totals); the cart is `cart-problem.json` (3 Geisha flagged `out_of_stock`); adding more
  /// than is left and checking out answer `409 out_of_stock` with what is left; saving an
  /// address answers `500 internal_error`.
  Problem,
  /// `fixture:expired`: as `fixture:member` (a key resumes at once), but every member call
  /// answers `401 session_expired` (`登录已失效`).
  Expired,
  /// `fixture:member-error`: signing in works, the catalog works, every member call
  /// answers `500 internal_error` (the member pages' failed states).
  MemberError,
  /// `fixture:down`: no backend at all: every request fails as a refused connection does.
  Down,
  /// `fixture:error`: a backend that answers every request `500 internal_error`.
  Error,
}

impl FixtureSet {
  /// Every scenario.
  pub const ALL: [FixtureSet; 9] = [
    FixtureSet::Contract,
    FixtureSet::Member,
    FixtureSet::Empty,
    FixtureSet::New,
    FixtureSet::Problem,
    FixtureSet::Expired,
    FixtureSet::MemberError,
    FixtureSet::Down,
    FixtureSet::Error,
  ];

  /// The set `name` names (`guest`/`contract`, `member`, `empty`, `new`, `problem`,
  /// `expired`, `member-error`, `down`, `error`); anything else is the contract set.
  pub fn parse(name: &str) -> Self {
    let name = name.trim();
    Self::ALL
      .into_iter()
      .find(|set| set.name() == name)
      .unwrap_or_default()
  }

  /// Its name, as [`Self::parse`] reads it.
  pub fn name(self) -> &'static str {
    match self {
      FixtureSet::Contract => "guest",
      FixtureSet::Member => "member",
      FixtureSet::Empty => "empty",
      FixtureSet::New => "new",
      FixtureSet::Problem => "problem",
      FixtureSet::Expired => "expired",
      FixtureSet::MemberError => "member-error",
      FixtureSet::Down => "down",
      FixtureSet::Error => "error",
    }
  }

  /// What is left of `product` when this scenario changes its stock ([`FixtureSet::Problem`]).
  fn stock_left(self, product: &str) -> Option<u32> {
    match (self, product) {
      (FixtureSet::Problem, "nyeri") => Some(0),
      (FixtureSet::Problem, "geisha") => Some(2),
      _ => None,
    }
  }
}

/// A fixture's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Answer {
  pub(crate) status: u16,
  pub(crate) body: Cow<'static, str>,
}

fn ok(body: &'static str) -> Result<Answer, ApiFailure> {
  Ok(Answer {
    status: 200,
    body: Cow::Borrowed(body),
  })
}

fn created(body: &'static str) -> Result<Answer, ApiFailure> {
  Ok(Answer {
    status: 201,
    body: Cow::Borrowed(body),
  })
}

fn json(value: &impl serde::Serialize) -> Result<Answer, ApiFailure> {
  json_status(200, value)
}

fn json_status(status: u16, value: &impl serde::Serialize) -> Result<Answer, ApiFailure> {
  Ok(Answer {
    status,
    body: Cow::Owned(serde_json::to_string(value).expect("fixture answers serialize")),
  })
}

/// `409 out_of_stock` for `product` with `available` left (the committed envelope's
/// details, re-aimed).
fn out_of_stock(product: &ProductSummary, available: u32) -> Result<Answer, ApiFailure> {
  let mut envelope: ApiError = serde_json::from_str(E_OUT_OF_STOCK).expect("out_of_stock.json");
  if let Some(details) = envelope.error.details.as_mut() {
    details.product_id = Some(product.id.clone());
    details.product_name = Some(product.name.clone());
    details.available = Some(available);
  }
  json_status(ErrorCode::OutOfStock.status(), &envelope)
}

/// The committed envelope of `code` (`errors/<code>.json`).
pub(crate) fn error_body(code: ErrorCode) -> &'static str {
  let path = format!("errors/{}.json", code.as_str());
  FILES
    .iter()
    .find(|(file, _)| *file == path)
    .map(|(_, body)| *body)
    .unwrap_or(ERROR)
}

fn fail(code: ErrorCode) -> Result<Answer, ApiFailure> {
  Ok(Answer {
    status: code.status(),
    body: Cow::Borrowed(error_body(code)),
  })
}

/// The typed body of `call`, or the answer to a body that does not parse.
fn body<T: serde::de::DeserializeOwned>(call: &Call) -> Result<T, Result<Answer, ApiFailure>> {
  let Some(bytes) = &call.body else {
    return Err(fail(ErrorCode::BadRequest));
  };
  serde_json::from_slice(bytes).map_err(|_| fail(ErrorCode::ValidationFailed))
}

macro_rules! parse_body {
  ($call:expr) => {
    match body($call) {
      Ok(value) => value,
      Err(answer) => return answer,
    }
  };
}

fn products() -> ProductList {
  serde_json::from_str(PRODUCTS).expect("products.json is a ProductList")
}

fn known_product(id: &str) -> bool {
  products().items.iter().any(|p| p.id == id)
}

/// The committed detail of product `id` (`catalog/product-<id>.json`).
pub(crate) fn product_detail(id: &str) -> Option<&'static str> {
  let path = format!("catalog/product-{id}.json");
  FILES
    .iter()
    .find(|(file, _)| *file == path)
    .map(|(_, body)| *body)
}

impl FixtureSet {
  /// The product list of this scenario: `products.json` with [`Self::stock_left`] applied.
  fn products(self) -> ProductList {
    let mut list = products();
    for product in &mut list.items {
      if let Some(left) = self.stock_left(&product.id) {
        product.stock = left;
      }
    }
    list
  }

  /// The detail of `id`: its fixture, with [`Self::stock_left`] applied as the server would
  /// (nyeri's empty stock is `catalog/detail-no-stock.json`).
  fn detail(self, id: &str) -> Result<Answer, ApiFailure> {
    let Some(body) = product_detail(id) else {
      return fail(ErrorCode::NotFound);
    };
    match self.stock_left(id) {
      None => ok(body),
      Some(0) if id == "nyeri" => ok(PRODUCT_NO_STOCK),
      Some(left) => {
        let mut detail: ProductDetail = serde_json::from_str(body).expect("a ProductDetail");
        detail.stock = left;
        detail.max_quantity = u8::try_from(left.min(9)).unwrap_or(9);
        for cell in &mut detail.prices {
          cell.totals.truncate(usize::from(detail.max_quantity));
        }
        json(&detail)
      }
    }
  }
}

/// The SKUs of the fixture member cart.
fn cart_skus() -> Vec<String> {
  let cart: Cart = serde_json::from_str(CART).expect("cart.json is a Cart");
  cart.lines.into_iter().map(|line| line.sku).collect()
}

const KNOWN_ADDRESSES: [u32; 3] = [1, 2, 3];

/// `value` with `%XX` escapes decoded (what [`encode_query_value`] encodes).
pub(crate) fn percent_decode(value: &str) -> String {
  let bytes = value.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%'
      && let Some(byte) = value
        .get(i + 1..i + 3)
        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
    {
      out.push(byte);
      i += 3;
      continue;
    }
    out.push(bytes[i]);
    i += 1;
  }
  String::from_utf8_lossy(&out).into_owned()
}

/// The query of a `/products?…` path.
pub(crate) fn parse_product_query(path: &str) -> ProductListQuery {
  let mut query = ProductListQuery::default();
  let Some((_, string)) = path.split_once('?') else {
    return query;
  };
  for pair in string.split('&') {
    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
    let value = percent_decode(value);
    match key {
      "origin" if !value.is_empty() => query.origin = Some(value),
      "roast" => {
        query.roast = [RoastBand::Light, RoastBand::Medium, RoastBand::Dark]
          .into_iter()
          .find(|band| band.code() == value);
      }
      "sort" => {
        query.sort = [
          ProductSort::Recommended,
          ProductSort::PriceAsc,
          ProductSort::PriceDesc,
          ProductSort::Score,
        ]
        .into_iter()
        .find(|sort| sort.code() == value);
      }
      "q" if !value.trim().is_empty() => query.q = Some(value),
      _ => {}
    }
  }
  query
}

/// Whether roast level `roast` is in `band` (浅 1–2, 中 3, 深 4–5).
pub fn in_band(band: RoastBand, roast: u8) -> bool {
  match band {
    RoastBand::Light => roast <= 2,
    RoastBand::Medium => roast == 3,
    RoastBand::Dark => roast >= 4,
  }
}

/// Whether `product` matches search text `q`: a case-insensitive substring of its name,
/// subtitle, origin, region, process or notes (02-list.md, "筛选").
pub fn matches_search(product: &ProductSummary, q: &str) -> bool {
  let needle = q.trim().to_lowercase();
  if needle.is_empty() {
    return true;
  }
  [
    &product.name,
    &product.subtitle,
    &product.origin,
    &product.region,
    &product.process,
  ]
  .into_iter()
  .chain(product.notes.iter())
  .any(|field| field.to_lowercase().contains(&needle))
}

/// `list` filtered, sorted and searched by `query`, as the server does (the facets always
/// count everything).
pub fn filter_products(mut list: ProductList, query: &ProductListQuery) -> ProductList {
  list.items.retain(|product| {
    query.origin.as_deref().is_none_or(|o| product.origin == o)
      && query.roast.is_none_or(|band| in_band(band, product.roast))
      && query
        .q
        .as_deref()
        .is_none_or(|q| matches_search(product, q))
  });
  match query.sort.unwrap_or_default() {
    ProductSort::Recommended => {}
    ProductSort::PriceAsc => list.items.sort_by_key(|p| p.price250),
    ProductSort::PriceDesc => list.items.sort_by_key(|p| std::cmp::Reverse(p.price250)),
    ProductSort::Score => list.items.sort_by(|a, b| b.score.total_cmp(&a.score)),
  }
  list
}

fn validate_credentials(
  username: &str,
  password: &str,
  register: bool,
) -> Option<Result<Answer, ApiFailure>> {
  if username.trim().is_empty() {
    return Some(fail(ErrorCode::UsernameRequired));
  }
  if register && !username_is_valid(username.trim()) {
    return Some(fail(ErrorCode::UsernameInvalid));
  }
  let length = password.chars().count();
  if length < 6 {
    return Some(fail(ErrorCode::PasswordTooShort));
  }
  if length > 128 {
    return Some(fail(ErrorCode::PasswordTooLong));
  }
  None
}

fn validate_address(input: &AddressInput) -> Option<Result<Answer, ApiFailure>> {
  let recipient = input.recipient.trim();
  let region = input.region.trim();
  let detail = input.detail.trim();
  if recipient.is_empty() {
    return Some(fail(ErrorCode::RecipientRequired));
  }
  if region.is_empty() {
    return Some(fail(ErrorCode::RegionRequired));
  }
  if detail.is_empty() {
    return Some(fail(ErrorCode::DetailRequired));
  }
  if let Some(phone) = input.phone.as_deref().map(str::trim)
    && !phone.is_empty()
    && !(phone.len() == 11 && phone.bytes().all(|b| b.is_ascii_digit()))
  {
    return Some(fail(ErrorCode::PhoneInvalid));
  }
  if recipient.chars().count() > 20 || region.chars().count() > 40 || detail.chars().count() > 80 {
    return Some(fail(ErrorCode::TextTooLong));
  }
  None
}

fn quantity_ok(quantity: u8) -> bool {
  (1..=9).contains(&quantity)
}

impl FixtureSet {
  /// The answer to `call` from a client with `client`'s session headers; an unknown route
  /// is the `404` envelope.
  pub(crate) fn answer(self, call: &Call, client: &ApiClient) -> Result<Answer, ApiFailure> {
    match self {
      FixtureSet::Down => {
        return Err(ApiFailure::Network(
          "fixture:down · connection refused".into(),
        ));
      }
      FixtureSet::Error => return fail(ErrorCode::InternalError),
      _ => {}
    }
    if call.endpoint.access == Access::Member {
      match self {
        _ if !client.signed_in() => return fail(ErrorCode::LoginRequired),
        FixtureSet::Expired => return fail(ErrorCode::SessionExpired),
        FixtureSet::MemberError => return fail(ErrorCode::InternalError),
        _ => {}
      }
    }
    let empty = self == FixtureSet::Empty;
    // A new member's book: no addresses, no orders, the new profile.
    let fresh = empty || self == FixtureSet::New;
    let problem = self == FixtureSet::Problem;
    let segments: Vec<&str> = call.route().trim_start_matches('/').split('/').collect();
    let method = call.endpoint.method;
    match (method, segments.as_slice()) {
      (Method::Get, ["health"]) => ok(HEALTH),

      // auth
      (Method::Post, ["auth", "register"]) => {
        let request: RegisterRequest = parse_body!(call);
        if let Some(answer) = validate_credentials(&request.username, &request.password, true) {
          return answer;
        }
        if request.username.trim().to_lowercase() == TAKEN_USERNAME.to_lowercase() {
          return fail(ErrorCode::UsernameTaken);
        }
        let mut session: AuthSession = serde_json::from_str(AUTH_REGISTER).expect("register.json");
        session.user.username = request.username.trim().to_owned();
        json_status(201, &session)
      }
      (Method::Post, ["auth", "login"]) => {
        let request: LoginRequest = parse_body!(call);
        if let Some(answer) = validate_credentials(&request.username, &request.password, false) {
          return answer;
        }
        if request.password == WRONG_PASSWORD {
          return fail(ErrorCode::InvalidCredentials);
        }
        if client.fingerprint().is_some() {
          ok(AUTH_LOGIN)
        } else {
          ok(AUTH_LOGIN_NO_KEY)
        }
      }
      (Method::Post, ["auth", "resume"]) => match (client.fingerprint(), self) {
        (None, _) => fail(ErrorCode::FingerprintRequired),
        (Some(_), FixtureSet::Member | FixtureSet::Expired) => ok(AUTH_RESUME),
        (Some(_), _) => fail(ErrorCode::NotRemembered),
      },
      (Method::Post, ["auth", "logout"]) => ok(AUTH_LOGOUT),
      (Method::Get, ["auth", "me"]) => ok(AUTH_ME),

      // catalog
      (Method::Get, ["products"]) if empty => ok(PRODUCTS_EMPTY),
      (Method::Get, ["products"]) => json(&filter_products(
        self.products(),
        &parse_product_query(&call.path),
      )),
      (Method::Get, ["products", _]) if empty => fail(ErrorCode::NotFound),
      (Method::Get, ["products", id]) => self.detail(&percent_decode(id)),

      // cart
      (Method::Get, ["cart"]) => ok(match self {
        FixtureSet::Empty => CART_EMPTY,
        FixtureSet::New => CART_NO_ADDRESS,
        FixtureSet::Problem => CART_PROBLEM,
        _ => CART,
      }),
      (Method::Post, ["cart", "lines"]) => {
        let request: AddCartLineRequest = parse_body!(call);
        let Some(product) = self
          .products()
          .items
          .into_iter()
          .find(|p| p.id == request.product_id)
        else {
          return fail(ErrorCode::NotFound);
        };
        if !quantity_ok(request.quantity) {
          return fail(ErrorCode::QuantityInvalid);
        }
        if let Some(left) = self.stock_left(&product.id)
          && u32::from(request.quantity) > left
        {
          return out_of_stock(&product, left);
        }
        if self == FixtureSet::New {
          ok(CART_NO_ADDRESS)
        } else {
          ok(CART_ADDED)
        }
      }
      (Method::Patch, ["cart", "lines", sku]) => {
        let request: UpdateCartLineRequest = parse_body!(call);
        if !cart_skus().contains(&percent_decode(sku)) {
          return fail(ErrorCode::NotFound);
        }
        if request.quantity.is_some_and(|q| !quantity_ok(q)) {
          return fail(ErrorCode::QuantityInvalid);
        }
        if request.selected == Some(false) {
          ok(CART_PARTLY_SELECTED)
        } else {
          ok(CART)
        }
      }
      (Method::Delete, ["cart", "lines", sku]) => {
        if cart_skus().contains(&percent_decode(sku)) {
          ok(CART_REMOVED)
        } else {
          fail(ErrorCode::NotFound)
        }
      }
      (Method::Put, ["cart", "selection"]) => {
        let request: CartSelectionRequest = parse_body!(call);
        if request.selected {
          ok(CART)
        } else {
          ok(CART_NONE_SELECTED)
        }
      }
      (Method::Put, ["cart", "coupon"]) => {
        let request: ApplyCouponRequest = parse_body!(call);
        match request.code.trim().to_uppercase().as_str() {
          "" => ok(CART_COUPON_REMOVED),
          "TTY10" => ok(CART_COUPON),
          _ => fail(ErrorCode::CouponInvalid),
        }
      }
      (Method::Delete, ["cart", "coupon"]) => ok(CART_COUPON_REMOVED),
      (Method::Put, ["cart", "address"]) => {
        let request: SetCartAddressRequest = parse_body!(call);
        if !fresh && KNOWN_ADDRESSES.contains(&request.address_id) {
          ok(CART_ADDRESS_SET)
        } else {
          fail(ErrorCode::NotFound)
        }
      }
      (Method::Post, ["cart", "quote"]) => {
        let request: CartQuoteRequest = parse_body!(call);
        if request.lines.len() > 50 {
          return fail(ErrorCode::CartFull);
        }
        if request.lines.iter().any(|l| !known_product(&l.product_id)) {
          return fail(ErrorCode::NotFound);
        }
        if request.lines.iter().any(|l| !quantity_ok(l.quantity)) {
          return fail(ErrorCode::QuantityInvalid);
        }
        if let Some(code) = request.coupon_code.as_deref()
          && !code.trim().is_empty()
          && code.trim().to_uppercase() != "TTY10"
        {
          return fail(ErrorCode::CouponInvalid);
        }
        if request.lines.is_empty() {
          ok(CART_EMPTY)
        } else {
          ok(CART_GUEST_QUOTE)
        }
      }
      (Method::Post, ["cart", "merge"]) => {
        let _: CartMergeRequest = parse_body!(call);
        ok(CART_MERGE)
      }

      // addresses
      (Method::Get, ["addresses"]) if fresh => ok(ADDRESSES_EMPTY),
      (Method::Get, ["addresses"]) => ok(ADDRESSES),
      (Method::Post, ["addresses"]) => {
        let input: AddressInput = parse_body!(call);
        if let Some(answer) = validate_address(&input) {
          return answer;
        }
        if problem {
          fail(ErrorCode::InternalError)
        } else if fresh {
          created(ADDRESS_SAVED_FIRST)
        } else {
          created(ADDRESS_SAVED_NO_PHONE)
        }
      }
      (Method::Put, ["addresses", id]) => {
        let input: AddressInput = parse_body!(call);
        if fresh || !id.parse().is_ok_and(|id| KNOWN_ADDRESSES.contains(&id)) {
          return fail(ErrorCode::NotFound);
        }
        validate_address(&input).unwrap_or_else(|| {
          if problem {
            fail(ErrorCode::InternalError)
          } else {
            ok(ADDRESS_UPDATED)
          }
        })
      }
      (Method::Delete, ["addresses", id]) => {
        if !fresh && id.parse().is_ok_and(|id| KNOWN_ADDRESSES.contains(&id)) {
          ok(ADDRESSES_DELETED_DEFAULT)
        } else {
          fail(ErrorCode::NotFound)
        }
      }
      (Method::Post, ["addresses", id, "default"]) => {
        if !fresh && id.parse().is_ok_and(|id| KNOWN_ADDRESSES.contains(&id)) {
          ok(ADDRESSES_DEFAULT_MOVED)
        } else {
          fail(ErrorCode::NotFound)
        }
      }

      // orders
      (Method::Post, ["orders"]) => {
        let request: PlaceOrderRequest = parse_body!(call);
        let key = request.idempotency_key.len();
        if !(8..=64).contains(&key)
          || !request
            .idempotency_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
          return fail(ErrorCode::ValidationFailed);
        }
        match self {
          FixtureSet::Empty => fail(ErrorCode::EmptySelection),
          FixtureSet::New => fail(ErrorCode::AddressRequired),
          FixtureSet::Problem => {
            let geisha = self.products().items.into_iter().find(|p| p.id == "geisha");
            out_of_stock(&geisha.expect("geisha is listed"), 2)
          }
          _ => created(ORDER_PLACED),
        }
      }
      (Method::Get, ["orders"]) if fresh => ok(ORDERS_EMPTY),
      (Method::Get, ["orders"]) => ok(ORDERS),
      (Method::Get, ["orders", number]) => match (fresh, *number) {
        (false, "RT2611150203") => ok(ORDER_COUPON),
        (false, "RT2611060117") => ok(ORDER),
        (false, "RT2611220417") => {
          let placed: PlacedOrder = serde_json::from_str(ORDER_PLACED).expect("placed.json");
          json(&placed.order)
        }
        _ => fail(ErrorCode::NotFound),
      },

      // profile
      (Method::Get, ["profile"]) => ok(match (fresh, client.fingerprint().is_some()) {
        (true, true) => PROFILE_NEW,
        (true, false) => PROFILE_NO_KEY,
        (false, true) => PROFILE,
        (false, false) => PROFILE_ORDERS_NO_KEY,
      }),
      (Method::Get, ["settings"]) => ok(SETTINGS),
      (Method::Patch, ["settings"]) => {
        let patch: SettingsPatch = parse_body!(call);
        if patch
          == (SettingsPatch {
            animations: Some(false),
            mouse_tracking: None,
          })
        {
          return ok(SETTINGS_ANIMATIONS_OFF);
        }
        let current: Settings = serde_json::from_str(SETTINGS).expect("settings.json");
        json(&Settings {
          animations: patch.animations.unwrap_or(current.animations),
          mouse_tracking: patch.mouse_tracking.unwrap_or(current.mouse_tracking),
        })
      }
      _ => fail(ErrorCode::NotFound),
    }
  }
}
