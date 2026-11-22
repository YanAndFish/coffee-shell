//! 登录 · 注册 (01-login.md): the first page of a new connection (unless its key is
//! remembered), and where a gated checkout or `⏎ 去登录` sends a guest.
//!
//! Store: [`crate::stores::SessionStore`] — the form (`draft`, `set_username`,
//! `set_password`, `toggle_remember`, `set_mode`), the card's focus (`form_focus`), the
//! submit's progress (`step`, `submitting`), `resuming`, `has_key`. Actions:
//! `actions::submit_auth`, `actions::continue_as_guest`. Fields report their focus as
//! [`USERNAME_FIELD`] / [`PASSWORD_FIELD`].
//!
//! Layouts (responsive.md, "01 登录"):
//!
//! | width | 32 rows or more | fewer |
//! |---|---|---|
//! | XL ≥160 | brand column (block wordmark, cup, points; left edge col 9) + full card in 65 | one-row brand + compact card |
//! | L 130–159 | the same, left edge col 5 | the same |
//! | M 100–129 | one-row brand in 45 + compact card | the same |
//! | S 80–99 | centred `◆ROAST/TTY · 标语` row over the compact card | the same |
//!
//! The focus starts in 用户名 (K1); see [`card`] for the focus ring.

mod brand;
mod card;
pub mod commands;

use nsh_core::{element::Screen, prelude::*};

use super::PageModule;
use crate::{
  commands::Mode,
  router::{Page, Route},
  stores::{LoginMode, Stores, session::FormControl},
  ui::{self, Bp, Layout, Viewport, layout},
};

/// `登录 LOGIN`.
pub const MODE: Mode = Mode::new("登录", "LOGIN");
/// `注册 SIGN UP` (register mode).
pub const REGISTER_MODE: Mode = Mode::new("注册", "SIGN UP");

/// The username field's name in the focus tracking.
pub const USERNAME_FIELD: &str = "login.username";
/// The password field's name in the focus tracking.
pub const PASSWORD_FIELD: &str = "login.password";

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Login,
  mode,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

fn mode(stores: &Stores) -> Mode {
  match stores.session.mode() {
    LoginMode::Login => MODE,
    LoginMode::Register => REGISTER_MODE,
  }
}

/// How the page lays itself out at a terminal size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
  /// Brand column and card side by side: the column `indent` cells in, the card's column
  /// `right` wide with the card `lead` cells in; `tall` (32 rows or more, L and XL): the
  /// block wordmark with the cup and the full card, else the one-row forms and the compact
  /// card.
  Split {
    /// Cells before the brand column.
    indent: u16,
    /// The card column's width.
    right: u16,
    /// Cells before the card in its column.
    lead: u16,
    /// The tall forms.
    tall: bool,
  },
  /// S: the brand row over the compact card, centred.
  Single,
}

impl Form {
  /// The layout for `viewport` (too small never reaches the page).
  pub fn of(viewport: Viewport) -> Self {
    let tall = !viewport.short();
    match viewport.bp() {
      Bp::TooSmall | Bp::S => Form::Single,
      // The page has the whole width (the shell keeps no margins on it): 01-login.md's
      // columns are the screen's.
      Bp::M => Form::Split {
        indent: 2,
        right: card::CARD_COLS + 2,
        lead: 0,
        tall: false,
      },
      Bp::L => Form::Split {
        indent: 5,
        right: 65,
        lead: 2,
        tall,
      },
      Bp::Xl => Form::Split {
        indent: 9,
        right: 65,
        lead: 2,
        tall,
      },
    }
  }
}

fn body(stores: Stores, form: Form) -> View {
  match form {
    Form::Single => view()
      .flex_1()
      .justify_center()
      .child(brand::line())
      .child(layout::vspace(1))
      .child(
        view()
          .flex_none()
          .flex_row()
          .justify_center()
          .child(card::card(stores, true)),
      ),
    Form::Split {
      indent,
      right,
      lead,
      tall,
    } => {
      let brand = if tall {
        brand::tall(stores)
      } else {
        brand::compact()
      };
      view()
        .flex_1()
        .flex_row()
        .child(
          view().flex_1().min_w(px(0.0)).justify_center().child(
            view()
              .flex_none()
              .flex_row()
              .child(layout::hspace(indent))
              .child(brand),
          ),
        )
        .child(
          view()
            .flex_none()
            .w(layout::cols(right))
            .justify_center()
            .child(
              view()
                .flex_none()
                .flex_row()
                .child(layout::hspace(lead))
                .child(card::card(stores, !tall)),
            ),
        )
    }
  }
}

/// The login page.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  // K1: the focus starts in 用户名 (typing `g` there types a g).
  untracked(|| stores.session.set_form_focus(Some(FormControl::Username)));
  card::follow_submit(stores.session);
  let form = computed(move || Form::of(layout.viewport()));
  ui::page(
    "登录 LOGIN",
    view().flex_1().child(dynamic(move || {
      let form = form.get();
      untracked(|| body(stores, form))
    })),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_form_follows_the_breakpoints() {
    let form = |cols, rows| Form::of(Viewport::new(cols, rows));
    assert_eq!(form(80, 24), Form::Single);
    assert_eq!(form(99, 40), Form::Single);
    assert!(matches!(form(100, 30), Form::Split { tall: false, .. }));
    assert!(matches!(form(120, 40), Form::Split { tall: false, .. }));
    assert!(matches!(
      form(130, 40),
      Form::Split {
        indent: 5,
        tall: true,
        ..
      }
    ));
    assert!(matches!(form(130, 31), Form::Split { tall: false, .. }));
    assert!(matches!(
      form(160, 50),
      Form::Split {
        indent: 9,
        right: 65,
        tall: true,
        ..
      }
    ));
    assert!(matches!(form(160, 30), Form::Split { tall: false, .. }));
  }
}
