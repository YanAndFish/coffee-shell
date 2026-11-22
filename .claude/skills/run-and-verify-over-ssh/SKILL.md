---
name: run-and-verify-over-ssh
description: Run the whole ROAST/TTY demo locally (backend + coffee-sshd + coffee-tui) and verify it non-interactively over SSH with scripts/pty-run.py - screens at several sizes, keys, resize, colour depths, and no leftover processes. Use to prove a UI or SSH change works end to end.
---

# Run and verify over SSH

## 1. Start it

```sh
pnpm install
pnpm --filter @coffee/server db:reset     # first time: migrate + seed the 8 beans into .data/
pnpm dev                                  # builds coffee-tui + coffee-sshd, starts backend + sshd
```

`pnpm dev` (`scripts/dev.mjs`) prints `✓ ready — connect with: ssh -p 2222 localhost` once the backend answers
`/v1/health` and sshd listens. It fails with a clear message when a piece is missing (no `dev` script in
`apps/server`, cargo build failure, port in use, sshd exiting). Ctrl+C stops both. `--skip-build` reuses the built
binaries. Run it in the background for automation (`run_in_background`), then wait for the ready line.

Environment it honours: `HOST`/`PORT` (backend), `COFFEE_SSH_ADDR` (127.0.0.1:2222), `COFFEE_SSH_HOST_KEY`
(`.data/ssh_host_ed25519_key`), `COFFEE_COLORS`. The internal key is random per run and never printed.
<!-- TODO(integration): confirm db:reset seeds, and that coffee-sshd reads exactly these variables -->

## 2. Connect by hand

`pnpm ssh` (= `ssh -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null localhost`). Try both with
your key and without (`-o PubkeyAuthentication=no -o IdentitiesOnly=yes -o IdentityFile=/dev/null`): both must get in
without a password.

## 3. Verify without a human: `scripts/pty-run.py`

It runs a command on a real PTY of a given size, replays the output in a small terminal emulator (wide CJK handled),
and prints the final screen as text. `expect` mangles multi-byte text; do not use it.

```sh
SSH="ssh -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR localhost"

# The shop at the design size, with products from the backend:
python3 scripts/pty-run.py --size 160x50 --step 'until=ROAST/TTY' --expect '演示环境' -- $SSH

# Too small, then resized back (state kept):
python3 scripts/pty-run.py --size 60x20 --step 'until=终端太小' --step resize=160x50 \
  --step 'gone=终端太小' -- $SSH

# Keys: move, open, back; snapshots in between:
python3 scripts/pty-run.py --size 100x30 --step 'until=选购' --step key=down*2 --step key=enter \
  --step wait=0.5 --step snap --step key=esc -- $SSH

# Colour depths (exported to the session by sshd):
python3 scripts/pty-run.py --size 100x30 --env COFFEE_COLORS=mono -- $SSH
```

Steps: `wait=S`, `until=TEXT`, `gone=TEXT`, `text=STR`, `key=NAME[*N]` (enter, esc, tab, up, ctrl-c, ...),
`resize=COLSxROWS`, `exit=S`, `snap`. `--expect TEXT` makes the exit status fail when TEXT is missing; `--frame`
draws the screen's border; `--raw FILE` keeps the bytes. Without SSH: `-- target/debug/coffee-tui`.
<!-- TODO(integration): confirm whether coffee-sshd forwards COFFEE_COLORS from the client env or only its own -->

## 4. Check the sizes and states

Run at 160×50, 130×40, 100×30, 80×24 and 60×20 (too small). For each: the demo notice is visible, the status bar
lists the page's keys, nothing is cut mid-character, CJK columns line up.

## 5. Check cleanup

```sh
python3 scripts/pty-run.py --size 100x30 --step 'until=ROAST/TTY' --step key=ctrl-c --step key=ctrl-c \
  --step exit=5 -- $SSH                    # quitting ends the SSH session
pgrep -fl coffee-tui                       # after the client is gone: no children left (allow 2 s)
```

Killing the client (pty-run hangs up at the end) must also end its `coffee-tui` within 2 seconds.

## 6. Automated end-to-end tests

`pnpm test:e2e` (or `cargo test --workspace`) runs `crates/coffee-sshd/tests`, which start sshd and a client
themselves. Report automated and manual evidence separately, with the commands and the screen tails.
