---
name: add-ui-component
description: Add or extend a styled component in apps/tui/src/ui (the ROAST/TTY design-system layer over nsh_base), e.g. Button, Panel, Chip, Stepper, Toast, Dialog. Use when a page needs a control or look that ui/ does not have yet.
---

# Add a UI component

`apps/tui/src/ui/` is the only layer that touches `nsh_base` and theme tokens. Pages use what it exports. Read the
`nsh-tui-conventions` skill first.

## 1. Find the spec

- Design system rules: `docs/design/raw/design-system/README.md` (Chinese; colours, glyphs, states, motion).
- The component's own spec: `docs/design/raw/design-system/components/<Name>/README.md` and its CSS in
  `components/bundle.css` (class prefix `t-`). Components: Wordmark, Window, StatusBar, NavTabs, Panel, Button, Kbd,
  Input, Toggle, Chip, Stepper, Tag, DataRow, AddressCard, Leader, Meter, Radar, Toast, Dialog, EmptyState, Cover.
- Tokens: hex values in `apps/tui/theme.json`; their accessors in `apps/tui/src/ui/theme.rs`; roles, 16-colour and
  mono fallbacks in `docs/design/tokens.md`. Per-component terminal specs: `docs/design/components.md`.
- Translate pixels to cells: 1 cell ≈ 8×16 points; a 1px border is a box-drawing line; shadows do not exist in the
  terminal; use glyph + attribute for state (`▶`, `✓`, `●/○`, bold, reverse, dim).

## 2. Design the API before the code

- Props that vary by **meaning** only: `variant` (an enum: e.g. `Primary | Secondary | Ghost | Danger`), `size`,
  states (`selected`, `disabled`, `loading`, `invalid`, `focused` from the base control), slots (label, key hint,
  children) and events (`on_press`, `on_change`).
- Never colours, radii, paddings or fonts as props. Layout (width, flex, margin) comes from the caller.
- Keyboard: every action already has a key in the command table (`commands.rs`), bound once in `shell/mod.rs`.
  A `ui/` component never imports `commands` or binds a key: it takes the key glyph and label as values and a
  callback (`ui::action(key, label, on_press)`, `ui::status_bar(.., on_hint)` with `Hint { id, key, text }`); the
  page passes `move || commands::press(&stores, id)`. Pressable chrome is
  `view().role(AccessibilityRole::Button).focusable(false)` so it never takes Enter/Space from the keys. Text input
  controls (phase 2) build on the `nsh_base` control that has focus and editing.

## 3. Write it

1. Create `apps/tui/src/ui/<name>.rs` (snake_case file, PascalCase component). Use `#[component]` for retained
   behaviour/state and a plain `fn` returning an element for value-like pieces (a `Kbd`, a `Tag`).
2. Read colours from the `ui/theme.rs` accessors only (`.text_color(theme::acc)`, `.bg(theme::surface)`, `Tone`);
   no hex, no `rgb(...)`. A bordered view clips its children: an overlapping legend is an absolutely positioned
   sibling (see `ui/panel.rs`).
3. Every state must still read in 16 colours and monochrome: add a glyph or attribute, not just a colour.
4. Measure CJK text in cells (`unicode-width`), and let text containers grow (min height + padding, not fixed height).
5. Export it from `apps/tui/src/ui/mod.rs`: a `pub mod <name>;` line and a `pub use <name>::{...};` line, both
   kept in alphabetical order.
6. There is no component gallery yet: phase 2 adds one (its route and path are not decided). Until then, cover
   every variant and state from `apps/tui/App.test.tsx`.

## 4. Test it

- Rust: `#[cfg(test)]` unit tests for pure logic (widths, fitting, formatting), as `ui/status_bar.rs` does.
- Behaviour: a case in `apps/tui/App.test.tsx` (`nsh/test`): `open('.', { platform: 'linux', width: cols * 8,
  height: rows * 16 })` - always `platform: 'linux'` (the headless default is a phone, whose shortcuts never fire)
  and an explicit size. Find views by text/role/label, press keys, check texts and glyphs. `nsh test` runs it
  headless; `nsh test --target tui` runs it on an in-memory terminal, where `app.screenshot()` returns the cells
  (`symbol`, `fg`, `bg`, `modifiers`) - see the `screenCells` helper and the `update-snapshots` skill.

## 5. Verify

```sh
cd apps/tui && nsh check && nsh test && nsh test --target tui
cd ../.. && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
```

Then look at it for real: `cargo build -p coffee-host && python3 scripts/pty-run.py --size 160x50 -- target/debug/coffee-tui`
(and `--env COFFEE_COLORS=mono`). Grep your diff for `#` hex literals and `rgb(` outside `theme.json`.
