---
name: update-snapshots
description: Create, review and update the terminal cell snapshots of ROAST/TTY screens and UI components (nsh test --target tui, 160x50 / 130x40 / 100x30 / 80x24). Use when a UI change makes a snapshot test fail or a new screen/component needs snapshots.
---

# Update snapshots

Snapshots are what the UI draws on an in-memory terminal, taken by `nsh test --target tui` with
`app.screenshot()` (JSON: `format: "nsh-tui-cells"`, `width`, `height`, `rows[y][x]` with `symbol`, `fg`, `bg`,
`modifiers`). A test opens the app as a Linux desktop (`platform: 'linux'`: the headless default is a phone, whose
shortcuts never fire) at a size in points: cells × 8 wide, cells × 16 high (8×16 points per cell).

| Size (cells) | `open(entry, { platform: 'linux', width, height })` |
| ------------ | --------------------------------------------------- |
| 160×50       | `{ width: 1280, height: 800 }`                      |
| 130×40       | `{ width: 1040, height: 640 }`                      |
| 100×30       | `{ width: 800, height: 480 }`                       |
| 80×24        | `{ width: 640, height: 384 }`                       |

## Where they live

There are **no stored snapshot files**: each test asserts on what is drawn, in TypeScript, one file per subject in
`apps/tui/` (nsh runs every `*.test.tsx` there; vitest never does):

| File                    | Opens                                      | Covers                                                    |
| ----------------------- | ------------------------------------------ | --------------------------------------------------------- |
| `Gallery.test.tsx`      | `./src/ui/gallery.entry.tsx` (the gallery) | every `ui/` component, 14 pages, the four sizes           |
| `App.test.tsx`          | `.` (the app)                              | the shell: stores, router, commands, overlays, scenarios  |
| `<Name>.test.tsx`       | `.` with a fixture scenario in `storage`   | one screen (`Login`, `Catalog`, `Detail`, `Cart`, …)      |

A screen's test picks its data with storage: `coffee.transport` (`fixture:` guest, `fixture:member`,
`fixture:empty`, `fixture:new`, `fixture:problem`, `fixture:expired`, `fixture:member-error`, `fixture:down`,
`fixture:error`; see `apps/tui/src/api/fixtures.rs`) and `coffee.fingerprint` (an SSH key). Fixture answers take
one engine turn: wait for a text (`app.waitFor('text:…')`) before reading the screen.

To look at the components in the real host (pinned colours, any depth):
`COFFEE_UI_GALLERY=1 target/debug/coffee-tui`, or through `scripts/pty-run.py --env COFFEE_UI_GALLERY=1`
(`← →` turn pages).

## How the assertions work

- `app.texts()` (both targets): the texts on screen. The headless run (`nsh test`) draws nothing, so the cell
  helpers return `null` there and a test skips its cell checks.
- On the terminal target the files share small helpers (copy them into a new file): `screenCells(app)` parses the
  screenshot into rows of `Cell { symbol, fg, bg, modifiers }`; `rowsOf(cells)` / `screenRows(app)` give one string
  per row (the blank second cell of a wide character dropped, `isWide` by code point range); `cellsAt(cells, text)`
  returns the cells where `text` starts, so a test checks a span: `expect(cellsAt(c, '▶ 耶加雪菲')![0].fg)`.
- Colours in the JSON are **nsh's**, before `coffee-tui` pins the palette: text tones are ANSI names (`yellow` for
  `acc`, `green` for `acc2`, `reset` for `fg`), fills are their **carrier** RGB (`carrier-selection`, `carrier-kbd`)
  or exact saturated colours (`#f0b45a` under a primary button), and a **marked** tone (`dim`, `line-strong`,
  `disabled`, …) carries `italic` in `modifiers` — the mark the pin removes. Attributes are `bold`, `underlined`,
  `reversed`, `crossed_out`, `dim`, `italic`.
- The pinned result (what a user sees at 24-bit / 256 / 16 colours and mono) is tested in Rust:
  `crates/coffee-tui/tests/pin_render.rs`. Check a real frame with `scripts/pty-run.py --raw FILE`.

## Steps

1. Run the failing suite and read the diff it prints: `cd apps/tui && nsh test --target tui` (or `nsh test` for the
   headless run).
2. Decide: is the change **intended** (your UI change) or a **regression**? If you cannot explain every changed
   cell from your diff, it is a regression — fix the code, not the test.
3. For an intended change, update the expectation by hand in the file that owns it (the gallery for a component, the
   screen's `<Name>.test.tsx` for a page), in the test for that size. Each size needs its own `open(...)`:
   `nsh test --target tui` cannot resize an open app.
4. Check CJK alignment (2 cells per character), the demo notice, status-bar keys, glyph states (`▶ ✓ ✗ ●/○`) and
   that 80×24 still fits.
5. Run both suites: `nsh test && nsh test --target tui` (or `pnpm test:tui` from the root).
6. Cross-check one size for real (repo root, after `cargo build -p coffee-tui`):
   `python3 scripts/pty-run.py --size 100x30 --env COFFEE_API_URL=fixture: -- target/debug/coffee-tui`, plus
   `--env COFFEE_COLORS=16` and `mono`.
7. Commit test changes with the code change that caused them, and say in the commit/report what changed and why.

Never loosen an assertion just to make CI green.
