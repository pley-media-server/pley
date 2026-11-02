import json
import shutil
import tempfile
import unittest

import support  # noqa: F401
import xbmc
import xbmcaddon
from pley import api, kodi, playback, tracks
from pley.service import Player

BASE = "http://127.0.0.1:5000/s"

# A film: video 0, audio 1 (English, the default) and 2 (French), subtitles
# 3 (English text), 4 (English pictures), 5 (French forced text), and two
# files next to it: 1000 (Spanish text), 1001 (German pictures).
STREAMS = [
    {"index": 0, "kind": "video", "codec": "hevc"},
    {"index": 1, "kind": "audio", "codec": "eac3", "language": "eng", "default": True},
    {"index": 2, "kind": "audio", "codec": "ac3", "language": "fre"},
    {"index": 3, "kind": "subtitle", "codec": "subrip", "language": "eng", "text_based": True},
    {"index": 4, "kind": "subtitle", "codec": "hdmv_pgs_subtitle", "language": "eng", "text_based": False},
    {"index": 5, "kind": "subtitle", "codec": "subrip", "language": "fre", "forced": True, "text_based": True},
    {"index": 1000, "kind": "subtitle", "codec": "subrip", "language": "spa", "external": True, "text_based": True},
    {"index": 1001, "kind": "subtitle", "codec": "hdmv_pgs_subtitle", "language": "ger", "external": True, "text_based": False},
]


def subtitle_tracks(streams=STREAMS):
    """PlaybackInfo.subtitles, as the server lists them."""
    return [
        {
            "index": s["index"],
            "language": s.get("language"),
            "forced": bool(s.get("forced")),
            "external": bool(s.get("external")),
            "text_based": s["text_based"],
            "url": "/api/files/7/subtitles/%d.vtt" % s["index"] if s["text_based"] else None,
        }
        for s in streams
        if s["kind"] == "subtitle"
    ]


def a_file(audio=1, subtitle=None):
    return {"id": 7, "container": "mkv", "streams": STREAMS, "selected_audio": audio, "selected_subtitle": subtitle}


def as_vobsub(file):
    """`file` with its English pictures (4) in VobSub rather than PGS."""
    streams = [dict(s, codec="dvd_subtitle") if s["index"] == 4 else s for s in STREAMS]
    return dict(file, streams=streams)


def direct(audio=1, sup=False):
    """PlaybackInfo of a file played as it is; `sup`: from a server that
    makes `.sup` files of PGS tracks (it says `pts_offset_ms`)."""
    info = {"hls": False, "file_id": 7, "audio_index": audio, "subtitle_index": None, "burn_subtitles": False, "subtitles": subtitle_tracks()}
    if sup:
        info["pts_offset_ms"] = None
    return info


def streamed(audio=1, burned=None, sup=False):
    info = {"hls": True, "file_id": 7, "audio_index": audio, "subtitle_index": burned, "burn_subtitles": burned is not None, "subtitles": subtitle_tracks()}
    if sup:
        info["pts_offset_ms"] = 1400
    return info


def kodi_lists(audio=2, subtitles=5, captions=0):
    """What Kodi lists: its streams by place, as Player.GetProperties says."""
    return (
        [{"index": i, "language": ""} for i in range(audio)],
        [{"index": i, "language": "", "isforced": False} for i in range(subtitles + captions)],
    )


class Kodi:
    """Kodi's player, as JSON-RPC shows it: its lists, what plays, and the
    calls made to change it (which change it)."""

    def __init__(self, audio=2, subtitles=5, playing_audio=0, playing_subtitle=0, enabled=False):
        self.audios, self.subtitles = kodi_lists(audio, subtitles)
        self.audio, self.subtitle, self.enabled = playing_audio, playing_subtitle, enabled
        self.calls = []

    def rpc(self, method, params):
        if method == "Player.GetProperties":
            return {
                "audiostreams": self.audios,
                "currentaudiostream": {"index": self.audio} if self.audios else None,
                "subtitles": self.subtitles,
                "currentsubtitle": {"index": self.subtitle} if self.subtitles else None,
                "subtitleenabled": self.enabled,
            }
        self.calls.append((method, {k: v for k, v in params.items() if k != "playerid"}))
        if method == "Player.SetAudioStream":
            self.audio = params["stream"]
        elif method == "Player.SetSubtitle":
            if params["subtitle"] == "off":
                self.enabled = False
            else:
                self.subtitle, self.enabled = params["subtitle"], params.get("enable", True)
        return "OK"


class Clock:
    def __init__(self):
        self.now = 100.0

    def __call__(self):
        return self.now


def follower(layout, player, clock=None):
    saved = []
    return tracks.Follower(layout, player.rpc, saved.append, clock or Clock()), saved


class LayoutTest(unittest.TestCase):
    def test_kodi_lists_the_files_it_was_given_first(self):
        # Played as it is: the text file next to the media (given with
        # setSubtitles), then the file's own subtitles in its order. The
        # German pictures next to it aren't given: Kodi never lists them.
        lay = tracks.layout(direct(), a_file())
        self.assertEqual([s["index"] for s in lay["subtitles"]], [1000, 3, 4, 5])
        self.assertEqual([s["index"] for s in lay["audio"]], [1, 2])
        # The files given are the ones Kodi lists first, in the same order.
        self.assertEqual(playback.beside(BASE, direct()), [BASE + "/api/files/7/subtitles/1000.spa.vtt"])
        # Converted: every text track is given beside the stream, which
        # carries none, and only the audio the server put in.
        lay = tracks.layout(streamed(), a_file())
        self.assertEqual([s["index"] for s in lay["subtitles"]], [3, 5, 1000])
        self.assertEqual(lay["audio"], [])
        self.assertEqual(len(playback.beside(BASE, streamed())), 3)

    def test_places_both_ways(self):
        entries = tracks.layout(direct(), a_file())["subtitles"]
        _, found = kodi_lists(subtitles=4)
        self.assertEqual([tracks.to_kodi(entries, i, found) for i in (1000, 3, 4, 5, 1001)], [0, 1, 2, 3, None])
        self.assertEqual([tracks.from_kodi(entries, p, found) for p in (0, 1, 2, 3)], [1000, 3, 4, 5])
        # Captions Kodi found in the video come after them all: not pley's.
        _, found = kodi_lists(subtitles=4, captions=1)
        self.assertIsNone(tracks.from_kodi(entries, 4, found))
        self.assertEqual(tracks.to_kodi(entries, 5, found), 3)

    def test_kodi_listing_fewer_goes_by_language(self):
        entries = tracks.layout(direct(), a_file())["subtitles"]
        # Kodi left out the English pictures: the French forced track is
        # third, found by its language.
        found = [
            {"index": 0, "language": "spa", "isforced": False},
            {"index": 1, "language": "eng", "isforced": False},
            {"index": 2, "language": "fre", "isforced": True},
        ]
        self.assertEqual(tracks.to_kodi(entries, 5, found), 2)
        self.assertEqual(tracks.from_kodi(entries, 2, found), 5)
        # Two English tracks, neither forced: no telling which.
        self.assertIsNone(tracks.to_kodi(entries, 3, found))
        self.assertIsNone(tracks.from_kodi(entries, 1, found))
        # Kodi's French is fre, a file's may be fra.
        self.assertEqual(tracks.language("FRA"), "fre")

    def test_files_given_count_forced_as_they_are_named(self):
        # A stream's English and its "Signs & Songs", neither flagged: Kodi
        # reads the second's name (4.eng.forced.vtt) as forced, so does the
        # layout, and Kodi listing only the forced one still finds it.
        subtitles = [
            {"index": 3, "language": "eng", "title": "English", "forced": False, "external": False, "text_based": True, "url": "/api/files/7/subtitles/3.vtt"},
            {"index": 4, "language": "eng", "title": "Signs & Songs", "forced": False, "external": False, "text_based": True, "url": "/api/files/7/subtitles/4.vtt"},
        ]
        info = {"hls": True, "file_id": 7, "audio_index": 1, "subtitle_index": None, "burn_subtitles": False, "subtitles": subtitles}
        entries = tracks.layout(info, a_file())["subtitles"]
        self.assertEqual([(s["index"], s["forced"]) for s in entries], [(3, False), (4, True)])
        found = [{"index": 0, "language": "eng", "isforced": True}]
        self.assertEqual(tracks.to_kodi(entries, 4, found), 0)
        self.assertEqual(tracks.from_kodi(entries, 0, found), 4)
        # The file's own tracks, played as it is, keep their flag: Kodi reads
        # that from the file.
        info = dict(info, hls=False)
        self.assertEqual([(s["index"], s["forced"]) for s in tracks.layout(info, a_file())["subtitles"]], [(3, False), (4, False)])

    def test_picture_subtitles_drawn_on(self):
        # VobSub and DVB: drawn onto converted video.
        english_vobsub = as_vobsub(a_file(subtitle=4))
        self.assertEqual(tracks.burn_index(english_vobsub), 4)
        self.assertIsNone(tracks.burn_index(a_file(subtitle=3)))
        self.assertIsNone(tracks.burn_index(a_file()))
        # Kodi shows the file's own pictures when it plays as it is.
        self.assertFalse(tracks.needs_burn(direct(sup=True), english_vobsub))
        self.assertTrue(tracks.needs_burn(streamed(sup=True), english_vobsub))
        self.assertFalse(tracks.needs_burn(streamed(burned=4, sup=True), english_vobsub))
        self.assertFalse(tracks.needs_burn(streamed(), a_file(subtitle=3)))

    def test_pgs_comes_beside_what_plays_as_a_sup_file(self):
        # PGS isn't drawn on: Kodi gets it as a .sup file beside the stream,
        # where the server makes one.
        english_pgs = a_file(subtitle=4)
        self.assertIsNone(tracks.burn_index(english_pgs))
        self.assertFalse(tracks.needs_burn(streamed(sup=True), english_pgs))
        self.assertEqual(
            playback.beside(BASE, streamed(sup=True), english_pgs),
            [BASE + p for p in ("/api/files/7/subtitles/3.eng.vtt", "/api/files/7/subtitles/4.eng.sup",
                                "/api/files/7/subtitles/5.fre.forced.vtt", "/api/files/7/subtitles/1000.spa.vtt")],
        )
        lay = tracks.layout(streamed(sup=True), english_pgs)
        self.assertEqual([s["index"] for s in lay["subtitles"]], [3, 4, 5, 1000])
        self.assertEqual((lay["subtitle_index"], lay["burned"]), (4, None))
        # Only the account's: Kodi reads every file it is given as it opens.
        self.assertEqual(len(playback.beside(BASE, streamed(sup=True), a_file(subtitle=3))), 3)
        self.assertEqual(len(playback.beside(BASE, streamed(sup=True))), 3)
        # Pictures next to the media, played as it is: the German PGS file
        # too, which Kodi wasn't given before.
        german = a_file(subtitle=1001)
        self.assertFalse(tracks.needs_burn(direct(sup=True), german))
        self.assertEqual(
            playback.beside(BASE, direct(sup=True), german),
            [BASE + "/api/files/7/subtitles/1000.spa.vtt", BASE + "/api/files/7/subtitles/1001.ger.sup"],
        )
        self.assertEqual([s["index"] for s in tracks.layout(direct(sup=True), german)["subtitles"]], [1000, 1001, 3, 4, 5])
        # A server before .sup files draws them on, as before.
        self.assertTrue(tracks.needs_burn(streamed(), english_pgs))
        self.assertTrue(tracks.needs_burn(direct(), german))
        self.assertEqual(len(playback.beside(BASE, streamed(), english_pgs)), 3)
        # Drawn on all the same (asked for by an older Kodi): not given.
        self.assertEqual(len(playback.beside(BASE, streamed(burned=4, sup=True), english_pgs)), 3)


class FollowerTest(unittest.TestCase):
    def test_the_choice_is_shown_once_kodi_lists_the_streams(self):
        # French audio, the file's own French forced subtitles; Kodi starts
        # on its own choice and hasn't listed the subtitles yet.
        player = Kodi(subtitles=0)
        f, saved = follower(tracks.layout(direct(audio=2), a_file(audio=2, subtitle=5)), player)
        f.turn()
        self.assertEqual(player.calls, [])
        player.subtitles = kodi_lists(subtitles=4)[1]
        f.turn()
        self.assertEqual(player.calls, [
            ("Player.SetAudioStream", {"stream": 1}),
            ("Player.SetSubtitle", {"subtitle": 3, "enable": True}),
        ])
        self.assertEqual(saved, [])

    def test_subtitles_off_are_hidden(self):
        player = Kodi(subtitles=4, playing_subtitle=1, enabled=True)
        f, _ = follower(tracks.layout(direct(), a_file()), player)
        f.turn()
        self.assertEqual(player.calls, [("Player.SetSubtitle", {"subtitle": "off"})])

    def test_a_converted_stream(self):
        # The server put the French audio in and drew the pictures on:
        # nothing to switch, and Kodi's subtitles hidden.
        player = Kodi(audio=1, subtitles=3, enabled=True)
        f, _ = follower(tracks.layout(streamed(audio=2, burned=4), a_file(audio=2, subtitle=4)), player)
        f.turn()
        self.assertEqual(player.calls, [("Player.SetSubtitle", {"subtitle": "off"})])
        # The Spanish file next to it, given beside the stream: third.
        player = Kodi(audio=1, subtitles=3)
        f, _ = follower(tracks.layout(streamed(), a_file(subtitle=1000)), player)
        f.turn()
        self.assertEqual(player.calls, [("Player.SetSubtitle", {"subtitle": 2, "enable": True})])

    def test_without_the_details_kodi_keeps_its_own(self):
        player = Kodi(subtitles=4, playing_subtitle=2, enabled=True)
        f, _ = follower(tracks.layout(direct(), None), player)
        f.turn()
        self.assertEqual(player.calls, [])

    def test_a_kodi_that_never_lists_them_all(self):
        player = Kodi(subtitles=3)
        f, _ = follower(tracks.layout(direct(), a_file(subtitle=1000)), player)
        for _ in range(tracks.READY_TURNS):
            f.turn()
        self.assertEqual(player.calls, [])
        f.turn()
        # By language at last: the only Spanish one (none here: so none).
        self.assertTrue(f.shown)

    def test_changes_in_kodi_are_saved(self):
        clock = Clock()
        player = Kodi(subtitles=4)
        f, saved = follower(tracks.layout(direct(), a_file()), player, clock)
        f.turn()
        # Kodi takes a moment to switch: what it plays then is where it starts.
        clock.now += 1
        f.turn()
        clock.now += tracks.QUIET
        f.turn()
        self.assertEqual(saved, [])
        # French audio, chosen in Kodi's player; saved once it held.
        player.audio = 1
        f.turn()
        self.assertEqual(saved, [])
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(saved, [{"file_id": 7, "audio_index": 2, "subtitle_index": None}])
        # The Spanish file next to it, turned on.
        player.subtitle, player.enabled = 0, True
        f.turn()
        clock.now += tracks.SETTLE
        f.turn()
        # Subtitles alone: the audio picked stays as it was.
        self.assertEqual(saved[-1], {"file_id": 7, "subtitle_index": 1000})
        # Off again.
        player.enabled = False
        f.turn()
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(saved[-1], {"file_id": 7, "subtitle_index": None})
        self.assertEqual(len(saved), 3)
        # Changed and changed back before it held: nothing.
        player.audio = 0
        f.turn()
        player.audio = 1
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(len(saved), 3)

    def test_a_change_as_it_ends_and_one_pley_doesnt_know(self):
        clock = Clock()
        player = Kodi(subtitles=4)
        player.subtitles = kodi_lists(subtitles=4, captions=1)[1]
        f, saved = follower(tracks.layout(direct(), a_file()), player, clock)
        f.turn()
        clock.now += tracks.QUIET
        f.turn()
        # Captions Kodi found in the video: not saved.
        player.subtitle, player.enabled = 4, True
        f.turn()
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(saved, [])
        # The file's English subtitles, and the playback ends at once: a
        # change still settling isn't saved.
        player.subtitle = 1
        clock.now += tracks.SETTLE
        f.turn()
        clock.now += tracks.SETTLE - 1
        f.finish()
        self.assertEqual(saved, [])
        # Held, and the playback ends before a turn saved it: saved.
        f.turn()
        clock.now += tracks.SETTLE
        f.finish()
        self.assertEqual(saved, [{"file_id": 7, "subtitle_index": 3}])
        # Audio and subtitles changed together: both.
        player.audio, player.subtitle = 1, 0
        f.turn()
        clock.now += tracks.SETTLE
        f.finish()
        self.assertEqual(saved[-1], {"file_id": 7, "audio_index": 2, "subtitle_index": 1000})

    def test_subtitles_off_as_kodi_stops_are_not_saved(self):
        # A stream with the account's PGS beside it, as a .sup file.
        clock = Clock()
        player = Kodi(audio=1, subtitles=4)
        f, saved = follower(tracks.layout(streamed(sup=True), a_file(subtitle=4)), player, clock)
        f.turn()
        self.assertEqual(player.calls, [("Player.SetSubtitle", {"subtitle": 1, "enable": True})])
        clock.now += tracks.QUIET
        f.turn()
        # Kodi stopping: no audio, and no subtitles playing. A stream's
        # layout lists no audio, but Kodi played some: nothing to go by,
        # however long it lasts.
        player.audios, player.subtitles = [], []
        for _ in range(3):
            f.turn()
            clock.now += tracks.SETTLE
        f.finish()
        self.assertEqual(saved, [])
        # Subtitles turned off as the last turn saw them: not saved as it
        # ends, held or not. Off for SETTLE seconds of turns while it plays,
        # they are.
        player.audios, player.subtitles = kodi_lists(audio=1, subtitles=4)
        player.enabled = False
        f.turn()
        clock.now += tracks.SETTLE
        f.finish()
        self.assertEqual(saved, [])
        f.turn()
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(saved, [{"file_id": 7, "subtitle_index": None}])

    def test_audio_changed_with_subtitles_off_all_along_is_saved_as_it_ends(self):
        # Subtitles off from the start, the account's choice.
        clock = Clock()
        player = Kodi(subtitles=4)
        f, saved = follower(tracks.layout(direct(), a_file()), player, clock)
        f.turn()
        clock.now += tracks.QUIET
        f.turn()
        self.assertEqual(f.baseline, (0, None))
        # French audio, chosen in Kodi's player. Kodi stops before a turn
        # saved it, listing nothing as it stops: nothing to go by.
        player.audio = 1
        f.turn()
        clock.now += 1
        f.turn()
        player.audios, player.subtitles = [], []
        clock.now += tracks.SETTLE - 1
        f.turn()
        # It held, and the subtitles, off all along, are no part of it:
        # saved as the playback ends.
        f.finish()
        self.assertEqual(saved, [{"file_id": 7, "audio_index": 2, "subtitle_index": None}])

    def test_a_converted_streams_audio_is_the_servers(self):
        clock = Clock()
        player = Kodi(audio=1, subtitles=3)
        f, saved = follower(tracks.layout(streamed(audio=2), a_file(audio=2)), player, clock)
        f.turn()
        clock.now += tracks.QUIET
        f.turn()
        player.subtitle, player.enabled = 1, True
        f.turn()
        clock.now += tracks.SETTLE
        f.finish()
        self.assertEqual(saved, [{"file_id": 7, "subtitle_index": 5}])

    def test_a_file_without_audio_is_followed(self):
        # Kodi plays no audio from the start: a turn without audio is no
        # player stopping here, and a change is saved all the same.
        clock = Clock()
        silent = dict(a_file(audio=None), streams=[s for s in STREAMS if s["kind"] != "audio"])
        player = Kodi(audio=0, subtitles=4)
        f, saved = follower(tracks.layout(direct(audio=None), silent), player, clock)
        f.turn()
        clock.now += tracks.QUIET
        f.turn()
        # The file's English subtitles, turned on in Kodi's player.
        player.subtitle, player.enabled = 1, True
        f.turn()
        clock.now += tracks.SETTLE
        f.turn()
        self.assertEqual(saved, [{"file_id": 7, "subtitle_index": 3}])


class Server:
    """POST /api/playback and GET /api/items/{id}, noted."""

    def __init__(self, item, answers):
        self.item, self.answers, self.posts = item, list(answers), []

    def post(self, path, body=None):
        self.posts.append(dict(body))
        return dict(self.answers.pop(0), session_id="s%d" % len(self.posts), url="/api/files/7/stream", method="direct")

    def get(self, path, **query):
        return self.item


class PrepareTest(unittest.TestCase):
    def setUp(self):
        self.profile = tempfile.mkdtemp()
        xbmcaddon.settings["_profile"] = self.profile

    def tearDown(self):
        xbmcaddon.settings.pop("_profile", None)
        kodi.clear_property(kodi.PLAYING)
        shutil.rmtree(self.profile, ignore_errors=True)

    def item(self, subtitle, vobsub=False):
        file = a_file(subtitle=subtitle)
        return {"id": 9, "kind": "movie", "title": "Heat", "duration_ms": 1000, "files": [as_vobsub(file) if vobsub else file]}

    def test_picture_subtitles_are_asked_to_be_drawn_on(self):
        # The details known: the pictures are named at once.
        vobsub = self.item(4, vobsub=True)
        server = Server(vobsub, [streamed(burned=4, sup=True)])
        playback.prepare(server, BASE, 9, 0, vobsub)
        self.assertEqual((server.posts[0]["file_id"], server.posts[0]["subtitle_index"]), (7, 4))
        playing = json.loads(kodi.get_property(kodi.PLAYING))
        self.assertEqual(playing["tracks"]["burned"], 4)
        # Asked for with the details: the stream that came without them is
        # asked for again.
        server = Server(vobsub, [streamed(sup=True), streamed(burned=4, sup=True)])
        playback.prepare(server, BASE, 9, 0)
        self.assertNotIn("subtitle_index", server.posts[0])
        self.assertEqual(server.posts[1]["subtitle_index"], 4)
        self.assertEqual(json.loads(kodi.get_property(kodi.PLAYING))["session_id"], "s2")
        # Text subtitles, and pictures Kodi shows in a file it plays as it is: once.
        server = Server(self.item(3), [streamed(sup=True)])
        playback.prepare(server, BASE, 9, 0)
        server = Server(vobsub, [direct(sup=True)])
        playback.prepare(server, BASE, 9, 0)
        self.assertEqual(len(server.posts), 1)
        playing = json.loads(kodi.get_property(kodi.PLAYING))
        self.assertEqual((playing["tracks"]["subtitle_index"], playing["tracks"]["known"]), (4, True))

    def test_pgs_is_given_to_kodi_rather_than_drawn_on(self):
        # The details known: PGS isn't named, the stream keeps its picture,
        # and Kodi gets the track's .sup beside it.
        server = Server(self.item(4), [streamed(sup=True)])
        li = playback.prepare(server, BASE, 9, 0, self.item(4))
        self.assertNotIn("subtitle_index", server.posts[0])
        self.assertIn(BASE + "/api/files/7/subtitles/4.eng.sup", li.subtitles)
        self.assertIsNone(json.loads(kodi.get_property(kodi.PLAYING))["tracks"]["burned"])
        # A server before .sup files: asked for again, drawn on.
        server = Server(self.item(4), [streamed(), streamed(burned=4)])
        li = playback.prepare(server, BASE, 9, 0, self.item(4))
        self.assertEqual(server.posts[1]["subtitle_index"], 4)
        self.assertNotIn(BASE + "/api/files/7/subtitles/4.eng.sup", li.subtitles)


class ServicePlayer(Player):
    """The service's player with Kodi's JSON-RPC stood in for, its requests noted."""

    def __init__(self, kodi_player):
        class Now:
            def put(self, job):
                job()

        super().__init__(tokens=None, user_agent="pleyKodi/test", worker=Now())
        self.file = "http://proxy/7"
        self.requests = []
        self.real_rpc = xbmc.executeJSONRPC

        def rpc(request):
            asked = json.loads(request)
            return json.dumps({"result": kodi_player.rpc(asked["method"], asked["params"])})

        xbmc.executeJSONRPC = rpc

    def isPlaying(self):
        return bool(self.file)

    def getPlayingFile(self):
        return self.file

    def getTime(self):
        return 1.0

    def call(self, method, path, body=None, timeout=5, fine=(410,)):
        self.requests.append((method, path, body))
        return api.Response(204, {}, b"")


class ServiceTest(unittest.TestCase):
    def setUp(self):
        self.player = Kodi(subtitles=4)
        self.service = ServicePlayer(self.player)

    def tearDown(self):
        xbmc.executeJSONRPC = self.service.real_rpc
        kodi.clear_property(kodi.PLAYING)

    def play(self) -> Clock:
        """French audio and the Spanish file next to the film, shown as Kodi
        starts; then where it starts, past the quiet seconds. The follower's
        clock."""
        playing = {"item_id": 9, "session_id": "s", "duration_ms": 100_000, "path": self.service.file,
                   "tracks": tracks.layout(direct(audio=2), a_file(audio=2, subtitle=1000))}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.service.onAVStarted()
        self.assertEqual(self.player.calls, [
            ("Player.SetAudioStream", {"stream": 1}),
            ("Player.SetSubtitle", {"subtitle": 0, "enable": True}),
        ])
        clock = Clock()
        self.service.tracks.clock = clock
        self.service.tracks.quiet_until = 0
        self.service.tick()
        return clock

    def stop(self) -> list:
        """Kodi stops: the service's requests to save tracks."""
        self.service.file = ""
        self.service.onPlayBackStopped()
        self.assertIsNone(self.service.tracks)
        return [r for r in self.service.requests if r[1].endswith("/streams")]

    def test_the_service_shows_and_saves_them(self):
        clock = self.play()
        # The film's English subtitles, picked in Kodi's player; they held,
        # and playback stops before the next turn saved them.
        self.player.subtitle = 1
        self.service.tick()
        clock.now += tracks.SETTLE
        self.assertEqual(self.stop(), [("PUT", "/api/items/9/streams", {"file_id": 7, "subtitle_index": 3})])
        # Saved before the last report.
        self.assertEqual(self.service.requests[-3][1], "/api/items/9/streams")

    def test_subtitles_kodi_turns_off_as_it_stops_are_not_saved(self):
        clock = self.play()
        self.player.enabled = False
        self.service.tick()
        clock.now += tracks.SETTLE
        self.assertEqual(self.stop(), [])
        # The last report and the session's end go all the same.
        self.assertEqual([r[0] for r in self.service.requests[-2:]], ["POST", "DELETE"])


class DefaultsTest(unittest.TestCase):
    ME = {"name": "ana", "audio_language": "fre", "subtitle_language": None, "subtitle_mode": "forced"}

    def test_rows(self):
        T = kodi.text
        self.assertEqual([label for _, label in tracks.rows(self.ME, T)], [
            "Audio language: French",
            "Subtitles: Only for foreign dialogue",
        ])
        always = dict(self.ME, subtitle_mode="always", audio_language=None)
        self.assertEqual([label for _, label in tracks.rows(always, T)], [
            "Audio language: The file's default",
            "Subtitles: Always",
            "Subtitle language: Same as the audio language",
        ])
        self.assertEqual([T(w) for _, w in tracks.SUBTITLE_MODES], [
            "Off", "Only for foreign dialogue", "When the audio is in another language", "Always",
        ])

    def edit(self, picks, client):
        asked, said = [], []

        def select(heading, labels, preselect):
            asked.append((heading, labels, preselect))
            return picks.pop(0)

        me = tracks.edit_defaults(client, dict(self.ME), select, lambda w, e: said.append((w, e)), kodi.text, api.ApiError)
        return me, asked, said

    def test_each_change_is_saved_with_all_three(self):
        class Client:
            def __init__(self):
                self.puts = []

            def put(self, path, body):
                self.puts.append((path, body))
                return dict(DefaultsTest.ME, **body)

        client = Client()
        # Subtitles → When the audio is in another language; then Subtitle
        # language → German (the languages follow "Same as the audio
        # language"); then Back.
        german = 1 + [c for c, _ in tracks.LANGUAGES].index("ger")
        me, asked, said = self.edit([1, 2, 2, german, -1], client)
        self.assertEqual(asked[1][0], kodi.text("subtitles"))
        self.assertEqual(asked[1][2], 1)
        self.assertEqual(len(asked[2][1]), 3)
        self.assertEqual(asked[3][1][0], kodi.text("same_as_audio"))
        self.assertEqual(client.puts, [
            ("/api/me/track-defaults", {"audio_language": "fre", "subtitle_language": None, "subtitle_mode": "foreign"}),
            ("/api/me/track-defaults", {"audio_language": "fre", "subtitle_language": "ger", "subtitle_mode": "foreign"}),
        ])
        self.assertEqual((me["subtitle_mode"], me["subtitle_language"]), ("foreign", "ger"))
        self.assertEqual(said, [(kodi.text("saved"), False)] * 2)

    def test_a_server_that_doesnt_let_apps_change_them(self):
        class Client:
            def put(self, path, body):
                raise api.ApiError(403, "forbidden", "not for apps")

        # Audio language → The file's default: refused, said, and the list closes.
        me, asked, said = self.edit([0, 0], Client())
        self.assertEqual(said, [(kodi.text("tracks_on_web"), True)])
        self.assertEqual(me["audio_language"], "fre")
        self.assertEqual(len(asked), 2)


if __name__ == "__main__":
    unittest.main()
