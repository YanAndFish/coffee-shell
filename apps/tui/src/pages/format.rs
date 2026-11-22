//! Numbers and dates as copy.md writes them ("数字格式"). Formatting only: every amount is
//! the server's, never computed here.

use coffee_api::{OrderStatus, Weight};

/// `¥98`: list prices, unit prices.
pub fn yuan(amount: u32) -> String {
  format!("¥{amount}")
}

/// `¥426.00`: totals, subtotals, receipts, orders.
pub fn yuan_cents(amount: u32) -> String {
  format!("¥{amount}.00")
}

/// `−¥10.00` (U+2212): a discount.
pub fn discount(amount: u32) -> String {
  format!("−¥{amount}.00")
}

/// `¥12.00` or `包邮`: shipping.
pub fn shipping(amount: u32) -> String {
  if amount == 0 {
    "包邮".to_owned()
  } else {
    yuan_cents(amount)
  }
}

/// `1,280`: thousands separators.
pub fn thousands(value: u32) -> String {
  let digits = value.to_string();
  let mut out = String::new();
  for (i, c) in digits.chars().enumerate() {
    if i > 0 && (digits.len() - i).is_multiple_of(3) {
      out.push(',');
    }
    out.push(c);
  }
  out
}

/// ` × 2`'s number part: `× 2` (toasts, receipts).
pub fn times(quantity: u8) -> String {
  format!("× {quantity}")
}

/// `250g ×1`: an order summary's line.
pub fn order_line(weight: Weight, quantity: u8) -> String {
  format!("{} ×{quantity}", weight.label())
}

/// `11-18` from a shop date `2026-11-18`.
pub fn month_day(date: &str) -> String {
  date.get(5..10).unwrap_or(date).to_owned()
}

/// `11 月 25 日` from a shop date `2026-11-25` (no leading zeros).
pub fn month_day_zh(date: &str) -> String {
  let mut parts = date.split('-').skip(1).map(|p| p.trim_start_matches('0'));
  match (parts.next(), parts.next()) {
    (Some(month), Some(day)) => format!("{month} 月 {day} 日"),
    _ => date.to_owned(),
  }
}

/// `● 烘焙中`.
pub fn order_status(status: OrderStatus) -> String {
  format!("● {}", status.label())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formats_follow_copy_md() {
    assert_eq!(yuan(98), "¥98");
    assert_eq!(yuan_cents(426), "¥426.00");
    assert_eq!(discount(10), "−¥10.00");
    assert_eq!(shipping(0), "包邮");
    assert_eq!(shipping(12), "¥12.00");
    assert_eq!(thousands(1280), "1,280");
    assert_eq!(thousands(2000), "2,000");
    assert_eq!(thousands(1234567), "1,234,567");
    assert_eq!(thousands(12), "12");
    assert_eq!(times(1), "× 1");
    assert_eq!(order_line(Weight::G250, 1), "250g ×1");
    assert_eq!(month_day("2026-11-18"), "11-18");
    assert_eq!(month_day_zh("2026-11-05"), "11 月 5 日");
    assert_eq!(order_status(OrderStatus::Roasting), "● 烘焙中");
  }
}
