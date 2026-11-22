//! The cart's words (copy.md, "购物车、结算与回执"): pure functions from the server's cart
//! and order to the strings the screen shows. Formatting only: every amount is a field of
//! the answer, never computed here.

use coffee_api::{Address, CartLine, CartLineProblem, CartTotals, Order, OrderItem, roast_name};

use crate::{
  pages::format,
  stores::{CheckoutStep, cart::Removed},
  ui::{CartForm, text::display_width},
};

/// `哥伦比亚 · 中浅烘`: a line's subtitle.
pub fn subtitle(line: &CartLine) -> String {
  format!("{} · {}", line.origin, roast_name(line.roast))
}

/// `250g · 手冲 · 标准袋`: a SKU.
pub fn spec(weight: &str, grind: &str, pack: &str) -> String {
  format!("{weight} · {grind} · {pack}")
}

/// The SKU of a cart line.
pub fn line_spec(line: &CartLine) -> String {
  spec(line.weight.label(), line.grind.label(), line.pack.label())
}

/// `{产区} · {烘焙度} · {规格}`: the medium form's second row.
pub fn detail(line: &CartLine) -> String {
  format!("{} · {}", subtitle(line), line_spec(line))
}

/// A cart line as the row shows it in `form`, its name column `name_cells` wide: in the
/// medium form [`detail`] takes the subtitle's row (no spec column there); where it does not
/// fit the name column the row's spec is empty and the page draws [`detail`] across the
/// row instead (the SKU tells two lines of one bean apart and must not be cut).
pub fn row(line: &CartLine, form: CartForm, name_cells: usize) -> crate::ui::CartLine {
  let spec = match form {
    CartForm::Medium => {
      let full = detail(line);
      if display_width(&full) <= name_cells {
        full
      } else {
        String::new()
      }
    }
    CartForm::Wide | CartForm::Small => line_spec(line),
  };
  crate::ui::CartLine {
    name: line.name.clone().into(),
    subtitle: subtitle(line).into(),
    spec: spec.into(),
    unit: format::yuan(line.unit_price).into(),
    total: format::yuan_cents(line.line_total).into(),
  }
}

/// The flag of a problem line (P-cart-4): `! 该重量暂时缺货` (`sold_out`), `! 库存不足 ·
/// 仅剩 {n} 份` (`out_of_stock`, `n` the server's `maxQuantity`).
pub fn problem(line: &CartLine) -> Option<String> {
  match line.problem? {
    CartLineProblem::SoldOut => Some("! 该重量暂时缺货".to_owned()),
    CartLineProblem::OutOfStock if line.max_quantity == 0 => Some("! 库存不足".to_owned()),
    CartLineProblem::OutOfStock => Some(format!("! 库存不足 · 仅剩 {} 份", line.max_quantity)),
  }
}

/// The most a line's stepper goes to: the server's `maxQuantity` (at least 1).
pub fn max_quantity(line: &CartLine) -> u32 {
  u32::from(line.max_quantity.max(1))
}

/// Whether every line is selected (`[✓] 全选`); an empty cart is not.
pub fn all_selected(lines: &[CartLine]) -> bool {
  !lines.is_empty() && lines.iter().all(|l| l.selected)
}

/// `已选 {n} 件`.
pub fn selected(count: u32) -> String {
  format!("已选 {count} 件")
}

/// `已删除 {商品名} · u 撤销`: the undo bar.
pub fn undo(removed: &Removed) -> String {
  format!("已删除 {} · u 撤销", removed.name)
}

/// `c 结算 {n} 件`'s label (the key leads it on the button).
pub fn checkout_label(count: u32) -> String {
  format!("结算 {count} 件")
}

/// The free-shipping line: `满 ¥199 包邮` (nothing selected), `已满 ¥199 · 本单包邮`, or
/// `再买 ¥{差额} 即可包邮`, all from the server's `freeShipping`.
pub fn free_shipping(totals: &CartTotals) -> String {
  let free = &totals.free_shipping;
  if totals.subtotal == 0 {
    format!("满 {} 包邮", format::yuan(free.threshold))
  } else if free.reached {
    format!("已满 {} · 本单包邮", format::yuan(free.threshold))
  } else {
    format!("再买 {} 即可包邮", format::yuan(free.gap))
  }
}

/// The checkout block's percentage at `step` (the steps switch at 30 / 60 / 90 %; each shows
/// the middle of its stretch, as the mock's `锁定烘焙批次 45%`).
pub fn step_percent(step: CheckoutStep) -> f32 {
  match step {
    CheckoutStep::Verify => 15.0,
    CheckoutStep::Lock => 45.0,
    CheckoutStep::Create => 75.0,
    CheckoutStep::Done => 100.0,
  }
}

/// `{地区} {详细地址}`: one row of the address (M).
pub fn street(address: &Address) -> String {
  format!("{} {}", address.region, address.detail)
}

/// `寄往 林晓 138 **** 5521 · 上海市 徐汇区 天平路 88 弄 3 号 602 室`: the address in one
/// row (S).
pub fn ship_to_line(address: &Address) -> String {
  let name = match address.phone_masked.as_deref().filter(|p| !p.is_empty()) {
    Some(phone) => format!("{} {phone}", address.recipient),
    None => address.recipient.clone(),
  };
  format!("寄往 {name} · {}", street(address))
}

/// `商品小计 ¥426.00 · 优惠 −¥0.00 · 运费 包邮`: the amounts in one row (S).
pub fn amounts_line(totals: &CartTotals) -> String {
  format!(
    "商品小计 {} · 优惠 {} · 运费 {}",
    format::yuan_cents(totals.subtotal),
    format::discount(totals.discount),
    format::shipping(totals.shipping)
  )
}

/// `订单号 RT2611220417 · 预计 11 月 25 日 送达`.
pub fn order_line(order: &Order) -> String {
  format!(
    "订单号 {} · 预计 {} 送达",
    order.number,
    format::month_day_zh(&order.eta_date)
  )
}

/// `250g · 手冲 · 标准袋 × 1`: a receipt item's spec and quantity.
pub fn item_spec(item: &OrderItem) -> String {
  format!(
    "{} {}",
    spec(item.weight.label(), item.grind.label(), item.pack.label()),
    format::times(item.quantity)
  )
}

/// `寄往 林晓 · 上海市 徐汇区 天平路 88 弄 3 号 602 室`.
pub fn receipt_ship_to(order: &Order) -> String {
  format!(
    "寄往 {} · {} {}",
    order.address.recipient, order.address.region, order.address.detail
  )
}

/// The receipt's demo notice (D7).
pub const DEMO_ORDER: &str = "! 演示订单 · 没有真实扣款，也不会发货";

/// The address block's words without an address: a guest's, a member's without one.
pub const GUEST_ADDRESS: &str = "登录后选择收货地址";
/// A member without an address.
pub const NO_ADDRESS: &str = "还没有收货地址，去地址本添加一个";

#[cfg(test)]
mod tests {
  use coffee_api::{Cart, PlacedOrder};

  use super::*;

  fn cart(json: &str) -> Cart {
    serde_json::from_str(json).unwrap()
  }

  const CART: &str = include_str!("../../../../../packages/contract/fixtures/v1/cart/cart.json");
  const COUPON: &str =
    include_str!("../../../../../packages/contract/fixtures/v1/cart/cart-coupon.json");
  const NONE: &str =
    include_str!("../../../../../packages/contract/fixtures/v1/cart/none-selected.json");
  const PROBLEM: &str =
    include_str!("../../../../../packages/contract/fixtures/v1/cart/cart-problem.json");
  const PLACED: &str =
    include_str!("../../../../../packages/contract/fixtures/v1/orders/placed.json");

  #[test]
  fn rows_read_as_the_mock() {
    let cart = cart(CART);
    let line = &cart.lines[0];
    let wide = row(line, CartForm::Wide, 40);
    assert_eq!(wide.name.as_ref(), "慧兰 粉红波旁");
    assert_eq!(wide.subtitle.as_ref(), "哥伦比亚 · 中浅烘");
    assert_eq!(wide.spec.as_ref(), "250g · 手冲 · 标准袋");
    assert_eq!(
      (wide.unit.as_ref(), wide.total.as_ref()),
      ("¥118", "¥118.00")
    );
    assert_eq!(
      row(line, CartForm::Medium, 45).spec.as_ref(),
      "哥伦比亚 · 中浅烘 · 250g · 手冲 · 标准袋"
    );
    assert_eq!(row(line, CartForm::Medium, 18).spec.as_ref(), "");
    assert_eq!(
      CartForm::Small
        .second_row(&row(line, CartForm::Small, 40))
        .as_ref(),
      "250g · 手冲 · 标准袋 · ¥118"
    );
    assert!(all_selected(&cart.lines));
    assert!(!all_selected(&[]));
    assert_eq!(selected(3), "已选 3 件");
    assert_eq!(checkout_label(3), "结算 3 件");
    assert_eq!(problem(line), None);
  }

  #[test]
  fn problem_lines_say_why() {
    let cart = cart(PROBLEM);
    let line = &cart.lines[0];
    assert_eq!(problem(line).as_deref(), Some("! 库存不足 · 仅剩 2 份"));
    assert_eq!(max_quantity(line), 2);
    let mut sold_out = line.clone();
    sold_out.problem = Some(CartLineProblem::SoldOut);
    assert_eq!(problem(&sold_out).as_deref(), Some("! 该重量暂时缺货"));
  }

  #[test]
  fn free_shipping_follows_the_server() {
    assert_eq!(free_shipping(&cart(CART).totals), "已满 ¥199 · 本单包邮");
    assert_eq!(free_shipping(&cart(COUPON).totals), "再买 ¥111 即可包邮");
    assert_eq!(free_shipping(&cart(NONE).totals), "满 ¥199 包邮");
    assert_eq!(
      amounts_line(&cart(COUPON).totals),
      "商品小计 ¥98.00 · 优惠 −¥10.00 · 运费 ¥12.00"
    );
  }

  #[test]
  fn addresses_and_steps() {
    let cart = cart(CART);
    let address = cart.address.unwrap();
    assert_eq!(street(&address), "上海市 徐汇区 天平路 88 弄 3 号 602 室");
    assert_eq!(
      ship_to_line(&address),
      "寄往 林晓 138 **** 5521 · 上海市 徐汇区 天平路 88 弄 3 号 602 室"
    );
    assert!(step_percent(CheckoutStep::Verify) < step_percent(CheckoutStep::Lock));
    assert_eq!(step_percent(CheckoutStep::Lock), 45.0);
  }

  #[test]
  fn the_receipt_reads_as_the_mock() {
    let placed: PlacedOrder = serde_json::from_str(PLACED).unwrap();
    let order = placed.order;
    assert_eq!(
      order_line(&order),
      "订单号 RT2611220417 · 预计 11 月 25 日 送达"
    );
    assert_eq!(item_spec(&order.items[0]), "250g · 手冲 · 标准袋 × 1");
    assert_eq!(
      receipt_ship_to(&order),
      "寄往 林晓 · 上海市 徐汇区 天平路 88 弄 3 号 602 室"
    );
  }
}
