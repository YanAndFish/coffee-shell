//! Pages phase 2 builds: each says so plainly and offers the way back to the beans.

use nsh_core::{element::Screen, prelude::*};

use crate::{
  commands::press,
  stores::Stores,
  ui::{self, EmptyAction},
};

/// The placeholder of a tab: its panel `legend` and what will live there (`what`).
pub fn tab_page(legend: &'static str, what: &'static str) -> Screen {
  let stores = Stores::get();
  ui::page(
    legend,
    ui::panel(
      legend,
      ui::empty_state(
        "∅",
        format!("{what}：第二阶段实现"),
        Some("这一阶段只有选购页可用".into()),
        Some(EmptyAction {
          key: "1",
          label: "去挑豆子",
          run: Box::new(move || press(&stores, "nav.catalog")),
        }),
      ),
    ),
  )
}

/// The placeholder of product `id`'s detail page.
pub fn detail_page(id: SharedString) -> Screen {
  let stores = Stores::get();
  let catalog = stores.catalog;
  let name = {
    let id = id.clone();
    move || {
      catalog
        .product(&id)
        .map(|product| SharedString::from(product.name))
        .unwrap_or_else(|| id.clone())
    }
  };
  ui::page(
    "详情 DETAIL",
    ui::panel(
      move || SharedString::from(format!("详情 DETAIL · {}", name())),
      ui::empty_state(
        "◆",
        "商品详情：第二阶段实现",
        catalog.product(&id).map(|product| product.subtitle.into()),
        Some(EmptyAction {
          key: "Esc",
          label: "返回",
          run: Box::new(move || press(&stores, "detail.back")),
        }),
      ),
    ),
  )
}
