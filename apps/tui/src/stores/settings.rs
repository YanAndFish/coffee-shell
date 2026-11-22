//! UI settings (P-profile-6): 界面动画 (`m`) and 鼠标追踪 (`o`), both on by default. A
//! member's are saved on the account (`GET` / `PATCH /v1/settings`); a guest's live only in
//! this session. A toggle shows at once and rolls back if the server refuses it.

use coffee_api::{Settings, SettingsPatch};
use nsh_core::prelude::*;

use super::session::SessionStore;
use crate::api::ApiFailure;

/// The settings of a new account (and of guests).
pub const DEFAULT_SETTINGS: Settings = Settings {
  animations: true,
  mouse_tracking: true,
};

/// Which setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Setting {
  /// 界面动画.
  Animations,
  /// 鼠标追踪.
  MouseTracking,
}

/// The settings' state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct SettingsStore {
  session: SessionStore,
  settings: Signal<Settings>,
  revision: Signal<u64>,
}

impl SettingsStore {
  /// The defaults, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      settings: signal(DEFAULT_SETTINGS),
      revision: signal(0),
    }
  }

  /// The settings (tracked).
  pub fn settings(&self) -> Settings {
    self.settings.get()
  }

  /// 界面动画 (tracked): off stills the spinner, the steam and the progress bars.
  pub fn animations(&self) -> bool {
    self.settings.with(|s| s.animations)
  }

  /// 鼠标追踪 (tracked): `● 鼠标` / `○ 鼠标`.
  pub fn mouse_tracking(&self) -> bool {
    self.settings.with(|s| s.mouse_tracking)
  }

  fn bump(&self) -> u64 {
    let revision = self.revision.get_untracked() + 1;
    self.revision.set(revision);
    revision
  }

  /// Loads a member's settings. Await it in tests.
  pub async fn fetch(self) {
    let client = self.session.client();
    let revision = self.bump();
    if let Ok(settings) = client.settings().await
      && revision == self.revision.get_untracked()
    {
      self.settings.set(settings);
    }
  }

  /// Loads a member's settings in a task of the root's owner.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Toggles `setting`: shown at once; for a member saved on the account (rolled back if
  /// the server refuses). The new value.
  pub async fn toggle(self, setting: Setting) -> Result<bool, ApiFailure> {
    let before = self.settings.get_untracked();
    let mut next = before;
    let value = match setting {
      Setting::Animations => {
        next.animations = !before.animations;
        next.animations
      }
      Setting::MouseTracking => {
        next.mouse_tracking = !before.mouse_tracking;
        next.mouse_tracking
      }
    };
    let revision = self.bump();
    self.settings.set(next);
    if !self.session.is_member_untracked() {
      return Ok(value);
    }
    let patch = match setting {
      Setting::Animations => SettingsPatch {
        animations: Some(value),
        mouse_tracking: None,
      },
      Setting::MouseTracking => SettingsPatch {
        animations: None,
        mouse_tracking: Some(value),
      },
    };
    let client = self.session.client();
    let result = client.update_settings(&patch).await;
    if revision == self.revision.get_untracked() {
      match &result {
        Ok(saved) => self.settings.set(*saved),
        Err(_) => self.settings.set(before),
      }
    }
    result.map(|_| value)
  }

  /// Back to the defaults (logout: a guest's settings start fresh).
  pub fn reset(&self) {
    self.bump();
    self.settings.set(DEFAULT_SETTINGS);
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  #[test]
  fn guests_toggle_locally_and_members_save_or_roll_back() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, (session, settings)) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(
        Transport::Fixture(FixtureSet::Contract),
        None,
      ));
      (session, SettingsStore::new(session))
    });
    assert!(settings.animations() && settings.mouse_tracking());
    assert_eq!(
      now_or_never(settings.toggle(Setting::Animations)),
      Ok(false)
    );
    assert!(!settings.animations(), "a guest's setting stays local");
    settings.reset();
    let auth = serde_json::from_str(include_str!(
      "../../../../packages/contract/fixtures/v1/auth/login.json"
    ))
    .unwrap();
    session.signed_in(auth);
    now_or_never(settings.fetch());
    assert_eq!(
      now_or_never(settings.toggle(Setting::Animations)),
      Ok(false)
    );
    assert!(!settings.animations());
    assert_eq!(
      now_or_never(settings.toggle(Setting::MouseTracking)),
      Ok(false)
    );
    owner.dispose();

    // A refused save rolls back.
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, settings) = create_root(|| {
      let session =
        SessionStore::with_client(ApiClient::new(Transport::Fixture(FixtureSet::Down), None));
      let auth = serde_json::from_str(include_str!(
        "../../../../packages/contract/fixtures/v1/auth/login.json"
      ))
      .unwrap();
      session.signed_in(auth);
      SettingsStore::new(session)
    });
    assert!(now_or_never(settings.toggle(Setting::MouseTracking)).is_err());
    assert!(settings.mouse_tracking());
    owner.dispose();
  }
}
