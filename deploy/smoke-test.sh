#!/usr/bin/env bash
# Smoke test of deploy/compose.yaml (CI's `docker` job runs it; it works by hand too):
#
#   COFFEE_GATEWAY_IMAGE=roast-tty-ssh:ci COFFEE_SERVER_IMAGE=roast-tty-server:ci \
#     deploy/smoke-test.sh            # images built beforehand
#   COMPOSE_UP_ARGS=--build deploy/smoke-test.sh
#
# Starts the pair with a throwaway COFFEE_INTERNAL_KEY and checks: the gateway answers
# with its SSH banner; the backend is not published and refuses calls without the key; a
# stock `ssh guest@` session (no key; a user name no route lists, so the default route: the
# shop) shows the product list served by the backend, not the built-in sample data;
# `ssh gallery@` gets the UI gallery and the gateway logs `route=gallery`; a stock
# `ssh guest@` session with a key registers, adds a bean, saves an
# address, checks out and gets its receipt (scripts/e2e/run.py's `smoke` flow; its screens
# land in .data/e2e-smoke/); the sessions' coffee-tui are gone after the clients leave; a
# restarted backend starts again on the same database without migrating or seeding twice.
#
# It runs as a Compose project of its own (`roast-tty-smoke-<pid>`, or COFFEE_SMOKE_PROJECT)
# on a free port (or COFFEE_SMOKE_PORT), never as the deployment's `roast-tty` on 2222: a
# deployment on the same machine keeps its containers, its database and its host key.
# Everything of the test's own project (containers, networks, volumes) is removed at the
# end. Needs python3 and ssh.
set -euo pipefail
cd "$(dirname "$0")/.."

project=${COFFEE_SMOKE_PROJECT:-roast-tty-smoke-$$}
if [ "$project" = roast-tty ]; then
  echo "COFFEE_SMOKE_PROJECT=roast-tty is the deployment's project: the cleanup would delete its volumes" >&2
  exit 2
fi
compose=(docker compose -p "$project" -f deploy/compose.yaml)
port=${COFFEE_SMOKE_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')}
# Both reach compose.yaml; they beat a deploy/.env and the shell's own settings.
COFFEE_SSH_PORT=$port
COFFEE_INTERNAL_KEY=$(openssl rand -hex 32)
export COFFEE_SSH_PORT COFFEE_INTERNAL_KEY
screen=$(mktemp)

step() { printf '\n== %s\n' "$*"; }
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    "${compose[@]}" ps -a || true
    "${compose[@]}" logs --no-color --tail 80 || true
  fi
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  rm -f "$screen"
  exit "$status"
}
trap cleanup EXIT

step "start project $project on port $port (backend healthy, then the gateway)"
# shellcheck disable=SC2086 # COMPOSE_UP_ARGS is a list of flags
"${compose[@]}" up -d ${COMPOSE_UP_ARGS:---no-build} --wait --wait-timeout 180
"${compose[@]}" ps

step "the gateway answers with its SSH banner"
banner=
for _ in $(seq 1 30); do
  banner=$(timeout 5 bash -c "exec 3<>/dev/tcp/127.0.0.1/$port && head -n 1 <&3" 2>/dev/null | tr -d '\r') || true
  [[ $banner == SSH-2.0-ROAST_TTY* ]] && break
  sleep 1
done
echo "banner: $banner"
[[ $banner == SSH-2.0-ROAST_TTY* ]]

step "the backend is private and wants the internal key"
if "${compose[@]}" port backend 3000 >/dev/null 2>&1; then
  echo "the backend's port is published" >&2
  exit 1
fi
"${compose[@]}" exec -T backend node -e "
  fetch('http://127.0.0.1:3000/v1/products').then(r => {
    console.log('without the key:', r.status)
    process.exit(r.status === 401 ? 0 : 1)
  })"

step "a stock ssh session (no key) shows the products from the backend"
# A new connection opens on the login page with the focus in 用户名: Esc leaves the field,
# then `g` browses as a guest. The pause keeps Esc and g from reading as Alt+g.
python3 scripts/pty-run.py --size 160x50 --timeout 60 \
  --step 'until=用户名' --step 'wait=1' --step 'key=esc' --step 'wait=1' --step 'text=g' \
  --step 'until=耶加雪菲 孔加' --expect '演示环境' \
  -- ssh -p "$port" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -o PubkeyAuthentication=no -o IdentitiesOnly=yes guest@127.0.0.1 \
  | tee "$screen"
if grep -q '样例数据' "$screen"; then
  echo "the session shows the built-in sample data, not the backend's" >&2
  exit 1
fi
"${compose[@]}" logs --no-color gateway | grep -E 'session (started|ended)'

step "gallery@ is routed to the UI gallery (deploy/gateway.toml), and the log says so"
python3 scripts/pty-run.py --size 160x50 --timeout 60 \
  --step 'until=UI 组件库 GALLERY' --expect '演示环境' \
  -- ssh -p "$port" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -o PubkeyAuthentication=no -o IdentitiesOnly=yes gallery@127.0.0.1 \
  | tee "$screen"
"${compose[@]}" logs --no-color gateway | grep -E 'session started.*user="gallery".*route=gallery'

step "a stock ssh session with a key buys: register → add → address → checkout → receipt"
python3 scripts/e2e/run.py --gateway "127.0.0.1:$port" --out .data/e2e-smoke smoke
grep -h -E '订单号|实付|演示订单' .data/e2e-smoke/smoke/*-receipt.txt | sed -E 's/[[:space:]]+/ /g'

step "the sessions' coffee-tui are gone after the clients left"
gone=
for _ in $(seq 1 10); do
  if ! "${compose[@]}" top gateway | grep -q coffee-tui; then
    gone=yes
    break
  fi
  sleep 0.5
done
"${compose[@]}" top gateway
[ -n "$gone" ]

step "a restart reuses the database (nothing migrated or seeded twice)"
"${compose[@]}" restart backend
"${compose[@]}" up -d --no-build --wait --wait-timeout 120 backend
"${compose[@]}" logs --no-color backend | tee /dev/stderr | grep -q 'No pending migrations to apply'
"${compose[@]}" logs --no-color backend | grep -q 'catalog: 0 created'

step "ok"
