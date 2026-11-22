//! The header of the app pages (the login page has none): the wordmark (home, as `1`), the
//! tabs with the cart's count badge and the underline of the current tab on the header's
//! bottom line, the search entry, and who is shopping.
//!
//! ```text
//!                                                                                                        ╭───────────────────────────────────╮
//!   ◆ROAST/TTY              ‹1› 选购   ‹2› 购物车 ▐3▌  ‹3› 地址本     ‹4› 我的                           │ ‹/› 搜索豆子、产区或风味…         │    ‹林› 林晓 Lv.3
//!                                                                                                        ╰───────────────────────────────────╯
//! ───────────────────────━━━━━━━━━━━━━━───────────────────────────────────────────────────────────────────────────────────────────────────────────────────
//!  ◆ROAST/TTY  ‹1› 选购  ‹2› 购物车▐3▌  ‹3› 地址本  ‹4› 我的    ‹/› 搜索     ‹林›              (S, 80×24)
//! ─────────────━━━━━━━━───────────────────────────────────────────────────────────
//! ```
//!
//! Tall terminals (32 rows and more) from M up: 4 rows, the search entry a 3-row box (37
//! cells at XL, the room left at L and M, at least 20). S and short terminals: 2 rows, the
//! search entry one row (`‹/› 搜索` at S). Tabs are 14 cells wide at L and XL, tight (two
//! apart) at M and S. Who is shopping: `‹林› 林晓 Lv.3` (XL, L), `‹林› 林晓` (M), `‹林›` (S);
//! a guest `‹客› 访客 登录 →` (`‹客› 访客` at M). `◇ 样例数据` (XL) says the fixtures answer instead of a backend.
//!
//! The search entry shows the catalog's query (or the placeholder) and, pressed, runs the
//! `/` command: the text field itself is the catalog page's (`actions::SEARCH_FIELD`), so
//! one field owns the typing and the focus. A press on a tab runs the tab's command, as its
//! key does (leaving a text field first, so the press is not lost to the field).

use nsh_core::prelude::*;

use crate::{
  commands,
  router::TABS,
  stores::{Stores, actions::SEARCH_FIELD},
  ui::{
    self, Bp, FrameStyle, Layout, Tone, Viewport, kbd, label, layout, nav_tabs::wide_tabs,
    nav_tabs_with, pressable, toned,
  },
};

/// The search entry's placeholder.
pub const SEARCH_PLACEHOLDER: &str = "搜索豆子、产区或风味…";

/// The commands of the tabs, by index.
pub const TAB_COMMANDS: [&str; 4] = ["nav.catalog", "nav.cart", "nav.addresses", "nav.profile"];

/// How the header lays itself out in a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderLayout {
  /// Its rows, the bottom line included: 4, or 2 (S, short).
  pub rows: u16,
  /// Cells before the wordmark.
  pub margin: u16,
  /// Cells between the wordmark and the first tab.
  pub gap: u16,
  /// Wide tabs (14 cells each).
  pub wide: bool,
  /// The search entry's width in cells (`None`: it takes the room left).
  pub search: Option<u16>,
  /// The search entry is the 3-row box.
  pub boxed: bool,
}

impl HeaderLayout {
  /// The column of the first tab (where the bottom line's underline counts from).
  pub fn tabs_start(&self) -> u16 {
    self.margin + 10 + self.gap
  }
}

/// The header's layout in `viewport`.
pub fn header_layout(viewport: Viewport) -> HeaderLayout {
  let bp = viewport.bp();
  let tall = !viewport.short() && bp >= Bp::M;
  let wide = wide_tabs(bp);
  HeaderLayout {
    rows: if tall { 4 } else { 2 },
    margin: if bp == Bp::Xl { 2 } else { 1 },
    gap: if wide { 11 } else { 2 },
    wide,
    search: match bp {
      Bp::Xl => Some(37),
      Bp::S | Bp::TooSmall => Some(9),
      Bp::M | Bp::L => None,
    },
    boxed: tall,
  }
}

/// Who is shopping, after the avatar: `林晓 Lv.3` (L, XL), `林晓` (M), nothing (S); a guest
/// `访客`.
pub fn account(user: Option<(&str, u32)>, bp: Bp) -> String {
  match (user, bp) {
    (_, Bp::S | Bp::TooSmall) => String::new(),
    (Some((name, _)), Bp::M) => name.to_owned(),
    (Some((name, level)), _) => format!("{name} Lv.{level}"),
    (None, _) => "访客".to_owned(),
  }
}

/// The avatar's character: the name's first, `客` for a guest.
pub fn avatar(user: Option<&str>) -> String {
  user
    .and_then(|name| name.chars().next())
    .map_or_else(|| "客".to_owned(), String::from)
}

/// What the search entry says: the query, else the placeholder (`搜索` at S).
pub fn search_text(query: &str, bp: Bp) -> (String, bool) {
  match (query.is_empty(), bp <= Bp::S) {
    (false, _) => (query.to_owned(), true),
    (true, true) => ("搜索".to_owned(), false),
    (true, false) => (SEARCH_PLACEHOLDER.to_owned(), false),
  }
}

/// Leaves a text field, then runs command `id` as its key would.
fn press(stores: Stores, id: &'static str) {
  stores.ui.leave_field();
  commands::press(&stores, id);
}

/// The search entry.
fn search_entry(stores: Stores, layout: Layout, header: HeaderLayout) -> View {
  let focused = move || stores.ui.focused_field() == Some(SEARCH_FIELD);
  let text = move || {
    let query = stores.catalog.query().search;
    search_text(&query, layout.bp())
  };
  let line = view()
    .flex_1()
    .min_w(px(0.0))
    .flex_row()
    .child(kbd("/"))
    .child(text_run(" "))
    .child(
      toned(
        move || SharedString::from(text().0),
        move || if text().1 { Tone::Fg } else { Tone::Mute },
      )
      .number_of_lines(1),
    );
  let entry = pressable()
    .label("搜索商品")
    .on_press(move || press(stores, "app.search"));
  let entry = match header.search {
    Some(cells) => entry.flex_none().w(layout::cols(cells)),
    None => entry.flex_1().min_w(layout::cols(20)),
  };
  if header.boxed {
    entry.child(ui::frame(
      || FrameStyle::Round,
      move || if focused() { Tone::Acc } else { Tone::Line },
      None,
      Some(3),
      1,
      line,
    ))
  } else {
    entry.child(line)
  }
}

fn text_run(text: &'static str) -> Text {
  label(text, Tone::Mute)
}

/// Who is shopping: the avatar cap, the name and tier; a guest's `登录 →` goes to the login
/// page.
fn account_chip(stores: Stores, layout: Layout) -> View {
  let session = stores.session;
  let user = move || {
    session
      .member()
      .map(|m| (m.user.username.to_string(), m.user.tier.level))
  };
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .child(kbd(move || {
      SharedString::from(avatar(user().as_ref().map(|(name, _)| name.as_str())))
    }))
    .child(dynamic(move || {
      let user = user();
      let text = account(
        user.as_ref().map(|(name, level)| (name.as_str(), *level)),
        layout.bp(),
      );
      (!text.is_empty()).then(|| {
        view()
          .flex_none()
          .flex_row()
          .child(layout::hspace(1))
          .child(label(text, Tone::Soft))
      })
    }))
    .child(dynamic(move || {
      (user().is_none() && layout.bp() >= Bp::L).then(|| {
        pressable()
          .flex_none()
          .label("登录")
          .on_press(move || {
            stores.ui.leave_field();
            crate::stores::actions::go_login(&stores);
          })
          .flex_row()
          .child(layout::hspace(2))
          .child(label("登录 →", Tone::Acc))
      })
    }))
}

/// The header's top rows: wordmark, tabs, search entry, account.
fn top(stores: Stores, layout: Layout, header: HeaderLayout) -> View {
  let session = stores.session;
  let cart = stores.cart;
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .child(layout::hspace(header.margin))
    .child(ui::wordmark(move || press(stores, TAB_COMMANDS[0])))
    .child(layout::hspace(header.gap))
    .child(nav_tabs_with(
      &TABS,
      move || stores.router.tab(),
      move |index| if index == 1 { cart.item_count() } else { 0 },
      move || header.wide,
      move |index| press(stores, TAB_COMMANDS[index]),
    ))
    .child(match header.search {
      // A fixed entry (XL, S): the room left goes before it.
      Some(_) => view().flex_1().min_w(layout::cols(2)),
      // The entry takes the room left (L, M).
      None => layout::hspace(2),
    })
    .child(when(
      move || session.offline() && layout.bp() == Bp::Xl,
      || {
        view()
          .flex_none()
          .flex_row()
          .child(label("◇ 样例数据", Tone::Mute))
          .child(layout::hspace(2))
      },
    ))
    .child(search_entry(stores, layout, header))
    .child(layout::hspace(if header.wide { 4 } else { 2 }))
    .child(account_chip(stores, layout))
    .child(layout::hspace(header.margin))
}

/// The header of `stores`' session, following the terminal's size.
pub fn header(stores: Stores, layout: Layout) -> impl IntoElement {
  let cart = stores.cart;
  let header = computed(move || header_layout(layout.viewport()));
  dynamic(move || {
    let header = header.get();
    view()
      .flex_none()
      .w_full()
      .label("顶栏")
      .child(top(stores, layout, header))
      .child(ui::nav_rule(
        &TABS,
        move || stores.router.tab(),
        move || vec![0, cart.item_count(), 0, 0],
        header.tabs_start(),
        move || header.wide,
      ))
  })
}

/// The header's rows in `viewport` on an app page (the login page has none).
pub fn rows(viewport: Viewport) -> u16 {
  header_layout(viewport).rows
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_header_follows_the_breakpoints() {
    let xl = header_layout(Viewport::new(160, 50));
    assert_eq!(xl.rows, 4);
    assert_eq!(xl.tabs_start(), 23, "the design's column 23");
    assert_eq!(xl.search, Some(37));
    assert!(xl.boxed && xl.wide);
    let l = header_layout(Viewport::new(130, 40));
    assert_eq!((l.rows, l.search, l.wide), (4, None, true));
    let m = header_layout(Viewport::new(100, 30));
    assert_eq!(
      (m.rows, m.boxed, m.wide),
      (2, false, false),
      "short: 2 rows"
    );
    let m_tall = header_layout(Viewport::new(100, 40));
    assert_eq!((m_tall.rows, m_tall.boxed), (4, true));
    let s = header_layout(Viewport::new(80, 24));
    assert_eq!((s.rows, s.tabs_start()), (2, 13), "the S mock's column 13");
    assert_eq!(rows(Viewport::new(160, 31)), 2);
  }

  #[test]
  fn the_account_shrinks_with_the_width() {
    assert_eq!(account(Some(("林晓", 3)), Bp::Xl), "林晓 Lv.3");
    assert_eq!(account(Some(("林晓", 3)), Bp::M), "林晓");
    assert_eq!(account(Some(("林晓", 3)), Bp::S), "");
    assert_eq!(account(None, Bp::L), "访客");
    assert_eq!(avatar(Some("林晓")), "林");
    assert_eq!(avatar(None), "客");
    assert_eq!(search_text("", Bp::S), ("搜索".to_owned(), false));
    assert_eq!(search_text("茉莉", Bp::Xl), ("茉莉".to_owned(), true));
    assert_eq!(search_text("", Bp::M).0, SEARCH_PLACEHOLDER);
  }
}
