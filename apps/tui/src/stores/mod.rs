//! The app's state: `Copy` structs of private signals with public functions, made once by
//! the root and provided to everything under it ([`Stores::provide`]). Pages read the
//! stores and call their functions; a page never writes another page's state.

pub mod catalog;
pub mod session;
pub mod toast;

use nsh_core::prelude::*;

pub use catalog::{CatalogPhase, CatalogState, CatalogStore};
pub use session::SessionStore;
pub use toast::{Toast, ToastStore};

use crate::router::{Route, Router};

/// Every store of the app. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct Stores {
  /// Who is shopping, and the backend client.
  pub session: SessionStore,
  /// The pages, bottom to top.
  pub router: Router,
  /// The bean list.
  pub catalog: CatalogStore,
  /// The notice showing.
  pub toasts: ToastStore,
}

impl Stores {
  /// Makes the stores and provides them to what the current owner makes. Call it once,
  /// in the root.
  pub fn provide() -> Self {
    let stores = Self {
      session: SessionStore::new(),
      router: Router::new(Route::Catalog),
      catalog: CatalogStore::new(),
      toasts: ToastStore::new(),
    };
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
}
