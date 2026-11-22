//! The pages, one per route. Pages read the stores and call their functions (directly or
//! through the command table); they compose `ui` components and never pick colours.
//! Phase 1 implements the catalog; the others are honest placeholders.

mod catalog;
mod placeholder;

use nsh_core::element::Screen;

use crate::router::Route;

/// The page of `route`, for the shell's stack.
pub fn page(route: &Route) -> Screen {
  match route {
    Route::Catalog => catalog::catalog_page(),
    Route::Detail(id) => placeholder::detail_page(id.clone()),
    Route::Login => placeholder::tab_page("登录 LOGIN", "登录与注册"),
    Route::Cart => placeholder::tab_page("购物车 CART", "购物车"),
    Route::Addresses => placeholder::tab_page("地址本 ADDRESS", "地址本"),
    Route::Profile => placeholder::tab_page("我的 PROFILE", "个人信息与订单"),
  }
}
