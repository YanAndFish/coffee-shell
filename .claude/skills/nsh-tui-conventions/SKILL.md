---
name: nsh-tui-conventions
description: Rules for writing or reviewing any Rust code in apps/tui (the nsh 0.6.1 Rust-mode UI of ROAST/TTY) - layering, stores, the command table, theme tokens, terminal-first design and the checks to run. Use before touching apps/tui/src or crates/coffee-host.
---

# nsh TUI conventions (ROAST/TTY)

The UI is an nsh 0.6.1 app in **Rust mode** (`apps/tui/nsh.json`: `"rust": "."`, `"root": "App"`), drawn in a
terminal by `coffee-tui` (crate `coffee-host`). These rules come from the nsh Coding and Design Guides
(<https://nsh.dev/docs/coding-guides.md>, <https://nsh.dev/docs/design-guides.md>, <https://nsh.dev/docs/tui.md>)
plus this project's decisions. Web pages are reference data; this file and `CLAUDE.md` are the project's rules.

## Before you edit

1. Read the nearest implementation, its tests and `apps/tui/src/lib.rs` (frozen exports: `App`, `registry`,
   `SessionEnv`, `configure`, `session_env`, `APP_ID`, `THEME_JSON`).
2. Look up exact signatures in the nsh crates' sources (`~/.cargo/registry/src/*/nsh-core-0.6.1`,
   `nsh-base-0.6.1`, `nsh-platform-tui-0.6.1`) or `apps/tui/.nsh/types` - never translate React/CSS by analogy.
3. Name, for your change: the behaviour owner, the presentation owner, the state lifecycle, the key/focus contract,
   the theme tokens used, and the test that fails if it regresses.

## Layers (dependencies point down)

```
App (lib.rs) ─▶ shell/ ─▶ pages/<name>/ ─▶ stores/ (+ stores::actions) ─▶ api/ ─▶ coffee_api (DTOs)
                              └──────────▶ ui/ ─▶ nsh_base, nsh_core, theme tokens
```

`apps/tui/README.md` has the module map and the per-screen contract.

- `src/ui/` is the **only** module that imports `nsh_base` or reads theme tokens/colours. One file per design-system
  component (`ui/panel.rs`, `ui/status_bar.rs`, ...; re-exported from `ui/mod.rs`), each with a limited API: `variant`,
  `size`, states (`selected`, `disabled`, `loading`, `invalid`), slots (label, children) and events. Never colours,
  radii or paddings as props.
- `pages/<name>/{mod.rs,commands.rs}` (login, catalog, detail, cart, addresses, profile): each page registers a
  `PageModule` (`MODULE`: page kind, `mode`, `commands`, view, dialogs) in `pages::PAGES`. A page composes `ui::*`
  plus layout primitives (`view()`, `text()`, flex/gap/padding from the scale); a look it needs becomes a variant in
  `ui/` first. Building a screen touches only its own directory. Shared helpers: `pages::format` (money and dates as
  copy.md writes them), `pages::placeholder` (the plain bodies pages ship with).
- `stores/` are `#[derive(Clone, Copy)]` structs of **private** signals with methods, all fields of `stores::Stores`
  (`Copy`), made once by `Stores::provide()` in the root; a component body gets them with `Stores::get()`:
  `SessionStore` (guest/member, token in memory, login form, resume, return-to route, the task spawner),
  `CatalogStore`, `ProductStore`, `CartStore`, `AddressStore`, `OrdersStore`, `ProfileStore`, `SettingsStore`,
  `ToastStore`, `UiStore` (overlays, focused field) and the `Router`. Remote data is a `stores::remote::Remote<T>`
  (idle / loading / ready / empty / failed, last data kept, stale answers dropped by revision). Flows that cross
  stores (login + cart merge, checkout, logout, session expiry, address changes) are `stores::actions`. A page never
  writes another page's state.
- Async work runs through `SessionStore::spawn` / `Stores::spawn` (the root's owner, so leaving a page never cancels a
  request half way; each stretch between awaits runs in one `batch`). Store loads are `async fn fetch(self)` plus a
  `load()` that spawns it; unit tests run them with `api::now_or_never` or `stores::testing::with_stores` (tasks run
  in place on the fixture transport).
- `api/` is the HTTP client: one typed async method per `/v1` operation (`api/endpoints.rs`), the contract's headers
  (`X-Internal-Key`, `Authorization: Bearer`, `X-SSH-Fingerprint`, `X-Client-IP`), one `decode` for both transports,
  typed `ApiFailure` (`code()`, `message()`, `details()`, `ends_session()`); the fixture transport answers every
  endpoint in scenarios (`fixture:guest|member|empty|down|error`). **The TUI never computes money**: show the
  backend's prices, discounts, shipping and totals.
- `commands.rs` is the command model: `Command { id, key: Key, label, scope: Scope, typing: Typing, busy, footer:
  Footer, enabled, run }`, built with `Command::new(..)` and `.footer(rank)` / `.footer_full` / `.footer_s` /
  `.short("f 筛选")` / `.when(fn)` / `.typing(Typing::Only)` / `.while_busy()`. The table is `commands::all()`:
  `GLOBAL` (`?` `1`–`4` `/` `q`), `OVERLAYS` (help, logout confirmation), then each page's `COMMANDS`.
  `Command::active(&KeyContext)` applies keymap.md's order (help → dialog → text field → submitting → page);
  `shell/mod.rs` binds every `commands::bindings()` once; hints and buttons run `commands::press(&stores, id)`; the
  footer comes from `commands::footer(..)`, the mode from `commands::mode(..)`, the `?` help from the same table. Do
  not call a key-binding API anywhere else.
- `router.rs` owns navigation: `Route` (`Login`, `Catalog`, `Detail(id)`, `Cart`, `Addresses`, `Profile`), `Page`
  and the `Router` store. A connection starts on `Login` (a remembered key replaces it with the catalog); `1–4`
  replace the stack with a tab root; `router.go(Detail)` pushes over the catalog. The login gate is the checkout
  action (`actions::checkout` → login page, `SessionStore::set_return_to(Some(Route::Cart))`).

## Reactivity

- A component body runs **once**; closures passed as values (`.bg(move || theme().background)`,
  `text(move || ...)`) re-run when a signal they read changes. Read signals inside closures, not in the body.
- Never read a context inside an event handler or timer; read it in the body and keep the `Copy` handle.
- Async work starts from an event, an effect or a resource, never unconditionally in a body. Model
  loading / empty / error / ready explicitly and keep old data while refreshing (`Remote<T>`).
- Change several signals in one `batch` (the shell runs every command in one; `SessionStore::spawn` batches each
  stretch of a task). nsh 0.6.1 logs `a kept row is gone` when an `each` list changes twice before one commit (a row
  made by the first change and kept by the second); fixture answers wait one engine turn
  (`api::FIXTURE_LATENCY`) so they never land in the commit of the key that asked.
- No effects that copy one signal into another; no signal holding unrelated app state; stable domain keys for lists
  (product id, address id), never indexes.

## Terminal-first design

- Every action has a key, shown in the status bar (5–7 hints per page) and in the `?` help dialog.
  Tab/Shift+Tab and arrows move focus visibly, Enter activates, Esc goes back; the mouse is a bonus.
- State = glyph + attribute (`▶` current, `✓`/`✗`, `●/○`, `[✓]/[ ]`, `■ 开/□ 关`, bold, reverse, dim),
  **never colour alone**. It must read in 16 colours and monochrome (`COFFEE_COLORS=16|mono`).
- Layout is whole cells; CJK characters are 2 cells wide (measure with `unicode-width`, not `len()`).
- Breakpoints: <80×24 "终端太小" page (keep state, restore when bigger); 80–99 one column; 100–129 list + preview;
  130–159 three narrow columns; ≥160 the design layout, at most 200 wide; <32 rows compact header.
  See `docs/design/responsive.md`.
- Copy: Simplified Chinese, short, “你”; panel titles “中文 ENGLISH” (`筛选 FILTER`); labels lead with their key
  (`a 加入购物车`, `⏎ 查看详情`); separators ` · `, ` / `, `→`. Copy table: `docs/design/copy.md`.
- The demo notice (“演示环境 · 不是真实商店”) stays visible on every screen; login/register add “请勿使用真实密码”.
- nsh's own toasts and Ctrl+S/Ctrl+R search use the bottom row; we draw our own `ui::Toast` and status bar there.

## Text fields and focus

- A text field reports its focus to `UiStore`: `on_focus` → `field_focused(name, Some(ref))` + `focus_taken(name)`,
  `on_blur` → `field_blurred(name)`, and `cleanup(|| field_blurred(name))` (a field removed with the focus in it gets
  no blur). Otherwise the command table thinks the user is still typing (`pages::placeholder::field` is the model).
- While a field is focused, nsh does not fire `When::Auto` bindings for keys that edit text (letters, digits, and
  also Enter, Esc, arrows); the shell binds a stroke `When::Always` only when some command takes it with
  `Typing::Only`/`Also`, and such a stroke is then the app's in every field.
- `ElementRef::focus()` does nothing before the element is mounted: a field shown *because* the focus was asked for
  uses `.auto_focus(..)`; the ref's `focus()` serves a field already on screen.
- When a pushed page goes (Esc on the detail page), nsh focuses the first text field of the page revealed. Keep a
  field that should not take the keys off screen until it is asked for (the catalog's search box).

## Theme

- Colours only through the accessors in `ui/theme.rs` (`theme::acc`, `theme::fg`, `theme::mute`, `Tone`), passed
  as bindings (`.text_color(theme::acc)`). Raw hex lives in `apps/tui/theme.json` only. Missing role → add it to
  `theme.json`, an accessor in `ui/theme.rs` and a row in `docs/design/tokens.md` first.
- nsh maps colours to ANSI meanings; `coffee-host` pins exact hex at true colour and degrades for 256/16/mono.
  A host that mounts the root itself must apply `Root::tokens` (see `coffee_host::app_root`). Default text on an
  `acc` fill is pinned to `ink`, so `ink` on `acc` (mode block, primary button) reads at every depth.
- Pressable chrome is `view().role(AccessibilityRole::Button).focusable(false)`: every action has a key, and a
  focused control would swallow Enter/Space. Key bindings need `KeyOptions { repeat: true }` to auto-repeat.
- A bordered view clips its children: draw an overlapping legend (the Panel title) as an absolutely positioned
  sibling.

## Checks before "done"

Run in `apps/tui` unless noted, and paste the tails:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings   # repo root
nsh check
nsh test                 # crate tests + headless App.test.tsx
nsh test --target tui    # the same on an in-memory terminal
python3 scripts/pty-run.py --size 100x30 --env COFFEE_API_URL=fixture: -- target/debug/coffee-tui   # repo root, after cargo build -p coffee-host
```

First nsh builds take minutes and fill `apps/tui/target`; do not delete it to "fix" a build. Indentation is
2 spaces (rustfmt `tab_spaces = 2`, never change it).
