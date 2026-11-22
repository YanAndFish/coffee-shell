//! The session: who is shopping (guest or member), how to reach the backend, and the login /
//! register form.
//!
//! It wraps [`crate::session_env`], which the host sets before the UI starts. Without one
//! (under `nsh run` and `nsh test`) the session is a guest's on the contract fixtures; there,
//! the storage keys [`TRANSPORT_OVERRIDE_KEY`] (`fixture:member`, `fixture:empty`,
//! `fixture:down`, a live URL…) and [`FINGERPRINT_OVERRIDE_KEY`] (an SSH key fingerprint)
//! stand in for the host, which tests set with `open(".", { storage })`.
//!
//! The member token lives in memory only (P-api-1): [`SessionStore::client`] hands out a
//! client carrying it. The async work (register, login, resume, logout) is in
//! [`super::actions`], which drives this store through `begin_auth` → `auth_step` →
//! `signed_in` / `auth_failed`.

use coffee_api::{AuthSession, LoginRequest, RegisterRequest, RememberState, RememberStatus, User};
use nsh_core::{prelude::*, storage::Storage};

use crate::{
  SessionEnv,
  api::{ApiClient, ApiFailure, ErrorCode, FixtureSet, Transport, username_is_valid},
  router::Route,
};

/// The storage key that picks the transport when no host configured the session.
pub const TRANSPORT_OVERRIDE_KEY: &str = "coffee.transport";

/// The storage key that gives the connection an SSH key fingerprint when no host configured
/// the session (tests of 记住我 and resume).
pub const FINGERPRINT_OVERRIDE_KEY: &str = "coffee.fingerprint";

/// The transport, internal key and connection facts for `env`; with no env, `override_url`
/// (from storage) or the contract fixtures, and `override_fingerprint`. Fixture answers
/// take one engine turn, as the network would.
pub fn client_for(
  env: Option<&SessionEnv>,
  override_url: Option<&str>,
  override_fingerprint: Option<&str>,
) -> ApiClient {
  let client = match env {
    Some(env) => ApiClient::new(Transport::from_url(&env.api_url), env.internal_key.clone())
      .with_fingerprint(env.ssh_fingerprint.clone())
      .with_client_ip(env.client_ip.clone()),
    None => ApiClient::new(
      override_url
        .map(Transport::from_url)
        .unwrap_or(Transport::Fixture(FixtureSet::Contract)),
      None,
    )
    .with_fingerprint(override_fingerprint.map(str::to_owned)),
  };
  client.with_fixture_latency()
}

/// A signed-in member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
  /// Who.
  pub user: User,
  /// 记住我 for this connection.
  pub remembered: RememberStatus,
}

/// The form's mode: `登录 · SIGN IN` or `注册 · SIGN UP`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LoginMode {
  /// Sign in.
  #[default]
  Login,
  /// Create an account (D4).
  Register,
}

/// The login / register form, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoginDraft {
  /// Login or register.
  pub mode: LoginMode,
  /// `用户名`.
  pub username: String,
  /// `密码` (never logged, cleared after every attempt that fails on it).
  pub password: String,
  /// `记住我 30 天` (only with a key; checked by default then).
  pub remember: bool,
  /// The error line, without its `✗ `: local validation or the server's message.
  pub error: Option<String>,
}

/// The progress of a submit (01-login.md: 连接 0–35%, 校验 35–70%, 同步 70–100%).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AuthStep {
  /// `握手 · 建立安全连接`: the request is out.
  Connect,
  /// `校验凭据`: the session came back.
  Verify,
  /// `同步购物车与地址本`: merging the guest cart, loading cart and addresses.
  Sync,
}

impl AuthStep {
  /// The step's copy.
  pub fn label(self) -> &'static str {
    match self {
      AuthStep::Connect => "握手 · 建立安全连接",
      AuthStep::Verify => "校验凭据",
      AuthStep::Sync => "同步购物车与地址本",
    }
  }

  /// Where the progress bar starts for this step, in percent.
  pub fn percent(self) -> u8 {
    match self {
      AuthStep::Connect => 0,
      AuthStep::Verify => 35,
      AuthStep::Sync => 70,
    }
  }
}

/// A control of the login card that holds the keyboard focus (01-login.md, "焦点顺序"):
/// Tab goes 用户名 → 密码 → 记住我 → 主按钮 → 访客浏览 → 注册 / 去登录, round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormControl {
  /// The `用户名` field.
  Username,
  /// The `密码` field.
  Password,
  /// `记住我 30 天`.
  Remember,
  /// The primary button: `登录 ⏎` / `注册并登录 ⏎`.
  Submit,
  /// `g 访客浏览`.
  Guest,
  /// `注册新账号 →` / `← 已有账号，去登录`.
  SwitchMode,
}

impl FormControl {
  /// Whether it is a text field (letters type into it).
  pub fn is_field(self) -> bool {
    matches!(self, FormControl::Username | FormControl::Password)
  }
}

/// The network copy of the login form (copy.md, "校验错误").
pub const LOGIN_NETWORK_ERROR: &str = "连接服务器失败，按 ⏎ 重试";

/// Local validation of `draft` (before any request), in the form's order; the copy of the
/// first problem.
pub fn validate(draft: &LoginDraft) -> Result<(), &'static str> {
  let username = draft.username.trim();
  if username.is_empty() {
    return Err(ErrorCode::UsernameRequired.message());
  }
  if draft.mode == LoginMode::Register && !username_is_valid(username) {
    return Err(ErrorCode::UsernameInvalid.message());
  }
  if draft.password.chars().count() < 6 {
    return Err(ErrorCode::PasswordTooShort.message());
  }
  Ok(())
}

/// What the form says about `failure`.
pub fn auth_error(failure: &ApiFailure) -> String {
  if failure.is_network() {
    LOGIN_NETWORK_ERROR.to_owned()
  } else {
    failure.message()
  }
}

/// The session's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct SessionStore {
  base: Signal<ApiClient>,
  token: Signal<Option<String>>,
  member: Signal<Option<Member>>,
  resuming: Signal<bool>,
  draft: Signal<LoginDraft>,
  form_focus: Signal<Option<FormControl>>,
  step: Signal<Option<AuthStep>>,
  auth_revision: Signal<u64>,
  logging_out: Signal<bool>,
  return_to: Signal<Option<Route>>,
  expired: Signal<bool>,
  /// The owner tasks run under (the root's: a task outlives the page that started it).
  owner: Owner,
  /// Run tasks to their end in place (tests on the fixture transport).
  inline: bool,
}

impl SessionStore {
  /// The session of this process: [`crate::session_env`], else the storage overrides.
  pub fn new() -> Self {
    let no_host = crate::session_env().is_none();
    let stored = |key| no_host.then(|| Storage::get_item(key)).flatten();
    Self::with_client(client_for(
      crate::session_env(),
      stored(TRANSPORT_OVERRIDE_KEY).as_deref(),
      stored(FINGERPRINT_OVERRIDE_KEY).as_deref(),
    ))
  }

  /// A guest session over `client` (its fingerprint says whether the connection has a key).
  pub fn with_client(client: ApiClient) -> Self {
    Self::make(client, false)
  }

  /// [`Self::with_client`] whose tasks run to their end in place, at once: for tests on
  /// the fixture transport, which never waits (a live request would panic).
  pub fn inline(client: ApiClient) -> Self {
    Self::make(client, true)
  }

  fn make(client: ApiClient, inline: bool) -> Self {
    let has_key = client.fingerprint().is_some();
    Self {
      base: signal(client),
      token: signal(None),
      member: signal(None),
      resuming: signal(false),
      draft: signal(LoginDraft {
        remember: has_key,
        ..LoginDraft::default()
      }),
      form_focus: signal(None),
      step: signal(None),
      auth_revision: signal(0),
      logging_out: signal(false),
      return_to: signal(None),
      expired: signal(false),
      owner: Owner::current(),
      inline,
    }
  }

  /// Runs `future` as a task of the root's owner (a page going away does not cancel it),
  /// or to its end at once for an inline session. Each stretch of the task between two
  /// awaits runs in one `batch`: the UI sees its changes together, never half of them.
  pub fn spawn(&self, future: impl Future<Output = ()> + 'static) {
    let future = Batched(Box::pin(future));
    if self.inline {
      self.owner.run(|| crate::api::now_or_never(future));
    } else {
      self.owner.run(|| {
        spawn(future);
      });
    }
  }

  /// The backend client, carrying the member token when signed in (untracked: requests
  /// read it when they start).
  pub fn client(&self) -> ApiClient {
    self
      .base
      .get_untracked()
      .with_token(self.token.get_untracked())
  }

  /// Whether the session serves the fixtures rather than a backend (tracked).
  pub fn offline(&self) -> bool {
    self
      .base
      .with(|client| matches!(client.transport(), Transport::Fixture(_)))
  }

  /// Whether the connection offered an SSH key (`● 公钥已识别` / `○ 未提供公钥`).
  pub fn has_key(&self) -> bool {
    self
      .base
      .with_untracked(|client| client.fingerprint().is_some())
  }

  /// Whether a member is signed in (tracked).
  pub fn is_member(&self) -> bool {
    self.member.with(Option::is_some)
  }

  /// Whether a member is signed in, untracked: for handlers.
  pub fn is_member_untracked(&self) -> bool {
    self.member.with_untracked(Option::is_some)
  }

  /// The member, if signed in (tracked).
  pub fn member(&self) -> Option<Member> {
    self.member.get()
  }

  /// The signed-in user's name; `None` for a guest (tracked).
  pub fn user(&self) -> Option<SharedString> {
    self
      .member
      .with(|member| member.as_ref().map(|m| m.user.username.clone().into()))
  }

  /// 记住我 for this connection, for a member (tracked).
  pub fn remembered(&self) -> Option<RememberStatus> {
    self
      .member
      .with(|member| member.as_ref().map(|m| m.remembered.clone()))
  }

  /// Whether this connection's key is remembered for the member (tracked): the logout
  /// confirmation then says `同时解除当前公钥的免密登录。`.
  pub fn key_remembered(&self) -> bool {
    self
      .remembered()
      .is_some_and(|r| r.state == RememberState::Remembered)
  }

  /// Whether the resume-on-connect is under way (tracked).
  pub fn resuming(&self) -> bool {
    self.resuming.get()
  }

  /// Marks the resume-on-connect under way or done.
  pub fn set_resuming(&self, resuming: bool) {
    self.resuming.set(resuming);
  }

  // --- The form ---------------------------------------------------------------------

  /// The form (tracked).
  pub fn draft(&self) -> LoginDraft {
    self.draft.get()
  }

  /// The form's mode (tracked).
  pub fn mode(&self) -> LoginMode {
    self.draft.with(|draft| draft.mode)
  }

  fn edit(&self, f: impl FnOnce(&mut LoginDraft)) {
    self.draft.update(|draft| {
      f(draft);
      // Any change clears the error line (源稿 `err: ''`).
      draft.error = None;
    });
  }

  /// Sets `用户名`.
  pub fn set_username(&self, username: &str) {
    self.edit(|draft| draft.username = username.to_owned());
  }

  /// Sets `密码`.
  pub fn set_password(&self, password: &str) {
    self.edit(|draft| draft.password = password.to_owned());
  }

  /// Toggles `记住我` (nothing without a key: the row is disabled).
  pub fn toggle_remember(&self) {
    if self.has_key() {
      self.edit(|draft| draft.remember = !draft.remember);
    }
  }

  /// Switches the form to `mode`, keeping the username and clearing password and error.
  pub fn set_mode(&self, mode: LoginMode) {
    self.draft.update(|draft| {
      if draft.mode != mode {
        draft.mode = mode;
        draft.password.clear();
        draft.error = None;
      }
    });
  }

  /// Shows `message` on the error line.
  pub fn set_error(&self, message: impl Into<String>) {
    let message = message.into();
    self.draft.update(|draft| draft.error = Some(message));
  }

  /// The card's control with the keyboard focus; `None`: no focus (tracked). The page
  /// follows it (a field takes nsh's text focus, a button shows `›`) and reports the moves
  /// Tab and the mouse make; ⏎ and space press what it names.
  pub fn form_focus(&self) -> Option<FormControl> {
    self.form_focus.get()
  }

  /// [`Self::form_focus`], untracked: for handlers.
  pub fn form_focus_untracked(&self) -> Option<FormControl> {
    self.form_focus.get_untracked()
  }

  /// Puts the card's keyboard focus on `control` (`None`: nowhere).
  pub fn set_form_focus(&self, control: Option<FormControl>) {
    self.form_focus.set(control);
  }

  /// `control` lost the keyboard focus: no focus, unless another control took it since.
  pub fn form_focus_left(&self, control: FormControl) {
    if self.form_focus.get_untracked() == Some(control) {
      self.form_focus.set(None);
    }
  }

  /// The register request the form makes.
  pub fn register_request(&self) -> RegisterRequest {
    let has_key = self.has_key();
    self.draft.with_untracked(|draft| RegisterRequest {
      username: draft.username.trim().to_owned(),
      password: draft.password.clone(),
      remember: draft.remember && has_key,
    })
  }

  /// The login request the form makes.
  pub fn login_request(&self) -> LoginRequest {
    let has_key = self.has_key();
    self.draft.with_untracked(|draft| LoginRequest {
      username: draft.username.trim().to_owned(),
      password: draft.password.clone(),
      remember: draft.remember && has_key,
    })
  }

  // --- Submitting -------------------------------------------------------------------

  /// The submit's progress; `None` when not submitting (tracked).
  pub fn step(&self) -> Option<AuthStep> {
    self.step.get()
  }

  /// Whether a register / login is under way (tracked): only `?` works meanwhile.
  pub fn submitting(&self) -> bool {
    self.step.with(Option::is_some)
  }

  /// Starts a submit: validates the form locally (the error line on a problem), else the
  /// revision the answers must carry. `None` while one is under way or on a problem.
  pub fn begin_auth(&self) -> Option<u64> {
    if self.step.get_untracked().is_some() {
      return None;
    }
    if let Err(message) = self.draft.with_untracked(validate) {
      self.set_error(message);
      return None;
    }
    let revision = self.auth_revision.get_untracked() + 1;
    batch(|| {
      self.auth_revision.set(revision);
      self.step.set(Some(AuthStep::Connect));
      self.draft.update(|draft| draft.error = None);
    });
    Some(revision)
  }

  fn current(&self, revision: u64) -> bool {
    revision == self.auth_revision.get_untracked()
  }

  /// Moves submit `revision` to `step`.
  pub fn auth_step(&self, revision: u64, step: AuthStep) {
    if self.current(revision) {
      self.step.set(Some(step));
    }
  }

  /// Submit `revision` failed: the error line, and the password cleared when it was wrong
  /// (01-login.md: "密码框清空"). Network failures keep the input (`按 ⏎ 重试`).
  pub fn auth_failed(&self, revision: u64, failure: &ApiFailure) {
    if !self.current(revision) {
      return;
    }
    let message = auth_error(failure);
    let clear = failure.is(ErrorCode::InvalidCredentials);
    batch(|| {
      self.step.set(None);
      self.draft.update(|draft| {
        if clear {
          draft.password.clear();
        }
        draft.error = Some(message);
      });
    });
  }

  /// Submit `revision` is over (signed in and synced).
  pub fn auth_done(&self, revision: u64) {
    if self.current(revision) {
      self.step.set(None);
    }
  }

  /// Signs `session` in: the token (memory only) and the member. The form's password goes.
  pub fn signed_in(&self, session: AuthSession) {
    batch(|| {
      self.token.set(Some(session.token));
      self.member.set(Some(Member {
        user: session.user,
        remembered: session.remembered,
      }));
      self.resuming.set(false);
      self.draft.update(|draft| {
        draft.password.clear();
        draft.error = None;
        draft.mode = LoginMode::Login;
      });
    });
  }

  /// Ends the member session locally (logout, expiry): a guest again.
  pub fn signed_out(&self) {
    batch(|| {
      self.token.set(None);
      self.member.set(None);
      self.logging_out.set(false);
      self.expired.set(false);
      self.step.set(None);
      // A pending submit's answer is dropped.
      self
        .auth_revision
        .set(self.auth_revision.get_untracked() + 1);
    });
  }

  /// Whether a logout is under way (tracked): `⠹ 正在退出…`, only `?` works.
  pub fn logging_out(&self) -> bool {
    self.logging_out.get()
  }

  /// Whether a logout is under way, untracked: for handlers.
  pub fn logging_out_untracked(&self) -> bool {
    self.logging_out.get_untracked()
  }

  /// Marks a logout under way.
  pub fn set_logging_out(&self, logging_out: bool) {
    self.logging_out.set(logging_out);
  }

  /// A store's request failed with `failure`: when it says the member session is gone
  /// (`401 session_expired` / `login_required` while signed in), the session is marked
  /// expired and the shell runs `actions::session_expired` (back to the login page).
  pub fn report(&self, failure: &ApiFailure) {
    if failure.ends_session() && self.is_member_untracked() && !self.expired.get_untracked() {
      self.expired.set(true);
    }
  }

  /// Whether a store saw the member session end (tracked).
  pub fn expired(&self) -> bool {
    self.expired.get()
  }

  // --- Where to go after login ------------------------------------------------------

  /// The route to show after login (tracked): the cart, when checkout sent a guest here.
  pub fn return_to(&self) -> Option<Route> {
    self.return_to.get()
  }

  /// Remembers where to go after login.
  pub fn set_return_to(&self, route: Option<Route>) {
    self.return_to.set(route);
  }

  /// The route to show after login, forgotten once taken.
  pub fn take_return_to(&self) -> Option<Route> {
    let route = self.return_to.get_untracked();
    if route.is_some() {
      self.return_to.set(None);
    }
    route
  }
}

/// A future whose every poll runs in one `batch`.
struct Batched(std::pin::Pin<Box<dyn Future<Output = ()>>>);

impl Future for Batched {
  type Output = ();

  fn poll(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<()> {
    batch(|| self.0.as_mut().poll(cx))
  }
}

impl Default for SessionStore {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::now_or_never;

  #[test]
  fn the_host_env_wins() {
    let env = SessionEnv {
      api_url: "http://127.0.0.1:3000".into(),
      internal_key: Some("k".into()),
      ssh_fingerprint: Some("SHA256:abc".into()),
      client_ip: Some("10.0.0.2".into()),
      ..SessionEnv::default()
    };
    let client = client_for(Some(&env), Some("fixture:down"), Some("SHA256:other"));
    assert_eq!(
      client,
      ApiClient::new(
        Transport::Live("http://127.0.0.1:3000".into()),
        Some("k".into())
      )
      .with_fingerprint(Some("SHA256:abc".into()))
      .with_client_ip(Some("10.0.0.2".into()))
      .with_fixture_latency()
    );
  }

  #[test]
  fn without_a_host_the_fixtures_and_the_overrides_serve() {
    assert_eq!(
      client_for(None, None, None).transport(),
      &Transport::Fixture(FixtureSet::Contract)
    );
    let client = client_for(None, Some("fixture:member"), Some("SHA256:k"));
    assert_eq!(client.transport(), &Transport::Fixture(FixtureSet::Member));
    assert_eq!(client.fingerprint(), Some("SHA256:k"));
    let env = SessionEnv {
      api_url: "fixture:empty".into(),
      ..SessionEnv::default()
    };
    assert_eq!(
      client_for(Some(&env), None, None).transport(),
      &Transport::Fixture(FixtureSet::Empty)
    );
  }

  fn draft(mode: LoginMode, username: &str, password: &str) -> LoginDraft {
    LoginDraft {
      mode,
      username: username.into(),
      password: password.into(),
      ..LoginDraft::default()
    }
  }

  #[test]
  fn local_validation_follows_the_form_order() {
    assert_eq!(
      validate(&draft(LoginMode::Login, " ", "")),
      Err("请输入用户名")
    );
    assert_eq!(
      validate(&draft(LoginMode::Login, "x", "12345")),
      Err("密码至少 6 位")
    );
    assert_eq!(validate(&draft(LoginMode::Login, "x", "123456")), Ok(()));
    assert_eq!(
      validate(&draft(LoginMode::Register, "x", "123456")),
      Err("用户名需为 2–16 个字符，可用中文、字母、数字和下划线")
    );
    assert_eq!(
      validate(&draft(LoginMode::Register, "林晓", "123456")),
      Ok(())
    );
    assert_eq!(
      auth_error(&ApiFailure::Network("refused".into())),
      LOGIN_NETWORK_ERROR
    );
  }

  fn with_session(client: ApiClient, test: impl FnOnce(SessionStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, session) = create_root(|| SessionStore::with_client(client));
    test(session);
    owner.dispose();
  }

  fn fixture(fingerprint: Option<&str>) -> ApiClient {
    ApiClient::new(Transport::Fixture(FixtureSet::Contract), None)
      .with_fingerprint(fingerprint.map(Into::into))
  }

  #[test]
  fn the_form_edits_and_switches_modes() {
    with_session(fixture(Some("SHA256:k")), |session| {
      assert!(session.has_key());
      assert!(session.draft().remember, "checked by default with a key");
      session.set_username("林晓");
      session.set_password("secret");
      session.set_error("x");
      session.set_password("secret1");
      assert_eq!(session.draft().error, None, "an edit clears the error");
      session.toggle_remember();
      assert!(!session.login_request().remember);
      session.set_mode(LoginMode::Register);
      let draft = session.draft();
      assert_eq!(
        (draft.mode, draft.username.as_str(), draft.password.as_str()),
        (LoginMode::Register, "林晓", "")
      );
    });
    with_session(fixture(None), |session| {
      assert!(!session.has_key());
      assert!(!session.draft().remember);
      session.toggle_remember();
      assert!(!session.draft().remember, "disabled without a key");
    });
  }

  #[test]
  fn a_submit_validates_then_steps_and_signs_in() {
    with_session(fixture(None), |session| {
      assert_eq!(session.begin_auth(), None);
      assert_eq!(session.draft().error.as_deref(), Some("请输入用户名"));
      session.set_username("林晓");
      session.set_password("secret1");
      let revision = session.begin_auth().unwrap();
      assert_eq!(session.step(), Some(AuthStep::Connect));
      assert_eq!(session.begin_auth(), None, "one submit at a time");
      session.auth_step(revision, AuthStep::Verify);
      let auth = now_or_never(session.client().login(&session.login_request())).unwrap();
      session.signed_in(auth);
      session.auth_done(revision);
      assert!(session.is_member());
      assert_eq!(session.user().as_deref(), Some("林晓"));
      assert!(session.client().signed_in());
      assert!(!session.key_remembered());
      assert_eq!(session.step(), None);
      assert_eq!(session.draft().password, "");
      session.signed_out();
      assert!(!session.is_member());
      assert!(!session.client().signed_in());
    });
  }

  #[test]
  fn a_wrong_password_clears_it_and_a_network_error_keeps_it() {
    with_session(fixture(None), |session| {
      session.set_username("林晓");
      session.set_password("wrongpass");
      let revision = session.begin_auth().unwrap();
      session.auth_failed(revision, &ApiFailure::of(ErrorCode::InvalidCredentials));
      let draft = session.draft();
      assert_eq!(draft.error.as_deref(), Some("用户名或密码不正确"));
      assert_eq!(draft.password, "");
      assert!(!session.submitting());

      session.set_password("secret1");
      let revision = session.begin_auth().unwrap();
      session.auth_failed(revision, &ApiFailure::Network("refused".into()));
      assert_eq!(session.draft().error.as_deref(), Some(LOGIN_NETWORK_ERROR));
      assert_eq!(session.draft().password, "secret1");
    });
  }

  #[test]
  fn a_stale_answer_after_logout_is_dropped() {
    with_session(fixture(None), |session| {
      session.set_username("林晓");
      session.set_password("secret1");
      let revision = session.begin_auth().unwrap();
      session.signed_out();
      session.auth_failed(revision, &ApiFailure::Network("late".into()));
      assert_eq!(session.draft().error, None);
      assert!(!session.submitting());
    });
  }

  #[test]
  fn an_expired_session_is_reported_once_signed_in() {
    with_session(fixture(None), |session| {
      let expired = ApiFailure::of(ErrorCode::SessionExpired);
      session.report(&expired);
      assert!(!session.expired(), "a guest has no session to lose");
      let auth = now_or_never(session.client().login(&LoginRequest {
        username: "林晓".into(),
        password: "secret1".into(),
        remember: false,
      }))
      .unwrap();
      session.signed_in(auth);
      session.report(&ApiFailure::of(ErrorCode::NotFound));
      assert!(!session.expired());
      session.report(&expired);
      assert!(session.expired());
      session.signed_out();
      assert!(!session.expired());
    });
  }

  #[test]
  fn the_form_focus_moves_and_is_left_once() {
    with_session(fixture(None), |session| {
      assert_eq!(session.form_focus(), None);
      session.set_form_focus(Some(FormControl::Username));
      assert!(FormControl::Username.is_field() && !FormControl::Guest.is_field());
      // Tab: the field's blur, then the next control's focus.
      session.form_focus_left(FormControl::Username);
      session.set_form_focus(Some(FormControl::Password));
      // A late blur of a control that no longer has the focus changes nothing.
      session.form_focus_left(FormControl::Username);
      assert_eq!(session.form_focus(), Some(FormControl::Password));
      session.form_focus_left(FormControl::Password);
      assert_eq!(session.form_focus_untracked(), None);
    });
  }

  #[test]
  fn the_return_route_is_taken_once() {
    with_session(fixture(None), |session| {
      session.set_return_to(Some(Route::Cart));
      assert_eq!(session.return_to(), Some(Route::Cart));
      assert_eq!(session.take_return_to(), Some(Route::Cart));
      assert_eq!(session.take_return_to(), None);
    });
  }
}
