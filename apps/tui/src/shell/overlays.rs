//! The shell's own overlays: the `?` help (`快捷键 KEYMAP`) and the logout confirmation
//! (`退出登录 LOGOUT`, U4 / P-ui-1), drawn with `ui::dialog` / `ui::confirm` between the system
//! row and the status bar. Their keys are `commands::OVERLAYS`; their state is `UiStore`
//! (`overlay`, `logout_focus`) and `SessionStore` (`key_remembered`, `logging_out`).
//!
//! The help lists the keys of the command table in keymap.md's three groups ("帮助对话框"):
//! each entry names the commands it stands for and the keystrokes it shows, so a test proves
//! every entry against the table (`help_entries_match_the_command_table`). Its layout follows
//! the room: three columns of 33 cells (two apart) with a blank row between entries in the design's
//! 112×24; two columns, the third group split across them and no blank rows, when the
//! terminal is narrower than 111 columns (80×24 included); the mouse note and `[ Esc 关闭 ]`
//! share the last row when they fit.

use nsh_core::prelude::*;

use crate::{
  commands,
  stores::{Overlay, Stores},
  ui::{
    self, Button, ButtonKind, ConfirmText, DialogSize, Layout, Tone, Viewport, dialog::dialog_size,
    frame::line_run, label, layout, strong, text::display_width,
  },
};

/// One help entry: the key cap, what it does, the commands it stands for and the keystrokes
/// they must take (none: a key handled elsewhere, Tab by nsh, Ctrl+C by the host).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpEntry {
  /// The key cap's text (11 cells wide, centred): `↑↓ j k`.
  pub key: &'static str,
  /// What it does (`soft`).
  pub text: &'static str,
  /// The command-table ids it stands for.
  pub commands: &'static [&'static str],
  /// Keystrokes those commands take.
  pub strokes: &'static [&'static str],
}

/// A group of the help: `── 全局`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpGroup {
  /// Its title.
  pub title: &'static str,
  /// Its entries.
  pub entries: &'static [HelpEntry],
}

const fn entry(
  key: &'static str,
  text: &'static str,
  commands: &'static [&'static str],
  strokes: &'static [&'static str],
) -> HelpEntry {
  HelpEntry {
    key,
    text,
    commands,
    strokes,
  }
}

/// The help's groups (keymap.md, "帮助对话框"; `q` says it asks first, U4).
pub const HELP_GROUPS: [HelpGroup; 3] = [
  HelpGroup {
    title: "全局",
    entries: &[
      entry(
        "1 – 4",
        "切换页面 选购…我的",
        &["nav.catalog", "nav.cart", "nav.addresses", "nav.profile"],
        &["1", "2", "3", "4"],
      ),
      entry("/", "搜索豆子、产区、风味", &["app.search"], &["/"]),
      entry("?", "打开 / 关闭帮助", &[commands::HELP], &["shift+/"]),
      entry(
        "Esc",
        "返回上一级 · 关闭弹窗",
        &[
          "help.close",
          "logout.cancel",
          "detail.back",
          "dialog.cancel",
        ],
        &["escape"],
      ),
      entry("Tab", "在控件之间移动焦点", &["app.focus"], &[]),
      entry("q", "退出登录（先确认）", &["app.logout"], &["q"]),
      entry("Ctrl+C ×2", "断开连接", &[], &[]),
    ],
  },
  HelpGroup {
    title: "选购 · 详情",
    entries: &[
      entry(
        "↑↓ j k",
        "移动选择",
        &["catalog.move"],
        &["up", "down", "j", "k"],
      ),
      entry("⏎ l", "打开商品详情", &["catalog.open"], &["enter", "l"]),
      entry("a", "加入购物车", &["catalog.add", "detail.add"], &["a"]),
      entry(
        "f r s",
        "产区 / 烘焙 / 排序",
        &["catalog.origin", "catalog.roast", "catalog.sort"],
        &["f", "r", "s"],
      ),
      entry(
        "← →",
        "切换规格选项",
        &["detail.choose"],
        &["left", "right"],
      ),
      entry("+ −", "调整数量", &["detail.quantity"], &["shift+=", "-"]),
      entry("b", "立即购买", &["detail.buy"], &["b"]),
    ],
  },
  HelpGroup {
    title: "购物车 · 地址 · 设置",
    entries: &[
      entry(
        "space",
        "勾选商品 / 设为默认",
        &["cart.toggle", "addresses.default"],
        &["space"],
      ),
      entry("a", "全选 / 全不选", &["cart.all"], &["a"]),
      entry(
        "x d",
        "删除",
        &["cart.remove", "addresses.delete"],
        &["x", "d"],
      ),
      entry("u", "撤销删除", &["cart.undo"], &["u"]),
      entry("c", "去结算", &["cart.checkout"], &["c"]),
      entry(
        "n e",
        "新增 / 编辑地址",
        &["addresses.new", "addresses.edit"],
        &["n", "e"],
      ),
      entry(
        "m o",
        "动画 / 鼠标追踪 开关",
        &["profile.animations", "profile.mouse"],
        &["m", "o"],
      ),
    ],
  },
];

/// The help's title.
pub const HELP_TITLE: &str = "快捷键 KEYMAP";
/// The mouse note under the groups.
pub const MOUSE_NOTE: &str = "鼠标：悬停高亮 · 单击选中 · 双击打开 · 底栏的快捷键提示可以直接点击";
/// The design's help size.
pub const HELP_COLS: u16 = 112;
/// The design's help height.
pub const HELP_ROWS: u16 = 24;
/// A column of entries: an 11-cell key cap, a blank and the text (21 cells).
pub const COLUMN_CELLS: u16 = 33;
/// The cells between two columns.
pub const COLUMN_GAP: u16 = 2;

/// How the help lays itself out in a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpLayout {
  /// The dialog's columns and rows (narrowed to the terminal).
  pub cols: u16,
  /// Its rows.
  pub rows: u16,
  /// Blank cells between its side lines and the content.
  pub pad: u16,
  /// Columns of entries: 3 (the design) or 2.
  pub columns: u16,
  /// A blank row between entries.
  pub spaced: bool,
  /// The mouse note and `[ Esc 关闭 ]` share a row.
  pub note_inline: bool,
}

impl HelpLayout {
  /// The rows its content takes.
  pub fn content_rows(&self) -> u16 {
    let entries = 7u16;
    let groups = if self.columns == 3 {
      // Heading, a blank, the entries (a blank between each when spaced).
      2 + if self.spaced {
        entries * 2 - 1
      } else {
        entries
      }
    } else {
      // Heading + 7, a blank, heading + 4 (the third group split over both columns).
      1 + entries + 1 + 1 + 4
    };
    let blank = u16::from(self.spaced);
    // A blank, the rule, (a blank), the note (and the button's own row).
    groups + blank + 1 + blank + 1 + u16::from(!self.note_inline)
  }
}

/// The help's layout in `viewport`.
pub fn help_layout(viewport: Viewport) -> HelpLayout {
  let (cols, rows) = dialog_size(HELP_COLS, HELP_ROWS, viewport);
  let three = 3 * COLUMN_CELLS + 2 * COLUMN_GAP;
  let pad = if cols >= three + 2 + 6 { 3 } else { 2 };
  let inner = cols.saturating_sub(2 + 2 * pad);
  let columns = if inner >= three { 3 } else { 2 };
  let button = Button::new(ButtonKind::Default, "Esc", "关闭").cells();
  let note_inline = usize::from(inner) >= display_width(MOUSE_NOTE) + 2 + button;
  // Inside the frame: two blank rows on top, one under.
  let room = rows.saturating_sub(2 + 2 + 1);
  let mut layout = HelpLayout {
    cols,
    rows,
    pad,
    columns,
    spaced: true,
    note_inline,
  };
  if layout.content_rows() > room {
    layout.spaced = false;
  }
  layout
}

fn entry_row(entry: &HelpEntry) -> View {
  view()
    .flex_none()
    .flex_row()
    .h(layout::rows(1))
    .child(ui::kbd_wide(entry.key))
    .child(layout::hspace(1))
    .child(label(entry.text, Tone::Soft).number_of_lines(1))
}

/// A column: `title` (none: a blank row in its place) and `entries`.
fn column(title: Option<&'static str>, entries: &[HelpEntry], spaced: bool) -> View {
  let mut column = view()
    .flex_none()
    .w(layout::cols(COLUMN_CELLS))
    .child(match title {
      Some(title) => ui::heading(title).into_element(),
      None => layout::vspace(1).into_element(),
    });
  if spaced {
    column = column.child(layout::vspace(1));
  }
  for (index, entry) in entries.iter().enumerate() {
    if spaced && index > 0 {
      column = column.child(layout::vspace(1));
    }
    column = column.child(entry_row(entry));
  }
  column
}

/// The groups as the layout places them.
fn groups(help: HelpLayout) -> View {
  let row = view().flex_none().flex_row();
  let gap = || layout::hspace(COLUMN_GAP);
  if help.columns == 3 {
    let [a, b, c] = HELP_GROUPS;
    row
      .child(column(Some(a.title), a.entries, help.spaced))
      .child(gap())
      .child(column(Some(b.title), b.entries, help.spaced))
      .child(gap())
      .child(column(Some(c.title), c.entries, help.spaced))
  } else {
    let [a, b, c] = HELP_GROUPS;
    let (first, rest) = c.entries.split_at(3);
    row
      .child(
        view()
          .flex_none()
          .child(column(Some(a.title), a.entries, false))
          .child(layout::vspace(1))
          .child(column(Some(c.title), first, false)),
      )
      .child(gap())
      .child(
        view()
          .flex_none()
          .child(column(Some(b.title), b.entries, false))
          .child(layout::vspace(1))
          .child(column(None, rest, false)),
      )
  }
}

/// The help's body for `help` in its layout; `close` runs on a press of `[ Esc 关闭 ]`.
fn help_body(help: HelpLayout, close: impl Fn() + 'static) -> AnyElement {
  let button = Button::new(ButtonKind::Default, "Esc", "关闭").on_press(close);
  let note = label(MOUSE_NOTE, Tone::Mute).number_of_lines(1);
  let blank = || layout::vspace(u16::from(help.spaced));
  let mut body = view()
    .flex_1()
    .label("快捷键帮助")
    .child(groups(help))
    .child(blank())
    .child(line_run('─', Tone::Line))
    .child(blank());
  body = if help.note_inline {
    body.child(
      view()
        .flex_none()
        .flex_row()
        .child(view().flex_1().min_w(px(0.0)).child(note))
        .child(button),
    )
  } else {
    body
      .child(note)
      .child(view().flex_none().flex_row().justify_end().child(button))
  };
  body.into_element()
}

/// The help dialog, laid out for the terminal now.
pub fn help(stores: Stores) -> AnyElement {
  let layout = Layout::get();
  let help = computed(move || help_layout(layout.viewport()));
  dynamic(move || {
    let help = help.get();
    ui::dialog(
      || true,
      HELP_TITLE,
      DialogSize {
        cols: HELP_COLS,
        rows: HELP_ROWS,
        pad: help.pad,
      },
      move || help_body(help, move || commands::press(&stores, "help.close")),
      Vec::new,
    )
  })
  .into_element()
}

/// The logout confirmation's text: its second line follows 记住我 (`remembered`).
pub fn logout_text(remembered: bool) -> ConfirmText {
  ConfirmText {
    title: "退出登录 LOGOUT".into(),
    question: "确定退出登录吗？".into(),
    detail: if remembered {
      "同时解除当前公钥的免密登录。"
    } else {
      "购物车、地址本与订单都会保留在账号里。"
    }
    .into(),
    note: "再按 q 也可以确认".into(),
    confirm: "退出登录".into(),
  }
}

/// The logout confirmation (56×11): `ui::confirm` on `UiStore::logout_focus`, its buttons
/// pressing the commands their keys run; once confirmed, the buttons make way for
/// `⠋ 正在退出…` until the session ends.
pub fn logout(stores: Stores) -> AnyElement {
  let session = stores.session;
  let words = logout_text(untracked(|| session.key_remembered()));
  let busy = words.clone();
  view()
    .flex_none()
    .child(ui::confirm(
      move || !session.logging_out(),
      words,
      stores.ui.logout_focus(),
      move || commands::press(&stores, "logout.cancel"),
      move || commands::press(&stores, "logout.confirm"),
    ))
    .child(ui::dialog(
      move || session.logging_out(),
      busy.title.clone(),
      DialogSize {
        cols: ui::dialog::CONFIRM_COLS,
        rows: ui::dialog::CONFIRM_ROWS,
        pad: 4,
      },
      move || {
        view()
          .flex_1()
          .child(strong(busy.question.clone(), Tone::Fg))
          .child(label(busy.detail.clone(), Tone::Mute))
          .child(view().flex_1())
          .child(
            view()
              .flex_none()
              .flex_row()
              .justify_end()
              .child(ui::spinner(move || stores.settings.animations()))
              .child(label(" 正在退出…", Tone::Acc)),
          )
          .into_element()
      },
      Vec::new,
    ))
    .into_element()
}

/// Whether `overlay` is drawn by the shell (the others by their pages).
pub fn is_shell_overlay(overlay: Overlay) -> bool {
  matches!(overlay, Overlay::Help | Overlay::Logout)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::Command;

  #[test]
  fn help_entries_match_the_command_table() {
    for group in HELP_GROUPS {
      assert_eq!(group.entries.len(), 7, "{}", group.title);
      for entry in group.entries {
        let commands: Vec<&Command> = entry
          .commands
          .iter()
          .map(|id| commands::by_id(id).unwrap_or_else(|| panic!("{id} is not in the table")))
          .collect();
        for stroke in entry.strokes {
          assert!(
            commands.iter().any(|c| c.key.strokes.contains(stroke)),
            "{}: no command of {:?} takes {stroke}",
            entry.key,
            entry.commands
          );
        }
        assert!(display_width(entry.key) <= 11, "{}", entry.key);
        assert!(
          11 + 1 + display_width(entry.text) <= usize::from(COLUMN_CELLS),
          "{} is wider than a column",
          entry.text
        );
      }
    }
    // Every global key of the table is in the help.
    for command in commands::GLOBAL {
      assert!(
        HELP_GROUPS[0]
          .entries
          .iter()
          .any(|e| e.commands.contains(&command.id)),
        "{} is not in the help",
        command.id
      );
    }
  }

  #[test]
  fn the_help_fits_every_terminal() {
    let xl = help_layout(Viewport::new(160, 50));
    assert_eq!(
      xl,
      HelpLayout {
        cols: 112,
        rows: 24,
        pad: 3,
        columns: 3,
        spaced: true,
        note_inline: true
      }
    );
    assert_eq!(xl.content_rows(), 19, "the design's 24 rows");
    let s = help_layout(Viewport::new(80, 24));
    assert_eq!((s.cols, s.rows, s.columns), (76, 22, 2));
    assert!(!s.spaced && !s.note_inline);
    let m = help_layout(Viewport::new(100, 30));
    assert_eq!((m.columns, m.note_inline), (2, true));
    for (cols, rows) in [
      (80, 24),
      (99, 24),
      (100, 30),
      (115, 24),
      (130, 40),
      (160, 24),
    ] {
      let help = help_layout(Viewport::new(cols, rows));
      let inner = help.cols - 2 - 2 * help.pad;
      let width = help.columns * COLUMN_CELLS + (help.columns - 1) * COLUMN_GAP;
      assert!(width <= inner, "{cols}×{rows}: {help:?}");
      assert!(
        help.content_rows() <= help.rows - 5,
        "{cols}×{rows}: {help:?}"
      );
    }
  }

  #[test]
  fn the_logout_text_follows_remember_me() {
    assert_eq!(logout_text(true).detail, "同时解除当前公钥的免密登录。");
    assert_eq!(
      logout_text(false).detail,
      "购物车、地址本与订单都会保留在账号里。"
    );
    assert!(is_shell_overlay(Overlay::Logout) && !is_shell_overlay(Overlay::AddressDialog));
  }
}
