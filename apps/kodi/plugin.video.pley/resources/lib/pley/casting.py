"""Casts from the user's remote, on another of their devices (docs/API.md,
"Casting"): the service's event stream makes this Kodi one the remote
plays things on, and the service plays what it is sent as the add-on's own
Play plays it, through the plugin's play routes, in place of what plays,
saying where it came from. The rule is the one every app follows:

- a cast for another receiver is another device's, and one that comes
  while Kodi can't take it (signed out, or "Who's watching?" on screen,
  whose PINs a remote mustn't play past) does nothing;
- a movie or an episode plays from `start_ms`, else where Play starts it
  (its resume point), with the audio and subtitles the remote picked on
  its page in place of the account's choice for that play, where the file
  it plays (its first, as Play plays) has them; a show or a season its
  next episode, as its Play does; an album or an artist its tracks as
  Kodi's music queue, shuffled with `shuffle`; a track its album from it,
  as the album's page plays it;
  a playlist what it holds, music as Kodi's music queue and titles as its
  video queue; a channel live, the channels around it in Kodi's queue of
  channels, as Live TV plays one;
- "Playing from {device}" shows as it starts ("another device" when the
  remote has no name), "Nothing to play" when there is nothing.
"""

from __future__ import annotations

import time

from . import kodi, listing, livetv, playback, remote, tracks, words

# How long "Playing from …" shows.
BANNER_MS = 4000
NONE = {"act": "none"}
# What playing a cast came to (play()).
PLAYED, NOTHING, DROPPED = "played", "nothing", "dropped"


def receiver_query(receiver: str, live: bool) -> dict:
    """The event stream's query that makes this Kodi a receiver: its own id
    (the `client_id` its plays are made with), a TV named as its plays name
    it, taking titles and playlists always, and channels where the account
    has Live TV."""
    found = {"receiver": receiver}
    found.update(remote.device())
    found["casts"] = "item,live" if live else "item"
    return found


def _id(value):
    """An id in a cast: None when missing or not a whole number."""
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def action(receiver: str, taking: bool, cast: dict) -> dict:
    """What a cast event asks of this Kodi (`receiver`, its id): {act:
    "none"}, or {act: "item", item_id, start_ms, shuffle, audio_index,
    subtitle_index}, {act: "playlist", playlist_id, shuffle} or {act:
    "channel", channel_id}. The tracks are those the remote picked (the
    server's stream indexes, subtitles -1 for none), None when not named.
    `taking`: whether Kodi takes casts now (signed in, and "Who's
    watching?" not on screen)."""
    if not taking or not receiver or cast.get("receiver_id") != receiver:
        return dict(NONE)
    shuffle = cast.get("shuffle") is True
    if _id(cast.get("item_id")) is not None:
        start = cast.get("start_ms")
        start = max(0, int(start)) if isinstance(start, (int, float)) and not isinstance(start, bool) else None
        audio, subtitle = _id(cast.get("audio_index")), _id(cast.get("subtitle_index"))
        return {
            "act": "item",
            "item_id": cast["item_id"],
            "start_ms": start,
            "shuffle": shuffle,
            "audio_index": audio if audio is not None and audio >= 0 else None,
            "subtitle_index": subtitle if subtitle is not None and subtitle >= -1 else None,
        }
    if _id(cast.get("playlist_id")) is not None:
        return {"act": "playlist", "playlist_id": cast["playlist_id"], "shuffle": shuffle}
    if _id(cast.get("channel_id")) is not None:
        return {"act": "channel", "channel_id": cast["channel_id"]}
    return dict(NONE)


def banner(device) -> str:
    """"Playing from {device}", as the cast starts: the remote's name, on
    one line and no longer than remotes show one; "another device" when it
    has none."""
    name = " ".join(device.split())[: remote.NAME_LENGTH] if isinstance(device, str) else ""
    return kodi.text("playing_from") % name if name else kodi.text("playing_from_another")


def plays(kind) -> str:
    """What a title cast here plays, by its kind, as its page's Play does:
    "video" (a movie, an episode, an extra), "next_episode" (a show, a
    season), "album", "artist", "track" (its album from it), or "nothing"
    (a kind a newer server knows)."""
    if kind in ("movie", "episode", "extra"):
        return "video"
    if kind in ("show", "season"):
        return "next_episode"
    if kind in ("album", "artist", "track"):
        return kind
    return "nothing"


def play(client, base: str, cast: dict, player, current=lambda: True) -> str:
    """Plays `cast` (action()) on Kodi's `player`, in place of what plays,
    asking the server what it needs with `client` (through the service's
    proxy at `base`, which list items' artwork names too): PLAYED, NOTHING,
    or DROPPED when it no longer may (`current()` false: another cast came
    meanwhile, or Kodi stopped taking them). Raises api.ApiError."""
    if cast["act"] == "channel":
        return channel(client, base, cast["channel_id"], player, current)
    if cast["act"] == "playlist":
        found = client.get("/api/playlists/%d" % cast["playlist_id"], summaries=1) or {}
        if not current():
            return DROPPED
        items = [e["item"] for e in found.get("entries") or [] if (e.get("item") or {}).get("kind") in listing.PLAYABLE]
        if not items:
            return NOTHING
        music = found.get("kind") == "audio"
        queue, at = playback.queue(base, items, shuffle=cast["shuffle"], music=music)
        player.play(queue, startpos=at)
        return PLAYED
    detail = client.get("/api/items/%d" % cast["item_id"]) or {}
    if not current():
        return DROPPED
    what = plays(detail.get("kind"))
    start = cast["start_ms"]
    # The tracks picked on the remote's page, those of the file Play plays.
    given = tracks.given_tracks(tracks.file_of(detail), cast.get("audio_index"), cast.get("subtitle_index"))
    more = {name: value for name, value in given.items() if value is not None}
    if what == "next_episode":
        detail = (client.get("/api/items/%d/on-deck" % detail["id"]) or {}).get("item")
        if not current():
            return DROPPED
        if not detail:
            return NOTHING
        # Where the show's Play starts it, with the account's tracks.
        what, start, more = "video", None, {}
    if what == "video":
        if start is None:
            # As Play starts it: where it was left, if it was.
            start = (words.state(detail).get("position_ms") or 0) if words.in_progress(detail) else 0
        player.play(playback.play_url(detail["id"], kind=detail["kind"], start=int(start), **more))
        return PLAYED
    if what in ("album", "artist"):
        found = playback.music_tracks(client, detail)
        if not current():
            return DROPPED
        if not found:
            return NOTHING
        queue, at = playback.queue(base, found, shuffle=cast["shuffle"])
        player.play(queue, startpos=at)
        return PLAYED
    if what == "track":
        album = []
        if _id(detail.get("parent_id")) is not None:
            album = client.get("/api/items/%d/children" % detail["parent_id"]).get("items", [])
            if not current():
                return DROPPED
        album = [t for t in album if t.get("kind") == "track"]
        at = next((i for i, t in enumerate(album) if t.get("id") == detail.get("id")), None)
        if at is None:
            album, at = [detail], 0
        queue, at = playback.queue(base, album, at, start_ms=start or 0)
        player.play(queue, startpos=at)
        return PLAYED
    return NOTHING


def channel(client, base: str, channel_id: int, player, current) -> str:
    """A channel cast here plays as Live TV plays one: in Kodi's queue of
    channels, the lineup's around it, so channel up and down go through
    them."""
    page = livetv.browse(client, "all", around=channel_id)
    if not current():
        return DROPPED
    channels = page["channels"]
    at = next((i for i, c in enumerate(channels) if c.get("id") == channel_id), None)
    if at is None:
        # Not among the lineup's (hidden since): on its own.
        channels, at = [livetv.channel(client, channel_id)], 0
        if not current():
            return DROPPED
    queue, start = livetv.queue(base, channels, at, time.time())
    player.play(queue, startpos=start)
    return PLAYED
