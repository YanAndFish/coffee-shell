//! 选购 BROWSE (02-list.md): the bean list with its filters, sort, search and preview, and
//! the S-width filter dialog (`筛选 FILTER`).
//!
//! Store: [`crate::stores::CatalogStore`] — `phase`, `failure`, `products`, `count`,
//! `facets`, `origins`, `query`, `cursor` / `current` / `is_cursor` / `move_cursor` /
//! `select`, `set_origin` / `set_roast` / `set_sort` / `set_search` / `cycle_*` /
//! `clear_filters`, `ensure_loaded` / `load`, and the filter dialog's `filter_*`.
//! Actions: `actions::open_product`, `actions::quick_add`, `actions::search`. The search box
//! reports its focus as [`crate::stores::actions::SEARCH_FIELD`] (`/` asks it to focus).
//!
//! The body is a placeholder (plain lines); the list screen replaces it.

pub mod commands;

use coffee_api::roast_name;
use nsh_core::{element::Screen, prelude::*};

use super::{
  Dialog, PageModule, format,
  placeholder::{self, Line, phase_lines},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{
    Overlay, Phase, Stores,
    actions::SEARCH_FIELD,
    catalog::{FilterGroup, roast_label, sort_label},
  },
  ui::{self, Tone},
};

/// `选购 BROWSE`.
pub const MODE: Mode = Mode::new("选购", "BROWSE");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Catalog,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[Dialog {
    overlay: Overlay::FilterDialog,
    view: filter_dialog,
  }],
};

/// The list panel's legend: `商品列表 · {n} 款 · {排序}`.
pub fn legend(phase: Phase, count: usize, sort: &str) -> String {
  match phase {
    Phase::Idle | Phase::Loading => "商品列表 · 载入中".into(),
    Phase::Failed => "商品列表 · 加载失败".into(),
    Phase::Ready | Phase::Empty => format!("商品列表 · {count} 款 · {sort}"),
  }
}

fn lines(stores: Stores) -> Vec<Line> {
  let catalog = stores.catalog;
  let query = catalog.query();
  let mut lines = vec![Line::soft(format!(
    "产区 {} · 烘焙度 {} · 排序 {}",
    query.origin_label(),
    roast_label(query.roast),
    sort_label(query.sort)
  ))];
  lines.extend(phase_lines(
    catalog.phase(),
    catalog.failure(),
    "商品",
    "没有匹配的豆子 · Esc 清除筛选",
    Some("catalog.retry"),
  ));
  for product in catalog.products() {
    let cursor = if catalog.is_cursor(&product.id) {
      "▶"
    } else {
      " "
    };
    let tag = product
      .tag
      .as_ref()
      .map(|tag| format!(" [{tag}]"))
      .unwrap_or_default();
    let text = format!(
      "{cursor} {}{tag} · {} · {} · {}",
      product.name,
      product.origin,
      roast_name(product.roast),
      format::yuan(product.price250)
    );
    lines.push(if catalog.is_cursor(&product.id) {
      Line::new(text, Tone::Acc).press("catalog.open")
    } else {
      Line::text(text)
    });
  }
  lines
}

/// The list page (placeholder body).
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let catalog = stores.catalog;
  let ui_store = stores.ui;
  // The page's work: ask for the beans once it shows (the store keeps them after).
  effect(move || untracked(|| catalog.ensure_loaded()));
  // The search box shows while it is asked for (`/`), focused, or holds text: a field
  // always there would take the focus when the detail page above goes (nsh gives the
  // revealed page's first field the focus), and with it the list's keys.
  let searching = move || {
    ui_store.focus_requested(SEARCH_FIELD)
      || ui_store.focused_field() == Some(SEARCH_FIELD)
      || !catalog.query().search.is_empty()
  };
  ui::page(
    "选购 BROWSE",
    ui::panel(
      move || {
        let query = catalog.query();
        SharedString::from(legend(
          catalog.phase(),
          catalog.count(),
          sort_label(query.sort),
        ))
      },
      view()
        .flex_1()
        .child(when(searching, move || {
          placeholder::field(
            SEARCH_FIELD,
            "/ 搜索",
            move || catalog.query().search,
            move |text| catalog.set_search(text),
            false,
          )
        }))
        .child(placeholder::lines(move || lines(stores))),
    ),
  )
}

/// The S-width filter dialog (placeholder body): each group with its options, the focus
/// as `▶`, the query's choice as `◉`.
pub fn filter_dialog() -> AnyElement {
  let stores = Stores::get();
  let catalog = stores.catalog;
  placeholder::panel(
    || "筛选 FILTER".to_owned(),
    move || {
      let (focus_group, focus_index) = catalog.filter_focus();
      let mut lines = Vec::new();
      for group in FilterGroup::ALL {
        lines.push(Line::new(format!("── {}", group.label()), Tone::Mute));
        let chosen = catalog.filter_selected(group);
        for (index, option) in catalog.filter_options(group).into_iter().enumerate() {
          let cursor = if (group, index) == (focus_group, focus_index) {
            "▶"
          } else {
            " "
          };
          let mark = if index == chosen { "◉" } else { "○" };
          lines.push(Line::text(format!("{cursor} {mark} {option}")));
        }
      }
      lines.push(Line::soft(
        "↑↓ 选择 · Tab 下一组 · ⏎ / space 选中，立即生效 · Esc 关闭",
      ));
      lines
    },
  )
  .into_element()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_legend_follows_the_phase() {
    assert_eq!(legend(Phase::Loading, 0, "推荐"), "商品列表 · 载入中");
    assert_eq!(legend(Phase::Ready, 8, "推荐"), "商品列表 · 8 款 · 推荐");
    assert_eq!(
      legend(Phase::Empty, 0, "杯测分"),
      "商品列表 · 0 款 · 杯测分"
    );
    assert_eq!(legend(Phase::Failed, 0, "推荐"), "商品列表 · 加载失败");
  }
}
