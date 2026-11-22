//! The page stack: nsh's navigation stack and its pages, styled for the shop. The terminal
//! shows no system header; the shell's own header and status bar frame every page.

use nsh_base::{nav_page, nav_stack};
use nsh_core::{
  element::{Screen, Stack},
  prelude::*,
};

use super::layout;

/// The stack filling its parent; its children are [`page`]s, bottom to top.
pub fn page_stack() -> Stack {
  nav_stack().flex_1()
}

/// A page titled `title` (for screen readers and the phones' system header) holding
/// `content`, one blank row under the header.
pub fn page(title: impl Into<SharedString>, content: impl IntoElement) -> Screen {
  nav_page()
    .title(title.into())
    .child(view().flex_1().mt(layout::rows(1)).child(content))
}
