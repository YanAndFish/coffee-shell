# ROAST/TTY 终端咖啡馆

A demo coffee-bean shop you reach over SSH: a Rust TUI built with [nsh](https://nsh.dev) (Rust mode), served one process per session by a Rust SSH front end, backed by a local Node API (Koa + Prisma + SQLite).

> **Demo only.** This is not a real shop: nothing is sold, charged or shipped. Do not use a real password.

## Layout

| Path                 | What                                                           |
| -------------------- | -------------------------------------------------------------- |
| `apps/tui`           | `coffee-app`: the nsh UI (library only; `nsh.json` lives here) |
| `crates/coffee-host` | `coffee-tui`: runs the UI in the current terminal              |
| `crates/coffee-sshd` | the SSH front end (one `coffee-tui` per session on a PTY)      |
| `crates/coffee-api`  | serde DTOs of the backend API                                  |
| `apps/server`        | `@coffee/server`: the backend                                  |
| `packages/contract`  | `@coffee/contract`: the API contract                           |
| `docs/design/raw`    | exported design files (read-only)                              |

## Prerequisites

- Rust 1.99.1 (pinned by `rust-toolchain.toml`; rustup installs it)
- nsh CLI 0.6.1 with the `tui` component: `brew install longbridge/tap/nsh && nsh component add tui`
- Node.js >= 26 and pnpm 12 (`corepack enable` or a standalone pnpm)

## Scripts

Run from the repository root with `pnpm <script>`:

| Script              | Does                                                             |
| ------------------- | ---------------------------------------------------------------- |
| `dev`               | backend + `coffee-sshd` together (placeholder for now)           |
| `build`             | `cargo build --workspace`, then each package's `build`           |
| `test`              | `cargo test --workspace`, then vitest                            |
| `lint` / `lint:fix` | ESLint (it also formats; no Prettier)                            |
| `check`             | lint, typecheck, `cargo fmt --check`, `cargo clippy -D warnings` |
| `ssh`               | `ssh -p 2222 localhost` without host-key checks                  |

The UI alone: `cargo run -p coffee-host --bin coffee-tui`, or `cd apps/tui && nsh run --target tui`. `nsh check` and `nsh test` (add `--target tui` for the terminal host) run in `apps/tui`.

Indentation is two spaces everywhere, Rust included (`rustfmt.toml`).
