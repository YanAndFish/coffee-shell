#!/usr/bin/env bash
# Smoke test of deploy/compose.yaml (CI's `docker` job runs it; it works by hand too):
#
#   COFFEE_GATEWAY_IMAGE=roast-tty-ssh:ci COFFEE_SERVER_IMAGE=roast-tty-server:ci \
#     deploy/smoke-test.sh            # images built beforehand
#   COMPOSE_UP_ARGS=--build deploy/smoke-test.sh
#
# Starts the pair with a throwaway COFFEE_INTERNAL_KEY and checks: the gateway answers
# with its SSH banner; the backend is not published and refuses calls without the key; a
# stock `ssh` session (no key) shows the product list served by the backend, not the
# built-in sample data; the session's coffee-tui is gone after the client leaves; a
# restarted backend starts again on the same database without migrating or seeding twice.
# Everything (containers, networks, volumes) is removed at the end.
set -euo pipefail
cd "$(dirname "$0")/.."

compose=(docker compose -f deploy/compose.yaml)
port=${COFFEE_SSH_PORT:-2222}
COFFEE_INTERNAL_KEY=$(openssl rand -hex 32)
export COFFEE_INTERNAL_KEY
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

step "start (backend healthy, then the gateway)"
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

step "the session's coffee-tui is gone after the client left"
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
