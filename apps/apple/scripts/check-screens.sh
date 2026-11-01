#!/usr/bin/env bash
# Checks the Apple apps' screens on the simulators against a real Pley
# server: builds the server and its transcoder, makes the screens' test
# media (make-test-media.sh --screens, or PLEY_CHECK_MEDIA if set), starts
# a disposable server (a temporary data folder, a free port on 127.0.0.1,
# no Bonjour, no Plex port, no tray), and sets up its household: an
# administrator ("Admin", with a PIN) and the managed profile Kids (with a
# PIN), and the libraries Movies, Shows, Clips and Music. Then it runs the
# UI tests (PleyUITests on each iPhone and iPad simulator, PleyTVUITests on
# the Apple TV one): sign in with a code, "Who's watching?", Home, an item,
# Me (Settings on Apple TV), a switch back by PIN, and Sign out. The server
# and the temporary folder are removed afterwards, whatever happens. macOS
# only.
#
#   PLEY_IOS_SIMS      the iPhone and iPad simulators, separated by ";"
#                      (default "iPhone 17;iPad (A16)")
#   PLEY_TV_SIM        the Apple TV simulator (default "Apple TV 4K (4th generation)");
#                      empty skips Apple TV, as an empty PLEY_IOS_SIMS skips the others
#   PLEY_SHOTS         a folder for the walk's screenshots
#   PLEY_SCREENS_LOGS  a folder for the test logs of the simulators that failed
#   PLEY_BUNDLE_ID     the app's bundle ID, when Local.xcconfig changes it
#
# The app is removed from each simulator first, so the walk starts at the
# servers. The server reads the web app from apps/web/dist: build it first
# (bun run build), or link it there in a worktree.
set -euo pipefail

root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

if [[ ! -e apps/web/dist/index.html ]]; then
  echo "check-screens: apps/web/dist is missing: build the web app first (bun run build)" >&2
  exit 1
fi

ios_sims=${PLEY_IOS_SIMS-"iPhone 17;iPad (A16)"}
tv_sim=${PLEY_TV_SIM-"Apple TV 4K (4th generation)"}
bundle=${PLEY_BUNDLE_ID:-com.formandelement.pley}
shots=${PLEY_SHOTS:-}
if [[ -n "$shots" ]]; then
  mkdir -p "$shots"
  shots=$(cd "$shots" && pwd)
fi
logs=${PLEY_SCREENS_LOGS:-}
if [[ -n "$logs" ]]; then
  mkdir -p "$logs"
fi

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
    -e 's/((token|code|password|secret|authorization|cookie|pin)[^A-Za-z0-9]{1,3})[A-Za-z0-9._~+\/=-]{4,}/\1(left out)/Ig' \
    -e 's/Bearer [A-Za-z0-9._~+\/=-]+/Bearer (left out)/g'
}

media=${PLEY_CHECK_MEDIA:-"$temp/media"}
mkdir -p "$data" "$media"
apps/apple/scripts/make-test-media.sh --screens "$media"

cargo build -p pley-server -p pley-transcoder

port=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"
password=$(openssl rand -hex 16)
# PINs of four random digits.
pin() { python3 -c 'import secrets;print(f"{secrets.randbelow(10000):04d}")'; }
admin_pin=$(pin)
kids_pin=$(pin)
while [[ "$kids_pin" == "$admin_pin" ]]; do kids_pin=$(pin); done

target/debug/pley --data-dir "$data" reset-password checkadmin "$password" --admin >/dev/null
PLEY_TRANSCODER_WORK_DIR="$temp/transcoder" \
  target/debug/pley --data-dir "$data" --bind "127.0.0.1:$port" --plex-bind off --no-discovery --no-tray \
  >"$temp/server.log" 2>&1 &
server=$!

for _ in $(seq 1 100); do
  if curl -fsS "$origin/api/health" >/dev/null 2>&1; then break; fi
  if ! kill -0 "$server" 2>/dev/null; then
    echo "check-screens: the server stopped:" >&2
    tail -n 20 "$temp/server.log" | redacted >&2
    exit 1
  fi
  sleep 0.2
done
curl -fsS "$origin/api/health" >/dev/null

token=$(curl -fsS -X POST "$origin/api/auth/login" -H 'Content-Type: application/json' \
  -d "{\"username\":\"checkadmin\",\"password\":\"$password\"}" |
  python3 -c 'import json,sys;print(json.load(sys.stdin)["token"])')
# The administrator's requests, as the web app sends them.
admin() {
  curl -fsS -X "$1" "$origin$2" -H 'Content-Type: application/json' -H "Cookie: pley_token=$token" \
    -H "Origin: $origin" -H 'Sec-Fetch-Site: same-origin' ${3:+-d "$3"}
}

admin PATCH /api/me '{"display_name":"Admin"}' >/dev/null
admin PUT /api/me/pin "{\"pin\":\"$admin_pin\",\"password\":\"$password\"}" >/dev/null
admin POST /api/users "{\"username\":\"kids\",\"display_name\":\"Kids\",\"managed\":true,\"pin\":\"$kids_pin\"}" \
  >/dev/null
for library in "Movies movies Movies" "Shows shows Shows" "Clips movies Clips" "Music music Music"; do
  read -r name kind folder <<<"$library"
  admin POST /api/libraries "{\"name\":\"$name\",\"kind\":\"$kind\",\"paths\":[\"$media/$folder\"]}" >/dev/null
done

# The scans, done: no library is scanning, and Home has rows.
scanned=no
for _ in $(seq 1 240); do
  if admin GET /api/libraries | python3 -c 'import json,sys;sys.exit(any(l.get("scanning") for l in json.load(sys.stdin)))' \
    && admin GET /api/hubs/home | python3 -c 'import json,sys;sys.exit(not json.load(sys.stdin)["hubs"])'; then
    scanned=yes
    break
  fi
  sleep 0.5
done
if [[ $scanned == no ]]; then
  echo "check-screens: the libraries didn't finish scanning" >&2
  exit 1
fi

# The simulator's UDID by its name.
udid() {
  xcrun simctl list devices available -j |
    python3 -c 'import json,sys;n=sys.argv[1];print(next(d["udid"] for r in json.load(sys.stdin)["devices"].values() for d in r if d["name"]==n))' "$1"
}

# xcodebuild hands TEST_RUNNER_ variables to the tests, without the prefix.
export TEST_RUNNER_PLEY_ORIGIN="$origin"
export TEST_RUNNER_PLEY_ADMIN_TOKEN="$token"
export TEST_RUNNER_PLEY_ADMIN_NAME="Admin"
export TEST_RUNNER_PLEY_ADMIN_PIN="$admin_pin"
export TEST_RUNNER_PLEY_KIDS_PIN="$kids_pin"
export TEST_RUNNER_PLEY_SHOTS="$shots"

failed=()
walk() {
  local scheme=$1 name=$2 id log
  if ! id=$(udid "$name"); then
    echo "check-screens: no simulator named $name" >&2
    failed+=("$name")
    return
  fi
  xcrun simctl boot "$id" 2>/dev/null || true
  xcrun simctl uninstall "$id" "$bundle" 2>/dev/null || true
  log="$temp/$scheme-$id.log"
  echo "check-screens: $scheme on $name"
  if xcodebuild test -project apps/apple/Pley.xcodeproj -scheme "$scheme" -destination "id=$id" \
    -resultBundlePath "$temp/$scheme-$id.xcresult" >"$log" 2>&1; then
    echo "check-screens: $name passed"
  else
    echo "check-screens: $name failed:" >&2
    grep -E "error:|failed|XCTAssert|didn't show|couldn't" "$log" | tail -n 20 | redacted >&2 || true
    if [[ -n "$logs" ]]; then redacted <"$log" >"$logs/$scheme-$name.log"; fi
    failed+=("$name")
  fi
}

if [[ -n "$ios_sims" ]]; then
  IFS=';' read -r -a phones <<<"$ios_sims"
  for name in "${phones[@]}"; do
    if [[ -n "$name" ]]; then walk PleyUITests "$name"; fi
  done
fi
if [[ -n "$tv_sim" ]]; then walk PleyTVUITests "$tv_sim"; fi

if grep -q "pley_mk_" "$temp/server.log"; then
  echo "check-screens: the server's log names a keyed media address" >&2
  exit 1
fi
if [[ ${#failed[@]} -gt 0 ]]; then
  echo "check-screens: failed on ${failed[*]}" >&2
  exit 1
fi
echo "check-screens: passed"
