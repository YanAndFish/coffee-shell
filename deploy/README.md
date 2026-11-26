# deploy/

Two images and a compose file that runs them together:

| File                | Image (tag used here) | What                                                                            |
| ------------------- | --------------------- | ------------------------------------------------------------------------------- |
| `Dockerfile`        | `roast-tty-ssh`       | the SSH gateway: `tty-gateway`, `gateway.toml` and the `coffee-tui` it starts   |
| `gateway.toml`      |                       | the shop's gateway configuration (dev, Docker and the e2e flows all read it)    |
| `server.Dockerfile` | `roast-tty-server`    | the backend `@coffee/server` (Koa + Prisma + SQLite)                            |
| `compose.yaml`      |                       | both, the backend on a private network, the gateway on port 2222                |
| `smoke-test.sh`     |                       | starts the pair and checks it end to end over a stock `ssh` (CI's `docker` job) |
| `server/`           |                       | the backend image's entry point and its Prisma CLI settings                     |

One process per SSH session on its own PTY: see `docs/adr/0001-ssh-session-model.md`; routes, the config file,
reload and shutdown: `docs/adr/0003-app-agnostic-gateway.md`. Each `*.Dockerfile.dockerignore`
limits its build context (`target/`, `node_modules/`, databases and `.env*` are never sent).

## Run both with compose

```sh
export COFFEE_INTERNAL_KEY="$(openssl rand -base64 32)"   # or put it in deploy/.env (git-ignored)
docker compose -f deploy/compose.yaml up -d --build
ssh -p 2222 localhost
ssh -p 2222 gallery@localhost                             # the UI gallery; any other user name: the shop
docker compose -f deploy/compose.yaml logs -f gateway     # sessions (user, route), fingerprints, refusals
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
(not the built-in sample data), that a stock `ssh` session with a throwaway key registers, adds a bag, saves an
address, checks out and gets a receipt (`scripts/e2e/run.py --gateway 127.0.0.1:2222 smoke`; its text screenshots
go to `.data/e2e-smoke/`), that the sessions' `coffee-tui` are gone after the clients leave, and that a restarted
backend migrates and seeds nothing twice. Then it removes everything. It needs `python3` and `ssh` on the host.

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

## Gateway settings (the config file)

`tty-gateway --config <file>` reads one TOML file; the image holds `deploy/gateway.toml` and points
`TTY_GATEWAY_CONFIG` at it. The gateway knows nothing about the shop: the shop is that file (its banner, its Chinese
messages, its routes and their environment). `tty-gateway --check --config <file>` checks a file and prints its
routes (environment names only, never values); exit status 1 means the file is broken, a missing program is only a
warning. Unknown keys are errors. Durations are `"500ms"`, `"30s"`, `"15m"`, `"2h"` or `"0"`.

| Key                                            | Default                         | Meaning                                                                                        |
| ---------------------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------- |
| `server.listen`                                | `127.0.0.1:2222`                | listen address (`TTY_GATEWAY_ADDR` overrides it; needs a restart)                              |
| `server.host_key`                              | `.data/ssh_host_ed25519_key`    | ed25519 host key, made (0600) on first run (`TTY_GATEWAY_HOST_KEY`; needs a restart)           |
| `server.server_id`                             | `SSH-2.0-tty_gateway`           | the SSH version line (the shop: `SSH-2.0-ROAST_TTY`)                                           |
| `server.banner`                                | none                            | the authentication banner, the same for every route (sent before the user is known)            |
| `server.default_route`                         | the only route                  | the route of every user name no route lists; needed with two routes or more                    |
| `server.shutdown_grace`                        | `2s`                            | on SIGTERM / SIGINT, how long sessions get after the hang-up before they are killed            |
| `limits.max_sessions`                          | `64`                            | concurrent sessions in all                                                                     |
| `limits.max_sessions_per_ip`                   | `8`                             | concurrent sessions per client address (twice as many connections may be open)                 |
| `limits.connections_per_minute`                | `30`                            | new connections per client address per minute, in bursts of as many (`0`: off)                 |
| `limits.auth_timeout`                          | `30s`                           | from accept to a running session (handshake, login, shell request); at least `1s`              |
| `limits.idle_timeout`                          | `15m`                           | no input, from the session's first output, before it closes (`0`: never; status 124)           |
| `limits.max_session_time`                      | `0` (never)                     | a session's longest life, input or not (status 124)                                            |
| `messages.*`                                   | neutral English                 | what clients are told (below)                                                                  |
| `routes.NAME.users`                            | `[NAME]`                        | the SSH user names of this route (exact; one name in one route only)                           |
| `routes.NAME.command`                          | required                        | the program and its arguments; a bare name is looked for next to `tty-gateway`, then on `PATH` |
| `routes.NAME.cwd`                              | the temporary directory         | the program's working directory                                                                |
| `routes.NAME.accept_env`                       | `["LANG", "LC_*", "COLORTERM"]` | what the client may send into a session (`ssh -o SendEnv`); never a name the route sets        |
| `routes.NAME.env`                              | none                            | the session's environment (below)                                                              |
| `routes.NAME.idle_timeout`, `max_session_time` | `[limits]`                      | this route's own                                                                               |
| `routes.NAME.max_sessions`                     | none                            | sessions on this route at once, besides the global caps (`full_route` when reached)            |
| `routes.NAME.messages.*`                       | `[messages]`                    | this route's own wording                                                                       |

The messages: `no_pty` (a shell without a PTY), `unavailable` (the program cannot start), `crashed` (`{code}`: its
non-zero exit status), `idle` (`{minutes}`), `time_limit`, `full`, `full_per_ip`, `full_route` (`{max}`: the cap
reached) and `shutting_down`.

A session's environment starts empty: `PATH`, `HOME`, `TMPDIR`, `USER` and `LOGNAME` from the gateway, the
client's `TERM` (an odd value becomes `xterm-256color`), `LANG` (the client's if it is UTF-8, else `C.UTF-8`), what the
client sent that `accept_env` allows, then the route's `env`. Its values take `${user}` (only a plain name of
`A-Z a-z 0-9 . _ @ + -`, at most 64), `${route}`, `${fingerprint}` (`SHA256:…`, only after a verified key),
`${client_ip}`, `${env:NAME}` and `${env:NAME:-default}` (the gateway's environment when the file is read). An entry
whose variable has no value is left out, so a secret given to the gateway reaches only the routes that name it.

The gateway itself reads only these variables:

| Variable               | Image default                | Meaning                                                   |
| ---------------------- | ---------------------------- | --------------------------------------------------------- |
| `TTY_GATEWAY_CONFIG`   | the image's `gateway.toml`   | the file, when there is no `--config`                     |
| `TTY_GATEWAY_ADDR`     | `0.0.0.0:2222`               | overrides `server.listen`                                 |
| `TTY_GATEWAY_HOST_KEY` | `/data/ssh_host_ed25519_key` | overrides `server.host_key`                               |
| `TTY_GATEWAY_LOG`      | `info,russh=warn`            | log filter (`tracing` syntax, else `RUST_LOG`); to stderr |

The shop's routes in `deploy/gateway.toml`:

| Route     | Users                               | Session environment (besides `TERM`, `LANG`, …)                                                                             |
| --------- | ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `coffee`  | `coffee`, `shop`; the default route | `COFFEE_API_URL`, `COFFEE_INTERNAL_KEY`, `COFFEE_COLORS` (from the gateway's), `COFFEE_SSH_FINGERPRINT`, `COFFEE_CLIENT_IP` |
| `gallery` | `gallery`                           | `COFFEE_UI_GALLERY=1`, `COFFEE_CLIENT_IP`; no internal key                                                                  |

Both run `coffee-tui` (next to `tty-gateway` in the image). So compose still passes the app's settings to the
gateway's environment, and the route hands them on:

| Variable              | Compose               | Meaning                                                                      |
| --------------------- | --------------------- | ---------------------------------------------------------------------------- |
| `COFFEE_API_URL`      | `http://backend:3000` | the backend, for every shop session                                          |
| `COFFEE_INTERNAL_KEY` | required              | the same key as the backend's; only the `coffee` route gets it; never logged |
| `COFFEE_COLORS`       | empty (left out)      | force `truecolor` / `256` / `16` / `mono` for every shop session             |
| `COFFEE_SSH_PORT`     | `2222`                | the published port on the host                                               |

**Your own configuration**: copy `deploy/gateway.toml`, change what you need (limits, messages, another route) and
mount it over the image's file, or anywhere with `TTY_GATEWAY_CONFIG` pointing at it:

```yaml
gateway:
  environment:
    TTY_GATEWAY_CONFIG: /config/gateway.toml
  volumes:
    - ./my-gateway.toml:/config/gateway.toml:ro
```

Check it first: `docker compose -f deploy/compose.yaml run --rm --no-deps gateway --check`, or
`tty-gateway --check --config my-gateway.toml` locally.

**Reload** without dropping anyone: `docker compose -f deploy/compose.yaml kill -s HUP gateway`. The file is read and
checked again; new connections use it, running sessions keep the configuration they started with, and a broken file
is logged and ignored (the old configuration stays). `listen` and `host_key` changes are logged and need a restart.

**Stopping**: `docker compose stop` sends SIGTERM. The gateway stops accepting, tells every session `shutting_down`,
hangs up each program (SIGHUP to its process group), kills what is left after `server.shutdown_grace` (2 s) and
closes the connections: at most the grace plus one second, well inside Docker's 10 s before SIGKILL. Keep
`shutdown_grace` below that if you raise it.

Connections over the per-address limits are closed before the SSH handshake (logged once per burst, with the
address). A session refused for the session limits still sees why, in the banner and on the channel.

What the gateway logs at `info`: connections (peer address), logins (method, user name, route and, for keys, the
SHA256 fingerprint, by design), sessions (user, route, pid, `TERM`, size, how they ended), refusals, reloads and the
shutdown. Never the internal key, environment values or anything typed.

Colours: the shop's routes accept only `LANG`, `LC_*` and `COLORTERM` from a client, and OpenSSH sends `COLORTERM` only when
asked (`ssh -o SendEnv=COLORTERM`). Sessions still get 24-bit colour from a truecolor terminal brand in `TERM` (kitty,
Ghostty, foot, Alacritty, WezTerm, rio, `*-direct`) or from a `*-256color` terminal that answers XTGETTCAP `RGB`; the
rest get 256 colours. Set `COFFEE_COLORS=truecolor` to force 24-bit colour for everyone only when you know the
clients' terminals support it.

Each session is one `coffee-tui`: about 23 MB RSS on the login page and 35–38 MB after browsing a product and the
cart (ADR 0001, "第二阶段完成后复测"). Size the gateway container for `limits.max_sessions` of them at 40 MB each
(64 × 40 MB ≈ 2.6 GB) plus the gateway (under 10 MB).

## Backend settings (environment)

| Variable              | Image default             | Meaning                                                      |
| --------------------- | ------------------------- | ------------------------------------------------------------ |
| `COFFEE_INTERNAL_KEY` | unset (compose: required) | every route but `/v1/health` requires it as `X-Internal-Key` |
| `HOST` / `PORT`       | `0.0.0.0` / `3000`        | listen address inside the container                          |
| `DATABASE_URL`        | `file:/data/coffee.db`    | the SQLite file, on the `/data` volume                       |
