---
name: run-and-verify-over-ssh
description: Run the whole ROAST/TTY demo locally (backend + tty-gateway + coffee-tui) and verify it non-interactively over SSH with scripts/pty-run.py - screens at several sizes, keys, resize, colour depths, user-name routing (shop, gallery@), and no leftover processes. Use to prove a UI or SSH change works end to end.
---

# Run and verify over SSH

## 1. Start it

```sh
pnpm install
pnpm dev        # builds coffee-tui + tty-gateway, migrates + seeds the backend, starts backend + gateway
```

The backend's `dev` script runs `prisma migrate deploy` and the (idempotent) seed on every start, so a fresh
checkout needs no database step. `db:reset` is for humans only: Prisma refuses `migrate reset` from an AI agent.

`pnpm dev` (`scripts/dev.mjs`) prints `✓ ready — connect with: ssh -p 2222 localhost` once the backend answers
`/v1/health` and the gateway listens. It fails with a clear message when a piece is missing (no `dev` script in
`apps/server`, cargo build failure, port in use, the gateway exiting). Ctrl+C (or SIGINT/SIGTERM to
`node scripts/dev.mjs`) stops whatever runs, in any phase: the cargo build (with its rustc jobs) as well as backend
and gateway, each in its own process group. `pnpm dev --skip-build` reuses the built binaries. Run it in the
background for automation (`run_in_background`), then wait for the line containing `✓ ready` (plain `ready` also
matches the seed's `already present`). Other ports: `PORT=3917 TTY_GATEWAY_ADDR=127.0.0.1:2917`.
Each session starts `target/debug/coffee-tui` afresh, so `cargo build -p coffee-tui` is enough to try a TUI change
without restarting `pnpm dev`.

The gateway runs `target/debug/tty-gateway --config deploy/gateway.toml`. That file is the shop: the banner, the
Chinese messages, the limits and two routes. The **SSH user name** picks the route: `gallery` gets the UI gallery
(`coffee-tui` with `COFFEE_UI_GALLERY=1`), `coffee`, `shop` and every other name (the default route) get the shop.
`pnpm dev` gives the gateway `COFFEE_API_URL` and a random per-run `COFFEE_INTERNAL_KEY` (never printed); the
shop route hands them, `COFFEE_COLORS` (if you set it) and the session's `COFFEE_SSH_FINGERPRINT` /
`COFFEE_CLIENT_IP` to each `coffee-tui` through its `env` table. The gateway itself reads only `TTY_GATEWAY_CONFIG`,
`TTY_GATEWAY_ADDR`, `TTY_GATEWAY_HOST_KEY` (default `.data/ssh_host_ed25519_key`) and `TTY_GATEWAY_LOG`.

Limits are in the file, not the environment (`deploy/README.md` lists every key): `connections_per_minute = 30`
new connections per address per minute, so a script that connects in a tight loop hits it. Use a copy with
`connections_per_minute = 0` (and a free `listen`) and start the gateway on it yourself; never edit the limits in
`deploy/gateway.toml` for a test. Check any file first: `target/debug/tty-gateway --check --config FILE`. A running
gateway rereads its file on `kill -HUP <pid>` (new connections only; a broken file keeps the old one).

Without a backend: `COFFEE_API_URL=fixture: target/debug/tty-gateway --config deploy/gateway.toml` (after
`cargo build -p coffee-tui -p tty-gateway`) serves sessions from the committed fixtures (the header shows
`◇ 样例数据`).

## 2. Connect by hand

`pnpm ssh` (= `ssh -p 2222 -o SendEnv=COLORTERM -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
localhost`). Try both with your key and without (`-o PubkeyAuthentication=no -o IdentitiesOnly=yes`): both must get
in without a password prompt, after the banner `◆ROAST/TTY 终端咖啡馆 / 演示环境 · 不是真实商店 ...`. Never touch
`~/.ssh`. Quitting takes Ctrl+C **twice** (the first shows `再按一次 Ctrl+C 退出`).

The user name picks the program: `ssh -p 2222 gallery@localhost` (plus the options above) opens the gallery,
`coffee@`, `shop@` or any other name the shop. The gateway logs `user` and `route` on every `authenticated` and
`session started` line.

## 3. Verify without a human: `scripts/pty-run.py`

It runs a command on a real PTY of a given size, replays the output in a small terminal emulator (wide CJK handled),
and prints the final screen as text. `expect` mangles multi-byte text; do not use it.

```sh
SSH="ssh -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR localhost"

# A new connection opens on the login page (the focus in 用户名, so the status bar says 离开输入):
python3 scripts/pty-run.py --size 160x50 --step 'until=离开输入' --expect '登录 · SIGN IN' --expect '演示环境' -- $SSH

# Routing: an unknown user name gets the default route (the shop); gallery@ gets the gallery:
python3 scripts/pty-run.py --size 160x50 --step 'until=离开输入' -- ${SSH/localhost/whoever@localhost}
python3 scripts/pty-run.py --size 160x50 --step wait=2 --step snap -- ${SSH/localhost/gallery@localhost}

# Too small, then resized back (state kept):
python3 scripts/pty-run.py --size 60x20 --step 'until=终端太小' --step resize=160x50 \
  --step 'gone=终端太小' -- $SSH

# Keys: Esc leaves the field, g browses as a guest (wait between them: Esc+g at once reads as Alt+g), then move,
# open a product (loaded once its SKU line shows; at 100×30 that line is below the fold), back; a snapshot:
python3 scripts/pty-run.py --size 160x50 --step 'until=离开输入' --step key=esc --step 'until=访客' \
  --step text=g --step 'until=商品列表' --step key=down*2 --step key=enter --step 'until=SKU RT-' --step snap \
  --step key=esc --step 'until=商品列表' -- $SSH

# Colour depth over SSH (grep the raw bytes: 38;2; = truecolor, 38;5; = 256). The session gets 24-bit colour
# from a forwarded COLORTERM (pty-run sets COLORTERM=truecolor for its child unless --colorterm says otherwise):
python3 scripts/pty-run.py --size 100x30 --raw /tmp/x.raw --step 'until=离开输入' \
  -- pnpm ssh                      # sends COLORTERM
# ... or from a truecolor brand in TERM, which plain ssh does send (no COLORTERM at all here):
python3 scripts/pty-run.py --size 100x30 --term xterm-kitty --colorterm '' --raw /tmp/kitty.raw \
  --step 'until=离开输入' -- $SSH # 38;2; only
# ... while a plain xterm-256color session stays at 256 (pty-run does not answer XTGETTCAP):
python3 scripts/pty-run.py --size 100x30 --colorterm '' --raw /tmp/plain.raw --step 'until=离开输入' -- $SSH
# The whole decision table: crates/tty-host/src/color.rs.

# Forcing a depth: the shop's route takes only LANG, LC_* and COLORTERM from the client, so COFFEE_COLORS must be
# set on the gateway (`COFFEE_COLORS=mono pnpm dev --skip-build`; the route passes it on), or test the TUI alone:
python3 scripts/pty-run.py --size 100x30 --env COFFEE_COLORS=mono --env COFFEE_API_URL=fixture: \
  -- target/debug/coffee-tui
```

Steps: `wait=S`, `until=TEXT`, `gone=TEXT`, `text=STR`, `key=NAME[*N]` (enter, esc, tab, up, ctrl-c, ...),
`resize=COLSxROWS`, `exit=S`, `snap`. `--expect TEXT` makes the exit status fail when TEXT is missing; `--frame`
draws the screen's border; `--raw FILE` keeps the bytes (colours are only there: the printed screen drops them).
Without SSH: `-- target/debug/coffee-tui` (`--env COFFEE_API_URL=fixture:` / `fixture:empty` / `fixture:down` for
the committed fixtures; the header then shows `◇ 样例数据`).

## 4. Check the sizes and states

Run at 160×50, 130×40, 100×30, 80×24 and 60×20 (too small). For each: the demo notice is visible, the status bar
lists the page's keys, nothing is cut mid-character, CJK columns line up.

## 5. Check cleanup

```sh
python3 scripts/pty-run.py --size 100x30 --step 'until=离开输入' --step key=ctrl-c --step 'until=再按一次' \
  --step key=ctrl-c --step exit=5 -- $SSH  # quitting ends the SSH session (status 0)
pgrep -fl coffee-tui                       # after the client is gone: no children left (allow 2 s)
```

Killing the client (`kill -9` the `ssh` process) must also end its `coffee-tui` within 2 seconds (the gateway logs
`session ended: client left ... how="hangup"`); so must a client EOF (`ssh -tt ... </dev/null` ends at once). A
SIGTERM to `tty-gateway` with sessions open tells each client it is shutting down, hangs the programs up and exits
within `server.shutdown_grace` (2 s) + 1 s. When done, stop `pnpm dev` (Ctrl+C, or SIGINT to `pnpm dev` and
`node scripts/dev.mjs`) and prove nothing is left:
`pgrep -fl "dev.mjs|tty-gateway|coffee-tui|src/main.ts"` prints nothing and ports 3000/2222 are free
(`lsof -nP -iTCP:3000 -iTCP:2222 -sTCP:LISTEN`).

## 6. Automated end-to-end tests

Two layers, both automated:

- `pnpm test:e2e` (also part of `cargo test --workspace`) runs the in-process SSH tests, built on
  `crates/tty-gateway-testkit` (an in-process gateway on an ephemeral port, a russh client with a `vt100` screen):
  `crates/tty-gateway/tests` with stand-in programs and neutral messages (auth, limits, idle, time limit, crashes,
  resize, routes, reload, `--check`, shutdown, input floods, EOF) and `crates/coffee-tui/tests/ssh.rs` with the real
  `coffee-tui`, `deploy/gateway.toml` and a stub backend serving the contract fixtures (Chinese messages, colour
  depth, the brand). Fast. `TTY_GATEWAY_LOG=info` makes their servers log to stderr.
- `pnpm test:e2e:real` builds `coffee-tui` + `tty-gateway` and runs `scripts/e2e/run.py`: the shop's flows over the
  **real** stack. Each flow gets a copy of a freshly migrated and seeded SQLite database, its own backend
  (`node apps/server/src/main.ts`) and `tty-gateway` (on `deploy/gateway.toml`, with its own port and no connection
  rate) on free ports, and stock `ssh` clients (throwaway keys in
  `.data/e2e/work/keys`, or none) on PTYs read by `pty-run.py`'s emulator. Flows: `main`, `money`, `stock`,
  `session`, `gateway` (what each checks: `docs/acceptance.md`). About 2 minutes.

```sh
pnpm test:e2e:real                          # all flows
python3 scripts/e2e/run.py main money       # some (binaries already built)
python3 scripts/e2e/run.py --repeat 5       # stability: a flaky flow is a bug to root-cause
E2E_WAIT=30 python3 scripts/e2e/run.py      # more time per step on a slow machine (default 20 s)
python3 scripts/e2e/run.py --gateway 127.0.0.1:2222 smoke   # a purchase on a gateway you started
```

Output: `.data/e2e/<flow>/NN-<session>-<step>.txt` (text screenshots), `backend.log`, `sshd.log`, `shop.db`, and on a
failure `FAIL.txt` with the failing step, the last screen of every open session and the log tails. After each flow
the runner fails if a `coffee-tui` is left or a log holds a password, the internal key or a bearer token.

Writing or changing a flow (`scripts/e2e/flows.py`; shop steps in `shop.py`; the stack and `Term` in `harness.py`):

- **Wait for the screen, never for time or a key count.** `t.wait("text", ...)`, `t.wait_any`, `t.gone`,
  `t.wait_footer` (the status bar), `t.wait_for(predicate, "what")`; each has a timeout and fails with the screen.
  Steps that move focus or a selection press their key until the screen confirms (`focus_login_field` reads the
  status bar's `离开输入` and the cursor's row; `choose` reads the `▐ … ▌` option), with a bound.
- Wait for a page to be ready before keys that act on it (a detail is ready when its `SKU RT-…` line shows).
- `t.key("esc")` pauses after Esc so the next key is not read as Alt+key.
- Money is checked against `shop.quote()` (an independent copy of the rules), the database through `stack.sql()`.
- `t.snap("label")` keeps a screenshot; screens and logs must hold no real secrets (test passwords only).

## 7. The containers

`deploy/smoke-test.sh` (Docker with compose, `python3` and `ssh` needed; CI's `docker` job runs it) starts the gateway
and backend images with `deploy/compose.yaml` and checks them over a stock `ssh`, including a purchase (the `smoke`
flow above, screens in `.data/e2e-smoke/`). With no local Docker daemon, rely on CI; do not start one just for this.
