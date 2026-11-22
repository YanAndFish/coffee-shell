//! Button: every action, its key first (`components/Button`). One row tall.
//!
//! ```text
//! ▐ ⏎ 查看详情 ▌  [ a 快速加入 ]  [ b 立即购买 ]  [ q 退出登录 ]  ╎ 已删除 慧兰 · u 撤销 ╎  3 更换 →
//! ```
//!
//! | kind | drawn | colour |
//! |---|---|---|
//! | `Primary` | `▐ 文字 ▌` | `acc` fill, `ink` bold (reverse video in monochrome) |
//! | `Default` | `[ 文字 ]` | brackets `line`, text `fg` |
//! | `Outline` | `[ 文字 ]` | brackets and bold text `acc` ("b 立即购买" only) |
//! | `Danger` | `[ 文字 ]` | brackets `danger-line`, text `danger` |
//! | `Undo` | `╎ 文字 ╎` | dashes `danger-line`, text `danger` |
//! | `Green` | `[ 文字 → ]` | brackets `acc2-line`, text `acc2` |
//! | `Link` | `文字 →` | `acc`, no brackets |
//!
//! The "add" card is [`super::address_card::add_card`].
//!
//! States: *focused* (the caller's `focused()`: a `›` in the cell to the left, the label
//! bold and underlined); *disabled* (`disabled()`: the label `disabled` and struck through,
//! no fill, presses do nothing); *loading*: the caller puts [`super::meter::progress`] in
//! the button's place.
//!
//! The press is a mouse convenience: the button never takes the keyboard focus (the
//! command table owns every key) and runs the same `on_press` the key runs.
//!
//! ```ignore
//! Button::new(ButtonKind::Primary, "⏎", "退出登录").on_press(move || press(&stores, "logout.confirm"))
//! Button::new(ButtonKind::Default, "g", "访客浏览").key_cap().width(22)
//! Button::new(ButtonKind::Primary, "⏎", "登录").key_after().width(45)
//! ```

use std::rc::Rc;

use nsh_core::prelude::*;

use super::{
  action::pressable,
  kbd::{kbd, kbd_on_fill, kbd_width},
  label::{label, toned},
  layout,
  text::display_width,
  theme::{Fill, Tone},
};

/// A button's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonKind {
  /// `▐ 文字 ▌`: the one main action of a screen.
  Primary,
  /// `[ 文字 ]`: secondary actions.
  Default,
  /// `[ 文字 ]` in amber: "立即购买".
  Outline,
  /// `[ 文字 ]` in coral: logout, delete.
  Danger,
  /// `╎ 文字 ╎`: the undo bar after a delete.
  Undo,
  /// `[ 文字 → ]` in mint: a recommendation.
  Green,
  /// `文字 →`: an inline link ("3 更换 →").
  Link,
}

/// Where a button's key goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyPlace {
  /// Plain, before the label: `⏎ 查看详情`.
  Lead,
  /// As a key cap before the label: `‹g› 访客浏览`.
  Cap,
  /// As a key cap after the label: `登录 ‹⏎›`.
  CapAfter,
}

/// A button: build it, then use it as an element.
#[derive(Clone)]
pub struct Button {
  kind: ButtonKind,
  key: SharedString,
  label: SharedString,
  place: KeyPlace,
  arrow: bool,
  width: Option<u16>,
  disabled: Option<Rc<dyn Fn() -> bool>>,
  focused: Option<Rc<dyn Fn() -> bool>>,
  on_press: Option<Rc<dyn Fn()>>,
}

impl Button {
  /// A `kind` button for the key `key` (empty for none) doing `label`.
  pub fn new(
    kind: ButtonKind,
    key: impl Into<SharedString>,
    label: impl Into<SharedString>,
  ) -> Self {
    Self {
      kind,
      key: key.into(),
      label: label.into(),
      place: KeyPlace::Lead,
      arrow: matches!(kind, ButtonKind::Green | ButtonKind::Link),
      width: None,
      disabled: None,
      focused: None,
      on_press: None,
    }
  }

  /// The key as a cap before the label (`[ ‹g› 访客浏览 ]`).
  pub fn key_cap(self) -> Self {
    Self {
      place: KeyPlace::Cap,
      ..self
    }
  }

  /// The key as a cap after the label (`▐ 登录 ‹⏎› ▌`).
  pub fn key_after(self) -> Self {
    Self {
      place: KeyPlace::CapAfter,
      ..self
    }
  }

  /// A trailing ` →` (on by default for `Green` and `Link`).
  pub fn arrow(self) -> Self {
    Self {
      arrow: true,
      ..self
    }
  }

  /// Stretched to `cells` columns, its content centred (the odd cell goes right).
  pub fn width(self, cells: u16) -> Self {
    Self {
      width: Some(cells),
      ..self
    }
  }

  /// Disabled while `disabled()` holds (tracked).
  pub fn disabled(self, disabled: impl Fn() -> bool + 'static) -> Self {
    Self {
      disabled: Some(Rc::new(disabled)),
      ..self
    }
  }

  /// Shows the keyboard focus while `focused()` holds (tracked).
  pub fn focused(self, focused: impl Fn() -> bool + 'static) -> Self {
    Self {
      focused: Some(Rc::new(focused)),
      ..self
    }
  }

  /// Runs `on_press` on a click (as the key does).
  pub fn on_press(self, on_press: impl Fn() + 'static) -> Self {
    Self {
      on_press: Some(Rc::new(on_press)),
      ..self
    }
  }

  /// The cells of its content: key, label and arrow, without ends or padding.
  pub fn content_width(&self) -> usize {
    let key = if self.key.is_empty() {
      0
    } else {
      match self.place {
        KeyPlace::Lead => display_width(&self.key) + 1,
        KeyPlace::Cap | KeyPlace::CapAfter => kbd_width(&self.key) + 1,
      }
    };
    key + display_width(&self.label) + if self.arrow { 2 } else { 0 }
  }

  /// The cells of its ends and inner padding (`▐ ` + ` ▌`, `[ ` + ` ]`, none for a link).
  pub fn chrome_width(&self) -> usize {
    match self.kind {
      ButtonKind::Link => 0,
      _ => 4,
    }
  }

  /// The cells it takes.
  pub fn cells(&self) -> usize {
    self
      .width
      .map_or(self.content_width() + self.chrome_width(), usize::from)
  }

  /// The blank cells left and right of the content inside the ends.
  pub fn spare(&self) -> (usize, usize) {
    let inner = self.cells().saturating_sub(self.chrome_width());
    let spare = inner.saturating_sub(self.content_width());
    (spare / 2, spare - spare / 2)
  }

  fn tones(&self) -> (Tone, Tone) {
    match self.kind {
      ButtonKind::Primary => (Tone::Acc, Tone::Ink),
      ButtonKind::Default => (Tone::Line, Tone::Fg),
      ButtonKind::Outline | ButtonKind::Link => (Tone::Acc, Tone::Acc),
      ButtonKind::Danger | ButtonKind::Undo => (Tone::DangerLine, Tone::Danger),
      ButtonKind::Green => (Tone::Acc2Line, Tone::Acc2),
    }
  }

  fn ends(&self) -> (&'static str, &'static str) {
    match self.kind {
      ButtonKind::Primary => ("▐", "▌"),
      ButtonKind::Undo => ("╎", "╎"),
      ButtonKind::Link => ("", ""),
      _ => ("[", "]"),
    }
  }
}

fn spaces(n: usize) -> Option<Text> {
  (n > 0).then(|| text(" ".repeat(n)))
}

impl IntoElement for Button {
  fn into_element(self) -> AnyElement {
    let disabled: Rc<dyn Fn() -> bool> = self.disabled.clone().unwrap_or(Rc::new(|| false));
    let focused: Rc<dyn Fn() -> bool> = self.focused.clone().unwrap_or(Rc::new(|| false));
    let (end_tone, text_tone) = self.tones();
    let (open, close) = self.ends();
    let primary = self.kind == ButtonKind::Primary;
    let bold = matches!(self.kind, ButtonKind::Primary | ButtonKind::Outline);
    let (left, right) = self.spare();
    let pad = usize::from(self.kind != ButtonKind::Link);

    let text_of = {
      let disabled = disabled.clone();
      move || {
        if disabled() {
          Tone::Disabled
        } else {
          text_tone
        }
      }
    };
    let styled = {
      let disabled = disabled.clone();
      let focused = focused.clone();
      move |t: Text| -> Text {
        let (d1, d2, f1) = (disabled.clone(), disabled.clone(), focused.clone());
        t.style_when(move || bold && !d1(), |s| s.font_weight(FontWeight::BOLD))
          .style_when(move || d2(), |s| s.line_through())
          .style_when(
            move || f1(),
            |s| s.font_weight(FontWeight::BOLD).underline(),
          )
      }
    };
    let key_part: Option<AnyElement> = if self.key.is_empty() {
      None
    } else {
      Some(match self.place {
        KeyPlace::Lead => styled(toned(format!("{} ", self.key), text_of.clone())).into_element(),
        KeyPlace::Cap | KeyPlace::CapAfter if primary => kbd_on_fill(&self.key).into_element(),
        KeyPlace::Cap | KeyPlace::CapAfter => kbd(self.key.clone()).into_element(),
      })
    };
    let label_text = if self.arrow {
      format!("{} →", self.label)
    } else {
      self.label.to_string()
    };
    let label_part = styled(toned(label_text, text_of.clone()));
    let mut content = view().flex_row().flex_none();
    content = content.child(spaces(left + pad));
    match self.place {
      KeyPlace::CapAfter => content = content.child(label_part).child(text(" ")).child(key_part),
      KeyPlace::Cap => content = content.child(key_part).child(text(" ")).child(label_part),
      KeyPlace::Lead => content = content.child(key_part).child(label_part),
    }
    content = content.child(spaces(right + pad));
    let content = if primary {
      let fill_disabled = disabled.clone();
      content.bg(move || {
        if fill_disabled() {
          AdaptiveColor::TRANSPARENT
        } else {
          Fill::Acc.color()
        }
      })
    } else {
      content
    };
    let end = |glyph: &'static str| {
      let disabled = disabled.clone();
      (!glyph.is_empty()).then(|| {
        toned(
          glyph,
          move || if disabled() { Tone::Disabled } else { end_tone },
        )
      })
    };
    let press = self.on_press.clone();
    let press_disabled = disabled.clone();
    let cursor_focused = focused.clone();
    pressable()
      .flex_none()
      .flex_row()
      .label(SharedString::from(
        format!("{} {}", self.key, self.label).trim().to_owned(),
      ))
      .on_press(move || {
        if !press_disabled()
          && let Some(press) = &press
        {
          press();
        }
      })
      .child(end(open))
      .child(content)
      .child(end(close))
      .child(
        view()
          .absolute()
          .top(px(0.0))
          .left(px(-layout::CELL_WIDTH))
          .style_when(move || !cursor_focused(), |s| s.hidden())
          .child(label("›", Tone::Acc).font_weight(FontWeight::BOLD)),
      )
      .into_element()
  }
}

/// A button of `kind` for `key` doing `label`, pressed with `on_press` (natural width).
pub fn button(
  kind: ButtonKind,
  key: impl Into<SharedString>,
  label: impl Into<SharedString>,
  on_press: impl Fn() + 'static,
) -> AnyElement {
  Button::new(kind, key, label)
    .on_press(on_press)
    .into_element()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn widths_match_the_design() {
    // `▐ ⏎ 查看详情 ▌`
    assert_eq!(
      Button::new(ButtonKind::Primary, "⏎", "查看详情").cells(),
      14
    );
    // `[ a 快速加入 ]`
    assert_eq!(
      Button::new(ButtonKind::Default, "a", "快速加入").cells(),
      14
    );
    // `3 更换 →`
    assert_eq!(Button::new(ButtonKind::Link, "3", "更换").cells(), 8);
    // `[ Esc 取消 ]` and `▐ ⏎ 退出登录 ▌`, two cells apart: 12 + 2 + 14.
    assert_eq!(Button::new(ButtonKind::Default, "Esc", "取消").cells(), 12);
    assert_eq!(
      Button::new(ButtonKind::Primary, "⏎", "退出登录").cells(),
      14
    );
  }

  #[test]
  fn stretched_content_is_centred_like_the_login_card() {
    // `▐                 登录 ‹⏎›                  ▌`: 45 cells, 17 blanks left, 18 right
    // (one of each is the pill's own padding).
    let login = Button::new(ButtonKind::Primary, "⏎", "登录")
      .key_after()
      .width(45);
    assert_eq!(login.content_width(), 8);
    assert_eq!(login.spare(), (16, 17));
    // `[    ‹g› 访客浏览    ]`: 22 cells.
    let guest = Button::new(ButtonKind::Default, "g", "访客浏览")
      .key_cap()
      .width(22);
    assert_eq!(guest.spare(), (3, 3));
    // `[    注册新账号 →    ]`
    let register = Button::new(ButtonKind::Default, "", "注册新账号")
      .arrow()
      .width(22);
    assert_eq!(register.content_width(), 12);
    assert_eq!(register.spare(), (3, 3));
  }
}
