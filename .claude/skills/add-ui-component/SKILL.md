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
- Tokens: `apps/tui/theme.json` (and `docs/design/tokens*`). <!-- TODO(integration): confirm tokens file path -->
- Translate pixels to cells: 1 cell ≈ 8×16 points; a 1px border is a box-drawing line; shadows do not exist in the
  terminal; use glyph + attribute for state (`▶`, `✓`, `●/○`, bold, reverse, dim).

## 2. Design the API before the code

- Props that vary by **meaning** only: `variant` (an enum: e.g. `Primary | Secondary | Ghost | Danger`), `size`,
  states (`selected`, `disabled`, `loading`, `invalid`, `focused` from the base control), slots (label, key hint,
  children) and events (`on_press`, `on_change`).
- Never colours, radii, paddings or fonts as props. Layout (width, flex, margin) comes from the caller.
- Keyboard: build on the `nsh_base` control that already has focus, press and accessibility (a button, a switch, an
  input) instead of a bare pressable view. If the control has a shortcut, take its `Command` from the command table
  (`commands.rs`) rather than binding a key yourself. <!-- TODO(integration): confirm how ui/ components receive a Command -->

## 3. Write it

1. Create `apps/tui/src/ui/<name>.rs` (snake_case file, PascalCase component). Use `#[component]` for retained
   behaviour/state and a plain `fn` returning an element for value-like pieces (a `Kbd`, a `Tag`).
2. Read colours from tokens only (`theme().color("acc")`, `theme().border`, ...); no hex, no `rgb(...)`.
3. Every state must still read in 16 colours and monochrome: add a glyph or attribute, not just a colour.
4. Measure CJK text in cells (`unicode-width`), and let text containers grow (min height + padding, not fixed height).
5. Export it from `apps/tui/src/ui/mod.rs`. <!-- TODO(integration): confirm ui/mod.rs layout -->
6. Add it to the component gallery route (every variant, size and state) so it can be reviewed in one place.
   <!-- TODO(integration): the gallery route lands in phase 2; confirm its path -->

## 4. Test it

- Rust: a headless test (`nsh_testing::Headless`, or the crate's existing test helper) that presses it, checks
  disabled behaviour, focus, and the texts/glyphs it shows. Find views by role/label/text or `.test_id(...)`.
- Terminal: a case in a `*.test.tsx` run by `nsh test --target tui` that opens the gallery at 160×50, 130×40, 100×30
  and 80×24 (`open(".", { width: cols * 8, height: rows * 16 })`) and compares `app.screenshot("cells.json")` with
  the stored snapshot - see the `update-snapshots` skill.

## 5. Verify

```sh
cd apps/tui && nsh check && nsh test && nsh test --target tui
cd ../.. && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
```

Then look at it for real: `cargo build -p coffee-host && python3 scripts/pty-run.py --size 160x50 -- target/debug/coffee-tui`
(and `--env COFFEE_COLORS=mono`). Grep your diff for `#` hex literals and `rgb(` outside `theme.json`.
