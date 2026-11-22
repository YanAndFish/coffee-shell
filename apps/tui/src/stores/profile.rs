//! The member's profile (06-profile.md): account, 记住我 state, stats, taste profile and
//! recommendations, as `GET /v1/profile` gives them.

use coffee_api::Profile;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// The profile's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct ProfileStore {
  session: SessionStore,
  profile: Remote<Profile>,
}

impl ProfileStore {
  /// Nothing loaded, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      profile: Remote::new(|_| false),
    }
  }

  /// The profile's state (tracked).
  pub fn state(&self) -> RemoteState<Profile> {
    self.profile.state()
  }

  /// The profile's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.profile.phase()
  }

  /// The failure of the last load (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.profile.failure()
  }

  /// The profile, once loaded (tracked).
  pub fn profile(&self) -> Option<Profile> {
    self.profile.data()
  }

  /// Loads the profile. Await it in tests.
  pub async fn fetch(self) {
    let client = self.session.client();
    let revision = self.profile.begin();
    let result = client.profile().await;
    self.profile.finish(revision, result);
  }

  /// Loads the profile in a task of the root's owner.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Loads the profile if nothing was asked yet (or it went stale).
  pub fn ensure_loaded(&self) {
    if self.profile.is_idle() {
      self.load();
    }
  }

  /// The profile is stale (an order, an address): reloaded on the next visit.
  pub fn invalidate(&self) {
    self.profile.reset();
  }

  /// Forgets it (logout).
  pub fn reset(&self) {
    self.profile.reset();
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  #[test]
  fn the_profile_loads_with_and_without_taste() {
    for (set, taste) in [(FixtureSet::Contract, true), (FixtureSet::Empty, false)] {
      let runtime = Runtime::default();
      let _entered = runtime.enter();
      let (owner, profile) = create_root(|| {
        let session = SessionStore::with_client(
          ApiClient::new(Transport::Fixture(set), None).with_fingerprint(Some("SHA256:k".into())),
        );
        let auth = serde_json::from_str(include_str!(
          "../../../../packages/contract/fixtures/v1/auth/login.json"
        ))
        .unwrap();
        session.signed_in(auth);
        ProfileStore::new(session)
      });
      now_or_never(profile.fetch());
      assert_eq!(profile.phase(), Phase::Ready);
      assert_eq!(profile.profile().unwrap().taste.is_some(), taste);
      profile.invalidate();
      assert_eq!(profile.phase(), Phase::Idle);
      owner.dispose();
    }
  }
}
