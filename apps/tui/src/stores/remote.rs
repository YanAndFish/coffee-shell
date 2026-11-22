//! [`Remote`]: data the backend owns, as a store holds it — the one loading pattern every
//! store uses.
//!
//! `idle → loading → ready | empty | failed`, and back to `loading` on a reload. The last
//! good data stays while reloading and after a failed reload (so a page keeps showing what
//! it had). Every load takes a revision ([`Remote::begin`]); an answer for an older one is
//! dropped ([`Remote::finish`]), so a slow first answer cannot overwrite a newer one, and a
//! direct write ([`Remote::set`], a mutation's result) supersedes every load in flight.

use nsh_core::prelude::*;

use crate::api::ApiFailure;

/// Where some remote data stands.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteState<T> {
  /// Nothing asked yet.
  Idle,
  /// A request is under way; the data shown before it, if any.
  Loading(Option<T>),
  /// The data, with something in it.
  Ready(T),
  /// The data, with nothing in it (an empty list, an empty cart).
  Empty(T),
  /// The request failed; the data shown before it, if any.
  Failed(ApiFailure, Option<T>),
}

impl<T> RemoteState<T> {
  /// The phase, without the data.
  pub fn phase(&self) -> Phase {
    match self {
      RemoteState::Idle => Phase::Idle,
      RemoteState::Loading(_) => Phase::Loading,
      RemoteState::Ready(_) => Phase::Ready,
      RemoteState::Empty(_) => Phase::Empty,
      RemoteState::Failed(..) => Phase::Failed,
    }
  }

  /// The data shown: the answer, or the last one while reloading or after a failure.
  pub fn data(&self) -> Option<&T> {
    match self {
      RemoteState::Idle => None,
      RemoteState::Loading(data) | RemoteState::Failed(_, data) => data.as_ref(),
      RemoteState::Ready(data) | RemoteState::Empty(data) => Some(data),
    }
  }

  /// The failure, when failed.
  pub fn failure(&self) -> Option<&ApiFailure> {
    match self {
      RemoteState::Failed(failure, _) => Some(failure),
      _ => None,
    }
  }
}

/// The phase of a [`RemoteState`]: what a page branches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
  /// Nothing asked yet.
  Idle,
  /// A request is under way.
  Loading,
  /// Data with something in it.
  Ready,
  /// Data with nothing in it.
  Empty,
  /// The last request failed.
  Failed,
}

impl Phase {
  /// Idle or loading: nothing settled yet.
  pub fn pending(self) -> bool {
    matches!(self, Phase::Idle | Phase::Loading)
  }
}

/// Remote data behind its functions. `Copy`: hold it anywhere.
pub struct Remote<T: 'static> {
  state: Signal<RemoteState<T>>,
  revision: Signal<u64>,
  empty: fn(&T) -> bool,
}

impl<T: 'static> Clone for Remote<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: 'static> Copy for Remote<T> {}

impl<T: Clone + PartialEq + 'static> Remote<T> {
  /// Idle data that counts as empty when `empty` says so.
  pub fn new(empty: fn(&T) -> bool) -> Self {
    Self {
      state: signal(RemoteState::Idle),
      revision: signal(0),
      empty,
    }
  }

  /// The state (tracked).
  pub fn state(&self) -> RemoteState<T> {
    self.state.get()
  }

  /// The phase (tracked).
  pub fn phase(&self) -> Phase {
    self.state.with(RemoteState::phase)
  }

  /// The phase, untracked: for handlers deciding what to do.
  pub fn phase_untracked(&self) -> Phase {
    self.state.with_untracked(RemoteState::phase)
  }

  /// The data shown (tracked; a clone).
  pub fn data(&self) -> Option<T> {
    self.state.with(|state| state.data().cloned())
  }

  /// `f` of the data shown (tracked).
  pub fn with<R>(&self, f: impl FnOnce(Option<&T>) -> R) -> R {
    self.state.with(|state| f(state.data()))
  }

  /// `f` of the data shown, untracked.
  pub fn with_untracked<R>(&self, f: impl FnOnce(Option<&T>) -> R) -> R {
    self.state.with_untracked(|state| f(state.data()))
  }

  /// The failure, when failed (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.state.with(|state| state.failure().cloned())
  }

  /// The revision of the latest load or write.
  pub fn revision(&self) -> u64 {
    self.revision.get_untracked()
  }

  fn bump(&self) -> u64 {
    let revision = self.revision.get_untracked() + 1;
    self.revision.set(revision);
    revision
  }

  /// Starts a load: the state is loading (keeping the data shown), and the revision its
  /// answer must carry.
  pub fn begin(&self) -> u64 {
    batch(|| {
      let revision = self.bump();
      let previous = self.state.with_untracked(|state| state.data().cloned());
      self.state.set(RemoteState::Loading(previous));
      revision
    })
  }

  /// Starts a request that keeps the state as it is (a mutation: the page keeps showing
  /// the data, not a loading state), and the revision its answer must carry.
  pub fn begin_quiet(&self) -> u64 {
    self.bump()
  }

  /// Whether `revision` is still the latest.
  pub fn is_current(&self, revision: u64) -> bool {
    revision == self.revision.get_untracked()
  }

  /// Applies the answer of request `revision`; an answer of an older request is dropped.
  /// Whether it applied.
  pub fn finish(&self, revision: u64, result: Result<T, ApiFailure>) -> bool {
    if !self.is_current(revision) {
      return false;
    }
    let next = match result {
      Ok(data) => self.settled(data),
      Err(failure) => {
        let previous = self.state.with_untracked(|state| state.data().cloned());
        RemoteState::Failed(failure, previous)
      }
    };
    self.state.set(next);
    true
  }

  /// Applies the answer of a quiet request `revision` only when it succeeded (a failed
  /// mutation leaves the data as it was). Whether it applied.
  pub fn finish_quiet(&self, revision: u64, result: Result<T, ApiFailure>) -> bool {
    match result {
      Ok(data) if self.is_current(revision) => {
        self.state.set(self.settled(data));
        true
      }
      _ => false,
    }
  }

  fn settled(&self, data: T) -> RemoteState<T> {
    if (self.empty)(&data) {
      RemoteState::Empty(data)
    } else {
      RemoteState::Ready(data)
    }
  }

  /// Sets the data (a mutation's answer); loads in flight are dropped.
  pub fn set(&self, data: T) {
    batch(|| {
      self.bump();
      self.state.set(self.settled(data));
    });
  }

  /// Forgets everything (logout): idle again; loads in flight are dropped.
  pub fn reset(&self) {
    batch(|| {
      self.bump();
      self.state.set(RemoteState::Idle);
    });
  }

  /// Whether nothing was asked yet (untracked).
  pub fn is_idle(&self) -> bool {
    self.phase_untracked() == Phase::Idle
  }

  /// Whether a load is under way (untracked).
  pub fn is_loading(&self) -> bool {
    self.phase_untracked() == Phase::Loading
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;

  fn with_remote(test: impl FnOnce(Remote<Vec<u8>>)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, remote) = create_root(|| Remote::new(Vec::is_empty));
    test(remote);
    owner.dispose();
  }

  #[test]
  fn loads_settle_into_ready_or_empty() {
    with_remote(|remote| {
      assert_eq!(remote.phase(), Phase::Idle);
      assert!(remote.phase().pending());
      let revision = remote.begin();
      assert_eq!(remote.state(), RemoteState::Loading(None));
      assert!(remote.finish(revision, Ok(vec![1])));
      assert_eq!(remote.phase(), Phase::Ready);
      let revision = remote.begin();
      assert_eq!(remote.state(), RemoteState::Loading(Some(vec![1])));
      remote.finish(revision, Ok(vec![]));
      assert_eq!(remote.state(), RemoteState::Empty(vec![]));
    });
  }

  #[test]
  fn a_failure_keeps_the_last_data() {
    with_remote(|remote| {
      let revision = remote.begin();
      remote.finish(revision, Ok(vec![7]));
      let revision = remote.begin();
      let failure = ApiFailure::Network("refused".into());
      remote.finish(revision, Err(failure.clone()));
      assert_eq!(remote.phase(), Phase::Failed);
      assert_eq!(remote.failure(), Some(failure));
      assert_eq!(remote.data(), Some(vec![7]));
    });
  }

  #[test]
  fn stale_answers_are_dropped() {
    with_remote(|remote| {
      let old = remote.begin();
      let new = remote.begin();
      assert!(remote.finish(new, Ok(vec![2])));
      assert!(!remote.finish(old, Ok(vec![1])));
      assert_eq!(remote.data(), Some(vec![2]));
      // A direct write supersedes a load in flight.
      let load = remote.begin();
      remote.set(vec![3]);
      assert!(!remote.finish(load, Ok(vec![4])));
      assert_eq!(remote.data(), Some(vec![3]));
      // A quiet request keeps the state, and a failed one changes nothing.
      let quiet = remote.begin_quiet();
      assert_eq!(remote.phase(), Phase::Ready);
      assert!(!remote.finish_quiet(quiet, Err(ApiFailure::Network("x".into()))));
      assert_eq!(remote.data(), Some(vec![3]));
      let quiet = remote.begin_quiet();
      assert!(remote.finish_quiet(quiet, Ok(vec![5])));
      assert_eq!(remote.data(), Some(vec![5]));
      remote.reset();
      assert_eq!(remote.state(), RemoteState::Idle);
    });
  }
}
