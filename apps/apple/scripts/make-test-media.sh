#!/usr/bin/env bash
# Makes the Apple apps' test media with FFmpeg (and make-pgs.py for the
# picture subtitles): generated pictures and tones, nothing copied.
#
#   make-test-media.sh <dir>
#
# writes into <dir>/Movies/<Title> (<Year>)/ and
# <dir>/Shows/Pley Test Show/Season 01/:
#
#   1. Direct H264 (2020).mp4     H.264 High 4.1 + AAC, 90 s, with an .en.srt beside it
#   2. Two Audio (2020).mp4       H.264 + AAC stereo (eng) + AC-3 5.1 (fre)
#   3. Remux H264 AC3 (2021).mkv  H.264 + AC-3 5.1 + an SRT and a PGS track
#   4. HEVC HDR10 (2022).mkv      HEVC Main 10 HDR10 + E-AC-3 5.1, 30 s
#   5. MPEG2 (1999).mkv           MPEG-2 + MP2
#   6. DTS (2005).mkv             H.264 + DTS 5.1
#   7. AV1 (2024).mkv             AV1 10-bit 1080p, 30 s
#   8. Pley Test Show - S01E01.mkv and S01E02.mkv, 75 s each, with chapters
#      Prologue, Intro (10-30 s), Story and Credits (55-75 s)
#
# FFmpeg needs libx264, libx265, libsvtav1 and its own AAC, AC-3, E-AC-3,
# DTS (dca) and MP2 encoders. Files already there are kept.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: make-test-media.sh <dir>" >&2
  exit 2
fi
out=$1
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

ff() { ffmpeg -hide_banner -loglevel error -nostdin -y "$@"; }

# A 1080p test picture at 23.976 fps, `$1` seconds.
video() { echo "testsrc2=size=${2:-1920x1080}:rate=24000/1001:duration=$1"; }
# A tone (made stereo by the encoders' -ac 2) and 5.1 tones, `$1` seconds.
stereo() { echo "sine=frequency=440:sample_rate=48000:duration=$1"; }
surround() {
  echo "aevalsrc=0.2*sin(2*PI*440*t)|0.2*sin(2*PI*554*t)|0.2*sin(2*PI*659*t)|0.1*sin(2*PI*110*t)|0.2*sin(2*PI*880*t)|0.2*sin(2*PI*988*t):s=48000:c=5.1:d=$1"
}
h264=(-c:v libx264 -profile:v high -level 4.1 -pix_fmt yuv420p -preset veryfast -crf 26 -g 48)

movie() {
  local dir="$out/Movies/$1"
  mkdir -p "$dir"
  echo "$dir/$1.$2"
}

srt() {
  cat >"$1" <<'EOF'
1
00:00:02,000 --> 00:00:05,000
The first line.

2
00:00:08,000 --> 00:00:11,000
The second line.

3
00:00:14,000 --> 00:00:17,000
The third line.

4
00:00:20,000 --> 00:00:23,000
The fourth line.

5
00:00:26,000 --> 00:00:29,000
The fifth line.

6
00:00:32,000 --> 00:00:35,000
The sixth line.
EOF
}

made() { echo "make-test-media: $1"; }

# 1. Direct H264 (2020).mp4, with an external English SRT.
f=$(movie "Direct H264 (2020)" mp4)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "$(video 90)" -f lavfi -i "$(stereo 90)" "${h264[@]}" -c:a aac -ac 2 -b:a 128k \
    -movflags +faststart "$f"
  srt "${f%.mp4}.en.srt"
fi
made "1 Direct H264"

# 2. Two Audio (2020).mp4: AAC stereo in English and AC-3 5.1 in French.
f=$(movie "Two Audio (2020)" mp4)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "$(video 30)" -f lavfi -i "$(stereo 30)" -f lavfi -i "$(surround 30)" \
    -map 0:v -map 1:a -map 2:a "${h264[@]}" -c:a:0 aac -ac:a:0 2 -b:a:0 128k -c:a:1 ac3 -b:a:1 384k \
    -metadata:s:a:0 language=eng -metadata:s:a:1 language=fre -movflags +faststart "$f"
fi
made "2 Two Audio"

# 3. Remux H264 AC3 (2021).mkv, with an SRT track and a PGS track.
f=$(movie "Remux H264 AC3 (2021)" mkv)
if [[ ! -e "$f" ]]; then
  srt "$work/three.srt"
  python3 -I "$here/make-pgs.py" "$work/three.sup" 1920 1080
  ff -f lavfi -i "$(video 60)" -f lavfi -i "$(surround 60)" -i "$work/three.srt" -i "$work/three.sup" \
    -map 0:v -map 1:a -map 2:s -map 3:s "${h264[@]}" -c:a ac3 -b:a 384k -c:s:0 srt -c:s:1 copy \
    -metadata:s:s:0 language=eng -metadata:s:s:1 language=eng "$f"
fi
made "3 Remux H264 AC3"

# 4. HEVC HDR10 (2022).mkv: Main 10, PQ, BT.2020, with mastering metadata.
f=$(movie "HEVC HDR10 (2022)" mkv)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "$(video 30)" -f lavfi -i "$(surround 30)" \
    -vf "format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc:range=tv" -c:v libx265 -preset ultrafast -crf 28 -pix_fmt yuv420p10le -tag:v hvc1 \
    -x265-params "log-level=error:hdr10=1:hdr10-opt=1:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400" \
    -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc \
    -c:a eac3 -b:a 448k "$f"
fi
made "4 HEVC HDR10"

# 5. MPEG2 (1999).mkv: MPEG-2 video and MP2 audio.
f=$(movie "MPEG2 (1999)" mkv)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "testsrc2=size=720x576:rate=25:duration=30" -f lavfi -i "$(stereo 30)" \
    -c:v mpeg2video -b:v 4M -pix_fmt yuv420p -c:a mp2 -ac 2 -b:a 192k "$f"
fi
made "5 MPEG2"

# 6. DTS (2005).mkv: H.264 and DTS 5.1.
f=$(movie "DTS (2005)" mkv)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "$(video 30)" -f lavfi -i "$(surround 30)" "${h264[@]}" -c:a dca -strict -2 -ac 6 "$f"
fi
made "6 DTS"

# 7. AV1 (2024).mkv: AV1 10-bit, with AAC.
f=$(movie "AV1 (2024)" mkv)
if [[ ! -e "$f" ]]; then
  ff -f lavfi -i "$(video 30)" -f lavfi -i "$(stereo 30)" \
    -c:v libsvtav1 -preset 10 -crf 40 -pix_fmt yuv420p10le -g 48 -svtav1-params "lp=4" -c:a aac -ac 2 "$f"
fi
made "7 AV1"

# 8. Two episodes with chapters: the intro and the credits come from their names.
show="$out/Shows/Pley Test Show/Season 01"
mkdir -p "$show"
cat >"$work/chapters.txt" <<'EOF'
;FFMETADATA1
[CHAPTER]
TIMEBASE=1/1000
START=0
END=10000
title=Prologue
[CHAPTER]
TIMEBASE=1/1000
START=10000
END=30000
title=Intro
[CHAPTER]
TIMEBASE=1/1000
START=30000
END=55000
title=Story
[CHAPTER]
TIMEBASE=1/1000
START=55000
END=75000
title=Credits
EOF
for episode in 01 02; do
  f="$show/Pley Test Show - S01E$episode.mkv"
  if [[ ! -e "$f" ]]; then
    ff -f lavfi -i "$(video 75 1280x720)" -f lavfi -i "$(stereo 75)" -i "$work/chapters.txt" \
      -map 0:v -map 1:a -map_chapters 2 "${h264[@]}" -c:a aac -ac 2 -b:a 128k "$f"
  fi
  made "8 Pley Test Show S01E$episode"
done
