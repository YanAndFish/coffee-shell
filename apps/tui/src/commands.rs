//! The command model: every action the user can take, once. One table drives the key
//! bindings (the shell binds every keystroke here once and dispatches to the first command
//! active and enabled), the status bar's hints, the presses of those hints, and the `?`
//! help dialog.
//!
//! The table is [`all`]: the global and overlay commands of this file ([`GLOBAL`],
//! [`OVERLAYS`]) followed by each page module's own list (`pages/<name>/commands.rs`,
//! registered in [`crate::pages::PAGES`]). A screen adds or changes its keys in its own
//! directory only.
//!
//! What a key does follows keymap.md's order ("处理顺序"), as [`Command::active`] decides
//! from the [`KeyContext`]:
//!
//! 1. Help open: only the help's commands (`?` `Esc` `q` close it).
//! 2. A dialog open: only that dialog's commands (`?` does nothing, K24).
//! 3. A text field focused: only commands marked [`Typing::Only`] / [`Typing::Also`]
//!    (`⏎` submits, `Esc` leaves the field); single-letter commands do not fire, and nsh
//!    types the key into the field.
//! 4. Something submitting: only commands marked [`Command::while_busy`] (`?`).
//! 5. `?`, then the login page's own keys (no `1`–`4` `/` `q` there), else the global
//!    keys of the app pages, then the page's keys.
//!
//! Copy rule (design-system README, "快捷键前置"): every label starts with its key, as the
//! user sees the key (`⏎ 详情`, `a 加入`). A command with no keystrokes is a hint only: its
//! key is handled elsewhere (nsh's Tab traversal, or a global key such as `4` shown in a
//! page's footer as `4 查看订单`), and a press on it runs it.

use nsh_core::prelude::{batch, untracked};
use unicode_width::UnicodeWidthStr;

use crate::{
  pages,
  router::Page,
  stores::{Overlay, Stores, actions},
  ui::Bp,
};

/// A status-bar mode: `选购 BROWSE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mode {
  /// Chinese name, `选购`.
  pub zh: &'static str,
  /// English name in capitals, `BROWSE`.
  pub en: &'static str,
}

impl Mode {
  /// The mode `zh en`.
  pub const fn new(zh: &'static str, en: &'static str) -> Self {
    Self { zh, en }
  }
}

/// `帮助 HELP`: the help is open.
pub const HELP_MODE: Mode = Mode::new("帮助", "HELP");
/// `确认 CONFIRM`: the logout confirmation is open.
pub const CONFIRM_MODE: Mode = Mode::new("确认", "CONFIRM");
/// `编辑 INSERT`: the address dialog is open.
pub const INSERT_MODE: Mode = Mode::new("编辑", "INSERT");
/// `筛选 FILTER`: the S-width filter dialog is open.
pub const FILTER_MODE: Mode = Mode::new("筛选", "FILTER");

/// The mode the status bar shows now (tracked): an overlay's, else the page's.
pub fn mode(stores: &Stores) -> Mode {
  match stores.ui.overlay() {
    Some(Overlay::Help) => HELP_MODE,
    Some(Overlay::Logout) => CONFIRM_MODE,
    Some(Overlay::AddressDialog) => INSERT_MODE,
    Some(Overlay::FilterDialog) => FILTER_MODE,
    None => (pages::module(stores.page()).mode)(stores),
  }
}

/// Where a command applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
  /// Every page, the login page included (`?`).
  Pages,
  /// The app pages: every page but the login page (`1`–`4`, `/`, `q`).
  App,
  /// One kind of page.
  Page(Page),
  /// An overlay, while it is open (and nothing else then).
  Overlay(Overlay),
}

/// Whether a command fires while a text field has the keyboard focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Typing {
  /// Not while typing (the default: the key goes into the field).
  #[default]
  No,
  /// Only while typing (`Esc 离开输入`, `⏎` applying a coupon).
  Only,
  /// Both (`⏎` submitting the login form).
  Also,
}

/// A command's key: what the user reads, and the keystrokes that press it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
  /// The key cap's text: `⏎`, `↑↓`, `space`.
  pub glyph: &'static str,
  /// The keystrokes (`nsh/keymap` syntax: `enter`, `down`, `shift+/`) that run it; none
  /// for a hint whose key is handled elsewhere.
  pub strokes: &'static [&'static str],
}

impl Key {
  /// A key `glyph` pressed by `strokes`.
  pub const fn new(glyph: &'static str, strokes: &'static [&'static str]) -> Self {
    Self { glyph, strokes }
  }

  /// A hint's key: shown, but bound elsewhere.
  pub const fn hint(glyph: &'static str) -> Self {
    Self {
      glyph,
      strokes: &[],
    }
  }
}

/// A command's place in the status bar (lower ranks first; `None`: not shown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Footer {
  /// Rank at M / L / XL ("完整" column of keymap.md).
  pub full: Option<u8>,
  /// Rank at S ("S" column).
  pub s: Option<u8>,
  /// The label at S when it differs (`f 筛选`, `m 动画`), key first.
  pub short: Option<&'static str>,
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
  /// Whether it fires while a text field has the focus.
  pub typing: Typing,
  /// Whether it fires while something is submitting (only `?` and a dialog's cancel).
  pub busy: bool,
  /// Its place in the status bar.
  pub footer: Footer,
  /// Whether it can run now (tracked where it is read: the footer follows it).
  pub enabled: fn(&Stores) -> bool,
  /// Runs it; gets the keystroke that pressed it (`"j"`), or its first one for a press.
  pub run: fn(&Stores, &str),
}

/// Always enabled.
pub fn always(_: &Stores) -> bool {
  true
}

/// Does nothing (a hint for a key nsh handles itself, such as Tab).
pub fn nothing(_: &Stores, _: &str) {}

impl Command {
  /// A command `id` on `key`, labelled `label` (key first), in `scope`: not while typing,
  /// not while busy, not in the footer, always enabled.
  pub const fn new(
    id: &'static str,
    key: Key,
    label: &'static str,
    scope: Scope,
    run: fn(&Stores, &str),
  ) -> Self {
    Self {
      id,
      key,
      label,
      scope,
      typing: Typing::No,
      busy: false,
      footer: Footer {
        full: None,
        s: None,
        short: None,
      },
      enabled: always,
      run,
    }
  }

  /// Shown in the status bar at `rank`, at every width.
  pub const fn footer(mut self, rank: u8) -> Self {
    self.footer.full = Some(rank);
    self.footer.s = Some(rank);
    self
  }

  /// Shown in the status bar at `rank` at M / L / XL only.
  pub const fn footer_full(mut self, rank: u8) -> Self {
    self.footer.full = Some(rank);
    self
  }

  /// Shown in the status bar at `rank` at S only.
  pub const fn footer_s(mut self, rank: u8) -> Self {
    self.footer.s = Some(rank);
    self
  }

  /// Labelled `label` (key first) at S.
  pub const fn short(mut self, label: &'static str) -> Self {
    self.footer.short = Some(label);
    self
  }

  /// Enabled when `enabled` says so.
  pub const fn when(mut self, enabled: fn(&Stores) -> bool) -> Self {
    self.enabled = enabled;
    self
  }

  /// Fires while a text field has the focus as `typing` says.
  pub const fn typing(mut self, typing: Typing) -> Self {
    self.typing = typing;
    self
  }

  /// Fires while something is submitting.
  pub const fn while_busy(mut self) -> Self {
    self.busy = true;
    self
  }

  /// The label without its key: `详情`.
  pub fn text(&self) -> &'static str {
    strip_key(self.label, self.key.glyph)
  }

  /// Whether it applies in `ctx` (keymap.md's order, see the module docs).
  pub fn active(&self, ctx: &KeyContext) -> bool {
    let in_layer = match ctx.overlay {
      Some(overlay) => self.scope == Scope::Overlay(overlay),
      None => match self.scope {
        Scope::Pages => true,
        Scope::App => ctx.page != Page::Login,
        Scope::Page(page) => page == ctx.page,
        Scope::Overlay(_) => false,
      },
    };
    let typing = match self.typing {
      Typing::No => !ctx.typing,
      Typing::Only => ctx.typing,
      Typing::Also => true,
    };
    in_layer && typing && (self.busy || !ctx.busy)
  }

  /// Its status-bar hint at width class `bp`, if it has one there.
  pub fn footer_hint(&self, bp: Bp) -> Option<FooterHint> {
    self.rank(bp)?;
    let label = match self.footer.short {
      Some(short) if bp == Bp::S => short,
      _ => self.label,
    };
    let (key, text) = split_label(label, self.key.glyph);
    Some(FooterHint {
      id: self.id,
      key,
      text,
    })
  }

  fn rank(&self, bp: Bp) -> Option<u8> {
    if bp == Bp::S {
      self.footer.s
    } else {
      self.footer.full
    }
  }
}

fn strip_key(label: &'static str, glyph: &str) -> &'static str {
  label
    .strip_prefix(glyph)
    .map(str::trim_start)
    .unwrap_or(label)
}

/// `label` as (key, text): the command's glyph when the label starts with it, else the
/// label's first word.
fn split_label(label: &'static str, glyph: &'static str) -> (&'static str, &'static str) {
  if label.starts_with(glyph) {
    (glyph, strip_key(label, glyph))
  } else {
    label.split_once(' ').unwrap_or((label, ""))
  }
}

/// One status-bar hint: `↑↓ 移动`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FooterHint {
  /// The command it runs when pressed.
  pub id: &'static str,
  /// The key cap's text, `↑↓`.
  pub key: &'static str,
  /// What it does, `移动`.
  pub text: &'static str,
}

impl FooterHint {
  /// The cells the hint takes: key cap (key plus a cell each side), a gap, the text.
  pub fn width(&self) -> usize {
    self.key.width() + 2 + 1 + self.text.width()
  }
}

/// The cells between two hints.
pub const HINT_GAP: usize = 2;

/// What decides which commands are active now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyContext {
  /// The page on screen.
  pub page: Page,
  /// The overlay open, if any.
  pub overlay: Option<Overlay>,
  /// A text field has the keyboard focus.
  pub typing: bool,
  /// Something is submitting.
  pub busy: bool,
}

impl KeyContext {
  /// On `page`, nothing open, nothing typed, nothing submitting.
  pub const fn on(page: Page) -> Self {
    Self {
      page,
      overlay: None,
      typing: false,
      busy: false,
    }
  }
}

/// The context now (tracked: the footer follows it).
pub fn context(stores: &Stores) -> KeyContext {
  KeyContext {
    page: stores.page(),
    overlay: stores.ui.overlay(),
    typing: stores.ui.typing(),
    busy: stores.busy(),
  }
}

// --- The global and overlay commands ----------------------------------------------------

fn go_tab(stores: &Stores, stroke: &str) {
  if let Some(index) = stroke.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
    actions::go_tab(stores, index);
  }
}

fn member(stores: &Stores) -> bool {
  stores.session.is_member()
}

/// The id of the help command, which the status bar shows at its right end.
pub const HELP: &str = "app.help";

/// The commands of every page (the global keys of keymap.md).
pub const GLOBAL: &[Command] = &[
  Command::new(
    HELP,
    Key::new("?", &["shift+/"]),
    "? 帮助",
    Scope::Pages,
    |stores, _| stores.ui.toggle_help(),
  )
  .while_busy(),
  Command::new(
    "nav.catalog",
    Key::new("1", &["1"]),
    "1 选购",
    Scope::App,
    go_tab,
  ),
  Command::new(
    "nav.cart",
    Key::new("2", &["2"]),
    "2 购物车",
    Scope::App,
    go_tab,
  ),
  Command::new(
    "nav.addresses",
    Key::new("3", &["3"]),
    "3 地址本",
    Scope::App,
    go_tab,
  ),
  Command::new(
    "nav.profile",
    Key::new("4", &["4"]),
    "4 我的",
    Scope::App,
    go_tab,
  ),
  Command::new(
    "app.search",
    Key::new("/", &["/"]),
    "/ 搜索",
    Scope::App,
    search,
  ),
  Command::new(
    "app.logout",
    Key::new("q", &["q"]),
    "q 退出登录",
    Scope::App,
    |stores, _| actions::ask_logout(stores),
  )
  .when(member),
  Command::new(
    "app.focus",
    Key::hint("Tab"),
    "Tab 移动焦点",
    Scope::Pages,
    nothing,
  )
  .typing(Typing::Also),
];

/// The help's and the logout confirmation's commands (the address and filter dialogs'
/// are their pages').
pub const OVERLAYS: &[Command] = &[
  Command::new(
    "help.close",
    Key::new("Esc", &["escape", "q"]),
    "Esc 关闭",
    Scope::Overlay(Overlay::Help),
    |stores, _| stores.ui.close_if(Overlay::Help),
  )
  .footer(10)
  .while_busy(),
  Command::new(
    "help.toggle",
    Key::new("?", &["shift+/"]),
    "? 关闭",
    Scope::Overlay(Overlay::Help),
    |stores, _| stores.ui.close_if(Overlay::Help),
  )
  .footer(20)
  .while_busy(),
  Command::new(
    "logout.confirm",
    Key::new("q", &["q"]),
    "q 确认退出",
    Scope::Overlay(Overlay::Logout),
    |stores, _| actions::confirm_logout(stores, false),
  )
  .footer(10),
  Command::new(
    "logout.enter",
    Key::new("⏎", &["enter"]),
    "⏎ 确认",
    Scope::Overlay(Overlay::Logout),
    |stores, _| actions::confirm_logout(stores, true),
  )
  .footer(20),
  Command::new(
    "logout.switch",
    Key::new("Tab", &["tab", "shift+tab", "left", "right"]),
    "Tab 切换",
    Scope::Overlay(Overlay::Logout),
    |stores, _| stores.ui.toggle_logout_choice(),
  )
  .footer(30),
  Command::new(
    "logout.cancel",
    Key::new("Esc", &["escape"]),
    "Esc 取消",
    Scope::Overlay(Overlay::Logout),
    |stores, _| stores.ui.close_if(Overlay::Logout),
  )
  .footer(40),
];

/// Keystrokes that also go on to nsh after a command runs: Tab keeps moving the focus.
pub const PASS_THROUGH: &[&str] = &["tab", "shift+tab"];

/// `1–4 切换页面` (`1–4 切换` at S) for `page`'s footer: a press shows the next tab.
pub const fn tabs_hint(id: &'static str, page: Page) -> Command {
  Command::new(
    id,
    Key::hint("1–4"),
    "1–4 切换页面",
    Scope::Page(page),
    next_tab,
  )
  .short("1–4 切换")
}

/// `/ 搜索` for `page`'s footer (the key itself is the global `/`).
pub const fn search_hint(id: &'static str, page: Page) -> Command {
  Command::new(id, Key::hint("/"), "/ 搜索", Scope::Page(page), search)
}

fn next_tab(stores: &Stores, _: &str) {
  let next = untracked(|| stores.router.tab()).map_or(0, |tab| (tab + 1) % 4);
  actions::go_tab(stores, next);
}

fn search(stores: &Stores, _: &str) {
  actions::search(stores);
}

// --- The table --------------------------------------------------------------------------

/// Every command, in resolution order: global, overlays, then each page's.
pub fn all() -> impl Iterator<Item = &'static Command> {
  GLOBAL.iter().chain(OVERLAYS).chain(
    pages::PAGES
      .iter()
      .flat_map(|module| module.commands.iter()),
  )
}

/// The command `id`.
pub fn by_id(id: &str) -> Option<&'static Command> {
  all().find(|command| command.id == id)
}

/// Runs command `id` as its key would (a press on its hint or its button), if it is active
/// and enabled now.
pub fn press(stores: &Stores, id: &str) {
  let Some(command) = by_id(id) else {
    return;
  };
  let ctx = untracked(|| context(stores));
  if command.active(&ctx) && untracked(|| (command.enabled)(stores)) {
    let stroke = command.key.strokes.first().copied().unwrap_or("");
    batch(|| (command.run)(stores, stroke));
  }
}

/// A keystroke the shell binds, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
  /// The keystroke.
  pub stroke: &'static str,
  /// It fires while a text field has the focus (a command takes it while typing: `⏎`,
  /// `Esc`); else nsh gives it to the field.
  pub in_text: bool,
  /// The key goes on to nsh after the command (Tab).
  pub pass_through: bool,
}

/// Every keystroke some command takes, once each: what the shell binds.
pub fn bindings() -> Vec<Binding> {
  let mut bindings: Vec<Binding> = Vec::new();
  for command in all() {
    for stroke in command.key.strokes {
      let in_text = command.typing != Typing::No;
      match bindings.iter_mut().find(|b| b.stroke == *stroke) {
        Some(binding) => binding.in_text |= in_text,
        None => bindings.push(Binding {
          stroke,
          in_text,
          pass_through: PASS_THROUGH.contains(stroke),
        }),
      }
    }
  }
  bindings
}

/// The command `stroke` runs in `ctx`: the first active one taking it that `enabled`
/// allows.
pub fn resolve(
  ctx: &KeyContext,
  stroke: &str,
  enabled: impl Fn(&Command) -> bool,
) -> Option<&'static Command> {
  all().find(|command| {
    command.key.strokes.contains(&stroke) && command.active(ctx) && enabled(command)
  })
}

/// The status bar's hints in `ctx` at width class `bp`, in `room` cells: the active, enabled
/// commands with a rank at that width, by rank, as many as the width class allows
/// ([`Bp::footer_hints`]) and the room holds.
pub fn footer(
  ctx: &KeyContext,
  bp: Bp,
  room: usize,
  enabled: impl Fn(&Command) -> bool,
) -> Vec<FooterHint> {
  let mut candidates: Vec<&Command> = all()
    .filter(|command| command.rank(bp).is_some() && command.active(ctx))
    .filter(|command| enabled(command))
    .collect();
  candidates.sort_by_key(|command| command.rank(bp));
  let mut room = room;
  let mut hints = Vec::new();
  for command in candidates.into_iter().take(bp.footer_hints()) {
    let Some(hint) = command.footer_hint(bp) else {
      continue;
    };
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
  use nsh_core::{keymap::Keystroke, platform::Os};

  use super::*;

  fn ids(hints: Vec<FooterHint>) -> Vec<&'static str> {
    hints.into_iter().map(|h| h.id).collect()
  }

  fn labels(hints: Vec<FooterHint>) -> Vec<String> {
    hints
      .into_iter()
      .map(|h| format!("{} {}", h.key, h.text))
      .collect()
  }

  /// The footer in `ctx` at `bp` with room to spare, the commands `off` disabled.
  fn set(ctx: KeyContext, bp: Bp, off: &[&str]) -> Vec<FooterHint> {
    footer(&ctx, bp, 500, |c| !off.contains(&c.id))
  }

  fn typing(page: Page) -> KeyContext {
    KeyContext {
      typing: true,
      ..KeyContext::on(page)
    }
  }

  fn overlay(overlay: Overlay) -> KeyContext {
    KeyContext {
      overlay: Some(overlay),
      ..KeyContext::on(Page::Catalog)
    }
  }

  #[test]
  fn labels_start_with_their_key() {
    for command in all() {
      assert!(
        command.label.starts_with(command.key.glyph),
        "{}: {:?} does not start with {:?}",
        command.id,
        command.label,
        command.key.glyph
      );
      assert!(!command.text().is_empty(), "{} has no text", command.id);
      if let Some(short) = command.footer.short {
        assert!(short.contains(' '), "{}: short label {short:?}", command.id);
      }
    }
  }

  #[test]
  fn ids_are_unique_and_no_keystroke_is_taken_twice_in_one_layer() {
    let commands: Vec<&Command> = all().collect();
    let overlaps = |a: Scope, b: Scope| match (a, b) {
      (Scope::Overlay(x), Scope::Overlay(y)) => x == y,
      (Scope::Overlay(_), _) | (_, Scope::Overlay(_)) => false,
      (Scope::Page(x), Scope::Page(y)) => x == y,
      _ => true,
    };
    let typing_overlaps = |a: Typing, b: Typing| {
      !matches!(
        (a, b),
        (Typing::No, Typing::Only) | (Typing::Only, Typing::No)
      )
    };
    let unconditional =
      |c: &Command| std::ptr::fn_addr_eq(c.enabled, always as fn(&Stores) -> bool);
    for (i, a) in commands.iter().enumerate() {
      for b in &commands[i + 1..] {
        assert_ne!(a.id, b.id);
        if overlaps(a.scope, b.scope)
          && typing_overlaps(a.typing, b.typing)
          && unconditional(a)
          && unconditional(b)
        {
          for stroke in a.key.strokes {
            assert!(
              !b.key.strokes.contains(stroke),
              "{} and {} both take {stroke}",
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
    for binding in bindings() {
      for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let parsed = Keystroke::parse(binding.stroke, os)
          .unwrap_or_else(|e| panic!("{}: {e}", binding.stroke));
        assert!(
          parsed.reserved(os).is_none(),
          "{} is reserved on {os:?}",
          binding.stroke
        );
      }
    }
    let find = |stroke: &str| bindings().into_iter().find(|b| b.stroke == stroke).unwrap();
    assert!(find("enter").in_text, "⏎ submits from a text field");
    assert!(!find("g").in_text, "g is typed into a text field");
    assert!(find("tab").pass_through, "Tab still moves the focus");
  }

  #[test]
  fn every_page_has_a_module_and_a_mode() {
    for page in Page::ALL {
      assert_eq!(pages::module(page).page, page);
    }
    let modes = [
      crate::pages::login::MODE,
      crate::pages::login::REGISTER_MODE,
      crate::pages::catalog::MODE,
      crate::pages::detail::MODE,
      crate::pages::cart::MODE,
      crate::pages::addresses::MODE,
      crate::pages::profile::MODE,
    ];
    let names: Vec<String> = modes.iter().map(|m| format!("{} {}", m.zh, m.en)).collect();
    assert_eq!(
      names,
      [
        "登录 LOGIN",
        "注册 SIGN UP",
        "选购 BROWSE",
        "详情 DETAIL",
        "购物车 CART",
        "地址本 ADDRESS",
        "我的 PROFILE"
      ]
    );
  }

  #[test]
  fn keys_resolve_by_layer_page_and_state() {
    let all = |_: &Command| true;
    let id = |ctx: KeyContext, stroke| resolve(&ctx, stroke, all).map(|c| c.id);
    assert_eq!(id(KeyContext::on(Page::Catalog), "j"), Some("catalog.move"));
    assert_eq!(id(KeyContext::on(Page::Cart), "1"), Some("nav.catalog"));
    assert_eq!(id(KeyContext::on(Page::Profile), "q"), Some("app.logout"));
    assert_eq!(id(KeyContext::on(Page::Cart), "a"), Some("cart.all"));
    assert_eq!(id(KeyContext::on(Page::Catalog), "a"), Some("catalog.add"));
    // The login page takes none of 1–4 / q.
    assert_eq!(id(KeyContext::on(Page::Login), "1"), None);
    assert_eq!(id(KeyContext::on(Page::Login), "/"), None);
    assert_eq!(id(KeyContext::on(Page::Login), "q"), None);
    assert_eq!(id(KeyContext::on(Page::Login), "g"), Some("login.guest"));
    assert_eq!(id(KeyContext::on(Page::Login), "shift+/"), Some(HELP));
    // State decides between commands on one key.
    let ready = |c: &Command| c.id != "catalog.retry";
    let failed = |c: &Command| !matches!(c.id, "catalog.open" | "catalog.move");
    let ctx = KeyContext::on(Page::Catalog);
    assert_eq!(
      resolve(&ctx, "enter", ready).map(|c| c.id),
      Some("catalog.open")
    );
    assert_eq!(
      resolve(&ctx, "enter", failed).map(|c| c.id),
      Some("catalog.retry")
    );
    let receipt = |c: &Command| !matches!(c.id, "cart.checkout" | "cart.retry");
    let ctx = KeyContext::on(Page::Cart);
    assert_eq!(
      resolve(&ctx, "enter", receipt).map(|c| c.id),
      Some("cart.continue")
    );
    // A cart that never loaded: ⏎ retries, c checks nothing out.
    let failed_cart = |c: &Command| !matches!(c.id, "cart.checkout" | "cart.continue");
    assert_eq!(
      resolve(&ctx, "enter", failed_cart).map(|c| c.id),
      Some("cart.retry")
    );
    assert_eq!(resolve(&ctx, "c", failed_cart).map(|c| c.id), None);
  }

  #[test]
  fn single_letters_do_not_fire_in_a_text_field() {
    let all = |_: &Command| true;
    let id = |ctx: KeyContext, stroke| resolve(&ctx, stroke, all).map(|c| c.id);
    for (page, stroke) in [
      (Page::Login, "g"),
      (Page::Catalog, "j"),
      (Page::Catalog, "a"),
      (Page::Cart, "x"),
      (Page::Cart, "1"),
      (Page::Profile, "q"),
      (Page::Login, "shift+/"),
    ] {
      assert_eq!(id(typing(page), stroke), None, "{stroke} on {page:?}");
    }
    let login = |c: &Command| c.id != "login.register_submit";
    assert_eq!(
      resolve(&typing(Page::Login), "enter", login).map(|c| c.id),
      Some("login.submit")
    );
    assert_eq!(id(typing(Page::Login), "escape"), Some("login.leave"));
    assert_eq!(id(typing(Page::Cart), "enter"), Some("cart.coupon"));
    assert_eq!(
      id(typing(Page::Catalog), "escape"),
      Some("catalog.leave_search")
    );
    let ctx = KeyContext {
      typing: true,
      ..overlay(Overlay::AddressDialog)
    };
    assert_eq!(id(ctx, "enter"), Some("dialog.save"));
    assert_eq!(id(ctx, "space"), None, "space types a space");
  }

  #[test]
  fn while_submitting_only_help_works() {
    let all = |_: &Command| true;
    let busy = |page| KeyContext {
      busy: true,
      ..KeyContext::on(page)
    };
    for (page, stroke) in [
      (Page::Login, "enter"),
      (Page::Login, "g"),
      (Page::Cart, "1"),
      (Page::Cart, "c"),
      (Page::Cart, "q"),
    ] {
      assert!(resolve(&busy(page), stroke, all).is_none(), "{stroke}");
    }
    assert_eq!(
      resolve(&busy(Page::Cart), "shift+/", all).map(|c| c.id),
      Some(HELP)
    );
    // A dialog saving: only its Esc.
    let saving = KeyContext {
      busy: true,
      ..overlay(Overlay::AddressDialog)
    };
    assert_eq!(resolve(&saving, "enter", all).map(|c| c.id), None);
    assert_eq!(
      resolve(&saving, "escape", all).map(|c| c.id),
      Some("dialog.cancel")
    );
  }

  #[test]
  fn overlays_take_the_keys() {
    let all = |_: &Command| true;
    let id = |ctx: KeyContext, stroke| resolve(&ctx, stroke, all).map(|c| c.id);
    let help = overlay(Overlay::Help);
    assert_eq!(id(help, "j"), None);
    assert_eq!(id(help, "1"), None);
    assert_eq!(id(help, "escape"), Some("help.close"));
    assert_eq!(id(help, "q"), Some("help.close"), "q closes the help (K23)");
    assert_eq!(id(help, "shift+/"), Some("help.toggle"));
    let logout = overlay(Overlay::Logout);
    assert_eq!(id(logout, "q"), Some("logout.confirm"));
    assert_eq!(id(logout, "enter"), Some("logout.enter"));
    assert_eq!(id(logout, "right"), Some("logout.switch"));
    assert_eq!(id(logout, "escape"), Some("logout.cancel"));
    assert_eq!(id(logout, "shift+/"), None, "no help over a dialog (K24)");
    let filter = overlay(Overlay::FilterDialog);
    assert_eq!(id(filter, "r"), Some("filter.jump"));
    assert_eq!(id(filter, "space"), Some("filter.choose"));
    assert_eq!(id(filter, "a"), None);
  }

  #[test]
  fn footers_follow_the_keymap_per_page_state_and_width() {
    use Page::*;
    // 登录（无焦点）/（输入框里）/ 注册.
    let login = ["login.register_submit", "login.back"];
    assert_eq!(
      ids(set(KeyContext::on(Login), Bp::Xl, &login)),
      ["login.tab", "login.submit", "login.guest", "login.keys"]
    );
    assert_eq!(
      labels(set(KeyContext::on(Login), Bp::S, &login)),
      ["Tab 切换", "⏎ 登录", "g 访客"]
    );
    assert_eq!(
      ids(set(typing(Login), Bp::Xl, &login)),
      ["login.tab", "login.submit", "login.leave"]
    );
    let register = ["login.submit"];
    assert_eq!(
      ids(set(KeyContext::on(Login), Bp::Xl, &register)),
      [
        "login.tab",
        "login.register_submit",
        "login.guest",
        "login.back",
        "login.keys"
      ]
    );
    // 选购: 7, then 5 at M, 4 at S with `f 筛选`.
    let ready = ["catalog.retry"];
    assert_eq!(
      labels(set(KeyContext::on(Catalog), Bp::Xl, &ready)),
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
    // M: six (keymap.md: `/ 搜索` goes).
    assert_eq!(
      labels(set(KeyContext::on(Catalog), Bp::M, &ready)),
      ["↑↓ 移动", "⏎ 详情", "a 加入", "f 产区", "r 烘焙", "s 排序"]
    );
    assert_eq!(
      labels(set(KeyContext::on(Catalog), Bp::S, &ready)),
      ["↑↓ 移动", "⏎ 详情", "a 加入", "f 筛选"]
    );
    // 详情.
    assert_eq!(
      labels(set(KeyContext::on(Detail), Bp::Xl, &["detail.retry"])),
      [
        "↑↓ 规格行",
        "←→ 选择",
        "+ 数量",
        "a 加入",
        "b 购买",
        "Esc 返回"
      ]
    );
    assert_eq!(
      labels(set(KeyContext::on(Detail), Bp::S, &["detail.retry"])),
      ["←→ 选择", "a 加入", "b 购买", "Esc 返回"]
    );
    // 购物车 and 回执.
    let shopping = [
      "cart.continue",
      "cart.orders",
      "cart.tabs",
      "cart.search",
      "cart.retry",
    ];
    assert_eq!(
      labels(set(KeyContext::on(Cart), Bp::Xl, &shopping)),
      [
        "↑↓ 移动",
        "space 勾选",
        "←→ 数量",
        "x 删除",
        "u 撤销",
        "c 结算"
      ]
    );
    assert_eq!(
      labels(set(KeyContext::on(Cart), Bp::S, &shopping)),
      ["↑↓ 移动", "space 勾选", "x 删除", "c 结算"]
    );
    let receipt = [
      "cart.move",
      "cart.toggle",
      "cart.quantity",
      "cart.remove",
      "cart.undo",
      "cart.checkout",
      "cart.retry",
    ];
    assert_eq!(
      labels(set(KeyContext::on(Cart), Bp::Xl, &receipt)),
      ["⏎ 继续选购", "4 查看订单", "1–4 切换页面", "/ 搜索"]
    );
    assert_eq!(
      labels(set(KeyContext::on(Cart), Bp::S, &receipt)),
      ["⏎ 继续选购", "4 查看订单"]
    );
    // 地址本, member and guest.
    let member = [
      "addresses.login",
      "addresses.tabs",
      "addresses.search",
      "addresses.retry",
    ];
    assert_eq!(
      labels(set(KeyContext::on(Addresses), Bp::Xl, &member)),
      [
        "←↑↓→ 移动",
        "⏎ 用于本单",
        "n 新增",
        "e 编辑",
        "space 默认",
        "d 删除"
      ]
    );
    assert_eq!(
      labels(set(KeyContext::on(Addresses), Bp::S, &member)),
      ["↑↓ 移动", "⏎ 用于本单", "n 新增", "d 删除"]
    );
    let guest = [
      "addresses.retry",
      "addresses.move",
      "addresses.use",
      "addresses.new",
      "addresses.edit",
      "addresses.default",
      "addresses.delete",
    ];
    assert_eq!(
      labels(set(KeyContext::on(Addresses), Bp::Xl, &guest)),
      ["⏎ 去登录", "1–4 切换页面", "/ 搜索"]
    );
    // 我的, member (loaded, then failed) and guest.
    let member = ["profile.login", "profile.retry"];
    assert_eq!(
      labels(set(KeyContext::on(Profile), Bp::Xl, &["profile.login"]))[0],
      "⏎ 重试",
      "a failed profile offers the retry first"
    );
    assert_eq!(
      labels(set(KeyContext::on(Profile), Bp::S, &["profile.login"]))[0],
      "⏎ 重试"
    );
    assert_eq!(
      labels(set(KeyContext::on(Profile), Bp::Xl, &member)),
      [
        "m 动画开关",
        "o 鼠标追踪",
        "q 退出登录",
        "1–4 切换页面",
        "/ 搜索"
      ]
    );
    assert_eq!(
      labels(set(KeyContext::on(Profile), Bp::S, &member)),
      ["m 动画", "o 鼠标", "q 退出", "1–4 切换"]
    );
    assert_eq!(
      labels(set(
        KeyContext::on(Profile),
        Bp::Xl,
        &["profile.logout", "profile.retry"]
      )),
      [
        "⏎ 去登录",
        "m 动画开关",
        "o 鼠标追踪",
        "1–4 切换页面",
        "/ 搜索"
      ]
    );
    assert_eq!(
      labels(set(
        KeyContext::on(Profile),
        Bp::S,
        &["profile.logout", "profile.retry"]
      )),
      ["⏎ 去登录", "m 动画", "o 鼠标", "1–4 切换"]
    );
    // Overlays.
    assert_eq!(
      labels(set(overlay(Overlay::Help), Bp::Xl, &[])),
      ["Esc 关闭", "? 关闭"]
    );
    assert_eq!(
      labels(set(overlay(Overlay::Logout), Bp::S, &[])),
      ["q 确认退出", "⏎ 确认", "Tab 切换", "Esc 取消"]
    );
    assert_eq!(
      labels(set(overlay(Overlay::AddressDialog), Bp::Xl, &[])),
      [
        "Tab 下一项",
        "←→ 标签",
        "space 设为默认",
        "⏎ 保存",
        "Esc 取消"
      ]
    );
    assert_eq!(
      labels(set(overlay(Overlay::AddressDialog), Bp::S, &[])),
      ["Tab 下一项", "space 默认", "⏎ 保存", "Esc 取消"]
    );
    assert_eq!(
      labels(set(overlay(Overlay::FilterDialog), Bp::S, &[])),
      ["↑↓ 选择", "Tab 下一组", "⏎ 选中", "Esc 关闭"]
    );
    // Below 80×24 nothing; a row without room keeps no hints.
    assert!(set(KeyContext::on(Catalog), Bp::TooSmall, &ready).is_empty());
    assert!(footer(&KeyContext::on(Catalog), Bp::S, 3, |_| true).is_empty());
  }
}
