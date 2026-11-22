//! 地址本 ADDRESS (05-address.md): the address cards, the order address, the default, and
//! the new / edit dialog (`编辑 INSERT`, [`dialog`]). Guests see a card asking them to log in.
//!
//! Store: [`crate::stores::AddressStore`] — `phase`, `failure`, `items`, `count`,
//! `address`, `order_address`, `default_address`, `cursor` / `cursor_address` /
//! `move_cursor` / `select`, `ensure_loaded` / `load`, and the dialog's `dialog` / `edit` /
//! `saving` / `dialog_focus`. Actions: `actions::use_address`, `set_default_address`,
//! `delete_address`, `new_address`, `edit_address`, `save_address`, `cancel_address`,
//! `go_login`. The dialog's fields report their focus as `address.<field>` ([`FIELDS`]).
//!
//! Layout ([`grid`]): from L two columns of 11-row cards, at S and M one column of compact
//! cards (the cursor's card 5 rows with its actions, the others 4, the add card 3). The
//! title rows sit above (one row at S). The grid shows the rows that fit, the cursor's
//! always among them. Every button runs its command through the command table.

pub mod commands;
pub mod dialog;
pub mod grid;

use std::{cell::Cell, ops::Range, rc::Rc};

use coffee_api::Address;
use nsh_core::{element::Screen, prelude::*};

use self::grid::Shape;
use super::{Dialog, PageModule};
use crate::{
  commands::{self as table, Mode},
  router::{Page, Route},
  stores::{AddressCursor, Overlay, Phase, Stores},
  ui::{self, AddressView, Bp, Button, ButtonKind, FrameStyle, Layout, Legend, Tone, layout},
};

/// `地址本 ADDRESS`.
pub const MODE: Mode = Mode::new("地址本", "ADDRESS");

/// The dialog's text fields: (focus name, label).
pub const FIELDS: [(&str, &str); 4] = [
  ("address.recipient", "收货人"),
  ("address.phone", "手机号"),
  ("address.region", "所在地区"),
  ("address.detail", "详细地址"),
];

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Addresses,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[Dialog {
    overlay: Overlay::AddressDialog,
    view: dialog::dialog,
  }],
};

/// The intro before the order address (copy.md).
const INTRO: &str = "设置默认收货地址，或为本单单独指定一个。当前寄往：";

/// `{姓名} · {地区}` of the order address, or `未指定` (tracked).
fn shipping_to(stores: Stores) -> SharedString {
  stores
    .addresses
    .order_address()
    .map(|a| format!("{} · {}", a.recipient, a.region))
    .unwrap_or_else(|| "未指定".to_owned())
    .into()
}

/// Runs command `id` as its key would, the cursor first put on `cursor`.
fn press_on(stores: Stores, cursor: AddressCursor, id: &'static str) {
  batch(|| {
    stores.addresses.select(cursor);
    table::press(&stores, id);
  });
}

/// The address book.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let book = stores.addresses;
  let session = stores.session;
  // The page's work: a member's book, once.
  effect(move || {
    if session.is_member() {
      untracked(|| book.ensure_loaded());
    }
  });
  let member = computed(move || session.is_member());
  ui::page(
    "地址本 ADDRESS",
    view().flex_1().min_h(px(0.0)).child(dynamic(move || {
      if member.get() {
        member_view(stores).into_element()
      } else {
        guest_view(stores).into_element()
      }
    })),
  )
}

// --- The guest's card -------------------------------------------------------------------

/// The guest card's size (05-address.md, "访客").
const GUEST_COLS: u16 = 57;
const GUEST_ROWS: u16 = 11;

fn guest_view(stores: Stores) -> View {
  view()
    .flex_1()
    .items_center()
    .justify_center()
    .child(ui::card(
      Legend::new("地址本 ADDRESS"),
      GUEST_COLS,
      Some(GUEST_ROWS),
      view()
        .child(ui::label("你正在以访客身份浏览", Tone::Fg))
        .child(layout::vspace(1))
        .child(ui::label(
          "登录后可以管理地址本，并为订单选择收货地址。",
          Tone::Mute,
        ))
        .child(layout::vspace(2))
        .child(
          view().flex_row().child(
            Button::new(ButtonKind::Primary, "⏎", "去登录")
              .on_press(move || table::press(&stores, "addresses.login")),
          ),
        ),
    ))
}

// --- A member's book --------------------------------------------------------------------

fn member_view(stores: Stores) -> View {
  let layout = Layout::get();
  let narrow = computed(move || layout.bp() <= Bp::S);
  view()
    .flex_1()
    .min_h(px(0.0))
    .child(dynamic(move || {
      if narrow.get() {
        head_s(stores)
      } else {
        head(stores)
      }
    }))
    .child(body(stores))
}

/// `地址本 · {n} 条` (bold title, `mute` count), with `extra` after it in `mute`.
fn title(stores: Stores, extra: &'static str) -> Text {
  let book = stores.addresses;
  text("")
    .child(ui::strong("地址本", Tone::Fg))
    .child(ui::label(
      move || SharedString::from(format!(" · {} 条{extra}", book.count())),
      Tone::Mute,
    ))
}

fn new_button(stores: Stores) -> Button {
  Button::new(ButtonKind::Primary, "n", "新增地址")
    .on_press(move || table::press(&stores, "addresses.new"))
}

/// XL / L / M: the title; the intro with the order address and `▐ n 新增地址 ▌`; a blank row.
fn head(stores: Stores) -> AnyElement {
  view()
    .flex_none()
    .child(title(stores, ""))
    .child(
      view()
        .flex_row()
        .child(
          view().flex_1().min_w(px(0.0)).child(
            text("")
              .child(ui::label(INTRO, Tone::Mute))
              .child(ui::label(move || shipping_to(stores), Tone::Fg))
              .truncate(),
          ),
        )
        .child(new_button(stores)),
    )
    .child(layout::vspace(1))
    .into_element()
}

/// S: one row, ` 地址本 · 3 条 · 当前寄往：林晓 · 上海市 徐汇区 … ▐ n 新增地址 ▌ `.
fn head_s(stores: Stores) -> AnyElement {
  view()
    .flex_none()
    .flex_row()
    .child(layout::hspace(1))
    .child(
      view().flex_1().min_w(px(0.0)).child(
        title(stores, " · 当前寄往：")
          .child(ui::label(move || shipping_to(stores), Tone::Fg))
          .truncate(),
      ),
    )
    .child(new_button(stores))
    .child(layout::hspace(1))
    .into_element()
}

/// What the grid's place shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
  Loading,
  Failed,
  Grid,
}

fn body(stores: Stores) -> View {
  let book = stores.addresses;
  let settings = stores.settings;
  let state = computed(move || match book.phase() {
    Phase::Failed => Body::Failed,
    phase if phase.pending() && book.count() == 0 => Body::Loading,
    _ => Body::Grid,
  });
  view()
    .flex_1()
    .min_h(px(0.0))
    .child(dynamic(move || match state.get() {
      Body::Loading => {
        ui::loading_live("正在载入地址…", move || settings.animations()).into_element()
      }
      Body::Failed => ui::error_state(
        "地址加载失败",
        move || {
          SharedString::from(
            book
              .failure()
              .map(|failure| failure.message())
              .unwrap_or_default(),
          )
        },
        "⏎",
        "重试",
        move || table::press(&stores, "addresses.retry"),
      )
      .into_element(),
      Body::Grid => grid_view(stores).into_element(),
    }))
}

// --- The grid ---------------------------------------------------------------------------

/// One cell of the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CellModel {
  /// An address.
  Card {
    id: u32,
    view: AddressView,
    current: bool,
  },
  /// `+ 新增收货地址`.
  Add { current: bool },
}

impl CellModel {
  fn current(&self) -> bool {
    match self {
      CellModel::Card { current, .. } | CellModel::Add { current } => *current,
    }
  }
}

/// What the grid draws now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GridModel {
  shape: Shape,
  rows: Vec<Vec<CellModel>>,
  shown: Range<usize>,
}

/// The card's view of `address`; `for_order` when it is the order address.
fn card_view(address: &Address, for_order: bool) -> AddressView {
  let phone = address
    .phone_masked
    .clone()
    .unwrap_or_else(|| ui::mask_phone(address.phone.as_deref().unwrap_or("")));
  AddressView {
    name: address.recipient.clone().into(),
    phone: phone.into(),
    tag: Some(address.tag.label().into()),
    region: address.region.clone().into(),
    line: address.detail.clone().into(),
    default: address.is_default,
    for_order,
  }
}

/// The grid's model (tracked) in `area` (the cells measured, `(0, 0)` before the first
/// layout); `top` is the first row shown last time.
fn grid_model(stores: Stores, bp: Bp, area: (u16, u16), cols: u16, top: &Cell<usize>) -> GridModel {
  let book = stores.addresses;
  let items = book.items();
  let order = book.order_address().map(|a| a.id);
  let cursor = book.cursor();
  // Before the first layout, the content's width from the margins the frame gives it.
  let avail = if area.0 > 0 {
    area.0
  } else {
    cols.min(layout::MAX_CONTENT_COLS) - 2 * ui::content_margins(bp).0
  };
  let shape = grid::shape(bp, avail);
  let mut cells: Vec<CellModel> = items
    .iter()
    .map(|address| CellModel::Card {
      id: address.id,
      view: card_view(address, order == Some(address.id)),
      current: cursor == AddressCursor::Card(address.id),
    })
    .collect();
  cells.push(CellModel::Add {
    current: cursor == AddressCursor::New,
  });
  let at = cells.iter().position(CellModel::current).unwrap_or(0);
  let rows: Vec<Vec<CellModel>> = cells
    .chunks(shape.columns)
    .map(<[CellModel]>::to_vec)
    .collect();
  let heights: Vec<u16> = rows
    .iter()
    .map(|row| {
      row
        .iter()
        .map(|cell| grid::cell_rows(shape, cell.current(), matches!(cell, CellModel::Add { .. })))
        .max()
        .unwrap_or(0)
    })
    .collect();
  let shown = grid::window(
    &heights,
    grid::row_gap(shape),
    at / shape.columns,
    area.1,
    top.get(),
  );
  top.set(shown.start);
  GridModel { shape, rows, shown }
}

fn grid_view(stores: Stores) -> View {
  let layout = Layout::get();
  let area = signal((0u16, 0u16));
  let top = Rc::new(Cell::new(0usize));
  let model = computed(move || {
    grid_model(
      stores,
      layout.bp(),
      area.get(),
      layout.viewport().cols,
      &top,
    )
  });
  view()
    .flex_1()
    .min_h(px(0.0))
    .overflow_hidden()
    .on_layout(move |bounds: &Bounds<Points>| {
      let cells = (
        (bounds.size.width.0 / layout::CELL_WIDTH).floor() as u16,
        (bounds.size.height.0 / layout::CELL_HEIGHT).floor() as u16,
      );
      if area.get_untracked() != cells {
        area.set(cells);
      }
    })
    .child(dynamic(move || draw(stores, model.get())))
}

fn draw(stores: Stores, model: GridModel) -> AnyElement {
  let shape = model.shape;
  let gap = grid::row_gap(shape);
  let rows = model.rows[model.shown.clone()]
    .iter()
    .enumerate()
    .map(|(i, row)| {
      view()
        .flex_none()
        .child((i > 0 && gap > 0).then(|| layout::vspace(gap)))
        .child(
          view()
            .flex_none()
            .flex_row()
            .children(row.iter().enumerate().map(|(j, cell)| {
              view()
                .flex_none()
                .flex_row()
                .child((j > 0).then(|| layout::hspace(grid::COLUMN_GAP)))
                .child(cell_view(stores, shape, cell.clone()))
            })),
        )
    });
  view().flex_none().children(rows).into_element()
}

fn cell_view(stores: Stores, shape: Shape, cell: CellModel) -> AnyElement {
  match cell {
    CellModel::Card { id, view, current } => {
      let rows = grid::cell_rows(shape, current, false);
      let actions = if shape.is_grid() || current {
        card_actions(stores, id, view.default, shape.card_cols)
      } else {
        Vec::new()
      };
      let card = ui::address_card(
        view,
        shape.card_cols,
        rows,
        move || current,
        actions,
        move |clicks| {
          if clicks >= 2 {
            press_on(stores, AddressCursor::Card(id), "addresses.use");
          } else {
            stores.addresses.select(AddressCursor::Card(id));
          }
        },
      );
      // 鼠标追踪 on: the mouse over a card moves the cursor there (05-address.md).
      ui::track_hover(
        card,
        move || stores.settings.mouse_tracking(),
        move || {
          if !current {
            stores.addresses.select(AddressCursor::Card(id));
          }
        },
      )
      .into_element()
    }
    CellModel::Add { current } => add_view(stores, shape, current),
  }
}

/// `[ ⏎ 用于本单 ]  [ e 编辑 ]  [ space 设为默认 ]  [ d 删除 ]` of card `id` (`space` struck
/// through on the default card).
fn card_actions(stores: Stores, id: u32, default: bool, cols: u16) -> Vec<Button> {
  let run = move |command: &'static str| move || press_on(stores, AddressCursor::Card(id), command);
  vec![
    Button::new(ButtonKind::Default, "⏎", "用于本单").on_press(run("addresses.use")),
    Button::new(ButtonKind::Default, "e", "编辑").on_press(run("addresses.edit")),
    Button::new(ButtonKind::Default, "space", grid::default_label(cols))
      .disabled(move || default)
      .on_press(run("addresses.default")),
    Button::new(ButtonKind::Danger, "d", "删除").on_press(run("addresses.delete")),
  ]
}

/// The add card; under the cursor it carries `▶` where a card's is.
fn add_view(stores: Stores, shape: Shape, current: bool) -> AnyElement {
  let rows = grid::cell_rows(shape, current, true);
  let press = move || press_on(stores, AddressCursor::New, "addresses.new");
  if !current {
    return ui::track_hover(
      ui::add_card(shape.card_cols, rows, press),
      move || stores.settings.mouse_tracking(),
      move || stores.addresses.select(AddressCursor::New),
    )
    .into_element();
  }
  // `ui::add_card` has no cursor state (change request): the same dashed card, its line
  // amber and `▶` on the `+` row, as a card under the cursor carries it.
  let side = move |mark: bool| {
    view()
      .flex_1()
      .basis(px(0.0))
      .flex_row()
      .child(mark.then(|| ui::strong("▶", Tone::Acc)))
  };
  let plus = if rows >= 5 {
    view()
      .flex_none()
      .items_center()
      .child(ui::strong("+", Tone::Acc))
      .child(ui::label("新增收货地址", Tone::Mute))
  } else {
    view().flex_none().child(
      text("")
        .child(ui::strong("+ ", Tone::Acc))
        .child(ui::label("新增收货地址", Tone::Mute)),
    )
  };
  ui::pressable()
    .flex_none()
    .w(layout::cols(shape.card_cols))
    .h(layout::rows(rows))
    .label("新增收货地址")
    .on_press(press)
    .child(ui::frame(
      || FrameStyle::Dashed,
      || Tone::Acc,
      None,
      Some(rows),
      2,
      view().flex_1().justify_center().child(
        view()
          .flex_row()
          .items_center()
          .child(side(true))
          .child(plus)
          .child(side(false)),
      ),
    ))
    .into_element()
}

#[cfg(test)]
mod tests {
  use coffee_api::AddressTag;

  use super::*;

  #[test]
  fn cards_mask_the_phone_and_carry_their_tags() {
    let mut address = Address {
      id: 1,
      recipient: "林晓".into(),
      phone: Some("13800005521".into()),
      phone_masked: None,
      region: "上海市 徐汇区".into(),
      detail: "天平路 88 弄 3 号 602 室".into(),
      tag: AddressTag::Home,
      is_default: true,
    };
    let view = card_view(&address, true);
    assert_eq!(view.phone.as_ref(), "138 **** 5521");
    assert_eq!(view.tag.as_deref(), Some("家"));
    assert!(view.default && view.for_order);
    address.phone = None;
    address.tag = AddressTag::School;
    let view = card_view(&address, false);
    assert_eq!(view.phone.as_ref(), "—");
    assert_eq!(view.tag.as_deref(), Some("学校"));
    assert!(!view.for_order);
  }
}
