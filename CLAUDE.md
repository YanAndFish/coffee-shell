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
Screens, keymap, copy, states, responsive rules and tokens: `docs/design/` (start at its `README.md`); product and
API decisions of phase 2: `docs/design/decisions.md`. The API contract and its conventions: `packages/contract/README.md`.

## Map

| Path                 | What                                                             | Notes                                                                             |
| -------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI, **library only**                       | `nsh.json`, `theme.json`, `src/{ui,shell,pages,stores,api,commands.rs,router.rs}` |
| `crates/coffee-host` | bin `coffee-tui`: runs `coffee_app::App` in the current terminal | pins theme colours, colour depth, Ctrl+C twice                                    |
| `crates/coffee-sshd` | bin `coffee-sshd`: SSH front end, one PTY child per session      | end-to-end tests in `tests/`                                                      |
| `crates/coffee-api`  | serde DTOs of the backend API (no nsh)                           | tested against the contract fixtures                                              |
| `apps/server`        | `@coffee/server`: Koa + Prisma + SQLite, all 29 endpoints        | `routes/<area>.ts` + `services/<area>.ts`; DB `apps/server/.data/coffee.db`       |
| `packages/contract`  | `@coffee/contract`: zod → `openapi.json` + `fixtures/v1/*.json`  | single source of the API; `FooSchema` / type `Foo`; money in integer yuan         |
| `docs/design`        | screens, keymap, copy, states, responsive rules, tokens          | `raw/` are exported originals: read-only                                          |
| `docs/adr`           | architecture decisions                                           |                                                                                   |
| `deploy/`            | `Dockerfile` (gateway), `server.Dockerfile`, `compose.yaml`      | `smoke-test.sh`, `server/` (entry point); `README.md` lists every `COFFEE_*`      |
| `scripts/`           | `dev.mjs` (`pnpm dev`), `pty-run.py`, `check-mocks.py`           |                                                                                   |

Frozen interface (change only with every caller): `coffee_app::{App, registry, SessionEnv, configure, session_env,
APP_ID, THEME_JSON}`; root script names `dev build test lint check ssh`.

## Commands (repo root unless noted)

| Command                                                               | Does                                                                                  |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `pnpm install`                                                        | Node deps (pnpm 12, from `packageManager`); also installs the pre-commit hook         |
| `pnpm dev`                                                            | builds `coffee-tui` + `coffee-sshd`, migrates + seeds, starts backend and sshd        |
| `pnpm ssh`                                                            | `ssh -p 2222 localhost`, sends `COLORTERM`, no host-key checks                        |
| `pnpm check`                                                          | eslint, design mock sizes, `tsc`, `cargo fmt --check`, `cargo clippy -D warnings`     |
| `pnpm check:tui`                                                      | `nsh check` in `apps/tui` (cargo check + TS check of the `*.test.tsx`)                |
| `pnpm test`                                                           | `cargo build -p coffee-host`, `cargo test --workspace` (incl. SSH e2e), vitest        |
| `pnpm test:tui`                                                       | `nsh test` and `nsh test --target tui` in `apps/tui`                                  |
| `pnpm test:e2e`                                                       | only the SSH end-to-end tests (`crates/coffee-sshd/tests`)                            |
| `pnpm verify`                                                         | everything CI runs except the Docker build                                            |
| `python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui` | the UI's screen as text (`--env COFFEE_API_URL=fixture:…`, `COFFEE_UI_GALLERY=1`)     |
| `pnpm --filter @coffee/contract generate` / `check:generated`         | regenerate / check `openapi.json` and fixtures                                        |
| `pnpm --filter @coffee/server db:migrate` / `db:seed`                 | backend database (`pnpm dev` runs both); `db:reset` is for humans only (see pitfalls) |

CI (`.github/workflows/ci.yml`, ubuntu-24.04) runs the jobs `node`, `rust`, `nsh` and `docker` (builds both
images, `deploy/Dockerfile` and `deploy/server.Dockerfile`, no push, then `deploy/smoke-test.sh` runs the pair with
`compose.yaml` and checks it over `ssh`) on every push and PR.

## Layering rules (apps/tui)

Module map and per-screen contract: `apps/tui/README.md`. Dependencies point down: `shell → pages → stores → api`.

- **Only `src/ui/` touches `nsh_base` and theme tokens.** Pages compose `ui::*` and layout primitives; they never
  style base controls, write colours, radii or paddings. Missing look → new variant/size in `ui/` first.
- **Pages** are `pages/<name>/{mod.rs,commands.rs}` (login, catalog, detail, cart, addresses, profile): each exports
  a `PageModule { page, mode, commands, view, dialogs }` listed in `pages::PAGES`. A screen worker edits only its
  page's directory.
- **One command table** (`commands.rs`: `Command { id, key, label, scope, typing, busy, footer, enabled, run }`,
  `commands::all()` = global + overlay + every page's `COMMANDS`) drives key bindings (bound once in
  `shell/mod.rs`), buttons, the footer hints and the `?` help. Never bind a key elsewhere.
- **Stores** are `Copy` structs of private signals, all fields of `stores::Stores` (`session`, `router`,
  `catalog`, `product`, `cart`, `addresses`, `orders`, `profile`, `settings`, `toasts`, `ui`; `Stores::provide()`
  in the root, `Stores::get()` in a body); change them through their methods. Flows that cross stores (login +
  merge, checkout, logout, expiry, addresses) are `stores::actions`. A page never writes another page's state.
- **The TUI never computes money.** Prices, discounts, shipping and totals come from the backend as given (not even
  unit × quantity: the detail page reads `PriceCell.totals`).
- Async work starts from events, effects or resources, with explicit loading / empty / error states.
- Component bodies run once; only bindings (closures) re-run. Read signals inside closures, not in the body.
- A new connection opens on the **login page**; `g` browses as a guest; a remembered key resumes straight to the
  catalog. Checkout is the login gate (it returns to the cart).

## Contract first

1. Change the zod schema / `defineEndpoint` in `packages/contract`, regenerate `openapi.json` and `fixtures/v1/**`
   (commit both). Error codes, statuses and messages live in one table (`ERRORS`).
2. Mirror the DTO in `crates/coffee-api`; its tests round-trip every fixture (an unmapped fixture fails).
3. Implement the handler in `apps/server/src/services/<area>.ts` (the route is one `route()` line: auth, headers and
   input validation come from the contract); add a case to `test/conformance/<area>.ts`.
4. Use it from `apps/tui/src/api` and a store. See `.claude/skills/add-api-endpoint`.

## UI rules

- Every action has a key, listed in the footer and in `?` help. Tab/arrows move focus; Enter activates; Esc goes back.
- State is glyph + attribute (`▶ ✓ ✗ ●/○ [✓]/[ ]`, bold, reverse), **never colour alone**; check 16 colours and
  `COFFEE_COLORS=mono`.
- The demo notice is always visible (top bar or status bar, login, receipts); login/register say “请勿使用真实密码”.
- Copy is Simplified Chinese, short, “你”; panel titles are “中文 ENGLISH” (`购物车 CART`); keys lead labels (`a 加入购物车`).
- Breakpoints (cells): <80×24 “终端太小” page; 80–99 one column; 100–129 list + preview; 130–159 three narrow columns;
  ≥160 the design layout (max 200 wide); <32 rows compact header. Measure CJK with `unicode-width` (2 cells).
- **Colour carriers**: nsh draws ANSI meanings, not values, so each token travels on a carrier (an ANSI channel,
  maybe _marked_ italic; fills on xterm-cube `carrier-*` colours) that `coffee-host` pins back to the token at every
  depth (`INKS` / `FILLS` in `ui/theme.rs`; tokens.md “终端载体”). Never write raw colours; colour text only via
  `ui::label` / `strong` / `toned` or a `Tone`/`Fill` component (a bare `Tone::color()` loses its mark).
- **`When::Always` keys**: a stroke that any command takes while typing (`Typing::Only` / `Also`) is bound
  `When::Always` and then belongs to the app in _every_ text field. Give typing commands only `⏎`, `Esc`, Tab-like
  keys — never a letter, digit or space, or nobody can type it.

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
- One test file per screen, `apps/tui/<Name>.test.tsx` (`App.test.tsx` the shell, `Gallery.test.tsx` the
  components); each must `open('.', { platform: 'linux', width: cols * 8, height: rows * 16 })`: the headless
  default is a phone, whose shortcuts never fire. `nsh test --target tui` cannot resize an open app. vitest must
  never pick them up (`vitest.config.ts`).
- **Fixture mode** (no backend; header `◇ 样例数据`): `COFFEE_API_URL=fixture:<scenario>` or storage
  `coffee.transport` picks `guest` (default), `member`, `empty`, `new`, `problem`, `expired`, `member-error`, `down`
  or `error` (`api::FixtureSet`); storage `coffee.fingerprint` / `COFFEE_SSH_FINGERPRINT` gives a key. Answers are
  static: a change answers its fixture and is not remembered.
- **Gallery**: every `ui/` component on 14 pages: `COFFEE_UI_GALLERY=1 target/debug/coffee-tui` (pinned colours,
  any `COFFEE_COLORS`), tested by `apps/tui/Gallery.test.tsx`.
- Pressable chrome is `view().role(Button).focusable(false)`; key bindings need `KeyOptions { repeat: true }`.
- A bordered view clips its children: the Panel legend is an absolutely positioned sibling.
- nsh toasts and Ctrl+S search own the bottom row: we draw our own Toast and status bar.
- The host intercepts Ctrl+C: press it **twice** to quit. Never bind Ctrl+C in the app.
- crossterm needs its `use-dev-tty` feature in `coffee-host`: the default reader spins at 100 % CPU after a hang-up.

## Other pitfalls

- `expect` mangles multi-byte output: drive TUIs and SSH sessions with `scripts/pty-run.py`.
- Colour depth over SSH (`crates/coffee-host/src/color.rs`): `COLORTERM` (sent by `pnpm ssh`), else a 24-bit brand
  in `TERM` (`xterm-kitty`, `xterm-ghostty`, `foot`, `*-direct`, …), else a `*-256color` terminal answering XTGETTCAP
  `RGB`/`Tc` at startup gets truecolor. Servers may force `COFFEE_COLORS=truecolor|256|16|mono`; clients cannot
  (sshd takes only `LANG`, `LC_*`, `COLORTERM`).
- Startup keys (`coffee-host/src/terminal.rs`): keys typed during the terminal queries are split from the answers,
  and all input waits until nsh applied the app's first batches (its key bindings); nothing else may read stdin.
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

## Working here

- Work is split into slices **delegated to subagents** with disjoint file ownership; an orchestrator merges and
  re-runs the verification. Stay inside your brief's paths; list changes elsewhere as change requests. Skills
  (`.claude/skills/`): `nsh-tui-conventions`, `add-ui-component`, `add-screen`, `add-api-endpoint`,
  `run-and-verify-over-ssh`, `update-snapshots`.

## Verification before claiming done

1. `pnpm check` and `pnpm test` (Rust, SSH e2e, vitest); for `apps/tui`, `pnpm check:tui` and `pnpm test:tui`.
2. UI changes: `scripts/pty-run.py` at 160×50, 100×30, 80×24 and 60×20, plus `COFFEE_COLORS=16` and `mono`.
3. SSH or host changes: `pnpm dev`, then `pnpm ssh` through `pty-run.py` with and without a key
   (`-o PubkeyAuthentication=no -o IdentitiesOnly=yes`); resize; Ctrl+C twice; kill the client.
4. Stop everything you started and show `pgrep -fl "dev.mjs|coffee-sshd|coffee-tui|src/main.ts"` is empty.
5. Cross-cutting work: `pnpm verify`. Paste the tails; say what you could not verify. "Compiles" is not done.
