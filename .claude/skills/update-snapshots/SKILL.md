---
name: update-snapshots
description: Create, review and update the terminal cell snapshots of ROAST/TTY screens and UI components (nsh test --target tui, 160x50 / 130x40 / 100x30 / 80x24). Use when a UI change makes a snapshot test fail or a new screen/component needs snapshots.
---

# Update snapshots

Snapshots are what the UI draws on an in-memory terminal, taken by `nsh test --target tui` with
`app.screenshot("cells.json")` (JSON: `format: "nsh-tui-cells"`, `width`, `height`, `rows[y][x]` with `symbol`,
`fg`, `bg`, `modifiers`). A test opens the app as a Linux desktop (`platform: 'linux'`: the headless default is a
phone, whose shortcuts never fire) at a size in points: cells × 8 wide, cells × 16 high (8×16 points per cell).

| Size (cells) | `open(".", { platform: 'linux', width, height })` |
|---|---|
| 160×50 | `{ width: 1280, height: 800 }` |
| 130×40 | `{ width: 1040, height: 640 }` |
| 100×30 | `{ width: 800, height: 480 }` |
| 80×24 | `{ width: 640, height: 384 }` |

## Where they live today

There are **no stored snapshot files yet**. The tests in `apps/tui/App.test.tsx` assert on what is drawn:

- `app.texts()` (both targets) for the texts on screen;
- on the terminal target, `app.screenshot()` returns the cell JSON (headless it draws nothing, so the helpers
  return `null` and the test skips that part): `screenRows(app)` turns it into one string per row (wide
  characters' blank second cells dropped), `screenCells(app)` gives the cells with `fg`, `bg` and `modifiers`.
  `app.screenshot("x.json")` also saves the JSON at that path (relative to `apps/tui`).

nsh's colours in that JSON are its own ANSI meanings and exact fills (`"reset"`, `"darkgray"`, `"#f0b45a"`), before
`coffee-host` pins the palette; check the pinned result over a PTY (`scripts/pty-run.py --raw`).

Stored per-size snapshot files (with a compare-and-update helper) arrive with the phase 2 component gallery and
screens; their directory, format and update switch are not decided yet. Whoever adds them updates this skill.

## Steps (today)

1. Run the failing test and read the diff it prints:
   `cd apps/tui && nsh test --target tui` (or `nsh test` for the headless run).
2. Decide: is the change **intended** (your UI change) or a **regression**? If you cannot explain every changed
   cell from your diff, it is a regression - fix the code, not the test.
3. For an intended change, update the expectation in `App.test.tsx` by hand, in the test that covers that size.
   Each size needs its own `open(...)`: `nsh test --target tui` cannot resize an open app (the resize test returns
   early there; `nsh test` covers it headless).
4. Check CJK alignment (2 cells per character), the demo notice, status-bar keys, glyph states (`▶ ✓ ✗ ●/○`) and
   that 80×24 still fits.
5. Run both suites: `nsh test && nsh test --target tui` (or `pnpm test:tui` from the root).
6. Cross-check one size for real: `python3 scripts/pty-run.py --size 100x30 --env COFFEE_API_URL=fixture: --
   target/debug/coffee-tui` (repo root, after `cargo build -p coffee-host`).
7. Commit test changes with the code change that caused them, and say in the commit/report what changed and why.

Never loosen an assertion just to make CI green.
