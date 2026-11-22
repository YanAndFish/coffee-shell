//! DataRow: table-style lists with one sliding `▶` cursor (`components/DataRow`): the
//! product list, the cart, the orders.
//!
//! ```text
//!      名称                           产区         处理法     烘焙     价格 /250g
//! ├────────────────────────────────────────────────────────────────────────────────┤
//!   ▶  耶加雪菲 孔加 [新产季]         埃塞俄比亚   水洗       ●○○○○           ¥98
//!      Ethiopia · Yirgacheffe Konga G1
//!
//!      慧兰 粉红波旁 [店长推荐]       哥伦比亚     水洗       ●●○○○          ¥118
//! ```
//!
//! The first column (two cells) holds `▶` (`acc`) on the current row only, and the current
//! row's text rows stand on the `selection` band (monochrome: reverse video; 16 colours: no
//! band, the glyph and the name's accent carry it). Columns are fixed cell widths or one
//! flexible column, a cell apart; amounts align right. A page drops columns as the terminal
//! narrows. A click puts the cursor on a row, a double click opens it (the caller decides).

use nsh_core::{element::PressEvent, prelude::*};

use super::{
  action::pressable,
  label::{label, strong, toned},
  layout,
  stepper::{StepperForm, stepper},
  theme::{Fill, Tone},
  toggle::check_glyph,
};

/// How wide a column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
  /// What is left: the name column.
  Flex,
  /// A fixed number of cells.
  Cells(u16),
}

/// Where a column's content sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
  /// Left (text).
  Start,
  /// Right (amounts).
  End,
}

/// A column of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
  /// Its head, `名称`.
  pub title: &'static str,
  /// Its width.
  pub width: Width,
  /// Its alignment.
  pub align: Align,
}

/// The cells of the cursor column.
const CURSOR_COLS: u16 = 2;

/// The roast pips: `value` filled dots of `max` (`●●○○○`), clamped.
pub fn pips(value: u8, max: u8) -> String {
  let value = value.min(max);
  let mut out = "●".repeat(usize::from(value));
  out.push_str(&"○".repeat(usize::from(max - value)));
  out
}

fn cell(column: Column, content: impl IntoElement) -> View {
  let cell = match column.width {
    Width::Flex => view().flex_1().min_w(px(0.0)),
    Width::Cells(n) => view().flex_none().w(layout::cols(n)),
  };
  match column.align {
    Align::Start => cell.items_start(),
    Align::End => cell.items_end(),
  }
  .child(content)
}

fn row_frame() -> View {
  view().flex_row().gap_2().px_2()
}

fn band(current: impl Fn() -> bool + 'static) -> impl Fn() -> AdaptiveColor + 'static {
  move || {
    if current() {
      Fill::Selection.color()
    } else {
      AdaptiveColor::TRANSPARENT
    }
  }
}

fn cursor(current: impl Fn() -> bool + 'static) -> View {
  view().flex_none().w(layout::cols(CURSOR_COLS)).child(
    toned(move || if current() { "▶" } else { "" }, || Tone::Acc).font_weight(FontWeight::BOLD),
  )
}

/// The head row of `columns`, in `mute`.
pub fn table_head(columns: &[Column]) -> View {
  row_frame()
    .flex_none()
    .child(view().flex_none().w(layout::cols(CURSOR_COLS)))
    .child(
      columns
        .iter()
        .map(|&column| cell(column, label(column.title, Tone::Mute)))
        .collect::<Vec<_>>(),
    )
}

/// The rule under a table head: `─` across the table (nsh joins its ends to a frame line it
/// touches, `├──┤`).
pub fn table_rule() -> View {
  super::frame::line_run('─', Tone::Line)
}

/// A row of `columns` holding `cells` (one element per column, in order). `current()` says
/// whether the cursor is on it. A click runs `on_press(clicks)`: 1 for a click, 2 for a
/// double click. The row is as tall as its tallest cell (one or two text rows); leave the
/// blank row between rows to the list (`gap`).
pub fn data_row(
  columns: &[Column],
  current: impl Fn() -> bool + Clone + 'static,
  cells: Vec<AnyElement>,
  on_press: impl Fn(u32) + 'static,
) -> View {
  // Pressable and banded at once: a pressable box lays out the same with or without its band
  // (a band that is no control would reach across the panel's padding and move the row).
  row_frame()
    .flex_none()
    .focusable(false)
    .on_press(move |event: &PressEvent| on_press(event.clicks))
    .bg(band(current.clone()))
    .child(cursor(current))
    .child(
      columns
        .iter()
        .zip(cells)
        .map(|(&column, content)| cell(column, content))
        .collect::<Vec<_>>(),
    )
}

/// A two-line name cell: the name (bold; `acc` on the current row) with an optional tag
/// after it, and the subtitle (`mute`, one line, cut with `…`) under it unless `short`.
pub fn name_cell(
  name: impl Into<SharedString>,
  tag: Option<AnyElement>,
  subtitle: Option<SharedString>,
  current: impl Fn() -> bool + 'static,
  dimmed: impl Fn() -> bool + 'static,
) -> AnyElement {
  let tone = move || {
    if dimmed() {
      Tone::Mute
    } else if current() {
      Tone::Acc
    } else {
      Tone::Fg
    }
  };
  view()
    .w_full()
    .child(
      view()
        .flex_row()
        .child(
          toned(name.into(), tone)
            .font_weight(FontWeight::BOLD)
            .truncate(),
        )
        .child(tag.map(|tag| view().flex_none().pl(layout::cols(1)).child(tag))),
    )
    .child(subtitle.map(|subtitle| label(subtitle, Tone::Mute).truncate()))
    .into_element()
}

/// One line of a cart row (all text values, already formatted by the caller).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CartLine {
  /// The product name, `慧兰 粉红波旁`.
  pub name: SharedString,
  /// Its origin and roast, `哥伦比亚 · 中浅烘`.
  pub subtitle: SharedString,
  /// The SKU, `250g · 手冲 · 标准袋`.
  pub spec: SharedString,
  /// The unit price, `¥118`.
  pub unit: SharedString,
  /// The line total, `¥118.00`.
  pub total: SharedString,
}

/// What a cart row's controls do (the same commands as the cart's keys).
pub struct CartActions {
  /// Ticks or unticks the line (`space`).
  pub toggle: Box<dyn Fn()>,
  /// One less (`−`).
  pub dec: Box<dyn Fn()>,
  /// One more (`+`).
  pub inc: Box<dyn Fn()>,
  /// Deletes the line (`x`).
  pub remove: Box<dyn Fn()>,
  /// A click on the row: `clicks` 1 or 2.
  pub press: Box<dyn Fn(u32)>,
}

/// The cart's column widths (`04-cart.md`, 103 content cells): check box 3 + 2, name flex,
/// spec 21, unit 9, stepper 13, total 12 (right), delete 3.
pub const CART_COLUMNS: [u16; 6] = [3, 21, 9, 13, 12, 3];

/// How much of a cart row a width class shows (`responsive.md`, 04).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CartForm {
  /// XL, L: the subtitle under the name, then spec and unit price columns.
  Wide,
  /// M: no unit price column; the spec takes the subtitle's row.
  Medium,
  /// S: the second row is "spec · unit price"; no spec or unit columns.
  Small,
}

impl CartForm {
  /// The form for `bp`.
  pub fn at(bp: super::layout::Bp) -> Self {
    use super::layout::Bp;
    match bp {
      Bp::Xl | Bp::L => CartForm::Wide,
      Bp::M => CartForm::Medium,
      Bp::S | Bp::TooSmall => CartForm::Small,
    }
  }

  /// The row under the name for `line`.
  pub fn second_row(self, line: &CartLine) -> SharedString {
    match self {
      CartForm::Wide => line.subtitle.clone(),
      CartForm::Medium => line.spec.clone(),
      CartForm::Small => format!("{} · {}", line.spec, line.unit).into(),
    }
  }
}

/// A cart row (two text rows) in `form`: `▶ [✓]  慧兰 粉红波旁   250g · 手冲 · 标准袋 ¥118
/// [ −   1   + ]  ¥118.00   ✕`, the second row under the name. `checked()` ticks it (an
/// unticked row's name and total turn `mute`); `quantity()` within `1..=max()`.
pub fn cart_row(
  line: CartLine,
  form: CartForm,
  current: impl Fn() -> bool + Clone + 'static,
  checked: impl Fn() -> bool + Copy + 'static,
  quantity: impl Fn() -> u32 + Copy + 'static,
  max: impl Fn() -> u32 + Copy + 'static,
  actions: CartActions,
) -> View {
  let CartActions {
    toggle,
    dec,
    inc,
    remove,
    press,
  } = actions;
  let dim = move || !checked();
  let fixed = |cells: u16| view().flex_none().w(layout::cols(cells));
  view()
    .flex_none()
    .flex_row()
    .px_2()
    .focusable(false)
    .on_press(move |event: &PressEvent| press(event.clicks))
    .bg(band(current.clone()))
    .child(cursor(current.clone()))
    .child(
      fixed(CART_COLUMNS[0] + 2).child(
        pressable().label("勾选").on_press(toggle).child(
          toned(
            move || check_glyph(checked()),
            move || {
              if checked() { Tone::Acc } else { Tone::Mute }
            },
          )
          .style_when(checked, |s| s.font_weight(FontWeight::BOLD)),
        ),
      ),
    )
    .child(view().flex_1().min_w(px(0.0)).child(name_cell(
      line.name.clone(),
      None,
      Some(form.second_row(&line)),
      current,
      dim,
    )))
    .child(
      (form == CartForm::Wide)
        .then(|| fixed(CART_COLUMNS[1]).child(label(line.spec.clone(), Tone::Soft).truncate())),
    )
    .child(
      (form == CartForm::Wide)
        .then(|| fixed(CART_COLUMNS[2]).child(label(line.unit.clone(), Tone::Soft))),
    )
    .child(fixed(CART_COLUMNS[3]).child(stepper(StepperForm::Plain, quantity, 1, max, dec, inc)))
    .child(
      fixed(CART_COLUMNS[4]).items_end().child(
        toned(line.total.clone(), move || {
          if dim() { Tone::Mute } else { Tone::Fg }
        })
        .font_weight(FontWeight::BOLD),
      ),
    )
    .child(
      fixed(CART_COLUMNS[5]).items_end().child(
        pressable()
          .label("删除")
          .on_press(remove)
          .child(label("✕", Tone::Mute)),
      ),
    )
}

/// An order's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrderStatus {
  /// `● 烘焙中` (`acc`).
  Roasting,
  /// `● 运输中` (`acc`).
  Shipping,
  /// `● 已签收` (`acc2`).
  Delivered,
}

impl OrderStatus {
  /// Its text with the dot.
  pub fn text(self) -> &'static str {
    match self {
      OrderStatus::Roasting => "● 烘焙中",
      OrderStatus::Shipping => "● 运输中",
      OrderStatus::Delivered => "● 已签收",
    }
  }

  fn tone(self) -> Tone {
    match self {
      OrderStatus::Delivered => Tone::Acc2,
      _ => Tone::Acc,
    }
  }
}

/// An order row (two rows, not selectable): number, date, items (cut with `…`), amount
/// (right) and status (right), then a `┈` rule (`line-row`). Column widths come from the
/// caller: number, date, amount, status (the items take the rest).
pub fn order_row(
  number: impl Into<SharedString>,
  date: impl Into<SharedString>,
  items: impl Into<SharedString>,
  amount: impl Into<SharedString>,
  status: OrderStatus,
  widths: [u16; 4],
) -> View {
  let fixed = |cells: u16| view().flex_none().w(layout::cols(cells));
  view()
    .flex_none()
    .child(
      view()
        .flex_row()
        .gap(layout::cols(1))
        .child(fixed(widths[0]).child(label(number.into(), Tone::Fg)))
        .child(fixed(widths[1]).child(label(date.into(), Tone::Mute)))
        .child(
          view()
            .flex_1()
            .min_w(px(0.0))
            .child(label(items.into(), Tone::Soft).truncate()),
        )
        .child(
          fixed(widths[2])
            .items_end()
            .child(strong(amount.into(), Tone::Fg)),
        )
        .child(
          fixed(widths[3])
            .items_end()
            .child(label(status.text(), status.tone())),
        ),
    )
    .child(super::frame::line_run('┈', Tone::LineRow))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pips_show_the_roast() {
    assert_eq!(pips(1, 5), "●○○○○");
    assert_eq!(pips(4, 5), "●●●●○");
    assert_eq!(pips(9, 5), "●●●●●");
    assert_eq!(pips(0, 5), "○○○○○");
  }

  #[test]
  fn statuses_lead_with_the_dot() {
    for status in [
      OrderStatus::Roasting,
      OrderStatus::Shipping,
      OrderStatus::Delivered,
    ] {
      assert!(status.text().starts_with("● "));
    }
    assert_eq!(OrderStatus::Delivered.tone(), Tone::Acc2);
  }
}
