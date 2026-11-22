//! The status bar: the design's mode line (`ui::mode_line`, StatusBar in components.md).
//!
//! ```text
//! ▐ 选购 BROWSE ▌ ‹↑↓› 移动  ‹⏎› 详情  ‹a› 加入  ‹f› 产区  ‹r› 烘焙  ‹s› 排序  ‹/› 搜索     ● 鼠标   购物车 ¥426.00   ‹?› 帮助
//! ▐ 选购 BROWSE ▌ ‹↑↓› 移动  ‹⏎› 详情  ‹a› 加入  ‹f› 筛选  ‹?› 帮助       ¥426.00            (S)
//! ```
//!
//! The mode is the overlay's or the page's (`commands::mode`); the hints are the command
//! table's for the context now (`commands::footer`: 7 at L/XL, 5 at M, 4 at S), fitted to the
//! row. The right side is `● 鼠标` (鼠标追踪), `购物车 ¥{合计}` (the server's total) and
//! `‹?› 帮助`. When the hints do not fit beside it, `● 鼠标` goes first, then the total; the
//! help hint stays while `?` works. At S the help hint follows the four page hints and the
//! right side keeps only the total (hidden when it does not fit). A press on a hint runs its
//! command, as its key does. The demo notice lives in the system row.

use nsh_core::prelude::*;

use crate::{
  commands::{self, Command, FooterHint, HELP, HINT_GAP, KeyContext},
  pages::format,
  stores::Stores,
  ui::{Bp, Hint, Layout, Mode, StatusRight, mode_line},
};

/// The `ui` mode of a command-table mode.
pub fn ui_mode(mode: commands::Mode) -> Mode {
  Mode {
    zh: mode.zh,
    en: mode.en,
  }
}

/// The `ui` hint of a command-table hint.
fn ui_hint(hint: FooterHint) -> Hint {
  Hint {
    id: hint.id,
    key: hint.key,
    text: hint.text,
  }
}

/// The help hint `‹?› 帮助`.
pub fn help_hint() -> Hint {
  let help = commands::by_id(HELP).expect("the command table has the help command");
  Hint {
    id: help.id,
    key: help.key.glyph,
    text: help.text(),
  }
}

/// What the status bar shows besides its mode: the hints and the right side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
  /// The hints, in order (at S the help hint last).
  pub hints: Vec<Hint>,
  /// The right side.
  pub right: StatusRight,
}

/// The status bar in `ctx` at width class `bp` on a row `cols` wide showing `mode`; `mouse`
/// is 鼠标追踪, `total` the cart's total as written, `enabled` says which commands can run.
pub fn bar(
  ctx: &KeyContext,
  bp: Bp,
  cols: usize,
  mode: Mode,
  mouse: bool,
  total: &str,
  enabled: impl Fn(&Command) -> bool + Copy,
) -> Bar {
  let help = commands::by_id(HELP).is_some_and(|c| c.active(ctx) && enabled(c));
  let lead = mode.width() + 1;
  let total = Some(total.to_owned().into());
  if bp <= Bp::S {
    let help_room = if help {
      help_hint().width() + HINT_GAP
    } else {
      0
    };
    let room = cols.saturating_sub(lead + help_room);
    let mut hints: Vec<Hint> = commands::footer(ctx, bp, room, enabled)
      .into_iter()
      .map(ui_hint)
      .collect();
    if help && cols >= lead + help_hint().width() {
      hints.push(help_hint());
    }
    return Bar {
      hints,
      right: StatusRight {
        mouse: None,
        total,
        bare_total: true,
        help: None,
      },
    };
  }
  let help = help.then(help_hint);
  let wanted = commands::footer(ctx, bp, usize::MAX, enabled).len();
  // components.md: short of room, `● 鼠标` goes first, then the word `购物车` (the amount
  // stays), then the total.
  let sides = [
    StatusRight {
      mouse: Some(mouse),
      total: total.clone(),
      bare_total: false,
      help,
    },
    StatusRight {
      mouse: None,
      total: total.clone(),
      bare_total: false,
      help,
    },
    StatusRight {
      mouse: None,
      total,
      bare_total: true,
      help,
    },
    StatusRight {
      mouse: None,
      total: None,
      bare_total: false,
      help,
    },
  ];
  let fitted = |right: &StatusRight| {
    // The hints start one cell after the mode; two cells at least before the right side.
    let room = cols.saturating_sub(lead + 2 + right.width(bp));
    commands::footer(ctx, bp, room, enabled)
  };
  let (right, hints) = sides
    .iter()
    .map(|right| (right, fitted(right)))
    .find(|(_, hints)| hints.len() == wanted)
    .unwrap_or_else(|| (&sides[3], fitted(&sides[3])));
  Bar {
    hints: hints.into_iter().map(ui_hint).collect(),
    right: right.clone(),
  }
}

/// The status bar.
pub fn footer(stores: Stores, layout: Layout) -> View {
  let mode = computed(move || ui_mode(commands::mode(&stores)));
  let shown = computed(move || {
    let viewport = layout.viewport();
    let ctx = commands::context(&stores);
    bar(
      &ctx,
      viewport.bp(),
      usize::from(viewport.cols),
      mode.get(),
      stores.settings.mouse_tracking(),
      &format::yuan_cents(stores.cart.total()),
      |command| (command.enabled)(&stores),
    )
  });
  mode_line(
    move || mode.get(),
    move || shown.with(|bar| bar.hints.clone()),
    move || shown.with(|bar| bar.right.clone()),
    move |id| commands::press(&stores, id),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{router::Page, stores::Overlay, ui::status_bar::right_fits};

  fn labels(bar: &Bar) -> Vec<String> {
    bar
      .hints
      .iter()
      .map(|h| format!("{} {}", h.key, h.text))
      .collect()
  }

  const BROWSE: Mode = Mode {
    zh: "选购",
    en: "BROWSE",
  };

  fn browse(bp: Bp, cols: usize) -> Bar {
    bar(
      &KeyContext::on(Page::Catalog),
      bp,
      cols,
      BROWSE,
      true,
      "¥426.00",
      |c| c.id != "catalog.retry",
    )
  }

  #[test]
  fn the_design_bar_at_160_columns() {
    let bar = browse(Bp::Xl, 160);
    assert_eq!(
      labels(&bar),
      [
        "↑↓ 移动",
        "⏎ 详情",
        "a 加入",
        "f 产区",
        "r 烘焙",
        "s 排序",
        "/ 搜索"
      ]
    );
    assert_eq!(bar.right.mouse, Some(true));
    assert_eq!(bar.right.total.as_deref(), Some("¥426.00"));
    assert_eq!(bar.right.help, Some(help_hint()));
    assert!(right_fits(160, BROWSE, &bar.hints, &bar.right, Bp::Xl));
  }

  #[test]
  fn at_80_columns_four_hints_then_help_then_the_total() {
    let bar = browse(Bp::S, 80);
    assert_eq!(
      labels(&bar),
      ["↑↓ 移动", "⏎ 详情", "a 加入", "f 筛选", "? 帮助"]
    );
    assert_eq!(
      bar.right.at(Bp::S).width(Bp::S),
      crate::ui::text::display_width("¥426.00 ")
    );
    assert!(right_fits(80, BROWSE, &bar.hints, &bar.right, Bp::S));
  }

  #[test]
  fn the_right_side_gives_way_to_the_hints() {
    // 100 columns: six hints (keymap.md); the right side keeps the amount alone.
    let m = browse(Bp::M, 100);
    assert_eq!(m.hints.len(), 6);
    assert_eq!(m.right.mouse, None);
    assert!(m.right.bare_total && m.right.total.is_some());
    assert!(right_fits(100, BROWSE, &m.hints, &m.right, Bp::M));
    // A long mode and five hints on 100 columns: the mouse state goes first.
    let long = Mode {
      zh: "地址本",
      en: "ADDRESS",
    };
    let ctx = KeyContext::on(Page::Addresses);
    let member = |c: &Command| {
      !matches!(
        c.id,
        "addresses.login" | "addresses.tabs" | "addresses.search" | "addresses.retry"
      )
    };
    let address = bar(&ctx, Bp::M, 100, long, true, "¥1,426.00", member);
    assert_eq!(address.hints.len(), 6, "{:?}", labels(&address));
    assert_eq!(address.right.mouse, None);
    assert!(address.right.help.is_some());
    assert!(right_fits(100, long, &address.hints, &address.right, Bp::M));
  }

  #[test]
  fn the_help_hint_shows_only_where_help_works() {
    let help = KeyContext {
      overlay: Some(Overlay::Help),
      ..KeyContext::on(Page::Catalog)
    };
    let mode = ui_mode(commands::HELP_MODE);
    let open = bar(&help, Bp::S, 80, mode, false, "¥0.00", |_| true);
    assert_eq!(labels(&open), ["Esc 关闭", "? 关闭"]);
    let wide = bar(&help, Bp::Xl, 160, mode, false, "¥0.00", |_| true);
    assert_eq!(wide.right.help, None);
    let typing = KeyContext {
      typing: true,
      ..KeyContext::on(Page::Login)
    };
    let login = bar(
      &typing,
      Bp::S,
      80,
      ui_mode(crate::pages::login::MODE),
      false,
      "¥0.00",
      |c| c.id != "login.register_submit",
    );
    assert_eq!(labels(&login), ["Tab 切换", "⏎ 登录", "Esc 离开输入"]);
  }
}
