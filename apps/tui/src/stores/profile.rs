//! The member's profile (06-profile.md): account, 记住我 state, stats, taste profile and
//! recommendations, as `GET /v1/profile` gives them; and the page's Tab focus ring
//! (`动画开关 → 鼠标开关 → 退出登录 → 推荐按钮 → 订单列表`).

use coffee_api::Profile;
use nsh_core::prelude::*;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// What the profile page's Tab ring has focused (06-profile.md, "焦点与按键").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProfileFocus {
  /// `界面动画` (`⏎` toggles it).
  Animations,
  /// `鼠标追踪` (`⏎` toggles it).
  MouseTracking,
  /// `q 退出登录` (`⏎` asks to log out).
  Logout,
  /// The recommendation at this index (`⏎` opens it).
  Recommendation(usize),
  /// The order list (PageUp / PageDown scroll it).
  Orders,
}

/// The Tab ring with `recommendations` buttons, in order.
pub fn focus_ring(recommendations: usize) -> Vec<ProfileFocus> {
  let mut ring = vec![
    ProfileFocus::Animations,
    ProfileFocus::MouseTracking,
    ProfileFocus::Logout,
  ];
  ring.extend((0..recommendations).map(ProfileFocus::Recommendation));
  ring.push(ProfileFocus::Orders);
  ring
}

/// The focus after `from` in `ring` (`forward`: Tab, else Shift+Tab), wrapping around;
/// from nothing (or a target no longer in the ring), the first or the last.
pub fn next_focus(
  ring: &[ProfileFocus],
  from: Option<ProfileFocus>,
  forward: bool,
) -> ProfileFocus {
  let at = from.and_then(|from| ring.iter().position(|&f| f == from));
  let len = ring.len();
  let index = match (at, forward) {
    (Some(i), true) => (i + 1) % len,
    (Some(i), false) => (i + len - 1) % len,
    (None, true) => 0,
    (None, false) => len - 1,
  };
  ring[index]
}

/// The profile's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct ProfileStore {
  session: SessionStore,
  profile: Remote<Profile>,
  focus: Signal<Option<ProfileFocus>>,
}

impl ProfileStore {
  /// Nothing loaded, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      profile: Remote::new(|_| false),
      focus: signal(None),
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
    if let Err(failure) = &result {
      self.session.report(failure);
    }
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

  /// Forgets it and the focus (logout).
  pub fn reset(&self) {
    batch(|| {
      self.profile.reset();
      self.focus.set(None);
    });
  }

  /// What the page's Tab ring has focused (tracked).
  pub fn focus(&self) -> Option<ProfileFocus> {
    self.focus.get()
  }

  /// What the page's Tab ring has focused (untracked: for commands).
  pub fn focus_untracked(&self) -> Option<ProfileFocus> {
    self.focus.get_untracked()
  }

  /// Whether `target` has the focus (tracked).
  pub fn is_focused(&self, target: ProfileFocus) -> bool {
    self.focus.with(|focus| *focus == Some(target))
  }

  /// Tab (`forward`) or Shift+Tab: the next target of the ring, the profile's
  /// recommendations included.
  pub fn move_focus(&self, forward: bool) {
    let recommendations = self
      .profile
      .with_untracked(|profile| profile.map_or(0, |p| p.recommendations.len()));
    let ring = focus_ring(recommendations);
    let next = next_focus(&ring, self.focus.get_untracked(), forward);
    self.focus.set(Some(next));
  }

  /// Puts the focus on `target`, or nowhere.
  pub fn set_focus(&self, target: Option<ProfileFocus>) {
    self.focus.set(target);
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

  #[test]
  fn the_focus_ring_wraps_both_ways() {
    let ring = focus_ring(2);
    assert_eq!(
      ring,
      [
        ProfileFocus::Animations,
        ProfileFocus::MouseTracking,
        ProfileFocus::Logout,
        ProfileFocus::Recommendation(0),
        ProfileFocus::Recommendation(1),
        ProfileFocus::Orders,
      ]
    );
    assert_eq!(next_focus(&ring, None, true), ProfileFocus::Animations);
    assert_eq!(next_focus(&ring, None, false), ProfileFocus::Orders);
    assert_eq!(
      next_focus(&ring, Some(ProfileFocus::Logout), true),
      ProfileFocus::Recommendation(0)
    );
    assert_eq!(
      next_focus(&ring, Some(ProfileFocus::Orders), true),
      ProfileFocus::Animations
    );
    assert_eq!(
      next_focus(&ring, Some(ProfileFocus::Animations), false),
      ProfileFocus::Orders
    );
    // A recommendation that went away (a new profile has none): from the start.
    let none = focus_ring(0);
    assert_eq!(
      next_focus(&none, Some(ProfileFocus::Recommendation(1)), true),
      ProfileFocus::Animations
    );
    assert_eq!(
      next_focus(&none, Some(ProfileFocus::Logout), true),
      ProfileFocus::Orders
    );
  }

  #[test]
  fn tab_walks_the_ring_and_logout_clears_it() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, profile) = create_root(|| {
      let session = SessionStore::with_client(
        ApiClient::new(Transport::Fixture(FixtureSet::Contract), None)
          .with_fingerprint(Some("SHA256:k".into())),
      );
      let auth = serde_json::from_str(include_str!(
        "../../../../packages/contract/fixtures/v1/auth/login.json"
      ))
      .unwrap();
      session.signed_in(auth);
      ProfileStore::new(session)
    });
    now_or_never(profile.fetch());
    assert_eq!(profile.focus(), None);
    for _ in 0..4 {
      profile.move_focus(true);
    }
    assert_eq!(profile.focus(), Some(ProfileFocus::Recommendation(0)));
    assert!(profile.is_focused(ProfileFocus::Recommendation(0)));
    profile.move_focus(false);
    assert_eq!(profile.focus_untracked(), Some(ProfileFocus::Logout));
    profile.reset();
    assert_eq!(profile.focus(), None);
    owner.dispose();
  }
}
