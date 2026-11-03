#!/usr/bin/env bash
# Checks the Apple apps' sign-in against a real Pley server: builds the
# server, starts a disposable one (a temporary data folder, a free port on
# 127.0.0.1, no Bonjour, no tray), makes an administrator, and
# runs pley-apple-check as Apple TV and as iPhone. The server and its folder
# are removed afterwards, whatever happens. macOS only.
#
# The administrator's password is PLEY_CHECK_PASSWORD, or a random one.
# The server reads the web app from apps/web/dist: build it first
# (bun run build), or link it there in a worktree.
set -euo pipefail

root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

if [[ ! -e apps/web/dist/index.html ]]; then
  echo "check-server: apps/web/dist is missing: build the web app first (bun run build)" >&2
  exit 1
fi

PLEY_CHECK_PASSWORD=${PLEY_CHECK_PASSWORD:-$(openssl rand -hex 16)}
export PLEY_CHECK_PASSWORD

data=$(mktemp -d)
server=""
cleanup() {
  if [[ -n "$server" ]]; then
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  rm -rf "$data"
}
trap cleanup EXIT

port=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"

cargo build -p pley-server
target/debug/pley --data-dir "$data" reset-password checkadmin "$PLEY_CHECK_PASSWORD" --admin >/dev/null
target/debug/pley --data-dir "$data" --bind "127.0.0.1:$port" --no-discovery --no-tray \
  >"$data/server.log" 2>&1 &
server=$!

for _ in $(seq 1 100); do
  if curl -fsS "$origin/api/health" >/dev/null 2>&1; then break; fi
  if ! kill -0 "$server" 2>/dev/null; then
    echo "check-server: the server stopped:" >&2
    tail -n 20 "$data/server.log" >&2
    exit 1
  fi
  sleep 0.2
done
curl -fsS "$origin/api/health" >/dev/null

swift build --package-path apps/apple/PleyKit --product pley-apple-check
for client in tvos ios; do
  swift run --skip-build --package-path apps/apple/PleyKit pley-apple-check \
    --server "$origin" --client "$client" --admin checkadmin
done
echo "check-server: passed"
