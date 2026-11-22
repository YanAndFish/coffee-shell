//! The SKU panel of the detail page (03-detail.md, "规格 SKU"): weight (sold-out weights
//! struck and skipped), grind, pack, quantity with the stock note, the server's total, the
//! SKU code and the two actions. At XL and L each group is a name row and an option row
//! ([`stacked`]); at M and S each group is one row, its name left of its options ([`inline`]).
//!
//! Every amount here is the server's: the weight notes are `PriceCell.unit_price` of that
//! weight with the pack chosen, the total `PriceCell.totals[quantity − 1]`.

use std::rc::Rc;

use coffee_api::{Grind, Pack, ProductDetail, Weight};
use nsh_core::prelude::*;

use super::super::format;
use crate::{
  commands::press,
  stores::{
    ProductStore, SkuGroup, SkuSelection, Stores,
    product::{max_quantity, weight_available},
  },
  ui::{
    Button, ButtonKind, ChipPad, StepperForm, Tone, chip, group_label, label, layout, rule,
    sku_option, stepper, stepper::STEPPER_CELLS, stock_note, strong, text::display_width, toned,
    toned_strong,
  },
};

/// The grind chips, in [`Grind::ALL`] order.
pub const GRIND_LABELS: [&str; 5] = ["整豆", "手冲", "意式", "法压", "冷萃"];
/// The pack chips, in [`Pack::ALL`] order (`+¥20` is copy; prices are the server's).
pub const PACK_LABELS: [&str; 2] = ["标准袋", "礼盒装 +¥20"];

/// A group's focus hint: `← → 增减` on the quantity, else `← → 切换`.
pub fn hint(group: SkuGroup) -> &'static str {
  match group {
    SkuGroup::Quantity => "← → 增减",
    _ => "← → 切换",
  }
}

/// The note of `weight` with `pack`: the server's unit price (`¥118`) or `缺货`.
pub fn price_note(detail: &ProductDetail, weight: Weight, pack: Pack) -> String {
  if !weight_available(detail, weight) {
    return "缺货".to_owned();
  }
  detail
    .price(weight, pack)
    .map(|cell| format::yuan(cell.unit_price))
    .unwrap_or_default()
}

/// `已选 250g · 整豆 · 标准袋`.
pub fn selected_line(selection: &SkuSelection) -> String {
  format!(
    "已选 {} · {} · {}",
    selection.weight.label(),
    selection.grind.label(),
    selection.pack.label()
  )
}

/// `1 × ¥118`: the quantity and the server's unit price.
pub fn unit_line(quantity: u8, unit: u32) -> String {
  format!("{quantity} × {}", format::yuan(unit))
}

/// Options one cell apart.
fn option_row(chips: impl IntoIterator<Item = View>) -> View {
  view()
    .flex_none()
    .flex_row()
    .gap(layout::gap_cols(1))
    .children(chips)
}

fn focused(product: ProductStore, group: SkuGroup) -> impl Fn() -> bool + Copy + 'static {
  move || product.selection().focus == group
}

/// The four weights: 8-cell chips with their notes under them (`inline` false), or padded
/// chips holding the note (`[ 100g ¥53 ]`, `inline`). The notes follow the pack chosen.
fn weights(product: ProductStore, detail: Rc<ProductDetail>, inline: bool) -> AnyElement {
  let pack = computed(move || product.selection().pack);
  dynamic(move || {
    let pack = pack.get();
    let detail = detail.clone();
    untracked(move || {
      option_row(Weight::ALL.into_iter().map(move |weight| {
        let available = weight_available(&detail, weight);
        let note = price_note(&detail, weight, pack);
        let selected = move || product.selection().weight == weight;
        let choose = move || product.select_weight(weight);
        if inline {
          chip(
            format!("{} {note}", weight.label()),
            ChipPad::Padded,
            selected,
            move || available,
            choose,
          )
        } else {
          sku_option(weight.label(), note, selected, available, choose)
        }
      }))
    })
  })
}

fn grinds(product: ProductStore) -> View {
  option_row(
    Grind::ALL
      .into_iter()
      .zip(GRIND_LABELS)
      .map(move |(grind, name)| {
        chip(
          name.to_owned(),
          ChipPad::Tight,
          move || product.selection().grind == grind,
          || true,
          move || product.select_grind(grind),
        )
      }),
  )
}

fn packs(product: ProductStore) -> View {
  option_row(
    Pack::ALL
      .into_iter()
      .zip(PACK_LABELS)
      .map(move |(pack, name)| {
        chip(
          name.to_owned(),
          ChipPad::Padded,
          move || product.selection().pack == pack,
          || true,
          move || product.select_pack(pack),
        )
      }),
  )
}

/// The stepper (its presses move the focus to the quantity) and the stock note, two cells
/// apart; the note goes under the stepper when `room` cells cannot hold both (L).
fn quantity(product: ProductStore, detail: &ProductDetail, room: usize) -> View {
  let max = u32::from(max_quantity(detail));
  let (note, tone) = stock_note(detail.stock);
  let step = move |delta: isize| {
    batch(|| {
      product.focus(SkuGroup::Quantity);
      product.step_quantity(delta);
    })
  };
  let stepper = stepper(
    StepperForm::Divided,
    move || u32::from(product.selection().quantity),
    1,
    move || max,
    move || step(-1),
    move || step(1),
  );
  if STEPPER_CELLS + 2 + display_width(&note) <= room {
    view()
      .flex_none()
      .flex_row()
      .child(stepper)
      .child(layout::hspace(2))
      .child(label(note, tone))
  } else {
    view()
      .flex_none()
      .child(view().flex_row().child(stepper))
      .child(label(note, tone))
  }
}

/// `¥118.00  1 × ¥118`: the server's total (bold `acc`) and unit price (`mute`).
pub fn total_row(product: ProductStore) -> Text {
  text("")
    .child(strong(
      move || SharedString::from(product.total().map(format::yuan_cents).unwrap_or_default()),
      Tone::Acc,
    ))
    .child(label(
      move || {
        let quantity = product.selection().quantity;
        SharedString::from(
          product
            .unit_price()
            .map(|unit| format!("  {}", unit_line(quantity, unit)))
            .unwrap_or_default(),
        )
      },
      Tone::Mute,
    ))
}

fn selected_text(product: ProductStore) -> Text {
  label(
    move || SharedString::from(selected_line(&product.selection())),
    Tone::Soft,
  )
}

fn sku_text(product: ProductStore) -> Text {
  label(
    move || {
      SharedString::from(
        product
          .sku()
          .map(|sku| format!("SKU {sku}"))
          .unwrap_or_default(),
      )
    },
    Tone::Mute,
  )
}

/// `a 加入购物车` (primary) and `b 立即购买` (outline), one cell apart, disabled while the
/// selection cannot be bought; stretched to `widths` when given.
pub fn buttons(stores: Stores, widths: Option<(u16, u16)>) -> View {
  let product = stores.product;
  let add = Button::new(ButtonKind::Primary, "a", "加入购物车")
    .disabled(move || !product.can_buy())
    .on_press(move || press(&stores, "detail.add"));
  let buy = Button::new(ButtonKind::Outline, "b", "立即购买")
    .disabled(move || !product.can_buy())
    .on_press(move || press(&stores, "detail.buy"));
  let (add, buy) = match widths {
    Some((a, b)) => (add.width(a), buy.width(b)),
    None => (add, buy),
  };
  view()
    .flex_none()
    .flex_row()
    .child(add)
    .child(layout::hspace(1))
    .child(buy)
}

/// The widths of `a 加入购物车` and `b 立即购买` filling `inner` cells with one between them, in
/// the design's 24 : 19 (24 + 1 + 19 = 44 at XL; 22 + 1 + 17 = 40 at L).
pub fn button_widths(inner: u16) -> (u16, u16) {
  let room = inner.saturating_sub(1);
  let buy = room * 19 / 43;
  (room - buy, buy)
}

/// An option row of the stacked panel: two cells in from the group's name.
fn options(content: impl IntoElement) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(layout::hspace(2))
    .child(content)
}

/// The SKU panel's content at XL and L, `inner` cells wide (44 at XL, 40 at L); the two
/// buttons fill it ([`button_widths`]).
pub fn stacked(stores: Stores, detail: Rc<ProductDetail>, inner: u16) -> View {
  let product = stores.product;
  let group = |group: SkuGroup| group_label(group.label(), hint(group), focused(product, group));
  view()
    .flex_1()
    .px(layout::pad_cols(1))
    .child(layout::vspace(1))
    .child(group(SkuGroup::Weight))
    .child(options(weights(product, detail.clone(), false)))
    .child(layout::vspace(1))
    .child(group(SkuGroup::Grind))
    .child(options(grinds(product)))
    .child(layout::vspace(1))
    .child(group(SkuGroup::Pack))
    .child(options(packs(product)))
    .child(layout::vspace(1))
    .child(group(SkuGroup::Quantity))
    .child(options(quantity(
      product,
      &detail,
      usize::from(inner).saturating_sub(2),
    )))
    .child(layout::vspace(1))
    .child(rule(false))
    .child(layout::vspace(1))
    .child(total_row(product))
    .child(selected_text(product))
    .child(sku_text(product))
    .child(view().flex_1().min_h(layout::rows(1)))
    .child(buttons(stores, Some(button_widths(inner))))
    .child(layout::vspace(1))
}

/// One group on one row (S, M): `› 重量  [options]          ← → 切换`. `handle` marks the
/// row, so the page can scroll it into view.
fn inline_group(
  product: ProductStore,
  group: SkuGroup,
  handle: ElementRef,
  content: impl IntoElement,
) -> View {
  let on = focused(product, group);
  view()
    .flex_none()
    .flex_row()
    .element_ref(&handle)
    .child(toned(move || if on() { "› " } else { "  " }, || Tone::Acc))
    .child(toned_strong(
      group.label(),
      move || if on() { Tone::Fg } else { Tone::Mute },
      on,
    ))
    .child(layout::hspace(2))
    .child(content)
    .child(view().flex_1().min_w(layout::cols(1)))
    .child(when(on, move || label(hint(group), Tone::Mute)))
}

/// Where the purchase line goes at S and M.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purchase {
  /// In the panel (S): the total with `已选`, then the buttons with the SKU code.
  InPanel,
  /// In a fixed row under the scrolled content (M): the panel ends with `已选` and the
  /// SKU code ([`purchase_row`] holds the total and the buttons).
  Pinned,
}

/// The SKU panel's content at M and S: one row per group (`refs`: their handles, in
/// [`SkuGroup::ALL`] order), the rule, and the purchase lines as `purchase` says.
pub fn inline(
  stores: Stores,
  detail: Rc<ProductDetail>,
  refs: [ElementRef; 4],
  purchase: Purchase,
) -> View {
  let product = stores.product;
  let tail = match purchase {
    Purchase::InPanel => view()
      .flex_none()
      .child(
        view()
          .flex_row()
          .child(total_row(product))
          .child(layout::hspace(2))
          .child(selected_text(product)),
      )
      .child(
        view()
          .flex_row()
          .child(buttons(stores, None))
          .child(view().flex_1().min_w(layout::cols(2)))
          .child(sku_text(product)),
      ),
    Purchase::Pinned => view()
      .flex_none()
      .flex_row()
      .child(selected_text(product))
      .child(view().flex_1().min_w(layout::cols(2)))
      .child(sku_text(product)),
  };
  view()
    .flex_none()
    .child(inline_group(
      product,
      SkuGroup::Weight,
      refs[0],
      weights(product, detail.clone(), true),
    ))
    .child(inline_group(
      product,
      SkuGroup::Grind,
      refs[1],
      grinds(product),
    ))
    .child(inline_group(
      product,
      SkuGroup::Pack,
      refs[2],
      packs(product),
    ))
    .child(inline_group(
      product,
      SkuGroup::Quantity,
      refs[3],
      quantity(product, &detail, usize::MAX),
    ))
    .child(rule(false))
    .child(tail)
}

/// The pinned purchase row at M: the total left, the two buttons right.
pub fn purchase_row(stores: Stores) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(layout::hspace(1))
    .child(total_row(stores.product))
    .child(view().flex_1().min_w(layout::cols(2)))
    .child(buttons(stores, None))
    .child(layout::hspace(1))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn detail(name: &str) -> ProductDetail {
    let body = match name {
      "pinkb" => {
        include_str!("../../../../../packages/contract/fixtures/v1/catalog/product-pinkb.json")
      }
      _ => include_str!("../../../../../packages/contract/fixtures/v1/catalog/product-geisha.json"),
    };
    serde_json::from_str(body).unwrap()
  }

  #[test]
  fn weight_notes_are_the_servers_unit_prices_with_the_pack() {
    let pinkb = detail("pinkb");
    assert_eq!(price_note(&pinkb, Weight::G100, Pack::Standard), "¥53");
    assert_eq!(price_note(&pinkb, Weight::G250, Pack::Standard), "¥118");
    assert_eq!(price_note(&pinkb, Weight::G500, Pack::Standard), "¥224");
    assert_eq!(price_note(&pinkb, Weight::Kg1, Pack::Standard), "缺货");
    assert_eq!(price_note(&pinkb, Weight::G250, Pack::Gift), "¥138");
    let geisha = detail("geisha");
    assert_eq!(price_note(&geisha, Weight::G100, Pack::Gift), "¥141");
    assert_eq!(price_note(&geisha, Weight::G500, Pack::Gift), "缺货");
  }

  #[test]
  fn lines_read_as_copy_md() {
    let selection = SkuSelection {
      weight: Weight::G100,
      grind: Grind::PourOver,
      pack: Pack::Gift,
      quantity: 2,
      focus: SkuGroup::Quantity,
    };
    assert_eq!(selected_line(&selection), "已选 100g · 手冲 · 礼盒装");
    assert_eq!(unit_line(2, 141), "2 × ¥141");
    assert_eq!(hint(SkuGroup::Quantity), "← → 增减");
    assert_eq!(hint(SkuGroup::Pack), "← → 切换");
    assert_eq!(
      GRIND_LABELS.to_vec(),
      Grind::ALL.map(|g| g.label()).to_vec()
    );
    assert_eq!(PACK_LABELS[0], Pack::Standard.label());
    assert!(PACK_LABELS[1].starts_with(Pack::Gift.label()));
  }

  #[test]
  fn the_buttons_fill_the_panel_as_the_design_does() {
    assert_eq!(button_widths(44), (24, 19));
    assert_eq!(button_widths(40), (22, 17));
    // Wide enough for their labels (`▐ a 加入购物车 ▌` 16, `[ b 立即购买 ]` 14).
    assert!(button_widths(40).0 >= 16 && button_widths(40).1 >= 14);
  }
}
