"""What this Kodi plays, as pley's version 2 capability report
(docs/API.md, Playback).

Kodi's player is FFmpeg with the platform's decoders beside it: it opens
nearly every container and codec, switches audio tracks and draws every
kind of subtitle itself. What differs between Kodis is the screen (HDR,
Dolby Vision) and what the audio settings send to a receiver as it is.
"""

from __future__ import annotations

CONTAINERS = [
    "mkv", "matroska", "webm", "mp4", "m4v", "mov", "avi", "ts", "mpegts", "m2ts", "mpeg", "mpg", "vob",
    "wmv", "asf", "flv", "ogg", "ogv", "mp3", "m4a", "flac", "wav", "aac", "opus", "wma", "alac", "ape", "dsf",
]
VIDEO = [
    {"codec": "h264", "profiles": ["Baseline", "Constrained Baseline", "Main", "High", "High 10"]},
    {"codec": "hevc", "profiles": ["Main", "Main 10", "Rext"]},
    {"codec": "av1"},
    {"codec": "vp9"},
    {"codec": "vp8"},
    {"codec": "mpeg2video"},
    {"codec": "mpeg4"},
    {"codec": "vc1"},
    {"codec": "wmv3"},
    {"codec": "msmpeg4v3"},
]
AUDIO = [
    "aac", "mp3", "mp2", "ac3", "eac3", "dts", "truehd", "mlp", "flac", "alac", "opus", "vorbis", "wmav2",
    "wmapro", "pcm_s16le", "pcm_s24le", "pcm_s32le", "pcm_s16be", "pcm_s24be", "pcm_f32le", "pcm_bluray",
    "pcm_dvd", "ape", "dsd_lsbf", "dsd_msbf",
]
SUBTITLES = ["subrip", "srt", "ass", "ssa", "mov_text", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"]
# Kodi's setting for each format a receiver may take as it is.
PASSTHROUGH = (
    ("audiooutput.ac3passthrough", ["ac3"]),
    ("audiooutput.eac3passthrough", ["eac3"]),
    ("audiooutput.dtspassthrough", ["dts"]),
    ("audiooutput.truehdpassthrough", ["truehd"]),
)
# audiooutput.channels, Kodi's channel layouts: 1 is 2.0 … 8 is 5.1, 10 is 7.1.
CHANNELS = {1: 2, 2: 3, 3: 3, 4: 4, 5: 4, 6: 5, 7: 5, 8: 6, 9: 7, 10: 8}


def hdr_types(supported: str) -> list:
    """Kodi's System.SupportedHDRTypes ("HDR10, HLG, Dolby Vision") as
    pley's names. Kodi plays HDR10 and HLG on any screen (it tone-maps on
    SDR ones), so those are always there; Dolby Vision only where the
    screen and decoder take it."""
    found = {"hdr10", "hdr10plus", "hlg"}
    if "dolby vision" in supported.lower():
        found.add("dolby_vision")
    return sorted(found)


def build(
    supported_hdr: str,
    settings: dict,
    screen: tuple,
    model: str,
    coreelec: bool = False,
) -> dict:
    """The report: `supported_hdr` is System.SupportedHDRTypes, `settings`
    Kodi's audio settings by name (missing ones as None), `screen` (width,
    height)."""
    types = hdr_types(supported_hdr)
    passthrough = []
    if settings.get("audiooutput.passthrough"):
        for name, codecs in PASSTHROUGH:
            if settings.get(name):
                passthrough += codecs
    channels = CHANNELS.get(settings.get("audiooutput.channels") or 0, 2)
    width, height = screen
    caps = {
        "caps_version": 2,
        # Version 1's fields, for servers that only read those.
        "containers": CONTAINERS,
        "video_codecs": [entry["codec"] for entry in VIDEO],
        "audio_codecs": AUDIO,
        "hdr": True,
        "dolby_vision": "dolby_vision" in types,
        "video": VIDEO,
        "hdr_types": types,
        "audio_decode": AUDIO,
        "audio_passthrough": passthrough,
        # Kodi decodes everything and mixes it down for the speakers.
        "audio_channels": max(channels, 2),
        "subtitle_codecs": SUBTITLES,
        # PGS beside a stream or next to the media comes as a `.sup` file
        # (tracks.beside_tracks); VobSub and DVB are drawn onto converted
        # video, asked for with `subtitle_index`.
        "picture_subtitles_sideload": True,
        "audio_track_switching": True,
        # Kodi's HLS player (FFmpeg's) reads no CODECS, and MPEG-TS segments
        # carry no Dolby Vision configuration: Dolby Vision the server names
        # only in master.m3u8 would play as HDR10, so it isn't asked for.
        "hls": {"master_playlist": False, "segments": ["ts"]},
        "device": {"form": "tv", "model": model},
    }
    if "dolby_vision" in types:
        # CoreELEC plays profile 7 with its enhancement layer; elsewhere its
        # base layer, which the server repackages.
        caps["dolby_vision_profiles"] = [{"profile": p} for p in ((5, 7, 8) if coreelec else (5, 8))]
        # CoreELEC's own Dolby Vision shows it with HDR10+ beside it; on
        # Android, Kodi gets the platform's decoder, which many TV devices
        # show black with (androidx/media#1895).
        if coreelec:
            caps["dolby_vision_hdr10plus"] = True
    if width and height:
        caps["display"] = {"width": width, "height": height}
    return caps
