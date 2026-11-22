//! `最近订单 ORDERS`: the member's orders, newest first, read-only: number, date, items (cut
//! with `…` by cells), the server's amount and the status. In two columns the list scrolls
//! inside its panel (PageUp / PageDown); in one column the page scrolls. Loading, error with
//! `⏎ 重试`, and `还没有订单` with `1 去挑豆子`.

use coffee_api::{OrderStatus as Status, OrderSummary};
use nsh_core::prelude::*;

use super::geometry::Geometry;
use crate::{
  commands::press,
  pages::format,
  stores::{Phase, Stores, profile::ProfileFocus},
  ui::{
    Button, ButtonKind, OrderStatus, Tone, error_state, label,
    layout::{self, hspace, vspace},
    loading_live, order_row, panel, strong, table_rule,
    text::truncate,
  },
};

/// The status as the order row draws it.
pub fn status(status: Status) -> OrderStatus {
  match status {
    Status::Roasting => OrderStatus::Roasting,
    Status::Shipping => OrderStatus::Shipping,
    Status::Delivered => OrderStatus::Delivered,
  }
}

/// `慧兰 粉红波旁 250g ×1 · 「夜班」意式拼配 500g ×1`, cut to `cells` with `…` (wide
/// characters are never split).
pub fn items_summary(order: &OrderSummary, cells: u16) -> String {
  let whole = order
    .lines
    .iter()
    .map(|line| {
      format!(
        "{} {}",
        line.name,
        format::order_line(line.weight, line.quantity)
      )
    })
    .collect::<Vec<_>>()
    .join(" · ");
  truncate(&whole, usize::from(cells))
}

fn fixed(cells: u16) -> View {
  view().flex_none().w(layout::cols(cells))
}

/// The head row (`mute`), its columns where the order rows put theirs.
fn head(geo: Geometry) -> View {
  let [number, date, amount, state] = geo.order_widths;
  view()
    .flex_none()
    .flex_row()
    .gap(layout::cols(1))
    .child(fixed(number).child(label("订单号", Tone::Mute)))
    .child(fixed(date).child(label("日期", Tone::Mute)))
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .child(label("商品", Tone::Mute)),
    )
    .child(fixed(amount).items_end().child(label("金额", Tone::Mute)))
    .child(fixed(state).items_end().child(label("状态", Tone::Mute)))
}

fn row(order: &OrderSummary, geo: Geometry) -> View {
  order_row(
    order.number.clone(),
    format::month_day(&order.order_date),
    items_summary(order, geo.order_items()),
    format::yuan_cents(order.total),
    status(order.status),
    geo.order_widths,
  )
}

/// `还没有订单` and `[ 1 去挑豆子 ]`, centred.
fn empty(stores: Stores) -> View {
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .child(label("还没有订单", Tone::Soft))
    .child(vspace(1))
    .child(
      Button::new(ButtonKind::Default, "1", "去挑豆子")
        .on_press(move || press(&stores, "nav.catalog")),
    )
}

/// The list's body for the orders' state now: rows (in a scroll view when `scrolls`), or
/// the loading line, the error, the empty state. `trail` blank columns end every row (the
/// scroll bar's column).
fn body(stores: Stores, geo: Geometry, scrolls: bool, trail: u16) -> AnyElement {
  let orders = stores.orders;
  let settings = stores.settings;
  dynamic(move || {
    let items = orders.items();
    match orders.phase() {
      Phase::Failed => {
        let detail = orders.failure().map(|f| f.detail()).unwrap_or_default();
        error_state("订单加载失败", detail, "⏎", "重试", move || {
          press(&stores, "profile.retry")
        })
        .into_element()
      }
      Phase::Idle | Phase::Loading if items.is_empty() => {
        loading_live("正在载入订单…", move || settings.animations()).into_element()
      }
      Phase::Empty => empty(stores).into_element(),
      _ => {
        let rows = view().flex_none().children(
          items
            .iter()
            .map(|order| {
              view()
                .flex_row()
                .child(view().flex_1().min_w(px(0.0)).child(row(order, geo)))
                .child(hspace(trail))
            })
            .collect::<Vec<_>>(),
        );
        if scrolls {
          scroll().flex_1().child(rows).into_element()
        } else {
          rows.into_element()
        }
      }
    }
  })
}

/// The order panel: head, rule and body. In two columns it fills the rest of its column
/// and scrolls inside; in one column it is as tall as its rows.
pub fn orders_panel(stores: Stores, geo: Geometry) -> View {
  let profile = stores.profile;
  // ui::panel keeps a blank column inside each side line; the design's second one goes on
  // the left here and inside the rows on the right, where the scroll bar draws.
  let extra = geo.pad.saturating_sub(1);
  let scrolls = !geo.one_column;
  let focused = move || profile.is_focused(ProfileFocus::Orders);
  let content = view()
    .flex_1()
    .min_h(px(0.0))
    .child(geo.roomy.then(|| vspace(1)))
    .child(
      view()
        .flex_row()
        .child(
          view()
            .flex_none()
            .w(layout::cols(extra))
            .child(dynamic(move || focused().then(|| strong("›", Tone::Acc)))),
        )
        .child(view().flex_1().min_w(px(0.0)).child(head(geo)))
        .child(hspace(extra)),
    )
    .child(table_rule())
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(hspace(extra))
        .child(
          view()
            .flex_1()
            .min_w(px(0.0))
            .child(body(stores, geo, scrolls, extra)),
        ),
    );
  let panel = panel("最近订单 ORDERS", content);
  if scrolls {
    panel
  } else {
    view().flex_none().child(panel)
  }
}

#[cfg(test)]
mod tests {
  use coffee_api::OrderList;

  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn summaries_are_cut_by_cells() {
    let list: OrderList = serde_json::from_str(include_str!(
      "../../../../../packages/contract/fixtures/v1/orders/list.json"
    ))
    .unwrap();
    let second = &list.items[1];
    assert_eq!(
      items_summary(second, 50),
      "慧兰 粉红波旁 250g ×1 · 「夜班」意式拼配 500g ×1"
    );
    let cut = items_summary(second, 14);
    assert_eq!(cut, "慧兰 粉红波旁…");
    assert!(display_width(&cut) <= 14);
    // An odd width never splits a wide character.
    let odd = items_summary(&list.items[0], 19);
    assert!(display_width(&odd) <= 19 && odd.ends_with('…'), "{odd}");
    assert_eq!(status(Status::Shipping), OrderStatus::Shipping);
    assert_eq!(status(Status::Delivered).text(), "● 已签收");
  }
}
