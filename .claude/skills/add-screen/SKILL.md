---
name: add-screen
description: Add or rebuild a ROAST/TTY screen (page) in apps/tui/src/pages - login, shop list, detail, cart, address book, profile - wiring its route, commands, store reads and responsive layouts. Use when implementing one of the six designed screens or a new page.
---

# Add a screen

A screen is one page module under `apps/tui/src/pages/` (`<name>.rs`, or `<name>/mod.rs` once it has sub-panels), composed only from `ui::*` components and layout
primitives, reading stores and running commands. Read `nsh-tui-conventions` first.

## 1. Gather the design

- The canvas export: `docs/design/raw/canvas/<Screen>.dc.html` (Main = login, Shop, Detail, Cart, Address, Profile).
- The cell spec at 160×50: `docs/design/screens/01-login.md`, `02-list.md`, `03-detail.md`, `04-cart.md`,
  `05-address.md`, `06-profile.md`. Shared specs: `docs/design/keymap.md`, `copy.md`, `states.md`,
  `responsive.md`, `components.md`, `data.md`, `tokens.md` (index: `docs/design/README.md`).
- List the screen's commands (5–7 for the status bar), its data (which store, which endpoint) and its states
  (loading, empty `∅`, error `✗`, ready, too small).

## 2. Files

```
apps/tui/src/pages/<name>.rs          # the page: layout per breakpoint, composes ui::* (catalog.rs is the model)
apps/tui/src/pages/<name>/mod.rs      # instead, once the page has sub-panels (<name>/*.rs)
apps/tui/src/pages/mod.rs             # `pages::page(&Route) -> Screen`: map the route to the page
apps/tui/src/commands.rs              # the page's commands (scope = its `router::Page`)
apps/tui/src/router.rs                # `Route` variant (+ `Page` kind, mode name, tab)
apps/tui/src/stores/<store>.rs        # only if the page needs new store methods; new stores join `stores::Stores`
```

Today `Route::Catalog` is the real page (`pages/catalog.rs`); `Login`, `Detail`, `Cart`, `Addresses` and `Profile`
render honest placeholders from `pages/placeholder.rs`: replace the placeholder arm in `pages/mod.rs`.

## 3. Steps

1. **Commands first.** Add each action to the command table with its key (from the keymap), Chinese label, scope,
   `footer_rank` (status-bar order; 5–7 visible) and an `enabled` rule. The status bar and `?` help pick them up.
2. **Route.** Add the route and its number key / entry point; Esc returns to the previous route.
3. **Data.** Read from stores in the component body (`let stores = Stores::get();`) and from signals inside
   closures. Start loads from the route's entry (an effect or resource), never unconditionally in the body. The page
   must never compute prices or totals: show the backend's numbers.
4. **Layout.** One component per breakpoint family, chosen from the terminal size: <80×24 → the shared
   "终端太小" view; 80–99 one column (preview hidden, filters in a dialog, 4 hints + `?`); 100–129 list + preview;
   130–159 three narrow columns; ≥160 the design layout (max 200 wide); <32 rows compact header. Keep state across
   size changes.
5. **Copy and glyphs.** Panel titles “中文 ENGLISH”; keys lead labels; status glyphs, not colour alone; the demo
   notice stays visible.

## 4. Test

- Behaviour tests in `apps/tui/App.test.tsx` against the contract fixtures (no host configures a session there, so
  the app uses `packages/contract/fixtures/v1/*.json`; storage `{ 'coffee.transport': 'fixture:empty' }` or
  `'fixture:down'` picks the empty or failing set): load → ready, empty, error, each command's effect.
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
