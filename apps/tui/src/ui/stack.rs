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

/// The blank rows the shell keeps under the content at `bp` (responsive.md: XL 1, else 0;
/// none under 32 rows).
pub fn bottom_margin(bp: Bp, short: bool) -> u16 {
  u16::from(bp == Bp::Xl && !short)
}

/// The rows a page's content gets in a `cols` × `rows` terminal: what the system row, the
/// header (4 rows, 2 at S and under 32 rows), the status bar and the margins leave.
pub fn content_rows(viewport: super::layout::Viewport) -> u16 {
  let bp = viewport.bp();
  let short = viewport.short();
  let header = if short || bp <= Bp::S { 2 } else { 4 };
  viewport
    .rows
    .saturating_sub(1 + header + 1 + top_margin(bp, short) + bottom_margin(bp, short))
}

/// The columns a page's content gets: 200 at most, less the side margins (XL 2 each side,
/// L and M 1, S none).
pub fn content_cols(viewport: super::layout::Viewport) -> u16 {
  viewport
    .cols
    .min(layout::MAX_CONTENT_COLS)
    .saturating_sub(2 * super::window::content_margins(viewport.bp()).0)
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
    assert_eq!(bottom_margin(Bp::Xl, false), 1);
    assert_eq!(bottom_margin(Bp::Xl, true), 0);
    assert_eq!(bottom_margin(Bp::M, false), 0);
    use super::super::layout::Viewport;
    // The content areas of the four design sizes.
    assert_eq!(content_rows(Viewport::new(160, 50)), 42);
    assert_eq!(content_rows(Viewport::new(130, 40)), 33);
    assert_eq!(content_rows(Viewport::new(100, 40)), 34);
    assert_eq!(content_rows(Viewport::new(100, 30)), 26);
    assert_eq!(content_rows(Viewport::new(80, 24)), 20);
    assert_eq!(content_cols(Viewport::new(160, 50)), 156);
    assert_eq!(content_cols(Viewport::new(130, 40)), 128);
    assert_eq!(content_cols(Viewport::new(80, 24)), 80);
    assert_eq!(content_cols(Viewport::new(240, 60)), 196);
  }
}
