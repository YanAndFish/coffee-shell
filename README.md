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

`pnpm dev` waits for the backend's `/v1/health`, then prints `ssh -p 2222 localhost`; Ctrl+C stops everything. Both
processes share a random internal key made for that run. Runtime data (SQLite, the SSH host key) goes to `.data/`.

## Scripts

Run from the repository root with `pnpm <script>`:

| Script              | Does                                                                                                                 |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `dev`               | `scripts/dev.mjs`: build `coffee-tui`/`coffee-sshd`, run backend + sshd with prefixed logs (`--skip-build` to reuse) |
| `ssh`               | `ssh -p 2222 localhost` without host-key checks                                                                      |
| `build`             | `cargo build --workspace`, then each package's `build`                                                               |
| `test`              | build `coffee-tui`, `cargo test --workspace` (SSH end-to-end tests included), vitest                                 |
| `test:tui`          | `nsh test` and `nsh test --target tui` in `apps/tui`                                                                 |
| `test:e2e`          | only the SSH end-to-end tests in `crates/coffee-sshd/tests`                                                          |
| `lint` / `lint:fix` | ESLint, which also formats (no Prettier)                                                                             |
| `typecheck`         | `tsc` in every package                                                                                               |
| `check`             | lint, typecheck, `cargo fmt --check`, `cargo clippy -D warnings`                                                     |
| `check:tui`         | `nsh check` in `apps/tui`                                                                                            |
| `verify`            | everything CI runs: `check`, `check:tui`, `build`, `test`, `test:tui`                                                |

Other useful commands:

```sh
cargo run -p coffee-host --bin coffee-tui                              # the UI alone, in this terminal
python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui     # its screen as text, no terminal needed
pnpm --filter @coffee/server db:reset                                   # recreate and seed the database
```

## Layout

| Path                 | What                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI (library only; `nsh.json`, `theme.json`)                              |
| `crates/coffee-host` | `coffee-tui`: runs the UI in the current terminal                                              |
| `crates/coffee-sshd` | the SSH front end (one `coffee-tui` per session on a PTY) and its end-to-end tests             |
| `crates/coffee-api`  | serde DTOs of the backend API                                                                  |
| `apps/server`        | `@coffee/server`: the backend                                                                  |
| `packages/contract`  | `@coffee/contract`: the API contract (zod → `openapi.json` + fixtures)                         |
| `docs/design`        | screens, keymap, copy, responsive rules, tokens; `raw/` holds the exported designs (read-only) |
| `scripts`            | `dev.mjs`, `pty-run.py`                                                                        |
| `.github/workflows`  | CI: `node`, `rust` and `nsh` jobs                                                              |
| `.claude/skills`     | step-by-step guides for coding agents                                                          |

## More

- [`CLAUDE.md`](CLAUDE.md): architecture, layering rules, pinned versions and pitfalls (for people and agents).
- [`docs/`](docs/): design specs and decisions.

Indentation is two spaces everywhere, Rust included (`rustfmt.toml`, `.editorconfig`).
