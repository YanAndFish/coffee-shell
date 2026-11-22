//! The client against the contract: every endpoint answered from its fixtures and decoded,
//! error envelopes mapped to typed failures, the headers each call carries, and the
//! fixture scenarios.

use std::path::{Path, PathBuf};

use super::{fixtures::*, *};

fn client(set: FixtureSet) -> ApiClient {
  ApiClient::new(Transport::Fixture(set), None)
}

fn member(set: FixtureSet) -> ApiClient {
  client(set)
    .with_fingerprint(Some("SHA256:fixture".into()))
    .with_token(Some("t".repeat(43)))
}

fn run<T>(future: impl Future<Output = T>) -> T {
  now_or_never(future)
}

fn ids(list: &ProductList) -> Vec<&str> {
  list.items.iter().map(|p| p.id.as_str()).collect()
}

#[test]
fn urls_pick_the_transport() {
  assert_eq!(
    Transport::from_url("http://127.0.0.1:3000/"),
    Transport::Live("http://127.0.0.1:3000".into())
  );
  for (url, set) in [
    ("fixture:", FixtureSet::Contract),
    ("fixture:guest", FixtureSet::Contract),
    ("fixture:contract", FixtureSet::Contract),
    ("fixture:member", FixtureSet::Member),
    ("fixture:empty", FixtureSet::Empty),
    ("fixture:down", FixtureSet::Down),
    ("fixture:error", FixtureSet::Error),
    ("fixture:new", FixtureSet::New),
    ("fixture:problem", FixtureSet::Problem),
    ("fixture:expired", FixtureSet::Expired),
    ("fixture:member-error", FixtureSet::MemberError),
    ("fixture:slow", FixtureSet::Slow),
  ] {
    assert_eq!(Transport::from_url(url), Transport::Fixture(set), "{url}");
  }
  for set in FixtureSet::ALL {
    assert_eq!(FixtureSet::parse(set.name()), set);
  }
}

fn fixtures_dir() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/contract/fixtures/v1")
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
  for entry in std::fs::read_dir(dir).unwrap() {
    let path = entry.unwrap().path();
    if path.is_dir() {
      walk(&path, root, out);
    } else if path.extension().is_some_and(|e| e == "json") {
      out.push(
        path
          .strip_prefix(root)
          .unwrap()
          .to_string_lossy()
          .replace('\\', "/"),
      );
    }
  }
}

#[test]
fn the_transport_embeds_every_committed_fixture() {
  let mut on_disk = Vec::new();
  walk(&fixtures_dir(), &fixtures_dir(), &mut on_disk);
  on_disk.sort();
  let mut embedded: Vec<String> = FILES.iter().map(|(path, _)| (*path).to_owned()).collect();
  embedded.sort();
  assert_eq!(embedded, on_disk, "add new fixtures to api/fixtures.rs");
  for (path, body) in FILES {
    let disk = std::fs::read_to_string(fixtures_dir().join(path)).unwrap();
    assert_eq!(&disk, body, "{path} is stale");
  }
}

/// Every fixture file decodes into its DTO through the client's one decoding path, with the
/// status its endpoint answers.
#[test]
fn every_fixture_decodes_through_the_client() {
  fn check<T: serde::de::DeserializeOwned>(path: &str, body: &str) {
    if let Err(failure) = decode::<T>(200, body.as_bytes()) {
      panic!("{path}: {failure}");
    }
  }
  for (path, body) in FILES {
    let file = path.rsplit('/').next().unwrap();
    match *path {
      "health.json" => check::<Health>(path, body),
      "products.json" => check::<ProductList>(path, body),
      _ if path.starts_with("errors/") || *path == "error.json" => {
        let failure = decode::<Health>(404, body.as_bytes()).unwrap_err();
        assert!(failure.code().is_some(), "{path}: unknown code");
      }
      _ if path.starts_with("auth/") => match file {
        "logout.json" => check::<OkResponse>(path, body),
        "me.json" => check::<Me>(path, body),
        _ => check::<AuthSession>(path, body),
      },
      _ if path.starts_with("catalog/products-") => check::<ProductList>(path, body),
      _ if path.starts_with("catalog/product-") || path.starts_with("catalog/detail-") => {
        check::<ProductDetail>(path, body)
      }
      "cart/merge.json" => check::<CartMergeResult>(path, body),
      _ if path.starts_with("cart/") => check::<Cart>(path, body),
      _ if path.starts_with("addresses/list") || file.starts_with("deleted") => {
        check::<AddressList>(path, body)
      }
      "addresses/default-moved.json" => check::<AddressList>(path, body),
      _ if path.starts_with("addresses/") => check::<AddressSaved>(path, body),
      "orders/placed.json" => check::<PlacedOrder>(path, body),
      _ if path.starts_with("orders/list") => check::<OrderList>(path, body),
      _ if path.starts_with("orders/") => check::<Order>(path, body),
      _ if path.starts_with("profile/settings") => check::<Settings>(path, body),
      _ if path.starts_with("profile/") => check::<Profile>(path, body),
      _ => panic!("{path}: no DTO"),
    }
  }
}

#[test]
fn every_error_envelope_becomes_a_typed_failure() {
  for code in ErrorCode::ALL {
    let body = error_body(*code);
    let failure = decode::<Cart>(code.status(), body.as_bytes()).unwrap_err();
    assert_eq!(failure.code(), Some(*code));
    assert!(failure.is(*code));
    assert_eq!(failure.message(), code.message());
    assert_eq!(failure, ApiFailure::of(*code).with_details_of(&failure));
  }
  let stock = decode::<Cart>(409, error_body(ErrorCode::OutOfStock).as_bytes()).unwrap_err();
  let details = stock.details().unwrap();
  assert_eq!(details.product_name.as_deref(), Some("波奎特 瑰夏"));
  assert_eq!(details.available, Some(6));
  let expired = ApiFailure::of(ErrorCode::SessionExpired);
  assert!(expired.ends_session());
  assert!(ApiFailure::of(ErrorCode::LoginRequired).ends_session());
  assert!(!ApiFailure::of(ErrorCode::NotFound).ends_session());
}

impl ApiFailure {
  /// `self` with the details of `other` (the envelopes carry sample details).
  fn with_details_of(mut self, other: &ApiFailure) -> Self {
    if let ApiFailure::Status { details, .. } = &mut self {
      *details = other.details().cloned().map(Box::new);
    }
    self
  }
}

#[test]
fn the_error_envelope_becomes_a_typed_failure() {
  let failure = decode::<ProductList>(404, ERROR.as_bytes()).unwrap_err();
  assert_eq!(failure, ApiFailure::of(ErrorCode::NotFound));
  assert_eq!(failure.technical(), "资源不存在（HTTP 404 · not_found）");
  // People read the contract's message only.
  assert_eq!(failure.detail(), "资源不存在");
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
      details: None,
    }
  );
  assert_eq!(failure.code(), None);
  assert_eq!(failure.technical(), "HTTP 502 · http_502");
  assert_eq!(failure.detail(), "服务器返回了错误，请稍后重试");
  assert_eq!(failure.message(), "商店后端返回了错误");
}

#[test]
fn a_failure_never_shows_people_its_transport_error() {
  let down = ApiFailure::Network("io: Connection refused (os error 61)".into());
  assert_eq!(down.detail(), "连接服务器失败，请稍后重试");
  assert!(down.technical().contains("os error 61"));
  let odd = ApiFailure::Decode("missing field `id`".into());
  assert_eq!(odd.detail(), "服务器的应答无法识别，请稍后重试");
  assert_eq!(
    ApiFailure::of(ErrorCode::InternalError).detail(),
    "服务器内部错误"
  );
  for failure in [down, odd, ApiFailure::of(ErrorCode::SessionExpired)] {
    let detail = failure.detail();
    assert!(
      !detail.contains("HTTP") && !detail.contains("error") && !detail.contains('_'),
      "{detail}"
    );
  }
}

#[test]
fn a_body_off_contract_is_a_decode_failure() {
  let failure = decode::<ProductList>(200, br#"{"items":[{"id":1}]}"#).unwrap_err();
  assert!(matches!(failure, ApiFailure::Decode(_)));
  assert!(failure.to_string().starts_with("商店后端的应答看不懂"));
  assert_eq!(
    ApiFailure::Network("refused".into()).message(),
    NETWORK_MESSAGE
  );
}

/// The README's endpoint table, row by row: (method, path, operationId, auth, fingerprint).
fn contract_table() -> Vec<(String, String, String, String, String)> {
  let readme = include_str!("../../../../packages/contract/README.md");
  readme
    .lines()
    .filter(|line| {
      line.starts_with("| GET") || line.starts_with("| POST") || {
        line.starts_with("| PUT") || line.starts_with("| PATCH") || line.starts_with("| DELETE")
      }
    })
    .map(|line| {
      let cells: Vec<String> = line
        .split('|')
        .map(|cell| cell.trim().trim_matches('`').to_owned())
        .collect();
      (
        cells[1].clone(),
        cells[2].clone(),
        cells[3].clone(),
        cells[5].clone(),
        cells[6].clone(),
      )
    })
    .collect()
}

#[test]
fn the_endpoint_table_is_the_contracts() {
  let table = contract_table();
  assert_eq!(table.len(), ENDPOINTS.len());
  for (method, path, operation, auth, fingerprint) in table {
    let endpoint = ENDPOINTS
      .iter()
      .find(|e| e.operation == operation)
      .unwrap_or_else(|| panic!("{operation} has no endpoint"));
    assert_eq!(endpoint.method.as_str(), method, "{operation}");
    assert_eq!(format!("/v1{}", endpoint.path), path, "{operation}");
    assert_eq!(
      endpoint.access == Access::Member,
      auth == "user",
      "{operation}"
    );
    assert_eq!(endpoint.fingerprint, fingerprint != "none", "{operation}");
    assert_eq!(
      endpoint.client_ip,
      path.starts_with("/v1/auth/"),
      "{operation}"
    );
  }
}

#[test]
fn calls_carry_the_session_headers_the_contract_names() {
  let client = ApiClient::new(
    Transport::Live("http://127.0.0.1:3000".into()),
    Some("s3cret".into()),
  )
  .with_fingerprint(Some("SHA256:abc".into()))
  .with_client_ip(Some("10.0.0.7".into()))
  .with_token(Some("tok".into()));
  let names = |call: &Call| -> Vec<String> {
    client
      .headers(call)
      .into_iter()
      .map(|(name, _)| name)
      .collect()
  };
  let cart = Call::new(endpoints::GET_CART, "/cart");
  assert_eq!(
    names(&cart),
    vec![INTERNAL_KEY_HEADER, AUTHORIZATION_HEADER]
  );
  let login = Call::new(endpoints::LOGIN, "/auth/login");
  assert_eq!(
    names(&login),
    vec![
      INTERNAL_KEY_HEADER,
      SSH_FINGERPRINT_HEADER,
      CLIENT_IP_HEADER
    ]
  );
  let logout = Call::new(endpoints::LOGOUT, "/auth/logout");
  assert_eq!(names(&logout).len(), 4);
  let profile = Call::new(endpoints::GET_PROFILE, "/profile");
  assert_eq!(
    names(&profile),
    vec![
      INTERNAL_KEY_HEADER,
      AUTHORIZATION_HEADER,
      SSH_FINGERPRINT_HEADER
    ]
  );
  let quote = Call::new(endpoints::QUOTE_CART, "/cart/quote").json(&CartQuoteRequest::default());
  assert_eq!(names(&quote), vec![INTERNAL_KEY_HEADER]);
  assert!(
    client
      .headers(&cart)
      .contains(&(AUTHORIZATION_HEADER.into(), "Bearer tok".into()))
  );

  let request = client.http_request("http://127.0.0.1:3000", &quote);
  assert_eq!(request.method, "POST");
  assert_eq!(request.url, "http://127.0.0.1:3000/v1/cart/quote");
  assert!(
    request
      .headers
      .contains(&("Content-Type".into(), "application/json".into()))
  );
  let anonymous = ApiClient::new(Transport::Live("http://h".into()), None);
  let request = anonymous.http_request("http://h", &cart);
  assert!(
    !request
      .headers
      .iter()
      .any(|(name, _)| name == INTERNAL_KEY_HEADER || name == AUTHORIZATION_HEADER)
  );
  assert_eq!(request.method, "GET");
}

#[test]
fn path_parameters_are_encoded() {
  let call = Call::new(endpoints::GET_PRODUCT, "/products/a%20b?x=1");
  assert_eq!(call.route(), "/products/a%20b");
  assert_eq!(percent_decode("a%20b%E8%8C%89"), "a b茉");
  assert_eq!(percent_decode("100%"), "100%");
}

#[test]
fn every_endpoint_answers_from_its_fixtures() {
  let guest = client(FixtureSet::Contract);
  let m = member(FixtureSet::Contract);
  assert!(run(guest.health()).unwrap().demo);
  // auth
  let request = |username: &str, password: &str| RegisterRequest {
    username: username.into(),
    password: password.into(),
    remember: true,
  };
  assert_eq!(
    run(guest.register(&request("bean_lover", "secret1")))
      .unwrap()
      .user
      .username,
    "bean_lover"
  );
  let login = LoginRequest {
    username: "林晓".into(),
    password: "secret1".into(),
    remember: true,
  };
  let session = run(guest.login(&login)).unwrap();
  assert_eq!(session.token.len(), 43);
  assert_eq!(session.remembered.state, RememberState::NoKey);
  let keyed = guest.clone().with_fingerprint(Some("SHA256:k".into()));
  assert_eq!(
    run(keyed.login(&login)).unwrap().remembered.state,
    RememberState::Remembered
  );
  assert!(run(m.logout()).unwrap().ok);
  assert_eq!(run(m.me()).unwrap().user.username, "林晓");
  // catalog
  assert_eq!(run(guest.products()).unwrap().items.len(), 8);
  for summary in run(guest.products()).unwrap().items {
    let detail = run(guest.product(&summary.id)).unwrap();
    assert_eq!((detail.id, detail.stock), (summary.id, summary.stock));
  }
  assert_eq!(run(guest.product("pinkb")).unwrap().code, "PNK");
  assert_eq!(run(guest.product("geisha")).unwrap().max_quantity, 6);
  // cart
  assert_eq!(run(m.cart()).unwrap().totals.item_count, 3);
  let add = AddCartLineRequest {
    product_id: "yirga".into(),
    weight: Weight::G250,
    grind: Grind::WholeBean,
    pack: Pack::Standard,
    quantity: 1,
    index: None,
  };
  assert_eq!(run(m.add_cart_line(&add)).unwrap().lines.len(), 3);
  let unselect = UpdateCartLineRequest {
    quantity: None,
    selected: Some(false),
  };
  assert_eq!(
    run(m.update_cart_line("RT-PNK-250G-PO-STD", &unselect))
      .unwrap()
      .totals
      .selected_count,
    2
  );
  assert_eq!(
    run(m.remove_cart_line("RT-PNK-250G-PO-STD"))
      .unwrap()
      .lines
      .len(),
    2
  );
  assert_eq!(
    run(m.set_cart_selection(false))
      .unwrap()
      .totals
      .selected_count,
    0
  );
  assert_eq!(run(m.apply_coupon(" tty10 ")).unwrap().totals.discount, 10);
  assert!(run(m.remove_coupon()).unwrap().coupon.is_none());
  assert_eq!(
    run(m.set_cart_address(2)).unwrap().address.map(|a| a.id),
    Some(2)
  );
  let quote = CartQuoteRequest {
    lines: vec![GuestCartLine {
      product_id: "yirga".into(),
      weight: Weight::G250,
      grind: Grind::PourOver,
      pack: Pack::Standard,
      quantity: 2,
      selected: true,
    }],
    coupon_code: None,
  };
  assert_eq!(run(guest.quote_cart(&quote)).unwrap().totals.item_count, 2);
  let merge = CartMergeRequest {
    lines: quote.lines.clone(),
    coupon_code: None,
  };
  assert_eq!(run(m.merge_cart(&merge)).unwrap().merged_count, 1);
  // addresses
  assert_eq!(run(m.addresses()).unwrap().items.len(), 3);
  let input = AddressInput {
    recipient: "周一".into(),
    region: "上海市".into(),
    detail: "某路 1 号".into(),
    phone: None,
    tag: AddressTag::Home,
    is_default: false,
  };
  assert_eq!(run(m.create_address(&input)).unwrap().address.id, 4);
  assert_eq!(run(m.update_address(2, &input)).unwrap().address.id, 2);
  assert_eq!(
    run(m.delete_address(1)).unwrap().default_address_id,
    Some(2)
  );
  assert_eq!(
    run(m.set_default_address(3)).unwrap().default_address_id,
    Some(3)
  );
  // orders
  let placed = run(m.place_order(&PlaceOrderRequest {
    idempotency_key: "checkout-0001".into(),
    expected_total: None,
  }))
  .unwrap();
  assert_eq!(placed.order.number, "RT2611220417");
  assert_eq!(run(m.orders(Some(20))).unwrap().total, 4);
  assert_eq!(
    run(m.order("RT2611150203")).unwrap().coupon_code.as_deref(),
    Some("TTY10")
  );
  assert_eq!(run(m.order("RT2611220417")).unwrap(), placed.order);
  // profile
  assert!(run(m.profile()).unwrap().taste.is_some());
  assert!(run(m.settings()).unwrap().animations);
  let off = SettingsPatch {
    animations: Some(false),
    mouse_tracking: None,
  };
  assert!(!run(m.update_settings(&off)).unwrap().animations);
  let mouse_off = SettingsPatch {
    animations: None,
    mouse_tracking: Some(false),
  };
  let settings = run(m.update_settings(&mouse_off)).unwrap();
  assert!(settings.animations && !settings.mouse_tracking);
}

#[test]
fn the_derived_product_list_is_the_committed_one() {
  let guest = client(FixtureSet::Contract);
  let committed = |body: &str| serde_json::from_str::<ProductList>(body).unwrap();
  let query = |origin: Option<&str>, roast, sort, q: Option<&str>| ProductListQuery {
    origin: origin.map(Into::into),
    roast,
    sort,
    q: q.map(Into::into),
  };
  let light = query(
    None,
    Some(RoastBand::Light),
    Some(ProductSort::PriceAsc),
    None,
  );
  assert_eq!(
    run(guest.list_products(&light)).unwrap(),
    committed(PRODUCTS_LIGHT_BY_PRICE)
  );
  let search = query(None, None, Some(ProductSort::Score), Some("茉莉"));
  assert_eq!(
    run(guest.list_products(&search)).unwrap(),
    committed(PRODUCTS_SEARCH)
  );
  let nothing = query(None, None, None, Some("xyz"));
  assert_eq!(
    run(guest.list_products(&nothing)).unwrap(),
    committed(PRODUCTS_EMPTY)
  );
  let kenya = query(Some("肯尼亚"), None, None, None);
  assert_eq!(ids(&run(guest.list_products(&kenya)).unwrap()), ["nyeri"]);
  let dark = query(
    None,
    Some(RoastBand::Dark),
    Some(ProductSort::PriceDesc),
    None,
  );
  assert_eq!(
    ids(&run(guest.list_products(&dark)).unwrap()),
    ["night", "antigua", "mandh"]
  );
  let medium = query(None, Some(RoastBand::Medium), None, Some("YUNNAN"));
  assert_eq!(ids(&run(guest.list_products(&medium)).unwrap()), ["yunnan"]);
  assert_eq!(
    parse_product_query(
      "/products?origin=%E8%82%AF%E5%B0%BC%E4%BA%9A&roast=dark&sort=score&q=a%20b"
    ),
    ProductListQuery {
      origin: Some("肯尼亚".into()),
      roast: Some(RoastBand::Dark),
      sort: Some(ProductSort::Score),
      q: Some("a b".into()),
    }
  );
}

#[test]
fn the_scenarios_answer_as_documented() {
  // Member calls need a token.
  let guest = client(FixtureSet::Contract);
  assert!(run(guest.cart()).unwrap_err().is(ErrorCode::LoginRequired));
  // Resume needs a key, and a remembered one.
  assert!(
    run(guest.resume())
      .unwrap_err()
      .is(ErrorCode::FingerprintRequired)
  );
  let keyed = guest.clone().with_fingerprint(Some("SHA256:k".into()));
  assert!(
    run(keyed.resume())
      .unwrap_err()
      .is(ErrorCode::NotRemembered)
  );
  let remembered = client(FixtureSet::Member).with_fingerprint(Some("SHA256:k".into()));
  assert_eq!(run(remembered.resume()).unwrap().user.username, "林晓");
  // Credentials are validated as the server does.
  let login = |username: &str, password: &str| LoginRequest {
    username: username.into(),
    password: password.into(),
    remember: false,
  };
  let code = |result: Result<AuthSession, ApiFailure>| result.unwrap_err().code();
  assert_eq!(
    code(run(guest.login(&login(" ", "secret1")))),
    Some(ErrorCode::UsernameRequired)
  );
  assert_eq!(
    code(run(guest.login(&login("lin", "123")))),
    Some(ErrorCode::PasswordTooShort)
  );
  assert_eq!(
    code(run(guest.login(&login("lin", WRONG_PASSWORD)))),
    Some(ErrorCode::InvalidCredentials)
  );
  let register = |username: &str| RegisterRequest {
    username: username.into(),
    password: "secret1".into(),
    remember: false,
  };
  assert_eq!(
    code(run(guest.register(&register("a")))),
    Some(ErrorCode::UsernameInvalid)
  );
  assert_eq!(
    code(run(guest.register(&register(TAKEN_USERNAME)))),
    Some(ErrorCode::UsernameTaken)
  );
  // Cart rules.
  let m = member(FixtureSet::Contract);
  assert!(
    run(m.apply_coupon("NOPE"))
      .unwrap_err()
      .is(ErrorCode::CouponInvalid)
  );
  assert!(
    run(m.remove_cart_line("RT-XXX"))
      .unwrap_err()
      .is(ErrorCode::NotFound)
  );
  assert_eq!(
    run(guest.quote_cart(&CartQuoteRequest::default()))
      .unwrap()
      .totals
      .item_count,
    0
  );
  assert!(
    run(guest.product("nope"))
      .unwrap_err()
      .is(ErrorCode::NotFound)
  );
  // Address validation, in the server's order.
  let blank = AddressInput::default();
  assert!(
    run(m.create_address(&blank))
      .unwrap_err()
      .is(ErrorCode::RecipientRequired)
  );
  let bad_phone = AddressInput {
    recipient: "a".into(),
    region: "b".into(),
    detail: "c".into(),
    phone: Some("123".into()),
    ..AddressInput::default()
  };
  assert!(
    run(m.create_address(&bad_phone))
      .unwrap_err()
      .is(ErrorCode::PhoneInvalid)
  );
  // The empty scenario.
  let empty = member(FixtureSet::Empty);
  assert!(run(empty.products()).unwrap().items.is_empty());
  assert!(run(empty.cart()).unwrap().lines.is_empty());
  assert!(run(empty.addresses()).unwrap().items.is_empty());
  assert_eq!(run(empty.orders(None)).unwrap().total, 0);
  assert!(run(empty.profile()).unwrap().taste.is_none());
  // Down and error.
  assert!(
    run(client(FixtureSet::Down).products())
      .unwrap_err()
      .is_network()
  );
  assert!(
    run(client(FixtureSet::Error).products())
      .unwrap_err()
      .is(ErrorCode::InternalError)
  );
  // An unknown route is 404.
  let unknown: Result<Health, _> = run(guest.send(Call::new(endpoints::GET_HEALTH, "/nope")));
  assert!(unknown.unwrap_err().is(ErrorCode::NotFound));
}

#[test]
fn usernames_follow_the_contract_pattern() {
  for ok in ["林晓", "bean_lover", "ab", "咖啡豆2026", "a_1"] {
    assert!(username_is_valid(ok), "{ok}");
  }
  for bad in [
    "a",
    "",
    "has space",
    "bean-lover",
    "x".repeat(17).as_str(),
    "emoji☕",
  ] {
    assert!(!username_is_valid(bad), "{bad}");
  }
}

#[test]
fn every_member_answer_is_the_same_member() {
  let login = LoginRequest {
    username: "林晓".into(),
    password: "secret1".into(),
    remember: true,
  };
  let me = run(member(FixtureSet::Contract).me()).unwrap().user;
  assert_eq!(me.username, "林晓");
  for set in [FixtureSet::Contract, FixtureSet::Empty, FixtureSet::New] {
    for client in [member(set), client(set).with_token(Some("t".repeat(43)))] {
      assert_eq!(run(client.login(&login)).unwrap().user, me, "{set:?}");
      assert_eq!(run(client.profile()).unwrap().user, me, "{set:?}");
    }
  }
  // Without a key: her own profile (orders, taste), marked `no_key`.
  let keyless = client(FixtureSet::Contract).with_token(Some("t".repeat(43)));
  let profile = run(keyless.profile()).unwrap();
  assert_eq!(profile.remembered.state, RememberState::NoKey);
  assert_eq!(profile.stats.orders, 4);
  assert!(profile.taste.is_some());
  // A new member without a key: the new profile, marked `no_key`.
  let fresh = client(FixtureSet::Empty).with_token(Some("t".repeat(43)));
  let profile = run(fresh.profile()).unwrap();
  assert_eq!(profile.remembered.state, RememberState::NoKey);
  assert!(profile.taste.is_none());
  // Registering answers the register fixture with the typed name.
  let register = RegisterRequest {
    username: " 新会员_1 ".into(),
    password: "secret1".into(),
    remember: false,
  };
  let session = run(client(FixtureSet::Contract).register(&register)).unwrap();
  assert_eq!(session.user.username, "新会员_1");
  assert_eq!(session.user.id, me.id);
}

#[test]
fn the_state_scenarios_answer_as_documented() {
  let keyless = |set| client(set).with_fingerprint(Some("SHA256:k".into()));
  let order = PlaceOrderRequest {
    idempotency_key: "checkout-0001".into(),
    expected_total: None,
  };
  // new: one line, no address; checkout needs one; nothing else yet.
  let new = member(FixtureSet::New);
  let cart = run(new.cart()).unwrap();
  assert_eq!((cart.lines.len(), cart.address.is_none()), (1, true));
  assert_eq!(cart.totals.shipping, 12);
  assert_eq!(run(new.products()).unwrap().items.len(), 8);
  assert!(run(new.addresses()).unwrap().items.is_empty());
  assert_eq!(run(new.orders(None)).unwrap().total, 0);
  assert!(run(new.profile()).unwrap().taste.is_none());
  assert!(
    run(new.place_order(&order))
      .unwrap_err()
      .is(ErrorCode::AddressRequired)
  );
  assert!(
    run(keyless(FixtureSet::New).resume())
      .unwrap_err()
      .is(ErrorCode::NotRemembered)
  );
  // problem: short stock, a flagged line, failing checkout and address saves.
  let problem = member(FixtureSet::Problem);
  let stock = |id: &str| {
    let list = run(problem.products()).unwrap();
    list.items.into_iter().find(|p| p.id == id).unwrap().stock
  };
  assert_eq!(
    (stock("nyeri"), stock("geisha"), stock("yirga")),
    (0, 2, 42)
  );
  let nyeri = run(problem.product("nyeri")).unwrap();
  assert_eq!((nyeri.stock, nyeri.max_quantity), (0, 0));
  let geisha = run(problem.product("geisha")).unwrap();
  assert_eq!((geisha.stock, geisha.max_quantity), (2, 2));
  assert!(geisha.prices.iter().all(|cell| cell.totals.len() == 2));
  let cart = run(problem.cart()).unwrap();
  assert!(cart.lines.iter().any(|line| line.problem.is_some()));
  let add = |product: &str, quantity| AddCartLineRequest {
    product_id: product.into(),
    weight: Weight::G100,
    grind: Grind::WholeBean,
    pack: Pack::Standard,
    quantity,
    index: None,
  };
  let short = run(problem.add_cart_line(&add("geisha", 3))).unwrap_err();
  assert!(short.is(ErrorCode::OutOfStock));
  assert_eq!(short.details().unwrap().available, Some(2));
  assert!(run(problem.add_cart_line(&add("geisha", 2))).is_ok());
  let none = run(problem.add_cart_line(&add("nyeri", 1))).unwrap_err();
  let details = none.details().unwrap();
  assert_eq!(details.product_name.as_deref(), Some("涅里 AA 双洗"));
  assert_eq!(details.available, Some(0));
  let checkout = run(problem.place_order(&order)).unwrap_err();
  assert_eq!(
    (checkout.code(), checkout.details().unwrap().available),
    (Some(ErrorCode::OutOfStock), Some(2))
  );
  let input = AddressInput {
    recipient: "周一".into(),
    region: "上海市".into(),
    detail: "某路 1 号".into(),
    ..AddressInput::default()
  };
  for failure in [
    run(problem.create_address(&input)).unwrap_err(),
    run(problem.update_address(2, &input)).unwrap_err(),
  ] {
    assert!(failure.is(ErrorCode::InternalError));
  }
  assert_eq!(run(problem.addresses()).unwrap().items.len(), 3);
  // expired: a remembered key resumes, then every member call has expired.
  let expired = keyless(FixtureSet::Expired);
  assert_eq!(run(expired.resume()).unwrap().user.username, "林晓");
  let expired = member(FixtureSet::Expired);
  assert!(
    run(expired.cart())
      .unwrap_err()
      .is(ErrorCode::SessionExpired)
  );
  assert!(run(expired.products()).is_ok());
  // member-error: signing in works, member reads fail.
  let failing = member(FixtureSet::MemberError);
  let login = LoginRequest {
    username: "林晓".into(),
    password: "secret1".into(),
    remember: false,
  };
  assert!(run(failing.login(&login)).is_ok());
  assert!(run(failing.products()).is_ok());
  for failure in [
    run(failing.cart()).unwrap_err(),
    run(failing.addresses()).unwrap_err(),
    run(failing.profile()).unwrap_err(),
  ] {
    assert!(failure.is(ErrorCode::InternalError));
  }
  assert!(
    run(failing.orders(None))
      .unwrap_err()
      .is(ErrorCode::InternalError)
  );
}
