---
name: add-ui-component
description: Add or extend a styled component in apps/tui/src/ui (the ROAST/TTY design-system layer over nsh_base), e.g. Button, Panel, Chip, Stepper, Toast, Dialog. Use when a page needs a control or look that ui/ does not have yet.
---

# Add a UI component

`apps/tui/src/ui/` is the only layer that touches `nsh_base` and theme tokens. Pages use what it exports
(`ui::*`, re-exported from `ui/mod.rs`). Read the `nsh-tui-conventions` skill first.

## 1. Find the spec

- Terminal spec of every component: `docs/design/components.md`, including the table **实现与规格的差异** at its end
  (what nsh 0.6.1 cannot draw and what `ui/` does instead). Tokens: `docs/design/tokens.md`, including **终端载体**.
- Source: `docs/design/raw/design-system/components/<Name>/README.md` and `components/bundle.css` (class prefix
  `t-`). Screens that use it: `docs/design/screens/*.md` (160×50 and 80×24 mocks with exact cells).
- Look at the gallery first: it shows every component in every state.
  `cargo build -p coffee-host && python3 scripts/pty-run.py --size 160x50 --env COFFEE_UI_GALLERY=1 --env
  COFFEE_API_URL=fixture: --step key=right -- target/debug/coffee-tui` (`←/→` turn pages; see `ui/gallery.rs`).

## 2. What already exists (one file each)

| File | Public API |
| --- | --- |
| `label.rs` | `label(text, Tone)`, `strong(text, Tone)`, `toned(text, Fn -> Tone)`, `toned_strong(..)`: the only way to colour text |
| `theme.rs` | `Tone` (text/line roles), `Fill` (backgrounds), `INKS` / `FILLS` (the carrier tables the host's pin reads) |
| `frame.rs` | `frame(style, tone, legend, rows, pad, content)`, `FrameStyle::{Round, Heavy, Double, Dashed}`, `Legend`, `line_run(glyph, tone)` |
| `layout.rs` | `Layout`, `Viewport`, `Bp`, `cols(n)` / `rows(n)`, `hspace(n)` / `vspace(n)`, `gap_cols(n)` / `pad_cols(n)` |
| `text.rs` | `display_width`, `truncate`, `fit`, `center`, `pad_start` / `pad_end`, `repeat` |
| `button.rs` | `Button::new(ButtonKind, key, label)` + `.key_cap()` `.key_after()` `.arrow()` `.width(n)` `.disabled(f)` `.focused(f)` `.on_press(f)`; `button(..)` |
| `kbd.rs` | `kbd(key)`, `kbd_wide(key)`, `kbd_on_fill(key)` |
| `tag.rs` | `tag(TagKind, text)`, `count_badge(f)`, `pill(Fill, content)`, `pill_text(Fill, text, padded)` |
| `chip.rs` | `chip(text, ChipPad, selected, available, on_press)`, `segmented(..)`, `sku_option(..)`, `group_label(..)`, `section_head(..)` |
| `toggle.rs` | `checkbox(..)`, `toggle_note(..)`, `switch(..)`, `switch_row(..)`, `radio(..)` |
| `stepper.rs` | `stepper(StepperForm, value, min, max, on_dec, on_inc)`, `stock_note(stock)`, `max_quantity(stock)` |
| `input.rs` | `input(InputSpec, value, focused, error, on_change, on_focus_change, on_submit)`; `InputForm::{Prompt, Plain, Search}`, `LabelPlace` |
| `panel.rs` | `panel(legend, content)`, `panel_toned`, `panel_plain`, `card(Legend, cols, rows, content)`, `inset`, `heading`, `rule` |
| `data_row.rs` | `table_head`, `table_rule`, `data_row`, `name_cell`, `cart_row(CartLine, CartForm, ..)`, `order_row`, `pips` |
| `address_card.rs` | `address_card(AddressView, cols, rows, current, actions, on_press)`, `add_card`, `mask_phone` |
| `leader.rs` / `meter.rs` / `radar.rs` | `leader`, `receipt_leader`; `meter`, `bar`, `thin_bar`, `progress`; `radar(values, tone, fits, label)`, `radar_fits` |
| `toast.rs` / `dialog.rs` | `toast(current)`, `toast_box`; `dialog(open, title, DialogSize, content, footer)`, `dialog_box`, `confirm(open, ConfirmText, DialogFocus, ..)`, `DialogFocus` |
| `status_bar.rs` / `nav_tabs.rs` / `window.rs` / `wordmark.rs` | `mode_line(mode, hints, right, on_hint)`, `status_bar` (phase 1); `nav_tabs_with`, `nav_rule`; `system_row`, `content_margins`; `wordmark_block`, `wordmark_line`, `wordmark`, `wordmark_for` |
| `empty_state.rs` / `error_state.rs` / `loading.rs` / `art.rs` | `empty_state_with(EmptyArt, ..)`; `error_state`, `error_line`; `spinner(animate)`, `loading`, `loading_live`; `cup`, `steam`, `empty_cup` |

## 3. Design the API before the code

- Props that vary by **meaning** only: a kind enum (`ButtonKind`, `TagKind`), a width in cells when the design
  stretches it, states as closures (`selected`, `disabled`, `focused`, `current`: `impl Fn() -> bool`, tracked), slots
  (text, children) and handlers (`on_press`, `on_change`). Never colours, paddings or fonts as props.
- Keyboard: every action has a key in the command table (`commands.rs`), bound once in `shell/mod.rs`. A `ui/`
  component never imports `commands` or binds a key: it takes the key glyph and label as values and the handler the
  key also runs. Pressable chrome is `ui::pressable()` (`role(Button).focusable(false)`) so it never takes Enter or
  Space. Only text inputs take nsh's focus, driven by the caller's focus ring (`input`'s `focused()`), reporting it
  back (`on_focus_change`). Keys that must also act inside a field (Esc, Tab, Shift+Tab, ⏎) are bound with
  `KeyOptions { when: When::Always, .. }`.
- A dialog is an overlay with a `DialogFocus` ring (the focus trap); its keys stay in the command table.

## 4. Write it — the nsh 0.6.1 terminal rules that bite

1. `apps/tui/src/ui/<name>.rs` (snake_case file), a plain `fn` returning `View`/`AnyElement` or a builder implementing
   `IntoElement` (`Button`). `#[component]` only for a root (`UiGallery`). Rustdoc: purpose, the drawing in a `text`
   block, props, states, an example.
2. **Colours**: only `Tone` / `Fill` through `ui::label` & co. They are carriers (`theme.rs`, "Carriers"): a marked tone
   is drawn italic and the host's pin turns it into the token. A new colour = a new row in `theme.json`, in `INKS` or
   `FILLS`, in `docs/design/tokens.md`, and a case in `crates/coffee-host/tests/pin_render.rs`.
3. **Frames**: nsh draws only thin borders and only on boxes it deems cards; use `frame()` (glyph frames) for anything
   else, with a known height when you have one.
4. **Spacing**: horizontal padding is at most 2 columns (one on a box of text), vertical padding under 24 pt and gaps
   under 16 pt are 0 rows, gaps inside framed boxes are 0: use `hspace(n)` / `vspace(n)` for exact cells, `gap_cols(1|2)`
   for row gaps. An explicit height only counts on an empty box (or one whose children are all absolute).
5. **Fills** are drawn only on boxes under 3 rows. Never put a saturated fill on a pressable view (nsh paints its text
   in the fill's accent): put the fill on a child.
6. **Text**: a one-line text cut at its box ends in `…` unless it is only line glyphs; runs of `─━═╌┈` clipped by a box
   draw cleanly (`line_run`); add `.text_overflow(EllipsizeMode::Clip)` with `number_of_lines(1)`. A text consisting only
   of `─` is a divider nsh joins to frames. nsh clears braille next to a text's box: keep a blank (non-text) cell
   between braille and text.
7. Overlays (`overlay().absolute().inset(px(0.0))`) need a box with an opaque fill to clear what is behind; give that
   box `role(AccessibilityRole::Summary)` or nsh frames it itself.
8. State = glyph + attribute (bold, underline, reverse via a fill, strike-through); check 16 colours and mono.
9. Measure CJK with `ui::text::display_width`; nothing may overflow at 80 columns.
10. Export it from `ui/mod.rs` (`pub mod` and `pub use`, alphabetical) and add it to the gallery (`ui/gallery.rs`): a
    page or a block showing every variant and state, fitting 80×24.

## 5. Test it

- Rust `#[cfg(test)]` for every pure helper (widths, layout maths, geometry).
- `apps/tui/Gallery.test.tsx`: opens `./src/ui/gallery.entry.tsx` at 160×50, 130×40, 100×30, 80×24 (`platform:
  'linux'`, explicit size); texts headless, cells (`symbol`, `fg`, `bg`, `modifiers`) on `--target tui`. Colours in that
  JSON are nsh's own (carriers, `italic` marks); pinned colours are checked in `pin_render.rs`.
- `crates/coffee-host/tests/pin_render.rs`: a new token at true colour, 256, 16 and mono.

## 6. Verify

```sh
cd apps/tui && nsh check && nsh test && nsh test --target tui
cd ../.. && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
cargo test -p coffee-app -p coffee-host && pnpm lint
```

Then look at it for real, at every depth: `cargo build -p coffee-host && python3 scripts/pty-run.py --size 160x50
--env COFFEE_UI_GALLERY=1 --env COFFEE_COLORS=mono -- target/debug/coffee-tui` (`--raw FILE` keeps the SGR bytes).
Grep your diff for `#` hex literals and `rgb(` outside `theme.json`.
