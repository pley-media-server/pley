"""Live TV (docs/API.md, "Live TV"): the server's IPTV channels and their
guide, as pley's window and Kodi's lists show them, and how one plays.

A channel plays as the provider sends it, through the server
(`GET …/stream.ts?client_id=`): Kodi reads MPEG-TS itself, deinterlaces
interlaced channels with its own deinterlacer, and passes surround sound
through, so the server converts nothing. The server's live HLS (`POST
…/watch`) is made for players that can't: it converts what isn't H.264,
makes the sound AAC stereo, and starts a few segments behind.

The plugin's play route first asks to watch (`POST …/watch`), which tunes
the channel, says when every connection is in use (409), and gives what is
on for the player's info; then hands Kodi the stream, read for that watch
session (`<client_id>-stream`), which stays while it is read. A device
watches one channel at a time: changing channel lets go of the one before.
The service leaves the session (`DELETE /api/live/sessions/{session}`)
once Kodi stops reading it, rather than the server waiting 45 s.
"""

from __future__ import annotations

import datetime
import json
import re
import time
from urllib.parse import urlencode

import xbmc
import xbmcgui

from . import api, i18n, kodi, remote, words
from .listing import image

SUMMARY = "/api/live"
CHANNELS = "/api/live/channels"
GROUPS = "/api/live/groups"
BROWSE = "/api/live/browse"
GUIDE = "/api/live/guide"
# Channels asked for at a time (the server's own page, 500 at most).
PAGE = 100
# A channel's schedule, asked for this many hours at a time.
SCHEDULE_HOURS = 6
LOGO_WIDTH = 320
PICTURE_WIDTH = 640
MIME = "video/mp2t"
# The plugin's play route for a channel, which Kodi asks as it comes to it
# in the queue of channels (Next and Previous change channel).
PLAY_ROUTE = "live_play"
# Channels queued either side of the one played.
QUEUED = 100

# A language's own clock where Kodi is in another: 24 hours in French and
# Spanish, 12 in English.
CLOCKS = {"en": "%I:%M %p", "fr": "%H:%M", "es": "%H:%M"}


# ------------------------------------------------------------ the server


def summary(client) -> dict:
    """GET /api/live: whether to show Live TV ({available, allowed,
    channels, sources, recent}). {} from a server without Live TV, or one
    that refuses it: no Live TV then. Out of reach, it raises, as the
    calls beside it do."""
    try:
        found = client.get(SUMMARY)
    except api.ApiError as e:
        if e.status == 0:
            raise
        return {}
    return found if isinstance(found, dict) else {}


def available(found: dict) -> bool:
    return bool((found or {}).get("available"))


def groups(client) -> dict:
    """GET /api/live/groups: {total, favorites, recent, groups: [{name,
    region, label, count}]}, the groups in the lineup's order."""
    found = client.get(GROUPS) or {}
    return found if isinstance(found, dict) else {}


def browse(client, category: str, group=None, query=None, offset: int = 0, limit: int = PAGE, around=None) -> dict:
    """A page of a category's channels ("all", "favorites", "recent" or
    "group" of `group`), matching `query` (a name, a number's beginning, or
    what's on): {total, offset, channels}; `around`, the page holding that
    channel. A lineup can have tens of thousands: no list asks for more."""
    asked = {"category": category, "offset": offset, "limit": limit}
    if category == "group":
        asked["group"] = group or ""
    if query:
        asked["q"] = query
    if around is not None:
        asked["around"] = around
        asked.pop("offset")
    found = client.get(BROWSE, **asked) or {}
    return {"total": found.get("total") or 0, "offset": found.get("offset") or 0, "channels": found.get("channels") or []}


def channel(client, channel_id: int) -> dict:
    return client.get("%s/%d" % (CHANNELS, channel_id))


def set_favorite(client, channel_id: int, favorite: bool) -> None:
    path = "%s/%d/favorite" % (CHANNELS, channel_id)
    if favorite:
        client.put(path)
    else:
        client.delete(path)


def watch(client, channel_id: int, client_id: str, take_over=None) -> dict:
    """Tunes the channel for this device (what it watched before ends):
    the channel, with what is on. Raises ApiError, 409 when every
    connection of its source is in use (its `details` say what holds
    each, `connections`); `take_over`, the channel whose connection to take
    then, where the server lets this account. The session is the one the
    user's remote lists and commands (its stream is read for it,
    stream_path): the service is told it (kodi.LIVE)."""
    asked = {"client_id": client_id, "controls": remote.controls("live", has_order=len(queued_channels() or []) > 1)}
    asked.update(remote.device())
    if take_over is not None:
        asked["take_over"] = int(take_over)
    answer = client.post("%s/%d/watch" % (CHANNELS, channel_id), asked) or {}
    if answer.get("session_id"):
        kodi.set_property(kodi.LIVE, json.dumps({"session_id": answer["session_id"], "channel_id": channel_id}))
    return answer.get("channel") or {"id": channel_id, "name": ""}


def stream_path(channel_id: int, client_id: str, take_over=None) -> str:
    """The channel's MPEG-TS as the provider sends it, on the server, read
    for this device's watch session (`<client_id>-stream`), which stays
    while it is read: the remote reaches Kodi through that session."""
    asked = {"client_id": client_id + "-stream", "device_kind": remote.device()["device_kind"]}
    if take_over is not None:
        asked["take_over"] = int(take_over)
    return "%s/%d/stream.ts?%s" % (CHANNELS, channel_id, urlencode(asked))


def schedule(client, channel_id: int, start: float, hours: int = SCHEDULE_HOURS) -> list:
    """The channel's programmes from `start` (the one on then included) over
    `hours`: the guide of that channel alone (`ids`)."""
    begin = int(start)
    guide = client.get(GUIDE, **{"from": begin, "to": begin + hours * 3600, "ids": channel_id}) or {}
    return (guide.get("programs") or {}).get(str(channel_id)) or []


# ------------------------------------------------------------ what's on


def airing(program, now: float) -> bool:
    return bool(program) and program["start"] <= now < program["stop"]


def on_now(channel: dict, now: float):
    """What the channel shows at `now`, if the guide says: its `now`, or
    its `next` once that has begun (the channels were listed a while ago)."""
    for key in ("now", "next"):
        program = channel.get(key)
        if airing(program, now):
            return program
    return None


def up_next(channel: dict, now: float):
    """What comes after what's on now, if the guide says."""
    program = channel.get("next")
    if program and program["start"] >= now:
        return program
    return None


def progress(program: dict, now: float) -> float:
    """How far into `program` `now` is, 0 to 1."""
    length = max(1, program["stop"] - program["start"])
    return min(1.0, max(0.0, (now - program["start"]) / length))


def time_format() -> str:
    """Kodi's time format, without seconds ("%H:%M", "%I:%M %p"), when Kodi
    is in the language shown (its region is the person's); else the
    language's own."""
    language = kodi.language()
    if i18n.supported(kodi.kodi_language()) != language:
        return CLOCKS[language]
    region = getattr(xbmc, "getRegion", None)
    found = region("time") if region else ""
    return (found or "%H:%M").replace(":%S", "").strip()


def clock(ts: float) -> str:
    """"20:00", or "8:00 PM", as Kodi writes times."""
    text = time.strftime(time_format(), time.localtime(ts))
    # Twelve-hour times without the leading zero, as the web writes them.
    return text[1:] if text.startswith("0") and "%I" in time_format() else text


def span(program: dict) -> str:
    """"20:00 – 21:00"."""
    return "%s – %s" % (clock(program["start"]), clock(program["stop"]))


def day(ts: float, now: float) -> str:
    """"Today", "Tomorrow", "Yesterday", or "Fri 30 Oct"."""
    # Dates apart, not 24-hour steps: a day is 23 or 25 hours when daylight
    # saving starts or ends.
    apart = (datetime.date.fromtimestamp(ts) - datetime.date.fromtimestamp(now)).days
    name = {0: "today", 1: "tomorrow", -1: "yesterday"}.get(apart)
    if name:
        return kodi.text(name)
    when = time.localtime(ts)
    return kodi.text("day_in_guide").format(
        weekday=kodi.text_of(kodi.WEEKDAYS[when.tm_wday]), day=when.tm_mday, month=words.month(when.tm_mon)
    )


def left(program: dict, now: float) -> str:
    """"12 min left", "1h 5m left"."""
    return words.time_left((now - program["start"]) * 1000, (program["stop"] - program["start"]) * 1000)


def episode(program: dict) -> str:
    """"S1 · E7", when the guide numbers it."""
    return words.episode_label(program.get("season"), program.get("episode"))


def badges(program: dict) -> list:
    """Live, New and the rating, as the web's programme details show them."""
    found = []
    if program.get("is_live"):
        found.append(kodi.text("live"))
    if program.get("is_new"):
        found.append(kodi.text("new"))
    if program.get("rating"):
        found.append(program["rating"])
    return found


def name(channel: dict, group=None) -> str:
    """"101 Pley News HD", or the name alone; in `group`, without the
    region the group says."""
    number = channel.get("number")
    called = short_name(channel, group)
    return "%s %s" % (number, called) if number else called


def nothing_on(channel: dict) -> str:
    """What a channel without a programme now says (the web's words)."""
    return kodi.text("nothing_on_now") if channel.get("has_guide") else kodi.text("no_guide")


def headline(program: dict) -> str:
    """A programme's title, with its episode's own title: "Morning
    Briefing: Part 7"."""
    title = program.get("title") or ""
    subtitle = program.get("subtitle")
    return kodi.text("label_value") % (title, subtitle) if subtitle and subtitle != title else title


def plot(channel: dict, now: float) -> str:
    """What's on now and next, for Kodi's info on a channel."""
    current = on_now(channel, now)
    later = up_next(channel, now)
    parts = []
    if current:
        # Its title is the item's tagline and second label.
        line = " · ".join(p for p in [left(current, now), episode(current)] + badges(current) if p)
        parts.append("[B]%s[/B]%s" % (span(current), "  " + line if line else ""))
        if current.get("description"):
            parts.append("\n" + current["description"].strip())
    else:
        parts.append(nothing_on(channel))
    if later:
        parts.append("\n[B]%s[/B]  %s  %s" % (kodi.text("next"), clock(later["start"]), headline(later)))
    return "\n".join(parts)


def split_region(text: str):
    """(region, the rest) of a name that begins with one the way providers
    write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the server
    splits group names; None for one that doesn't."""
    text = re.sub(r"^\|+\s*", "", (text or "").strip())
    found = (
        re.match(r"^\[([^\]]{1,16})\]\s*(.+)$", text)
        or re.match(r"^([^|]{1,16})\|\s*(.+)$", text)
        or re.match(r"^([A-Z]{1,4}):\s*(.+)$", text)
    )
    if not found:
        return None
    region, rest = found.group(1).strip(), re.sub(r"^[|:\-\s]+", "", found.group(2)).strip()
    return (region, rest) if region and rest else None


def short_name(channel: dict, group=None) -> str:
    """A channel's name in `group`, without the region the group already
    says ("SKY SPORTS 1" in UK · Sport, not "UK| SKY SPORTS 1"); its whole
    name elsewhere."""
    whole = channel.get("name") or ""
    if not group:
        return whole
    mine, its = split_region(whole), split_region(group)
    return mine[1] if mine and its and mine[0].lower() == its[0].lower() else whole


def group_tree(found: list) -> list:
    """The groups as the web's side lists them (groupSections): the regions
    ("UK" of "UK| Sport", "UK| News"…) in alphabetical order, each opening
    to its groups, then the other groups in the lineup's order, a region of
    one group among them. With fewer than two regions, the groups alone, in
    their order. Entries: {"kind": "region", "region", "groups", "count"}
    and {"kind": "group", "group"}."""
    by_region = {}
    for group in found:
        if group.get("region"):
            by_region.setdefault(group["region"], []).append(group)
    regions = sorted((r for r, groups in by_region.items() if len(groups) > 1), key=lambda r: r.lower())
    if len(regions) < 2:
        return [{"kind": "group", "group": group} for group in found]
    entries = [
        {"kind": "region", "region": r, "groups": by_region[r], "count": sum(g.get("count") or 0 for g in by_region[r])} for r in regions
    ]
    gathered = set(regions)
    return entries + [{"kind": "group", "group": g} for g in found if g.get("region") not in gathered]


def matching_groups(found: list, query: str, most: int = 12) -> list:
    """The groups whose names hold what was searched for (the web's), to
    go to from the results."""
    query = (query or "").strip().lower()
    return [g for g in found if query and query in (g.get("name") or "").lower()][:most]


def group_name(group: dict) -> str:
    """"UK · Sport", or a group's name without a region."""
    return "%s · %s" % (group["region"], group["label"]) if group.get("region") else group.get("label") or group.get("name") or ""


def by_number(found: list, typed: str):
    """The channel numbered `typed`, else the first whose number starts
    with it (the web's number keys); None when none does."""
    exact = next((c for c in found if (c.get("number") or "") == typed), None)
    return exact or next((c for c in found if (c.get("number") or "").startswith(typed)), None)


# ------------------------------------------------------------ Kodi's items


def play_url(channel_id: int, group=None, take_over=None) -> str:
    """The plugin's play route for a channel (opened from `group`: its name
    in the player is the group's; `take_over`: the connection to take)."""
    asked = {"route": PLAY_ROUTE, "id": channel_id}
    if group:
        asked["group"] = group
    if take_over is not None:
        asked["take_over"] = take_over
    return "plugin://%s/?%s" % (kodi.ADDON_ID, urlencode(asked))


def logo(base: str, channel: dict, width: int = LOGO_WIDTH):
    return image(base, channel.get("logo"), width)


def list_item(base: str, channel: dict, now: float, group=None) -> xbmcgui.ListItem:
    """A channel in Kodi's lists, its queue of channels and its player:
    its number and name (in `group`, without the group's region), what's on
    now beside it, now and next in its info, its logo, and the programme's
    picture as fanart. Kodi plays it as a stream without an end: no
    duration, never watched, no resume point."""
    current = on_now(channel, now)
    called = name(channel, group)
    li = xbmcgui.ListItem(called, headline(current) if current else "", offscreen=True)
    art = {}
    found = logo(base, channel)
    if found:
        art.update(thumb=found, icon=found)
    picture = image(base, (current or {}).get("image"), PICTURE_WIDTH)
    if picture:
        art["fanart"] = picture
    if art:
        li.setArt(art)
    tag = li.getVideoInfoTag()
    tag.setMediaType("video")
    tag.setTitle(called)
    tag.setPlot(plot(channel, now))
    # Kodi counts a stream it stopped as played, and keeps it by address:
    # a channel is never watched, nor has a place to resume from.
    tag.setPlaycount(0)
    tag.setResumePoint(0.0, 0.0)
    if current:
        tag.setTagLine(headline(current))
        if current.get("categories"):
            tag.setGenres(list(current["categories"]))
    li.setProperty("IsPlayable", "true")
    return li


def play_item(base: str, channel: dict, client_id: str, now: float, group=None, take_over=None) -> xbmcgui.ListItem:
    """Kodi's item for playing a channel: its stream, said to be MPEG-TS,
    so Kodi asks the server nothing before it plays."""
    li = list_item(base, channel, now, group)
    li.setPath(api.path_in(base, stream_path(channel["id"], client_id, take_over)))
    li.setMimeType(MIME)
    li.setContentLookup(False)
    return li


def queue(base: str, found: list, at: int, now: float, group=None) -> tuple:
    """Kodi's video queue, holding the channels of `found` (those of the
    category the channel was opened from) around the one at `at` (QUEUED
    either side), and where that one is in it: Next and Previous in the
    player change channel within the category."""
    first = max(0, at - QUEUED)
    playlist = xbmc.PlayList(xbmc.PLAYLIST_VIDEO)
    playlist.clear()
    for channel in found[first : at + QUEUED + 1]:
        playlist.add(play_url(channel["id"], group), list_item(base, channel, now, group))
    playlist.unshuffle()
    return playlist, at - first


def is_ours(playlist) -> bool:
    """Whether Kodi's video queue is a queue of pley's channels."""
    try:
        return playlist.size() > 0 and "route=%s" % PLAY_ROUTE in playlist[0].getPath() and kodi.ADDON_ID in playlist[0].getPath()
    except (RuntimeError, IndexError, AttributeError, TypeError):
        return False


def queued_channels():
    """The channels of Kodi's video queue, in order, when it is a queue of
    pley's channels; None otherwise."""
    playlist = xbmc.PlayList(xbmc.PLAYLIST_VIDEO)
    if not is_ours(playlist):
        return None
    found = []
    for at in range(playlist.size()):
        params = remote.entry_params(playlist, at)
        if params.get("route") == PLAY_ROUTE and params.get("id", "").isdigit():
            found.append(int(params["id"]))
    return found


def forget_queue() -> None:
    """Empties Kodi's video queue when it is pley's channels: after a
    channel that can't be watched, Kodi would try each one after it."""
    playlist = xbmc.PlayList(xbmc.PLAYLIST_VIDEO)
    if is_ours(playlist):
        playlist.clear()


def channel_of(path: str):
    """The channel a stream's address (stream_path, or the proxy's address
    of it) plays; None for anything else."""
    found = re.search(r"/api/live/channels/(\d+)/stream\.ts", path or "")
    return int(found.group(1)) if found else None


def taken_over(message) -> bool:
    """Someone took over the connection of the channel that played: the
    server's words under "Live TV stopped", with OK and Watch again: whether
    to watch it again (as any channel is watched: every connection in use
    says so)."""
    body = message or kodi.text("live_stopped_body")
    return bool(xbmcgui.Dialog().yesno(kodi.text("live_stopped"), body, nolabel=kodi.text("ok"), yeslabel=kodi.text("watch_again")))


def who(connection: dict) -> str:
    """Who holds a connection: "You, on another device", the names (told
    to administrators), "Someone else" or "3 people"."""
    if connection.get("yours"):
        return kodi.text("you_elsewhere")
    if connection.get("watchers"):
        return ", ".join(connection["watchers"])
    people = connection.get("people") or 0
    return kodi.count("people", people) if people > 1 else kodi.text("someone_else")


def busy(message: str, connections=()) -> object:
    """Every connection is in use (409): the server's words, then each
    connection with its channel and who holds it. Where this account may
    take one over, Kodi's list offers to watch here instead of it: the
    channel whose connection to take, or None."""
    connections = [c for c in connections or [] if isinstance(c, dict)]
    heading = kodi.text("all_in_use")
    takeable = [c for c in connections if c.get("can_take_over")]
    if not takeable:
        lines = ["%s  ·  %s" % (c.get("channel_name") or "", who(c)) for c in connections]
        xbmcgui.Dialog().ok(heading, "\n".join([message or heading] + ([""] + lines if lines else [])))
        return None
    entries = []
    for c in connections:
        li = xbmcgui.ListItem(c.get("channel_name") or "", offscreen=True)
        line = who(c)
        if c.get("can_take_over"):
            line = "%s  ·  %s" % (line, kodi.text("watch_here"))
        li.setLabel2(line)
        entries.append(li)
    chosen = xbmcgui.Dialog().select(heading, entries, useDetails=True)
    if chosen < 0 or not connections[chosen].get("can_take_over"):
        return None
    return connections[chosen].get("channel_id")
