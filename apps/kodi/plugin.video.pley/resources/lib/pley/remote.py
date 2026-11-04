"""The user's remote, on another of their devices (docs/API.md, "Remote
control"): what Kodi obeys, and what it does with a command. Kodi only
obeys; it shows no remote of its own. The rule is the shared one of
docs/API.md, written afresh here; tests/test_remote.py runs every case of
packages/tv/test/fixtures/remote.json, which every app shares, through it:

- a command for another session is another player's;
- play while playing, or pause while paused, changes nothing but says
  where it is again, so a remote a little behind settles;
- a seek stays within the title, a skip counts from where it is, and
  either without its place does nothing;
- next with nothing next, or previous on a video, does nothing; previous
  on music starts the track again past 3 seconds or on the first track,
  else plays the one before;
- a live channel's next and previous are channel up and down, in the
  order of Kodi's queue of channels, round its ends;
- a command it doesn't know does nothing.

What was played or sought is reported at once (`report`); another track
or channel tells the server with its own session.
"""

from __future__ import annotations

import urllib.parse

import xbmc

from . import kodi

VIDEO = ["play", "pause", "seek", "skip"]
MUSIC = ["play", "pause", "seek", "skip", "next", "previous"]
# Kodi reads a live channel as it comes, with nothing to go back in: only
# channel up and down.
LIVE = ["next", "previous"]
# "Previous" this far into a track starts it again.
RESTART_AFTER_MS = 3000
# What remotes call a device, at most (the server cuts it there too).
NAME_LENGTH = 64

NONE = {"act": "none", "report": False}
REPORT = {"act": "none", "report": True}


def controls(kind: str, has_next: bool = False, has_order: bool = False) -> list:
    """What a player obeys: a video next only when something comes after
    it, a channel up and down only in a queue of channels."""
    if kind == "music":
        return list(MUSIC)
    if kind == "live":
        return list(LIVE) if has_order else []
    return VIDEO + ["next"] if has_next else list(VIDEO)


def _ms(value):
    """A place or an offset in a command: None when missing or not a number."""
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def _within(ms, duration_ms):
    """From the start to the end; past the start only while the length isn't known."""
    start = max(0, ms)
    return min(start, duration_ms) if duration_ms else start


def obey(player: dict, command: dict) -> dict:
    """What a video's or a track's player does with `command`: {act, report},
    with `position_ms` for a seek. `player`: its session_id, state
    ("playing" or "paused"), position_ms, duration_ms (None when unknown),
    has_next, and a track's index in the queue (music only)."""
    if not player.get("session_id") or command.get("session_id") != player["session_id"]:
        return dict(NONE)
    music = "index" in player
    playing = player.get("state") == "playing"
    what = command.get("command")
    if what == "play":
        return dict(REPORT) if playing else {"act": "play", "report": True}
    if what == "pause":
        return {"act": "pause", "report": True} if playing else dict(REPORT)
    if what in ("seek", "skip"):
        ms = _ms(command.get("position_ms" if what == "seek" else "offset_ms"))
        if ms is None:
            return dict(NONE)
        if what == "skip":
            ms += player.get("position_ms") or 0
        return {"act": "seek", "position_ms": _within(ms, player.get("duration_ms")), "report": True}
    if what == "next":
        return {"act": "next", "report": False} if player.get("has_next") else dict(NONE)
    if what == "previous" and music:
        if (player.get("position_ms") or 0) > RESTART_AFTER_MS or not player.get("index"):
            return {"act": "seek", "position_ms": 0, "report": True}
        return {"act": "previous", "report": False}
    return dict(NONE)


def obey_live(player: dict, command: dict) -> dict:
    """What a live channel's player does with `command`: {act, report},
    with `channel` (its id) for a change of channel. `player`: its
    session_id, the channel it plays, and the channels of its queue in
    order (None without one). A channel not in the queue goes to its first."""
    if not player.get("session_id") or command.get("session_id") != player["session_id"]:
        return dict(NONE)
    by = {"next": 1, "previous": -1}.get(command.get("command"))
    channels = player.get("channels") or []
    if by is None or len(channels) < 2:
        return dict(NONE)
    at = player.get("channel")
    to = channels[(channels.index(at) + by) % len(channels)] if at in channels else channels[0]
    if to == at:
        return dict(NONE)
    return {"act": "channel", "channel": to, "report": False}


def entry_params(playlist, at: int) -> dict:
    """The plugin's parameters of the entry at `at` of one of Kodi's
    playlists ({} for anything else's, or out of it)."""
    try:
        path = playlist[at].getPath()
    except (RuntimeError, IndexError, AttributeError, TypeError):
        return {}
    if not path.startswith("plugin://%s/" % kodi.ADDON_ID):
        return {}
    return dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(path).query))


def queue_place(item_id: int):
    """Where title `item_id` is in the queue Kodi plays it from: ("music"
    or "video", its place, how many are queued); None when Kodi plays it
    on its own. Kodi asks the plugin's play route for an entry once it is
    the queue's current one, so this holds from then on."""
    for kind, which in (("music", xbmc.PLAYLIST_MUSIC), ("video", xbmc.PLAYLIST_VIDEO)):
        playlist = xbmc.PlayList(which)
        try:
            at, size = playlist.getposition(), playlist.size()
        except RuntimeError:
            continue
        params = entry_params(playlist, at) if 0 <= at < size else {}
        if params.get("route") == "play" and params.get("id") == str(item_id):
            return kind, at, size
    return None


def device_name():
    """What remotes call this Kodi: its own name (Settings, Services,
    General, Device name), as Kodi says it; None when it says nothing,
    and the server names it from the add-on's User-Agent."""
    name = " ".join((xbmc.getInfoLabel("System.FriendlyName") or "").split())
    return name[:NAME_LENGTH] or None


def device() -> dict:
    """What a play or watch request says of this Kodi for the user's
    remotes. Kodi is made for the TV, wherever it runs (a TV, a box under
    one, a computer by one): remotes list it with the TVs."""
    found = {"device_kind": "tv"}
    name = device_name()
    if name:
        found["device_name"] = name
    return found
