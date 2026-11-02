#!/usr/bin/env bash
# A made-up IPTV provider for the Apple apps' Live TV checks, on
# 127.0.0.1 only: an M3U lineup, an XMLTV guide and four live HLS
# channels made by FFmpeg from test patterns and tones.
#
#   live-source.sh <dir> <port>
#
# <dir>/list.m3u lists "Test One" (101, H.264 and AAC, "UK| News"), "Test
# Two" (102, MPEG-2 and MP2, "UK| Sport"), "Test Three" (103, H.264 720p50,
# "FR| Info") and "Test Radio" (201, AAC only, "Radio"), each with a logo;
# <dir>/guide.xml has half-hour programmes from an hour ago to six hours
# on ("Test One at 18:00"…). Each channel is a live HLS playlist of
# 2-second segments, six kept. It runs until stopped (TERM or INT), and
# stops its FFmpegs and its web server with it. macOS or Linux, with
# ffmpeg and python3.
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: live-source.sh <dir> <port>" >&2
  exit 2
fi
dir=$1
port=$2
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)
base="http://127.0.0.1:$port"

children=()
stop() {
  for pid in ${children[@]+"${children[@]}"}; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap stop EXIT
trap 'exit 0' TERM INT

# id;number;name;group
channels=(
  "one;101;Test One;UK| News"
  "two;102;Test Two;UK| Sport"
  "three;103;Test Three;FR| Info"
  "radio;201;Test Radio;Radio"
)

# The lineup.
{
  echo "#EXTM3U"
  for entry in "${channels[@]}"; do
    IFS=';' read -r id number name group <<<"$entry"
    echo "#EXTINF:-1 tvg-id=\"$id.test\" tvg-chno=\"$number\" tvg-logo=\"$base/logo-$id.png\" group-title=\"$group\",$name"
    echo "$base/$id/index.m3u8"
  done
} >"$dir/list.m3u"

# The guide: half-hour programmes, from an hour ago to six hours on.
python3 -I - "$dir/guide.xml" <<'PY'
import sys, time
from datetime import datetime, timezone
out = sys.argv[1]
now = int(time.time())
start = now // 1800 * 1800 - 3600
names = {"one": "Test One", "two": "Test Two", "three": "Test Three", "radio": "Test Radio"}
lines = ['<?xml version="1.0" encoding="UTF-8"?>', '<tv>']
for cid, name in names.items():
    lines.append(f'  <channel id="{cid}.test"><display-name>{name}</display-name></channel>')
def stamp(t):
    return datetime.fromtimestamp(t, timezone.utc).strftime("%Y%m%d%H%M%S +0000")
for cid, name in names.items():
    for n in range(14):
        a = start + n * 1800
        b = a + 1800
        hm = datetime.fromtimestamp(a, timezone.utc).strftime("%H:%M")
        lines.append(f'  <programme start="{stamp(a)}" stop="{stamp(b)}" channel="{cid}.test">')
        lines.append(f'    <title>{name} at {hm}</title>')
        lines.append(f'    <desc>A made-up programme of {name}.</desc>')
        lines.append('  </programme>')
lines.append('</tv>')
open(out, "w").write("\n".join(lines) + "\n")
PY

for entry in "${channels[@]}"; do
  id=${entry%%;*}
  ffmpeg -nostdin -loglevel error -y -f lavfi -i "color=c=0x3366aa:s=200x200" \
    -vf "drawtext=text='${id}':fontcolor=white:fontsize=48:x=(w-text_w)/2:y=(h-text_h)/2" -frames:v 1 \
    "$dir/logo-$id.png" 2>/dev/null ||
    ffmpeg -nostdin -loglevel error -y -f lavfi -i "color=c=0x3366aa:s=200x200" -frames:v 1 "$dir/logo-$id.png"
done

hls=(-f hls -hls_time 2 -hls_list_size 6 -hls_flags delete_segments+omit_endlist)
live() {
  local id=$1
  shift
  mkdir -p "$dir/$id"
  ffmpeg -nostdin -loglevel error -re "$@" "${hls[@]}" -hls_segment_filename "$dir/$id/seg%05d.ts" \
    "$dir/$id/index.m3u8" &
  children+=($!)
}
live one -f lavfi -i "testsrc2=s=1280x720:r=25" -f lavfi -i "sine=frequency=440:sample_rate=48000" \
  -c:v libx264 -preset veryfast -tune zerolatency -g 50 -pix_fmt yuv420p -c:a aac -b:a 128k
live two -f lavfi -i "testsrc2=s=720x576:r=25" -f lavfi -i "sine=frequency=550:sample_rate=48000" \
  -c:v mpeg2video -b:v 3M -g 25 -c:a mp2 -b:a 192k
live three -f lavfi -i "testsrc2=s=1280x720:r=50" -f lavfi -i "sine=frequency=660:sample_rate=48000" \
  -c:v libx264 -preset veryfast -tune zerolatency -g 100 -pix_fmt yuv420p -c:a aac -b:a 128k
live radio -f lavfi -i "sine=frequency=330:sample_rate=48000" -c:a aac -b:a 128k

(cd "$dir" && exec python3 -I -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
children+=($!)

# Ready once every channel has its first segments.
for _ in $(seq 1 60); do
  ready=yes
  for entry in "${channels[@]}"; do
    [[ -s "$dir/${entry%%;*}/index.m3u8" ]] || ready=no
  done
  if [[ $ready == yes ]] && curl -fsS "$base/list.m3u" >/dev/null 2>&1; then break; fi
  sleep 1
done
echo "live-source: ready on $base"
wait
