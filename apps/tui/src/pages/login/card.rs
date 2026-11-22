//! The login / register card (01-login.md, "登录卡内部", "注册", "小尺寸"): one card that
//! switches between `登录 · SIGN IN` and `注册 · SIGN UP` in place, in two sizes.
//!
//! ```text
//! ╭─ 登录 · SIGN IN ──────────────────╮      ╭─ 登录 · SIGN IN ───────────────────╮
//! │                                   │      │  欢迎回来  登录以同步购物车、地址本与订单。│
//! │    欢迎回来                        │      │                                    │
//! │    登录以同步购物车、地址本与订单。   │      │  ┏━ 用户名 ━━━━━━━━━━━━━━━━━━━━━━━━┓  │
//! │                                   │      │  ┃ › 林晓                          ┃  │
//! │    用户名            (2–16 个字符)  │      │  ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛  │
//! │    ┏━━━━━━━━━━━━━━━━━━━━━━━━━┓    │      │  ╭─ 密码 ───────────────────────────╮  │
//! │    ┃ › 林晓                   ┃    │      │  │ › ••••••••••                    │  │
//! │    ┗━━━━━━━━━━━━━━━━━━━━━━━━━┛    │      │  ╰─────────────────────────────────╯  │
//! │    …                              │      │  [✓] 记住我 30 天 · 绑定当前 SSH 公钥  │
//! ```
//!
//! The full card (55 columns, its content 45 wide) stands beside the tall brand column; the
//! compact one (55 × 17, content 49 wide, labels on the input frames, title and subtitle on
//! one row, no Tab line, the 记住我 note on its row) serves short and narrow terminals.
//!
//! Focus: the six controls are nsh's Tab stops in the design's order (the two fields, and a
//! focusable box around 记住我 and each button); each reports its focus to
//! `SessionStore::form_focus`, and takes the focus when the store puts it there (Esc, the
//! first focus, a refused password, a rebuilt card). The buttons show it with their `›`.

use std::{cell::Cell, rc::Rc, time::Duration};

use nsh_core::prelude::*;

use super::commands::press_control;
use crate::{
  api::ErrorCode,
  stores::{LoginMode, SessionStore, Stores, session::FormControl},
  ui::{
    self, Button, ButtonKind, FrameStyle, InputForm, InputSpec, LabelPlace, Legend, Tone, layout,
  },
};

/// The card's width.
pub const CARD_COLS: u16 = 55;
/// The full card's content width.
const FULL_COLS: u16 = 45;
/// The compact card's content width.
const COMPACT_COLS: u16 = 49;
/// A submit shorter than this never shows the progress block (01-login.md, "提交中").
pub const PROGRESS_DELAY: Duration = Duration::from_millis(150);

/// The card's copy in one mode (copy.md, "登录与注册").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardCopy {
  /// The legend: `登录 · SIGN IN`.
  pub legend: &'static str,
  /// The title: `欢迎回来`.
  pub title: &'static str,
  /// The subtitle of the full card.
  pub subtitle: &'static str,
  /// The subtitle of the compact card.
  pub short_subtitle: &'static str,
  /// The primary button's label (its key `⏎` follows it).
  pub submit: &'static str,
  /// The second default button.
  pub switch: &'static str,
  /// The switch button ends in ` →`.
  pub switch_arrow: bool,
  /// The notes at the end of the field labels' rows.
  pub hints: (Option<&'static str>, Option<&'static str>),
}

/// The card's copy for `mode`.
pub fn copy(mode: LoginMode) -> CardCopy {
  match mode {
    LoginMode::Login => CardCopy {
      legend: "登录 · SIGN IN",
      title: "欢迎回来",
      subtitle: "登录以同步购物车、地址本与订单。",
      short_subtitle: "登录以同步购物车、地址本与订单。",
      submit: "登录",
      switch: "注册新账号",
      switch_arrow: true,
      hints: (None, None),
    },
    LoginMode::Register => CardCopy {
      legend: "注册 · SIGN UP",
      title: "创建账号",
      subtitle: "只需要用户名和密码，不收集邮箱或手机号。",
      short_subtitle: "只需要用户名和密码。",
      submit: "注册并登录",
      switch: "← 已有账号，去登录",
      switch_arrow: false,
      hints: (Some("2–16 个字符"), Some("至少 6 位")),
    },
  }
}

/// `记住我 30 天`.
pub const REMEMBER: &str = "记住我 30 天";
/// The 记住我 note with a key, and without one.
pub const REMEMBER_NOTE: (&str, &str) = (
  "绑定当前 SSH 公钥，下次连接免密登录",
  "当前连接没有 SSH 公钥，无法记住登录",
);
/// The compact card's 记住我 note, on the check box's row (copy.md: "矮屏合并为 `记住我 30 天 ·
/// 绑定当前 SSH 公钥`"; without a key the same cut of the long note).
pub const REMEMBER_SHORT_NOTE: (&str, &str) = (" · 绑定当前 SSH 公钥", " · 当前连接没有 SSH 公钥");
/// The full card's key line.
pub const KEYS_LINE: &str = "Tab 切换输入 · ⏎ 提交 · ? 查看全部快捷键";
/// The card's demo notice (D8).
pub const DEMO_PASSWORD: &str = "! 演示环境 · 请勿使用真实密码";
/// What the subtitle says while a remembered key is being tried.
pub const RESUMING: &str = "正在通过 SSH 公钥恢复登录…";

/// The control an error line is about: the field to fix.
pub fn error_target(message: &str) -> Option<FormControl> {
  let is = |code: ErrorCode| message == code.message();
  if is(ErrorCode::UsernameRequired)
    || is(ErrorCode::UsernameInvalid)
    || is(ErrorCode::UsernameTaken)
  {
    Some(FormControl::Username)
  } else if is(ErrorCode::PasswordTooShort) || is(ErrorCode::InvalidCredentials) {
    Some(FormControl::Password)
  } else {
    None
  }
}

/// Moves the focus with the submits: off the card while one runs (only `?` works then, and
/// nothing types), then to the field an error names, else back where it was.
pub fn follow_submit(session: SessionStore) {
  let state = computed(move || (session.submitting(), session.draft().error));
  let saved = Cell::new(None);
  let before: Cell<(bool, Option<String>)> = Cell::new((false, None));
  effect(move || {
    let (busy, error) = state.get();
    let (was_busy, was_error) = before.replace((busy, error.clone()));
    untracked(|| {
      if busy {
        if !was_busy {
          saved.set(session.form_focus_untracked());
          session.set_form_focus(None);
        }
        return;
      }
      let fresh = error.is_some() && (was_busy || error != was_error);
      let target = fresh
        .then(|| error.as_deref().and_then(error_target))
        .flatten();
      if let Some(target) = target {
        session.set_form_focus(Some(target));
      } else if was_busy {
        session.set_form_focus(saved.take());
      }
    });
  });
}

/// Set once the view made now is on screen: an element ref reaches its node only then (a
/// stop is a view, which has no `auto_focus`).
fn mounted() -> Signal<bool> {
  let mounted = signal(false);
  set_timeout(Duration::ZERO, move || mounted.set(true));
  mounted
}

/// A press with the mouse: as the key does, unless something is submitting.
fn click(stores: Stores, control: FormControl) -> impl Fn() + Clone + 'static {
  move || {
    if !untracked(|| stores.busy()) {
      batch(|| press_control(&stores, control));
    }
  }
}

/// A Tab stop around a control that is not a text field: it takes nsh's focus while the
/// store's focus is on `control`, and reports the focus Tab or a click gives it.
fn stop(stores: Stores, control: FormControl, focusable: bool, child: impl IntoElement) -> View {
  let session = stores.session;
  let handle = element_ref();
  let mounted = mounted();
  // Whether nsh's focus is here as far as this stop knows: a blur that comes after the store
  // moved the focus away (and maybe back) is stale and changes nothing.
  let held = Rc::new(Cell::new(false));
  // What the effect last asked of nsh; a focus nsh gave (Tab, a click) counts as asked, so
  // the effect does not ask again: a second Tab in the same frame would lose to it.
  let was = Rc::new(Cell::new(false));
  let (left, asked) = (held.clone(), was.clone());
  effect(move || {
    let now = mounted.get() && session.form_focus() == Some(control);
    if now && !asked.get() {
      handle.focus();
    } else if !now && asked.get() {
      left.set(false);
      handle.blur();
    }
    asked.set(now);
  });
  let (focus, given) = (held.clone(), was.clone());
  view()
    .flex_none()
    .flex_row()
    .focusable(focusable)
    // While a submit runs only `?` works (K2): Tab skips the stop.
    .disabled(move || session.submitting())
    .element_ref(&handle)
    .on_focus(move || {
      focus.set(true);
      given.set(true);
      session.set_form_focus(Some(control));
    })
    .on_blur(move || {
      if held.replace(false) {
        was.set(false);
        session.form_focus_left(control);
      }
    })
    .child(child)
}

/// A text field of the card, named `name` in the shell's focus tracking.
fn field(
  stores: Stores,
  control: FormControl,
  name: &'static str,
  spec: InputSpec,
  value: impl Fn() -> String + Copy + 'static,
  on_change: impl Fn(String) + Copy + 'static,
) -> View {
  let session = stores.session;
  let ui_store = stores.ui;
  // The field takes the focus as it mounts when the ring is on it (`ui::input`'s
  // `auto_focus`), and its handle lets Esc blur it in the same commit.
  let handle = element_ref();
  let wanted = move || session.form_focus() == Some(control);
  // As for a stop: a blur reported after the store moved the focus away is stale. The shell
  // hears at once that the keys are the page's again.
  let held = Rc::new(Cell::new(false));
  let left = held.clone();
  effect(move || {
    if !wanted() && left.replace(false) {
      ui_store.field_blurred(name);
    }
  });
  // A field that goes with the focus in it (the card rebuilt, the page left) reports no
  // blur: the keys go back to the page.
  cleanup(move || ui_store.field_blurred(name));
  ui::input(
    InputSpec {
      element: Some(handle),
      // While a submit runs only `?` works (K2): Tab does not reach the field.
      disabled: Some(Rc::new(move || session.submitting())),
      ..spec
    },
    value,
    wanted,
    || None,
    on_change,
    move |focused| {
      batch(|| {
        if focused {
          held.set(true);
          session.set_form_focus(Some(control));
          ui_store.field_focused(name, Some(handle));
        } else if held.replace(false) {
          session.form_focus_left(control);
          ui_store.field_blurred(name);
        }
      })
    },
    // ⏎ is the command table's (`login.submit`), which runs before the field hears it.
    || {},
  )
}

fn fields(stores: Stores, compact: bool, mode: LoginMode) -> View {
  let session = stores.session;
  let (user_hint, password_hint) = copy(mode).hints;
  let spec = |label: &'static str, hint: Option<&'static str>, masked: bool| InputSpec {
    place: if compact {
      LabelPlace::OnFrame
    } else {
      LabelPlace::Above
    },
    hint: if compact { None } else { hint.map(Into::into) },
    masked,
    ..InputSpec::new(InputForm::Prompt, Some(label))
  };
  view()
    .flex_none()
    .child(field(
      stores,
      FormControl::Username,
      super::USERNAME_FIELD,
      spec("用户名", user_hint, false),
      move || session.draft().username,
      move |text| session.set_username(&text),
    ))
    .child((!compact).then(|| layout::vspace(1)))
    .child(field(
      stores,
      FormControl::Password,
      super::PASSWORD_FIELD,
      spec("密码", password_hint, true),
      move || session.draft().password,
      move |text| session.set_password(&text),
    ))
}

/// The subtitle, or the resume notice while a remembered key is tried.
fn subtitle(stores: Stores, compact: bool) -> AnyElement {
  let session = stores.session;
  let settings = stores.settings;
  dynamic(move || {
    if session.resuming() {
      view()
        .flex_none()
        .flex_row()
        .child(ui::spinner(move || settings.animations()))
        .child(layout::hspace(1))
        .child(ui::label(RESUMING, Tone::Soft))
        .into_element()
    } else {
      ui::label(
        move || {
          let copy = copy(session.mode());
          SharedString::from(if compact {
            copy.short_subtitle
          } else {
            copy.subtitle
          })
        },
        Tone::Mute,
      )
      .into_element()
    }
  })
}

fn title(stores: Stores) -> Text {
  let session = stores.session;
  ui::strong(
    move || SharedString::from(copy(session.mode()).title),
    Tone::Fg,
  )
}

fn remember(stores: Stores, compact: bool) -> View {
  let session = stores.session;
  let has_key = session.has_key();
  let pick = |(with, without): (&'static str, &'static str)| if has_key { with } else { without };
  let check = ui::checkbox(
    REMEMBER,
    move || session.draft().remember,
    move || !has_key,
    move || session.form_focus() == Some(FormControl::Remember),
    click(stores, FormControl::Remember),
  );
  let check = stop(stores, FormControl::Remember, has_key, check);
  if compact {
    view()
      .flex_none()
      .flex_row()
      .child(check)
      .child(ui::label(pick(REMEMBER_SHORT_NOTE), Tone::Mute))
  } else {
    view()
      .flex_none()
      .child(check)
      .child(ui::toggle_note(pick(REMEMBER_NOTE), 4))
  }
}

/// The primary button, or the progress block while a submit runs (after
/// [`PROGRESS_DELAY`]).
fn primary(stores: Stores, width: u16) -> View {
  let session = stores.session;
  let settings = stores.settings;
  let submitting = computed(move || session.submitting());
  let mode = computed(move || session.mode());
  let button = move |mode: LoginMode| {
    Button::new(ButtonKind::Primary, "⏎", copy(mode).submit)
      .key_after()
      .width(width)
      .focused(move || session.form_focus() == Some(FormControl::Submit))
      .on_press(click(stores, FormControl::Submit))
  };
  let progress = move || {
    ui::progress(
      move || SharedString::from(session.step().map_or("", |step| step.label())),
      move || session.step().map_or(0.0, |step| f32::from(step.percent())),
      usize::from(width),
      Tone::Acc,
      move || settings.animations(),
    )
  };
  stop(
    stores,
    FormControl::Submit,
    true,
    dynamic(move || {
      if submitting.get() {
        untracked(|| {
          let shown = signal(false);
          set_timeout(PROGRESS_DELAY, move || shown.set(true));
          let mode = mode.get_untracked();
          dynamic(move || {
            if shown.get() {
              progress().into_element()
            } else {
              button(mode).into_element()
            }
          })
          .into_element()
        })
      } else {
        let mode = mode.get();
        untracked(|| button(mode).into_element())
      }
    }),
  )
}

fn secondary(stores: Stores, width: u16) -> View {
  let session = stores.session;
  let mode = computed(move || session.mode());
  let focused = move |control| move || session.form_focus() == Some(control);
  let guest = Button::new(ButtonKind::Default, "g", "访客浏览")
    .key_cap()
    .width(width)
    .focused(focused(FormControl::Guest))
    .on_press(click(stores, FormControl::Guest));
  let switch = dynamic(move || {
    let copy = copy(mode.get());
    untracked(|| {
      let button = Button::new(ButtonKind::Default, "", copy.switch).width(width);
      let button = if copy.switch_arrow {
        button.arrow()
      } else {
        button
      };
      button
        .focused(focused(FormControl::SwitchMode))
        .on_press(click(stores, FormControl::SwitchMode))
        .into_element()
    })
  });
  view()
    .flex_none()
    .flex_row()
    .child(stop(stores, FormControl::Guest, true, guest))
    .child(layout::hspace(1))
    .child(stop(stores, FormControl::SwitchMode, true, switch))
}

/// The error line and the blank row under it, while the form has an error.
fn error(stores: Stores) -> AnyElement {
  let session = stores.session;
  let error = computed(move || session.draft().error);
  dynamic(move || {
    error.get().map(|message| {
      let message = SharedString::from(message);
      view()
        .flex_none()
        .child(ui::error_line(move || Some(message.clone())))
        .child(layout::vspace(1))
    })
  })
}

fn centred(content: impl IntoElement) -> View {
  view()
    .flex_none()
    .flex_row()
    .justify_center()
    .child(content)
}

/// The card: full (`compact` false) or compact.
pub fn card(stores: Stores, compact: bool) -> View {
  let session = stores.session;
  let mode = computed(move || session.mode());
  let legend = Legend::new(move || SharedString::from(copy(session.mode()).legend));
  let width = if compact { COMPACT_COLS } else { FULL_COLS };
  let fields = dynamic(move || {
    let mode = mode.get();
    untracked(|| fields(stores, compact, mode))
  });
  let head = if compact {
    view()
      .flex_none()
      .flex_row()
      .child(title(stores))
      .child(layout::hspace(2))
      .child(subtitle(stores, true))
  } else {
    view()
      .flex_none()
      .child(layout::vspace(1))
      .child(title(stores))
      .child(subtitle(stores, false))
  };
  let content = view()
    .flex_none()
    .child(head)
    .child(layout::vspace(1))
    .child(fields)
    .child((!compact).then(|| layout::vspace(1)))
    .child(remember(stores, compact))
    .child(layout::vspace(1))
    .child(error(stores))
    .child(primary(stores, width))
    .child(layout::vspace(1))
    .child(secondary(stores, (width - 1) / 2))
    .child(layout::vspace(1))
    .child((!compact).then(|| centred(ui::label(KEYS_LINE, Tone::Mute))))
    .child(centred(ui::strong(DEMO_PASSWORD, Tone::Acc)));
  if compact {
    view()
      .flex_none()
      .w(layout::cols(CARD_COLS))
      .child(ui::frame(
        || FrameStyle::Round,
        || Tone::Line,
        Some(legend),
        None,
        2,
        content,
      ))
  } else {
    ui::card(legend, CARD_COLS, None, content)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn errors_point_at_the_field_to_fix() {
    assert_eq!(error_target("请输入用户名"), Some(FormControl::Username));
    assert_eq!(
      error_target("这个用户名已被注册"),
      Some(FormControl::Username)
    );
    assert_eq!(
      error_target("用户名或密码不正确"),
      Some(FormControl::Password)
    );
    assert_eq!(error_target("密码至少 6 位"), Some(FormControl::Password));
    assert_eq!(error_target("连接服务器失败，按 ⏎ 重试"), None);
  }

  #[test]
  fn the_copy_fits_the_cards() {
    for mode in [LoginMode::Login, LoginMode::Register] {
      let copy = copy(mode);
      assert!(display_width(copy.subtitle) <= usize::from(FULL_COLS));
      // The compact card's title row: `欢迎回来  登录以同步…`.
      assert!(
        display_width(copy.title) + 2 + display_width(copy.short_subtitle)
          <= usize::from(COMPACT_COLS)
      );
      let switch = Button::new(ButtonKind::Default, "", copy.switch);
      let switch = if copy.switch_arrow {
        switch.arrow()
      } else {
        switch
      };
      assert!(switch.cells() <= usize::from(FULL_COLS - 1) / 2);
    }
    for note in [REMEMBER_SHORT_NOTE.0, REMEMBER_SHORT_NOTE.1] {
      // `[✓] 记住我 30 天` and the note on one row of the compact card.
      assert!(4 + display_width(REMEMBER) + display_width(note) <= usize::from(COMPACT_COLS));
    }
    for note in [REMEMBER_NOTE.0, REMEMBER_NOTE.1] {
      assert!(4 + display_width(note) <= usize::from(FULL_COLS));
    }
    assert!(display_width(KEYS_LINE) <= usize::from(FULL_COLS));
  }
}
