//! The component gallery: every `ui` component in every variant and state, on pages that fit
//! 80×24 and spread out at 160×50.
//!
//! - **Tests** open it from `src/ui/gallery.entry.tsx` (`open('./src/ui/gallery.entry.tsx',
//!   …)` in `apps/tui/Gallery.test.tsx`); it is registered next to `App`.
//! - **The real host**: `COFFEE_UI_GALLERY=1 target/debug/coffee-tui` (or through
//!   `scripts/pty-run.py --env COFFEE_UI_GALLERY=1`, or `ssh -p 2222 gallery@localhost`, the
//!   gateway's `gallery` route) shows it over the app, with the host's
//!   palette pin and colour depths (`COFFEE_COLORS=16|mono`).
//!
//! Keys (the gallery's own small table; the app never mounts it): `← →` / `h l` / `[ ]` turn
//! pages (`Home` / `End`: first / last); `↑ ↓` / `j k` move the demo cursors; `space` ticks
//! the demo check box; `Tab` / `Shift+Tab` move the demo focus (inputs, dialog buttons); `Esc`
//! leaves a field or closes the dialog or toast; `t` shows the next toast; `o` opens the
//! demo dialog, `c` the logout confirmation; `m` turns the animations on and off (off at
//! start, so a headless test goes quiet).

use std::sync::OnceLock;

use nsh_core::{keymap::When, prelude::*};

use super::{
  address_card::{AddressView, add_card, address_card},
  art::{cup, steam},
  button::{Button, ButtonKind},
  chip::{ChipPad, group_label, section_head, segmented, sku_option},
  data_row::{
    Align, CartActions, CartForm, CartLine, Column, OrderStatus, Width, cart_row, data_row,
    name_cell, order_row, pips, table_head, table_rule,
  },
  dialog::{ConfirmText, DialogFocus, DialogSize, confirm, dialog, dialog_box},
  empty_state::{EmptyArt, empty_state_with},
  error_state::error_state,
  input::{InputForm, InputSpec, LabelPlace, input},
  kbd::{kbd, kbd_wide},
  label::{label, strong},
  layout::{self, Bp, Layout},
  leader::{leader, receipt_leader},
  loading::loading_live,
  meter::{bar, meter, progress, thin_bar},
  nav_tabs::{NavTab, nav_rule, nav_tabs_with, wide_tabs},
  panel::{card, heading, inset, panel, panel_toned, rule},
  radar::{radar, radar_fits, radar_label},
  status_bar::{Hint, Mode, StatusRight, mode_line},
  stepper::{StepperForm, max_quantity, stepper, stock_note},
  tag::{TagKind, count_badge, tag},
  theme::{self, Tone},
  toast::{ToastKind, toast, toast_box, toast_width},
  toggle::{checkbox, radio, switch_row, toggle_note},
  window::system_row,
  wordmark::{wordmark, wordmark_block, wordmark_line},
};
use crate::ui::frame::Legend;

/// The environment variable that puts the gallery over the app in the real host.
pub const GALLERY_ENV: &str = "COFFEE_UI_GALLERY";

/// Whether the process was asked to show the gallery (`COFFEE_UI_GALLERY` set, not `0`).
pub fn requested() -> bool {
  static REQUESTED: OnceLock<bool> = OnceLock::new();
  *REQUESTED
    .get_or_init(|| std::env::var(GALLERY_ENV).is_ok_and(|value| !value.is_empty() && value != "0"))
}

/// The gallery as an overlay over the app (the real host's switch): it covers the shell and
/// its keys, bound later, win.
pub fn over_app() -> AnyElement {
  overlay()
    .absolute()
    .inset(px(0.0))
    .child(
      view()
        .flex_1()
        .role(AccessibilityRole::Summary)
        .bg(theme::bg)
        .child(ui_gallery()),
    )
    .into_element()
}

/// One page of the gallery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GalleryPage {
  /// Wordmark, window, navigation.
  Brand,
  /// Panels, cards, insets, headings, rules.
  Panels,
  /// Status bars and key caps.
  StatusBar,
  /// Buttons.
  Buttons,
  /// Inputs.
  Inputs,
  /// Toggles and steppers.
  Toggles,
  /// Chips and tags.
  Chips,
  /// The product table.
  Rows,
  /// Cart and order rows.
  CartRows,
  /// Address cards.
  Addresses,
  /// Meters, the radar, leaders.
  Meters,
  /// Toasts.
  Toasts,
  /// Empty, loading and error states, art.
  States,
  /// Dialogs.
  Dialogs,
}

/// The pages in order.
pub const PAGES: [GalleryPage; 14] = [
  GalleryPage::Brand,
  GalleryPage::Panels,
  GalleryPage::StatusBar,
  GalleryPage::Buttons,
  GalleryPage::Inputs,
  GalleryPage::Toggles,
  GalleryPage::Chips,
  GalleryPage::Rows,
  GalleryPage::CartRows,
  GalleryPage::Addresses,
  GalleryPage::Meters,
  GalleryPage::Toasts,
  GalleryPage::States,
  GalleryPage::Dialogs,
];

impl GalleryPage {
  /// Its title, "中文 ENGLISH".
  pub fn title(self) -> &'static str {
    match self {
      GalleryPage::Brand => "字标 · 窗口 · 导航 WORDMARK",
      GalleryPage::Panels => "面板 PANEL",
      GalleryPage::StatusBar => "状态栏 · 键帽 STATUSBAR",
      GalleryPage::Buttons => "按钮 BUTTON",
      GalleryPage::Inputs => "输入框 INPUT",
      GalleryPage::Toggles => "开关 · 数量 TOGGLE",
      GalleryPage::Chips => "选项 · 标签 CHIP",
      GalleryPage::Rows => "数据行 DATAROW",
      GalleryPage::CartRows => "购物车行 · 订单行 ROWS",
      GalleryPage::Addresses => "地址卡 ADDRESS",
      GalleryPage::Meters => "计量 · 雷达 · 引导线 METER",
      GalleryPage::Toasts => "通知 TOAST",
      GalleryPage::States => "空 · 载入 · 错误 STATES",
      GalleryPage::Dialogs => "对话框 DIALOG",
    }
  }
}

/// The page after `index` (wrapping) and before it.
pub fn next_page(index: usize) -> usize {
  (index + 1) % PAGES.len()
}

/// The page before `index` (wrapping).
pub fn prev_page(index: usize) -> usize {
  (index + PAGES.len() - 1) % PAGES.len()
}

/// The gallery's state: the page, the demo cursor and focus, the toggles, the inputs, the
/// toast and dialogs. `Copy`.
#[derive(Clone, Copy)]
struct Demo {
  page: Signal<usize>,
  cursor: Signal<usize>,
  focus: Signal<Option<usize>>,
  checked: Signal<bool>,
  animate: Signal<bool>,
  switch_on: Signal<bool>,
  roast: Signal<usize>,
  quantity: Signal<u32>,
  username: Signal<String>,
  password: Signal<String>,
  coupon: Signal<String>,
  search: Signal<String>,
  toast: Signal<Option<usize>>,
  dialog: Signal<bool>,
  confirm: Signal<bool>,
  dialog_focus: DialogFocus,
  confirm_focus: DialogFocus,
}

const TOASTS: [(ToastKind, &str, &str); 3] = [
  (
    ToastKind::Ok,
    "已加入购物车",
    "耶加雪菲 孔加 · 250g · 整豆 × 1",
  ),
  (ToastKind::Info, "以访客身份浏览", "结算前需要登录"),
  (
    ToastKind::Warn,
    "已从购物车移除",
    "慧兰 粉红波旁 · 按 u 撤销",
  ),
];

const TABS: [NavTab; 4] = [
  NavTab {
    key: "1",
    label: "选购",
  },
  NavTab {
    key: "2",
    label: "购物车",
  },
  NavTab {
    key: "3",
    label: "地址本",
  },
  NavTab {
    key: "4",
    label: "我的",
  },
];

/// A product of the demo table.
struct Bean {
  name: &'static str,
  sub: &'static str,
  tag: Option<&'static str>,
  origin: &'static str,
  process: &'static str,
  roast: u8,
  price: &'static str,
}

const BEANS: [Bean; 3] = [
  Bean {
    name: "耶加雪菲 孔加",
    sub: "Ethiopia · Yirgacheffe Konga G1",
    tag: Some("新产季"),
    origin: "埃塞俄比亚",
    process: "水洗",
    roast: 1,
    price: "¥98",
  },
  Bean {
    name: "慧兰 粉红波旁",
    sub: "Colombia · Huila Pink Bourbon",
    tag: Some("店长推荐"),
    origin: "哥伦比亚",
    process: "水洗",
    roast: 2,
    price: "¥118",
  },
  Bean {
    name: "曼特宁 G1",
    sub: "Indonesia · Sumatra Mandheling",
    tag: None,
    origin: "印尼",
    process: "湿刨",
    roast: 5,
    price: "¥72",
  },
];

/// The flavour of the demo radar (the design's "my taste").
const TASTE: [f32; 6] = [8.0, 7.0, 4.0, 2.0, 7.0, 8.0];

/// The gallery's root: every component, page by page (see the module docs for its keys).
#[component]
pub fn UiGallery() -> impl IntoElement {
  let layout = match use_context::<Layout>() {
    Some(layout) => layout,
    None => Layout::provide(),
  };
  let demo = Demo {
    page: signal(0),
    cursor: signal(0),
    focus: signal(None),
    checked: signal(true),
    animate: signal(false),
    switch_on: signal(true),
    roast: signal(0),
    quantity: signal(1),
    username: signal("林晓".to_owned()),
    password: signal("secret12".to_owned()),
    coupon: signal(String::new()),
    search: signal(String::new()),
    toast: signal(None),
    dialog: signal(false),
    confirm: signal(false),
    dialog_focus: DialogFocus::new(2, 1),
    confirm_focus: DialogFocus::new(2, 1),
  };
  bind_keys(demo);
  view()
    .flex_1()
    .bg(theme::bg)
    .text_color(theme::fg)
    .child(system_row(|| SharedString::from("21:04")))
    .child(
      view()
        .flex_none()
        .flex_row()
        .px(layout::cols(1))
        .child(wordmark_line())
        .child(label("  UI 组件库 GALLERY", Tone::Mute))
        .child(view().flex_1())
        .child(label(
          move || {
            let page = demo.page.get();
            SharedString::from(format!(
              "{}/{} · {}",
              page + 1,
              PAGES.len(),
              PAGES[page].title()
            ))
          },
          Tone::Soft,
        )),
    )
    .child(
      view()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .px(layout::cols(1))
        .pt(layout::rows(1))
        .child(dynamic(move || {
          page_view(PAGES[demo.page.get()], demo, layout)
        })),
    )
    .child(toast(move || {
      demo
        .toast
        .get()
        .map(|i| (TOASTS[i].0, TOASTS[i].1.into(), TOASTS[i].2.into()))
    }))
    .child(dialog(
      move || demo.dialog.get(),
      "编辑地址 EDIT",
      DialogSize {
        cols: 70,
        rows: 15,
        pad: 3,
      },
      move || dialog_content(demo),
      move || {
        vec![
          Button::new(ButtonKind::Default, "Esc", "取消")
            .focused(move || demo.dialog_focus.is(0))
            .on_press(move || demo.dialog.set(false)),
          Button::new(ButtonKind::Primary, "⏎", "保存")
            .focused(move || demo.dialog_focus.is(1))
            .on_press(move || demo.dialog.set(false)),
        ]
      },
    ))
    .child(confirm(
      move || demo.confirm.get(),
      logout_words(),
      demo.confirm_focus,
      move || demo.confirm.set(false),
      move || demo.confirm.set(false),
    ))
    .child(mode_line(
      move || gallery_mode(demo),
      move || gallery_hints(demo),
      move || StatusRight {
        mouse: Some(true),
        total: Some("¥426.00".into()),
        bare_total: false,
        help: Some(Hint {
          id: "help",
          key: "?",
          text: "帮助",
        }),
      },
      move |id| press_hint(demo, id),
    ))
}

fn logout_words() -> ConfirmText {
  ConfirmText {
    title: "退出登录 LOGOUT".into(),
    question: "确定退出登录吗？".into(),
    detail: "同时解除当前公钥的免密登录。".into(),
    note: "再按 q 也可以确认".into(),
    confirm: "退出登录".into(),
  }
}

fn gallery_mode(demo: Demo) -> Mode {
  if demo.confirm.get() {
    Mode {
      zh: "确认",
      en: "CONFIRM",
    }
  } else if demo.dialog.get() {
    Mode {
      zh: "编辑",
      en: "INSERT",
    }
  } else {
    Mode {
      zh: "组件",
      en: "GALLERY",
    }
  }
}

fn gallery_hints(demo: Demo) -> Vec<Hint> {
  if demo.confirm.get() || demo.dialog.get() {
    return vec![
      Hint {
        id: "dialog.ok",
        key: "⏎",
        text: "确认",
      },
      Hint {
        id: "dialog.next",
        key: "Tab",
        text: "切换",
      },
      Hint {
        id: "dialog.close",
        key: "Esc",
        text: "取消",
      },
    ];
  }
  let mut hints = vec![
    Hint {
      id: "page.next",
      key: "←→",
      text: "翻页",
    },
    Hint {
      id: "cursor.next",
      key: "↑↓",
      text: "移动",
    },
  ];
  if Layout::get().bp() > Bp::S {
    hints.push(Hint {
      id: "toast",
      key: "t",
      text: "通知",
    });
    hints.push(Hint {
      id: "dialog",
      key: "o",
      text: "对话框",
    });
  }
  hints
}

fn press_hint(demo: Demo, id: &str) {
  match id {
    "page.next" => demo.page.update(|p| *p = next_page(*p)),
    "cursor.next" => demo.cursor.update(|c| *c = (*c + 1) % 3),
    "toast" => next_toast(demo),
    "dialog" => open_dialog(demo),
    "dialog.ok" | "dialog.close" => close_dialogs(demo),
    "dialog.next" => focus_ring(demo).map_or((), |ring| ring.next()),
    _ => {}
  }
}

fn next_toast(demo: Demo) {
  demo
    .toast
    .update(|t| *t = Some(t.map_or(0, |t| (t + 1) % TOASTS.len())));
}

fn open_dialog(demo: Demo) {
  demo.dialog_focus.set(1);
  demo.dialog.set(true);
}

fn close_dialogs(demo: Demo) {
  demo.dialog.set(false);
  demo.confirm.set(false);
}

fn focus_ring(demo: Demo) -> Option<DialogFocus> {
  if demo.confirm.get_untracked() {
    Some(demo.confirm_focus)
  } else if demo.dialog.get_untracked() {
    Some(demo.dialog_focus)
  } else {
    None
  }
}

/// Binds the gallery's keys (its own small table: the app never mounts the gallery).
fn bind_keys(demo: Demo) {
  let options = KeyOptions {
    repeat: true,
    ..KeyOptions::default()
  };
  let bind = |strokes: &[&str], run: fn(Demo)| {
    for stroke in strokes {
      on_key_with(stroke, options, move |_| run(demo));
    }
  };
  // Keys that also act while a field has the focus (nsh leaves every key a field can edit
  // with to the field unless the binding says otherwise).
  let always = KeyOptions {
    when: When::Always,
    ..options
  };
  let bind_always = |strokes: &[&str], run: fn(Demo)| {
    for stroke in strokes {
      on_key_with(stroke, always, move |_| run(demo));
    }
  };
  bind(&["right", "l", "]"], |demo| match focus_ring(demo) {
    Some(ring) => ring.next(),
    None => demo.page.update(|p| *p = next_page(*p)),
  });
  bind(&["left", "h", "["], |demo| match focus_ring(demo) {
    Some(ring) => ring.prev(),
    None => demo.page.update(|p| *p = prev_page(*p)),
  });
  bind(&["home"], |demo| demo.page.set(0));
  bind(&["end"], |demo| demo.page.set(PAGES.len() - 1));
  bind(&["down", "j"], |demo| {
    demo.cursor.update(|c| *c = (*c + 1) % 3)
  });
  bind(&["up", "k"], |demo| {
    demo.cursor.update(|c| *c = (*c + 2) % 3)
  });
  bind(&["space"], |demo| demo.checked.update(|c| *c = !*c));
  bind_always(&["tab"], |demo| match focus_ring(demo) {
    Some(ring) => ring.next(),
    None => demo
      .focus
      .update(|f| *f = Some(f.map_or(0, |f| (f + 1) % 4))),
  });
  bind_always(&["shift+tab"], |demo| match focus_ring(demo) {
    Some(ring) => ring.prev(),
    None => demo
      .focus
      .update(|f| *f = Some(f.map_or(3, |f| (f + 3) % 4))),
  });
  bind_always(&["escape"], |demo| {
    if focus_ring(demo).is_some() {
      close_dialogs(demo);
    } else if demo.focus.get_untracked().is_some() {
      demo.focus.set(None);
    } else {
      demo.toast.set(None);
    }
  });
  bind(&["enter"], |demo| {
    if focus_ring(demo).is_some() {
      close_dialogs(demo);
    }
  });
  bind(&["t"], next_toast);
  bind(&["o"], open_dialog);
  bind(&["c"], |demo| {
    demo.confirm_focus.set(1);
    demo.confirm.set(true);
  });
  bind(&["q"], |demo| {
    if demo.confirm.get_untracked() {
      demo.confirm.set(false);
    }
  });
  bind(&["m"], |demo| demo.animate.update(|a| *a = !*a));
  bind(&["s"], |demo| demo.switch_on.update(|s| *s = !*s));
  bind(&["r"], |demo| demo.roast.update(|r| *r = (*r + 1) % 4));
  bind(&["shift+=", "="], |demo| {
    demo.quantity.update(|q| *q = (*q + 1).min(max_quantity(6)))
  });
  bind(&["-", "shift+-"], |demo| {
    demo.quantity.update(|q| *q = (*q - 1).max(1))
  });
}

/// A column whose children stand a blank row apart (nsh's terminal layout turns column gaps
/// into rows only outside framed boxes, so the rows are spacers).
struct Spaced {
  inner: View,
  empty: bool,
}

impl Spaced {
  fn new() -> Self {
    Self {
      inner: view().flex_none(),
      empty: true,
    }
  }

  fn child(mut self, child: impl IntoElement) -> Self {
    if !self.empty {
      self.inner = self.inner.child(layout::vspace(1));
    }
    self.empty = false;
    self.inner = self.inner.child(child);
    self
  }

  fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
    for child in children {
      self = self.child(child);
    }
    self
  }
}

impl IntoElement for Spaced {
  fn into_element(self) -> AnyElement {
    self.inner.into_element()
  }
}

/// A titled block of a page: its heading and content.
fn block(title: &'static str, content: impl IntoElement) -> View {
  view()
    .flex_none()
    .child(strong(title, Tone::Mute))
    .child(content)
}

/// Columns side by side when the terminal is wide enough, else one under the other.
fn columns(layout: Layout, at: Bp, items: Vec<View>) -> AnyElement {
  if layout.bp() >= at {
    view()
      .flex_none()
      .flex_row()
      .gap(layout::gap_cols(2))
      .items_start()
      .children(items)
      .into_element()
  } else {
    Spaced::new().children(items).into_element()
  }
}

fn page_view(page: GalleryPage, demo: Demo, layout: Layout) -> AnyElement {
  match page {
    GalleryPage::Brand => brand_page(layout),
    GalleryPage::Panels => panels_page(layout),
    GalleryPage::StatusBar => status_page(),
    GalleryPage::Buttons => buttons_page(demo),
    GalleryPage::Inputs => inputs_page(demo, layout),
    GalleryPage::Toggles => toggles_page(demo, layout),
    GalleryPage::Chips => chips_page(demo),
    GalleryPage::Rows => rows_page(demo, layout),
    GalleryPage::CartRows => cart_page(demo, layout),
    GalleryPage::Addresses => address_page(demo, layout),
    GalleryPage::Meters => meters_page(demo, layout),
    GalleryPage::Toasts => toasts_page(layout),
    GalleryPage::States => states_page(demo, layout),
    GalleryPage::Dialogs => dialogs_page(demo),
  }
}

fn brand_page(layout: Layout) -> AnyElement {
  let badges = |i: usize| if i == 1 { 3 } else { 0 };
  Spaced::new()
    .child(block("块字 BLOCK 53×5", wordmark_block()))
    .child(block(
      "单行 HEADER 10×1（可点击：回到选购）",
      wordmark(|| {}),
    ))
    .child(block(
      "导航 NAVTABS（当前：购物车，数量胶囊 3）",
      view()
        .child(nav_tabs_with(
          &TABS,
          || Some(1),
          badges,
          move || wide_tabs(layout.bp()),
          |_| {},
        ))
        .child(nav_rule(
          &TABS,
          || Some(1),
          || vec![0, 3, 0, 0],
          0,
          move || wide_tabs(layout.bp()),
        )),
    ))
    .child(block(
      "紧排 TIGHT",
      nav_tabs_with(&TABS, || Some(0), badges, || false, |_| {}),
    ))
    .into_element()
}

fn panels_page(layout: Layout) -> AnyElement {
  let filter = panel(
    "筛选 FILTER",
    view()
      .pt(layout::rows(1))
      .child(section_head("产区", "f"))
      .child(radio("全部", Some("8".into()), || true, || {}))
      .child(radio("埃塞俄比亚", Some("1".into()), || false, || {})),
  )
  .h(layout::rows(7));
  let receipt = panel_toned(
    "订单已提交 ORDER",
    Tone::Acc2,
    view()
      .pt(layout::rows(1))
      .child(heading("豆子档案"))
      .child(leader("产区", "慧兰 · 皮塔利托", 30))
      .child(rule(true)),
  )
  .h(layout::rows(7));
  let address = inset(
    view()
      .child(
        view()
          .flex_row()
          .child(label("收货地址", Tone::Mute))
          .child(view().flex_1())
          .child(Button::new(ButtonKind::Link, "3", "更换")),
      )
      .child(strong("林晓", Tone::Fg))
      .child(label("上海市 徐汇区", Tone::Mute)),
  );
  let guest = card(
    Legend::new("我的 PROFILE"),
    40,
    Some(6),
    view()
      .child(strong("你正在以访客身份浏览", Tone::Fg))
      .child(label("登录后可以查看订单。", Tone::Mute)),
  );
  Spaced::new()
    .child(columns(
      layout,
      Bp::S,
      vec![
        view().w(layout::cols(34)).child(filter),
        view().w(layout::cols(36)).child(receipt),
      ],
    ))
    .child(columns(
      layout,
      Bp::S,
      vec![view().w(layout::cols(34)).child(address), guest],
    ))
    .child(view().w(layout::cols(40)).child(rule(false)))
    .into_element()
}

fn status_page() -> AnyElement {
  let hint = |id, key, text| Hint { id, key, text };
  let right = || StatusRight {
    mouse: Some(false),
    total: Some("¥0.00".into()),
    bare_total: false,
    help: Some(Hint {
      id: "help",
      key: "?",
      text: "帮助",
    }),
  };
  Spaced::new()
    .child(block(
      "选购 BROWSE",
      mode_line(
        || Mode {
          zh: "选购",
          en: "BROWSE",
        },
        move || {
          vec![
            hint("m", "↑↓", "移动"),
            hint("o", "⏎", "详情"),
            hint("a", "a", "加入"),
          ]
        },
        move || StatusRight {
          mouse: Some(true),
          total: Some("¥426.00".into()),
          bare_total: false,
          help: right().help,
        },
        |_| {},
      ),
    ))
    .child(block(
      "登录 LOGIN（鼠标追踪关）",
      mode_line(
        || Mode {
          zh: "登录",
          en: "LOGIN",
        },
        move || vec![hint("t", "Tab", "切换"), hint("l", "⏎", "登录")],
        right,
        |_| {},
      ),
    ))
    .child(block(
      "键帽 KBD",
      view()
        .flex_row()
        .gap(layout::gap_cols(2))
        .child(kbd("a"))
        .child(kbd("⏎"))
        .child(kbd("↑↓"))
        .child(kbd("space"))
        .child(kbd("Esc"))
        .child(kbd_wide("1 – 4"))
        .child(kbd_wide("Ctrl+C ×2")),
    ))
    .into_element()
}

fn buttons_page(demo: Demo) -> AnyElement {
  let row = |buttons: Vec<Button>| {
    view()
      .flex_row()
      .flex_wrap()
      .gap(layout::gap_cols(2))
      .pl(layout::cols(1))
      .children(buttons)
  };
  Spaced::new()
    .child(block(
      "变体 KINDS",
      view()
        .child(row(vec![
          Button::new(ButtonKind::Primary, "⏎", "查看详情"),
          Button::new(ButtonKind::Default, "a", "快速加入"),
          Button::new(ButtonKind::Outline, "b", "立即购买"),
          Button::new(ButtonKind::Danger, "q", "退出登录"),
        ]))
        .child(row(vec![
          Button::new(ButtonKind::Undo, "", "已删除 慧兰 粉红波旁 · u 撤销"),
          Button::new(ButtonKind::Green, "", "波奎特 瑰夏"),
          Button::new(ButtonKind::Link, "3", "更换"),
        ])),
    ))
    .child(block(
      "焦点 FOCUSED · 禁用 DISABLED",
      row(vec![
        Button::new(ButtonKind::Default, "e", "编辑").focused(|| true),
        Button::new(ButtonKind::Primary, "⏎", "退出登录").focused(|| true),
        Button::new(ButtonKind::Default, "space", "设为默认").disabled(|| true),
        Button::new(ButtonKind::Primary, "c", "结算").disabled(|| true),
      ]),
    ))
    .child(block(
      "撑满 WIDTH 45 / 22",
      view()
        .pl(layout::cols(1))
        .child(
          Button::new(ButtonKind::Primary, "⏎", "登录")
            .key_after()
            .width(45),
        )
        .child(
          view()
            .flex_row()
            .gap(layout::gap_cols(1))
            .child(
              Button::new(ButtonKind::Default, "g", "访客浏览")
                .key_cap()
                .width(22),
            )
            .child(
              Button::new(ButtonKind::Default, "", "注册新账号")
                .arrow()
                .width(22),
            ),
        ),
    ))
    .child(block(
      "提交中 LOADING（替换按钮）",
      view()
        .pl(layout::cols(1))
        .w(layout::cols(46))
        .child(progress(
          "校验凭据",
          || 42.0,
          45,
          Tone::Acc,
          move || demo.animate.get(),
        )),
    ))
    .into_element()
}

fn inputs_page(demo: Demo, layout: Layout) -> AnyElement {
  let focused = move |i: usize| move || demo.focus.get() == Some(i);
  let set_focus = move |i: usize| {
    move |on: bool| {
      if on {
        demo.focus.set(Some(i));
      } else if demo.focus.get_untracked() == Some(i) {
        demo.focus.set(None);
      }
    }
  };
  let username = input(
    InputSpec {
      hint: Some(std::rc::Rc::new(|| "2–16 个字符".into())),
      ..InputSpec::new(InputForm::Prompt, Some("用户名"))
    },
    move || demo.username.get(),
    focused(0),
    move || {
      demo
        .username
        .with(|u| u.is_empty())
        .then(|| "请输入用户名".into())
    },
    move |text| demo.username.set(text),
    set_focus(0),
    || {},
  );
  let password = input(
    InputSpec {
      masked: true,
      ..InputSpec::new(InputForm::Prompt, Some("密码"))
    },
    move || demo.password.get(),
    focused(1),
    || None,
    move |text| demo.password.set(text),
    set_focus(1),
    || {},
  );
  let coupon = input(
    InputSpec {
      place: LabelPlace::OnFrame,
      uppercase: true,
      placeholder: "试试 TTY10".into(),
      ..InputSpec::new(InputForm::Plain, Some("优惠码"))
    },
    move || demo.coupon.get(),
    focused(2),
    move || {
      demo
        .coupon
        .with(|c| !c.is_empty() && c != "TTY10")
        .then(|| "优惠码无效或已过期".into())
    },
    move |text| demo.coupon.set(text),
    set_focus(2),
    || {},
  );
  let search = input(
    InputSpec {
      placeholder: "搜索豆子、产区或风味…".into(),
      ..InputSpec::new(InputForm::Search, None)
    },
    move || demo.search.get(),
    focused(3),
    || None,
    move |text| demo.search.set(text),
    set_focus(3),
    || {},
  );
  let width = if layout.bp() >= Bp::M { 45 } else { 37 };
  view()
    .child(label(
      "Tab 进入输入框 · Esc 离开 · 空用户名与错误的优惠码显示错误行",
      Tone::Mute,
    ))
    .child(columns(
      layout,
      Bp::L,
      vec![
        view()
          .w(layout::cols(width))
          .child(username)
          .child(password),
        view()
          .w(layout::cols(width))
          .child(coupon)
          .child(text(" "))
          .child(search),
      ],
    ))
    .into_element()
}

fn toggles_page(demo: Demo, layout: Layout) -> AnyElement {
  let checks = view()
    .child(checkbox(
      "记住我 30 天",
      move || demo.checked.get(),
      || false,
      || false,
      move || demo.checked.update(|c| *c = !*c),
    ))
    .child(toggle_note("绑定当前 SSH 公钥，下次连接免密登录", 4))
    .child(checkbox("设为默认", || false, || false, || true, || {}))
    .child(checkbox("记住我 30 天", || false, || true, || false, || {}))
    .child(toggle_note("当前连接没有 SSH 公钥，无法记住登录", 4));
  let switches = view()
    .w(layout::cols(34))
    .gap(layout::rows(1))
    .child(switch_row(
      "界面动画",
      "m",
      "蒸汽、加载指示与进度条",
      move || demo.animate.get(),
      move || demo.animate.update(|a| *a = !*a),
    ))
    .child(switch_row(
      "鼠标追踪",
      "s",
      "悬停即移动选择高亮",
      move || demo.switch_on.get(),
      move || demo.switch_on.update(|s| *s = !*s),
    ));
  let radios = view().w(layout::cols(24)).children(
    ["全部", "埃塞俄比亚", "哥伦比亚"]
      .into_iter()
      .zip(["8", "1", "1"])
      .enumerate()
      .map(move |(i, (name, count))| {
        radio(
          name,
          Some(count.into()),
          move || demo.cursor.get() == i,
          move || demo.cursor.set(i),
        )
      }),
  );
  let (note, tone) = stock_note(26);
  let (low, low_tone) = stock_note(6);
  let steppers = view()
    .child(
      view()
        .flex_row()
        .child(stepper(
          StepperForm::Divided,
          move || demo.quantity.get(),
          1,
          || max_quantity(6),
          move || demo.quantity.update(|q| *q = (*q - 1).max(1)),
          move || demo.quantity.update(|q| *q = (*q + 1).min(6)),
        ))
        .child(label(format!("  {note}"), tone)),
    )
    .child(
      view()
        .flex_row()
        .child(stepper(StepperForm::Plain, || 6, 1, || 6, || {}, || {}))
        .child(label(format!("  {low}"), low_tone)),
    )
    .child(label(stock_note(0).0, stock_note(0).1));
  Spaced::new()
    .child(columns(
      layout,
      Bp::M,
      vec![
        block("勾选 CHECKBOX（space 切换）", checks),
        block("开关 SWITCH（m / s）", switches),
      ],
    ))
    .child(columns(
      layout,
      Bp::M,
      vec![
        block("单选 RADIO（↑↓）", radios),
        block("数量 STEPPER（+ −）", steppers),
      ],
    ))
    .into_element()
}

fn chips_page(demo: Demo) -> AnyElement {
  let weights = [
    ("100g", "¥53", true),
    ("250g", "¥118", true),
    ("500g", "¥224", true),
    ("1kg", "¥425", false),
  ];
  Spaced::new()
    .child(block(
      "分段 SEGMENTED（r 切换）",
      segmented(
        &["全部", "浅", "中", "深"],
        ChipPad::Tight,
        move || demo.roast.get(),
        move |i| demo.roast.set(i),
      ),
    ))
    .child(block(
      "排序 SORT 11 列",
      view()
        .child(segmented(
          &["推荐", "价格 ↑"],
          ChipPad::Width(11),
          || 0,
          |_| {},
        ))
        .child(segmented(
          &["价格 ↓", "杯测分"],
          ChipPad::Width(11),
          || 9,
          |_| {},
        )),
    ))
    .child(block(
      "规格 SKU（1kg 缺货）",
      view()
        .w(layout::cols(40))
        .child(group_label("重量", "← → 切换", || true))
        .child(
          view()
            .flex_row()
            .gap(layout::gap_cols(1))
            .child(layout::hspace(1))
            .children(
              weights
                .into_iter()
                .enumerate()
                .map(move |(i, (w, price, ok))| {
                  sku_option(
                    w,
                    price,
                    move || demo.cursor.get() == i,
                    ok,
                    move || demo.cursor.set(i),
                  )
                }),
            ),
        )
        .child(group_label("研磨", "← → 切换", || false))
        .child(view().flex_row().child(layout::hspace(2)).child(segmented(
          &["整豆", "手冲", "意式"],
          ChipPad::Tight,
          || 0,
          |_| {},
        ))),
    ))
    .child(block(
      "挑选 PICK · 标签 TAG",
      view()
        .child(
          view()
            .flex_row()
            .child(label("标签  ", Tone::Mute))
            .child(segmented(
              &["家", "公司", "学校"],
              ChipPad::Padded,
              || 0,
              |_| {},
            )),
        )
        .child(
          view()
            .flex_row()
            .gap(layout::gap_cols(2))
            .child(tag(TagKind::Outline, "新产季"))
            .child(tag(TagKind::Note, "茉莉"))
            .child(tag(TagKind::Solid, "店长推荐"))
            .child(tag(TagKind::Default, "默认"))
            .child(tag(TagKind::Neutral, "家"))
            .child(tag(TagKind::Outline, "本单寄往"))
            .child(count_badge(|| 3))
            .child(count_badge(|| 0)),
        ),
    ))
    .into_element()
}

fn rows_page(demo: Demo, layout: Layout) -> AnyElement {
  let short = layout.short();
  let wide = layout.bp() >= Bp::M;
  let heads = heads_static(wide);
  let rows = BEANS.iter().enumerate().map(move |(i, bean)| {
    let current = move || demo.cursor.get() == i;
    let mut cells = vec![
      name_cell(
        bean.name,
        bean.tag.map(|t| tag(TagKind::Outline, t).into_element()),
        (!short).then(|| bean.sub.into()),
        current,
        || false,
      ),
      label(bean.origin, Tone::Soft).into_element(),
    ];
    if wide {
      cells.push(label(bean.process, Tone::Mute).into_element());
    }
    cells.push(label(pips(bean.roast, 5), Tone::Acc).into_element());
    cells.push(strong(bean.price, Tone::Fg).into_element());
    data_row(&heads, current, cells, move |_| demo.cursor.set(i))
  });
  view()
    .child(label("↑↓ 移动游标：▶ + 选中底色带（单色反显）", Tone::Mute))
    .child(text(" "))
    .child(table_head(&heads_static(wide)))
    .child(table_rule())
    .child(
      view()
        .gap(layout::rows(u16::from(!short)))
        .children(rows.collect::<Vec<_>>()),
    )
    .into_element()
}

fn heads_static(wide: bool) -> Vec<Column> {
  let mut heads = vec![
    Column {
      title: "名称",
      width: Width::Flex,
      align: Align::Start,
    },
    Column {
      title: "产区",
      width: Width::Cells(12),
      align: Align::Start,
    },
  ];
  if wide {
    heads.push(Column {
      title: "处理法",
      width: Width::Cells(8),
      align: Align::Start,
    });
  }
  heads.push(Column {
    title: "烘焙",
    width: Width::Cells(6),
    align: Align::Start,
  });
  heads.push(Column {
    title: "价格 /250g",
    width: Width::Cells(10),
    align: Align::End,
  });
  heads
}

fn cart_page(demo: Demo, layout: Layout) -> AnyElement {
  let lines = [
    CartLine {
      name: "慧兰 粉红波旁".into(),
      subtitle: "哥伦比亚 · 中浅烘".into(),
      spec: "250g · 手冲 · 标准袋".into(),
      unit: "¥118".into(),
      total: "¥118.00".into(),
    },
    CartLine {
      name: "「夜班」意式拼配".into(),
      subtitle: "拼配 · 中深烘".into(),
      spec: "500g · 意式 · 标准袋".into(),
      unit: "¥167".into(),
      total: "¥167.00".into(),
    },
  ];
  let carts = lines.into_iter().enumerate().map(move |(i, line)| {
    let checked = move || i == 0 || demo.checked.get();
    cart_row(
      line,
      CartForm::at(layout.bp()),
      move || demo.cursor.get() == i,
      checked,
      move || if i == 0 { demo.quantity.get() } else { 1 },
      || 6,
      CartActions {
        toggle: Box::new(move || demo.checked.update(|c| *c = !*c)),
        dec: Box::new(move || demo.quantity.update(|q| *q = (*q - 1).max(1))),
        inc: Box::new(move || demo.quantity.update(|q| *q = (*q + 1).min(6))),
        remove: Box::new(|| {}),
        press: Box::new(move |_| demo.cursor.set(i)),
      },
    )
  });
  let widths = if layout.bp() >= Bp::M {
    [13, 6, 9, 9]
  } else {
    [12, 5, 8, 8]
  };
  view()
    .child(block(
      "购物车行 CART（space 勾选第 2 行 · + − 数量）",
      Spaced::new().children(carts.collect::<Vec<_>>()),
    ))
    .child(text(" "))
    .child(block(
      "订单行 ORDERS",
      view()
        .px(layout::pad_cols(2))
        .child(order_row(
          "RT2611180031",
          "11-18",
          "慧兰 粉红波旁 250g ×1 · 「夜班」意式拼配 500g ×1",
          "¥285.00",
          OrderStatus::Shipping,
          widths,
        ))
        .child(order_row(
          "RT2611060117",
          "11-06",
          "耶加雪菲 孔加 250g ×2",
          "¥208.00",
          OrderStatus::Delivered,
          widths,
        ))
        .child(order_row(
          "RT2611220417",
          "11-22",
          "波奎特 瑰夏 100g ×1",
          "¥133.00",
          OrderStatus::Roasting,
          widths,
        )),
    ))
    .into_element()
}

fn addresses() -> [AddressView; 2] {
  [
    AddressView {
      name: "林晓".into(),
      phone: super::address_card::mask_phone("13800005521").into(),
      tag: Some("家".into()),
      region: "上海市 徐汇区".into(),
      line: "天平路 88 弄 3 号 602 室".into(),
      default: true,
      for_order: true,
    },
    AddressView {
      name: "陈默".into(),
      phone: super::address_card::mask_phone("13900000817").into(),
      tag: Some("公司".into()),
      region: "浙江省 杭州市 西湖区".into(),
      line: "文三路 128 号 5 幢 1 单元 301".into(),
      default: false,
      for_order: false,
    },
  ]
}

fn card_actions(default: bool) -> Vec<Button> {
  vec![
    Button::new(ButtonKind::Default, "⏎", "用于本单"),
    Button::new(ButtonKind::Default, "e", "编辑"),
    Button::new(ButtonKind::Default, "space", "设为默认").disabled(move || default),
    Button::new(ButtonKind::Danger, "d", "删除"),
  ]
}

fn address_page(demo: Demo, layout: Layout) -> AnyElement {
  let [first, second] = addresses();
  let wide = layout.bp() >= Bp::Xl;
  let current = move |i: usize| move || demo.cursor.get() % 2 == i;
  if wide {
    Spaced::new()
      .child(
        view()
          .flex_row()
          .gap(layout::gap_cols(2))
          .child(address_card(
            first,
            77,
            11,
            current(0),
            card_actions(true),
            |_| {},
          ))
          .child(address_card(
            second,
            77,
            11,
            current(1),
            card_actions(false),
            |_| {},
          )),
      )
      .child(add_card(77, 11, || {}))
      .into_element()
  } else {
    let cols = layout.viewport().cols.saturating_sub(3).min(77);
    view()
      .child(address_card(
        first,
        cols,
        11,
        current(0),
        card_actions(true),
        |_| {},
      ))
      .child(address_card(
        second,
        cols,
        4,
        current(1),
        Vec::new(),
        |_| {},
      ))
      .child(add_card(cols, 3, || {}))
      .into_element()
  }
}

fn meters_page(demo: Demo, layout: Layout) -> AnyElement {
  let short = layout.short();
  let length = if short || layout.bp() <= Bp::M {
    10
  } else {
    20
  };
  let fits = move || radar_fits(layout.short(), 25);
  let names = ["酸度", "甜感", "醇厚", "苦度", "余韵", "香气"];
  let meters =
    view().children((0..6).map(move |i| meter(names[i], move || TASTE[i], length, Tone::Acc)));
  Spaced::new()
    .child(
      view()
        .flex_row()
        .gap(layout::gap_cols(2))
        .child(block(
          "雷达 RADAR（矮屏回退为计量条）",
          radar(|| TASTE, Tone::Acc2, fits, radar_label("我的口味", TASTE)),
        ))
        .child(block("风味 FLAVOUR", meters)),
    )
    .child(block(
      "进度 PROGRESS · 会员 · 免运费",
      view()
        .w(layout::cols(41))
        .child(progress(
          "同步购物车与地址本",
          || 64.0,
          41,
          Tone::Acc2,
          move || demo.animate.get(),
        ))
        .child(bar(|| 64.0, 100.0, 41, Tone::Acc))
        .child(thin_bar(|| 426.0, 199.0, 39))
        .child(thin_bar(|| 120.0, 199.0, 39)),
    ))
    .child(block(
      "引导线 LEADER",
      view()
        .child(leader("产区", "慧兰 · 皮塔利托", 31))
        .child(receipt_leader(
          "慧兰 粉红波旁",
          "250g · 手冲 · 标准袋 × 1",
          "¥118.00",
          60.min(usize::from(layout.viewport().cols) - 4),
        )),
    ))
    .into_element()
}

fn toasts_page(layout: Layout) -> AnyElement {
  let width = toast_width(layout.viewport().cols.saturating_sub(2)).min(45);
  let wide = layout.bp() >= Bp::Xl;
  let boxes: Vec<View> = TOASTS
    .iter()
    .map(|(kind, title, sub)| toast_box(*kind, (*title).into(), (*sub).into(), width))
    .collect();
  view()
    .child(label("t 弹出下一条（右下角浮层）· Esc 关闭", Tone::Mute))
    .child(text(" "))
    .child(if wide {
      view().flex_row().gap(layout::gap_cols(2)).children(boxes)
    } else {
      view().children(boxes)
    })
    .into_element()
}

fn states_page(demo: Demo, layout: Layout) -> AnyElement {
  let tall = !layout.short();
  Spaced::new()
    .child(
      view()
        .flex_row()
        .gap(layout::gap_cols(2))
        .child(when(
          move || tall,
          move || {
            block(
              "字符画 ART",
              view().child(steam(move || demo.animate.get())).child(cup()),
            )
          },
        ))
        .child(block(
          "空购物车 EMPTY",
          view().h(layout::rows(10)).child(empty_state_with(
            EmptyArt::Cup,
            "购物车是空的",
            None,
            Some(Button::new(ButtonKind::Primary, "1", "去挑豆子")),
          )),
        ))
        .child(block(
          "无结果 NO RESULTS",
          view()
            .h(layout::rows(10))
            .w(layout::cols(20))
            .child(empty_state_with(
              EmptyArt::Glyph(super::art::EMPTY_SET),
              "没有匹配的豆子",
              None,
              Some(Button::new(ButtonKind::Default, "Esc", "清除筛选")),
            )),
        )),
    )
    .child(
      view()
        .flex_row()
        .gap(layout::gap_cols(2))
        .child(block(
          "载入 LOADING（m 开动画）",
          view()
            .h(layout::rows(3))
            .w(layout::cols(24))
            .child(loading_live("正在载入订单…", move || {
              demo.animate.get()
            })),
        ))
        .child(block(
          "错误 ERROR",
          view()
            .h(layout::rows(5))
            .w(layout::cols(30))
            .child(error_state(
              "订单加载失败",
              "连接服务器失败",
              "⏎",
              "重试",
              || {},
            )),
        )),
    )
    .into_element()
}

fn dialog_content(demo: Demo) -> AnyElement {
  view()
    .child(label("收货人", Tone::Mute))
    .child(strong("陈默", Tone::Fg))
    .child(text(" "))
    .child(
      view()
        .flex_row()
        .child(label("标签  ", Tone::Mute))
        .child(segmented(
          &["家", "公司", "学校"],
          ChipPad::Padded,
          || 1,
          |_| {},
        ))
        .child(view().flex_1())
        .child(checkbox(
          "设为默认",
          move || demo.checked.get(),
          || false,
          || false,
          move || demo.checked.update(|c| *c = !*c),
        )),
    )
    .into_element()
}

fn dialogs_page(demo: Demo) -> AnyElement {
  view()
    .child(label(
      "o 打开编辑对话框 · c 打开退出确认 · Tab / ← → 在按钮间移动（焦点不离开对话框）· Esc 关闭",
      Tone::Mute,
    ))
    .child(text(" "))
    .child(dialog_box(
      "退出登录 LOGOUT",
      56,
      11,
      4,
      view()
        .child(strong("确定退出登录吗？", Tone::Fg))
        .child(label("同时解除当前公钥的免密登录。", Tone::Mute))
        .child(text(" "))
        .child(label("再按 q 也可以确认", Tone::Mute)),
      vec![
        Button::new(ButtonKind::Default, "Esc", "取消"),
        Button::new(ButtonKind::Primary, "⏎", "退出登录").focused(|| true),
      ],
    ))
    .child(dynamic(move || {
      demo
        .confirm
        .get()
        .then(|| label("（确认框已打开）", Tone::Mute))
    }))
    .into_element()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pages_wrap_both_ways() {
    assert_eq!(next_page(PAGES.len() - 1), 0);
    assert_eq!(prev_page(0), PAGES.len() - 1);
    assert_eq!(next_page(0), 1);
    let titles: std::collections::HashSet<_> = PAGES.iter().map(|p| p.title()).collect();
    assert_eq!(titles.len(), PAGES.len());
  }
}
