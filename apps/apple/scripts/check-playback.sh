#!/usr/bin/env bash
# Checks the Apple apps' playback against a real Pley server: builds the
# server and its transcoder, makes the test media (make-test-media.sh, or
# PLEY_CHECK_MEDIA if set), starts a disposable server (a temporary data
# folder, a free port on 127.0.0.1, no Bonjour, no Plex port, no tray, the
# managed transcoder working in the same temporary folder), makes an
# administrator, and runs pley-apple-play, which plays the media through
# the apps' player. The server, the transcoder and the temporary folder are
# removed afterwards, whatever happens. macOS only.
#
# The administrator's password is PLEY_CHECK_PASSWORD, or a random one.
# The server reads the web app from apps/web/dist: build it first
# (bun run build), or link it there in a worktree.
set -euo pipefail

root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

if [[ ! -e apps/web/dist/index.html ]]; then
  echo "check-playback: apps/web/dist is missing: build the web app first (bun run build)" >&2
  exit 1
fi

PLEY_CHECK_PASSWORD=${PLEY_CHECK_PASSWORD:-$(openssl rand -hex 16)}
export PLEY_CHECK_PASSWORD

temp=$(mktemp -d)
data="$temp/data"
server=""
cleanup() {
  if [[ -n "$server" ]]; then
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  rm -rf "$temp"
}
trap cleanup EXIT

# The server's log as the check prints it: keyed media addresses and
# anything that looks like a token or a code left out.
redacted() {
  sed -E -e 's/pley_mk_[A-Za-z0-9_-]+/pley_mk_(key)/g' \
    -e 's/((token|code|password|secret|authorization|cookie)[^A-Za-z0-9]{1,3})[A-Za-z0-9._~+\/=-]{8,}/\1(left out)/Ig' \
    -e 's/Bearer [A-Za-z0-9._~+\/=-]+/Bearer (left out)/g'
}

media=${PLEY_CHECK_MEDIA:-"$temp/media"}
mkdir -p "$data" "$media"
apps/apple/scripts/make-test-media.sh "$media"

cargo build -p pley-server -p pley-transcoder
swift build --package-path apps/apple/PleyKit --product pley-apple-play

port=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"

target/debug/pley --data-dir "$data" reset-password checkadmin "$PLEY_CHECK_PASSWORD" --admin >/dev/null
PLEY_TRANSCODER_WORK_DIR="$temp/transcoder" \
  target/debug/pley --data-dir "$data" --bind "127.0.0.1:$port" --plex-bind off --no-discovery --no-tray \
  >"$temp/server.log" 2>&1 &
server=$!

for _ in $(seq 1 100); do
  if curl -fsS "$origin/api/health" >/dev/null 2>&1; then break; fi
  if ! kill -0 "$server" 2>/dev/null; then
    echo "check-playback: the server stopped:" >&2
    tail -n 20 "$temp/server.log" | redacted >&2
    exit 1
  fi
  sleep 0.2
done
curl -fsS "$origin/api/health" >/dev/null

# The player's picture subtitles go to the temporary folder, never to the
# user's Caches; the check says so if the run left a folder there.
caches="$HOME/Library/Caches/pley/picture-subtitles"
cached_before=no
if [[ -e "$caches" ]]; then cached_before=yes; fi

if ! swift run --skip-build --package-path apps/apple/PleyKit pley-apple-play \
  --server "$origin" --admin checkadmin --media "$media" --caps-fixtures apps/server/tests/fixtures/caps \
  --work-dir "$temp/player"; then
  echo "check-playback: the server's last lines:" >&2
  tail -n 30 "$temp/server.log" | redacted >&2
  exit 1
fi
if grep -q "pley_mk_" "$temp/server.log"; then
  echo "check-playback: the server's log names a keyed media address" >&2
  exit 1
fi
if [[ $cached_before == no && -e "$caches" ]]; then
  echo "check-playback: the run left $caches behind" >&2
  exit 1
fi
if [[ -e "$temp/player/picture-subtitles" ]]; then
  echo "check-playback: the player left its picture-subtitles folder behind" >&2
  exit 1
fi
echo "check-playback: passed"
