//! The pages, one module (directory) per screen: `splash`, `login`, `catalog`, `detail`,
//! `cart`, `addresses`, `profile`. Each is `pages/<name>/{mod.rs,commands.rs}` and registers
//! itself here through its [`PageModule`] (`MODULE`): its page kind, its status-bar mode,
//! its commands (keys, footer hints, help), its view, and the dialogs it draws.
//!
//! A screen is built by editing its own directory only: the view in `mod.rs` (more files
//! beside it as it grows), the command list in `commands.rs`. Pages read the stores and
//! change them through their functions or `stores::actions`; they compose `ui` components
//! and never pick colours. Shared helpers: [`format`] (money, dates, quantities as copy.md
//! writes them) and [`placeholder`] (the plain bodies the pages ship with until their
//! screen is built).

pub mod addresses;
pub mod cart;
pub mod catalog;
pub mod detail;
pub mod format;
pub mod login;
pub mod placeholder;
pub mod profile;
pub mod splash;

use nsh_core::{element::Screen, prelude::*};

use crate::{
  commands::{Command, Mode},
  router::{Page, Route},
  stores::{Overlay, Stores},
};

/// A dialog a page draws over everything while its overlay is open (the shell's dialog
/// outlet asks the page that owns the overlay).
#[derive(Clone, Copy)]
pub struct Dialog {
  /// The overlay it is.
  pub overlay: Overlay,
  /// Its view; it runs once per opening.
  pub view: fn() -> AnyElement,
}

/// One page's registration.
#[derive(Clone, Copy)]
pub struct PageModule {
  /// Its kind of page (what its command scopes name).
  pub page: Page,
  /// Its status-bar mode now (tracked): `选购 BROWSE`, `注册 SIGN UP` …
  pub mode: fn(&Stores) -> Mode,
  /// Its commands: keys, footer hints, help entries.
  pub commands: &'static [Command],
  /// Its view for a route of its kind.
  pub view: fn(&Route) -> Screen,
  /// The dialogs it draws.
  pub dialogs: &'static [Dialog],
}

/// Every page module, in [`Page::ALL`] order.
pub const PAGES: [PageModule; 7] = [
  splash::MODULE,
  login::MODULE,
  catalog::MODULE,
  detail::MODULE,
  cart::MODULE,
  addresses::MODULE,
  profile::MODULE,
];

/// The module of `page`.
pub fn module(page: Page) -> &'static PageModule {
  PAGES
    .iter()
    .find(|module| module.page == page)
    .expect("every page has a module")
}

/// The page of `route`, for the shell's stack.
pub fn page(route: &Route) -> Screen {
  (module(route.page()).view)(route)
}

/// The dialog drawn for `overlay`, if a page draws it.
pub fn dialog(overlay: Overlay) -> Option<AnyElement> {
  PAGES
    .iter()
    .flat_map(|module| module.dialogs.iter())
    .find(|dialog| dialog.overlay == overlay)
    .map(|dialog| (dialog.view)())
}
