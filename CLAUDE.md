# ROAST/TTY 终端咖啡馆 — notes for coding agents

A **demo** coffee-bean shop you reach with `ssh -p 2222 localhost`: no SSH password, no payment, nothing ships. A
Rust TUI built with [nsh](https://nsh.dev) 0.7.1 (Rust mode) runs one process per SSH session and talks to a local
Node backend. Accounts are in-app (user name + password); an SSH key only “remembers” a login. The UI must say it is
a demo at all times.

```
ssh client ─ tty-gateway (russh: any key or none; SSH user name → route; PTY, limits; deploy/gateway.toml)
               └─ one coffee-tui child per session (coffee-tui on tty-host: nsh AppView + crossterm, real PTY)
                      └─ HTTP 127.0.0.1 + X-Internal-Key ─ @coffee/server (Koa + Prisma + SQLite)
```

Read before changing a layer: `docs/adr/0001-ssh-session-model.md` (sessions, auth, Ctrl+C, colour depth, limits),
`docs/adr/0002-phase-2-architecture.md` (contract first, money on the server, accounts + key remember, colour
carriers, fixture mode, tests), `docs/adr/0003-app-agnostic-gateway.md` (routes by SSH user name, the gateway's
config file and templates, reload, graceful shutdown, the host split), `docs/design/` (screens `00-splash`…`06-profile`, keymap, copy, states, tokens; `decisions.md` = product
rules U1–U7 and API rules P-…), `packages/contract/README.md` (the API), `docs/acceptance.md` (the phase 2 checks).

## Map

| Path                         | What                                                                                                                           | Notes                                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `apps/tui`                   | `coffee-app`: the nsh UI, **library only**                                                                                     | `nsh.json`, `theme.json`, `src/{ui,shell,pages,stores,api,commands.rs,router.rs}`                         |
| `crates/coffee-tui`          | bin `coffee-tui`: the shop's thin entry point on `tty-host`                                                                    | session env, gallery switch, colour pin (`palette.rs`, `tests/pin_render.rs`), SSH tests (`tests/ssh.rs`) |
| `crates/tty-host`            | lib: generic nsh terminal host, `tty_host::run(HostOptions)`                                                                   | colour depth, `FrameFilter`, Ctrl+C twice, startup queries, panics; no shop knowledge                     |
| `crates/tty-gateway`         | bin `tty-gateway --config <file> [--check]`: SSH gateway                                                                       | SSH user name → route → one PTY child per session; no shop knowledge; tests with stand-in programs        |
| `crates/tty-gateway-testkit` | dev-only: in-process `TestServer`, russh client, `vt100` screen                                                                | shared by the gateway's and the shop's SSH tests                                                          |
| `crates/coffee-api`          | serde DTOs of the backend API (no nsh)                                                                                         | round-trips every contract fixture                                                                        |
| `apps/server`                | `@coffee/server`: Koa + Prisma + SQLite, 29 `/v1` endpoints                                                                    | `routes/<area>.ts` + `services/<area>.ts`; DB `apps/server/.data/coffee.db`                               |
| `packages/contract`          | `@coffee/contract`: zod → `openapi.json` + `fixtures/v1/*.json`                                                                | single source of the API; `FooSchema` / type `Foo`; money in integer yuan                                 |
| `scripts/`                   | `dev.mjs`, `pty-run.py`, `e2e/`, `check-mocks.py`                                                                              | `e2e/`: real-stack flows over stock `ssh` (`pnpm test:e2e:real`)                                          |
| `deploy/`                    | `gateway.toml` (the shop's routes: `coffee` = default, `gallery`), `Dockerfile` (gateway), `server.Dockerfile`, `compose.yaml` | `smoke-test.sh`, `server/` (entry point); `README.md` lists every config key and `COFFEE_*`               |
| `docs/`                      | `adr/`, `design/` (`raw/` read-only), `acceptance.md`                                                                          |                                                                                                           |

Frozen interface (change only with every caller): `coffee_app::{App, registry, SessionEnv, configure, session_env,
APP_ID, THEME_JSON}`; root script names `dev build test lint check ssh`.

## Commands (repo root)

| Command                                                               | Does                                                                                                                                                                                          |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm install`                                                        | Node deps (pnpm 12 from `packageManager`); installs the pre-commit hook                                                                                                                       |
| `pnpm dev` / `pnpm ssh`                                               | build both binaries, migrate + seed, `tty-gateway --check`, run backend + gateway on `deploy/gateway.toml` / connect (`COLORTERM`, no host-key checks); `gallery@localhost` is the UI gallery |
| `pnpm check`                                                          | eslint, mock sizes, `tsc`, `cargo fmt --check`, `cargo clippy -D warnings`                                                                                                                    |
| `pnpm test`                                                           | `cargo test --workspace` (incl. the gateway's and the shop's SSH tests), vitest                                                                                                               |
| `pnpm check:tui` / `pnpm test:tui`                                    | `nsh check` / `nsh test` + `nsh test --target tui` in `apps/tui`                                                                                                                              |
| `pnpm test:e2e`                                                       | `cargo test -p tty-gateway -p coffee-tui --tests` (russh client; stub backend for the shop)                                                                                                   |
| `pnpm test:e2e:real`                                                  | `scripts/e2e`: 5 flows, real backend + `tty-gateway` + stock `ssh`, ~2 min; screens and `gateway.log` in `.data/e2e/`                                                                         |
| `target/debug/tty-gateway --check --config deploy/gateway.toml`       | check a gateway config and print its routes (env names only); exit 1 names the bad key                                                                                                        |
| `pnpm verify`                                                         | `check`, `check:tui`, `build`, `test`, `test:tui`                                                                                                                                             |
| `python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui` | a screen as text (`--env COFFEE_API_URL=fixture:…`, `COFFEE_UI_GALLERY=1`)                                                                                                                    |
| `pnpm --filter @coffee/contract generate` / `check:generated`         | regenerate / check `openapi.json` and fixtures                                                                                                                                                |
| `pnpm --filter @coffee/server db:migrate` / `db:seed`                 | backend database (`pnpm dev` runs both); `db:reset` is for humans only                                                                                                                        |

CI (`.github/workflows/ci.yml`, ubuntu-24.04, every push and PR): `node`, `rust`, `e2e` (`scripts/e2e/run.py`,
screens uploaded on failure), `nsh`, `docker` (both images, then `deploy/smoke-test.sh`: the compose pair over `ssh`,
including a purchase).

## Layering rules (apps/tui)

Module map and per-screen contract: `apps/tui/README.md`. Dependencies point down: `shell → pages → stores → api`.

- **Only `src/ui/` touches `nsh_base` and theme tokens.** Pages compose `ui::*`; they never style base controls or
  write colours, radii, paddings. A missing look is a new variant in `ui/` first.
- **Pages** are `pages/<name>/` (splash, login, catalog, detail, cart, addresses, profile), each a `PageModule { page, mode,
commands, view, dialogs }` in `pages::PAGES`. A screen worker edits only its page's directory.
- **One command table** (`commands.rs`: `Command { id, key, label, scope, typing, busy, footer, enabled, run }`)
  drives key bindings (bound once in `shell/mod.rs`), buttons, status-bar hints and `?` help. Never bind keys elsewhere.
- **Stores** are `Copy` structs of private signals in `stores::Stores`, changed through their methods; cross-store
  flows (login + merge, checkout, logout, expiry, addresses) are `stores::actions`. A page never writes another's state.
- **The TUI never computes money**, not even unit × quantity: amounts are the server's (`PriceCell.totals`, `Cart.totals`).
- Async work starts from events, effects or resources, with explicit loading / empty / error states. Component bodies
  run once; read signals inside closures.
- A connection opens on the **splash** (`pages/splash`, ≥900 ms, at most 6 s, `⏎` skips; it never mentions the SSH
  key), then the **login page**; `g` browses as a guest; a remembered key resumes to the catalogue.
  Checkout is the login gate (it returns to the cart).

## Contract first

1. Change the zod schema / `defineEndpoint` in `packages/contract`; `generate`; commit `openapi.json` and
   `fixtures/v1/**`. Error codes, statuses and messages live in one table (`ERRORS`).
2. Mirror the DTO in `crates/coffee-api`; its tests round-trip every fixture (an unmapped fixture fails).
3. Implement `apps/server/src/services/<area>.ts` (the route is one `route()` line; validation comes from the
   contract); add a case to `test/conformance/<area>.ts`.
4. Use it from `apps/tui/src/api`, a store, and the fixture transport. Step by step: `.claude/skills/add-api-endpoint`.

## UI rules

- Every action has a key, shown in the status bar and in `?` help. Tab/arrows move focus; ⏎ activates; Esc goes back.
- State is glyph + attribute (`▶ ✓ ✗ ●/○ [✓]/[ ]`, bold, reverse), **never colour alone**; check `COFFEE_COLORS=16`/`mono`.
- The demo notice is always visible (system row, login card, receipts); login/register say “请勿使用真实密码”.
- Copy is Simplified Chinese, short, “你”; panel titles “中文 ENGLISH”; keys lead labels (`a 加入购物车`).
- Breakpoints (cells): <80×24 “终端太小”; 80–99 one column; 100–129 list + preview; 130–159 three columns; ≥160 the
  design (max 200 wide); <32 rows compact header. Measure CJK with `unicode-width` (2 cells).
- **Colour carriers**: nsh draws ANSI meanings, not values; each token rides a carrier (`INKS` / `FILLS` in
  `ui/theme.rs`) that `coffee-tui`'s pin (`palette.rs`, a `tty_host::FrameFilter`) puts back at every depth. Colour text only via `ui::label` / `strong` / `toned`
  or a `Tone`/`Fill` component.
- **`When::Always` keys**: a stroke any typing command takes belongs to the app in every text field. Give typing
  commands only ⏎, Esc and Tab-like keys, never a letter, digit or space.

## Pinned versions

| Tool                                   | Version                                      | Why                                                     |
| -------------------------------------- | -------------------------------------------- | ------------------------------------------------------- |
| nsh CLI + crates                       | 0.7.1 (`=0.7.1` in `Cargo.toml`)             | API moves fast; CLI and crates must match               |
| Rust                                   | 1.99.1 (`rust-toolchain.toml`), edition 2024 |                                                         |
| Node / pnpm                            | 26 / 12.9.1                                  | Node 26 runs the TS sources directly (type stripping)   |
| TypeScript                             | 6.0.3                                        | TS 7 breaks typescript-eslint 8                         |
| prisma, @prisma/client, SQLite adapter | 7.9.1                                        | 7.10.x is refused by pnpm's `trustPolicy: no-downgrade` |

## Pitfalls

- `apps/tui` is lib-only; `.nsh/` is generated and git-ignored. nsh builds into `apps/tui/target` (≈8 GB): keep it.
- One test file per screen, `apps/tui/<Name>.test.tsx`, sharing `testkit.ts`; each opens the app at a desktop size
  (`platform: 'linux'`, `width: cols * 8`, `height: rows * 16`): the headless default is a phone. vitest never runs them.
- **Fixture mode** (`◇ 样例数据`): `COFFEE_API_URL=fixture:<scenario>` (`guest`, `member`, `empty`, `new`, `problem`,
  `expired`, `member-error`, `down`, `error`, `slow`); `COFFEE_SSH_FINGERPRINT` gives a key. Answers are static.
  **Gallery**: `COFFEE_UI_GALLERY=1 target/debug/coffee-tui`.
- The host intercepts Ctrl+C: press it **twice** to quit; never bind it in the app. nsh toasts and Ctrl+S own the
  bottom row, so we draw our own Toast and status bar.
- crossterm needs `use-dev-tty` in `tty-host` (the default reader spins at 100 % CPU after a hang-up).
- Keys reach the app only after its first batches; Esc and a letter sent together read as Alt+letter: drivers wait
  for the screen between them.
- `expect` mangles multi-byte output: drive TUIs and SSH sessions with `scripts/pty-run.py` or `scripts/e2e`.
- Colour depth over SSH: `COLORTERM` (sent by `pnpm ssh`), a truecolor brand in `TERM`, or an XTGETTCAP answer;
  servers may force `COFFEE_COLORS` (the routes pass `${env:COFFEE_COLORS}`); clients cannot (a route's `accept_env` is
  `LANG`, `LC_*`, `COLORTERM`, and a client can never set one of the route's own env names).
- `prisma migrate reset` refuses AI agents: `db:reset` is for humans; never work around it. Use a scratch database
  (`DATABASE_URL=file:/tmp/x.db`). The backend refuses to start on a missing, unmigrated or locked database.
- The gateway allows 30 new connections per address per minute (`limits.connections_per_minute` in
  `deploy/gateway.toml`): scripts that connect in a loop run a copy with `connections_per_minute = 0` (the e2e driver
  does); never change limits in the committed file for a test.
- Gateway tuning lives in the config file: the gateway itself reads only `TTY_GATEWAY_CONFIG` / `_ADDR` / `_HOST_KEY` /
  `_LOG`. Secrets never go in the file: name them as `${env:NAME}` in the one route that needs them (an entry with an
  absent variable is omitted). `COFFEE_SSH_FINGERPRINT`, `COFFEE_CLIENT_IP`, … are the app's inputs, set by the route.
- Reload with SIGHUP to the gateway's own pid (`pkill -HUP -x tty-gateway`; a SIGHUP to `dev.mjs` stops everything):
  new connections only, a bad file keeps the running one, `listen` / `host_key` need a restart. SIGTERM drains within
  `shutdown_grace` + 1 s; a second SIGTERM/SIGINT exits at once and may orphan session programs.
- `crates/tty-gateway` and `crates/tty-host` know nothing about the shop (no `COFFEE_*`, no shop text): shop behaviour
  belongs in `deploy/gateway.toml` or `crates/coffee-tui`.
- pnpm 12 takes one script per `pnpm --filter <pkg> <script>`. The repo path may contain non-ASCII characters: quote it.
- Design mocks (`<!-- mock WxH -->`) are checked by `scripts/check-mocks.py`; keep their `<!-- prettier-ignore -->`.

## Style

- **2-space indentation everywhere, Rust and Python included** (`rustfmt.toml` `tab_spaces = 2` is the owner's
  choice). ESLint formats TS/JSON/YAML/Markdown (`pnpm lint:fix`).
- The pre-commit hook (simple-git-hooks + lint-staged) runs `eslint --fix` and `rustfmt`; keep it enabled.
- Conventional commits in English. No secrets or personal data in the repo, logs, fixtures or screenshots.

## How work is delegated and verified

- An orchestrator splits work into slices for parallel workers (subagents), each in its own worktree with
  **disjoint paths**. Stay inside your brief; files shared by several slices (root config, this file, lockfiles) change
  through the orchestrator: list what you need as change requests. Skills (`.claude/skills/`): `nsh-tui-conventions`,
  `add-ui-component`, `add-screen`, `add-api-endpoint`, `run-and-verify-over-ssh`, `update-snapshots`.
- The orchestrator merges, re-runs the checks below and pushes; workers push only their own branch.
- **Every background wait has a timeout** (no bare `tail -f` or unbounded `until`); stop what you started.

## Verification before claiming done

1. `pnpm check` and `pnpm test`; for `apps/tui`, `pnpm check:tui` and `pnpm test:tui`.
2. UI changes: `scripts/pty-run.py` at 160×50, 100×30, 80×24 and 60×20, plus `COFFEE_COLORS=16` and `mono`.
3. Anything a shopper can reach (UI, API, SSH, host): `pnpm test:e2e:real` (all flows green; read `FAIL.txt` and
   the screens in `.data/e2e/` on a failure). Flaky is failing: find the cause.
4. SSH, gateway or host changes: also by hand with and without a key (`-o PubkeyAuthentication=no -o IdentitiesOnly=yes`);
   `coffee@`, `gallery@` and another user name; resize; Ctrl+C twice; kill the client; `ssh -tt … </dev/null` ends at once;
   a config change plus `pkill -HUP -x tty-gateway`; `pkill -TERM -x tty-gateway` with a session open.
5. Stop everything you started: `pgrep -fl "dev.mjs|tty-gateway|coffee-tui|src/main.ts"` is empty and your ports
   are free (`lsof -nP -iTCP:<port> -sTCP:LISTEN`).
6. Cross-cutting work: `pnpm verify`. Paste the tails; say what you could not verify. "Compiles" is not done.
