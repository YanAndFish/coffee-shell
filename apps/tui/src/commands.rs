//! The command table: every action the user can take, once. It drives the key bindings
//! (the shell binds every keystroke here and dispatches to the first command in scope and
//! enabled), the status bar's hints, the presses of those hints, and (phase 2) the `?`
//! help dialog.
//!
//! Copy rule (design-system README, "快捷键前置"): every label starts with its key, as the
//! user sees the key (`⏎ 详情`, `t 重试`); the key's glyph is lower case or a symbol.
//! Keys follow the design's map; the retry key `t` is one the map leaves free (`r` is the
//! roast filter, `g` the login page's guest browsing).

use nsh_core::prelude::untracked;

use crate::{
  router::Page,
  stores::{CatalogPhase, Stores, toast::ToastKind},
  ui::{Bp, Hint, Mode, status_bar::HINT_GAP, text::display_width},
};

/// Where a command applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
  /// On every page.
  Global,
  /// On one kind of page.
  Page(Page),
}

impl Scope {
  /// Whether it applies on `page`.
  pub fn covers(self, page: Page) -> bool {
    match self {
      Scope::Global => true,
      Scope::Page(scope) => scope == page,
    }
  }
}

/// A command's key: what the user reads, and the keystrokes that press it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
  /// The key cap's text: `⏎`, `↑↓`, `t`.
  pub glyph: &'static str,
  /// The keystrokes (`nsh/keymap` syntax: `enter`, `down`, `shift+/`) that run it.
  pub strokes: &'static [&'static str],
}

/// One action.
#[derive(Clone, Copy)]
pub struct Command {
  /// Stable id, `catalog.open`; a hint names its command by it.
  pub id: &'static str,
  /// Its key.
  pub key: Key,
  /// What the user reads, key first: `⏎ 详情`.
  pub label: &'static str,
  /// Where it applies.
  pub scope: Scope,
  /// Its place among the status bar's hints (lower first); `None`: not in the status bar.
  pub footer_rank: Option<u8>,
  /// Whether it can run now (tracked where it is read).
  pub enabled: fn(&Stores) -> bool,
  /// Runs it; gets the keystroke that pressed it (`"j"`), or its first one for a press.
  pub run: fn(&Stores, &str),
}

impl Command {
  /// The label without its key: `详情`.
  pub fn text(&self) -> &'static str {
    self
      .label
      .strip_prefix(self.key.glyph)
      .map(str::trim_start)
      .unwrap_or(self.label)
  }

  /// Its status bar hint.
  pub fn hint(&self) -> Hint {
    Hint {
      id: self.id,
      key: self.key.glyph,
      text: self.text(),
    }
  }
}

fn always(_: &Stores) -> bool {
  true
}

fn catalog_ready(stores: &Stores) -> bool {
  stores.catalog.phase() == CatalogPhase::Ready
}

fn catalog_reloadable(stores: &Stores) -> bool {
  matches!(
    stores.catalog.phase(),
    CatalogPhase::Failed | CatalogPhase::Empty
  )
}

fn go_tab(stores: &Stores, stroke: &str) {
  if let Some(index) = stroke.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
    stores.router.go_tab(index);
  }
}

fn move_cursor(stores: &Stores, stroke: &str) {
  let delta = match stroke {
    "up" | "k" => -1,
    _ => 1,
  };
  stores.catalog.move_cursor(delta);
}

fn open_detail(stores: &Stores, _: &str) {
  if let Some(id) = stores.catalog.cursor() {
    stores.router.push(crate::router::Route::Detail(id));
  }
}

fn retry_catalog(stores: &Stores, _: &str) {
  stores.catalog.load(stores.session.client());
}

fn back(stores: &Stores, _: &str) {
  stores.router.pop();
}

fn help(stores: &Stores, _: &str) {
  stores.toasts.show(
    ToastKind::Info,
    "快捷键帮助",
    "第二阶段实现 · 状态栏列出本页按键",
  );
}

/// The id of the help command, which the status bar shows at its right end.
pub const HELP: &str = "app.help";

/// Every command. The first one in scope and enabled for a keystroke runs it.
pub const COMMANDS: &[Command] = &[
  Command {
    id: "nav.catalog",
    key: Key {
      glyph: "1",
      strokes: &["1"],
    },
    label: "1 选购",
    scope: Scope::Global,
    footer_rank: None,
    enabled: always,
    run: go_tab,
  },
  Command {
    id: "nav.cart",
    key: Key {
      glyph: "2",
      strokes: &["2"],
    },
    label: "2 购物车",
    scope: Scope::Global,
    footer_rank: None,
    enabled: always,
    run: go_tab,
  },
  Command {
    id: "nav.addresses",
    key: Key {
      glyph: "3",
      strokes: &["3"],
    },
    label: "3 地址本",
    scope: Scope::Global,
    footer_rank: None,
    enabled: always,
    run: go_tab,
  },
  Command {
    id: "nav.profile",
    key: Key {
      glyph: "4",
      strokes: &["4"],
    },
    label: "4 我的",
    scope: Scope::Global,
    footer_rank: None,
    enabled: always,
    run: go_tab,
  },
  Command {
    id: HELP,
    key: Key {
      glyph: "?",
      strokes: &["shift+/"],
    },
    label: "? 帮助",
    scope: Scope::Global,
    footer_rank: None,
    enabled: always,
    run: help,
  },
  Command {
    id: "catalog.move",
    key: Key {
      glyph: "↑↓",
      strokes: &["down", "j", "up", "k"],
    },
    label: "↑↓ 移动",
    scope: Scope::Page(Page::Catalog),
    footer_rank: Some(10),
    enabled: catalog_ready,
    run: move_cursor,
  },
  Command {
    id: "catalog.open",
    key: Key {
      glyph: "⏎",
      strokes: &["enter", "l"],
    },
    label: "⏎ 详情",
    scope: Scope::Page(Page::Catalog),
    footer_rank: Some(20),
    enabled: catalog_ready,
    run: open_detail,
  },
  Command {
    id: "catalog.retry",
    key: Key {
      glyph: "t",
      strokes: &["t"],
    },
    label: "t 重试",
    scope: Scope::Page(Page::Catalog),
    footer_rank: Some(5),
    enabled: catalog_reloadable,
    run: retry_catalog,
  },
  Command {
    id: "detail.back",
    key: Key {
      glyph: "Esc",
      strokes: &["escape"],
    },
    label: "Esc 返回",
    scope: Scope::Page(Page::Detail),
    footer_rank: Some(90),
    enabled: always,
    run: back,
  },
];

/// The command `id`.
pub fn by_id(id: &str) -> Option<&'static Command> {
  COMMANDS.iter().find(|command| command.id == id)
}

/// Runs command `id` as its key would (a press on its hint or its action), if it is
/// enabled now.
pub fn press(stores: &Stores, id: &str) {
  if let Some(command) = by_id(id)
    && untracked(|| (command.enabled)(stores))
  {
    (command.run)(stores, command.key.strokes.first().copied().unwrap_or(""));
  }
}

/// Every keystroke some command takes, once each: what the shell binds.
pub fn strokes() -> Vec<&'static str> {
  let mut strokes: Vec<&'static str> = Vec::new();
  for command in COMMANDS {
    for stroke in command.key.strokes {
      if !strokes.contains(stroke) {
        strokes.push(stroke);
      }
    }
  }
  strokes
}

/// The command `stroke` runs on `page`: the first in scope that `enabled` allows.
pub fn resolve(
  page: Page,
  stroke: &str,
  enabled: impl Fn(&Command) -> bool,
) -> Option<&'static Command> {
  COMMANDS.iter().find(|command| {
    command.scope.covers(page) && command.key.strokes.contains(&stroke) && enabled(command)
  })
}

/// The status bar's hints on `page` at width class `bp` in a bar of `cols` columns showing
/// `mode` and the `notice`: the enabled commands with a footer rank, by rank, as many as the
/// width class allows and the row has room for.
pub fn footer(
  page: Page,
  bp: Bp,
  cols: usize,
  mode: Mode,
  notice: &str,
  enabled: impl Fn(&Command) -> bool,
) -> Vec<Hint> {
  let mut candidates: Vec<&Command> = COMMANDS
    .iter()
    .filter(|command| command.footer_rank.is_some() && command.scope.covers(page))
    .filter(|command| enabled(command))
    .collect();
  candidates.sort_by_key(|command| command.footer_rank);
  let help = by_id(HELP).map_or(0, |help| help.hint().width());
  // The mode block, the hints' padding (a cell each side), the notice and the help hint
  // with its padding.
  let mut room = cols.saturating_sub(mode.width() + 2 + display_width(notice) + help + 2);
  let mut hints = Vec::new();
  for command in candidates.into_iter().take(bp.footer_hints()) {
    let hint = command.hint();
    let need = hint.width() + if hints.is_empty() { 0 } else { HINT_GAP };
    if need > room {
      break;
    }
    room -= need;
    hints.push(hint);
  }
  hints
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  const NOTICE: &str = crate::shell::DEMO_NOTICE;

  fn mode() -> Mode {
    crate::router::Route::Catalog.mode()
  }

  #[test]
  fn labels_start_with_their_key() {
    for command in COMMANDS {
      assert!(
        command.label.starts_with(command.key.glyph),
        "{}: {:?} does not start with {:?}",
        command.id,
        command.label,
        command.key.glyph
      );
      assert!(!command.text().is_empty(), "{} has no text", command.id);
      assert!(!command.key.strokes.is_empty(), "{} has no key", command.id);
      assert_eq!(
        command.key.glyph,
        command.key.glyph.to_lowercase().replace("esc", "Esc")
      );
    }
  }

  #[test]
  fn ids_are_unique_and_no_keystroke_is_taken_twice_in_one_scope() {
    for (i, a) in COMMANDS.iter().enumerate() {
      for b in &COMMANDS[i + 1..] {
        assert_ne!(a.id, b.id);
        let overlap = a.scope == b.scope || a.scope == Scope::Global || b.scope == Scope::Global;
        if overlap {
          for stroke in a.key.strokes {
            assert!(
              !b.key.strokes.contains(stroke),
              "{} and {} share {stroke}",
              a.id,
              b.id
            );
          }
        }
      }
    }
  }

  #[test]
  fn every_keystroke_parses_and_is_not_a_reserved_one() {
    use nsh_core::{keymap::Keystroke, platform::Os};
    for stroke in strokes() {
      for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let parsed = Keystroke::parse(stroke, os).unwrap();
        assert!(
          parsed.reserved(os).is_none(),
          "{stroke} is reserved on {os:?}"
        );
      }
    }
  }

  #[test]
  fn keystrokes_resolve_by_page_and_state() {
    let all = |_: &Command| true;
    assert_eq!(
      resolve(Page::Catalog, "j", all).map(|c| c.id),
      Some("catalog.move")
    );
    assert_eq!(
      resolve(Page::Catalog, "enter", all).map(|c| c.id),
      Some("catalog.open")
    );
    assert_eq!(resolve(Page::Cart, "enter", all).map(|c| c.id), None);
    assert_eq!(
      resolve(Page::Cart, "1", all).map(|c| c.id),
      Some("nav.catalog")
    );
    assert_eq!(
      resolve(Page::Detail, "escape", all).map(|c| c.id),
      Some("detail.back")
    );
    assert_eq!(resolve(Page::Catalog, "escape", all).map(|c| c.id), None);
    let not_retry = |c: &Command| c.id != "catalog.retry";
    assert_eq!(resolve(Page::Catalog, "t", not_retry).map(|c| c.id), None);
  }

  #[test]
  fn the_footer_follows_the_page_the_state_and_the_width() {
    let ready = |c: &Command| c.id != "catalog.retry";
    let ids = |hints: Vec<Hint>| hints.into_iter().map(|h| h.id).collect::<Vec<_>>();
    assert_eq!(
      ids(footer(Page::Catalog, Bp::Xl, 160, mode(), NOTICE, ready)),
      vec!["catalog.move", "catalog.open"]
    );
    let failed = |c: &Command| c.id == "catalog.retry" || c.scope == Scope::Global;
    assert_eq!(
      ids(footer(Page::Catalog, Bp::S, 80, mode(), NOTICE, failed)),
      vec!["catalog.retry"]
    );
    assert_eq!(
      ids(footer(Page::Detail, Bp::M, 100, mode(), NOTICE, ready)),
      vec!["detail.back"]
    );
    assert!(footer(Page::Catalog, Bp::TooSmall, 60, mode(), NOTICE, ready).is_empty());
    // A row too narrow for any hint keeps the notice and drops the hints.
    assert!(footer(Page::Catalog, Bp::S, 40, mode(), NOTICE, ready).is_empty());
  }

  #[test]
  fn the_footer_fits_80_columns() {
    let all = |_: &Command| true;
    for page in [Page::Catalog, Page::Detail, Page::Cart] {
      let hints = footer(page, Bp::S, 80, mode(), NOTICE, all);
      assert!(hints.len() <= Bp::S.footer_hints());
      let used: usize = hints.iter().map(Hint::width).sum::<usize>()
        + HINT_GAP * hints.len().saturating_sub(1)
        + mode().width()
        + 2
        + display_width(NOTICE)
        + by_id(HELP).unwrap().hint().width()
        + 2;
      assert!(used <= 80, "{page:?} needs {used} columns");
    }
  }
}
