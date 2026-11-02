"""Audio and subtitles: the account's choice shown when a file starts in
Kodi, a change made in Kodi's player saved for the show or the movie, and
the account's defaults (docs/API.md, Your account).

pley names tracks by their stream index in the file (sidecar subtitle files
from 1000); Kodi by their place in its lists. Kodi's audio streams are the
file's, in its order (a converted stream has only the one the server put
in). Its subtitles are the files it was given beside what plays
(ListItem.setSubtitles), in that order, then the file's own, in its order:
Kodi adds the files as it opens the address, before it reads the streams
(CVideoPlayer::OpenInputStream, then OpenDemuxStream), and lists every
stream in the order it was added. Captions it finds in the video come
after them all.
"""

from __future__ import annotations

import re
import threading
import time

# Kodi's video player, in JSON-RPC.
VIDEO_PLAYER = 1
PROPERTIES = ["audiostreams", "currentaudiostream", "subtitles", "currentsubtitle", "subtitleenabled"]
# Turns of the service's loop (one a second) to wait for Kodi to list
# every stream before the choice is shown with what it lists.
READY_TURNS = 8
# Seconds after the choice is shown in which what Kodi plays is taken as
# where it starts (Kodi switches streams a moment after it is asked).
QUIET = 3
# Seconds a change made in Kodi's player holds before it is saved: choosing
# a subtitle and turning them on are two changes.
SETTLE = 2

# The languages offered for the defaults, as ISO 639-2/B codes (the TV
# apps' and the Android app's 34).
LANGUAGES = [
    ("ara", "Arabic"), ("bul", "Bulgarian"), ("cat", "Catalan"), ("chi", "Chinese"), ("hrv", "Croatian"),
    ("cze", "Czech"), ("dan", "Danish"), ("dut", "Dutch"), ("eng", "English"), ("fin", "Finnish"),
    ("fre", "French"), ("ger", "German"), ("gre", "Greek"), ("heb", "Hebrew"), ("hin", "Hindi"),
    ("hun", "Hungarian"), ("ind", "Indonesian"), ("ita", "Italian"), ("jpn", "Japanese"), ("kor", "Korean"),
    ("nor", "Norwegian"), ("per", "Persian"), ("pol", "Polish"), ("por", "Portuguese"), ("rum", "Romanian"),
    ("rus", "Russian"), ("srp", "Serbian"), ("slo", "Slovak"), ("spa", "Spanish"), ("swe", "Swedish"),
    ("tha", "Thai"), ("tur", "Turkish"), ("ukr", "Ukrainian"), ("vie", "Vietnamese"),
]
# subtitle_mode, and the name of its words (the TV apps' SUBTITLE_MODES).
SUBTITLE_MODES = [
    ("off", "subtitles_off"),
    ("forced", "subtitles_forced"),
    ("foreign", "subtitles_foreign"),
    ("always", "subtitles_always"),
]
# Modes that show full subtitles, in a language of their own.
WITH_LANGUAGE = ("foreign", "always")
# A server that doesn't let apps change the defaults.
REFUSED = (403, 404, 405)

# ISO 639-2/T codes as Kodi names them (639-2/B), where they differ.
B_CODES = {
    "fra": "fre", "deu": "ger", "zho": "chi", "ces": "cze", "nld": "dut", "ell": "gre", "fas": "per",
    "ron": "rum", "slk": "slo", "sqi": "alb", "hye": "arm", "eus": "baq", "mya": "bur", "kat": "geo",
    "isl": "ice", "mkd": "mac", "msa": "may", "mri": "mao", "bod": "tib", "cym": "wel",
}


# ------------------------------------------------------------ the choice


def is_picture(stream) -> bool:
    """Subtitles shown as pictures (PGS, VobSub, DVB)."""
    return bool(stream) and stream.get("text_based") is False


def is_pgs(stream) -> bool:
    """PGS, which Kodi draws from a `.sup` file beside what plays (the
    server makes one of any PGS track: /api/files/{id}/subtitles/{n}.sup).
    VobSub and DVB have no file of their own Kodi reads over HTTP."""
    return is_picture(stream) and stream.get("codec") == "hdmv_pgs_subtitle"


def gives_sup(info: dict) -> bool:
    """Whether the server answering `info` makes `.sup` files of PGS tracks:
    it says how its streams are timed (`pts_offset_ms`), which came with
    them. An older server draws PGS onto converted video instead."""
    return "pts_offset_ms" in (info or {})


def file_of(detail, file_id=None):
    """The item's file `file_id`, else the first (the one the server plays
    when asked for none)."""
    files = (detail or {}).get("files") or []
    if file_id is None:
        return files[0] if files else None
    return next((file for file in files if file.get("id") == file_id), None)


def chosen_subtitle(file):
    """The subtitles the account's choice picks in `file` (None: off)."""
    wanted = (file or {}).get("selected_subtitle")
    if wanted is None:
        return None
    for stream in file.get("streams") or []:
        if stream.get("kind") == "subtitle" and stream.get("index") == wanted:
            return stream
    return None


def burn_index(file):
    """The subtitles to ask the server to draw onto the video when the file
    is converted: the account's, when they are pictures other than PGS
    (Kodi shows the file's own pictures when it plays as it is, and the
    server then draws nothing; PGS it draws from a `.sup` file beside a
    stream too)."""
    chosen = chosen_subtitle(file)
    return chosen["index"] if is_picture(chosen) and not is_pgs(chosen) else None


def needs_burn(info: dict, file) -> bool:
    """Whether the session must be asked for again with the account's
    picture subtitles drawn on: it streams (whose segments carry no
    subtitles), or they are a file next to the media, which Kodi isn't
    given; and they aren't drawn, nor PGS the server makes a `.sup` of."""
    chosen = chosen_subtitle(file)
    if not is_picture(chosen) or info.get("subtitle_index") == chosen["index"]:
        return False
    if is_pgs(chosen) and gives_sup(info):
        return False
    return bool(info.get("hls") or chosen.get("external"))


def is_forced(stream) -> bool:
    """Forced subtitles, as the server counts them: flagged so, or named so
    ("English (Forced)", "Signs & Songs"), as many files only name them."""
    if stream.get("forced"):
        return True
    words = re.split(r"[^0-9a-z]+", (stream.get("title") or "").lower())
    return "forced" in words or "signs" in words


def beside_tracks(info: dict, file=None) -> list:
    """The subtitle tracks Kodi is given as files beside what plays: every
    text track beside a converted stream, which carries none; the text
    files next to the media beside a file played as it is; and the
    account's PGS subtitles where they aren't in what plays (a stream, a
    file next to the media), as a `.sup` file (`url` set to it), unless
    they are drawn onto the video. Only those: Kodi reads every file it is
    given as it opens, and the server reads the whole media file for a
    track whose `.sup` it hasn't made yet."""
    chosen = chosen_subtitle(file)
    pgs = chosen["index"] if is_pgs(chosen) and gives_sup(info) and not info.get("burn_subtitles") else None
    beside = []
    for s in info.get("subtitles") or []:
        if not (info.get("hls") or s.get("external")):
            continue
        if s.get("url"):
            beside.append(s)
        elif s.get("index") == pgs:
            beside.append(dict(s, url="/api/files/%s/subtitles/%d.sup" % (info.get("file_id"), pgs)))
    return beside


def layout(info: dict, file) -> dict:
    """What the service needs to show the account's choice in Kodi and to
    save a change: the tracks in Kodi's order, and the choice (`file`, the
    file's details, None when they couldn't be had: Kodi's own choice
    then stays). The audio is the server's choice for this Kodi (of the
    profile's language, the track it plays best), the details' only from
    a server too old to say."""
    hls = bool(info.get("hls"))
    # Forced as Kodi sees them: the files given by their names (playback.beside),
    # the file's own by their flag.
    subtitles = [dict(s, forced=is_forced(s)) for s in beside_tracks(info, file)]
    if not hls:
        subtitles += sorted((s for s in info.get("subtitles") or [] if not s.get("external")), key=lambda s: s["index"])
    audio = []
    if file and not hls:
        audio = sorted((s for s in file.get("streams") or [] if s.get("kind") == "audio"), key=lambda s: s["index"])
    audio_index = info.get("audio_index")
    if audio_index is None and file:
        audio_index = file.get("selected_audio")
    return {
        "hls": hls,
        "file_id": info.get("file_id"),
        "audio": [{"index": s["index"], "language": s.get("language")} for s in audio],
        "subtitles": [{"index": s["index"], "language": s.get("language"), "forced": bool(s.get("forced"))} for s in subtitles],
        "audio_index": audio_index,
        "known": file is not None,
        "subtitle_index": (file or {}).get("selected_subtitle"),
        "burned": info.get("subtitle_index") if info.get("burn_subtitles") else None,
    }


# ------------------------------------------------------ pley's and Kodi's


def language(code) -> str:
    code = (code or "").strip().lower()
    return B_CODES.get(code, code)


def to_kodi(entries: list, index, found: list):
    """Where Kodi lists pley's track `index` (its place in `entries`, pley's
    tracks in Kodi's order), given what Kodi lists (`found`); None when it
    isn't there. Should Kodi list fewer than expected, the one in the same
    language (and as forced) when only one is."""
    at = next((i for i, e in enumerate(entries) if e["index"] == index), None)
    if at is None:
        return None
    if len(found) >= len(entries):
        return at
    place = _matched(at, _pairs(entries, "forced"), _pairs(found, "isforced"))
    return found[place].get("index", place) if place is not None else None


def from_kodi(entries: list, position, found: list):
    """pley's track at Kodi's place `position` (None for one pley doesn't
    know, as captions Kodi found in the video)."""
    if position is None or position < 0:
        return None
    if len(found) >= len(entries):
        return entries[position]["index"] if position < len(entries) else None
    at = next((i for i, s in enumerate(found) if s.get("index", i) == position), None)
    if at is None:
        return None
    place = _matched(at, _pairs(found, "isforced"), _pairs(entries, "forced"))
    return entries[place]["index"] if place is not None else None


def _pairs(streams: list, forced: str) -> list:
    return [(language(s.get("language")), bool(s.get(forced))) for s in streams]


def _matched(at: int, ours: list, theirs: list):
    """The place in `theirs` of the one stream like `ours[at]`, (language,
    forced) pairs: in its language, and as forced when that tells them
    apart; on both sides the only one so, else None."""

    def like(among, code, forced):
        same = [i for i, (other, _) in enumerate(among) if other == code]
        if len(same) > 1:
            same = [i for i in same if among[i][1] == forced]
        return same

    code, forced = ours[at]
    if not code or like(ours, code, forced) != [at]:
        return None
    same = like(theirs, code, forced)
    return same[0] if len(same) == 1 else None


def current(props: dict) -> tuple:
    """What Kodi plays: (audio's place, subtitles' place, None when off)."""
    audio = (props.get("currentaudiostream") or {}).get("index")
    subtitle = (props.get("currentsubtitle") or {}).get("index") if props.get("subtitleenabled") else None
    return audio, subtitle


class Follower:
    """One playback's tracks: the account's choice shown once Kodi lists the
    streams, and changes made in Kodi's player saved. `rpc(method, params)`
    is Kodi's JSON-RPC (its result, None on failure); `save(body)` sends
    PUT /api/items/{id}/streams's body. Turned from the service's loop."""

    def __init__(self, layout: dict, rpc, save, clock=time.monotonic):
        self.layout = layout
        self.rpc = rpc
        self.save = save
        self.clock = clock
        self.lock = threading.Lock()
        self.shown = False
        self.waited = 0
        self.quiet_until = 0.0
        self.baseline = None
        self.pending = None
        self.pending_since = 0.0
        # Whether a turn saw Kodi play audio: none playing after that is a
        # player stopping or between files, also where the layout lists no
        # audio (a converted stream's).
        self.had_audio = False
        # Off once Kodi lists a track the layout doesn't know (subtitles
        # added while it plays): its places no longer say pley's tracks.
        self.following = True

    def turn(self) -> None:
        # Kodi's callback and the loop may both turn it: one at a time, the
        # other passes.
        if not self.lock.acquire(blocking=False):
            return
        try:
            props = self.rpc("Player.GetProperties", {"playerid": VIDEO_PLAYER, "properties": PROPERTIES})
            if isinstance(props, dict):
                self._turn(props)
        finally:
            self.lock.release()

    def _turn(self, props: dict) -> None:
        if not self.following:
            return
        now = self.clock()
        if not self.shown:
            if self.show(props):
                self.shown = True
                self.quiet_until = now + QUIET
            return
        state = current(props)
        if state[0] is None and (props.get("audiostreams") or self.layout["audio"] or self.had_audio):
            # A player between files, or stopping: nothing to go by.
            return
        self.had_audio = self.had_audio or state[0] is not None
        if now < self.quiet_until or self.baseline is None:
            # Where it starts, the account's choice shown.
            self.baseline = state
            return
        if state == self.baseline:
            self.pending = None
            return
        if self.pending is None or self.pending[0] != state:
            self.pending = (state, self.body(state, props))
            self.pending_since = now
            return
        if now - self.pending_since >= SETTLE:
            self._flush()

    def finish(self) -> None:
        """The playback ended: a change that held SETTLE seconds, which no
        turn came to save, is saved. One still settling isn't, nor one that
        turned subtitles off: Kodi turns them off as it stops, and the last
        turn may have seen that."""
        if self.lock.acquire(timeout=1):
            try:
                if self.pending is not None:
                    state, _ = self.pending
                    # Subtitles off all along are no part of the change: audio
                    # changed with them off is saved.
                    turned_off = state[1] is None and self.baseline is not None and self.baseline[1] is not None
                    if not turned_off and self.clock() - self.pending_since >= SETTLE:
                        self._flush()
                    self.pending = None
            finally:
                self.lock.release()

    def _flush(self) -> None:
        if self.pending is None:
            return
        state, body = self.pending
        self.pending = None
        self.baseline = state
        if body is not None:
            self.save(body)

    def show(self, props: dict) -> bool:
        """Switches Kodi to the account's choice: whether it was done (or
        there was nothing to do), False to try again on the next turn while
        Kodi hasn't listed every stream yet."""
        lay = self.layout
        audios = props.get("audiostreams") or []
        subtitles = props.get("subtitles") or []
        ready = len(audios) >= len(lay["audio"]) and len(subtitles) >= len(lay["subtitles"])
        if not ready and self.waited < READY_TURNS:
            self.waited += 1
            return False
        playing_audio, playing_subtitle = current(props)
        if lay["audio"] and lay["audio_index"] is not None:
            # A converted stream has only the audio the server put in.
            at = to_kodi(lay["audio"], lay["audio_index"], audios)
            if at is not None and at != playing_audio:
                self.rpc("Player.SetAudioStream", {"playerid": VIDEO_PLAYER, "stream": at})
        if lay["burned"] is not None:
            # Drawn onto the video: nothing for Kodi to show over them.
            at = None
        elif not lay["known"]:
            return True
        elif lay["subtitle_index"] is None:
            at = None
        else:
            # Pictures in a stream that weren't drawn on, or a track Kodi
            # doesn't list: none rather than Kodi's own.
            at = to_kodi(lay["subtitles"], lay["subtitle_index"], subtitles)
        if at is None:
            if props.get("subtitleenabled"):
                self.rpc("Player.SetSubtitle", {"playerid": VIDEO_PLAYER, "subtitle": "off"})
        elif at != playing_subtitle:
            self.rpc("Player.SetSubtitle", {"playerid": VIDEO_PLAYER, "subtitle": at, "enable": True})
        return True

    def body(self, state: tuple, props: dict):
        """PUT /api/items/{id}/streams's body for what Kodi plays (None for a
        subtitle pley doesn't know, which isn't saved). The audio goes only
        when it was changed: the track playing was the server's choice for
        this Kodi, and another device may play another of that language
        better, so subtitles changed alone keep the audio picked before."""
        lay = self.layout
        audio_at, subtitle_at = state
        subtitle = None
        if subtitle_at is not None:
            subtitle = from_kodi(lay["subtitles"], subtitle_at, props.get("subtitles") or [])
            if subtitle is None:
                return None
        body = {"file_id": lay["file_id"], "subtitle_index": subtitle}
        if self.baseline is None or audio_at != self.baseline[0]:
            audio = None
            if lay["audio"]:
                audio = from_kodi(lay["audio"], audio_at, props.get("audiostreams") or [])
            body["audio_index"] = lay["audio_index"] if audio is None else audio
        return body


# ----------------------------------------------------------- the defaults


def language_name(code) -> str | None:
    if not code:
        return None
    return next((name for c, name in LANGUAGES if c == code), code)


def mode_word(mode) -> str:
    return next((word for m, word in SUBTITLE_MODES if m == mode), "subtitles_forced")


def rows(me: dict, text) -> list:
    """The defaults' rows, (what, label): Audio language, Subtitles, and the
    subtitle language when the mode shows full subtitles. `text(name)` is
    the add-on's words."""
    mode = me.get("subtitle_mode") or "forced"
    shown = [
        ("audio", "%s: %s" % (text("audio_language"), language_name(me.get("audio_language")) or text("files_default"))),
        ("mode", "%s: %s" % (text("subtitles"), text(mode_word(mode)))),
    ]
    if mode in WITH_LANGUAGE:
        name = language_name(me.get("subtitle_language")) or text("same_as_audio")
        shown.append(("subtitle", "%s: %s" % (text("subtitle_language"), name)))
    return shown


def choices(what: str, me: dict, text) -> tuple:
    """A row's choices: (heading, [(value, label)], the current one's place)."""
    if what == "mode":
        options = [(m, text(word)) for m, word in SUBTITLE_MODES]
        now = me.get("subtitle_mode") or "forced"
        heading = text("subtitles")
    else:
        none = text("files_default") if what == "audio" else text("same_as_audio")
        options = [(None, none)] + list(LANGUAGES)
        now = me.get("audio_language" if what == "audio" else "subtitle_language")
        heading = text("audio_language" if what == "audio" else "subtitle_language")
    at = next((i for i, (value, _) in enumerate(options) if value == now), 0)
    return heading, options, at


def changed(me: dict, what: str, value) -> dict:
    """PUT /api/me/track-defaults's body with one of them changed (all three
    are sent)."""
    body = {
        "audio_language": me.get("audio_language"),
        "subtitle_language": me.get("subtitle_language"),
        "subtitle_mode": me.get("subtitle_mode") or "forced",
    }
    body[{"audio": "audio_language", "subtitle": "subtitle_language", "mode": "subtitle_mode"}[what]] = value
    return body


def edit_defaults(client, me: dict, select, say, text, error_type) -> dict:
    """Audio and subtitles, as the TV apps' Settings rows: a row, then its
    choice, each saved at once for the profile on all its devices, until
    Back. `select(heading, labels, preselect)` is Kodi's list dialog,
    `say(words, error)` tells the person, `error_type` is api.ApiError.
    The profile as it is now."""
    while True:
        shown = rows(me, text)
        picked = select(text("tracks"), [label for _, label in shown], -1)
        if picked < 0:
            return me
        what = shown[picked][0]
        heading, options, at = choices(what, me, text)
        chosen = select(heading, [label for _, label in options], at)
        if chosen < 0 or chosen == at:
            continue
        try:
            answer = client.put("/api/me/track-defaults", changed(me, what, options[chosen][0]))
        except error_type as e:
            if e.status in REFUSED:
                say(text("tracks_on_web"), True)
                return me
            say(text("server_error") % e.message, True)
            continue
        if isinstance(answer, dict) and "subtitle_mode" in answer:
            me = answer
        else:
            me = dict(me, **changed(me, what, options[chosen][0]))
        say(text("saved"), False)
