# ROAST/TTY 终端咖啡馆

A coffee-bean shop you reach over SSH: `ssh -p 2222 localhost`, no SSH password. Inside, a full-screen terminal UI
lets you browse eight beans, choose weight, grind and pack, fill a cart, keep an address book, check out and see
your orders. It is a Rust TUI built with [nsh](https://nsh.dev) 0.7.1 (Rust mode), one process per SSH session behind
an application-agnostic Rust SSH gateway (`tty-gateway`), talking to a Node backend (Koa + Prisma + SQLite).

> **Demo only.** This is not a real shop: nothing is sold, charged or shipped, and orders complete without payment.
> The UI says so on every screen. Accounts are throwaway: never use a real password, address or phone number.

## Quick start (about 2 minutes)

Needs Rust ([rustup](https://rustup.rs); `rust-toolchain.toml` pins 1.99.1), Node.js 26 with pnpm 12.9.1
(`corepack enable`), Python 3 and OpenSSH. The first build takes a few minutes.

```sh
pnpm install    # Node dependencies and the pre-commit hook
pnpm dev        # builds coffee-tui + tty-gateway, migrates + seeds the database, starts backend and gateway
pnpm ssh        # in a second terminal: ssh -p 2222 localhost (forwards COLORTERM, skips host-key checks)
```

`pnpm dev` prints `✓ ready — connect with: ssh -p 2222 localhost` once both are up; Ctrl+C stops everything. Any SSH
key works and so does none; inside, press **Ctrl+C twice** to leave. The database is `apps/server/.data/coffee.db`
and the gateway's host key `.data/ssh_host_ed25519_key` (both git-ignored); `DATABASE_URL=file:/elsewhere.db pnpm dev`
uses another database. `pnpm dev` checks the gateway's configuration (`tty-gateway --check`) before it starts it;
`TTY_GATEWAY_CONFIG=my-gateway.toml pnpm dev` runs another file.

The gateway routes by SSH **user name** (`deploy/gateway.toml`):

```sh
ssh -p 2222 localhost              # your local user name: not a route, so the default route, the shop
ssh -p 2222 coffee@localhost       # the shop (also shop@)
ssh -p 2222 anything@localhost     # any other name: the default route, the shop
ssh -p 2222 gallery@localhost      # the UI gallery: every component, ← → to turn pages
```

## In the shop

A new connection opens on the login page. Register (a user name and a password; no e-mail), log in, or press `Esc`
then `g` to browse as a guest. Guests can fill a cart; checking out asks them to log in and merges their cart into
the account. With an SSH key, **记住我** (remember me, ticked by default) binds the key to the account for 30 days:
the next connection with that key signs in by itself. Logging out (`q`, then confirm) forgets the key.

| Where        | Keys                                                                                                            |
| ------------ | --------------------------------------------------------------------------------------------------------------- |
| Everywhere   | `1` 选购 shop · `2` 购物车 cart · `3` 地址本 addresses · `4` 我的 profile · `/` search · `?` help · `q` log out |
|              | `Tab` / `Shift+Tab` focus · `⏎` activate · `Esc` back or leave a field · Ctrl+C twice: disconnect               |
| Login        | `Tab` cycles 用户名 → 密码 → 记住我 → 登录 → 访客浏览 → 注册; `⏎` submits; `g` guest (outside a field)          |
| Catalogue    | `↑↓` / `j k` move · `⏎` / `l` details · `a` quick add · `f` origin · `r` roast · `s` sort · `Esc` clear         |
| Product      | `↑↓` row (weight, grind, pack, quantity) · `←→` choose · `+` `-` quantity · `a` add · `b` buy now · `Esc`       |
| Cart         | `↑↓` move · `space` tick · `←→` quantity · `x` remove · `u` undo · `a` all · `Tab` coupon · `c` check out       |
| Address book | arrows move · `⏎` use for this order · `n` new · `e` edit · `space` default · `d` delete                        |
| Profile      | `m` animations · `o` mouse tracking · `q` log out                                                               |

The coupon `TTY10` takes 10 % off; shipping is ¥12, free from ¥199 after the discount. Every amount comes from the
backend. Below 80×24 the shop shows `终端太小` and comes back when the window grows. The mouse works too (click,
double-click, hover, and the key hints in the status bar). The full keymap: [`docs/design/keymap.md`](docs/design/keymap.md).

## Without a backend: fixture mode and the gallery

```sh
cargo build -p coffee-tui
COFFEE_API_URL=fixture: target/debug/coffee-tui            # the UI on the contract's sample data (◇ 样例数据)
COFFEE_API_URL=fixture:member COFFEE_SSH_FINGERPRINT=SHA256:demo target/debug/coffee-tui   # a remembered key
COFFEE_UI_GALLERY=1 target/debug/coffee-tui                # every UI component, ← → to turn pages
```

Over SSH without a backend: `COFFEE_API_URL=fixture: target/debug/tty-gateway --config deploy/gateway.toml` (after
`cargo build -p coffee-tui -p tty-gateway`) serves every shop session from the fixtures.

Scenarios: `fixture:` (a new visitor; any password but `wrongpass` logs in as 林晓), `member`, `empty`, `new`,
`problem`, `expired`, `member-error`, `slow`, `down`, `error` (see [`apps/tui/README.md`](apps/tui/README.md)). Answers
are static samples: what you add, delete or save is not remembered.

## How it fits together

```
ssh client ──▶ tty-gateway (russh: any key or none; user name → route; limits)   crates/tty-gateway + deploy/gateway.toml
                 └─ one coffee-tui per session, on its own PTY   crates/coffee-tui (on crates/tty-host) + apps/tui (nsh)
                      └─ HTTP 127.0.0.1 + X-Internal-Key ──▶ @coffee/server (Koa + Prisma + SQLite)   apps/server
                                                              ▲
                     packages/contract: zod → openapi.json + fixtures, used by the server, crates/coffee-api and the TUI
```

The two infrastructure crates know nothing about the shop: `tty-gateway` runs whatever a route names, and
`tty-host` runs any nsh app in a terminal. The shop is one configuration (`deploy/gateway.toml`) and one thin binary
(`coffee-tui`).

- [ADR 0001](docs/adr/0001-ssh-session-model.md): one process per SSH session, authentication, Ctrl+C, colour
  depth and pinning, limits, measurements.
- [ADR 0002](docs/adr/0002-phase-2-architecture.md): contract first, money on the server, accounts with SSH-key
  “remember me”, colour carriers, fixture mode, tests per screen, the end-to-end driver.
- [ADR 0003](docs/adr/0003-app-agnostic-gateway.md): the application-agnostic gateway (routes, the config file,
  reload, graceful shutdown, robustness) and the split of the terminal host.
- [`docs/design/`](docs/design/README.md): screens, keymap, copy, states, responsive rules, tokens and the product
  decisions; [`packages/contract/README.md`](packages/contract/README.md): the API.

| Path                         | What                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ |
| `apps/tui`                   | `coffee-app`: the nsh UI (library only; `nsh.json`, `theme.json`)                                      |
| `crates/coffee-tui`          | bin `coffee-tui`: the shop's entry point on `tty-host` (session env, gallery switch, colour pin)       |
| `crates/tty-host`            | the generic nsh terminal host: terminal modes, colour depth, Ctrl+C twice, OSC 52, the run loop        |
| `crates/tty-gateway`         | bin `tty-gateway`: the SSH gateway (user name → route → one program per session on a PTY)              |
| `crates/tty-gateway-testkit` | test-only: an in-process gateway, a russh client with a `vt100` screen, stand-in programs              |
| `crates/coffee-api`          | serde DTOs of the backend API, tested against every contract fixture                                   |
| `apps/server`                | `@coffee/server`: the backend (29 `/v1` endpoints)                                                     |
| `packages/contract`          | `@coffee/contract`: the API contract                                                                   |
| `scripts`                    | `dev.mjs` (`pnpm dev`), `pty-run.py` (a screen as text), `e2e/` (flows), `check-mocks.py`              |
| `deploy`                     | `gateway.toml` (the shop's gateway config), the gateway and backend images, `compose.yaml`, smoke test |
| `docs`                       | `adr/`, `design/`, [`acceptance.md`](docs/acceptance.md) (how to re-run the phase 2 checks)            |

## Commands

Run from the repository root with `pnpm <script>`:

| Script              | Does                                                                                                              |
| ------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `dev`               | build `coffee-tui` and `tty-gateway`, migrate + seed, run backend and gateway with prefixed logs (`--skip-build`) |
| `ssh`               | `ssh -p 2222 localhost` without host-key checks, forwarding `COLORTERM`                                           |
| `build`             | `cargo build --workspace`, then each package's `build`                                                            |
| `test`              | `cargo test --workspace` (SSH end-to-end tests included), vitest                                                  |
| `test:tui`          | `nsh test` and `nsh test --target tui` in `apps/tui` (needs the nsh CLI)                                          |
| `test:e2e`          | only the SSH end-to-end tests: `crates/tty-gateway/tests` and the shop's `crates/coffee-tui/tests/ssh.rs`         |
| `test:e2e:real`     | the shop's flows over the real stack and stock `ssh` (`scripts/e2e`, about 2 minutes; see below)                  |
| `lint` / `lint:fix` | ESLint, which also formats (no Prettier)                                                                          |
| `typecheck`         | `tsc` in every package                                                                                            |
| `check`             | lint, `check:mocks`, typecheck, `cargo fmt --check`, `cargo clippy -D warnings`                                   |
| `check:mocks`       | every `<!-- mock WxH -->` in `docs/design` is exactly W×H cells (CJK = 2)                                         |
| `check:tui`         | `nsh check` in `apps/tui`                                                                                         |
| `verify`            | `check`, `check:tui`, `build`, `test`, `test:tui` (everything CI runs but `e2e` and Docker)                       |

`pnpm test:e2e:real` runs each flow on a fresh copy of a migrated database with its own backend and gateway on free
ports, and drives stock `ssh` clients by what their screens show; text screenshots and logs go to `.data/e2e/`.
`python3 scripts/e2e/run.py main` runs one flow, `--repeat 5` the set five times. The checklist behind it:
[`docs/acceptance.md`](docs/acceptance.md).

Other useful commands:

```sh
python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui     # a screen as text, no terminal needed
target/debug/tty-gateway --check --config deploy/gateway.toml           # check the gateway config, list its routes
pnpm --filter @coffee/server db:migrate                                 # apply migrations (pnpm dev does it)
pnpm --filter @coffee/server db:seed                                    # add missing catalogue rows (idempotent)
pnpm --filter @coffee/contract generate                                 # regenerate openapi.json and fixtures
```

The nsh CLI 0.7.1 with its `tui` component is needed only for `check:tui` / `test:tui`: `brew install
longbridge/tap/nsh` (macOS) or the installer from [nsh.dev](https://nsh.dev/docs/installation.md) with
`NSH_VERSION=v0.7.1`, then `nsh component add tui`.

## The gateway: a reverse proxy for terminal programs

`tty-gateway` is to terminal programs what NGINX is to web apps: one SSH port in front of several programs. The SSH
user name picks a **route**; a name no route lists gets `server.default_route`. Each route runs a local command on a
PTY, one per session, with an environment the route spells out. Logging in is the same on every route: any public
key or none, never a password (ADR 0001). A minimal two-route file:

```toml
[server]
listen = "127.0.0.1:2222"
default_route = "shop"

[routes.shop] # users = ["shop"] when omitted
command = [ "coffee-tui" ]

# a bare name: next to tty-gateway first, then PATH
[routes.shop.env]
COFFEE_API_URL = "${env:COFFEE_API_URL:-http://127.0.0.1:3000}"
COFFEE_INTERNAL_KEY = "${env:COFFEE_INTERNAL_KEY}" # no value in the gateway's environment: left out
COFFEE_SSH_FINGERPRINT = "${fingerprint}" # only after a verified key
COFFEE_CLIENT_IP = "${client_ip}"

[routes.top]
users = [
  "top",
  "htop"
]
command = [ "/usr/bin/top" ]
```

- Variables in `env` values: `${user}`, `${route}`, `${fingerprint}`, `${client_ip}`, `${env:NAME}` and
  `${env:NAME:-default}` (the gateway's own environment, read with the file). An entry whose variable has no value
  is left out, so a secret reaches only the routes that name it. Secrets stay in the environment, not in the file.
- `tty-gateway --config FILE --check` reads and checks the file, prints the routes (variable names, never values)
  and exits 1 on an error; a program that is not there yet is only a warning.
- A SIGHUP to the gateway (`pkill -HUP -x tty-gateway`; not to `pnpm dev`, which stops on it) reloads the file: new
  connections get the new routes, limits and messages, sessions already running keep theirs, and a broken file is
  logged and ignored (`reload failed: the running configuration stays`). `listen` and `host_key` need a restart.
- SIGTERM / SIGINT stop gracefully: no new connections, every session is told and hung up, whatever still runs
  after `server.shutdown_grace` (2 s) is killed, and the gateway is gone within grace + 1 s. A second signal during
  that drain exits at once and may leave session programs behind.

The shop's file is [`deploy/gateway.toml`](deploy/gateway.toml); every key, default and variable is in
[`deploy/README.md`](deploy/README.md), the reasons in [ADR 0003](docs/adr/0003-app-agnostic-gateway.md).

## Deploy with Docker Compose

```sh
export COFFEE_INTERNAL_KEY="$(openssl rand -base64 32)"    # or put it in deploy/.env (git-ignored)
docker compose -f deploy/compose.yaml up -d --build
ssh -p 2222 localhost
```

Two images: the gateway (`deploy/Dockerfile`: `tty-gateway`, `coffee-tui` and `gateway.toml`, port 2222) and the backend
(`deploy/server.Dockerfile`), which is not published and sits on an internal network only the gateway reaches. Both
run as an unprivileged user with all capabilities dropped; the backend migrates and seeds on every start (safe to
repeat). `deploy/smoke-test.sh` starts the pair and buys a bag over `ssh`. Settings (the gateway's config file and
how to mount your own, `COFFEE_*`, volumes): [`deploy/README.md`](deploy/README.md).

## Colours over SSH

The shop is drawn in 24-bit colour when the session knows the terminal can show it, else in the nearest of 256
colours; 16 colours and monochrome keep every state readable (glyphs and attributes, not colour alone). OpenSSH does
not send `COLORTERM` by default, so a session learns the depth from:

- `pnpm ssh` (or `ssh -o SendEnv=COLORTERM -p 2222 localhost`), which forwards your `COLORTERM=truecolor`;
- the terminal's `TERM`, which always arrives: kitty, Ghostty, foot, Alacritty, WezTerm, rio and any `*-direct` get
  24-bit colour with a plain `ssh`;
- the terminal itself: a `*-256color` terminal that answers XTGETTCAP `RGB`/`Tc` at startup gets 24-bit colour, one
  that stays silent keeps 256;
- or the server, for everyone: `COFFEE_COLORS=truecolor|256|16|mono pnpm dev` (force truecolor only when you know the
  clients support it). The shop's route hands it to every session (`COFFEE_COLORS = "${env:COFFEE_COLORS}"`).
  Clients cannot set it: the route accepts only `LANG`, `LC_*` and `COLORTERM` from them.

The rule (`NO_COLOR` included) is in [`crates/tty-host/src/color.rs`](crates/tty-host/src/color.rs).

## Troubleshooting

- **`127.0.0.1:3000 is already in use`** (or `:2222`): `pnpm dev` checks both ports before it builds. Stop what holds
  them (`lsof -nP -iTCP:3000 -iTCP:2222 -sTCP:LISTEN`) or move them: `PORT=3917 TTY_GATEWAY_ADDR=127.0.0.1:2917 pnpm dev`,
  then `ssh -p 2917 localhost`.
- **`ssh gallery@localhost` opens the shop**: the running gateway has an older configuration. Check the file
  (`target/debug/tty-gateway --check --config deploy/gateway.toml`), then reload it (`pkill -HUP -x tty-gateway`) or
  restart `pnpm dev`.
- **The backend stops at start with `[server] ✗ the database … does not exist / is not migrated / is locked`**: run
  `pnpm --filter @coffee/server db:migrate` (and `db:seed`), or stop the other process holding the SQLite file (a
  second backend, an open `sqlite3` shell).
- **Starting over with an empty database**: `pnpm --filter @coffee/server db:reset` deletes everything and reseeds.
  It is for people only: Prisma refuses `migrate reset` when an AI agent runs it, and agents must not work around
  that. Alternatively point `DATABASE_URL` at a new file.
- **A host-key warning after rebuilding** the compose pair: the gateway's key lives in the `host-key` volume; keep it
  (`down -v` deletes it). Locally it is `.data/ssh_host_ed25519_key`.
- **`ssh` asks for a passphrase**: your default key is encrypted and not in an agent. Press Enter to skip it (you get
  in without a key) or use `-o PubkeyAuthentication=no`.
- **Prisma prints “unsupported Node”** on Node 26: harmless.

## Contributing

Two-space indentation everywhere, Rust included (`rustfmt.toml`, `.editorconfig`); ESLint formats TS, JSON, YAML
and Markdown. `pnpm install` installs a pre-commit hook (simple-git-hooks + lint-staged: `eslint --fix` and `rustfmt`
on staged files). `.vscode/settings.json` is committed (fix on save; install the ESLint extension). Conventions,
layering rules and the checks to run before calling work done: [`CLAUDE.md`](CLAUDE.md).
