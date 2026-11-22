//! 详情 DETAIL (03-detail.md): one bean's profile, flavor and the SKU panel, over the list.
//!
//! Store: [`crate::stores::ProductStore`] — `id`, `phase`, `failure`, `detail`,
//! `selection`, `move_focus` / `focus` / `step` / `step_quantity` / `select_weight` /
//! `select_grind` / `select_pack` / `set_quantity`, `can_buy`, `unit_price`, `total` (the
//! server's `PriceCell.totals`), `sku`. Actions: `actions::add_selection(stores, buy_now)`,
//! `actions::back`. The route carries the product id; the page opens it on entry.
//!
//! The body is a placeholder (plain lines); the detail screen replaces it.

pub mod commands;

use coffee_api::{Grind, Pack, Weight, roast_name};
use nsh_core::{element::Screen, prelude::*};

use super::{
  PageModule, format,
  placeholder::{self, Line, phase_lines},
};
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{SkuGroup, Stores, product::weight_available},
  ui::Tone,
};

/// `详情 DETAIL`.
pub const MODE: Mode = Mode::new("详情", "DETAIL");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Detail,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

/// The stock line: `现货 {n} 份 · 每周二、五烘焙`, `仅剩 {n} 份 · 售完即止`, `暂时缺货`.
pub fn stock_line(stock: u32) -> String {
  match stock {
    0 => "暂时缺货".to_owned(),
    1..=10 => format!("仅剩 {stock} 份 · 售完即止"),
    _ => format!("现货 {stock} 份 · 每周二、五烘焙"),
  }
}

fn lines(stores: Stores) -> Vec<Line> {
  let product = stores.product;
  let mut lines = phase_lines(
    product.phase(),
    product.failure(),
    "商品",
    "",
    Some("detail.retry"),
  );
  let Some(detail) = product.detail() else {
    return lines;
  };
  let selection = product.selection();
  lines.push(Line::new(
    format!("{} · {}", detail.name, detail.subtitle),
    Tone::Fg,
  ));
  lines.push(Line::soft(format!(
    "{} / {} · {} · {} · 杯测 {} 分",
    detail.origin,
    detail.farm,
    detail.process,
    roast_name(detail.roast),
    detail.score
  )));
  lines.push(Line::new(
    stock_line(detail.stock),
    if detail.stock == 0 {
      Tone::Danger
    } else {
      Tone::Acc
    },
  ));
  let row = |group: SkuGroup, options: String| {
    let mark = if selection.focus == group { "›" } else { " " };
    Line::text(format!("{mark} {}  {options}", group.label()))
  };
  let pick = |chosen: bool, text: String| {
    if chosen { format!("[{text}]") } else { text }
  };
  let weights = Weight::ALL
    .iter()
    .map(|w| {
      let note = if weight_available(&detail, *w) {
        detail
          .price(*w, selection.pack)
          .map(|cell| format::yuan(cell.unit_price))
          .unwrap_or_default()
      } else {
        "缺货".to_owned()
      };
      pick(*w == selection.weight, format!("{} {note}", w.label()))
    })
    .collect::<Vec<_>>()
    .join("  ");
  lines.push(row(SkuGroup::Weight, weights));
  let grinds = Grind::ALL
    .iter()
    .map(|g| pick(*g == selection.grind, g.label().to_owned()))
    .collect::<Vec<_>>()
    .join("  ");
  lines.push(row(SkuGroup::Grind, grinds));
  let packs = Pack::ALL
    .iter()
    .map(|p| pick(*p == selection.pack, p.label().to_owned()))
    .collect::<Vec<_>>()
    .join("  ");
  lines.push(row(SkuGroup::Pack, packs));
  lines.push(row(
    SkuGroup::Quantity,
    format!("− {} +", selection.quantity),
  ));
  if let (Some(total), Some(unit)) = (product.total(), product.unit_price()) {
    lines.push(Line::new(
      format!(
        "合计 {} · {} × {}",
        format::yuan_cents(total),
        selection.quantity,
        format::yuan(unit)
      ),
      Tone::Acc,
    ));
  }
  if let Some(sku) = product.sku() {
    lines.push(Line::soft(format!("SKU {sku}")));
  }
  if product.can_buy() {
    lines.push(Line::text("a 加入购物车").press("detail.add"));
    lines.push(Line::text("b 立即购买").press("detail.buy"));
  } else {
    lines.push(Line::soft("a 加入购物车 · b 立即购买（不可购买）"));
  }
  lines.push(Line::soft("← Esc 返回").press("detail.back"));
  lines
}

/// The detail page of the route's product (placeholder body).
pub fn render(route: &Route) -> Screen {
  let stores = Stores::get();
  let id = match route {
    Route::Detail(id) => id.clone(),
    _ => SharedString::default(),
  };
  let product = stores.product;
  // The page's work: show its product (a no-op when it is already the one shown).
  {
    let id = id.clone();
    effect(move || untracked(|| product.open(&id)));
  }
  placeholder::screen(
    "详情 DETAIL",
    move || {
      let name = product
        .detail()
        .map(|d| d.name)
        .unwrap_or_else(|| id.to_string());
      format!("详情 DETAIL · {name}")
    },
    move || lines(stores),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn stock_lines() {
    assert_eq!(stock_line(0), "暂时缺货");
    assert_eq!(stock_line(6), "仅剩 6 份 · 售完即止");
    assert_eq!(stock_line(42), "现货 42 份 · 每周二、五烘焙");
  }
}
