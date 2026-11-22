//! 地址本 ADDRESS (05-address.md): the address cards, the order address, the default, and
//! the new / edit dialog (`编辑 INSERT`). Guests see a card asking them to log in.
//!
//! Store: [`crate::stores::AddressStore`] — `phase`, `failure`, `items`, `count`,
//! `address`, `order_address`, `default_address`, `cursor` / `cursor_address` /
//! `move_cursor` / `select`, `ensure_loaded` / `load`, and the dialog's `dialog` / `edit` /
//! `saving`. Actions: `actions::use_address`, `set_default_address`, `delete_address`,
//! `new_address`, `edit_address`, `save_address`, `cancel_address`, `go_login`. The
//! dialog's fields report their focus as `address.<field>` ([`FIELDS`]).
//!
//! The body and the dialog are placeholders (plain lines, bare fields); the address-book
//! screen replaces them.

pub mod commands;

use coffee_api::AddressInput;
use nsh_core::{element::Screen, prelude::*};

use super::{
  Dialog, PageModule,
  placeholder::{self, Line, phase_lines},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{AddressCursor, AddressStore, Overlay, Stores},
  ui::{self, Tone},
};

/// `地址本 ADDRESS`.
pub const MODE: Mode = Mode::new("地址本", "ADDRESS");

/// The dialog's text fields: (focus name, label).
pub const FIELDS: [(&str, &str); 4] = [
  ("address.recipient", "收货人"),
  ("address.phone", "手机号"),
  ("address.region", "所在地区"),
  ("address.detail", "详细地址"),
];

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Addresses,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[Dialog {
    overlay: Overlay::AddressDialog,
    view: dialog,
  }],
};

fn guest_lines() -> Vec<Line> {
  vec![
    Line::text("你正在以访客身份浏览"),
    Line::soft("登录后可以管理地址本，并为订单选择收货地址。"),
    Line::text("⏎ 去登录").press("addresses.login"),
  ]
}

fn lines(stores: Stores) -> Vec<Line> {
  if !stores.session.is_member() {
    return guest_lines();
  }
  let book = stores.addresses;
  let current = book
    .order_address()
    .map(|a| format!("{} · {}", a.recipient, a.region))
    .unwrap_or_else(|| "未指定".to_owned());
  let mut lines = vec![Line::soft(format!(
    "设置默认收货地址，或为本单单独指定一个。当前寄往：{current}"
  ))];
  lines.extend(phase_lines(
    book.phase(),
    book.failure(),
    "地址",
    "还没有地址",
    Some("addresses.retry"),
  ));
  let cursor = book.cursor();
  let order = book.order_address().map(|a| a.id);
  for address in book.items() {
    let mark = if cursor == AddressCursor::Card(address.id) {
      "▶"
    } else {
      " "
    };
    let mut tags = Vec::new();
    if Some(address.id) == order {
      tags.push("[本单寄往]".to_owned());
    }
    if address.is_default {
      tags.push("[默认]".to_owned());
    }
    tags.push(format!("[{}]", address.tag.label()));
    lines.push(Line::text(format!(
      "{mark} {} {} · {} {} {}",
      address.recipient,
      address.phone_masked.as_deref().unwrap_or("—"),
      address.region,
      address.detail,
      tags.join(" ")
    )));
  }
  let mark = if cursor == AddressCursor::New {
    "▶"
  } else {
    " "
  };
  lines.push(Line::text(format!("{mark} + 新增收货地址")).press("addresses.new"));
  lines
}

/// The address book (placeholder body).
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let book = stores.addresses;
  let session = stores.session;
  // The page's work: a member's book, once.
  effect(move || {
    if session.is_member() {
      untracked(|| book.ensure_loaded());
    }
  });
  placeholder::screen(
    "地址本 ADDRESS",
    move || {
      if session.is_member() {
        format!("地址本 · {} 条", book.count())
      } else {
        "地址本 ADDRESS".to_owned()
      }
    },
    move || lines(stores),
  )
}

fn input_field(book: AddressStore, index: usize) -> impl IntoElement {
  let (field, name) = FIELDS[index];
  let get = move |input: &AddressInput| -> String {
    match index {
      0 => input.recipient.clone(),
      1 => input.phone.clone().unwrap_or_default(),
      2 => input.region.clone(),
      _ => input.detail.clone(),
    }
  };
  placeholder::field(
    field,
    name,
    move || book.dialog().map(|d| get(&d.input)).unwrap_or_default(),
    move |text| {
      let text = text.to_owned();
      book.edit(|input| match index {
        0 => input.recipient = text,
        1 => input.phone = Some(text),
        2 => input.region = text,
        _ => input.detail = text,
      })
    },
    false,
  )
}

/// The new / edit dialog (placeholder body).
pub fn dialog() -> AnyElement {
  let stores = Stores::get();
  let book = stores.addresses;
  ui::panel(
    move || {
      SharedString::from(match book.dialog().and_then(|d| d.editing) {
        Some(_) => "编辑地址 EDIT",
        None => "新增地址 NEW",
      })
    },
    view()
      .flex_1()
      .children((0..FIELDS.len()).map(|index| input_field(book, index)))
      .child(placeholder::lines(move || {
        let Some(draft) = book.dialog() else {
          return Vec::new();
        };
        let mut lines = vec![Line::text(format!(
          "标签 {} · {} 设为默认",
          draft.input.tag.label(),
          if draft.input.is_default {
            "[✓]"
          } else {
            "[ ]"
          }
        ))];
        if let Some(error) = draft.error {
          lines.push(Line::error(format!("✗ {}", error.message)));
        }
        lines.push(if draft.saving {
          Line::new("⠋ 保存中…", Tone::Acc)
        } else {
          Line::text("Esc 取消 · ⏎ 保存")
        });
        lines
      })),
  )
  .into_element()
}
