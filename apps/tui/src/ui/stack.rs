//! The page stack: nsh's navigation stack and its pages, styled for the shop. The terminal
//! shows no system header; the shell's own header and status bar frame every page.

use nsh_base::{nav_page, nav_stack};
use nsh_core::{
  element::{Screen, Stack},
  prelude::*,
};

use super::layout::{self, Bp, Layout};

/// The stack filling its parent; its children are [`page`]s, bottom to top.
pub fn page_stack() -> Stack {
  nav_stack().flex_1()
}

/// The blank rows between the header and a page's content at `bp` (responsive.md, "内容边距":
/// XL and L 1, M and S none, none under 32 rows).
pub fn top_margin(bp: Bp, short: bool) -> u16 {
  u16::from(matches!(bp, Bp::Xl | Bp::L) && !short)
}

/// A page titled `title` (for screen readers and the phones' system header) holding
/// `content`, under its top margin ([`top_margin`]).
pub fn page(title: impl Into<SharedString>, content: impl IntoElement) -> Screen {
  let layout = Layout::get();
  let top = computed(move || {
    let viewport = layout.viewport();
    top_margin(viewport.bp(), viewport.short())
  });
  nav_page().title(title.into()).child(
    view()
      .flex_1()
      .child(dynamic(move || layout::vspace(top.get())))
      .child(view().flex_1().min_h(px(0.0)).child(content)),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_top_margin_follows_responsive_md() {
    assert_eq!(top_margin(Bp::Xl, false), 1);
    assert_eq!(top_margin(Bp::L, false), 1);
    assert_eq!(top_margin(Bp::M, false), 0);
    assert_eq!(top_margin(Bp::S, false), 0);
    assert_eq!(top_margin(Bp::Xl, true), 0);
    assert_eq!(top_margin(Bp::L, true), 0);
  }
}
