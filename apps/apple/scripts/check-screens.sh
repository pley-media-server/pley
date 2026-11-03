#!/usr/bin/env bash
# Checks the Apple apps' screens on the simulators against real Pley
# servers: builds the server and its transcoder, makes the screens' test
# media (make-test-media.sh --screens, or PLEY_CHECK_MEDIA if set), and
# builds the UI tests once. Then each simulator, all at the same time, gets
# a disposable server of its own (a data folder, a free port on 127.0.0.1,
# no Bonjour, no tray), with its household: an administrator ("Admin",
# with a PIN) and the managed profile Kids (with a PIN), the libraries
# Movies, Shows, Clips and Music, and once they are scanned the collection
# "Test collection" of two movies, and Live TV's source "Test channels"
# (live-source.sh: four made-up channels and their guide on another free
# port, one connection at a time). The UI tests run on it (PleyUITests on
# each iPhone and iPad simulator, PleyTVUITests on the Apple TV one): sign
# in with a code, "Who's watching?", Home, an item, a library, a
# collection, a playlist made, changed and deleted, Me (Settings on Apple
# TV), a switch back by PIN, a search, the profile's track defaults,
# History, Settings, Diagnostics and About, on Apple TV Always start as
# across a relaunch, and Sign out; and music, Live TV, the player's extras
# and the largest text size, each a test of its own. The servers and their
# folders are removed afterwards, whatever happens. macOS only.
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
#   PLEY_SCREENS_QUICK 1 keeps each simulator's server, its media and the
#                      booted simulator for the next quick run, and runs a
#                      failed test once only: for working on a test or a
#                      screen. stop stops the kept servers.
#
# Each simulator is booted afresh, the app removed from it and its
# Keychain emptied, so the first test starts at the servers; the tests
# after it keep its sign-in. A lock per simulator keeps two runs off the
# same one. A failed test is run once more, in a new test runner: a walk
# stops at its first failure, which ends the runner. A quick run boots a
# simulator only when it isn't, and empties it only when its server is
# new; it starts the server again when the server's binary changed.
# The server reads the web app from apps/web/dist: build it first (bun run
# build), or link it there in a worktree.
set -euo pipefail

root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

ios_sims=${PLEY_IOS_SIMS-"iPhone 17;iPad (A16)"}
tv_sim=${PLEY_TV_SIM-"Apple TV 4K (4th generation)"}
bundle=${PLEY_BUNDLE_ID:-com.formandelement.pley}
quick=${PLEY_SCREENS_QUICK:-}
# The quick runs' servers and media, kept between runs.
kept="${TMPDIR:-/tmp}/pley-check-screens-kept"
# The simulators held by a run: a lock directory each, shared by every run.
lockdir="${TMPDIR:-/tmp}/pley-check-screens"

# Stops the server and the live source kept in folder $1, and waits for
# them (and the live source's FFmpegs) to finish writing there.
stop_kept() {
  local file pids=()
  for file in "$1/server.pid" "$1/live.pid"; do
    if [[ -f "$file" ]]; then pids+=("$(cat "$file")"); fi
  done
  for pid in ${pids[@]+"${pids[@]}"}; do kill "$pid" 2>/dev/null || true; done
  for _ in $(seq 1 50); do
    local running=no
    for pid in ${pids[@]+"${pids[@]}"}; do
      if kill -0 "$pid" 2>/dev/null; then running=yes; fi
    done
    [[ $running == no ]] && break
    sleep 0.2
  done
}

if [[ $quick == stop ]]; then
  for dir in "$kept"/*/; do
    if [[ -f "$dir/server.pid" ]]; then stop_kept "$dir"; fi
  done
  rm -rf "$kept"
  echo "check-screens: the kept servers are stopped"
  exit 0
fi

if [[ ! -e apps/web/dist/index.html ]]; then
  echo "check-screens: apps/web/dist is missing: build the web app first (bun run build)" >&2
  exit 1
fi

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
# The simulators' walks, running.
walkers=()
cleanup() {
  for pid in ${walkers[@]+"${walkers[@]}"}; do kill "$pid" 2>/dev/null || true; done
  for pid in ${walkers[@]+"${walkers[@]}"}; do wait "$pid" 2>/dev/null || true; done
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

free_port() {
  python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'
}

# PINs of four random digits.
pin() { python3 -c 'import secrets;print(f"{secrets.randbelow(10000):04d}")'; }

# The media, made once; a quick run keeps it, made aside and moved in
# whole, so a run that stopped part way leaves none.
if [[ -n "${PLEY_CHECK_MEDIA:-}" ]]; then
  media=$PLEY_CHECK_MEDIA
  apps/apple/scripts/make-test-media.sh --screens "$media"
elif [[ -n "$quick" ]]; then
  media="$kept/media"
  if [[ ! -d "$media" ]]; then
    mkdir -p "$kept"
    apps/apple/scripts/make-test-media.sh --screens "$temp/media"
    mv "$temp/media" "$media" 2>/dev/null || rm -rf "${temp:?}/media"
  fi
else
  media="$temp/media"
  apps/apple/scripts/make-test-media.sh --screens "$media"
fi

cargo build -p pley-server -p pley-transcoder
binary="$root/target/debug/pley"

# The UI tests, built once for every simulator of their platform; each
# walk then runs them without building.
build_tests() {
  local scheme=$1 platform=$2
  echo "check-screens: building $scheme"
  if ! xcodebuild build-for-testing -project apps/apple/Pley.xcodeproj -scheme "$scheme" \
    -destination "generic/platform=$platform" >"$temp/build-$scheme.log" 2>&1; then
    echo "check-screens: $scheme didn't build:" >&2
    grep -E "error:" "$temp/build-$scheme.log" | head -n 20 >&2 || tail -n 20 "$temp/build-$scheme.log" >&2
    exit 1
  fi
}
if [[ -n "$ios_sims" ]]; then build_tests PleyUITests "iOS Simulator"; fi
if [[ -n "$tv_sim" ]]; then build_tests PleyTVUITests "tvOS Simulator"; fi

# The simulator's UDID by its name.
udid() {
  xcrun simctl list devices available -j |
    python3 -c 'import json,sys;n=sys.argv[1];print(next(d["udid"] for r in json.load(sys.stdin)["devices"].values() for d in r if d["name"]==n))' "$1"
}

# --- One simulator's walk, in a subshell of its own -------------------

# Each walk's own: its folder, its simulator's lock, its processes.
dir=""
lock=""
xcode=""
origin=""
token=""

# Starts the server on the walk's folder and port, and waits for it.
start_server() {
  PLEY_TRANSCODER_WORK_DIR="$dir/transcoder" \
    "$binary" --data-dir "$dir/data" --bind "127.0.0.1:$(cat "$dir/port")" --no-discovery --no-tray \
    </dev/null >>"$dir/server.log" 2>&1 &
  echo $! >"$dir/server.pid"
  stat -f %m "$binary" >"$dir/server.stamp"
  for _ in $(seq 1 100); do
    if curl -fsS "$origin/api/health" >/dev/null 2>&1; then return 0; fi
    if ! kill -0 "$(cat "$dir/server.pid")" 2>/dev/null; then break; fi
    sleep 0.2
  done
  if curl -fsS "$origin/api/health" >/dev/null 2>&1; then return 0; fi
  echo "check-screens: the server stopped:" >&2
  tail -n 20 "$dir/server.log" | redacted >&2
  return 1
}

# Starts live-source.sh on the walk's live port, and waits for its channels.
start_live() {
  : >"$dir/live.log"
  apps/apple/scripts/live-source.sh "$dir/live" "$(cat "$dir/live_port")" </dev/null >"$dir/live.log" 2>&1 &
  echo $! >"$dir/live.pid"
  for _ in $(seq 1 60); do
    if grep -q ready "$dir/live.log" 2>/dev/null; then return 0; fi
    sleep 1
  done
}

# The administrator's requests, as the web app sends them.
admin() {
  curl -fsS -X "$1" "$origin$2" -H 'Content-Type: application/json' -H "Cookie: pley_token=$token" \
    -H "Origin: $origin" -H 'Sec-Fetch-Site: same-origin' ${3:+-d "$3"}
}

# Signs the administrator in, and writes the token and the PINs for the
# tests in a file only this user reads, not in the environment, which any
# process can see (ps eww).
log_in() {
  # shellcheck disable=SC1091
  source "$dir/household"
  token=$(curl -fsS -X POST "$origin/api/auth/login" -H 'Content-Type: application/json' \
    -d "{\"username\":\"checkadmin\",\"password\":\"$password\"}" |
    python3 -c 'import json,sys;print(json.load(sys.stdin)["token"])')
  (umask 077 && TOKEN="$token" ADMIN_PIN="$admin_pin" KIDS_PIN="$kids_pin" python3 -c '
import json, os
print(json.dumps({"token": os.environ["TOKEN"], "admin_pin": os.environ["ADMIN_PIN"], "kids_pin": os.environ["KIDS_PIN"]}))
' >"$dir/secrets.json")
}

# A new server in the walk's folder, with its household, libraries and Live TV.
new_server() {
  mkdir -p "$dir/data"
  free_port >"$dir/port"
  free_port >"$dir/live_port"
  origin="http://127.0.0.1:$(cat "$dir/port")"
  local password admin_pin kids_pin
  password=$(openssl rand -hex 16)
  admin_pin=$(pin)
  kids_pin=$(pin)
  while [[ "$kids_pin" == "$admin_pin" ]]; do kids_pin=$(pin); done
  (umask 077 && printf 'password=%s\nadmin_pin=%s\nkids_pin=%s\n' "$password" "$admin_pin" "$kids_pin" \
    >"$dir/household")

  "$binary" --data-dir "$dir/data" reset-password checkadmin "$password" --admin >/dev/null
  start_server
  log_in

  admin PATCH /api/me '{"display_name":"Admin"}' >/dev/null
  admin PUT /api/me/pin "{\"pin\":\"$admin_pin\",\"password\":\"$password\"}" >/dev/null
  admin POST /api/users "{\"username\":\"kids\",\"display_name\":\"Kids\",\"managed\":true,\"pin\":\"$kids_pin\"}" \
    >/dev/null
  # Its own names: the walk's $name is the simulator's.
  local library name kind folder
  for library in "Movies movies Movies" "Shows shows Shows" "Clips movies Clips" "Music music Music"; do
    read -r name kind folder <<<"$library"
    admin POST /api/libraries "{\"name\":\"$name\",\"kind\":\"$kind\",\"paths\":[\"$media/$folder\"]}" >/dev/null
  done

  # The scans, done: no library is scanning, and Home has rows.
  local scanned=no
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
    return 1
  fi

  # A collection of the first two movies, for the collection's walk.
  local movies two
  movies=$(admin GET /api/libraries |
    python3 -c 'import json,sys;print(next(l["id"] for l in json.load(sys.stdin) if l["name"]=="Movies"))')
  two=$(admin GET "/api/libraries/$movies/items?limit=2" |
    python3 -c 'import json,sys;print(",".join(str(i["id"]) for i in json.load(sys.stdin)["items"][:2]))')
  admin POST "/api/libraries/$movies/collections" "{\"title\":\"Test collection\",\"item_ids\":[$two]}" >/dev/null

  # Live TV: a made-up provider on 127.0.0.1, with one connection, so a
  # second channel needs Watch here instead.
  start_live
  local live_base
  live_base="http://127.0.0.1:$(cat "$dir/live_port")"
  admin POST /api/live/sources "{\"name\":\"Test channels\",\"kind\":\"m3u\",\"url\":\"$live_base/list.m3u\",\"guide_url\":\"$live_base/guide.xml\",\"max_connections\":1}" >/dev/null
  local available=no
  for _ in $(seq 1 60); do
    if admin GET /api/live | python3 -c 'import json,sys;d=json.load(sys.stdin);sys.exit(not (d.get("available") and d.get("channels",0)>=4))'; then
      available=yes
      break
    fi
    sleep 1
  done
  if [[ $available == no ]]; then
    echo "check-screens: Live TV's channels didn't come" >&2
    return 1
  fi
  touch "$dir/ready"
}

# The server a quick run kept in the walk's folder, started again if its
# binary changed or it stopped; false when there is none to keep.
kept_server() {
  [[ -f "$dir/ready" && -f "$dir/port" ]] || return 1
  origin="http://127.0.0.1:$(cat "$dir/port")"
  local pid
  pid=$(cat "$dir/server.pid" 2>/dev/null || true)
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && [[ "$(cat "$dir/server.stamp")" == "$(stat -f %m "$binary")" ]] \
    && curl -fsS "$origin/api/health" >/dev/null 2>&1; then
    :
  else
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
      while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
    fi
    echo "check-screens: starting the kept server again"
    start_server || return 1
  fi
  if ! kill -0 "$(cat "$dir/live.pid" 2>/dev/null || echo 0)" 2>/dev/null; then start_live; fi
  log_in
}

# Holds simulator $1 for this run, or says who has it.
take() {
  mkdir -p "$lockdir"
  if mkdir "$lockdir/$1" 2>/dev/null; then
    # The walk's own process (bash 3.2 has no BASHPID): sh's parent.
    sh -c 'echo $PPID' >"$lockdir/$1/pid"
    lock="$lockdir/$1"
    return 0
  fi
  # A run that died leaves its lock: taken over when its process is gone.
  local holder
  holder=$(cat "$lockdir/$1/pid" 2>/dev/null || true)
  if [[ -n "$holder" ]] && ! kill -0 "$holder" 2>/dev/null; then
    rm -rf "${lockdir:?}/$1"
    take "$1"
    return
  fi
  return 1
}

walk_cleanup() {
  if [[ -n "$xcode" ]]; then kill "$xcode" 2>/dev/null || true; fi
  # A full run's server goes with it; the live source stops its FFmpegs
  # and its web server with it.
  if [[ -z "$quick" && -n "$dir" ]]; then stop_kept "$dir"; fi
  if [[ -n "$lock" ]]; then rm -rf "$lock"; fi
}

walk() {
  local scheme=$1 name=$2 id log fresh=yes
  trap walk_cleanup EXIT
  trap 'exit 143' TERM INT
  if ! id=$(udid "$name"); then
    echo "check-screens: no simulator named $name" >&2
    return 1
  fi
  if ! take "$id"; then
    echo "check-screens: $name is in use by another check-screens run" >&2
    return 1
  fi

  if [[ -n "$quick" ]]; then
    dir="$kept/$id"
    if kept_server; then
      fresh=no
    else
      stop_kept "$dir"
      rm -rf "$dir"
    fi
  else
    dir="$temp/$id"
  fi
  if [[ $fresh == yes ]]; then
    mkdir -p "$dir"
    echo "check-screens: a server for $name"
    new_server
  fi
  # Where this run's lines start in the server's log, which a quick run keeps.
  local first
  first=$(($(wc -l <"$dir/server.log") + 1))

  # Booted afresh: a simulator up for long, or still starting, kills the
  # test runner before it connects. A quick run keeps one that is up.
  if [[ -z "$quick" ]] || ! xcrun simctl list devices booted | grep -q "$id"; then
    xcrun simctl shutdown "$id" 2>/dev/null || true
    xcrun simctl boot "$id" 2>/dev/null || true
  fi
  xcrun simctl bootstatus "$id" -b >/dev/null
  if [[ $fresh == yes ]]; then
    xcrun simctl uninstall "$id" "$bundle" 2>/dev/null || true
    # The server list and sign-ins are in the Keychain, which outlives the app.
    xcrun simctl keychain "$id" reset 2>/dev/null || true
    xcrun simctl bootstatus "$id" -b >/dev/null
  fi

  log="$dir/$scheme.log"
  echo "check-screens: $scheme on $name"
  local only=() retry=(-retry-tests-on-failure -test-iterations 2 -test-repetition-relaunch-enabled YES)
  if [[ -n "${PLEY_UI_TESTS:-}" ]]; then
    local target=PleyUITests test wanted
    [[ $scheme == PleyTVUITests ]] && target=PleyTVUITests
    IFS=';' read -r -a wanted <<<"$PLEY_UI_TESTS"
    for test in "${wanted[@]}"; do
      [[ -n "$test" ]] && only+=("-only-testing:$target/ScreensTests/$test")
    done
  fi
  if [[ -n "$quick" ]]; then retry=(); fi
  rm -rf "$dir/$scheme.xcresult"
  # xcodebuild hands TEST_RUNNER_ variables to the tests, without the
  # prefix. It runs in the background, so a stop reaches this walk at once.
  TEST_RUNNER_PLEY_ORIGIN="$origin" TEST_RUNNER_PLEY_SECRETS_FILE="$dir/secrets.json" \
    TEST_RUNNER_PLEY_ADMIN_NAME="Admin" TEST_RUNNER_PLEY_SHOTS="$shots" \
    xcodebuild test-without-building -project apps/apple/Pley.xcodeproj -scheme "$scheme" -destination "id=$id" \
    ${only[@]+"${only[@]}"} ${retry[@]+"${retry[@]}"} -resultBundlePath "$dir/$scheme.xcresult" \
    </dev/null >"$log" 2>&1 &
  xcode=$!
  local passed=yes
  wait "$xcode" || passed=no
  xcode=""
  if [[ $passed == yes ]]; then
    echo "check-screens: $name passed"
  else
    echo "check-screens: $name failed:" >&2
    grep -E "error:|failed|XCTAssert|didn't show|couldn't" "$log" | tail -n 20 | redacted >&2 || true
    echo "check-screens: $name's server's last lines:" >&2
    tail -n 40 "$dir/server.log" | redacted >&2
    if [[ -n "$logs" ]]; then redacted <"$log" >"$logs/$scheme-$name.log"; fi
  fi
  if tail -n "+$first" "$dir/server.log" | grep -q "pley_mk_"; then
    echo "check-screens: $name's server's log names a keyed media address" >&2
    passed=no
  fi
  [[ $passed == yes ]]
}

# --- Every simulator at once ------------------------------------------

names=()
if [[ -n "$ios_sims" ]]; then
  IFS=';' read -r -a phones <<<"$ios_sims"
  for name in "${phones[@]}"; do
    if [[ -n "$name" ]]; then
      (walk PleyUITests "$name") &
      walkers+=($!)
      names+=("$name")
    fi
  done
fi
if [[ -n "$tv_sim" ]]; then
  (walk PleyTVUITests "$tv_sim") &
  walkers+=($!)
  names+=("$tv_sim")
fi

failed=()
for i in "${!walkers[@]}"; do
  wait "${walkers[$i]}" || failed+=("${names[$i]}")
done
walkers=()
if [[ ${#failed[@]} -gt 0 ]]; then
  echo "check-screens: failed on ${failed[*]}" >&2
  exit 1
fi
echo "check-screens: passed"
