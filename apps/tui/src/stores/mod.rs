//! The app's state: `Copy` structs of private signals with public functions, made once by
//! the root and provided to everything under it ([`Stores::provide`]). Pages read the
//! stores and change them through their functions or the [`actions`]; a page never writes
//! another page's state.
//!
//! | Store | Holds |
//! | --- | --- |
//! | [`SessionStore`] | guest / member, the token (memory only), the login form, return-to route |
//! | [`Router`] | the page stack |
//! | [`CatalogStore`] | the bean list, its query (filters, sort, search), facets, cursor |
//! | [`ProductStore`] | one product's detail and the SKU selection |
//! | [`CartStore`] | member cart or guest quote, undo, coupon, checkout, receipt |
//! | [`AddressStore`] | the address book, its cursor, the dialog draft |
//! | [`OrdersStore`] | recent orders |
//! | [`ProfileStore`] | the member's profile |
//! | [`SettingsStore`] | 界面动画 / 鼠标追踪 |
//! | [`ToastStore`] | the toast queue |
//! | [`UiStore`] | overlays (help, logout confirm, dialogs), the focused text field |
//!
//! Every remote value is a [`remote::Remote`] (loading / ready / empty / failed, stale
//! answers dropped). Async work runs under the root's owner ([`Stores::spawn`]), so a page
//! going away never cancels a request half way.

pub mod actions;
pub mod addresses;
pub mod cart;
pub mod catalog;
pub mod orders;
pub mod product;
pub mod profile;
pub mod remote;
pub mod session;
pub mod settings;
pub mod toast;
pub mod ui;

use nsh_core::prelude::*;

pub use addresses::{AddressCursor, AddressDraft, AddressField, AddressStore};
pub use cart::{CartStore, CheckoutCheck, CheckoutStep, CouponFeedback};
pub use catalog::{CatalogQuery, CatalogStore};
pub use orders::OrdersStore;
pub use product::{ProductStore, SkuGroup, SkuSelection};
pub use profile::ProfileStore;
pub use remote::{Phase, RemoteState};
pub use session::{AuthStep, LoginDraft, LoginMode, SessionStore};
pub use settings::{Setting, SettingsStore};
pub use toast::{Toast, ToastKind, ToastStore};
pub use ui::{LogoutChoice, Overlay, UiStore};

use crate::{
  router::{Page, Route, Router},
  ui::{Bp, Viewport},
};

/// The storage key that turns the splash off (`off`) when no host configured the session:
/// the screen tests open straight on the login page.
pub const SPLASH_OVERRIDE_KEY: &str = "coffee.splash";

/// Whether the connection opens on the splash.
fn splash_wanted() -> bool {
  crate::session_env().is_some()
    || nsh_core::storage::Storage::get_item(SPLASH_OVERRIDE_KEY).as_deref() != Some("off")
}

/// Every store of the app. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct Stores {
  /// Who is shopping, and the backend client.
  pub session: SessionStore,
  /// The pages, bottom to top.
  pub router: Router,
  /// The bean list.
  pub catalog: CatalogStore,
  /// The detail page's product and SKU selection.
  pub product: ProductStore,
  /// The cart.
  pub cart: CartStore,
  /// The address book.
  pub addresses: AddressStore,
  /// Recent orders.
  pub orders: OrdersStore,
  /// The member's profile.
  pub profile: ProfileStore,
  /// UI settings.
  pub settings: SettingsStore,
  /// The toasts.
  pub toasts: ToastStore,
  /// Overlays and the focused field.
  pub ui: UiStore,
  viewport: Memo<Viewport>,
}

impl Stores {
  /// Makes every store over `session`, with the terminal size from `viewport`, owned by
  /// the current owner. The app starts on the login page ([`Self::provide`] puts the
  /// splash first).
  pub fn new(session: SessionStore, viewport: impl Fn() -> Viewport + 'static) -> Self {
    Self {
      session,
      router: Router::new(Route::Login),
      catalog: CatalogStore::new(session),
      product: ProductStore::new(session),
      cart: CartStore::new(session),
      addresses: AddressStore::new(session),
      orders: OrdersStore::new(session),
      profile: ProfileStore::new(session),
      settings: SettingsStore::new(session),
      toasts: ToastStore::new(),
      ui: UiStore::new(),
      viewport: computed(viewport),
    }
  }

  /// Makes the stores for this process's session and provides them to what the current
  /// owner makes. Call it once, in the root. The connection opens on the splash, unless
  /// storage [`SPLASH_OVERRIDE_KEY`] says `off` (tests; read only when no host configured
  /// the session).
  pub fn provide() -> Self {
    let stores = Self::new(SessionStore::new(), Viewport::current);
    if splash_wanted() {
      stores.router.replace(Route::Splash);
    }
    provide_context(stores);
    stores
  }

  /// The stores the root provided. Read it in a component's body, never in a handler.
  ///
  /// # Panics
  ///
  /// Outside the app's root.
  pub fn get() -> Self {
    use_context::<Self>().expect("the root provides the stores")
  }

  /// The terminal's size in cells (tracked).
  pub fn viewport(&self) -> Viewport {
    self.viewport.get()
  }

  /// The width class (tracked): commands whose effect depends on it read it (`f` opens the
  /// filter dialog at S; `↑↓` move by the grid's columns in the address book).
  pub fn bp(&self) -> Bp {
    self.viewport.get().bp()
  }

  /// The page on screen (tracked).
  pub fn page(&self) -> Page {
    self.router.page()
  }

  /// Runs `future` under the root's owner: it outlives the page that started it
  /// ([`SessionStore::spawn`]).
  pub fn spawn(&self, future: impl Future<Output = ()> + 'static) {
    self.session.spawn(future);
  }

  /// Whether something is submitting (login / register, logout, checkout, an address
  /// save): only `?` works meanwhile (keymap.md, K2 / K6) (tracked).
  pub fn busy(&self) -> bool {
    self.session.submitting()
      || self.session.logging_out()
      || self.cart.submitting()
      || self.addresses.saving()
  }
}

/// Stores for unit tests: every task runs to its end at once on the fixture transport.
#[cfg(test)]
pub(crate) mod testing {
  use nsh_core::element::{Runtime, create_root};

  use super::*;
  use crate::api::{ApiClient, FixtureSet, Transport};

  /// Runs `test` with stores over fixture scenario `set`, a connection with or without a
  /// key, in a terminal of `cols` × `rows`.
  pub fn with_stores(
    set: FixtureSet,
    fingerprint: Option<&str>,
    (cols, rows): (u16, u16),
    test: impl FnOnce(Stores),
  ) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let client =
      ApiClient::new(Transport::Fixture(set), None).with_fingerprint(fingerprint.map(Into::into));
    let (owner, stores) = create_root(|| {
      Stores::new(SessionStore::inline(client), move || {
        Viewport::new(cols, rows)
      })
    });
    test(stores);
    owner.dispose();
  }
}
