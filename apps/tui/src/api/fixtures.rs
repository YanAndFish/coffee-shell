//! The committed contract fixtures (`packages/contract/fixtures/v1/*.json`, worker A's),
//! served in process by [`super::Transport::Fixture`].

use super::{ApiFailure, Method};

/// `GET /v1/health`.
pub(super) const HEALTH: &str =
  include_str!("../../../../packages/contract/fixtures/v1/health.json");
/// `GET /v1/products`.
pub(super) const PRODUCTS: &str =
  include_str!("../../../../packages/contract/fixtures/v1/products.json");
/// `GET /v1/products` matching nothing (the empty state).
pub(super) const PRODUCTS_EMPTY: &str =
  include_str!("../../../../packages/contract/fixtures/v1/catalog/products-empty.json");
/// The error envelope (`404 not_found`).
pub(super) const ERROR: &str = include_str!("../../../../packages/contract/fixtures/v1/error.json");

/// Which answers the fixture transport gives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FixtureSet {
  /// The contract fixtures as committed: the eight beans.
  #[default]
  Contract,
  /// An empty product list (the empty state).
  Empty,
  /// No backend at all: every request fails as a refused connection does.
  Down,
}

impl FixtureSet {
  /// The set `name` names (`contract`, `empty`, `down`); anything else is the contract.
  pub fn parse(name: &str) -> Self {
    match name.trim() {
      "empty" => FixtureSet::Empty,
      "down" => FixtureSet::Down,
      _ => FixtureSet::Contract,
    }
  }
}

/// A fixture's answer.
pub(super) struct Answer {
  pub(super) status: u16,
  pub(super) body: &'static str,
}

impl FixtureSet {
  /// The answer to `method` `path`; an unknown route is the `404` envelope.
  pub(super) fn answer(self, method: Method, path: &str) -> Result<Answer, ApiFailure> {
    let ok = |body| Ok(Answer { status: 200, body });
    match (self, method, path) {
      (FixtureSet::Down, ..) => Err(ApiFailure::Network(
        "fixture:down · connection refused".into(),
      )),
      (_, Method::Get, "/health") => ok(HEALTH),
      (FixtureSet::Empty, Method::Get, "/products") => ok(PRODUCTS_EMPTY),
      (FixtureSet::Contract, Method::Get, "/products") => ok(PRODUCTS),
      _ => Ok(Answer {
        status: 404,
        body: ERROR,
      }),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn routes_answer_their_fixture() {
    let answer = FixtureSet::Contract
      .answer(Method::Get, "/products")
      .unwrap();
    assert_eq!((answer.status, answer.body), (200, PRODUCTS));
    let answer = FixtureSet::Empty.answer(Method::Get, "/products").unwrap();
    assert_eq!(answer.body, PRODUCTS_EMPTY);
    let answer = FixtureSet::Contract.answer(Method::Get, "/nope").unwrap();
    assert_eq!((answer.status, answer.body), (404, ERROR));
    assert!(matches!(
      FixtureSet::Down.answer(Method::Get, "/products"),
      Err(ApiFailure::Network(_))
    ));
  }

  #[test]
  fn names_parse() {
    assert_eq!(FixtureSet::parse(""), FixtureSet::Contract);
    assert_eq!(FixtureSet::parse("contract"), FixtureSet::Contract);
    assert_eq!(FixtureSet::parse("empty"), FixtureSet::Empty);
    assert_eq!(FixtureSet::parse("down"), FixtureSet::Down);
  }
}
