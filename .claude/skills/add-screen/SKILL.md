---
name: add-screen
description: Add or rebuild a ROAST/TTY screen (page) in apps/tui/src/pages - login, shop list, detail, cart, address book, profile - wiring its route, commands, store reads and responsive layouts. Use when implementing one of the six designed screens or a new page.
---

# Add a screen

A screen is one page directory `apps/tui/src/pages/<name>/` (login, catalog, detail, cart, addresses, profile),
composed only from `ui::*` components and layout primitives, reading stores and running commands. Read
`nsh-tui-conventions` first, then `apps/tui/README.md` ("How to build a screen" lists, per screen, the store
functions, actions and keys already there).

## 1. Gather the design

- The canvas export: `docs/design/raw/canvas/<Screen>.dc.html` (Main = login, Shop, Detail, Cart, Address, Profile).
- The cell spec at 160×50: `docs/design/screens/01-login.md`, `02-list.md`, `03-detail.md`, `04-cart.md`,
  `05-address.md`, `06-profile.md`. Shared specs: `docs/design/keymap.md`, `copy.md`, `states.md`,
  `responsive.md`, `components.md`, `data.md`, `tokens.md` (index: `docs/design/README.md`).
- List the screen's commands (5–7 for the status bar, 4 at S), its data (which store) and its states (loading,
  empty `∅`, error `✗`, ready, guest, too small).

## 2. Files (yours alone)

```
apps/tui/src/pages/<name>/mod.rs       # MODULE (keep it), the view (`render`), dialogs; replaces the placeholder body
apps/tui/src/pages/<name>/commands.rs  # COMMANDS: the page's keys, footer ranks, help entries
apps/tui/src/pages/<name>/*.rs         # sub-panels as the page grows
apps/tui/App.test.tsx                  # the screen's tests (shared file: add cases, do not rewrite others)
```

Everything else is shared: `stores/`, `api/`, `commands.rs`, `router.rs`, `shell/`, `pages/{mod,format,
placeholder}.rs`. A missing store function, action or route is a change request to the core owner; a missing look
is a `ui/` change request.

## 3. Steps

1. **Commands.** The page's commands are already registered from keymap.md (keys, labels, scopes, footer ranks,
   `enabled` rules); adjust them in `commands.rs`. Scope `Scope::Page(Page::<Name>)`, or
   `Scope::Overlay(Overlay::..)` for the page's dialog; `Typing::Only` for keys of a focused field (`⏎`, `Esc`);
   `.footer(rank)` / `.footer_full` / `.footer_s` / `.short(..)` place the hint; `Key::hint(..)` shows a key handled
   elsewhere (Tab, a global key). `run` calls store functions or `stores::actions`.
2. **Data.** Read stores in the body (`let stores = Stores::get();`) and signals inside closures. Start loads from
   the page's entry: `effect(move || untracked(|| store.ensure_loaded()))`. Show the backend's numbers only (format
   with `pages::format`).
3. **Fields.** Wire every text field to `UiStore` as `pages::placeholder::field` does (focus, blur, cleanup, focus
   requests); its name is a `pub const` of the page.
4. **Layout.** One layout per breakpoint family, chosen from `Layout::get().bp()`: <80×24 → the shared
   "终端太小" view (the shell does it); 80–99 one column (4 hints + `?`); 100–129 list + preview; 130–159 three
   narrow columns; ≥160 the design layout (max 200 wide); <32 rows compact header. Keep state across size changes.
5. **Copy and glyphs.** Panel titles “中文 ENGLISH”; keys lead labels; status glyphs, not colour alone; the demo
   notice stays visible.

## 4. Test

- Rust unit tests for logic in the page (pure functions); store behaviour is the core's tests.
- Behaviour tests in `apps/tui/App.test.tsx` against the fixture transport (no host configures a session there):
  storage `coffee.transport` picks the scenario (`fixture:` a new visitor on the login page, `fixture:member` with
  `coffee.fingerprint` set = signed in at once, `fixture:empty`, `fixture:down`, `fixture:error`); the app starts on
  the login page (`g` browses as a guest). Cover load → ready, empty, error, guest, each command's effect.
- Open with `platform: 'linux'` and an explicit size; `nsh test --target tui` cannot resize an open app, so test
  each size with its own `open` (160×50, 130×40, 100×30, 80×24; see `update-snapshots`).
- Keys: one test per status-bar command proving the key does what the label says.

## 5. Verify

```sh
cd apps/tui && nsh check && nsh test && nsh test --target tui
```

Then over SSH at each size (see `run-and-verify-over-ssh`): `pnpm dev` and
`python3 scripts/pty-run.py --size 160x50 --step 'until=<a title on the page>' -- ssh -p 2222 ... localhost`,
plus `--env COFFEE_COLORS=mono` for the monochrome check. Compare with the design keymap: every key listed there
works and appears in the status bar or `?` help.
