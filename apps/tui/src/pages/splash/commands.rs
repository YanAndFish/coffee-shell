//! The splash's keys (keymap.md, "开屏"): `⏎` (also `Esc`, `space`) skips it; `?` is the
//! global help. No `1`–`4` `/` `q`: the global keys are the app pages' only.

use crate::{
  commands::{Command, Key, Scope},
  router::Page,
  stores::actions,
};

const PAGE: Scope = Scope::Page(Page::Splash);

/// The page's commands.
pub const COMMANDS: &[Command] = &[
  Command::new(
    "splash.skip",
    Key::new("⏎", &["enter", "escape", "space"]),
    "⏎ 跳过",
    PAGE,
    |stores, _| actions::leave_splash(stores),
  )
  .footer(10),
  Command::new(
    "splash.keys",
    Key::hint("?"),
    "? 快捷键",
    PAGE,
    |stores, _| stores.ui.toggle_help(),
  )
  .footer_full(20),
];
