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
- audio switches to one of the tracks the player can play, and subtitles
  to one of its subtitles or to none (-1); the one playing already is said
  again; a track it can't play, or a command without one, does nothing;
- the quality becomes one of the Quality menu's (Kodi's is the quality
  setting) as picking it there does; the one picked already is said
  again; one the menu doesn't offer for the file does nothing;
- a command it doesn't know does nothing.

What was played, sought or switched to is reported at once (`report`);
another track or channel tells the server with its own session. What a
video did shows for a moment as a notification (`notice`).
"""

from __future__ import annotations

import urllib.parse

import xbmc

from . import autoquality, kodi, words

VIDEO = ["play", "pause", "seek", "skip"]
MUSIC = ["play", "pause", "seek", "skip", "next", "previous"]
# Kodi reads a live channel as it comes, with nothing to go back in: only
# channel up and down.
LIVE = ["next", "previous"]
# The remote's track commands, which name the server's stream index.
TRACKS = ("audio", "subtitles")
# "Previous" this far into a track starts it again.
RESTART_AFTER_MS = 3000
# What remotes call a device, at most (the server cuts it there too).
NAME_LENGTH = 64
# Kodi's own device name, until it is given one.
DEFAULT_NAME = "Kodi"
# How long the notification of what the remote did shows, in ms.
NOTICE_MS = 2000
# Kodi's word for a track of no language it knows, as its menus say it.
UNKNOWN = 13205

NONE = {"act": "none", "report": False}
REPORT = {"act": "none", "report": True}


def controls(
    kind: str, has_next: bool = False, has_order: bool = False, audio: int = 0, subtitles: int = 0, quality: bool = False
) -> list:
    """What a player obeys: a video next only when something comes after
    it, audio and subtitles when its file has tracks of them (`audio` and
    `subtitles` count them), and the quality where it has the Quality menu;
    a channel up and down only in a queue of channels. Music and live
    channels change no track and have no quality."""
    if kind == "music":
        return list(MUSIC)
    if kind == "live":
        return list(LIVE) if has_order else []
    found = VIDEO + ["next"] if has_next else list(VIDEO)
    if audio > 0:
        found.append("audio")
    if subtitles > 0:
        found.append("subtitles")
    if quality:
        found.append("quality")
    return found


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


def obey_track(player: dict, command: dict) -> dict:
    """What a video's player does with the remote's `audio` or `subtitles`:
    {act, report}, with `stream_index` (the server's, -1 for no subtitles)
    for a track to switch to. `player`: its session_id, the server's indexes
    of the audio tracks and the subtitles it can switch to, and of those it
    plays (audio_index None when not known; subtitle_index -1 for none)."""
    if not player.get("session_id") or command.get("session_id") != player["session_id"]:
        return dict(NONE)
    index = command.get("stream_index")
    if not isinstance(index, int) or isinstance(index, bool):
        return dict(NONE)
    what = command.get("command")
    if what == "audio":
        if index not in (player.get("audio") or []):
            return dict(NONE)
        playing = player.get("audio_index")
    elif what == "subtitles":
        if index != -1 and index not in (player.get("subtitles") or []):
            return dict(NONE)
        playing = player.get("subtitle_index")
    else:
        return dict(NONE)
    if index == playing:
        return dict(REPORT)
    return {"act": what, "stream_index": index, "report": True}


def obey_quality(player: dict, command: dict) -> dict:
    """What a video's player does with the remote's `quality`: {act,
    report}, with `quality` (one of the menu's ids) for a quality to pick.
    `player`: its session_id, the ids its Quality menu offers for the file
    (`qualities`) and the one it has picked (`quality`)."""
    if not player.get("session_id") or command.get("session_id") != player["session_id"]:
        return dict(NONE)
    wanted = command.get("quality")
    if command.get("command") != "quality" or not isinstance(wanted, str) or wanted not in (player.get("qualities") or []):
        return dict(NONE)
    if wanted == player.get("quality"):
        return dict(REPORT)
    return {"act": "quality", "quality": wanted, "report": True}


def notice(command: dict, action: dict, text) -> str:
    """What Kodi says for a moment of a command it obeyed on a video
    (`action`, as obey, obey_track or obey_quality gives it for `command`):
    "Paused", "Playing", the skip asked for in whole seconds ("+30 s",
    "−10 s"), where a seek went ("Jumped to 12:34"), the audio or subtitles
    switched to, named by Kodi's own menus' words (`track`, Kodi's entry
    for it, which tracks.Follower.obey adds), "Subtitles off", or the
    quality picked as the Quality setting names it. "" for a command that
    changes nothing, and for next, whose title Kodi shows as it plays.
    `text(name)`: the add-on's words."""
    act = action.get("act")
    if act in ("pause", "play"):
        return text("remote_paused" if act == "pause" else "remote_playing")
    if act == "seek" and command.get("command") == "seek":
        return text("remote_jumped") % (words.clock(action["position_ms"]) or "0:00")
    if act == "seek" and command.get("command") == "skip":
        offset = _ms(command.get("offset_ms")) or 0
        if not offset:
            return ""
        # Rounded, and never 0 for a step there is, as Android TV's seek badge.
        seconds = max(1, int((abs(offset) + 500) // 1000))
        return text("remote_back" if offset < 0 else "remote_ahead") % seconds
    if act == "subtitles" and action.get("stream_index") == -1:
        return text("remote_subtitles_off")
    if act in TRACKS and action.get("track") is not None:
        return text("remote_" + act) % track_name(action["track"])
    if act == "quality":
        return text("remote_quality") % autoquality.label(action["quality"])
    return ""


def track_name(entry: dict) -> str:
    """A track of Kodi's player by the language and title Kodi's own Audio
    and Subtitles menus name it by (`entry`, as Player.GetProperties lists
    it): "French - Commentary", "French" without a title, Kodi's "Unknown"
    for a language it doesn't know."""
    code = (entry.get("language") or "").strip()
    convert = getattr(xbmc, "convertLanguage", None)
    language = (convert(code, getattr(xbmc, "ENGLISH_NAME", None)) if convert and code else "") or code
    if not language:
        found = getattr(xbmc, "getLocalizedString", None)
        language = (found(UNKNOWN) if found else "") or "Unknown"
    name = (entry.get("name") or "").strip()
    return "%s - %s" % (language, name) if name else language


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
    """What remotes call this Kodi: the name it was given (Settings,
    Services, General, Device name), at most NAME_LENGTH characters. None
    while it has Kodi's own, which Kodi says with its computer's host name
    added (System.FriendlyName, "Kodi (<host>)"), not for remotes and
    administrators to see: the server then names it from the add-on's
    User-Agent."""
    given = kodi.setting_value("services.devicename")
    name = " ".join(given.split()) if isinstance(given, str) else ""
    if not name or name == DEFAULT_NAME:
        return None
    return name[:NAME_LENGTH]


def device() -> dict:
    """What a play or watch request says of this Kodi for the user's
    remotes. Kodi is made for the TV, wherever it runs (a TV, a box under
    one, a computer by one): remotes list it with the TVs."""
    found = {"device_kind": "tv"}
    name = device_name()
    if name:
        found["device_name"] = name
    return found
