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
App (lib.rs) ─▶ shell/ ─▶ pages/<name>/ ─▶ stores/ ─▶ api/ ─▶ coffee_api (DTOs)
                              └──────────▶ ui/ ─▶ nsh_base, nsh_core, theme tokens
```

- `src/ui/` is the **only** module that imports `nsh_base` or reads theme tokens/colours. One file per design-system
  component (`ui/button.rs`, `ui/panel.rs`, ...), each with a limited API: `variant`, `size`, states (`selected`,
  `disabled`, `loading`, `invalid`), slots (label, children) and events. Never colours, radii or paddings as props.
- `pages/` compose `ui::*` plus layout primitives (`view()`, `text()`, flex/gap/padding from the scale). A look a page
  needs becomes a variant in `ui/` first.
- `stores/` are `#[derive(Clone, Copy)]` structs of **private** signals with methods (`Cart::add`, `Cart::lines`),
  created once in `App` and shared with `provide_context`; components get them with `use_context` in their body.
  A page never writes another page's state. <!-- TODO(integration): confirm store type names and module paths -->
- `api/` is the HTTP client: base URL and `X-Internal-Key` from `coffee_app::session_env()`; DTOs from
  `coffee_api`. **The TUI never computes money**: show the backend's prices, discounts, shipping and totals.
- `commands.rs` is the single command table: `Command { id, key, label, scope, footer_rank, enabled, run }`. Key
  bindings, buttons, the status-bar hints and the `?` help all derive from it. Do not call a key-binding API
  anywhere else. <!-- TODO(integration): confirm field names against apps/tui/src/commands.rs -->
- `router.rs` owns navigation (`1–4` switch 选购/购物车/地址本/我的, Esc back).

## Reactivity

- A component body runs **once**; closures passed as values (`.bg(move || theme().background)`,
  `text(move || ...)`) re-run when a signal they read changes. Read signals inside closures, not in the body.
- Never read a context inside an event handler or timer; read it in the body and keep the `Copy` handle.
- Async work starts from an event, an effect or a resource, never unconditionally in a body. Model
  loading / empty / error / ready explicitly and keep old data while refreshing.
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
  See `docs/design/responsive.md`. <!-- TODO(integration): confirm file name -->
- Copy: Simplified Chinese, short, “你”; panel titles “中文 ENGLISH” (`筛选 FILTER`); labels lead with their key
  (`a 加入购物车`, `⏎ 查看详情`); separators ` · `, ` / `, `→`. Copy table: `docs/design/copy.md`.
  <!-- TODO(integration): confirm file name -->
- The demo notice (“演示环境 · 不是真实商店”) stays visible on every screen; login/register add “请勿使用真实密码”.
- nsh's own toasts and Ctrl+S/Ctrl+R search use the bottom row; we draw our own `ui::Toast` and status bar there.

## Theme

- Colours only through tokens: `theme().background`, `theme().foreground`, `theme().color("acc")`, ... Raw hex
  lives in `apps/tui/theme.json` only. Missing role → add it to `theme.json` (and `docs/design/tokens`) first.
- nsh maps colours to ANSI meanings; `coffee-host` pins exact hex at true colour and degrades for 256/16/mono.
  A host that mounts the root itself must apply `Root::tokens` (see `coffee_host::app_root`).

## Checks before "done"

Run in `apps/tui` unless noted, and paste the tails:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings   # repo root
nsh check
nsh test                 # crate tests + headless App.test.tsx
nsh test --target tui    # the same on an in-memory terminal
python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui                 # repo root, after cargo build
```

First nsh builds take minutes and fill `apps/tui/target`; do not delete it to "fix" a build. Indentation is
2 spaces (rustfmt `tab_spaces = 2`, never change it).
