//! The receipt (`订单回执 RECEIPT`): a card min(77, columns − 2) wide, centred in the content
//! area, in place of the cart until the page is left.
//!
//! ```text
//! ╭─ 订单回执 RECEIPT ─────────────────────────────────────────────╮
//! │   ▐✓▌ 订单已提交                                               │
//! │       订单号 RT2611220417 · 预计 11 月 25 日 送达              │
//! │   ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌   │
//! │   慧兰 粉红波旁 250g · 手冲 · 标准袋 × 1 ┈┈┈┈┈┈┈┈┈┈┈ ¥118.00   │
//! │   ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌   │
//! │   实付                                                ¥426.00   │
//! │   寄往 林晓 · 上海市 徐汇区 天平路 88 弄 3 号 602 室            │
//! │   ! 演示订单 · 没有真实扣款，也不会发货                         │
//! │   ▐      ⏎ 继续选购      ▌ [      4 查看订单      ]             │
//! ╰─────────────────────────────────────────────────────────────────╯
//! ```

use coffee_api::Order;
use nsh_core::prelude::*;

use super::{Shape, copy};
use crate::{
  commands::press,
  pages::format,
  ui::{
    self, Button, ButtonKind, Fill, Legend, Tone, label,
    layout::{hspace, vspace},
    strong,
  },
};

/// The card's padding and frame: 4 blank cells and a line each side.
const CHROME: u16 = 10;

/// The receipt of `order`.
pub fn view_of(stores: crate::stores::Stores, shape: Shape, order: Order) -> View {
  let cols = shape.receipt_cols();
  let inner = cols.saturating_sub(CHROME);
  let roomy = !shape.short;
  let gap = move || roomy.then(|| vspace(1));
  let items = order
    .items
    .iter()
    .enumerate()
    .map(|(i, item)| {
      view()
        .flex_none()
        .child(if i > 0 { gap() } else { None })
        .child(ui::receipt_leader(
          &item.name,
          &copy::item_spec(item),
          &format::yuan_cents(item.line_total),
          usize::from(inner),
        ))
    })
    .collect::<Vec<_>>();
  let half = inner.saturating_sub(1) / 2;
  let content = view()
    .flex_none()
    .child(gap())
    .child(
      view()
        .flex_row()
        .child(ui::pill_text(Fill::Acc2, "✓", false))
        .child(hspace(1))
        .child(strong("订单已提交", Tone::Fg)),
    )
    .child(
      view()
        .flex_row()
        .child(hspace(4))
        .child(label(copy::order_line(&order), Tone::Mute).truncate()),
    )
    .child(gap())
    .child(ui::rule(true))
    .child(gap())
    .child(items)
    .child(gap())
    .child(ui::rule(true))
    .child(gap())
    .child(
      view()
        .flex_row()
        .child(label("实付", Tone::Fg))
        .child(view().flex_1())
        .child(strong(format::yuan_cents(order.total), Tone::Acc)),
    )
    .child(gap())
    .child(label(copy::receipt_ship_to(&order), Tone::Soft).truncate())
    .child(strong(copy::DEMO_ORDER, Tone::Acc).truncate())
    .child(vspace(1))
    .child(
      view()
        .flex_row()
        .child(
          Button::new(ButtonKind::Primary, "⏎", "继续选购")
            .width(half)
            .on_press(move || press(&stores, "cart.continue")),
        )
        .child(hspace(1))
        .child(
          Button::new(ButtonKind::Default, "4", "查看订单")
            .width(half)
            .on_press(move || press(&stores, "nav.profile")),
        ),
    );
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .child(ui::card(
      Legend::new("订单回执 RECEIPT").tone(Tone::Acc2),
      cols,
      None,
      content,
    ))
}
