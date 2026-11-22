//! Stepper: a quantity, one row, 13 cells (`components/Stepper`).
//!
//! ```text
//! [ − │ 1 │ + ]  现货 26 份 · 每周二、五烘焙     ← detail ([`StepperForm::Divided`])
//! [ −   1   + ]                                  ← cart row ([`StepperForm::Plain`])
//! ```
//!
//! `−` (U+2212) and `+` are pressable; the number is bold `fg`. At the lower bound `−` turns
//! `disabled`, at the upper bound `+` does. The caller owns the value, the bounds and the
//! commands (`+`/`=`, `-`/`_`, `← →`, `h l` are bound in the command table).

use nsh_core::prelude::*;

use super::{
  action::pressable,
  label::{label, strong, toned},
  theme::Tone,
};

/// The cells a stepper takes.
pub const STEPPER_CELLS: usize = 13;

/// The two forms of the design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepperForm {
  /// `[ − │ 1 │ + ]`.
  Divided,
  /// `[ −   1   + ]`.
  Plain,
}

/// A stepper showing `value()` between `min` and `max()`; `−` runs `on_dec`, `+` `on_inc`
/// (each only when not at its bound).
pub fn stepper(
  form: StepperForm,
  value: impl Fn() -> u32 + Copy + 'static,
  min: u32,
  max: impl Fn() -> u32 + Copy + 'static,
  on_dec: impl Fn() + 'static,
  on_inc: impl Fn() + 'static,
) -> View {
  let rule = match form {
    StepperForm::Divided => " │ ",
    StepperForm::Plain => "   ",
  };
  let at_min = move || value() <= min;
  let at_max = move || value() >= max();
  view()
    .flex_none()
    .flex_row()
    .label("数量")
    .child(label("[ ", Tone::Line))
    .child(
      pressable()
        .label("减少")
        .on_press(move || {
          if !at_min() {
            on_dec();
          }
        })
        .child(toned("−", move || {
          if at_min() { Tone::Disabled } else { Tone::Fg }
        })),
    )
    .child(label(rule, Tone::Line))
    .child(strong(
      move || SharedString::from(value().to_string()),
      Tone::Fg,
    ))
    .child(label(rule, Tone::Line))
    .child(
      pressable()
        .label("增加")
        .on_press(move || {
          if !at_max() {
            on_inc();
          }
        })
        .child(toned(
          "+",
          move || if at_max() { Tone::Disabled } else { Tone::Fg },
        )),
    )
    .child(label(" ]", Tone::Line))
}

/// The stock note beside the detail page's stepper, and its tone: more than 10 `acc2`
/// "现货 {n} 份 · 每周二、五烘焙", 1–10 `acc` "仅剩 {n} 份 · 售完即止", 0 `danger` "暂时缺货".
pub fn stock_note(stock: u32) -> (String, Tone) {
  match stock {
    0 => ("暂时缺货".to_owned(), Tone::Danger),
    1..=10 => (format!("仅剩 {stock} 份 · 售完即止"), Tone::Acc),
    _ => (format!("现货 {stock} 份 · 每周二、五烘焙"), Tone::Acc2),
  }
}

/// The upper bound of a quantity: min(9, stock).
pub fn max_quantity(stock: u32) -> u32 {
  stock.min(9)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn both_forms_are_13_cells() {
    assert_eq!(display_width("[ − │ 1 │ + ]"), STEPPER_CELLS);
    assert_eq!(display_width("[ −   1   + ]"), STEPPER_CELLS);
  }

  #[test]
  fn stock_notes_follow_the_thresholds() {
    assert_eq!(stock_note(26).1, Tone::Acc2);
    assert_eq!(stock_note(26).0, "现货 26 份 · 每周二、五烘焙");
    assert_eq!(
      stock_note(10),
      ("仅剩 10 份 · 售完即止".to_owned(), Tone::Acc)
    );
    assert_eq!(stock_note(0), ("暂时缺货".to_owned(), Tone::Danger));
    assert_eq!(max_quantity(6), 6);
    assert_eq!(max_quantity(42), 9);
  }
}
