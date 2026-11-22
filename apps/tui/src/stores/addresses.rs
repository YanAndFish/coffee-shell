//! The address book (05-address.md): the list, the grid cursor, the change operations and
//! the new / edit dialog's draft with the server's field errors.
//!
//! Validation is the server's (P-addr-2): saving sends the draft as typed, and an error
//! code becomes the dialog's error line on its field ([`field_of`]). Every change answers
//! the whole book, which replaces the one shown. The cursor is an address id, or the
//! `+ 新增收货地址` card ([`AddressCursor::New`]).

use coffee_api::{Address, AddressInput, AddressList, AddressSaved, AddressTag, ErrorCode};
use nsh_core::prelude::*;

use super::{
  remote::{Phase, Remote, RemoteState},
  session::SessionStore,
};
use crate::api::ApiFailure;

/// Where the grid's cursor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressCursor {
  /// On address `id`.
  Card(u32),
  /// On the `+ 新增收货地址` card (the last cell).
  New,
}

/// A field of the address dialog, in focus order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressField {
  /// `收货人`.
  Recipient,
  /// `手机号`.
  Phone,
  /// `所在地区`.
  Region,
  /// `详细地址`.
  Detail,
  /// `标签`.
  Tag,
  /// `设为默认`.
  Default,
}

/// The field an error code is about (`None`: the whole form, `保存失败，请重试`).
pub fn field_of(failure: &ApiFailure) -> Option<AddressField> {
  match failure.code()? {
    ErrorCode::RecipientRequired => Some(AddressField::Recipient),
    ErrorCode::RegionRequired => Some(AddressField::Region),
    ErrorCode::DetailRequired => Some(AddressField::Detail),
    ErrorCode::PhoneInvalid => Some(AddressField::Phone),
    ErrorCode::TextTooLong | ErrorCode::ValidationFailed => {
      match failure.details()?.field.as_deref()? {
        "body.recipient" => Some(AddressField::Recipient),
        "body.region" => Some(AddressField::Region),
        "body.detail" => Some(AddressField::Detail),
        "body.phone" => Some(AddressField::Phone),
        "body.tag" => Some(AddressField::Tag),
        _ => None,
      }
    }
    _ => None,
  }
}

/// The dialog's error line (copy.md: the server's message, or `保存失败，请重试`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
  /// The field it is about, when it is about one.
  pub field: Option<AddressField>,
  /// The line, without its `✗ `.
  pub message: String,
}

/// The copy of a save that failed without a field (copy.md).
pub const SAVE_FAILED: &str = "保存失败，请重试";

/// The new / edit address dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressDraft {
  /// The address edited (`编辑地址 EDIT`), or `None` for a new one (`新增地址 NEW`).
  pub editing: Option<u32>,
  /// The fields as typed (the phone in full, K10).
  pub input: AddressInput,
  /// The error line.
  pub error: Option<FieldError>,
  /// Whether the save is under way (`⠹ 保存中…`).
  pub saving: bool,
}

/// The cursor `delta` cells from `current` in a grid of `columns` over `items` and the new
/// card, clamped to the ends.
pub fn step_cursor(items: &[Address], current: AddressCursor, delta: isize) -> AddressCursor {
  let at = match current {
    AddressCursor::Card(id) => items.iter().position(|a| a.id == id).unwrap_or(0),
    AddressCursor::New => items.len(),
  };
  let index = at.saturating_add_signed(delta).min(items.len());
  items
    .get(index)
    .map_or(AddressCursor::New, |a| AddressCursor::Card(a.id))
}

fn no_items(book: &AddressList) -> bool {
  book.items.is_empty()
}

/// The address book's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct AddressStore {
  session: SessionStore,
  book: Remote<AddressList>,
  cursor: Signal<Option<AddressCursor>>,
  dialog: Signal<Option<AddressDraft>>,
}

impl AddressStore {
  /// No book yet, reaching the backend through `session`.
  pub fn new(session: SessionStore) -> Self {
    Self {
      session,
      book: Remote::new(no_items),
      cursor: signal(None),
      dialog: signal(None),
    }
  }

  // --- Reading ------------------------------------------------------------------------

  /// The book's state (tracked).
  pub fn state(&self) -> RemoteState<AddressList> {
    self.book.state()
  }

  /// The book's phase (tracked).
  pub fn phase(&self) -> Phase {
    self.book.phase()
  }

  /// The failure of the last load (tracked).
  pub fn failure(&self) -> Option<ApiFailure> {
    self.book.failure()
  }

  /// The addresses, oldest first (tracked).
  pub fn items(&self) -> Vec<Address> {
    self
      .book
      .with(|book| book.map(|b| b.items.clone()).unwrap_or_default())
  }

  /// How many (tracked): `地址本 · {n} 条`.
  pub fn count(&self) -> usize {
    self.book.with(|book| book.map_or(0, |b| b.items.len()))
  }

  /// Address `id` (tracked).
  pub fn address(&self, id: u32) -> Option<Address> {
    self
      .book
      .with(|book| book.and_then(|b| b.items.iter().find(|a| a.id == id).cloned()))
  }

  /// 本单寄往 (tracked): `当前寄往：{姓名} · {地区}`, `None` = `未指定`.
  pub fn order_address(&self) -> Option<Address> {
    let id = self
      .book
      .with(|book| book.and_then(|b| b.order_address_id))?;
    self.address(id)
  }

  /// The default address (tracked).
  pub fn default_address(&self) -> Option<Address> {
    let id = self
      .book
      .with(|book| book.and_then(|b| b.default_address_id))?;
    self.address(id)
  }

  /// The cursor: the chosen card if it still exists, else the first card, else the new
  /// card (tracked).
  pub fn cursor(&self) -> AddressCursor {
    let chosen = self.cursor.get();
    self.book.with(|book| {
      let items = book.map(|b| b.items.as_slice()).unwrap_or_default();
      match chosen {
        Some(AddressCursor::Card(id)) if items.iter().any(|a| a.id == id) => {
          AddressCursor::Card(id)
        }
        Some(AddressCursor::New) => AddressCursor::New,
        _ => items
          .first()
          .map_or(AddressCursor::New, |a| AddressCursor::Card(a.id)),
      }
    })
  }

  /// The address under the cursor (tracked).
  pub fn cursor_address(&self) -> Option<Address> {
    match self.cursor() {
      AddressCursor::Card(id) => self.address(id),
      AddressCursor::New => None,
    }
  }

  /// Moves the cursor `delta` cells (`← →` ±1, `↑ ↓` ± the grid's columns).
  pub fn move_cursor(&self, delta: isize) {
    let current = untracked(|| self.cursor());
    let next = self.book.with_untracked(|book| {
      step_cursor(
        book.map(|b| b.items.as_slice()).unwrap_or_default(),
        current,
        delta,
      )
    });
    self.cursor.set(Some(next));
  }

  /// Puts the cursor on `cursor`.
  pub fn select(&self, cursor: AddressCursor) {
    self.cursor.set(Some(cursor));
  }

  // --- Loading ------------------------------------------------------------------------

  /// Loads the book (members). Await it in tests.
  pub async fn fetch(self) {
    let client = self.session.client();
    let revision = self.book.begin();
    let result = client.addresses().await;
    if let Err(failure) = &result {
      self.session.report(failure);
    }
    self.book.finish(revision, result);
  }

  /// Loads the book in a task of the root's owner.
  pub fn load(&self) {
    let store = *self;
    self.session.spawn(store.fetch());
  }

  /// Loads the book if nothing was asked yet.
  pub fn ensure_loaded(&self) {
    if self.book.is_idle() {
      self.load();
    }
  }

  /// Forgets the book (logout).
  pub fn reset(&self) {
    batch(|| {
      self.book.reset();
      self.cursor.set(None);
      self.dialog.set(None);
    });
  }

  // --- Changing it --------------------------------------------------------------------

  async fn change(
    self,
    request: impl Future<Output = Result<AddressList, ApiFailure>>,
  ) -> Result<AddressList, ApiFailure> {
    let revision = self.book.begin_quiet();
    let result = request.await;
    if let Ok(book) = &result {
      self.book.finish_quiet(revision, Ok(book.clone()));
    }
    result
  }

  /// `space`: makes `id` the default.
  pub async fn set_default(self, id: u32) -> Result<AddressList, ApiFailure> {
    let client = self.session.client();
    self.change(client.set_default_address(id)).await
  }

  /// `d` / `x`: deletes `id` (no confirmation, P-addr-4); the cursor moves to the card that
  /// takes its place.
  pub async fn delete(self, id: u32) -> Result<AddressList, ApiFailure> {
    let index = self
      .book
      .with_untracked(|book| book.and_then(|b| b.items.iter().position(|a| a.id == id)));
    let client = self.session.client();
    let result = self.change(client.delete_address(id)).await;
    if let (Ok(book), Some(index)) = (&result, index) {
      let next = book
        .items
        .get(index)
        .or_else(|| book.items.last())
        .map_or(AddressCursor::New, |a| AddressCursor::Card(a.id));
      self.cursor.set(Some(next));
    }
    result
  }

  // --- The dialog ---------------------------------------------------------------------

  /// The dialog's draft, while it is open (tracked).
  pub fn dialog(&self) -> Option<AddressDraft> {
    self.dialog.get()
  }

  /// Whether a save is under way (tracked).
  pub fn saving(&self) -> bool {
    self.dialog.with(|d| d.as_ref().is_some_and(|d| d.saving))
  }

  /// `n`: a new address (`标签` 家; `设为默认` checked when the book is empty).
  pub fn open_new(&self) {
    let first = self.book.with_untracked(|book| book.is_none_or(no_items));
    self.dialog.set(Some(AddressDraft {
      editing: None,
      input: AddressInput {
        tag: AddressTag::Home,
        is_default: first,
        ..AddressInput::default()
      },
      error: None,
      saving: false,
    }));
  }

  /// `e`: edits address `id` (its full phone number, K10).
  pub fn open_edit(&self, id: u32) {
    let Some(address) = untracked(|| self.address(id)) else {
      return;
    };
    self.dialog.set(Some(AddressDraft {
      editing: Some(id),
      input: AddressInput {
        recipient: address.recipient,
        region: address.region,
        detail: address.detail,
        phone: address.phone,
        tag: address.tag,
        is_default: address.is_default,
      },
      error: None,
      saving: false,
    }));
  }

  /// Closes the dialog (Esc, or after a save).
  pub fn close_dialog(&self) {
    self.dialog.set(None);
  }

  /// Edits the draft's fields; any change clears the error line.
  pub fn edit(&self, f: impl FnOnce(&mut AddressInput)) {
    self.dialog.update(|dialog| {
      if let Some(draft) = dialog
        && !draft.saving
      {
        f(&mut draft.input);
        draft.error = None;
      }
    });
  }

  /// Saves the draft: the server validates; on success the dialog closes, the book is the
  /// server's and the cursor goes to the saved card. The saved address and whether it was
  /// new.
  pub async fn save(self) -> Result<(Address, bool), ApiFailure> {
    let Some(draft) = self.dialog.get_untracked() else {
      return Err(ApiFailure::of(ErrorCode::NotFound));
    };
    if draft.saving {
      return Err(ApiFailure::of(ErrorCode::BadRequest));
    }
    self.dialog.update(|d| {
      if let Some(d) = d {
        d.saving = true;
        d.error = None;
      }
    });
    let mut input = draft.input.clone();
    input.phone = input.phone.filter(|p| !p.trim().is_empty());
    let client = self.session.client();
    let revision = self.book.begin_quiet();
    let result: Result<AddressSaved, ApiFailure> = match draft.editing {
      Some(id) => client.update_address(id, &input).await,
      None => client.create_address(&input).await,
    };
    match result {
      Ok(saved) => {
        batch(|| {
          self.book.finish_quiet(revision, Ok(saved.book));
          self.cursor.set(Some(AddressCursor::Card(saved.address.id)));
          self.dialog.set(None);
        });
        Ok((saved.address, draft.editing.is_none()))
      }
      Err(failure) => {
        let error = FieldError {
          field: field_of(&failure),
          message: if failure.is_network() || failure.code().is_none() {
            SAVE_FAILED.to_owned()
          } else {
            failure.message()
          },
        };
        self.dialog.update(|d| {
          if let Some(d) = d {
            d.saving = false;
            d.error = Some(error);
          }
        });
        Err(failure)
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use coffee_api::{AuthSession, LoginRequest};
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport, now_or_never};

  fn with_book(set: FixtureSet, test: impl FnOnce(AddressStore)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, store) = create_root(|| {
      let session = SessionStore::with_client(ApiClient::new(Transport::Fixture(set), None));
      let request = LoginRequest {
        username: "林晓".into(),
        password: "secret1".into(),
        remember: false,
      };
      let auth: AuthSession = now_or_never(session.client().login(&request)).unwrap();
      session.signed_in(auth);
      AddressStore::new(session)
    });
    test(store);
    owner.dispose();
  }

  #[test]
  fn the_book_loads_with_its_cursor_and_order_address() {
    with_book(FixtureSet::Contract, |book| {
      now_or_never(book.fetch());
      assert_eq!(book.phase(), Phase::Ready);
      assert_eq!(book.count(), 3);
      assert_eq!(book.cursor(), AddressCursor::Card(1));
      assert_eq!(book.order_address().map(|a| a.id), Some(1));
      book.move_cursor(2);
      assert_eq!(book.cursor(), AddressCursor::Card(3));
      book.move_cursor(1);
      assert_eq!(book.cursor(), AddressCursor::New);
      book.move_cursor(5);
      assert_eq!(book.cursor(), AddressCursor::New);
      book.move_cursor(-9);
      assert_eq!(book.cursor(), AddressCursor::Card(1));
    });
    with_book(FixtureSet::Empty, |book| {
      now_or_never(book.fetch());
      assert_eq!(book.phase(), Phase::Empty);
      assert_eq!(book.cursor(), AddressCursor::New);
      assert_eq!(book.order_address(), None);
      book.open_new();
      assert!(
        book.dialog().unwrap().input.is_default,
        "the first is the default"
      );
    });
  }

  #[test]
  fn changes_replace_the_book() {
    with_book(FixtureSet::Contract, |book| {
      now_or_never(book.fetch());
      let moved = now_or_never(book.set_default(3)).unwrap();
      assert_eq!(moved.default_address_id, Some(3));
      assert_eq!(book.default_address().map(|a| a.id), Some(3));
      now_or_never(book.delete(1)).unwrap();
      assert_eq!(book.count(), 2);
      assert_eq!(book.cursor(), AddressCursor::Card(2));
    });
  }

  #[test]
  fn the_dialog_shows_the_servers_field_errors_then_saves() {
    with_book(FixtureSet::Contract, |book| {
      now_or_never(book.fetch());
      book.open_new();
      let failure = now_or_never(book.save()).unwrap_err();
      assert!(failure.is(ErrorCode::RecipientRequired));
      let draft = book.dialog().unwrap();
      assert!(!draft.saving);
      assert_eq!(
        draft.error,
        Some(FieldError {
          field: Some(AddressField::Recipient),
          message: "请填写收货人".into(),
        })
      );
      book.edit(|input| {
        input.recipient = "周一".into();
        input.region = "上海市".into();
        input.detail = "某路 1 号".into();
        input.phone = Some(String::new());
      });
      assert_eq!(
        book.dialog().unwrap().error,
        None,
        "an edit clears the error"
      );
      let (saved, new) = now_or_never(book.save()).unwrap();
      assert!(new);
      assert_eq!(saved.id, 4);
      assert_eq!(book.dialog(), None);
      assert_eq!(book.cursor(), AddressCursor::Card(4));
      assert_eq!(book.count(), 4);

      book.open_edit(2);
      let draft = book.dialog().unwrap();
      assert_eq!(draft.editing, Some(2));
      assert_eq!(
        draft.input.phone.as_deref().map(str::len),
        Some(11),
        "the full phone"
      );
      let (_, new) = now_or_never(book.save()).unwrap();
      assert!(!new);
    });
  }

  #[test]
  fn error_fields_follow_the_code_or_the_details() {
    assert_eq!(
      field_of(&ApiFailure::of(ErrorCode::PhoneInvalid)),
      Some(AddressField::Phone)
    );
    let long = crate::api::decode::<AddressSaved>(
      422,
      r#"{"error":{"code":"text_too_long","message":"内容太长了","details":{"field":"body.detail"}}}"#.as_bytes(),
    )
    .unwrap_err();
    assert_eq!(field_of(&long), Some(AddressField::Detail));
    assert_eq!(field_of(&ApiFailure::of(ErrorCode::AddressLimit)), None);
    assert_eq!(field_of(&ApiFailure::Network("x".into())), None);
  }
}
