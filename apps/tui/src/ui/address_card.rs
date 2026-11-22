//! AddressCard: one address of the address book, and the "add" placeholder card
//! (`components/AddressCard`).
//!
//! ```text
//! ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
//! ┃                                                                           ┃
//! ┃  ▶ 林晓  138 **** 5521                            [本单寄往] ▐默认▌ [家]  ┃
//! ┃     上海市 徐汇区                                                         ┃
//! ┃     天平路 88 弄 3 号 602 室                                              ┃
//! ┃                                                                           ┃
//! ┃     [ ⏎ 用于本单 ]  [ e 编辑 ]  [ space 设为默认 ]  [ d 删除 ]            ┃
//! ┃                                                                           ┃
//! ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
//! ```
//!
//! The current card is the heavy amber frame and `▶`; the others a thin `line` frame with the
//! `▶` cell blank. Head: `▶`, the name (bold), two cells, the masked phone (`mute`); the tags
//! right-aligned in the order `[本单寄往]` (mint outline), `▐默认▌` (amber), `[家/公司/学校]`
//! (neutral). The body is indented three cells: the region (`mute`), the street (`fg`). The
//! action row (current card only on S) sits third from the bottom. The add card is the same
//! size in a dashed `line-strong` frame, `+` (`acc`) and "新增收货地址" (`mute`) centred.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  button::Button,
  frame::{FrameStyle, frame},
  label::{label, strong, toned},
  layout,
  tag::{TagKind, tag},
  theme::Tone,
};

/// An address as the card shows it (already formatted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressView {
  /// The recipient, `林晓`.
  pub name: SharedString,
  /// The masked phone, `138 **** 5521` (or `—`).
  pub phone: SharedString,
  /// The tag, `家` / `公司` / `学校` (none: no tag).
  pub tag: Option<SharedString>,
  /// The region, `上海市 徐汇区`.
  pub region: SharedString,
  /// The street, `天平路 88 弄 3 号 602 室`.
  pub line: SharedString,
  /// It is the default address.
  pub default: bool,
  /// It is the address this order ships to.
  pub for_order: bool,
}

/// A phone number as the cards show it: `138 **** 5521` for 11 digits, `—` when empty, else
/// as given.
pub fn mask_phone(phone: &str) -> String {
  let digits: Vec<char> = phone.chars().filter(char::is_ascii_digit).collect();
  if phone.trim().is_empty() {
    "—".to_owned()
  } else if digits.len() == 11 {
    let head: String = digits[..3].iter().collect();
    let tail: String = digits[7..].iter().collect();
    format!("{head} **** {tail}")
  } else {
    phone.to_owned()
  }
}

/// The height from which a card shows its full layout (L's 9-row card; XL's 11 rows add
/// blank rows above the actions).
pub const FULL_ROWS: u16 = 9;

/// An address card `cols` × `rows` (XL 77×11; S 5 with actions, 4 without) showing `address`; `current()` makes it the
/// heavy amber one with `▶`. `actions` (the caller's buttons) go on the third row from the
/// bottom; give none to leave the row out (the S breakpoint's other cards). A click runs
/// `on_press(clicks)`.
pub fn address_card(
  address: AddressView,
  cols: u16,
  rows: u16,
  current: impl Fn() -> bool + Copy + 'static,
  actions: Vec<Button>,
  on_press: impl Fn(u32) + 'static,
) -> View {
  let style = move || {
    if current() {
      FrameStyle::Heavy
    } else {
      FrameStyle::Round
    }
  };
  let tone = move || if current() { Tone::Acc } else { Tone::Line };
  let tags = view()
    .flex_none()
    .flex_row()
    .gap(layout::gap_cols(1))
    .child(address.for_order.then(|| tag(TagKind::Outline, "本单寄往")))
    .child(address.default.then(|| tag(TagKind::Default, "默认")))
    .child(address.tag.clone().map(|name| tag(TagKind::Neutral, name)));
  let head = view()
    .flex_row()
    .child(
      toned(move || if current() { "▶ " } else { "  " }, || Tone::Acc)
        .font_weight(FontWeight::BOLD),
    )
    .child(strong(address.name.clone(), Tone::Fg))
    .child(label(format!("  {}", address.phone), Tone::Mute))
    .child(view().flex_1())
    .child(tags);
  // The full card (9 rows or more): a blank row, the head, region and street on two rows,
  // the actions third from the bottom. A compact one (S: 5 rows with actions, 4 without):
  // the head, then region and street on one row, then the actions.
  let full = rows >= FULL_ROWS;
  let body = view().flex_row().child(layout::hspace(3)).child(if full {
    view()
      .flex_1()
      .min_w(px(0.0))
      .child(label(address.region.clone(), Tone::Mute))
      .child(label(address.line.clone(), Tone::Fg).truncate())
  } else {
    view().flex_1().min_w(px(0.0)).child(
      text("")
        .child(label(format!("{} · ", address.region), Tone::Mute))
        .child(label(address.line.clone(), Tone::Fg))
        .truncate(),
    )
  });
  let has_actions = !actions.is_empty();
  let action_row = has_actions.then(|| {
    view()
      .flex_row()
      .child(layout::hspace(3))
      .child(view().flex_row().gap(layout::gap_cols(2)).children(actions))
  });
  let content = if full {
    view()
      .flex_1()
      .child(layout::vspace(1))
      .child(head)
      .child(body)
      .child(view().flex_1())
      .child(action_row)
      .child(has_actions.then(|| layout::vspace(1)))
  } else {
    view().flex_1().child(head).child(body).child(action_row)
  };
  pressable()
    .flex_none()
    .w(layout::cols(cols))
    .h(layout::rows(rows))
    .label(address.name.clone())
    .on_press(move |event: &nsh_core::element::PressEvent| on_press(event.clicks))
    .child(frame(style, tone, None, Some(rows), 2, content))
}

/// The "add" placeholder card, `cols` × `rows`: a dashed `line-strong` frame with `+` and
/// "新增收货地址" centred; a press runs `on_press` (the `n` command).
pub fn add_card(cols: u16, rows: u16, on_press: impl Fn() + 'static) -> View {
  pressable()
    .flex_none()
    .w(layout::cols(cols))
    .h(layout::rows(rows))
    .label("新增收货地址")
    .on_press(on_press)
    .child(frame(
      || FrameStyle::Dashed,
      || Tone::LineStrong,
      None,
      Some(rows),
      0,
      if rows >= 5 {
        view()
          .flex_1()
          .items_center()
          .justify_center()
          .child(strong("+", Tone::Acc))
          .child(label("新增收货地址", Tone::Mute))
      } else {
        // S: one row, `+ 新增收货地址`.
        view().flex_1().items_center().child(
          text("")
            .child(strong("+ ", Tone::Acc))
            .child(label("新增收货地址", Tone::Mute)),
        )
      },
    ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn phones_are_masked_in_the_middle() {
    assert_eq!(mask_phone("13800005521"), "138 **** 5521");
    assert_eq!(mask_phone("138 0000 5521"), "138 **** 5521");
    assert_eq!(mask_phone(""), "—");
    assert_eq!(mask_phone("12345"), "12345");
  }
}
