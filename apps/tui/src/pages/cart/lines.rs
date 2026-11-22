//! The cart panel (`购物车 · {件数} 件`): the table head, the lines with their one sliding
//! cursor (the list scrolls, the cursor's line kept in view), the problem flags, select-all
//! and the undo bar; the empty cup; loading and failure.
//!
//! ```text
//! ╭─ 购物车 · 3 件 ─────────────────────────────────────────────────────╮
//! │       商品                     规格                 单价     数量      小计  │
//! ├──────────────────────────────────────────────────────────────────────┤
//! │▶ [✓]  慧兰 粉红波旁            250g · 手冲 · 标准袋 ¥118  [ −   1   + ]  ¥118.00   ✕ │
//! │       哥伦比亚 · 中浅烘                                                     │
//! ├──────────────────────────────────────────────────────────────────────┤
//! │  [✓] 全选 ‹a›  已选 3 件                         ╎ 已删除 … · u 撤销 ╎ │
//! ╰──────────────────────────────────────────────────────────────────────╯
//! ```

use coffee_api::CartLine;
use nsh_core::prelude::*;

use super::{Shape, copy};
use crate::{
  commands::press,
  stores::{Phase, Stores, actions},
  ui::{
    self, Button, ButtonKind, CartActions, CartForm, EmptyArt, Tone, checkbox, kbd, label,
    layout::{self, cols, hspace},
  },
};

/// What the panel's body shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
  Loading,
  Failed,
  Empty,
  Lines,
}

/// The panel.
pub fn panel(stores: Stores, shape: Shape) -> View {
  let cart = stores.cart;
  let body = computed(move || match (cart.phase(), cart.cart()) {
    (_, Some(data)) if !data.lines.is_empty() => Body::Lines,
    (Phase::Failed, None) => Body::Failed,
    (Phase::Idle | Phase::Loading, None) => Body::Loading,
    _ => Body::Empty,
  });
  let animate = move || stores.settings.animations();
  ui::panel(
    move || SharedString::from(format!("购物车 · {} 件", cart.item_count())),
    dynamic(move || {
      let body = body.get();
      untracked(|| match body {
        Body::Loading => ui::loading_live("正在载入…", animate).into_element(),
        Body::Failed => ui::error_state(
          "购物车加载失败",
          move || {
            SharedString::from(
              cart
                .failure()
                .map(|failure| failure.detail())
                .unwrap_or_default(),
            )
          },
          "⏎",
          "重试",
          move || press(&stores, "cart.checkout"),
        )
        .into_element(),
        Body::Empty => ui::empty_state_with(
          EmptyArt::Cup,
          "购物车是空的",
          None,
          Some(
            Button::new(ButtonKind::Primary, "1", "去挑豆子")
              .on_press(move || press(&stores, "nav.catalog")),
          ),
        )
        .into_element(),
        Body::Lines => table(stores, shape).into_element(),
      })
    }),
  )
}

/// The head, the scrolling lines and the foot.
fn table(stores: Stores, shape: Shape) -> View {
  let form = shape.form();
  let small = form == CartForm::Small;
  view()
    .flex_1()
    .min_h(px(0.0))
    .child((!shape.short).then(|| layout::vspace(1)))
    .child((!small).then(|| head(form)))
    .child((!small).then(ui::table_rule))
    .child(
      scroll()
        .flex_1()
        .min_h(px(0.0))
        .child(rows(stores, form, shape.name_cells())),
    )
    .child((!small).then(ui::table_rule))
    .child(foot(stores, small))
}

/// The head row, on the cart row's columns (`ui::data_row::CART_COLUMNS`).
fn head(form: CartForm) -> View {
  let fixed = |cells: u16| view().flex_none().w(cols(cells));
  let wide = form == CartForm::Wide;
  view()
    .flex_none()
    .flex_row()
    .px_2()
    .child(fixed(2))
    .child(fixed(5))
    .child(
      view()
        .flex_1()
        .min_w(px(0.0))
        .child(label("商品", Tone::Mute)),
    )
    .child(wide.then(|| fixed(21).child(label("规格", Tone::Mute))))
    .child(wide.then(|| fixed(9).child(label("单价", Tone::Mute))))
    .child(fixed(13).child(label("数量", Tone::Mute)))
    .child(fixed(12).items_end().child(label("小计", Tone::Mute)))
    .child(fixed(3))
}

/// The lines: rebuilt when the server's cart changes (each line's numbers are its own),
/// never when only the cursor moves.
fn rows(stores: Stores, form: CartForm, name_cells: u16) -> AnyElement {
  let cart = stores.cart;
  let lines = computed(move || cart.lines());
  dynamic(move || {
    let lines = lines.get();
    let spaced = form != CartForm::Small;
    untracked(|| {
      view()
        .flex_none()
        .children(
          lines
            .into_iter()
            .map(|line| row(stores, form, name_cells, spaced, line))
            .collect::<Vec<_>>(),
        )
        .into_element()
    })
  })
}

fn row(stores: Stores, form: CartForm, name_cells: u16, spaced: bool, line: CartLine) -> View {
  let cart = stores.cart;
  let sku = line.sku.clone();
  let current = {
    let sku = sku.clone();
    move || cart.is_cursor(&sku)
  };
  // The cursor's line stays in view: the list scrolls to it when the cursor arrives.
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
  let at = move |sku: &str| {
    let sku = sku.to_owned();
    move || cart.select_line(&sku)
  };
  let on = |sku: &str, id: &'static str| {
    let select = at(sku);
    Box::new(move || {
      if !untracked(|| stores.busy()) {
        batch(&select);
        press(&stores, id);
      }
    }) as Box<dyn Fn()>
  };
  let dec = {
    let select = at(&sku);
    Box::new(move || {
      if !untracked(|| stores.busy()) {
        batch(|| {
          select();
          actions::step_line(&stores, -1);
        });
      }
    }) as Box<dyn Fn()>
  };
  let selected = line.selected;
  let quantity = u32::from(line.quantity);
  let max = copy::max_quantity(&line);
  let problem = copy::problem(&line);
  let actions = CartActions {
    toggle: on(&sku, "cart.toggle"),
    dec,
    inc: on(&sku, "cart.quantity"),
    remove: on(&sku, "cart.remove"),
    press: {
      let select = at(&sku);
      Box::new(move |_: u32| {
        if !untracked(|| stores.busy()) {
          select();
        }
      })
    },
  };
  let shown = copy::row(&line, form, usize::from(name_cells));
  let cells = ui::cart_row(
    shown.clone(),
    form,
    current,
    move || selected,
    move || quantity,
    move || max,
    actions,
  );
  // The medium form's second row when the name column is too narrow for it: across the row,
  // over the row's own (empty) second line.
  let across = (form == CartForm::Medium && shown.spec.is_empty()).then(|| {
    view()
      .absolute()
      .top(layout::rows(1))
      .left(px(0.0))
      .w_full()
      .flex_row()
      .px_2()
      .child(hspace(7))
      .child(label(copy::detail(&line), Tone::Mute).truncate())
  });
  // The row's third line: a problem's flag, or the blank between rows.
  let flag = problem.map(|problem| {
    view()
      .flex_none()
      .flex_row()
      .px_2()
      .child(hspace(7))
      .child(label(problem, Tone::Danger).truncate())
  });
  let gap = (flag.is_none() && spaced).then(|| layout::vspace(1));
  view()
    .flex_none()
    .element_ref(&handle)
    .child(cells)
    .child(across)
    .child(flag)
    .child(gap)
}

/// `[✓] 全选 ‹a›  已选 3 件` and, after a delete, the undo bar right.
fn foot(stores: Stores, small: bool) -> View {
  let cart = stores.cart;
  let all = computed(move || copy::all_selected(&cart.lines()));
  let count = move || {
    let n = cart.selected_count();
    SharedString::from(if small {
      format!("· {}", copy::selected(n))
    } else {
      copy::selected(n)
    })
  };
  view()
    .flex_none()
    .flex_row()
    .px_2()
    .child(hspace(2))
    .child(checkbox(
      "全选",
      move || all.get(),
      || false,
      || false,
      move || press(&stores, "cart.all"),
    ))
    .child(hspace(1))
    .child(kbd("a"))
    .child(hspace(if small { 1 } else { 2 }))
    .child(label(count, Tone::Soft))
    .child(view().flex_1())
    .child(dynamic(move || {
      cart.undo().map(|removed| {
        Button::new(ButtonKind::Undo, "", copy::undo(&removed))
          .on_press(move || press(&stores, "cart.undo"))
      })
    }))
}
