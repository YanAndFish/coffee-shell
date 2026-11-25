//! Routes and the routes store: the pages of nsh's navigation stack, bottom to top.
//!
//! The shell renders one `nav_page` per route in a `nav_stack` (keyed by the route), so a
//! page that stays in the stack keeps its state. A new connection starts on the splash
//! (00-splash.md), which gives way to the login page (01-login.md, "何时出现") or, for a
//! remembered key, the catalog. Tabs `1`–`4`
//! replace the whole stack with their root route; the detail page is pushed over the
//! catalog and popped by Esc / Backspace. When the user goes back natively, the page's
//! `on_dismiss` truncates the routes.
//!
//! The login gate is not a route rule: guests may open every tab (the address book and the
//! profile show a guest card). Checkout is gated (`stores::actions::checkout`): a guest goes
//! to the login page and comes back to the cart (D6, K7) through
//! `SessionStore::set_return_to`.

use nsh_core::prelude::*;

use crate::ui::NavTab;

/// A page of the app.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Route {
  /// The splash while the connection starts (`欢迎 WELCOME`): no tab, no global keys.
  Splash,
  /// Sign in or register (`登录 LOGIN`): no tab, no global keys.
  Login,
  /// The bean list: the tab `1 选购`.
  Catalog,
  /// One bean, by product id (`yirga`), over the catalog.
  Detail(SharedString),
  /// The tab `2 购物车` (with the receipt after checkout).
  Cart,
  /// The tab `3 地址本`.
  Addresses,
  /// The tab `4 我的`.
  Profile,
}

/// The kind of page a route shows, without its data: what command scopes and page modules
/// name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
  /// [`Route::Splash`].
  Splash,
  /// [`Route::Login`].
  Login,
  /// [`Route::Catalog`].
  Catalog,
  /// [`Route::Detail`].
  Detail,
  /// [`Route::Cart`].
  Cart,
  /// [`Route::Addresses`].
  Addresses,
  /// [`Route::Profile`].
  Profile,
}

impl Page {
  /// Every page, in the order of the page modules.
  pub const ALL: [Page; 7] = [
    Page::Splash,
    Page::Login,
    Page::Catalog,
    Page::Detail,
    Page::Cart,
    Page::Addresses,
    Page::Profile,
  ];
}

/// The header's tabs, in the design's fixed order.
pub const TABS: [NavTab; 4] = [
  NavTab {
    key: "1",
    label: "选购",
  },
  NavTab {
    key: "2",
    label: "购物车",
  },
  NavTab {
    key: "3",
    label: "地址本",
  },
  NavTab {
    key: "4",
    label: "我的",
  },
];

impl Route {
  /// The root route of tab `index` (0-based, as [`TABS`]).
  pub fn tab_root(index: usize) -> Option<Route> {
    match index {
      0 => Some(Route::Catalog),
      1 => Some(Route::Cart),
      2 => Some(Route::Addresses),
      3 => Some(Route::Profile),
      _ => None,
    }
  }

  /// Its kind of page.
  pub fn page(&self) -> Page {
    match self {
      Route::Splash => Page::Splash,
      Route::Login => Page::Login,
      Route::Catalog => Page::Catalog,
      Route::Detail(_) => Page::Detail,
      Route::Cart => Page::Cart,
      Route::Addresses => Page::Addresses,
      Route::Profile => Page::Profile,
    }
  }

  /// The tab this route belongs to (the detail page belongs to `选购`; splash and login to
  /// none).
  pub fn tab(&self) -> Option<usize> {
    match self {
      Route::Splash | Route::Login => None,
      Route::Catalog | Route::Detail(_) => Some(0),
      Route::Cart => Some(1),
      Route::Addresses => Some(2),
      Route::Profile => Some(3),
    }
  }

  /// The page's stable key in the navigation stack.
  pub fn key(&self) -> SharedString {
    match self {
      Route::Detail(id) => format!("detail/{id}").into(),
      other => format!("{other:?}").to_lowercase().into(),
    }
  }
}

/// The routes, bottom to top, behind their functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct Router {
  routes: Signal<Vec<Route>>,
}

impl Router {
  /// A stack holding `start` alone.
  pub fn new(start: Route) -> Self {
    Self {
      routes: signal(vec![start]),
    }
  }

  /// The routes, bottom to top (tracked).
  pub fn routes(&self) -> Vec<Route> {
    self.routes.get()
  }

  /// The top route: the page on screen (tracked).
  pub fn top(&self) -> Route {
    self
      .routes
      .with(|routes| routes.last().cloned())
      .unwrap_or(Route::Catalog)
  }

  /// The top route, untracked: for key handlers deciding what a key does.
  pub fn top_untracked(&self) -> Route {
    self
      .routes
      .with_untracked(|routes| routes.last().cloned())
      .unwrap_or(Route::Catalog)
  }

  /// The page on screen (tracked).
  pub fn page(&self) -> Page {
    self.top().page()
  }

  /// The current tab (tracked).
  pub fn tab(&self) -> Option<usize> {
    self.top().tab()
  }

  /// Shows tab `index`: its root route alone (nothing changes when already there).
  pub fn go_tab(&self, index: usize) {
    if let Some(root) = Route::tab_root(index) {
      self.replace(root);
    }
  }

  /// Goes to `route`: a detail page is pushed over the catalog (the catalog is put under
  /// it when the stack is elsewhere); any other route becomes the whole stack.
  pub fn go(&self, route: Route) {
    match route {
      Route::Detail(_) => {
        if self.top_untracked().page() != Page::Catalog
          && self.top_untracked().page() != Page::Detail
        {
          self.replace(Route::Catalog);
        }
        if self.top_untracked().page() == Page::Detail {
          self.pop();
        }
        self.push(route);
      }
      route => self.replace(route),
    }
  }

  /// Makes `route` the whole stack.
  pub fn replace(&self, route: Route) {
    let next = vec![route];
    if self.routes.with_untracked(|routes| *routes != next) {
      self.routes.set(next);
    }
  }

  /// Pushes `route` over the top one (not again when it is already on top).
  pub fn push(&self, route: Route) {
    if self
      .routes
      .with_untracked(|routes| routes.last() == Some(&route))
    {
      return;
    }
    self.routes.update(|routes| routes.push(route));
  }

  /// Pops the top page; the bottom one stays. Whether one was popped.
  pub fn pop(&self) -> bool {
    if self.routes.with_untracked(|routes| routes.len() <= 1) {
      return false;
    }
    self.routes.update(|routes| {
      routes.pop();
    });
    true
  }

  /// Keeps the bottom `depth` pages (at least one): the user went back natively.
  pub fn truncate(&self, depth: usize) {
    if self
      .routes
      .with_untracked(|routes| routes.len() > depth.max(1))
    {
      self.routes.update(|routes| routes.truncate(depth.max(1)));
    }
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;

  fn with_router(test: impl FnOnce(Router)) {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, router) = create_root(|| Router::new(Route::Login));
    test(router);
    owner.dispose();
  }

  #[test]
  fn a_connection_starts_on_the_login_page_and_tabs_replace_the_stack() {
    with_router(|router| {
      assert_eq!(router.top(), Route::Login);
      assert_eq!(router.tab(), None);
      router.go_tab(0);
      router.push(Route::Detail("yirga".into()));
      assert_eq!(router.routes().len(), 2);
      router.go_tab(1);
      assert_eq!(router.routes(), vec![Route::Cart]);
      assert_eq!(router.tab(), Some(1));
      router.go_tab(9);
      assert_eq!(router.routes(), vec![Route::Cart]);
    });
  }

  #[test]
  fn details_go_over_the_catalog() {
    with_router(|router| {
      router.go(Route::Detail("geisha".into()));
      assert_eq!(
        router.routes(),
        vec![Route::Catalog, Route::Detail("geisha".into())]
      );
      // A detail from a detail (a recommendation) replaces it.
      router.go(Route::Detail("pinkb".into()));
      assert_eq!(
        router.routes(),
        vec![Route::Catalog, Route::Detail("pinkb".into())]
      );
      router.go(Route::Profile);
      assert_eq!(router.routes(), vec![Route::Profile]);
    });
  }

  #[test]
  fn push_pop_and_truncate() {
    with_router(|router| {
      router.replace(Route::Catalog);
      assert!(!router.pop(), "the bottom page stays");
      router.push(Route::Detail("yirga".into()));
      router.push(Route::Detail("yirga".into()));
      assert_eq!(router.routes().len(), 2, "no second push of the top route");
      assert_eq!(router.top().tab(), Some(0));
      assert_eq!(router.page(), Page::Detail);
      assert!(router.pop());
      assert_eq!(router.top(), Route::Catalog);
      router.push(Route::Detail("a".into()));
      router.truncate(0);
      assert_eq!(router.routes(), vec![Route::Catalog]);
    });
  }

  #[test]
  fn tab_roots_and_keys() {
    for (index, _) in TABS.iter().enumerate() {
      let root = Route::tab_root(index).unwrap();
      assert_eq!(root.tab(), Some(index));
    }
    assert_eq!(Route::Login.tab(), None);
    assert_eq!(Route::Detail("yirga".into()).key(), "detail/yirga");
    assert_eq!(Route::Catalog.key(), "catalog");
    assert_eq!(Page::ALL.len(), 7);
    assert_eq!(Route::Splash.tab(), None);
  }
}
