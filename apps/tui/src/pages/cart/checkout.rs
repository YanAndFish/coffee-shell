//! The `结算 CHECKOUT` panel: ship-to (or the guest / no-address prompts), the coupon field
//! with `[ 应用 ]` and its `✓` / `✗` line, the server's amounts, the free-shipping bar, the
//! total, the demo notice and `c 结算 {n} 件` (disabled with nothing selected; the progress
//! block while submitting). Two forms: the column of XL / L / M ([`panel`]) and the compact
//! block under the cart at S ([`compact`]).

use coffee_api::Address;
use nsh_core::prelude::*;

use super::{COUPON_FIELD, Shape, copy};
use crate::{
  commands::press,
  pages::format,
  stores::{CouponFeedback, Stores},
  ui::{
    self, Button, ButtonKind, InputForm, InputSpec, LabelPlace, Tone, label,
    layout::{hspace, vspace},
    strong, toned,
  },
};

/// Where the order goes, as the address block shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ShipTo {
  /// A guest: `登录后选择收货地址`.
  Guest,
  /// A member without an address: `还没有收货地址，去地址本添加一个`.
  Nowhere,
  /// The order's address.
  Address(Address),
}

fn ship_to(stores: Stores) -> Memo<ShipTo> {
  let cart = stores.cart;
  let session = stores.session;
  computed(move || {
    if !session.is_member() {
      ShipTo::Guest
    } else {
      match cart.cart().and_then(|c| c.address) {
        Some(address) => ShipTo::Address(address),
        None => ShipTo::Nowhere,
      }
    }
  })
}

/// `3 更换 →`: the address book.
fn change(stores: Stores) -> Button {
  Button::new(ButtonKind::Link, "3", "更换").on_press(move || press(&stores, "nav.addresses"))
}

/// The coupon field (its focus reported to the shell as [`COUPON_FIELD`]) with `[ 应用 ]` on
/// its middle row; the error line `✗ 优惠码无效或已过期` under it.
fn coupon(stores: Stores, place: LabelPlace) -> View {
  let cart = stores.cart;
  let ui_store = stores.ui;
  // A field that goes away with the focus in it gives the keys back.
  cleanup(move || ui_store.field_blurred(COUPON_FIELD));
  let focused = move || {
    ui_store.focused_field() == Some(COUPON_FIELD) || ui_store.focus_requested(COUPON_FIELD)
  };
  let handle = element_ref();
  let field = ui::input(
    InputSpec {
      place,
      element: Some(handle),
      // While the order is placed only `?` works (K6).
      disabled: Some(std::rc::Rc::new(move || cart.submitting())),
      uppercase: true,
      placeholder: "试试 TTY10".into(),
      ..InputSpec::new(InputForm::Plain, Some("优惠码"))
    },
    move || cart.coupon_input(),
    focused,
    move || match cart.coupon_feedback() {
      Some(CouponFeedback::Invalid(message)) => Some(message.into()),
      _ => None,
    },
    move |text| cart.set_coupon_input(&text),
    move |focus| {
      batch(|| {
        if focus {
          ui_store.field_focused(COUPON_FIELD, Some(handle));
          ui_store.focus_taken(COUPON_FIELD);
        } else {
          ui_store.field_blurred(COUPON_FIELD);
        }
      })
    },
    // ⏎ in the field is the command table's `cart.coupon` (`⏎ 应用`).
    || {},
  );
  let above = match place {
    LabelPlace::Above => 2,
    LabelPlace::OnFrame => 1,
  };
  view()
    .flex_none()
    .flex_row()
    .child(view().flex_1().min_w(px(0.0)).child(field))
    .child(hspace(1))
    .child(view().flex_none().child(vspace(above)).child(
      Button::new(ButtonKind::Default, "", "应用").on_press(move || {
        if !untracked(|| stores.busy()) {
          actions_apply(stores);
        }
      }),
    ))
}

fn actions_apply(stores: Stores) {
  batch(|| crate::stores::actions::apply_coupon(&stores));
}

/// `✓ 已使用 TTY10 · 全单 9 折` under the field once the server took the code.
fn applied(stores: Stores) -> AnyElement {
  let cart = stores.cart;
  dynamic(move || match cart.coupon_feedback() {
    Some(CouponFeedback::Applied(coupon)) => Some(label(
      format!("✓ 已使用 {} · {}", coupon.code, coupon.description),
      Tone::Acc2,
    )),
    _ => None,
  })
}

/// A `key ……… value` row.
fn amount_row(
  key: &'static str,
  value: impl Fn() -> String + 'static,
  tone: impl Fn() -> Tone + 'static,
) -> View {
  view()
    .flex_none()
    .flex_row()
    .child(label(key, Tone::Soft))
    .child(view().flex_1())
    .child(toned(move || SharedString::from(value()), tone))
}

/// The server's totals, or zeros before any answer.
fn totals(stores: Stores) -> Memo<coffee_api::CartTotals> {
  let cart = stores.cart;
  computed(move || {
    cart
      .cart()
      .map(|c| c.totals)
      .unwrap_or(coffee_api::CartTotals {
        item_count: 0,
        selected_count: 0,
        subtotal: 0,
        discount: 0,
        shipping: 0,
        total: 0,
        free_shipping: coffee_api::FreeShipping {
          threshold: 199,
          reached: false,
          progress: 0,
          gap: 199,
        },
      })
  })
}

/// `c 结算 {n} 件` (`length` cells wide, or natural), or the progress block while the order
/// is being placed.
fn submit(stores: Stores, length: u16, stretch: bool) -> AnyElement {
  let cart = stores.cart;
  let submitting = computed(move || cart.submitting());
  let count = computed(move || cart.selected_count());
  dynamic(move || {
    if submitting.get() {
      untracked(|| {
        ui::progress(
          move || SharedString::from(cart.checkout_step().map_or("", |step| step.label())),
          move || cart.checkout_step().map_or(0.0, copy::step_percent),
          usize::from(length),
          Tone::Acc2,
          move || stores.settings.animations(),
        )
        .w(ui::layout::cols(length))
        .into_element()
      })
    } else {
      let n = count.get();
      let button = Button::new(ButtonKind::Primary, "c", copy::checkout_label(n))
        .disabled(move || count.get() == 0)
        .on_press(move || press(&stores, "cart.checkout"));
      if stretch {
        button.width(length).into_element()
      } else {
        button.into_element()
      }
    }
  })
}

/// The address inset: `收货地址` / `3 更换 →`, then the name and phone, the region and the
/// street (`two_rows`: region and street on one row, cut with `…`).
fn address(stores: Stores, one_row: bool) -> View {
  let ship = ship_to(stores);
  ui::inset(
    view()
      .flex_none()
      .child(
        view()
          .flex_row()
          .child(label("收货地址", Tone::Mute))
          .child(view().flex_1())
          .child(change(stores)),
      )
      .child(dynamic(move || {
        let ship = ship.get();
        untracked(|| match ship {
          ShipTo::Guest => label(copy::GUEST_ADDRESS, Tone::Mute).into_element(),
          ShipTo::Nowhere => strong(copy::NO_ADDRESS, Tone::Danger)
            .truncate()
            .into_element(),
          ShipTo::Address(address) => {
            let phone = address.phone_masked.clone().unwrap_or_default();
            let who = view()
              .flex_row()
              .child(strong(address.recipient.clone(), Tone::Fg))
              .child((!phone.is_empty()).then(|| label(format!("  {phone}"), Tone::Mute)));
            let place = if one_row {
              view().child(label(copy::street(&address), Tone::Soft).truncate())
            } else {
              view()
                .child(label(address.region.clone(), Tone::Soft).truncate())
                .child(label(address.detail.clone(), Tone::Fg).truncate())
            };
            view().child(who).child(place).into_element()
          }
        })
      })),
  )
}

/// The column form (XL 45, L 40, M 36 columns): `inner` is its content width in cells.
pub fn panel(stores: Stores, shape: Shape) -> View {
  let totals = totals(stores);
  let short = shape.short;
  let gap = move || (!short).then(|| vspace(1));
  let inner = shape.checkout_inner();
  let one_row = shape.bp == ui::Bp::M;
  let place = if short {
    LabelPlace::OnFrame
  } else {
    LabelPlace::Above
  };
  ui::panel(
    "结算 CHECKOUT",
    view()
      .flex_1()
      .min_h(px(0.0))
      .px(ui::layout::cols(1))
      .child(gap())
      .child(address(stores, one_row))
      .child(gap())
      .child(coupon(stores, place))
      .child(applied(stores))
      .child(gap())
      .child(amount_row(
        "商品小计",
        move || format::yuan_cents(totals.get().subtotal),
        || Tone::Fg,
      ))
      .child(amount_row(
        "优惠",
        move || format::discount(totals.get().discount),
        move || {
          if totals.get().discount > 0 {
            Tone::Acc2
          } else {
            Tone::Mute
          }
        },
      ))
      .child(amount_row(
        "运费",
        move || format::shipping(totals.get().shipping),
        || Tone::Fg,
      ))
      .child(gap())
      .child(ui::thin_bar(
        move || totals.get().free_shipping.progress as f32,
        100.0,
        usize::from(inner),
      ))
      .child(label(
        move || SharedString::from(copy::free_shipping(&totals.get())),
        Tone::Soft,
      ))
      .child(gap())
      .child(ui::rule(false))
      .child(gap())
      .child(
        view()
          .flex_none()
          .flex_row()
          .child(strong("合计", Tone::Fg))
          .child(view().flex_1())
          .child(strong(
            move || SharedString::from(format::yuan_cents(totals.get().total)),
            Tone::Acc,
          )),
      )
      .child(view().flex_1())
      // The demo notice over the button at every height (short: no blank row under it).
      .child(
        copy::demo_order_rows(usize::from(inner))
          .iter()
          .map(|row| strong(*row, Tone::Acc))
          .collect::<Vec<_>>(),
      )
      .child((!short).then(|| vspace(1)))
      .child(submit(stores, inner, true))
      .child(gap()),
  )
}

/// The rows the compact block takes, frame included.
pub const COMPACT_ROWS: u16 = 10;

/// The compact form under the cart (S): the address in one row with `3 更换 →`, the coupon
/// field (label on its frame) and `[ 应用 ]`, the amounts in one row, the bar and its words,
/// then `合计` and the button on one row.
pub fn compact(stores: Stores, shape: Shape) -> View {
  let cart = stores.cart;
  let totals = totals(stores);
  let ship = ship_to(stores);
  let bar = shape.compact_bar();
  ui::panel(
    "结算 CHECKOUT",
    view()
      .flex_none()
      .child(
        view()
          .flex_row()
          .child(view().flex_1().min_w(px(0.0)).child(dynamic(move || {
            let ship = ship.get();
            untracked(|| match ship {
              ShipTo::Guest => label(copy::GUEST_ADDRESS, Tone::Mute).truncate(),
              ShipTo::Nowhere => strong(copy::NO_ADDRESS, Tone::Danger).truncate(),
              ShipTo::Address(address) => {
                label(copy::ship_to_line(&address), Tone::Soft).truncate()
              }
            })
          })))
          .child(hspace(2))
          .child(change(stores)),
      )
      .child(coupon(stores, LabelPlace::OnFrame))
      .child(applied(stores))
      .child(label(
        move || SharedString::from(copy::amounts_line(&totals.get())),
        Tone::Soft,
      ))
      .child(
        view()
          .flex_row()
          .child(ui::thin_bar(
            move || totals.get().free_shipping.progress as f32,
            100.0,
            usize::from(bar),
          ))
          .child(hspace(2))
          .child(
            label(
              move || SharedString::from(copy::free_shipping(&totals.get())),
              Tone::Soft,
            )
            .truncate(),
          ),
      )
      // The demo notice's row gives way to the coupon's line or the two-row progress block
      // (the checkout button's own row still says what it does; the receipt repeats it).
      .child(dynamic(move || {
        let busy = cart.coupon_feedback().is_some() || cart.submitting();
        (!busy).then(|| strong(copy::DEMO_ORDER, Tone::Acc).truncate())
      }))
      .child(
        view()
          .flex_row()
          .items_end()
          .child(label("合计 ", Tone::Fg))
          .child(strong(
            move || SharedString::from(format::yuan_cents(totals.get().total)),
            Tone::Acc,
          ))
          .child(view().flex_1())
          .child(submit(stores, bar, false)),
      ),
  )
}
