//! The new / edit address dialog (05-address.md, "新增 / 编辑地址对话框"): `收货人` and
//! `手机号` side by side (stacked at S), `所在地区`, `详细地址`, the `标签` picker and
//! `设为默认`, then `[ Esc 取消 ]  ▐ ⏎ 保存 ▌`.
//!
//! The focus ring is [`AddressStore::dialog_focus`] (eight stops, Tab order of the spec):
//! the four text fields take nsh's text focus when the ring reaches them
//! ([`ui::input`]); the tag picker, the check box and the two buttons are focus stops of
//! their own, so nsh's Tab / Shift+Tab walk all eight and the ring follows the real focus.
//! `←` `→` choose the tag on the tag stop, `space` toggles `设为默认` on its stop, `⏎`
//! saves anywhere and `Esc` cancels (the commands in `commands.rs`).
//!
//! Errors: a field's error (the server's, P-addr-2) shows as `✗ …` under that field and
//! the ring moves there; one without a field (`保存失败，请重试`, `地址最多保存 20 条`) on
//! the error row under the tag row. The box keeps one spare row for that line, so it never
//! changes size under the caret.

use std::cell::Cell;

use coffee_api::{AddressInput, AddressTag};
use nsh_core::prelude::*;

use super::FIELDS;
use crate::{
  commands as table,
  stores::{AddressField, AddressStore, Overlay, Stores, addresses::DialogStop},
  ui::{
    self, Bp, Button, ButtonKind, ChipPad, DialogSize, InputForm, InputSpec, LabelPlace, Layout,
    Tone, layout,
  },
};

/// The dialog's width (narrowed to the terminal's columns − 4 by `ui::dialog`).
pub const COLS: u16 = 70;
/// Blank cells between the double line and the content.
pub const PAD: u16 = 3;

/// The text fields' stops, in [`FIELDS`] order.
const TEXT_FIELDS: [AddressField; 4] = [
  AddressField::Recipient,
  AddressField::Phone,
  AddressField::Region,
  AddressField::Detail,
];

/// The placeholders of [`FIELDS`] (copy.md).
const PLACEHOLDERS: [&str; 4] = ["", "11 位手机号", "省 / 市 / 区", "街道、门牌号、楼层"];

const TAG: DialogStop = DialogStop::Field(AddressField::Tag);
const DEFAULT: DialogStop = DialogStop::Field(AddressField::Default);

/// How the form is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Form {
  /// `收货人` above `手机号` (S) instead of side by side.
  pub stacked: bool,
  /// Fewer than 32 rows: the labels go on the fields' top lines, no blank rows between them.
  pub short: bool,
}

/// The form at width class `bp`, `short` or not.
pub fn form(bp: Bp, short: bool) -> Form {
  Form {
    stacked: bp <= Bp::S,
    short,
  }
}

/// The dialog's height for `form`: the spec's 23 (17 short) plus the row kept for an error
/// line; the stacked form one field taller (S: 21 at 80×24).
pub fn rows(form: Form) -> u16 {
  let field_rows: u16 = if form.stacked { 4 } else { 3 };
  let field = if form.short { 3 } else { 4 };
  let gaps = if form.short { 0 } else { field_rows };
  // The fields, the tag row and the blank under it, the error row, the buttons.
  let content = field_rows * field + gaps + 1 + 1 + 1 + 1;
  // The two lines, the two blank rows on top and the one under the buttons.
  content + 5
}

fn read(input: &AddressInput, index: usize) -> String {
  match index {
    0 => input.recipient.clone(),
    1 => input.phone.clone().unwrap_or_default(),
    2 => input.region.clone(),
    _ => input.detail.clone(),
  }
}

fn write(input: &mut AddressInput, index: usize, text: String) {
  match index {
    0 => input.recipient = text,
    1 => input.phone = Some(text),
    2 => input.region = text,
    _ => input.detail = text,
  }
}

/// The dialog (the address book's [`super::MODULE`] dialog for `Overlay::AddressDialog`).
pub fn dialog() -> AnyElement {
  let stores = Stores::get();
  let layout = Layout::get();
  let book = stores.addresses;
  let title = if untracked(|| book.dialog().and_then(|d| d.editing)).is_some() {
    "编辑地址 EDIT"
  } else {
    "新增地址 NEW"
  };
  let shape = computed(move || form(layout.bp(), layout.short()));
  dynamic(move || {
    let form = shape.get();
    ui::dialog(
      move || stores.ui.is_open(Overlay::AddressDialog),
      title,
      DialogSize {
        cols: COLS,
        rows: rows(form),
        pad: PAD,
      },
      move || content(stores, form),
      Vec::new,
    )
  })
}

/// The dialog's focus ring as its stops read it: the store's stop. A stop that is no text
/// field asks nsh for the focus once the box is laid out (`ElementRef::focus` does nothing
/// before); the fields take it as they mount (`ui::input`'s `auto_focus`).
#[derive(Clone, Copy)]
struct Ring {
  book: AddressStore,
  laid_out: Signal<bool>,
}

impl Ring {
  /// Whether `at` is the stop now (tracked).
  fn is(self, at: DialogStop) -> bool {
    self.book.dialog_focus() == at
  }

  /// Whether `at` should hold nsh's focus now (tracked).
  fn holds(self, at: DialogStop) -> bool {
    self.laid_out.get() && self.is(at)
  }
}

fn content(stores: Stores, form: Form) -> AnyElement {
  let book = stores.addresses;
  let ring = Ring {
    book,
    laid_out: signal(false),
  };
  let place = if form.short {
    LabelPlace::OnFrame
  } else {
    LabelPlace::Above
  };
  let gap = move || (!form.short).then(|| layout::vspace(1));
  let names = if form.stacked {
    view()
      .flex_none()
      .child(field(stores, ring, 0, place))
      .child(gap())
      .child(field(stores, ring, 1, place))
  } else {
    view()
      .flex_none()
      .flex_row()
      .child(
        view()
          .flex_1()
          .min_w(px(0.0))
          .child(field(stores, ring, 0, place)),
      )
      .child(layout::hspace(2))
      .child(
        view()
          .flex_1()
          .min_w(px(0.0))
          .child(field(stores, ring, 1, place)),
      )
  };
  view()
    .flex_1()
    .min_h(px(0.0))
    .on_layout(move |_: &Bounds<Points>| {
      if !ring.laid_out.get_untracked() {
        ring.laid_out.set(true);
      }
    })
    .child(names)
    .child(gap())
    .child(field(stores, ring, 2, place))
    .child(gap())
    .child(field(stores, ring, 3, place))
    .child(gap())
    .child(tag_row(stores, ring))
    .child(layout::vspace(1))
    .child(ui::error_line(move || {
      book
        .dialog()
        .and_then(|d| d.error)
        .filter(|e| !e.field.is_some_and(|f| TEXT_FIELDS.contains(&f)))
        .map(|e| SharedString::from(e.message))
    }))
    .child(view().flex_1())
    .child(buttons(stores, ring))
    .into_element()
}

/// One text field: its label, its frame (heavy while it has the focus) and its error.
fn field(stores: Stores, ring: Ring, index: usize, place: LabelPlace) -> View {
  let (name, label) = FIELDS[index];
  let which = TEXT_FIELDS[index];
  let at = DialogStop::Field(which);
  let book = stores.addresses;
  let ui_store = stores.ui;
  // A field closed with the focus in it reports no blur: give the keys back.
  cleanup(move || ui_store.field_blurred(name));
  let handle = element_ref();
  ui::input(
    InputSpec {
      form: InputForm::Plain,
      label: Some(label.into()),
      place,
      hint: None,
      placeholder: PLACEHOLDERS[index].into(),
      masked: false,
      uppercase: false,
      element: Some(handle),
      // While saving only Esc works: nothing types, Tab moves nothing.
      disabled: Some(std::rc::Rc::new(move || book.saving())),
    },
    move || {
      book
        .dialog()
        .map(|d| read(&d.input, index))
        .unwrap_or_default()
    },
    // The input takes the focus as it mounts (`auto_focus`): no wait for the layout.
    move || ring.is(at),
    move || {
      book
        .dialog()
        .and_then(|d| d.error)
        .filter(|e| e.field == Some(which))
        .map(|e| SharedString::from(e.message))
    },
    move |text| book.edit(|input| write(input, index, text)),
    move |focused| {
      if focused {
        batch(|| {
          book.focus_dialog(at);
          ui_store.field_focused(name, Some(handle));
        });
      } else {
        ui_store.field_blurred(name);
      }
    },
    || {},
  )
}

/// A focus stop that is not a text field: Tab reaches it, and it takes the focus when the
/// ring is put on it.
fn stop(ring: Ring, at: DialogStop, content: impl IntoElement) -> View {
  let handle = element_ref();
  let was = Cell::new(false);
  effect(move || {
    let now = ring.holds(at);
    if now && !was.get() {
      handle.focus();
    }
    was.set(now);
  });
  view()
    .flex_none()
    .focusable(true)
    .element_ref(&handle)
    .on_focus(move || ring.book.focus_dialog(at))
    .child(content)
}

/// `›` in the cell left of a control while `focused()`.
fn focus_mark(focused: impl Fn() -> bool + 'static) -> View {
  view()
    .absolute()
    .top(px(0.0))
    .left(px(-layout::CELL_WIDTH))
    .style_when(move || !focused(), |s| s.hidden())
    .child(ui::strong("›", Tone::Acc))
}

/// `标签  ▐ 家 ▌ [ 公司 ] [ 学校 ]` and, right-aligned, `[ ] 设为默认`.
fn tag_row(stores: Stores, ring: Ring) -> View {
  let book = stores.addresses;
  let on_tag = move || ring.is(TAG);
  let names: [&'static str; 3] = AddressTag::ALL.map(AddressTag::label);
  let selected = move || {
    book
      .dialog()
      .and_then(|d| AddressTag::ALL.iter().position(|t| *t == d.input.tag))
      .unwrap_or(0)
  };
  let picker = view()
    .flex_row()
    .child(
      view()
        .flex_none()
        .child(
          ui::toned("标签", move || if on_tag() { Tone::Fg } else { Tone::Mute })
            .style_when(on_tag, |s| s.font_weight(FontWeight::BOLD).underline()),
        )
        .child(focus_mark(on_tag)),
    )
    .child(layout::hspace(2))
    .child(ui::segmented(
      &names,
      ChipPad::Padded,
      selected,
      move |index| {
        batch(|| {
          book.focus_dialog(TAG);
          book.edit(|input| input.tag = AddressTag::ALL[index]);
        })
      },
    ));
  let checked = move || book.dialog().is_some_and(|d| d.input.is_default);
  let check = ui::checkbox(
    "设为默认",
    checked,
    || false,
    move || ring.is(DEFAULT),
    move || {
      batch(|| {
        book.focus_dialog(DEFAULT);
        book.edit(|input| input.is_default = !input.is_default);
      })
    },
  );
  view()
    .flex_none()
    .flex_row()
    .child(stop(ring, TAG, picker))
    .child(view().flex_1())
    .child(stop(ring, DEFAULT, check))
}

/// `[ Esc 取消 ]  ▐ ⏎ 保存 ▌`, right-aligned; `⠹ 保存中…` in the save button's place
/// while saving.
fn buttons(stores: Stores, ring: Ring) -> View {
  let book = stores.addresses;
  let settings = stores.settings;
  let saving = computed(move || book.saving());
  let cancel = Button::new(ButtonKind::Default, "Esc", "取消")
    .focused(move || ring.is(DialogStop::Cancel))
    .on_press(move || table::press(&stores, "dialog.cancel"));
  let save = dynamic(move || {
    if saving.get() {
      text("")
        .child(ui::spinner(move || settings.animations()))
        .child(ui::label(" 保存中…", Tone::Acc))
        .into_element()
    } else {
      Button::new(ButtonKind::Primary, "⏎", "保存")
        .focused(move || ring.is(DialogStop::Save))
        .on_press(move || table::press(&stores, "dialog.save"))
        .into_element()
    }
  });
  view()
    .flex_none()
    .flex_row()
    .justify_end()
    .child(stop(ring, DialogStop::Cancel, cancel))
    .child(layout::hspace(2))
    .child(stop(ring, DialogStop::Save, save))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_box_is_the_specs_plus_its_error_row() {
    // XL / L / M: 23 + 1.
    assert_eq!(rows(form(Bp::Xl, false)), 24);
    // Short (100×30): 17 + 1.
    assert_eq!(rows(form(Bp::M, true)), 18);
    // S (80×24): stacked and short, 21: fits the 22 rows between the bars.
    assert_eq!(rows(form(Bp::S, true)), 21);
    assert!(form(Bp::S, true).stacked && !form(Bp::M, false).stacked);
  }

  #[test]
  fn fields_read_and_write_their_part() {
    let mut input = AddressInput::default();
    for index in 0..4 {
      write(&mut input, index, format!("v{index}"));
    }
    assert_eq!(input.recipient, "v0");
    assert_eq!(input.phone.as_deref(), Some("v1"));
    assert_eq!(read(&input, 2), "v2");
    assert_eq!(read(&input, 3), "v3");
    assert_eq!(FIELDS[1].1, "手机号");
  }
}
