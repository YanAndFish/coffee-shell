# ROAST/TTY 终端咖啡馆 — notes for coding agents

A **demo** coffee-bean shop you reach with `ssh -p 2222 localhost`: no password, no payment, nothing ships. A Rust
TUI built with [nsh](https://nsh.dev) 0.6.1 (Rust mode) runs one process per SSH session and talks to a local Node
backend. The UI must say it is a demo at all times.

```
ssh client ─ coffee-sshd (russh: any key or none, PTY, session limits)
               └─ one coffee-tui child per session (nsh AppView + crossterm, real PTY)
                      └─ HTTP 127.0.0.1 + X-Internal-Key ─ @coffee/server (Koa + Prisma + SQLite)
```

Why one process per session, the auth model, Ctrl+C, colour pinning and limits: `docs/adr/0001-ssh-session-model.md`.
Screens, keymap, copy, states, responsive rules and tokens: `docs/design/` (start at its `README.md`).

## Map

| Path                 | What                                                             | Notes                                                                             |
| -------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI, **library only**                       | `nsh.json`, `theme.json`, `src/{ui,shell,pages,stores,api,commands.rs,router.rs}` |
| `crates/coffee-host` | bin `coffee-tui`: runs `coffee_app::App` in the current terminal | pins theme colours, colour depth, Ctrl+C twice                                    |
| `crates/coffee-sshd` | bin `coffee-sshd`: SSH front end, one PTY child per session      | end-to-end tests in `tests/`                                                      |
| `crates/coffee-api`  | serde DTOs of the backend API (no nsh)                           | tested against the contract fixtures                                              |
| `apps/server`        | `@coffee/server`: Koa + Prisma + SQLite                          | `HOST`/`PORT` (127.0.0.1:3000), DB `apps/server/.data/coffee.db`                  |
| `packages/contract`  | `@coffee/contract`: zod → `openapi.json` + `fixtures/v1/*.json`  | single source of the API; `FooSchema` / type `Foo`; money in integer yuan         |
| `docs/design`        | screens, keymap, copy, responsive rules, tokens                  | `raw/` are exported originals: read-only                                          |
| `docs/adr`           | architecture decisions                                           |                                                                                   |
| `deploy/`            | gateway image (`coffee-sshd` + `coffee-tui`) and its settings    | `deploy/README.md` lists every `COFFEE_*` variable                                |
| `scripts/`           | `dev.mjs` (`pnpm dev`), `pty-run.py`, `check-mocks.py`           |                                                                                   |
| `.claude/skills`     | how-tos for recurring tasks (see below)                          |                                                                                   |

Frozen interface (change only with every caller): `coffee_app::{App, registry, SessionEnv, configure, session_env,
APP_ID, THEME_JSON}`; root script names `dev build test lint check ssh`.

## Commands (repo root unless noted)

| Command                                                               | Does                                                                                  |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `pnpm install`                                                        | Node deps (pnpm 12, from `packageManager`); also installs the pre-commit hook         |
| `pnpm dev`                                                            | builds `coffee-tui` + `coffee-sshd`, migrates + seeds, starts backend and sshd        |
| `pnpm ssh`                                                            | `ssh -p 2222 localhost`, sends `COLORTERM`, no host-key checks                        |
| `pnpm check`                                                          | eslint, design mock sizes, `tsc`, `cargo fmt --check`, `cargo clippy -D warnings`     |
| `pnpm check:tui`                                                      | `nsh check` in `apps/tui` (cargo check + TS check of `App.test.tsx`)                  |
| `pnpm test`                                                           | `cargo build -p coffee-host`, `cargo test --workspace` (incl. SSH e2e), vitest        |
| `pnpm test:tui`                                                       | `nsh test` and `nsh test --target tui` in `apps/tui`                                  |
| `pnpm test:e2e`                                                       | only the SSH end-to-end tests (`crates/coffee-sshd/tests`)                            |
| `pnpm verify`                                                         | everything CI runs except the Docker build                                            |
| `pnpm lint:fix`                                                       | eslint also formats, Markdown tables included (no Prettier)                           |
| `python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui` | the UI's screen as text (`--env COFFEE_API_URL=fixture:` for fixtures)                |
| `pnpm --filter @coffee/contract generate` / `check:generated`         | regenerate / check `openapi.json` and fixtures                                        |
| `pnpm --filter @coffee/server db:migrate` / `db:seed`                 | backend database (`pnpm dev` runs both); `db:reset` is for humans only (see pitfalls) |

CI (`.github/workflows/ci.yml`, ubuntu-24.04) runs the jobs `node`, `rust`, `nsh` and `docker` (builds
`deploy/Dockerfile`, no push) on every push and PR.

## Layering rules (apps/tui)

- **Only `src/ui/` touches `nsh_base` and theme tokens.** Pages compose `ui::*` and layout primitives; they never
  style base controls, write colours, radii or paddings. Missing look → new variant/size in `ui/` first.
- **One command table** (`commands.rs`: `Command { id, key, label, scope, footer_rank, enabled, run }`) drives key
  bindings (bound once in `shell/mod.rs`), buttons, the footer hints and the `?` help. Never bind a key elsewhere.
- **Stores** are `Copy` structs of private signals (`SessionStore`, `CatalogStore`, `ToastStore`, `Router`; Cart,
  Address, Orders, Settings come in phase 2), bundled in `stores::Stores` (`Stores::provide()` in the root,
  `Stores::get()` in a body); change them through their methods. A page never writes another page's state.
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

## Pinned versions (and why)

| Tool                                            | Version                                      | Why                                                     |
| ----------------------------------------------- | -------------------------------------------- | ------------------------------------------------------- |
| nsh CLI + crates                                | 0.6.1 (`=0.6.1` in `Cargo.toml`)             | API moves fast; CLI and crates must match               |
| Rust                                            | 1.99.1 (`rust-toolchain.toml`), edition 2024 |                                                         |
| Node / pnpm                                     | 26 / 12.9.1                                  | Node 26 runs the TS sources directly (type stripping)   |
| TypeScript                                      | 6.0.3                                        | TS 7 breaks typescript-eslint 8                         |
| eslint / @antfu/eslint-config / vitest / tsdown | 10 / 10 / 5 / 0.23                           |                                                         |
| prisma, @prisma/client, SQLite adapter          | 7.9.1                                        | 7.10.x is refused by pnpm's `trustPolicy: no-downgrade` |

## nsh pitfalls

- `apps/tui` is lib-only (`nsh init`'s desktop `main.rs` pulled GTK): keep it that way. `.nsh/` is generated,
  git-ignored, its own Cargo workspace (excluded from the root one).
- nsh builds into `apps/tui/target` (≈8 GB locally); first builds take minutes. Do not delete it casually.
- Tests (`App.test.tsx`) must `open('.', { platform: 'linux', width: cols * 8, height: rows * 16 })`: the headless
  default is a phone, whose shortcuts never fire. `nsh test --target tui` cannot resize an open app.
- With no host session the app reads the contract fixtures; storage key `coffee.transport` or
  `COFFEE_API_URL=fixture:` / `fixture:empty` / `fixture:down` pick a set (the header shows `◇ 样例数据`).
- Pressable chrome is `view().role(Button).focusable(false)`; key bindings need `KeyOptions { repeat: true }`.
- A bordered view clips its children: the Panel legend is an absolutely positioned sibling.
- nsh's TUI maps colours to ANSI meanings and draws dark text as the terminal's foreground; `coffee-host` pins
  exact hex (and `ink` on `acc` fills). A custom host must apply the theme with `Root::tokens`.
- nsh toasts and Ctrl+S search own the bottom row: we draw our own Toast and status bar.
- The host intercepts Ctrl+C: press it **twice** to quit. Never bind Ctrl+C in the app.
- crossterm needs its `use-dev-tty` feature in `coffee-host`: the default reader spins at 100 % CPU after a hang-up.
- vitest must never pick up `apps/tui/*.test.tsx` (see `vitest.config.ts`).

## Other pitfalls

- `expect` mangles multi-byte output: drive TUIs and SSH sessions with `scripts/pty-run.py`.
- OpenSSH does not forward `COLORTERM`: plain `ssh` gets 256 colours. `pnpm ssh` sends it; servers may force
  `COFFEE_COLORS=truecolor|256|16|mono`. Clients cannot set `COFFEE_COLORS` (sshd takes only `LANG`, `LC_*`,
  `COLORTERM`).
- `prisma migrate reset` refuses to run for an AI agent: `db:reset` is for humans. Never work around that guard.
  Prisma also prints a harmless “unsupported Node” warning on Node 26.
- pnpm 12 takes one script per `pnpm --filter <pkg> <script>`; chain separate commands.
- The repo path may contain non-ASCII characters: quote it; runtime data lives in `.data/` (git-ignored).
- Design mocks (`<!-- mock WxH -->` blocks) are checked by `scripts/check-mocks.py`; keep their
  `<!-- prettier-ignore -->` (trailing spaces are part of the width).

## Style

- **2-space indentation everywhere, Rust included** (`rustfmt.toml` `tab_spaces = 2` is the owner's choice: never
  change it). `.editorconfig` says the same. ESLint formats TS/JSON/YAML/Markdown (`pnpm lint:fix`).
- The pre-commit hook (simple-git-hooks + lint-staged) runs `eslint --fix` and `rustfmt` on staged files.
  `.vscode/settings.json` (picked up by VS Code and its forks; needs the ESLint extension) fixes on save.
- Conventional commits in English. No secrets or personal data in the repo, logs or fixtures.
- Theme colours only through `ui/theme.rs` accessors (`theme::acc`); raw hex lives in `apps/tui/theme.json` alone.

## Working here

- Work is split into slices and **delegated to subagents** with disjoint file ownership; an orchestrator merges and
  re-runs the verification. Stay inside the paths your brief owns; list needed changes elsewhere as change requests.
- Skills: `nsh-tui-conventions`, `add-ui-component`, `add-screen`, `add-api-endpoint`, `run-and-verify-over-ssh`,
  `update-snapshots` in `.claude/skills/`.

## Verification before claiming done

1. `pnpm check` and `pnpm test` (Rust, SSH e2e, vitest); for `apps/tui`, `pnpm check:tui` and `pnpm test:tui`.
2. UI changes: `scripts/pty-run.py` at 160×50, 100×30, 80×24 and 60×20, plus `COFFEE_COLORS=16` and `mono`.
3. SSH or host changes: `pnpm dev`, then `pnpm ssh` through `pty-run.py` with and without a key
   (`-o PubkeyAuthentication=no -o IdentitiesOnly=yes`); resize; Ctrl+C twice; kill the client.
4. Stop everything you started and show `pgrep -fl "dev.mjs|coffee-sshd|coffee-tui|src/main.ts"` is empty.
5. Cross-cutting work: `pnpm verify`. Paste the tails; say what you could not verify. "Compiles" is not done.
