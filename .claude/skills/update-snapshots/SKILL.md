---
name: update-snapshots
description: Create, review and update the terminal cell snapshots of ROAST/TTY screens and UI components (nsh test --target tui, 160x50 / 130x40 / 100x30 / 80x24). Use when a UI change makes a snapshot test fail or a new screen/component needs snapshots.
---

# Update snapshots

Snapshots are what the UI draws on an in-memory terminal, taken by `nsh test --target tui` with
`app.screenshot("cells.json")` (JSON: `format: "nsh-tui-cells"`, `width`, `height`, `rows[y][x]` with `symbol`,
`fg`, `bg`, `modifiers`). A test opens the app at a size in points: cells × 8 wide, cells × 16 high (the default
8×16 points per cell).

| Size (cells) | `open(".", { width, height })` |
|---|---|
| 160×50 | `{ width: 1280, height: 800 }` |
| 130×40 | `{ width: 1040, height: 640 }` |
| 100×30 | `{ width: 800, height: 480 }` |
| 80×24 | `{ width: 640, height: 384 }` |

## Where they live

`apps/tui/tests/snapshots/<screen-or-component>@<cols>x<rows>.txt` (plain text: one line per row, symbols only,
easy to review in a diff) and, where colours/attributes matter, a `.cells.json` next to it.
<!-- TODO(integration): confirm the snapshot directory, file format and the helper that compares them -->

## Steps

1. Run the failing test and read the diff it prints:
   `cd apps/tui && nsh test --target tui --filter "<test name>"`.
2. Decide: is the change **intended** (your UI change) or a **regression**? If you cannot explain every changed
   cell from your diff, it is a regression - fix the code, not the snapshot.
3. Regenerate only the affected snapshots. The helper rewrites files when `UPDATE_SNAPSHOTS=1` is set:
   `UPDATE_SNAPSHOTS=1 nsh test --target tui --filter "<test name>"`.
   <!-- TODO(integration): confirm the environment variable reaches tests (nsh.json capabilities.process.env) -->
4. Review with `git diff apps/tui/tests/snapshots` - check CJK alignment (2 cells per character), the demo
   notice, status-bar keys, glyph states (`▶ ✓ ✗ ●/○`) and that 80×24 still fits.
5. Run the full suites without the update flag: `nsh test && nsh test --target tui` - they must pass unchanged.
6. Cross-check one size for real: `python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui` (repo root,
   after `cargo build -p coffee-host`) should match the 100×30 text snapshot.
7. Commit snapshots with the code change that caused them, and say in the commit/report which ones changed and why.

Never bulk-update every snapshot to make CI green, and never update snapshots in CI.
