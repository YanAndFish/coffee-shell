# deploy/

The SSH gateway image: `coffee-sshd` and the `coffee-tui` it starts for each session
(one process per SSH session on its own PTY — see `docs/adr/0001-ssh-session-model.md`).
The backend (`apps/server`) is a separate service; this image only needs its URL.

## Build

From the repository root (BuildKit, the default in current Docker):

```sh
docker build -f deploy/Dockerfile -t roast-tty-ssh .
```

`deploy/Dockerfile.dockerignore` limits the build context to the Rust workspace, so
`target/` and `node_modules/` are never sent.

## Run

```sh
docker volume create roast-tty-ssh-data
docker run --rm -p 2222:2222 \
  -v roast-tty-ssh-data:/data \
  -e COFFEE_API_URL=http://backend:3000 \
  -e COFFEE_INTERNAL_KEY=… \
  roast-tty-ssh
ssh -p 2222 localhost
```

The ed25519 host key is generated into the `/data` volume on first start (mode 0600,
owned by the image's non-root `coffee` user, uid 10001). Keep the volume, or every
client sees a changed host key after a restart.

## Settings (environment)

| Variable                         | Image default                | Meaning                                                     |
| -------------------------------- | ---------------------------- | ----------------------------------------------------------- |
| `COFFEE_SSH_ADDR`                | `0.0.0.0:2222`               | listen address (binary default `127.0.0.1:2222`)            |
| `COFFEE_SSH_HOST_KEY`            | `/data/ssh_host_ed25519_key` | host key path; made on first run                            |
| `COFFEE_TUI_BIN`                 | `/usr/local/bin/coffee-tui`  | the per-session program                                     |
| `COFFEE_API_URL`                 | `http://127.0.0.1:3000`      | backend base URL, passed to each session                    |
| `COFFEE_INTERNAL_KEY`            | unset                        | passed to each session as is; never logged                  |
| `COFFEE_COLORS`                  | unset                        | force `truecolor` / `256` / `16` / `mono` for every session |
| `COFFEE_SSH_MAX_SESSIONS`        | `64`                         | concurrent sessions in all                                  |
| `COFFEE_SSH_MAX_SESSIONS_PER_IP` | `8`                          | concurrent sessions per client address                      |
| `COFFEE_SSH_IDLE_TIMEOUT`        | `1800`                       | seconds without input before a session closes (`0`: never)  |
| `COFFEE_SSH_AUTH_TIMEOUT`        | `30`                         | seconds a connection may take to log in                     |
| `COFFEE_SSH_LOG`                 | `info,russh=warn`            | log filter (`tracing` syntax); logs go to stderr            |

Each idle session is one `coffee-tui` of about 16 MB RSS (see the ADR's measurement); size the
container for `COFFEE_SSH_MAX_SESSIONS` of them plus the gateway.
