"""What pley's screens say of a title, as the Android TV app says it
(apps/android/core/model/.../format: Durations.kt, Labels.kt, Media.kt),
which says it as the web does: runtimes, time left, card lines, episode
numbers and quality badges, in the language shown (kodi.text)."""

from __future__ import annotations

import datetime

from . import kodi

MINUTE_MS = 60_000
DAY = 86400


def whole_minutes(ms: float) -> int:
    """Whole minutes, half a minute rounded up (the web's Math.round)."""
    return int(ms / MINUTE_MS + 0.5)


def duration(ms) -> str:
    """6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s"."""
    if not ms or ms <= 0:
        return ""
    minutes = whole_minutes(ms)
    if minutes < 1:
        return kodi.text("seconds_short") % kodi.number(max(1, int(ms / 1000 + 0.5)))
    h, m = divmod(minutes, 60)
    if h == 0:
        return kodi.text("minutes_short") % kodi.number(m)
    if not m:
        return kodi.text("hours_short") % kodi.number(h)
    return kodi.text("hours_minutes").format(hours=kodi.number(h), minutes=kodi.number(m))


def runtime(ms) -> str:
    """"58 min" under an hour, "1h 52m" from one, "29s" under half a minute."""
    if not ms or ms <= 0:
        return ""
    minutes = whole_minutes(ms)
    return kodi.text("runtime_minutes") % kodi.number(minutes) if 1 <= minutes <= 59 else duration(ms)


def clock(ms) -> str:
    """A track's length as the web's formatClock: "3:45", "1:02:03"."""
    if not ms or ms <= 0:
        return ""
    seconds = int(ms / 1000 + 0.5)
    h, rest = divmod(seconds, 3600)
    m, s = divmod(rest, 60)
    return "%d:%02d:%02d" % (h, m, s) if h else "%d:%02d" % (m, s)


def time_left(position_ms, duration_ms) -> str:
    """"26 min left"; "" without a duration or once nothing is left."""
    if not duration_ms or duration_ms <= 0:
        return ""
    left = runtime(max(0, duration_ms - (position_ms or 0)))
    return kodi.text("time_left") % left if left else ""


def episode_label(season, episode) -> str:
    """"S1 · E3", "E3", or ""."""
    parts = []
    if season is not None:
        parts.append(kodi.text("season_short") % season)
    if episode is not None:
        parts.append(kodi.text("episode_short") % episode)
    return " · ".join(parts)


def episode_long_label(season, episode) -> str:
    """"Season 2 · Episode 5", for an episode's line on its show's page."""
    parts = []
    if season is not None:
        parts.append(kodi.text("specials") if season == 0 else kodi.text("season_number") % season)
    if episode is not None:
        parts.append(kodi.text("episode_number") % episode)
    return " · ".join(parts)


def season_title(title, index) -> str:
    if title:
        return title
    if index == 0:
        return kodi.text("specials")
    return kodi.text("season_number") % index if index is not None else kodi.text("season")


def card_subtitle(item: dict) -> str:
    """The line under a tile, by kind: a year, "3 seasons", "S2 · E5"…"""
    kind = item.get("kind")
    year = str(item["year"]) if item.get("year") else ""
    if kind == "movie":
        return year
    if kind == "show":
        if item.get("child_count"):
            return kodi.count("seasons", item["child_count"])
        if item.get("leaf_count") is not None:
            return kodi.count("episodes", item["leaf_count"])
        return year
    if kind == "season":
        return kodi.count("episodes", item["leaf_count"]) if item.get("leaf_count") is not None else ""
    if kind == "episode":
        return episode_label(item.get("parent_index"), item.get("index"))
    if kind == "artist":
        return kodi.count("albums", item["child_count"]) if item.get("child_count") is not None else ""
    if kind == "album":
        return " · ".join(p for p in (item.get("parent_title") or "", year) if p)
    if kind == "track":
        return " · ".join(p for p in (item.get("grandparent_title") or "", item.get("parent_title") or "") if p)
    return ""


def state(item: dict) -> dict:
    return item.get("user_state") or {}


def in_progress(item: dict) -> bool:
    """Started, not finished, not marked watched."""
    length = item.get("duration_ms")
    position = state(item).get("position_ms") or 0
    return bool(length) and not state(item).get("watched") and 0 < position < length


def progress(item: dict) -> float:
    """Played so far, 0 to 1."""
    length = item.get("duration_ms") or 0
    position = state(item).get("position_ms") or 0
    if position <= 0 or length <= 0:
        return 0.0
    return min(max(position / length, 0.0), 1.0)


def unwatched(item: dict) -> int:
    """Episodes of a show or season not watched yet."""
    if item.get("kind") not in ("show", "season") or item.get("leaf_count") is None:
        return 0
    return max(0, item["leaf_count"] - (item.get("viewed_leaf_count") or 0))


def resolution_badge(width, height):
    """"4K", "1080p", "720p", "SD", or None."""
    w, h = width or 0, height or 0
    if w >= 3800 or h >= 2100:
        return "4K"
    if w >= 1800 or h >= 1000:
        return "1080p"
    if w >= 1200 or h >= 700:
        return "720p"
    return "SD" if h > 0 else None


def dynamic_range_short(stream: dict):
    """"DV P8.1", "DV P7 FEL", "DV", "HDR10+", "HDR10", "HLG", "HDR", or None
    (SDR), as the server names them: the base layer's compatibility from
    profile 8 on only (profile 7's is always Blu-ray HDR10, never "P7.6")."""
    kind = stream.get("dynamic_range")
    if kind == "dolby_vision":
        profile = stream.get("dv_profile")
        if profile is None:
            return "DV"
        compat = stream.get("dv_bl_compat")
        layer = stream.get("dv_el") if profile == 7 else None
        return (
            "DV P%d" % profile
            + (".%d" % compat if profile >= 8 and compat in (1, 2, 4) else "")
            + (" " + layer.upper() if layer else "")
        )
    if kind == "hdr10plus":
        return "HDR10+"
    if kind == "hdr10":
        return "HDR10"
    if kind == "hlg":
        return "HLG"
    if kind == "sdr":
        return None
    return "HDR" if stream.get("hdr") else None


def is_atmos(stream) -> bool:
    if not stream or stream.get("kind") != "audio":
        return False
    return "atmos" in (stream.get("profile") or "").lower() or "atmos" in (stream.get("title") or "").lower()


def quality_badges(file) -> list:
    """A file's badges, in the web's order: resolution, dynamic range,
    then "Atmos", else "7.1" or "5.1"."""
    if not file:
        return []
    streams = file.get("streams") or []
    video = next((s for s in streams if s.get("kind") == "video"), None)
    audio = next((s for s in streams if s.get("kind") == "audio" and s.get("default")), None) or next(
        (s for s in streams if s.get("kind") == "audio"), None
    )
    channels = (audio or {}).get("channels") or 0
    badges = [
        resolution_badge(file.get("width") or (video or {}).get("width"), file.get("height") or (video or {}).get("height")),
        dynamic_range_short(video) if video else None,
    ]
    # Dolby Vision + HDR10+: a badge for each.
    if video and video.get("dynamic_range") == "dolby_vision" and video.get("hdr10plus"):
        badges.append("HDR10+")
    if is_atmos(audio):
        badges.append("Atmos")
    elif channels >= 8:
        badges.append("7.1")
    elif channels >= 6:
        badges.append("5.1")
    return [b for b in badges if b]


# A library's quality filters (GET /api/libraries/{id}/items' `resolution`
# and `dynamic_range`): the server's tokens and what the filters call them,
# best first, as the server lists them.
RESOLUTIONS = {"4k": "4K", "1080": "1080p", "720": "720p", "sd": "SD"}
# HDR10 and HDR10+ read as plain "HDR" and "HDR+" here and on the posters;
# a title's page names them exactly. "hdr", any of them, is in words
# (dynamic_ranges()).
RANGE_NAMES = {"dolby_vision": "Dolby Vision", "hdr10plus": "HDR+", "hdr10": "HDR", "hlg": "HLG", "sdr": "SDR"}
# A poster's dynamic range badge, by its file's (the web's mediaBadges):
# SDR and HDR of no known kind have none.
RANGE_BADGES = {"dolby_vision": "DV", "hdr10plus": "HDR+", "hdr10": "HDR", "hlg": "HLG"}


def dynamic_ranges() -> dict:
    """The dynamic range filter's choices, by the server's tokens, best first."""
    return dict({"hdr": kodi.text("any_hdr")}, **RANGE_NAMES)


def chip_ranges() -> dict:
    """The Filter chip's short words for a dynamic range while it's on, the
    posters' where they have one: "4K · DV" fits the chip row where
    "4K · Dolby Vision" can push it past the screen."""
    return dict(dynamic_ranges(), **RANGE_BADGES)


def media_badge_keys(media) -> list:
    """A library poster's badges from its listing's media summary (the
    web's mediaBadges), by the server's tokens: "4k" of the resolutions
    only, then the dynamic range; no codec, which on most posters says
    nothing worth the room."""
    if not isinstance(media, dict):
        return []
    keys = ["4k" if resolution_badge(media.get("width"), media.get("height")) == "4K" else None]
    range_ = media.get("dynamic_range") or ""
    keys.append(range_ if range_ in RANGE_BADGES else None)
    return [k for k in keys if k]


def media_badges(media) -> list:
    """media_badge_keys' words: ["4K", "DV"]."""
    return ["4K" if key == "4k" else RANGE_BADGES[key] for key in media_badge_keys(media)]


def size(n) -> str:
    """Bytes as the web writes them, in units of 1024: "23.4 GB", "150 MB";
    "" when unknown."""
    if not n or n < 0:
        return ""
    value, unit = float(n), 0
    while value >= 1024 and unit < len(kodi.BYTES) - 1:
        value /= 1024
        unit += 1
    return "%s %s" % (kodi.decimal(value, 0 if value >= 100 or unit == 0 else 1), kodi.text_of(kodi.BYTES[unit]))


def bitrate(bps) -> str:
    """Bits a second as the web writes them: "8.2 Mbps", "42 Mbps", "320 kbps"
    (the units the same in every language, the decimal mark the language's)."""
    if not bps or bps <= 0:
        return ""
    if bps >= 1_000_000:
        return "%s Mbps" % kodi.decimal(bps / 1_000_000, 0 if bps >= 10_000_000 else 1)
    return "%s kbps" % kodi.number(int(bps / 1000 + 0.5))


def media_line(media) -> str:
    """The line under a poster sorted by bitrate: "42 Mbps · 23.4 GB",
    "Bitrate unknown" when neither is known, "" without a summary."""
    if not isinstance(media, dict):
        return ""
    return " · ".join(p for p in (bitrate(media.get("bitrate")), size(media.get("size"))) if p) or kodi.text("bitrate_unknown")


def stars(value) -> str:
    """Your rating, 0 to 10 on the server, in stars: 7 → "3½ stars"."""
    whole, half = divmod(int(value), 2)
    if not whole:
        # Half a star counts as one.
        return kodi.counted("stars", 1) % "½"
    return kodi.counted("stars", whole + 0.5 * half) % ("%d%s" % (whole, "½" if half else ""))


def rating(value) -> str:
    """The critics' rating, "7.4"; "" without one."""
    return kodi.decimal(value, 1) if value else ""


def hue(seed: str) -> int:
    """The web's placeholder hue: h = h * 31 + code over UTF-16 units,
    unsigned 32-bit, mod 360."""
    h = 0
    data = seed.encode("utf-16-le")
    for i in range(0, len(data), 2):
        h = (h * 31 + (data[i] | data[i + 1] << 8)) & 0xFFFFFFFF
    return h % 360


def placeholder(seed: str) -> str:
    """The placeholder picture nearest the web's gradient for `seed`."""
    return "ph-%02d.png" % (int(hue(seed) / 15 + 0.5) % 24)


def month(number: int) -> str:
    """A month's short name in the language shown: 3 → "Mar"."""
    return kodi.text_of(kodi.MONTHS[number - 1])


def date(text) -> str:
    """"2024-03-05" → "Mar 5, 2024" (an episode's air date); as given when
    it isn't a date."""
    try:
        year, number, day = (int(part) for part in (text or "").split("-"))
        if not 1 <= number <= 12:
            raise ValueError(text)
        return kodi.text("date").format(month=month(number), day=day, year=year)
    except (ValueError, IndexError):
        return text or ""


def relative(at, now) -> str:
    """How far `at` is from `now` (Unix seconds), as pley's apps say it
    (Kodi has nothing that says it): "in 20 min", "in 5 hours",
    "tomorrow", "in 12 days"; before now "just now", "20 min ago",
    "5 hours ago", "yesterday", "3 days ago", and further back the date.
    Days are the calendar's, here."""
    if at is None:
        return ""
    ahead = at >= now
    seconds = abs(at - now)
    if seconds < 3600:
        if not ahead and seconds < 60:
            return kodi.text("just_now")
        minutes = kodi.number(min(max(int(seconds / 60 + 0.5), 1), 59))
        return kodi.text("in_minutes" if ahead else "minutes_ago") % minutes
    if seconds < DAY - 1800:
        return kodi.count("in_hours" if ahead else "hours_ago", int(seconds / 3600 + 0.5))
    days = max(abs((datetime.date.fromtimestamp(at) - datetime.date.fromtimestamp(now)).days), 1)
    if ahead:
        return kodi.text("tomorrow_in_text") if days == 1 else kodi.count("in_days", days)
    if days == 1:
        return kodi.text("yesterday_in_text")
    if days < 30:
        return kodi.count("days_ago", days)
    return date(datetime.date.fromtimestamp(at).isoformat())


def day(value, now) -> str:
    """A day as pley's apps show one: "Nov 4", with its year when it isn't
    this year's ("Nov 4, 2027"). `value`: Unix seconds, or "2026-11-04"
    (as given when it isn't a date)."""
    try:
        if isinstance(value, str):
            year, number, day_ = (int(part) for part in value.split("-"))
        else:
            found = datetime.date.fromtimestamp(value)
            year, number, day_ = found.year, found.month, found.day
        if not 1 <= number <= 12:
            raise ValueError(value)
        name = month(number)
    except (ValueError, IndexError, TypeError, OverflowError, OSError):
        return value if isinstance(value, str) else ""
    pattern = "day_of_year" if year == datetime.date.fromtimestamp(now).year else "date"
    return kodi.text(pattern).format(month=name, day=day_, year=year)


def seasons_label(numbers) -> str:
    """Seasons asked for, or ready: "Season 2", "Seasons 1–3, 5", "Seasons
    1–2 and specials", "Specials" (the web's seasonsLabel)."""
    numbers = sorted({n for n in numbers or [] if isinstance(n, int) and n >= 0})
    numbered = [n for n in numbers if n > 0]
    specials = bool(numbers) and numbers[0] == 0
    if not numbered:
        return kodi.text("specials") if specials else ""
    ranges = []
    first = last = numbered[0]
    for number in numbered[1:] + [None]:
        if number is not None and number == last + 1:
            last = number
            continue
        ranges.append(str(first) if first == last else "%d–%d" % (first, last))
        first = last = number
    label = kodi.text("season_number" if len(numbered) == 1 else "seasons_numbered") % ", ".join(ranges)
    return kodi.text("and_specials") % label if specials else label


def year_range(year, seasons=None) -> str:
    """"2021-2025" from a show's seasons, or its own year."""
    years = [s["year"] for s in seasons or [] if s.get("year")] + ([year] if year else [])
    if not years:
        return ""
    first, last = min(years), max(years)
    return "%d-%d" % (first, last) if last > first else str(first)


def item_meta(detail: dict, seasons=None) -> list:
    """An item page's metadata line (ItemPages.kt): a show's seasons and
    years, a season's year and episodes, an episode's air date and
    runtime, others' year and runtime; then the content rating, the rating
    and the quality badges. As (kind, text) pieces: text, badge (the
    content rating), rating, quality."""
    parts = []

    def text(value):
        if value:
            parts.append(("text", value))

    kind = detail.get("kind")
    files = detail.get("files") or []
    edition = (files[0].get("edition") if files else None) or ""
    if kind == "show":
        count = detail.get("child_count") or len([s for s in seasons or [] if (s.get("index") or 1) > 0]) or None
        if count:
            text(kodi.count("seasons", count))
        text(year_range(detail.get("year"), seasons))
    elif kind == "season":
        text(str(detail["year"]) if detail.get("year") else "")
        if detail.get("leaf_count"):
            text(kodi.count("episodes", detail["leaf_count"]))
    elif kind == "episode":
        text(date(detail.get("release_date")))
        text(edition)
        text(runtime(detail.get("duration_ms")))
    else:
        text(str(detail["year"]) if detail.get("year") else "")
        text(edition)
        text(runtime(detail.get("duration_ms")))
    if (detail.get("content_rating") or "").strip():
        parts.append(("badge", detail["content_rating"].strip()))
    if detail.get("rating"):
        parts.append(("rating", rating(detail["rating"])))
    parts += [("quality", b) for b in quality_badges(files[0] if files else None)]
    return parts


def music_meta(detail: dict, children=None) -> list:
    """An artist's or album's line (the web's MusicViews): an artist's
    albums and tracks; an album's year, tracks and length (its artist is
    the line above). As item_meta's pieces."""
    parts = []
    if detail.get("kind") == "artist":
        if detail.get("child_count") is not None:
            parts.append(("text", kodi.count("albums", detail["child_count"])))
        if detail.get("leaf_count") is not None:
            parts.append(("text", kodi.count("tracks", detail["leaf_count"])))
        return parts
    tracks = children or []
    if detail.get("year"):
        parts.append(("text", str(detail["year"])))
    if tracks:
        parts.append(("text", kodi.count("tracks", len(tracks))))
        total = sum(t.get("duration_ms") or 0 for t in tracks)
        if total:
            parts.append(("text", duration(total)))
    if detail.get("rating"):
        parts.append(("rating", rating(detail["rating"])))
    return parts


def meta(item: dict, detail=None) -> dict:
    """The metadata line's parts (HomeMeta): year, content rating, runtime
    (shows: their seasons), rating, badges; those known."""
    detail = detail if detail and detail.get("id") == item.get("id") else None
    kind = item.get("kind")
    if kind in ("show", "season", "artist", "album"):
        length = card_subtitle(item)
    else:
        length = runtime(item.get("duration_ms") or (detail or {}).get("duration_ms"))
    year = item.get("year") or (detail or {}).get("year")
    if kind == "show" and length == str(year or ""):
        length = ""
    files = (detail or {}).get("files") or []
    return {
        "year": str(year) if year else "",
        "content_rating": ((detail or {}).get("content_rating") or "").strip() or None,
        "runtime": length,
        "rating": rating(item.get("rating") or (detail or {}).get("rating")),
        "badges": quality_badges(files[0]) if files else [],
    }
