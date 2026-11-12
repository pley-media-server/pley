"""Asking the server to play something, for the plugin's play route (which
hands Kodi the answer) and pley's window (which plays it itself): what this
Kodi plays, the address to play, Kodi's item for it, and what the service
needs to report progress and offer Skip and Up Next; and Kodi's queues of
pley's titles, which the window and a cast from the user's remote play."""

from __future__ import annotations

import json
import os
import threading
import time
from urllib.parse import urlencode

import xbmc
import xbmcgui

from . import api, autoquality, kodi, listing, remote, tracks
from .api import path_in

# The quality setting (docs/QUALITY.md): a quality's id. Before Automatic
# it was an index into another order, under another name.
QUALITY = "video_quality"
OLD_QUALITY = "quality"
# Where Automatic keeps the ceiling of a network found slow, for each server.
CEILING = "auto_ceiling:"

MIME = {
    "mkv": "video/x-matroska",
    "matroska": "video/x-matroska",
    "webm": "video/webm",
    "mp4": "video/mp4",
    "m4v": "video/mp4",
    "mov": "video/quicktime",
    "avi": "video/x-msvideo",
    "ts": "video/mp2t",
    "m2ts": "video/mp2t",
    "mpegts": "video/mp2t",
    "mp3": "audio/mpeg",
    "flac": "audio/flac",
    "m4a": "audio/mp4",
    "ogg": "audio/ogg",
    "opus": "audio/ogg",
    "wav": "audio/wav",
}


def beside(base: str, info: dict, file=None) -> list:
    """The subtitles Kodi is given beside what it plays, named so it reads
    their language and whether they are forced (flagged, or only named so:
    "English (Forced)", "Signs & Songs"): every text track beside a
    converted stream, which carries none; beside a file played as it is,
    the files next to it (Kodi reads the file's own tracks itself, and an
    address over HTTP has no folder for it to look in); and the account's
    PGS subtitles as a `.sup` file where Kodi doesn't find them itself
    (`file`, the file's details, says which). Kodi lists them first, in
    this order (tracks.py)."""
    return [
        path_in(base, api.subtitle_name(s["url"], s.get("language"), tracks.is_forced(s)))
        for s in tracks.beside_tracks(info, file)
    ]


def prepare(
    client: api.Client, base: str, item_id: int, start_ms: int, detail=None, restart=None, kind=None, given=None
) -> xbmcgui.ListItem:
    """Kodi's item for playing `item_id` from `start_ms`, its address set,
    once the server said how. The item's details (for Kodi's info) are
    asked for while the server decides, unless given. Tells the service
    what plays (kodi.PLAYING). `restart` is Automatic's quality when the
    service plays the title again at another one. `kind` is the title's,
    when known without its details (from a listing). `given`: the tracks a
    cast named that the first file has (tracks.given_tracks), played in
    place of the account's choice."""
    started = time.perf_counter()
    kind = (detail or {}).get("kind") or kind
    details = {"item": detail} if detail else {}
    loader = None
    if not detail:
        loader = threading.Thread(target=load_details, args=(client, item_id, details), daemon=True)
        loader.start()
    asked = ask(restart, client)
    request = {
        "item_id": item_id,
        "start_ms": start_ms,
        "client_id": install_id(),
        "caps": report(),
        "max_bitrate": asked["max_bitrate"],
        "max_height": asked["max_height"],
        "mode": asked["mode"],
        "controls": obeys(item_id, kind, tracks.file_of(detail)),
    }
    request.update(remote.device())
    if (given or {}).get("audio") is not None:
        # A cast's audio, of the first file, which plays when none is named.
        request["audio_index"] = given["audio"]
    # The account's subtitles (or a cast's), when they are pictures Kodi
    # can't be given as a file (VobSub, DVB): drawn onto the video if the
    # file is converted (the server draws nothing when Kodi shows them
    # itself). PGS comes as a `.sup` file beside the stream (`beside`).
    burn = tracks.burn_index(tracks.with_given(tracks.file_of(detail), given))
    if burn is not None:
        request.update(file_id=tracks.file_of(detail)["id"], subtitle_index=burn)
    info = client.post("/api/playback", request)
    if loader:
        loader.join(10)
    detail = details.get("item")
    file = tracks.with_given(tracks.file_of(detail, info.get("file_id")), given) if detail else None
    if file is not None and "subtitle_index" not in request and tracks.needs_burn(info, file):
        # The details came with the answer: asked again, with them drawn on.
        request.update(file_id=file["id"], subtitle_index=tracks.chosen_subtitle(file)["index"])
        info = client.post("/api/playback", request)
    url = path_in(base, info["url"])
    duration_ms = info.get("duration_ms") or (detail or {}).get("duration_ms") or 0
    if detail:
        li = listing.list_item(base, detail)
        listing.video_details(li, detail, base)
        if detail["kind"] not in listing.MUSIC:
            # Kodi starts where this says, whatever it was asked: where
            # the server was told to start (none for "from the start").
            li.getVideoInfoTag().setResumePoint(start_ms / 1000.0, duration_ms / 1000.0)
    else:
        li = xbmcgui.ListItem(offscreen=True)
    li.setPath(url)
    if info.get("hls"):
        li.setMimeType("application/vnd.apple.mpegurl")
        li.setContentLookup(False)
    else:
        # Said here, Kodi needn't ask the server for the file's type first.
        mime = file_mime(detail, info.get("file_id"))
        if mime:
            li.setMimeType(mime)
            li.setContentLookup(False)
    subtitles = beside(base, info, file)
    if subtitles:
        li.setSubtitles(subtitles)
    markers = [m for m in (detail or {}).get("markers") or [] if m.get("file_id") == info.get("file_id")]
    playing = {
        "item_id": item_id,
        "file_id": info.get("file_id"),
        "session_id": info["session_id"],
        "duration_ms": duration_ms,
        "path": url,
        # What the service needs for Skip and Up Next, so it asks for none of it again.
        "kind": (detail or {}).get("kind") or kind,
        "markers": markers,
    }
    if (detail or {}).get("kind") not in listing.MUSIC:
        # The account's audio and subtitles, which the service shows in Kodi.
        playing["tracks"] = tracks.layout(info, file)
        # The Quality menu (Kodi's setting) as the file offers it, and its
        # pick, which reports say and the user's remote changes.
        only = info.get("original_only") is True
        playing["qualities"] = qualities(file, only)
        playing["quality"] = autoquality.picked(chosen_quality(), playing["qualities"])
        if given:
            # Played again (another quality, Automatic) with them too.
            playing["given"] = given
        if asked["automatic"] and not only:
            # What the service's Automatic watches the playback with: nothing
            # on a server that converts no video, which has no other quality
            # to step to.
            playing["auto"] = automatic(asked, info, file)
    kodi.set_property(kodi.PLAYING, json.dumps(playing))
    reason = info.get("reason")
    elapsed = (time.perf_counter() - started) * 1000
    kodi.log("playing %d by %s, ready in %d ms%s" % (item_id, info.get("method"), elapsed, ": " + reason if reason else ""))
    return li


def obeys(item_id: int, kind=None, file=None) -> list:
    """What the user's remote may do with this playback (remote.controls):
    a track's, one in Kodi's music queue or known to be music; a video's
    otherwise, with next for an episode (the one after it, as Up Next
    plays) or when Kodi's queue has something after it, audio and
    subtitles when `file` (its details) has tracks of them, and the
    quality. Without the details, which come with the server's answer,
    both tracks: most files have them, and a remote offers only the tracks
    the session lists."""
    place = remote.queue_place(item_id)
    music = (place is not None and place[0] == "music") or kind in listing.MUSIC
    if music:
        return remote.controls("music")
    has_next = (place is not None and place[1] + 1 < place[2]) or kind == "episode"
    audio = subtitles = 1
    if file is not None:
        streams = file.get("streams") or []
        audio = sum(1 for s in streams if s.get("kind") == "audio")
        subtitles = sum(1 for s in streams if s.get("kind") == "subtitle")
    # The quality setting is Kodi's Quality menu: the remote changes it.
    return remote.controls("video", has_next=has_next, audio=audio, subtitles=subtitles, quality=True)


def qualities(file, original_only: bool = False) -> list:
    """The ids of the Quality menu as the file offers it (`file`, its
    details; the whole menu when they couldn't be had), as the server lists
    them to the user's remotes: the original alone on a server that
    converts no video (`original_only` in its play answer)."""
    if original_only:
        return [autoquality.ORIGINAL]
    file = file or {}
    video = next((s for s in file.get("streams") or [] if s.get("kind") == "video"), None) or {}
    width, height = file.get("width") or video.get("width"), file.get("height") or video.get("height")
    return autoquality.menu_for(width, height, file.get("bitrate"))


def file_mime(detail, file_id):
    """The type of a file played as it is, from its container (None when
    unknown, and Kodi asks)."""
    for file in (detail or {}).get("files") or []:
        if file.get("id") == file_id:
            return MIME.get((file.get("container") or "").lower())
    return None


def load_details(client: api.Client, item_id: int, into: dict) -> None:
    try:
        into["item"] = client.get("/api/items/%d" % item_id)
    except api.ApiError as e:
        kodi.log("details of %d: %s" % (item_id, e.message), xbmc.LOGWARNING)


# ------------------------------------------------------------ Kodi's queues


def play_url(item_id: int, **more) -> str:
    """The plugin's play route for `item_id`, which Kodi asks as it comes
    to it; `more`: its kind, and `start`, where it starts whatever Kodi was
    told (route_play)."""
    return "plugin://%s/?%s" % (kodi.ADDON_ID, urlencode(dict(route="play", id=item_id, **more)))


def queue(base: str, items: list, at: int = 0, shuffle: bool = False, music: bool = True, start_ms: int = 0) -> tuple:
    """Kodi's music queue (its video queue, `music` false) holding `items`,
    each through the plugin's play route, which Kodi asks as it comes to
    it, so each is reported as it plays; and where to start in it: at
    `at`, `start_ms` into it, or, shuffled, wherever the shuffle put first
    (the web's audio queue)."""
    playlist = xbmc.PlayList(xbmc.PLAYLIST_MUSIC if music else xbmc.PLAYLIST_VIDEO)
    playlist.clear()
    for place, item in enumerate(items):
        more = {"start": int(start_ms)} if place == at and start_ms > 0 and not shuffle else {}
        playlist.add(play_url(item["id"], **more), listing.list_item(base, item))
    if shuffle:
        playlist.shuffle()
        at = 0
    else:
        playlist.unshuffle()
    return playlist, at


def newest_first(albums: list) -> list:
    """An artist's albums as the web lists them: newest first, then by title."""
    return sorted(albums, key=lambda a: (-(a.get("year") or 0), (a.get("sort_title") or a.get("title") or "").lower()))


def music_tracks(client, item: dict) -> list:
    """An album's tracks, or an artist's, album by album (newest first)."""
    children = lambda id_: client.get("/api/items/%d/children" % id_).get("items", [])  # noqa: E731
    if item.get("kind") == "album":
        return children(item["id"])
    albums = newest_first(children(item["id"]))
    lists = api.together(*[lambda a=a: children(a["id"]) for a in albums]) if albums else []
    return [track for found in lists for track in found or []]


def chosen_quality() -> str:
    """The quality setting's choice. One chosen before Automatic came is
    carried over, once: Original can't be told from the setting never
    changed, and becomes Automatic."""
    old = kodi.setting_int(OLD_QUALITY)
    if old:
        carried = autoquality.from_old_index(old)
        if carried:
            kodi.set_setting(QUALITY, carried)
        kodi.set_setting_int(OLD_QUALITY, 0)
        kodi.log("quality setting carried over: %s" % carried)
    return autoquality.from_setting(kodi.setting(QUALITY))


def ask(restart=None, client=None) -> dict:
    """How the playback is asked for. Original and Automatic let the server
    choose (the file as it is where it plays); a quality from the settings
    caps it, and the server converts a file that is over it (one under it
    plays as it is, as the other apps play a file the quality wouldn't make
    smaller). Automatic under a ceiling known for this network (the
    server's word, asked with `client`, else this Kodi's own memory)
    starts at the quality under it, the same way; the service's restart
    at a quality converts, and at CONVERTED converts the original at the
    server's own quality."""
    if restart == autoquality.CONVERTED:
        return dict(automatic=True, quality=autoquality.ORIGINAL, lowered=False, mode="transcode", max_bitrate=None, max_height=None)
    if restart is not None:
        max_bitrate, max_height = autoquality.limits(restart)
        mode = "auto" if restart == autoquality.ORIGINAL else "transcode"
        return dict(automatic=True, quality=restart, lowered=False, mode=mode, max_bitrate=max_bitrate, max_height=max_height)
    chosen = chosen_quality()
    if chosen != autoquality.AUTO:
        max_bitrate, max_height = autoquality.limits(chosen)
        return dict(automatic=False, quality=chosen, lowered=False, mode="auto", max_bitrate=max_bitrate, max_height=max_height)
    start = autoquality.start_quality(starting_ceiling(client) if client is not None else ceiling())
    max_bitrate, max_height = autoquality.limits(start) if start else (None, None)
    return dict(
        automatic=True,
        quality=start or autoquality.ORIGINAL,
        lowered=start is not None,
        mode="auto",
        max_bitrate=max_bitrate,
        max_height=max_height,
    )


def automatic(asked: dict, info: dict, file) -> dict:
    """What the service's Automatic needs of a playback: the quality it
    plays at (the original when the server played the file as it is,
    under a ceiling it fits), the bitrate that is, the file's bitrate and
    lines, and the server's own quality for the file; whether the video
    is converted, the lines the stream is named by, and whether its
    bitrate changes as it is sent (live_bitrate)."""
    file = file or {}
    video = next((s for s in file.get("streams") or [] if s.get("kind") == "video"), None) or {}
    file_kbps = (file.get("bitrate") or 0) / 1000.0 or None
    file_lines = file.get("height") or video.get("height") or None
    file_named = autoquality.named_lines(file.get("width") or video.get("width"), file_lines)
    quality = asked["quality"]
    converted = info.get("method") == "transcode"
    if not converted:
        quality = autoquality.ORIGINAL
    cap = info.get("auto_quality")
    if quality == autoquality.ORIGINAL:
        playing_kbps = autoquality.kbps_of(cap) if cap and converted else file_kbps
    else:
        playing_kbps = autoquality.kbps_of(quality)
        if file_kbps:
            playing_kbps = min(playing_kbps, file_kbps)
    return {
        "quality": quality,
        "playing_kbps": playing_kbps,
        "file_kbps": file_kbps,
        "file_lines": file_lines,
        "cap": cap,
        "lowered": asked["lowered"] and quality != autoquality.ORIGINAL,
        "server": server_key(),
        "prefix": autoquality.media_prefix(info["url"]),
        "converted": converted,
        "lines": autoquality.stream_lines(quality, converted, cap, file_named),
        "live": converted and info.get("live_bitrate") is True,
    }


def server_key() -> str:
    return kodi.setting("server_id") or kodi.setting("server_url")


def ceiling():
    """The ceiling Automatic remembers for this server, None past 30 minutes."""
    return autoquality.remembered_ceiling(kodi.remembered(CEILING + server_key()), time.time())


def remember_ceiling(server: str, kbps) -> None:
    """Remembered for the next titles in Automatic (None forgets it)."""
    kodi.remember(CEILING + server, autoquality.remember_ceiling(kbps, time.time()) if kbps else "")


NETWORK = "/api/playback/network"
SPEED_TEST = "/api/playback/speedtest"


def starting_ceiling(client: api.Client, clock=time.monotonic):
    """The ceiling Automatic starts under (kbps, None for none): the
    server's word on this Kodi's network, which keeps it a day after a
    step down and a week after a speed test, run here first when the
    server asks for one. Without an answer (out of reach, a server from
    before it), this Kodi's own memory, as before."""
    began = clock()
    answer = network(client)
    if answer is None:
        return ceiling()
    if autoquality.wants_probe(answer):
        result = speed_test(client, began + autoquality.START_BUDGET, clock)
        if result is None:
            # Too little came to tell: as if nothing were known.
            return None
        told = tell_network(client, dict(result))
        return autoquality.network_ceiling(told) if told is not None else autoquality.probe_ceiling(result)
    return autoquality.network_ceiling(answer)


def network(client: api.Client):
    """GET /api/playback/network for this Kodi, within 1.5 s: the answer,
    None on any failure (404 from an older server among them). The
    service's proxy keeps it a while, so a play on the home network waits
    for none of it (proxy.NetworkMemo)."""
    try:
        answer = quick(client, autoquality.NETWORK_TIMEOUT).get(NETWORK, client_id=install_id())
    except (api.ApiError, ValueError) as e:
        kodi.log("network: %s" % e, xbmc.LOGDEBUG)
        return None
    return answer if isinstance(answer, dict) else None


def quick(client, seconds: float):
    """`client`, waiting `seconds` at most for each answer."""
    if not isinstance(client, api.Client):
        return client
    return api.Client(client.base, client.user_agent, client.token, timeout=seconds, local=client.local)


def look_ahead(client: api.Client) -> None:
    """Asks for what the server knows of this network ahead of a play, in
    Automatic: the proxy keeps the answer, and the play doesn't wait."""
    try:
        if chosen_quality() == autoquality.AUTO:
            network(client)
    except Exception as e:  # noqa: BLE001 - a thread of its own, for nothing but speed
        kodi.log("network ahead: %r" % e, xbmc.LOGDEBUG)


def tell_network(client: api.Client, body: dict):
    """PUT /api/playback/network: a speed test's result ({probe_kbps,
    probe_complete}) or Automatic's ceiling ({ceiling_kbps}, None to
    forget). The server's answer, None on any failure (said nowhere)."""
    body = dict(body, client_id=install_id())
    try:
        answer = quick(client, 5).put(NETWORK, body)
    except (api.ApiError, ValueError) as e:
        kodi.log("network not told: %s" % e, xbmc.LOGDEBUG)
        return None
    return answer if isinstance(answer, dict) else None


def tell_ceiling(server: str, kbps, user_agent: str) -> None:
    """Automatic's ceiling for `server` changed (None: forgotten): told to
    the server too, through the service's proxy, so a Kodi on this network
    starts under it for a day, whatever this one remembers. Nothing is said
    when it can't be told; a server from before answers 404."""
    base = kodi.get_property(kodi.PROXY)
    if not base or server != server_key():
        return
    client = api.Client(base, user_agent, timeout=5, local=True)
    tell_network(client, {"ceiling_kbps": int(round(kbps)) if kbps else None})


def picked_automatic(user_agent: str) -> None:
    """Automatic chosen again in the settings: it starts afresh at the
    original, here and on the server."""
    server = server_key()
    remember_ceiling(server, None)
    tell_ceiling(server, None, user_agent)


def speed_test(client: api.Client, deadline: float, clock=time.monotonic):
    """The speed test before a first Automatic play on a network the server
    knows nothing of: {probe_kbps, probe_complete}, or None when too
    little came. Kodi's busy circle shows meanwhile."""
    test = autoquality.SpeedTest()
    xbmc.executebuiltin("ActivateWindow(busydialognocancel)")
    try:
        status = client.measure(SPEED_TEST, {"bytes": autoquality.PROBE_BYTES}, test, deadline, clock)
    finally:
        xbmc.executebuiltin("Dialog.Close(busydialognocancel)")
    result = test.result(clock()) if status == 200 else None
    kodi.log("speed test: %s" % (result or "nothing measured (%d, %d bytes)" % (status, test.size)))
    return result


def install_id() -> str:
    """This Kodi, as pley's sessions know it: the same player from one
    playback to the next."""
    path = os.path.join(kodi.profile_dir(), "install_id")
    try:
        with open(path, encoding="utf-8") as f:
            found = f.read().strip()
            if found:
                return found
    except OSError:
        pass
    made = "kodi-" + os.urandom(16).hex()
    with open(path, "w", encoding="utf-8") as f:
        f.write(made)
    return made


def report() -> dict:
    from . import caps

    settings = {
        name: kodi.setting_value(name)
        for name in ["audiooutput.passthrough", "audiooutput.channels"] + [n for n, _ in caps.PASSTHROUGH]
    }
    screen = (int(xbmc.getInfoLabel("System.ScreenWidth") or 0), int(xbmc.getInfoLabel("System.ScreenHeight") or 0))
    model = "Kodi %s on %s" % (kodi.kodi_version(), kodi.platform())
    return caps.build(xbmc.getInfoLabel("System.SupportedHDRTypes"), settings, screen, model, kodi.is_coreelec())
