# deploy/

Two images and a compose file that runs them together:

| File                | Image (tag used here) | What                                                                            |
| ------------------- | --------------------- | ------------------------------------------------------------------------------- |
| `Dockerfile`        | `roast-tty-ssh`       | the SSH gateway: `coffee-sshd` and the `coffee-tui` it starts for each session  |
| `server.Dockerfile` | `roast-tty-server`    | the backend `@coffee/server` (Koa + Prisma + SQLite)                            |
| `compose.yaml`      |                       | both, the backend on a private network, the gateway on port 2222                |
| `smoke-test.sh`     |                       | starts the pair and checks it end to end over a stock `ssh` (CI's `docker` job) |
| `server/`           |                       | the backend image's entry point and its Prisma CLI settings                     |

One process per SSH session on its own PTY: see `docs/adr/0001-ssh-session-model.md`. Each `*.Dockerfile.dockerignore`
limits its build context (`target/`, `node_modules/`, databases and `.env*` are never sent).

## Run both with compose

```sh
export COFFEE_INTERNAL_KEY="$(openssl rand -base64 32)"   # or put it in deploy/.env (git-ignored)
docker compose -f deploy/compose.yaml up -d --build
ssh -p 2222 localhost
docker compose -f deploy/compose.yaml logs -f gateway     # sessions, fingerprints, refusals
```

- **The internal key** is yours to make and keep: compose refuses to start without `COFFEE_INTERNAL_KEY`, both
  services get the same value, and neither image contains one. Never commit it.
- **The backend is private**: it listens on `0.0.0.0:3000` inside its container, but only on the `internal` network
  (no route out, no published port). Only the gateway reaches it, as `http://backend:3000`, with the key.
- **Volumes**: `database` holds the SQLite file (`/data/coffee.db`), `host-key` the gateway's ed25519 host key. Keep
  both: a new host key makes every client warn about a changed host. `down -v` deletes them.
- **Start-up**: the backend runs `prisma migrate deploy` (applies only migrations the database has not recorded; never
  resets or drops anything) and the seed (inserts only missing catalogue rows; never touches accounts, carts, orders
  or stock) on every start, then serves; the gateway waits for its health check. Restarting is always safe.
- Both run as uid 10001 with all capabilities dropped and `no-new-privileges`, under `init` (zombies reaped).
- Client addresses: with Docker's default userland proxy, connections from the Docker host itself
  (`ssh -p 2222 localhost`) reach the gateway from the bridge address, so they share one address's limits. Clients
  from other machines keep their own address (iptables DNAT).

`deploy/smoke-test.sh` (with `COMPOSE_UP_ARGS=--build`, or with `COFFEE_GATEWAY_IMAGE`/`COFFEE_SERVER_IMAGE` naming
images built beforehand) starts the pair with a throwaway key and checks the SSH banner, that the backend is not
published and refuses calls without the key, that a key-less stock `ssh` session shows the products from the backend
(not the built-in sample data), that the session's `coffee-tui` is gone after the client leaves, and that a restarted
backend migrates and seeds nothing twice. Then it removes everything.

## Build the images alone

From the repository root (BuildKit, the default in current Docker):

```sh
docker build -f deploy/Dockerfile -t roast-tty-ssh .
docker build -f deploy/server.Dockerfile -t roast-tty-server .
```

The backend image is multi-stage: the build stage installs the workspace (`pnpm install --frozen-lockfile`, which
generates the Prisma client), builds `dist/main.mjs` and a bundled `dist/seed.mjs` with tsdown, and takes the server's
own dependencies with `pnpm deploy` (the runtime libraries and the Prisma CLI for `migrate deploy`). The runtime stage
holds only those `node_modules`, `dist/`, the Prisma schema and migrations, `server/prisma.config.mjs` and
`server/start.sh`.

Run either alone:

```sh
docker run --rm -p 2222:2222 -v roast-tty-ssh-data:/data \
  -e COFFEE_API_URL=http://backend:3000 -e COFFEE_INTERNAL_KEY=… roast-tty-ssh
docker run --rm -v roast-tty-db:/data -e COFFEE_INTERNAL_KEY=… roast-tty-server   # publish nothing
```

## Gateway settings (environment)

| Variable                            | Image default                | Meaning                                                                               |
| ----------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------- |
| `COFFEE_SSH_ADDR`                   | `0.0.0.0:2222`               | listen address (binary default `127.0.0.1:2222`)                                      |
| `COFFEE_SSH_HOST_KEY`               | `/data/ssh_host_ed25519_key` | host key path; made (0600) on first run                                               |
| `COFFEE_TUI_BIN`                    | `/usr/local/bin/coffee-tui`  | the per-session program                                                               |
| `COFFEE_API_URL`                    | `http://127.0.0.1:3000`      | backend base URL, passed to each session (compose: `http://backend:3000`)             |
| `COFFEE_INTERNAL_KEY`               | unset                        | passed to each session as is; never logged                                            |
| `COFFEE_COLORS`                     | unset                        | force `truecolor` / `256` / `16` / `mono` for every session                           |
| `COFFEE_SSH_MAX_SESSIONS`           | `64`                         | concurrent sessions in all                                                            |
| `COFFEE_SSH_MAX_SESSIONS_PER_IP`    | `8`                          | concurrent sessions per client address (twice as many connections may be open)        |
| `COFFEE_SSH_CONNECTIONS_PER_MINUTE` | `30`                         | new connections per client address per minute, in bursts of as many (`0`: off)        |
| `COFFEE_SSH_IDLE_TIMEOUT`           | `900`                        | seconds without input, from the session's first output, before it closes (`0`: never) |
| `COFFEE_SSH_AUTH_TIMEOUT`           | `30`                         | seconds from accept to a running session (handshake, login, shell request)            |
| `COFFEE_SSH_LOG`                    | `info,russh=warn`            | log filter (`tracing` syntax); logs go to stderr                                      |

Connections over the per-address limits are closed before the SSH handshake (logged once per burst, with the
address). A session refused for the session limits still sees why, in the banner and on the channel.

What the gateway logs at `info`: connections (peer address), logins (method and, for keys, the SHA256 fingerprint, by
design), sessions (pid, `TERM`, size, how they ended) and refusals. Never the internal key, environment values or
anything typed.

Colours: clients send only `LANG`, `LC_*` and `COLORTERM` into a session, and OpenSSH sends `COLORTERM` only when
asked (`ssh -o SendEnv=COLORTERM`). Sessions still get 24-bit colour from a truecolor terminal brand in `TERM` (kitty,
Ghostty, foot, Alacritty, WezTerm, rio, `*-direct`) or from a `*-256color` terminal that answers XTGETTCAP `RGB`; the
rest get 256 colours. Set `COFFEE_COLORS=truecolor` to force 24-bit colour for everyone only when you know the
clients' terminals support it.

Each idle session is one `coffee-tui` of 15–20 MB RSS (see the ADR's phase 2 measurement); size the gateway container for
`COFFEE_SSH_MAX_SESSIONS` of them plus the gateway.

## Backend settings (environment)

| Variable              | Image default             | Meaning                                                      |
| --------------------- | ------------------------- | ------------------------------------------------------------ |
| `COFFEE_INTERNAL_KEY` | unset (compose: required) | every route but `/v1/health` requires it as `X-Internal-Key` |
| `HOST` / `PORT`       | `0.0.0.0` / `3000`        | listen address inside the container                          |
| `DATABASE_URL`        | `file:/data/coffee.db`    | the SQLite file, on the `/data` volume                       |
