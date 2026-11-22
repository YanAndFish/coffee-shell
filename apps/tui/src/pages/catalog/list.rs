//! The list panel (`商品列表 · {n} 款 · {排序}`): the filter row of M and S, the search box
//! while searching, the table head, and the body for the list's phase — the rows in a
//! scroll view that keeps the `▶` row in sight, or the loading, empty and error states.

use coffee_api::ProductSummary;
use nsh_core::prelude::*;

use super::{
  filters,
  form::{Body, Form, Shape, body, legend, row_label, second_line},
};
use crate::{
  commands::press,
  pages::format,
  stores::{Stores, actions, actions::SEARCH_FIELD, catalog::sort_label},
  ui::{
    self, EmptyAction, InputForm, InputSpec, TagKind, Tone, data_row, empty_state, error_state,
    label, layout, loading_live, name_cell, panel, pips, strong, table_head, table_rule, tag,
  },
};

/// The search box's placeholder (copy.md, "搜索框").
pub const SEARCH_PLACEHOLDER: &str = "搜索豆子、产区或风味…";

/// What the list panel shows now (tracked).
pub fn body_now(stores: Stores) -> Body {
  let catalog = stores.catalog;
  let has_data = catalog.facets().is_some();
  body(catalog.phase(), has_data, catalog.count())
}

/// Whether the search box shows: asked for (`/`), focused, or holding text. A box always
/// there would take the focus when the detail page above goes (nsh focuses the revealed
/// page's first field), and with it the list's keys.
fn searching(stores: Stores) -> bool {
  stores.ui.focus_requested(SEARCH_FIELD)
    || stores.ui.focused_field() == Some(SEARCH_FIELD)
    || !stores.catalog.query().search.is_empty()
}

/// The search box: `‹/› 搜索豆子、产区或风味…`, the text live-filtering the list on the
/// server, `✕` clearing it. It reports its focus to the shell as [`SEARCH_FIELD`]; `/` asks
/// it to take the focus, `Esc` / `⏎` leave it (the text stays).
fn search_box(stores: Stores) -> View {
  let ui_store = stores.ui;
  let catalog = stores.catalog;
  // A box removed with the focus in it gives the keys back (no blur comes then).
  cleanup(move || ui_store.field_blurred(SEARCH_FIELD));
  // A box shown because `/` asked for the focus takes it as it mounts (`ui::input`'s
  // `auto_focus`); its handle lets Esc and ⏎ blur it in the same commit.
  let handle = element_ref();
  let wants_focus = move || {
    ui_store.focus_requested(SEARCH_FIELD) || ui_store.focused_field() == Some(SEARCH_FIELD)
  };
  let spec = InputSpec {
    placeholder: SEARCH_PLACEHOLDER.into(),
    element: Some(handle),
    ..InputSpec::new(InputForm::Search, None)
  };
  ui::input(
    spec,
    move || catalog.query().search,
    wants_focus,
    || None,
    move |text: String| catalog.set_search(&text),
    move |focused| {
      if focused {
        batch(|| {
          ui_store.field_focused(SEARCH_FIELD, Some(handle));
          ui_store.focus_taken(SEARCH_FIELD);
        });
      } else {
        ui_store.field_blurred(SEARCH_FIELD);
      }
    },
    // ⏎ leaves the box, as the page's `Esc 离开搜索` command does (the field may take ⏎
    // as its submit before the command table sees it).
    move || ui_store.leave_field(),
  )
}

/// One bean's row: `▶`, the name (bold, `acc` on the cursor row) with its tag and the
/// subtitle under it (short: only the cursor row's second line), then origin, process,
/// roast pips and price. A click puts the cursor on it, a double click opens it; the cursor
/// row scrolls itself into sight.
fn row(stores: Stores, shape: Shape, columns: &[ui::Column], product: ProductSummary) -> View {
  let catalog = stores.catalog;
  let id = product.id.clone();
  let current = {
    let id = id.clone();
    move || catalog.is_cursor(&id)
  };
  let handle = element_ref();
  {
    let current = current.clone();
    effect(move || {
      if current() {
        handle.reveal(
          RevealOptions::default()
            .vertical(ScrollAlignment::Nearest)
            .animated(false),
        );
      }
    });
  }
  let subtitle = (!shape.short).then(|| SharedString::from(product.subtitle.clone()));
  let second = second_line(&product, shape.form);
  let name = view()
    .w_full()
    .child(name_cell(
      product.name.clone(),
      product
        .tag
        .clone()
        .map(|text| tag(TagKind::Outline, text).into_element()),
      subtitle,
      current.clone(),
      || false,
    ))
    .child(shape.short.then(|| {
      when(current.clone(), move || {
        label(second.clone(), Tone::Mute).truncate()
      })
    }));
  let mut cells = vec![
    name.into_element(),
    label(product.origin.clone(), Tone::Soft)
      .truncate()
      .into_element(),
  ];
  if shape.form.has_process() {
    cells.push(
      label(product.process.clone(), Tone::Mute)
        .truncate()
        .into_element(),
    );
  }
  cells.push(label(pips(product.roast, 5), Tone::Acc).into_element());
  cells.push(strong(format::yuan(product.price250), Tone::Fg).into_element());
  let row = data_row(columns, current, cells, move |clicks| {
    catalog.select(&id);
    if clicks >= 2 {
      actions::open_product(&stores, &id);
    }
  })
  .label(row_label(&product))
  .element_ref(&handle);
  // Three rows a bean when there is room: name, subtitle, a blank row (a gap in a scroll
  // view's content is no row in nsh's terminal layout, so the blank is a row of its own).
  view()
    .flex_none()
    .child(row)
    .child((!shape.short).then(|| layout::vspace(1)))
}

/// The rows in a scroll view (its bar on the panel's right inner edge when they do not fit).
fn rows(stores: Stores, shape: Shape) -> AnyElement {
  let catalog = stores.catalog;
  let columns = shape.form.columns();
  scroll()
    .flex_1()
    .min_h(px(0.0))
    .child(view().flex_none().child(each(
      move || catalog.products(),
      |product: &ProductSummary| product.id.clone(),
      move |product, _| row(stores, shape, &columns, product.get_untracked()),
    )))
    .into_element()
}

/// The body under the head for `phase`.
fn body_view(stores: Stores, shape: Shape, body: Body) -> AnyElement {
  let catalog = stores.catalog;
  let animate = move || stores.settings.animations();
  match body {
    Body::Loading => loading_live("正在载入商品…", animate).into_element(),
    Body::Failed => error_state(
      "商品加载失败",
      move || {
        SharedString::from(
          catalog
            .failure()
            .map(|failure| failure.detail())
            .unwrap_or_default(),
        )
      },
      "⏎",
      "重试",
      move || press(&stores, "catalog.retry"),
    )
    .into_element(),
    Body::Empty => empty_state(
      ui::empty_state::NO_RESULTS,
      "没有匹配的豆子",
      None,
      Some(EmptyAction {
        key: "Esc",
        label: "清除筛选",
        run: Box::new(move || press(&stores, "catalog.clear")),
      }),
    )
    .into_element(),
    Body::Rows => rows(stores, shape),
  }
}

/// The list panel for `shape`.
pub fn list_panel(stores: Stores, shape: Shape) -> View {
  let catalog = stores.catalog;
  let body = computed(move || body_now(stores));
  let filter_row: Option<AnyElement> = match shape.form {
    Form::M => Some(filters::chip_row(stores)),
    Form::S => Some(filters::summary_row(stores).into_element()),
    Form::Xl | Form::L => None,
  };
  panel(
    move || {
      SharedString::from(legend(
        body.get(),
        catalog.count(),
        sort_label(catalog.query().sort),
      ))
    },
    view()
      .flex_1()
      .min_h(px(0.0))
      .child((!shape.short).then(|| layout::vspace(1)))
      .child(filter_row)
      .child(when(move || searching(stores), move || search_box(stores)))
      .child(table_head(&shape.form.columns()))
      .child(table_rule())
      .child(dynamic(move || body_view(stores, shape, body.get()))),
  )
}
