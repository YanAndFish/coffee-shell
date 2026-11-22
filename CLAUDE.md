# ROAST/TTY 终端咖啡馆 — notes for coding agents

A **demo** coffee-bean shop you reach with `ssh -p 2222 localhost`: no password, no payment, nothing ships. A Rust
TUI built with [nsh](https://nsh.dev) 0.6.1 (Rust mode) runs one process per SSH session and talks to a local Node
backend. The UI must say it is a demo at all times.

```
ssh client ─ coffee-sshd (russh: any key or none, PTY, session limits)
               └─ one coffee-tui child per session (nsh AppView + crossterm, real PTY)
                      └─ HTTP 127.0.0.1 + X-Internal-Key ─ @coffee/server (Koa + Prisma + SQLite)
```

## Map

| Path                 | What                                                             | Notes                                                                             |
| -------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI, **library only**                       | `nsh.json`, `theme.json`, `src/{ui,shell,pages,stores,api,commands.rs,router.rs}` |
| `crates/coffee-host` | bin `coffee-tui`: runs `coffee_app::App` in the current terminal | pins theme colours, colour-depth detection                                        |
| `crates/coffee-sshd` | bin `coffee-sshd`: SSH front end, one PTY child per session      | end-to-end tests in `tests/`                                                      |
| `crates/coffee-api`  | serde DTOs of the backend API (no nsh)                           | tested against the contract fixtures                                              |
| `apps/server`        | `@coffee/server`: Koa + Prisma + SQLite                          | listens on `HOST`/`PORT` (127.0.0.1:3000)                                         |
| `packages/contract`  | `@coffee/contract`: zod → `openapi.json` + `fixtures/v1/*.json`  | single source of the API                                                          |
| `docs/design`        | screens, keymap, copy, responsive rules, tokens                  | `raw/` are exported originals: read-only                                          |
| `scripts/`           | `dev.mjs` (`pnpm dev`), `pty-run.py` (drive a TUI on a PTY)      |                                                                                   |
| `.claude/skills`     | how-tos for recurring tasks (see below)                          |                                                                                   |

Frozen interface (change only with every caller): `coffee_app::{App, registry, SessionEnv, configure, session_env,
APP_ID, THEME_JSON}`; root script names `dev build test lint check ssh`.

## Commands (repo root unless noted)

| Command                                                               | Does                                                                                              |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `pnpm install`                                                        | Node deps (pnpm 12, from `packageManager`)                                                        |
| `pnpm dev`                                                            | builds `coffee-tui` + `coffee-sshd`, starts backend and sshd with a per-run `COFFEE_INTERNAL_KEY` |
| `pnpm ssh`                                                            | `ssh -p 2222 localhost` without host-key checks                                                   |
| `pnpm check`                                                          | eslint, `tsc`, `cargo fmt --check`, `cargo clippy -D warnings`                                    |
| `pnpm check:tui`                                                      | `nsh check` in `apps/tui` (cargo check + TS check of `App.test.tsx`)                              |
| `pnpm test`                                                           | `cargo build -p coffee-host`, `cargo test --workspace` (incl. SSH e2e), vitest                    |
| `pnpm test:tui`                                                       | `nsh test` and `nsh test --target tui` in `apps/tui`                                              |
| `pnpm test:e2e`                                                       | only the SSH end-to-end tests (`crates/coffee-sshd/tests`)                                        |
| `pnpm verify`                                                         | everything CI runs; run it before saying "done" on cross-cutting work                             |
| `pnpm lint:fix`                                                       | eslint also formats (no Prettier)                                                                 |
| `cargo run -p coffee-host --bin coffee-tui`                           | the UI alone in this terminal (backend optional)                                                  |
| `python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui` | print the UI's screen as text                                                                     |
| `pnpm --filter @coffee/server db:migrate` / `db:seed` / `db:reset`    | backend database                                                                                  |

CI (`.github/workflows/ci.yml`) runs the jobs `node`, `rust` and `nsh` on every push and PR.

## Layering rules (apps/tui)

- **Only `src/ui/` touches `nsh_base` and theme tokens.** Pages compose `ui::*` and layout primitives; they never
  style base controls, write colours, radii or paddings. Missing look → new variant/size in `ui/` first.
- **One command table** (`commands.rs`: `Command { id, key, label, scope, footer_rank, enabled, run }`) drives key
  bindings, buttons, the footer hints and the `?` help. Never bind a key anywhere else.
- **Stores** are `Copy` structs of private signals (Session, Catalog, Cart, Address, Orders, Settings, Toast) given via
  `provide_context`; read them in component bodies, change them through their methods. A page never writes another
  page's state.
- **The TUI never computes money.** Prices, discounts, shipping and totals come from the backend as given.
- Async work starts from events, effects or resources, with explicit loading / empty / error states.
- Component bodies run once; only bindings (closures) re-run. Read signals inside closures, not in the body.

## Contract first

1. Change the zod schema in `packages/contract`, regenerate `openapi.json` and `fixtures/v1/*.json` (commit both).
2. Mirror the DTO in `crates/coffee-api`; its tests deserialize every fixture.
3. Implement the route in `apps/server`; vitest checks real responses against the schema.
4. Use it from `apps/tui/src/api` and a store. See `.claude/skills/add-api-endpoint`.

## UI rules

- Every action has a key, listed in the footer and in `?` help. Tab/arrows move focus; Enter activates; Esc goes back.
- State is glyph + attribute (`▶ ✓ ✗ ●/○ [✓]/[ ]`, bold, reverse), **never colour alone**; check 16 colours and
  `COFFEE_COLORS=mono`.
- The demo notice is always visible (top bar or status bar, login, receipts); login/register say “请勿使用真实密码”.
- Copy is Simplified Chinese, short, “你”; panel titles are “中文 ENGLISH” (`购物车 CART`); keys lead labels (`a 加入购物车`).
- Breakpoints (cells): <80×24 “终端太小” page; 80–99 one column; 100–129 list + preview; 130–159 three narrow columns;
  ≥160 the design layout (max 200 wide); <32 rows compact header. Measure CJK with `unicode-width` (2 cells).
- Design references: `docs/design/` and `docs/design/raw/design-system/README.md`.

## Pinned versions (and why)

| Tool                                            | Version                                      | Why                                          |
| ----------------------------------------------- | -------------------------------------------- | -------------------------------------------- |
| nsh CLI + crates                                | 0.6.1 (`=0.6.1` in `Cargo.toml`)             | API moves fast; CLI and crates must match    |
| Rust                                            | 1.99.1 (`rust-toolchain.toml`), edition 2024 |                                              |
| Node / pnpm                                     | 26 / 12.9.1                                  | Node 26 has `crypto.argon2` (no native deps) |
| TypeScript                                      | 6.0.3                                        | TS 7 breaks typescript-eslint 8              |
| eslint / @antfu/eslint-config / vitest / tsdown | 10 / 10 / 5 / 0.23                           |                                              |
| prisma, @prisma/client, SQLite adapter          | 7.10.1                                       | `latest` on npm is an 8.0 RC                 |

## nsh pitfalls

- `nsh init` makes a desktop `main.rs` with GTK deps: removed. `apps/tui` is lib-only; keep it that way (Linux CI).
- `.nsh/` is generated and git-ignored and has its own Cargo workspace; the root workspace excludes `apps/tui/.nsh`.
- nsh builds into `apps/tui/target` (≈8 GB locally); first builds take minutes. Do not delete it casually.
- `nsh check` = cargo check + nsh's own TS 7 check of `App.test.tsx`; `nsh test` = crate tests + headless TSX test;
  `--target tui` = the same on an in-memory terminal (`app.screenshot("cells.json")` for cell snapshots).
- nsh's TUI maps colours to ANSI meanings; `coffee-host` pins exact hex. A custom host must apply the theme with
  `Root::tokens` (see `coffee_host::app_root`).
- nsh toasts and Ctrl+S search own the bottom row: we draw our own Toast and status bar.
- vitest must never pick up `apps/tui/*.test.tsx` (see `vitest.config.ts`).
- `expect` mangles multi-byte output: use `scripts/pty-run.py` to drive TUIs and SSH sessions.
- The repo path may contain non-ASCII characters; keep SQLite files and other runtime data in `.data/` (git-ignored).

## Style

- **2-space indentation everywhere, Rust included** (`rustfmt.toml` `tab_spaces = 2` is the owner's choice: never
  change it). `.editorconfig` says the same. ESLint formats TS/JSON/YAML/Markdown (`pnpm lint:fix`).
- Conventional commits in English. No secrets or personal data in the repo, logs or fixtures.
- Theme colours only through tokens (`theme().color("acc")`); raw hex lives in `apps/tui/theme.json` alone.

## Working here

- Work is split into slices and **delegated to subagents** with disjoint file ownership; an orchestrator merges and
  re-runs the verification. Stay inside the paths your brief owns; list needed changes elsewhere as change requests.
- Before claiming done: run the checks for what you touched (`pnpm check`, `pnpm test`, `pnpm check:tui`,
  `pnpm test:tui`; for UI, `scripts/pty-run.py` at 160×50, 100×30 and 80×24) and paste the tails. Say what you could
  not verify. "Compiles" is not done.
- Skills: `nsh-tui-conventions`, `add-ui-component`, `add-screen`, `add-api-endpoint`, `run-and-verify-over-ssh`,
  `update-snapshots` in `.claude/skills/`.
