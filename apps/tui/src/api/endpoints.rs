//! Every `/v1` operation: its [`Endpoint`] (the contract's endpoint table) and the typed
//! method of [`ApiClient`] that calls it. Path parameters are percent-encoded.

use coffee_api::*;

use super::{Access, ApiClient, ApiFailure, Call, Endpoint, Method};

const fn public(operation: &'static str, method: Method, path: &'static str) -> Endpoint {
  Endpoint {
    operation,
    method,
    path,
    access: Access::Public,
    fingerprint: false,
    client_ip: false,
  }
}

const fn member(operation: &'static str, method: Method, path: &'static str) -> Endpoint {
  Endpoint {
    operation,
    method,
    path,
    access: Access::Member,
    fingerprint: false,
    client_ip: false,
  }
}

const fn auth(endpoint: Endpoint) -> Endpoint {
  Endpoint {
    fingerprint: true,
    client_ip: true,
    ..endpoint
  }
}

const fn keyed(endpoint: Endpoint) -> Endpoint {
  Endpoint {
    fingerprint: true,
    ..endpoint
  }
}

/// `GET /v1/health`.
pub const GET_HEALTH: Endpoint = public("getHealth", Method::Get, "/health");
/// `POST /v1/auth/register`.
pub const REGISTER: Endpoint = auth(public("register", Method::Post, "/auth/register"));
/// `POST /v1/auth/login`.
pub const LOGIN: Endpoint = auth(public("login", Method::Post, "/auth/login"));
/// `POST /v1/auth/resume`.
pub const RESUME: Endpoint = auth(public("resume", Method::Post, "/auth/resume"));
/// `POST /v1/auth/logout`.
pub const LOGOUT: Endpoint = auth(member("logout", Method::Post, "/auth/logout"));
/// `GET /v1/auth/me`.
pub const GET_ME: Endpoint = auth(member("getMe", Method::Get, "/auth/me"));
/// `GET /v1/products`.
pub const LIST_PRODUCTS: Endpoint = public("listProducts", Method::Get, "/products");
/// `GET /v1/products/{id}`.
pub const GET_PRODUCT: Endpoint = public("getProduct", Method::Get, "/products/{id}");
/// `GET /v1/cart`.
pub const GET_CART: Endpoint = member("getCart", Method::Get, "/cart");
/// `POST /v1/cart/lines`.
pub const ADD_CART_LINE: Endpoint = member("addCartLine", Method::Post, "/cart/lines");
/// `PATCH /v1/cart/lines/{sku}`.
pub const UPDATE_CART_LINE: Endpoint = member("updateCartLine", Method::Patch, "/cart/lines/{sku}");
/// `DELETE /v1/cart/lines/{sku}`.
pub const REMOVE_CART_LINE: Endpoint =
  member("removeCartLine", Method::Delete, "/cart/lines/{sku}");
/// `PUT /v1/cart/selection`.
pub const SET_CART_SELECTION: Endpoint = member("setCartSelection", Method::Put, "/cart/selection");
/// `PUT /v1/cart/coupon`.
pub const APPLY_COUPON: Endpoint = member("applyCoupon", Method::Put, "/cart/coupon");
/// `DELETE /v1/cart/coupon`.
pub const REMOVE_COUPON: Endpoint = member("removeCoupon", Method::Delete, "/cart/coupon");
/// `PUT /v1/cart/address`.
pub const SET_CART_ADDRESS: Endpoint = member("setCartAddress", Method::Put, "/cart/address");
/// `POST /v1/cart/quote`.
pub const QUOTE_CART: Endpoint = public("quoteCart", Method::Post, "/cart/quote");
/// `POST /v1/cart/merge`.
pub const MERGE_CART: Endpoint = member("mergeCart", Method::Post, "/cart/merge");
/// `GET /v1/addresses`.
pub const LIST_ADDRESSES: Endpoint = member("listAddresses", Method::Get, "/addresses");
/// `POST /v1/addresses`.
pub const CREATE_ADDRESS: Endpoint = member("createAddress", Method::Post, "/addresses");
/// `PUT /v1/addresses/{id}`.
pub const UPDATE_ADDRESS: Endpoint = member("updateAddress", Method::Put, "/addresses/{id}");
/// `DELETE /v1/addresses/{id}`.
pub const DELETE_ADDRESS: Endpoint = member("deleteAddress", Method::Delete, "/addresses/{id}");
/// `POST /v1/addresses/{id}/default`.
pub const SET_DEFAULT_ADDRESS: Endpoint =
  member("setDefaultAddress", Method::Post, "/addresses/{id}/default");
/// `POST /v1/orders`.
pub const PLACE_ORDER: Endpoint = member("placeOrder", Method::Post, "/orders");
/// `GET /v1/orders`.
pub const LIST_ORDERS: Endpoint = member("listOrders", Method::Get, "/orders");
/// `GET /v1/orders/{number}`.
pub const GET_ORDER: Endpoint = member("getOrder", Method::Get, "/orders/{number}");
/// `GET /v1/profile`.
pub const GET_PROFILE: Endpoint = keyed(member("getProfile", Method::Get, "/profile"));
/// `GET /v1/settings`.
pub const GET_SETTINGS: Endpoint = member("getSettings", Method::Get, "/settings");
/// `PATCH /v1/settings`.
pub const UPDATE_SETTINGS: Endpoint = member("updateSettings", Method::Patch, "/settings");

/// Every endpoint, in the contract's order.
pub const ENDPOINTS: &[Endpoint] = &[
  GET_HEALTH,
  REGISTER,
  LOGIN,
  RESUME,
  LOGOUT,
  GET_ME,
  LIST_PRODUCTS,
  GET_PRODUCT,
  GET_CART,
  ADD_CART_LINE,
  UPDATE_CART_LINE,
  REMOVE_CART_LINE,
  SET_CART_SELECTION,
  APPLY_COUPON,
  REMOVE_COUPON,
  SET_CART_ADDRESS,
  QUOTE_CART,
  MERGE_CART,
  LIST_ADDRESSES,
  CREATE_ADDRESS,
  UPDATE_ADDRESS,
  DELETE_ADDRESS,
  SET_DEFAULT_ADDRESS,
  PLACE_ORDER,
  LIST_ORDERS,
  GET_ORDER,
  GET_PROFILE,
  GET_SETTINGS,
  UPDATE_SETTINGS,
];

/// `segment` percent-encoded for a path.
fn seg(segment: &str) -> String {
  encode_query_value(segment)
}

impl ApiClient {
  /// `GET /v1/health`.
  pub async fn health(&self) -> Result<Health, ApiFailure> {
    self.send(Call::new(GET_HEALTH, "/health")).await
  }

  /// `POST /v1/auth/register`: a new member, signed in.
  pub async fn register(&self, request: &RegisterRequest) -> Result<AuthSession, ApiFailure> {
    self
      .send(Call::new(REGISTER, "/auth/register").json(request))
      .await
  }

  /// `POST /v1/auth/login`.
  pub async fn login(&self, request: &LoginRequest) -> Result<AuthSession, ApiFailure> {
    self
      .send(Call::new(LOGIN, "/auth/login").json(request))
      .await
  }

  /// `POST /v1/auth/resume`: the session of the account this connection's key is
  /// remembered for (`404 not_remembered` otherwise).
  pub async fn resume(&self) -> Result<AuthSession, ApiFailure> {
    self.send(Call::new(RESUME, "/auth/resume")).await
  }

  /// `POST /v1/auth/logout`.
  pub async fn logout(&self) -> Result<OkResponse, ApiFailure> {
    self.send(Call::new(LOGOUT, "/auth/logout")).await
  }

  /// `GET /v1/auth/me`.
  pub async fn me(&self) -> Result<Me, ApiFailure> {
    self.send(Call::new(GET_ME, "/auth/me")).await
  }

  /// `GET /v1/products` without filters.
  pub async fn products(&self) -> Result<ProductList, ApiFailure> {
    self.list_products(&ProductListQuery::default()).await
  }

  /// `GET /v1/products?origin&roast&sort&q`: filtered, sorted and searched by the server.
  pub async fn list_products(&self, query: &ProductListQuery) -> Result<ProductList, ApiFailure> {
    let path = format!("/products{}", query.to_query_string());
    self.send(Call::new(LIST_PRODUCTS, path)).await
  }

  /// `GET /v1/products/{id}`.
  pub async fn product(&self, id: &str) -> Result<ProductDetail, ApiFailure> {
    self
      .send(Call::new(GET_PRODUCT, format!("/products/{}", seg(id))))
      .await
  }

  /// `GET /v1/cart`.
  pub async fn cart(&self) -> Result<Cart, ApiFailure> {
    self.send(Call::new(GET_CART, "/cart")).await
  }

  /// `POST /v1/cart/lines`.
  pub async fn add_cart_line(&self, request: &AddCartLineRequest) -> Result<Cart, ApiFailure> {
    self
      .send(Call::new(ADD_CART_LINE, "/cart/lines").json(request))
      .await
  }

  /// `PATCH /v1/cart/lines/{sku}`.
  pub async fn update_cart_line(
    &self,
    sku: &str,
    request: &UpdateCartLineRequest,
  ) -> Result<Cart, ApiFailure> {
    self
      .send(Call::new(UPDATE_CART_LINE, format!("/cart/lines/{}", seg(sku))).json(request))
      .await
  }

  /// `DELETE /v1/cart/lines/{sku}`.
  pub async fn remove_cart_line(&self, sku: &str) -> Result<Cart, ApiFailure> {
    self
      .send(Call::new(
        REMOVE_CART_LINE,
        format!("/cart/lines/{}", seg(sku)),
      ))
      .await
  }

  /// `PUT /v1/cart/selection`: select or unselect every line.
  pub async fn set_cart_selection(&self, selected: bool) -> Result<Cart, ApiFailure> {
    self
      .send(
        Call::new(SET_CART_SELECTION, "/cart/selection").json(&CartSelectionRequest { selected }),
      )
      .await
  }

  /// `PUT /v1/cart/coupon` (an empty code removes it).
  pub async fn apply_coupon(&self, code: &str) -> Result<Cart, ApiFailure> {
    self
      .send(
        Call::new(APPLY_COUPON, "/cart/coupon").json(&ApplyCouponRequest {
          code: code.to_owned(),
        }),
      )
      .await
  }

  /// `DELETE /v1/cart/coupon`.
  pub async fn remove_coupon(&self) -> Result<Cart, ApiFailure> {
    self.send(Call::new(REMOVE_COUPON, "/cart/coupon")).await
  }

  /// `PUT /v1/cart/address`: 本单寄往.
  pub async fn set_cart_address(&self, address_id: u32) -> Result<Cart, ApiFailure> {
    self
      .send(
        Call::new(SET_CART_ADDRESS, "/cart/address").json(&SetCartAddressRequest { address_id }),
      )
      .await
  }

  /// `POST /v1/cart/quote`: a guest cart priced by the server.
  pub async fn quote_cart(&self, request: &CartQuoteRequest) -> Result<Cart, ApiFailure> {
    self
      .send(Call::new(QUOTE_CART, "/cart/quote").json(request))
      .await
  }

  /// `POST /v1/cart/merge`: the guest cart into the member's.
  pub async fn merge_cart(
    &self,
    request: &CartMergeRequest,
  ) -> Result<CartMergeResult, ApiFailure> {
    self
      .send(Call::new(MERGE_CART, "/cart/merge").json(request))
      .await
  }

  /// `GET /v1/addresses`.
  pub async fn addresses(&self) -> Result<AddressList, ApiFailure> {
    self.send(Call::new(LIST_ADDRESSES, "/addresses")).await
  }

  /// `POST /v1/addresses`.
  pub async fn create_address(&self, input: &AddressInput) -> Result<AddressSaved, ApiFailure> {
    self
      .send(Call::new(CREATE_ADDRESS, "/addresses").json(input))
      .await
  }

  /// `PUT /v1/addresses/{id}`.
  pub async fn update_address(
    &self,
    id: u32,
    input: &AddressInput,
  ) -> Result<AddressSaved, ApiFailure> {
    self
      .send(Call::new(UPDATE_ADDRESS, format!("/addresses/{id}")).json(input))
      .await
  }

  /// `DELETE /v1/addresses/{id}`.
  pub async fn delete_address(&self, id: u32) -> Result<AddressList, ApiFailure> {
    self
      .send(Call::new(DELETE_ADDRESS, format!("/addresses/{id}")))
      .await
  }

  /// `POST /v1/addresses/{id}/default`.
  pub async fn set_default_address(&self, id: u32) -> Result<AddressList, ApiFailure> {
    self
      .send(Call::new(
        SET_DEFAULT_ADDRESS,
        format!("/addresses/{id}/default"),
      ))
      .await
  }

  /// `POST /v1/orders`: checkout (the selected lines to the order address).
  pub async fn place_order(&self, request: &PlaceOrderRequest) -> Result<PlacedOrder, ApiFailure> {
    self
      .send(Call::new(PLACE_ORDER, "/orders").json(request))
      .await
  }

  /// `GET /v1/orders?limit` (1–50; `None` = the server's 20).
  pub async fn orders(&self, limit: Option<u8>) -> Result<OrderList, ApiFailure> {
    let path = match limit {
      Some(limit) => format!("/orders?limit={limit}"),
      None => "/orders".to_owned(),
    };
    self.send(Call::new(LIST_ORDERS, path)).await
  }

  /// `GET /v1/orders/{number}`.
  pub async fn order(&self, number: &str) -> Result<Order, ApiFailure> {
    self
      .send(Call::new(GET_ORDER, format!("/orders/{}", seg(number))))
      .await
  }

  /// `GET /v1/profile`.
  pub async fn profile(&self) -> Result<Profile, ApiFailure> {
    self.send(Call::new(GET_PROFILE, "/profile")).await
  }

  /// `GET /v1/settings`.
  pub async fn settings(&self) -> Result<Settings, ApiFailure> {
    self.send(Call::new(GET_SETTINGS, "/settings")).await
  }

  /// `PATCH /v1/settings`.
  pub async fn update_settings(&self, patch: &SettingsPatch) -> Result<Settings, ApiFailure> {
    self
      .send(Call::new(UPDATE_SETTINGS, "/settings").json(patch))
      .await
  }
}
