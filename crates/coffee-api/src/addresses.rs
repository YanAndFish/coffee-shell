//! The address book: `/v1/addresses`.

use serde::{Deserialize, Serialize};

/// Address label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressTag {
  /// 家.
  #[default]
  Home,
  /// 公司.
  Work,
  /// 学校.
  School,
}

impl AddressTag {
  /// All tags in display order.
  pub const ALL: [AddressTag; 3] = [AddressTag::Home, AddressTag::Work, AddressTag::School];

  /// The display label.
  pub fn label(self) -> &'static str {
    match self {
      AddressTag::Home => "家",
      AddressTag::Work => "公司",
      AddressTag::School => "学校",
    }
  }
}

/// A saved address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
  /// Database id.
  pub id: u32,
  /// 收货人.
  pub recipient: String,
  /// The full 11 digits (the edit dialog), or `None`.
  pub phone: Option<String>,
  /// `138 **** 5521` (cards, checkout), or `None` (show `—`).
  pub phone_masked: Option<String>,
  /// 所在地区.
  pub region: String,
  /// 详细地址.
  pub detail: String,
  /// Label.
  pub tag: AddressTag,
  /// The default address.
  pub is_default: bool,
}

/// `GET /v1/addresses` and the result of every change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressList {
  /// Oldest first.
  pub items: Vec<Address>,
  /// The default, `None` when empty.
  pub default_address_id: Option<u32>,
  /// 本单寄往 (`None` = `未指定`).
  pub order_address_id: Option<u32>,
}

/// Body of `POST /v1/addresses` and `PUT /v1/addresses/{id}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressInput {
  /// 收货人 (1–20 characters, trimmed).
  pub recipient: String,
  /// 所在地区 (1–40).
  pub region: String,
  /// 详细地址 (1–80).
  pub detail: String,
  /// 11 digits, or `None` / empty for none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub phone: Option<String>,
  /// Label.
  pub tag: AddressTag,
  /// 设为默认.
  pub is_default: bool,
}

/// Result of creating or updating an address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSaved {
  /// The saved address.
  pub address: Address,
  /// The whole book afterwards.
  pub book: AddressList,
}
