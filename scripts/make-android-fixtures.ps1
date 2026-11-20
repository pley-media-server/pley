# Makes the media the Android app's FixtureServer plays (apps/android/testing/fixtures),
# from ffmpeg's own test pattern and tone, so nothing in them comes from a film:
#  * sample-1080p.mp4: 20 s of 1080p at 24 fps, H.264 High with stereo AAC;
#  * sample-2160p-hdr10.mkv: 20 s of 2160p at 23.976 fps, HEVC Main 10 with HDR10
#    metadata, AC-3 5.1, an English SRT track and three chapters;
#  * hls/: the H.264 clip as an HLS stream of MPEG-TS segments named as the
#    server names them (index.m3u8, 0.ts, 1.ts…).
# The files are committed; run this again only to change them. Together they stay
# under 8 MB, or the script fails.
#   powershell -ExecutionPolicy Bypass -File scripts\make-android-fixtures.ps1
param(
    [string]$Ffmpeg = "ffmpeg"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root "apps/android/testing/fixtures/src/main/resources/app/pley/android/fixtures/media"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("pley-fixtures-" + [guid]::NewGuid())
$maxBytes = 8MB

function Invoke-Ffmpeg([string[]]$arguments) {
    & $Ffmpeg -hide_banner -loglevel error -y @arguments
    if ($LASTEXITCODE) { throw "ffmpeg failed: $($arguments -join ' ')" }
}

New-Item -ItemType Directory -Force $work | Out-Null
try {
    if (Test-Path $out) { Remove-Item -Recurse -Force $out }
    New-Item -ItemType Directory -Force (Join-Path $out "hls") | Out-Null

    # One key frame every 2 s, so the 4 s HLS segments start on one.
    $mp4 = Join-Path $out "sample-1080p.mp4"
    Invoke-Ffmpeg @(
        "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=24:duration=20",
        "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=20",
        "-map", "0:v", "-map", "1:a",
        "-c:v", "libx264", "-preset", "slow", "-profile:v", "high", "-level:v", "4.0",
        "-vf", "format=yuv420p,setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
        "-b:v", "450k", "-maxrate", "900k", "-bufsize", "1800k",
        "-g", "48", "-keyint_min", "48", "-sc_threshold", "0",
        "-c:a", "aac", "-b:a", "96k", "-ac", "2",
        "-metadata:s:a:0", "language=eng",
        "-map_metadata", "-1", "-movflags", "+faststart", "-fflags", "+bitexact",
        $mp4
    )

    # The same video and audio, copied into segments.
    Push-Location (Join-Path $out "hls")
    try {
        Invoke-Ffmpeg @(
            "-i", $mp4, "-c", "copy", "-f", "hls", "-hls_time", "4", "-hls_playlist_type", "vod",
            "-hls_segment_filename", "%d.ts", "index.m3u8"
        )
    } finally {
        Pop-Location
    }

    $srt = Join-Path $work "sample.srt"
    Set-Content -Encoding ascii -Path $srt -Value @(
        "1", "00:00:01,000 --> 00:00:04,000", "The fixture server plays this.", "",
        "2", "00:00:06,000 --> 00:00:09,000", "Nothing here is a film.", "",
        "3", "00:00:11,000 --> 00:00:14,000", "Test pattern and tone only.", "",
        "4", "00:00:16,000 --> 00:00:19,000", "The end credits.", ""
    )
    $chapters = Join-Path $work "chapters.txt"
    Set-Content -Encoding ascii -Path $chapters -Value @(
        ";FFMETADATA1",
        "[CHAPTER]", "TIMEBASE=1/1000", "START=0", "END=4000", "title=Intro",
        "[CHAPTER]", "TIMEBASE=1/1000", "START=4000", "END=16000", "title=Part 1",
        "[CHAPTER]", "TIMEBASE=1/1000", "START=16000", "END=20000", "title=Credits"
    )

    # HDR10: BT.2020 primaries, PQ, mastering display and light levels in the
    # stream. (x265's hdr10-opt is left off: it makes the rate control miss its
    # target several times over.)
    $x265 = @(
        "keyint=48", "min-keyint=48", "hdr10=1", "repeat-headers=1",
        "colorprim=bt2020", "transfer=smpte2084", "colormatrix=bt2020nc",
        "master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1)",
        "max-cll=1000,400", "log-level=error"
    ) -join ":"
    Invoke-Ffmpeg @(
        "-f", "lavfi", "-i", "testsrc2=size=3840x2160:rate=24000/1001:duration=20",
        "-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=20",
        "-i", $srt, "-i", $chapters,
        "-map", "0:v", "-map", "1:a", "-map", "2:s", "-map_metadata", "-1", "-map_chapters", "3",
        # The frames carry the colors, or the stream and the container say "unknown".
        "-vf", "format=yuv420p10le,setparams=range=tv:color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc",
        "-c:v", "libx265", "-preset", "fast", "-profile:v", "main10",
        "-b:v", "700k", "-maxrate", "1000k", "-bufsize", "2000k", "-x265-params", $x265,
        "-c:a", "ac3", "-b:a", "256k", "-ac", "6",
        "-c:s", "srt",
        "-metadata:s:a:0", "language=eng", "-metadata:s:s:0", "language=eng", "-metadata:s:s:0", "title=English",
        "-fflags", "+bitexact",
        (Join-Path $out "sample-2160p-hdr10.mkv")
    )
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$files = Get-ChildItem -Recurse -File $out
$total = ($files | Measure-Object -Property Length -Sum).Sum
foreach ($file in $files) {
    Write-Host ("{0,10:N0}  {1}" -f $file.Length, $file.FullName.Substring($out.Length + 1))
}
Write-Host ("{0,10:N0}  in all" -f $total)
if ($total -gt $maxBytes) {
    throw ("the fixture media take {0:N1} MB, more than {1:N0} MB" -f ($total / 1MB), ($maxBytes / 1MB))
}
