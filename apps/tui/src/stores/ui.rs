//! The shell's own state: which overlay is open (help, the logout confirmation, the address
//! dialog, the filter dialog), which text field has the keyboard focus, and focus requests.
//! The command table reads it to decide what a key does (keymap.md, "处理顺序").
//!
//! Text fields report their focus here ([`UiStore::field_focused`] / [`UiStore::field_blurred`]
//! from the field's `on_focus` / `on_blur`); while one has it, single-letter commands do not
//! fire and the footer shows the field's hints. [`UiStore::leave_field`] (Esc) blurs it.
//! [`UiStore::request_focus`] asks the field named so to take the focus (`/` → the search
//! box); the field's owner watches [`UiStore::focus_requested`].

use nsh_core::prelude::*;

/// A layer over the pages that takes the keys while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Overlay {
  /// The `?` help dialog (`帮助 HELP`).
  Help,
  /// The logout confirmation (`确认 CONFIRM`, U4 / P-ui-1).
  Logout,
  /// The new / edit address dialog (`编辑 INSERT`), drawn by the address book.
  AddressDialog,
  /// The S-width filter dialog (`筛选 FILTER`), drawn by the catalog.
  FilterDialog,
}

/// The button the logout confirmation's focus is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LogoutChoice {
  /// `⏎ 退出登录` (where it opens).
  #[default]
  Confirm,
  /// `Esc 取消`.
  Cancel,
}

/// A text field with the keyboard focus.
#[derive(Clone, Copy)]
struct Focused {
  field: &'static str,
  element: Option<ElementRef>,
}

impl PartialEq for Focused {
  fn eq(&self, other: &Self) -> bool {
    self.field == other.field
  }
}

/// The shell's state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct UiStore {
  overlay: Signal<Option<Overlay>>,
  focused: Signal<Option<Focused>>,
  focus_request: Signal<Option<&'static str>>,
  logout_choice: Signal<LogoutChoice>,
}

impl UiStore {
  /// No overlay, no field focused.
  pub fn new() -> Self {
    Self {
      overlay: signal(None),
      focused: signal(None),
      focus_request: signal(None),
      logout_choice: signal(LogoutChoice::default()),
    }
  }

  /// The overlay open (tracked).
  pub fn overlay(&self) -> Option<Overlay> {
    self.overlay.get()
  }

  /// The overlay open, untracked: for key handlers.
  pub fn overlay_untracked(&self) -> Option<Overlay> {
    self.overlay.get_untracked()
  }

  /// Whether `overlay` is open (tracked).
  pub fn is_open(&self, overlay: Overlay) -> bool {
    self.overlay() == Some(overlay)
  }

  /// Opens `overlay` (in place of any other). A field focused under it loses the keys.
  pub fn open(&self, overlay: Overlay) {
    batch(|| {
      if overlay == Overlay::Logout {
        self.logout_choice.set(LogoutChoice::Confirm);
      }
      self.overlay.set(Some(overlay));
    });
  }

  /// Closes the overlay open, if any.
  pub fn close(&self) {
    self.overlay.set(None);
  }

  /// Closes `overlay` if it is the one open.
  pub fn close_if(&self, overlay: Overlay) {
    if self.overlay.get_untracked() == Some(overlay) {
      self.overlay.set(None);
    }
  }

  /// `?`: opens the help, or closes it when open.
  pub fn toggle_help(&self) {
    if self.overlay.get_untracked() == Some(Overlay::Help) {
      self.close();
    } else {
      self.open(Overlay::Help);
    }
  }

  /// The logout confirmation's focused button (tracked).
  pub fn logout_choice(&self) -> LogoutChoice {
    self.logout_choice.get()
  }

  /// Moves the logout confirmation's focus to the other button.
  pub fn toggle_logout_choice(&self) {
    self.logout_choice.update(|choice| {
      *choice = match choice {
        LogoutChoice::Confirm => LogoutChoice::Cancel,
        LogoutChoice::Cancel => LogoutChoice::Confirm,
      }
    });
  }

  /// Puts the logout confirmation's focus on `choice`.
  pub fn set_logout_choice(&self, choice: LogoutChoice) {
    self.logout_choice.set(choice);
  }

  /// The text field with the keyboard focus (tracked): its name, as its page gave it.
  pub fn focused_field(&self) -> Option<&'static str> {
    self.focused.with(|focused| focused.map(|f| f.field))
  }

  /// Whether a text field has the keyboard focus (tracked).
  pub fn typing(&self) -> bool {
    self.focused.with(Option::is_some)
  }

  /// A field's `on_focus`: `field` (with its element, so Esc can blur it) has the focus.
  pub fn field_focused(&self, field: &'static str, element: Option<ElementRef>) {
    self.focused.set(Some(Focused { field, element }));
  }

  /// A field's `on_blur`: `field` lost the focus (a later field's focus stays).
  pub fn field_blurred(&self, field: &'static str) {
    if self
      .focused
      .with_untracked(|focused| focused.is_some_and(|f| f.field == field))
    {
      self.focused.set(None);
    }
  }

  /// Esc in a field: blurs it (the keys go back to the page).
  pub fn leave_field(&self) {
    if let Some(focused) = self.focused.get_untracked() {
      if let Some(element) = focused.element {
        element.blur();
      }
      self.focused.set(None);
    }
  }

  /// Asks field `field` to take the focus.
  pub fn request_focus(&self, field: &'static str) {
    self.focus_request.set(Some(field));
  }

  /// Whether field `field` was asked to take the focus (tracked): its owner focuses it and
  /// calls [`Self::focus_taken`].
  pub fn focus_requested(&self, field: &'static str) -> bool {
    self.focus_request.get() == Some(field)
  }

  /// The request for `field` is done.
  pub fn focus_taken(&self, field: &'static str) {
    if self.focus_request.get_untracked() == Some(field) {
      self.focus_request.set(None);
    }
  }

  /// Back to no overlay and no field (logout, session expiry).
  pub fn reset(&self) {
    batch(|| {
      self.overlay.set(None);
      self.focused.set(None);
      self.focus_request.set(None);
    });
  }
}

impl Default for UiStore {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;

  #[test]
  fn overlays_and_fields() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, ui) = create_root(UiStore::new);
    assert_eq!(ui.overlay(), None);
    ui.toggle_help();
    assert!(ui.is_open(Overlay::Help));
    ui.toggle_help();
    assert_eq!(ui.overlay(), None);
    ui.open(Overlay::Logout);
    assert_eq!(ui.logout_choice(), LogoutChoice::Confirm);
    ui.toggle_logout_choice();
    assert_eq!(ui.logout_choice(), LogoutChoice::Cancel);
    ui.close_if(Overlay::Help);
    assert!(ui.is_open(Overlay::Logout));
    ui.open(Overlay::Logout);
    assert_eq!(
      ui.logout_choice(),
      LogoutChoice::Confirm,
      "it opens on 退出登录"
    );
    ui.close_if(Overlay::Logout);
    assert_eq!(ui.overlay(), None);

    assert!(!ui.typing());
    ui.field_focused("login.username", None);
    assert_eq!(ui.focused_field(), Some("login.username"));
    ui.field_focused("login.password", None);
    // The blur of the field the focus left comes after the focus of the next one.
    ui.field_blurred("login.username");
    assert_eq!(ui.focused_field(), Some("login.password"));
    ui.leave_field();
    assert!(!ui.typing());

    ui.request_focus("catalog.search");
    assert!(ui.focus_requested("catalog.search"));
    assert!(!ui.focus_requested("other"));
    ui.focus_taken("catalog.search");
    assert!(!ui.focus_requested("catalog.search"));
    owner.dispose();
  }
}
