//! The `预览 PREVIEW` panel: the bean under the cursor — name, subtitle, flavour tags, the
//! flavour radar (six meters on short terminals), the summary line, the description, the
//! price-from line and the two actions. Only the title while nothing is listed.

use coffee_api::ProductSummary;
use nsh_core::prelude::*;

use super::{
  form::{Body, Form, Shape, flavour, preview_summary},
  list::body_now,
};
use crate::{
  commands::press,
  pages::format,
  stores::Stores,
  ui::{Button, ButtonKind, TagKind, Tone, label, layout, panel, radar, radar_label, strong, tag},
};

/// The two buttons: `▐ ⏎ 查看详情 ▌` and `[ a 快速加入 ]`, a cell apart (17 + 1 + 18 at XL,
/// their natural widths in the narrower panels). They press the keys' commands.
fn buttons(stores: Stores, form: Form) -> View {
  let open = Button::new(ButtonKind::Primary, "⏎", "查看详情")
    .on_press(move || press(&stores, "catalog.open"));
  let add = Button::new(ButtonKind::Default, "a", "快速加入")
    .on_press(move || press(&stores, "catalog.add"));
  let (open, add) = if form == Form::Xl {
    (open.width(17), add.width(18))
  } else {
    (open, add)
  };
  view()
    .flex_none()
    .flex_row()
    .gap(layout::gap_cols(1))
    .child(open)
    .child(add)
}

/// The preview of `product` in `shape`.
fn content(stores: Stores, shape: Shape, product: ProductSummary) -> View {
  let values = flavour(&product);
  let radar_text = radar_label(&product.name, values);
  let fits = shape.radar();
  let notes = view()
    .flex_none()
    .flex_row()
    .gap(layout::gap_cols(2))
    .children(
      product
        .notes
        .iter()
        .map(|note| tag(TagKind::Note, note.clone())),
    );
  let description = label(product.description.clone(), Tone::Soft);
  let description = match shape.description_lines() {
    0 => description,
    lines => description.number_of_lines(lines),
  };
  view()
    .flex_1()
    .min_h(px(0.0))
    .child((!shape.short).then(|| layout::vspace(1)))
    .child(strong(product.name.clone(), Tone::Fg).truncate())
    .child(label(product.subtitle.clone(), Tone::Mute).truncate())
    .child(layout::vspace(1))
    .child(notes)
    .child(layout::vspace(1))
    .child(view().flex_none().items_center().child(radar(
      move || values,
      Tone::Acc,
      move || fits,
      radar_text,
    )))
    .child(layout::vspace(1))
    .child(
      view()
        .flex_none()
        .items_center()
        .child(label(preview_summary(&product), Tone::Mute)),
    )
    .child(layout::vspace(1))
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .child(description),
    )
    .child(
      text("")
        .child(strong(format::yuan(product.price250), Tone::Acc))
        .child(label(" / 250g 起", Tone::Mute)),
    )
    .child(layout::vspace(1))
    .child(buttons(stores, shape.form))
    .child((!shape.short).then(|| layout::vspace(1)))
}

/// The preview panel for `shape`: the cursor's bean while the rows show, else only its
/// title.
pub fn preview_panel(stores: Stores, shape: Shape) -> View {
  let catalog = stores.catalog;
  panel(
    "预览 PREVIEW",
    view()
      .flex_1()
      .min_h(px(0.0))
      .flex_row()
      .child(layout::hspace(1))
      .child(view().flex_1().min_w(px(0.0)).child(dynamic(move || {
        (body_now(stores) == Body::Rows)
          .then(|| catalog.current())
          .flatten()
          .map(|product| content(stores, shape, product))
      })))
      .child(layout::hspace(1)),
  )
}
