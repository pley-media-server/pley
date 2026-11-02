#!/usr/bin/env bash
# Checks the Apple apps' screens on the simulators against a real Pley
# server: builds the server and its transcoder, makes the screens' test
# media (make-test-media.sh --screens, or PLEY_CHECK_MEDIA if set), starts
# a disposable server (a temporary data folder, a free port on 127.0.0.1,
# no Bonjour, no Plex port, no tray), and sets up its household: an
# administrator ("Admin", with a PIN) and the managed profile Kids (with a
# PIN), the libraries Movies, Shows, Clips and Music, and once they are
# scanned the collection "Test collection" of two movies, and Live TV's
# source "Test channels" (live-source.sh: four made-up channels and their
# guide on another free port, one connection at a time). Then it runs the
# UI tests (PleyUITests on each iPhone and iPad simulator, PleyTVUITests on
# the Apple TV one): sign in with a code, "Who's watching?", Home, an item,
# a library, a collection, a playlist made, changed and deleted, Me
# (Settings on Apple TV), a switch back by PIN, a search, the profile's
# track defaults, History, Settings, Diagnostics and About, on Apple TV
# Always start as across a relaunch, and Sign out. The server
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
#   PLEY_UI_TESTS      only these tests of ScreensTests, separated by ";"
#                      (testLiveTv;testSignIn…); all of them when empty
#
# Each simulator is booted afresh, the app removed from it and its
# Keychain emptied, so the walk starts at the servers; a lock per simulator
# keeps two runs off the same one. A failed test is run once more, in a new
# test runner: a walk stops at its first failure, which ends the runner.
# The server reads the web app from apps/web/dist: build it first (bun run
# build), or link it there in a worktree.
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
live=""
# The simulators this run holds: a lock directory each, shared by every run.
locks=()
lockdir="${TMPDIR:-/tmp}/pley-check-screens"
cleanup() {
  if [[ -n "$server" ]]; then
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  # The live source stops its FFmpegs and its web server with it.
  if [[ -n "$live" ]]; then
    kill "$live" 2>/dev/null || true
    wait "$live" 2>/dev/null || true
  fi
  for lock in ${locks[@]+"${locks[@]}"}; do rm -rf "$lock"; done
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

# A collection of the first two movies, for the collection's walk.
movies=$(admin GET /api/libraries |
  python3 -c 'import json,sys;print(next(l["id"] for l in json.load(sys.stdin) if l["name"]=="Movies"))')
two=$(admin GET "/api/libraries/$movies/items?limit=2" |
  python3 -c 'import json,sys;print(",".join(str(i["id"]) for i in json.load(sys.stdin)["items"][:2]))')
admin POST "/api/libraries/$movies/collections" "{\"title\":\"Test collection\",\"item_ids\":[$two]}" >/dev/null

# Live TV: a made-up provider on 127.0.0.1, with one connection, so a
# second channel needs Watch here instead.
live_port=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
apps/apple/scripts/live-source.sh "$temp/live" "$live_port" >"$temp/live.log" 2>&1 &
live=$!
for _ in $(seq 1 60); do
  if grep -q ready "$temp/live.log" 2>/dev/null; then break; fi
  sleep 1
done
live_base="http://127.0.0.1:$live_port"
admin POST /api/live/sources "{\"name\":\"Test channels\",\"kind\":\"m3u\",\"url\":\"$live_base/list.m3u\",\"guide_url\":\"$live_base/guide.xml\",\"max_connections\":1}" >/dev/null
available=no
for _ in $(seq 1 60); do
  if admin GET /api/live | python3 -c 'import json,sys;d=json.load(sys.stdin);sys.exit(not (d.get("available") and d.get("channels",0)>=4))'; then
    available=yes
    break
  fi
  sleep 1
done
if [[ $available == no ]]; then
  echo "check-screens: Live TV's channels didn't come" >&2
  exit 1
fi

# The simulator's UDID by its name.
udid() {
  xcrun simctl list devices available -j |
    python3 -c 'import json,sys;n=sys.argv[1];print(next(d["udid"] for r in json.load(sys.stdin)["devices"].values() for d in r if d["name"]==n))' "$1"
}

# The token and the PINs go to the tests in a file only this user reads,
# not in the environment, which any process can see (ps eww). It goes with
# the temporary folder.
secrets="$temp/secrets.json"
(umask 077 && TOKEN="$token" ADMIN_PIN="$admin_pin" KIDS_PIN="$kids_pin" python3 -c '
import json, os
print(json.dumps({"token": os.environ["TOKEN"], "admin_pin": os.environ["ADMIN_PIN"], "kids_pin": os.environ["KIDS_PIN"]}))
' >"$secrets")

# xcodebuild hands TEST_RUNNER_ variables to the tests, without the prefix.
export TEST_RUNNER_PLEY_ORIGIN="$origin"
export TEST_RUNNER_PLEY_SECRETS_FILE="$secrets"
export TEST_RUNNER_PLEY_ADMIN_NAME="Admin"
export TEST_RUNNER_PLEY_SHOTS="$shots"

# Holds simulator $1 for this run, or says who has it.
take() {
  mkdir -p "$lockdir"
  local lock="$lockdir/$1"
  if mkdir "$lock" 2>/dev/null; then
    echo $$ >"$lock/pid"
    locks+=("$lock")
    return 0
  fi
  # A run that died leaves its lock: taken over when its process is gone.
  local holder
  holder=$(cat "$lock/pid" 2>/dev/null || true)
  if [[ -n "$holder" ]] && ! kill -0 "$holder" 2>/dev/null; then
    rm -rf "$lock"
    take "$1"
    return
  fi
  return 1
}

failed=()
walk() {
  local scheme=$1 name=$2 id log
  if ! id=$(udid "$name"); then
    echo "check-screens: no simulator named $name" >&2
    failed+=("$name")
    return
  fi
  if ! take "$id"; then
    echo "check-screens: $name is in use by another check-screens run" >&2
    failed+=("$name")
    return
  fi
  # Booted afresh: a simulator up for long, or still starting, kills the
  # test runner before it connects.
  xcrun simctl shutdown "$id" 2>/dev/null || true
  xcrun simctl boot "$id" 2>/dev/null || true
  xcrun simctl bootstatus "$id" -b >/dev/null
  xcrun simctl uninstall "$id" "$bundle" 2>/dev/null || true
  # The server list and sign-ins are in the Keychain, which outlives the app.
  xcrun simctl keychain "$id" reset 2>/dev/null || true
  xcrun simctl bootstatus "$id" -b >/dev/null
  log="$temp/$scheme-$id.log"
  echo "check-screens: $scheme on $name"
  local only=()
  if [[ -n "${PLEY_UI_TESTS:-}" ]]; then
    local target=PleyUITests test wanted
    [[ $scheme == PleyTVUITests ]] && target=PleyTVUITests
    IFS=';' read -r -a wanted <<<"$PLEY_UI_TESTS"
    for test in "${wanted[@]}"; do
      [[ -n "$test" ]] && only+=("-only-testing:$target/ScreensTests/$test")
    done
  fi
  if xcodebuild test -project apps/apple/Pley.xcodeproj -scheme "$scheme" -destination "id=$id" \
    ${only[@]+"${only[@]}"} -retry-tests-on-failure -test-iterations 2 -test-repetition-relaunch-enabled YES \
    -resultBundlePath "$temp/$scheme-$id.xcresult" >"$log" 2>&1; then
    echo "check-screens: $name passed"
  else
    echo "check-screens: $name failed:" >&2
    grep -E "error:|failed|XCTAssert|didn't show|couldn't" "$log" | tail -n 20 | redacted >&2 || true
    echo "check-screens: the server's last lines:" >&2
    tail -n 40 "$temp/server.log" | redacted >&2
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
