# ROAST/TTY 终端咖啡馆

A demo coffee-bean shop you reach over SSH: `ssh -p 2222 localhost`, no password. A Rust TUI built with
[nsh](https://nsh.dev) (Rust mode) runs one process per session behind a Rust SSH front end and talks to a local
Node API (Koa + Prisma + SQLite).

> **Demo only.** This is not a real shop: nothing is sold, charged or shipped, and orders complete without payment.
> Do not use a real password - accounts here are throwaway.

```
ssh client ─ coffee-sshd (russh, PTY) ─ coffee-tui per session (nsh) ─ HTTP 127.0.0.1 ─ @coffee/server (Koa + Prisma + SQLite)
```

## Prerequisites

- **Rust** via [rustup](https://rustup.rs); `rust-toolchain.toml` pins 1.99.1 and rustup installs it on first use.
- **Node.js 26** and **pnpm 12.9.1** (`corepack enable`, or a standalone pnpm).
- **nsh CLI 0.6.1** with the `tui` component (only for `pnpm check:tui` / `pnpm test:tui`):
  - macOS: `brew install longbridge/tap/nsh`
  - Linux/macOS: the installer from [nsh.dev](https://nsh.dev/docs/installation.md) with `NSH_VERSION=v0.6.1`
  - then `nsh component add tui`
- Python 3 (standard library only) for `scripts/pty-run.py`.

## Quick start

```sh
pnpm install
pnpm dev        # builds coffee-tui + coffee-sshd, starts the backend and the SSH server
pnpm ssh        # in another terminal: ssh -p 2222 localhost, no host-key prompts
```

`pnpm dev` migrates and seeds the database, waits for the backend's `/v1/health`, then prints
`ssh -p 2222 localhost`; Ctrl+C stops everything. Both processes share a random internal key made for that run.
Runtime data goes to `.data/` (the SSH host key) and `apps/server/.data/coffee.db` (SQLite), both git-ignored.

Any SSH key works, and so does none: there is no password prompt. Inside, press **Ctrl+C twice** to leave.

### Colours

The shop is drawn in 24-bit colour when the session knows the terminal can show it. OpenSSH does not send
`COLORTERM` by default, so a plain `ssh -p 2222 localhost` gets 256 colours (close, not exact). For exact colours:

- `pnpm ssh` (or `ssh -o SendEnv=COLORTERM -p 2222 localhost`) passes your terminal's `COLORTERM=truecolor` along;
- or the server forces a depth for everyone: `COFFEE_COLORS=truecolor pnpm dev` (also `256`, `16`, `mono`).
  Only force truecolor when you know the clients support it.

## Scripts

Run from the repository root with `pnpm <script>`:

| Script              | Does                                                                                                                 |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `dev`               | `scripts/dev.mjs`: build `coffee-tui`/`coffee-sshd`, run backend + sshd with prefixed logs (`--skip-build` to reuse) |
| `ssh`               | `ssh -p 2222 localhost` without host-key checks, forwarding `COLORTERM`                                              |
| `build`             | `cargo build --workspace`, then each package's `build`                                                               |
| `test`              | build `coffee-tui`, `cargo test --workspace` (SSH end-to-end tests included), vitest                                 |
| `test:tui`          | `nsh test` and `nsh test --target tui` in `apps/tui`                                                                 |
| `test:e2e`          | only the SSH end-to-end tests in `crates/coffee-sshd/tests`                                                          |
| `lint` / `lint:fix` | ESLint, which also formats (no Prettier)                                                                             |
| `typecheck`         | `tsc` in every package                                                                                               |
| `check`             | lint, `check:mocks`, typecheck, `cargo fmt --check`, `cargo clippy -D warnings`                                      |
| `check:mocks`       | `scripts/check-mocks.py`: every `<!-- mock WxH -->` in `docs/design` is exactly W×H cells (CJK = 2)                  |
| `check:tui`         | `nsh check` in `apps/tui`                                                                                            |
| `verify`            | everything CI runs but the Docker build: `check`, `check:tui`, `build`, `test`, `test:tui`                           |

Other useful commands:

```sh
cargo run -p coffee-host --bin coffee-tui                              # the UI alone, in this terminal
python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui     # its screen as text, no terminal needed
pnpm --filter @coffee/server db:reset     # recreate and seed the database (run it yourself: Prisma refuses AI agents)
docker build -f deploy/Dockerfile -t roast-tty-ssh .   # the SSH gateway image (see deploy/README.md)
```

## Layout

| Path                 | What                                                                                                 |
| -------------------- | ---------------------------------------------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI (library only; `nsh.json`, `theme.json`)                                    |
| `crates/coffee-host` | `coffee-tui`: runs the UI in the current terminal                                                    |
| `crates/coffee-sshd` | the SSH front end (one `coffee-tui` per session on a PTY) and its end-to-end tests                   |
| `crates/coffee-api`  | serde DTOs of the backend API                                                                        |
| `apps/server`        | `@coffee/server`: the backend                                                                        |
| `packages/contract`  | `@coffee/contract`: the API contract (zod → `openapi.json` + fixtures)                               |
| `docs/design`        | screens, keymap, copy, responsive rules, tokens; `raw/` holds the exported designs (read-only)       |
| `docs/adr`           | architecture decisions ([0001: one PTY process per SSH session](docs/adr/0001-ssh-session-model.md)) |
| `deploy`             | the SSH gateway image and its settings                                                               |
| `scripts`            | `dev.mjs`, `pty-run.py`, `check-mocks.py`                                                            |
| `.github/workflows`  | CI: `node`, `rust`, `nsh` and `docker` jobs                                                          |
| `.claude/skills`     | step-by-step guides for coding agents                                                                |

## More

- [`CLAUDE.md`](CLAUDE.md): architecture, layering rules, pinned versions and pitfalls (for people and agents).
- [`docs/design/`](docs/design/README.md): screens, keymap, copy, states, responsive rules and tokens.
- [`docs/adr/0001-ssh-session-model.md`](docs/adr/0001-ssh-session-model.md): the SSH session model, auth, Ctrl+C,
  colour pinning, limits and measurements.
- [`deploy/README.md`](deploy/README.md): the gateway image and every `COFFEE_*` setting.

## Editor and hooks

Indentation is two spaces everywhere, Rust included (`rustfmt.toml`, `.editorconfig`). ESLint is the formatter.
`.vscode/settings.json` is committed: VS Code and its forks pick it up (fix on save, Prettier off); install the
ESLint extension. `pnpm install` also installs a pre-commit hook (simple-git-hooks + lint-staged) that runs
`eslint --fix` and `rustfmt` on staged files; `SKIP_SIMPLE_GIT_HOOKS=1 git commit …` skips it.
