//! Accounts and sessions: `/v1/auth/*`.

use serde::{Deserialize, Serialize};

/// Body of `POST /v1/auth/register`.
///
/// Username: 2–16 characters, each a Chinese character, an ASCII letter, a digit or `_`.
/// Password: 6–128 characters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
  /// The new username.
  pub username: String,
  /// The password (never logged).
  pub password: String,
  /// 记住我: bind this connection's key (sent as `X-SSH-Fingerprint`) for 30 days.
  pub remember: bool,
}

/// Body of `POST /v1/auth/login`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
  /// Matched ignoring the case of ASCII letters.
  pub username: String,
  /// The password (never logged).
  pub password: String,
  /// 记住我 (see [`RegisterRequest::remember`]).
  pub remember: bool,
}

/// Member tier: static display values for now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
  /// `熟豆会员`.
  pub name: String,
  /// `3` → `Lv.3`.
  pub level: u32,
  /// `1,280 / 2,000 豆`.
  pub points: u32,
  /// `4`.
  pub next_level: u32,
  /// `2000`.
  pub next_level_points: u32,
  /// `再积 720 豆升级 Lv.4`.
  pub points_to_next: u32,
  /// The meter's percentage, 0–100.
  pub progress: u32,
}

/// A member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
  /// Database id.
  pub id: u32,
  /// As registered; the avatar is its first character (Latin letters upper-cased).
  pub username: String,
  /// UTC instant.
  pub created_at: String,
  /// Member tier.
  pub tier: Tier,
}

/// 记住我 state of the current connection for the member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RememberState {
  /// `● 已记住当前公钥 · 至 {MM-DD}`.
  Remembered,
  /// `○ 未记住当前公钥`.
  NotRemembered,
  /// `○ 本次连接没有公钥`.
  NoKey,
}

/// 记住我 for the current connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberStatus {
  /// Which of the three.
  pub state: RememberState,
  /// UTC instant, only when remembered.
  pub expires_at: Option<String>,
  /// Shop-local date (`2026-12-22`), only when remembered.
  pub expires_on: Option<String>,
}

/// `GET /v1/auth/me`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
  /// The member.
  pub user: User,
  /// 记住我 for this connection.
  pub remembered: RememberStatus,
}

/// A new session (register, login, resume).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
  /// Send as `Authorization: Bearer <token>`; keep it in memory only.
  pub token: String,
  /// UTC instant; afterwards `401 session_expired`.
  pub expires_at: String,
  /// The member.
  pub user: User,
  /// 记住我 for this connection.
  pub remembered: RememberStatus,
}
