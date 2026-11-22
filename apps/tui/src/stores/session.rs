//! The session: who is shopping and how to reach the backend.
//!
//! It wraps [`crate::session_env`], which the host sets before the UI starts. Without one
//! (under `nsh run` and `nsh test`) the session is a guest's on the contract fixtures; there,
//! the storage key [`TRANSPORT_OVERRIDE_KEY`] may name another transport (`fixture:empty`,
//! `fixture:down`, or a live URL), which tests set with `open(".", { storage })`.

use nsh_core::{prelude::*, storage::Storage};

use crate::{
  SessionEnv,
  api::{ApiClient, FixtureSet, Transport},
};

/// The storage key that picks the transport when no host configured the session.
pub const TRANSPORT_OVERRIDE_KEY: &str = "coffee.transport";

/// The transport and internal key for `env`; with no env, `override_url` (from storage)
/// or the contract fixtures.
pub fn client_for(env: Option<&SessionEnv>, override_url: Option<&str>) -> ApiClient {
  match env {
    Some(env) => ApiClient::new(Transport::from_url(&env.api_url), env.internal_key.clone()),
    None => ApiClient::new(
      override_url
        .map(Transport::from_url)
        .unwrap_or(Transport::Fixture(FixtureSet::Contract)),
      None,
    ),
  }
}

/// The session's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct SessionStore {
  client: Signal<ApiClient>,
  user: Signal<Option<SharedString>>,
}

impl SessionStore {
  /// The session of this process: [`crate::session_env`], else the defaults above.
  pub fn new() -> Self {
    let override_url = crate::session_env()
      .is_none()
      .then(|| Storage::get_item(TRANSPORT_OVERRIDE_KEY))
      .flatten();
    Self::with_client(client_for(crate::session_env(), override_url.as_deref()))
  }

  /// A guest session over `client`.
  pub fn with_client(client: ApiClient) -> Self {
    Self {
      client: signal(client),
      user: signal(None),
    }
  }

  /// The backend client (untracked: requests read it when they start).
  pub fn client(&self) -> ApiClient {
    self.client.get_untracked()
  }

  /// Whether the session serves the fixtures rather than a backend (tracked).
  pub fn offline(&self) -> bool {
    self
      .client
      .with(|client| matches!(client.transport(), Transport::Fixture(_)))
  }

  /// The signed-in user's name; `None` for a guest (tracked). Phase 1 is guests only.
  pub fn user(&self) -> Option<SharedString> {
    self.user.get()
  }
}

impl Default for SessionStore {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_host_env_wins() {
    let env = SessionEnv {
      api_url: "http://127.0.0.1:3000".into(),
      internal_key: Some("k".into()),
      ..SessionEnv::default()
    };
    let client = client_for(Some(&env), Some("fixture:down"));
    assert_eq!(
      client,
      ApiClient::new(
        Transport::Live("http://127.0.0.1:3000".into()),
        Some("k".into())
      )
    );
  }

  #[test]
  fn without_a_host_the_fixtures_serve() {
    assert_eq!(
      client_for(None, None).transport(),
      &Transport::Fixture(FixtureSet::Contract)
    );
    assert_eq!(
      client_for(None, Some("fixture:empty")).transport(),
      &Transport::Fixture(FixtureSet::Empty)
    );
  }

  #[test]
  fn a_host_may_ask_for_the_fixtures() {
    let env = SessionEnv {
      api_url: "fixture:".into(),
      ..SessionEnv::default()
    };
    assert_eq!(
      client_for(Some(&env), None).transport(),
      &Transport::Fixture(FixtureSet::Contract)
    );
  }
}
