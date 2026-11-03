import json
import tempfile
import unittest

import support
import xbmc
import xbmcaddon
import xbmcgui
from pley import cast, kodi
from pley.cast import Reporter, control_of, play_of
from pley.service import Player
from test_service import KodiPlayer, Now, Tokens

DEVICE = "0123456789abcdef0123456789abcdef"


class DeviceTest(unittest.TestCase):
    def setUp(self):
        self.saved = dict(xbmcaddon.settings)
        xbmcaddon.settings["_profile"] = tempfile.mkdtemp()

    def tearDown(self):
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update(self.saved)
        xbmc.info_labels.pop("System.FriendlyName", None)

    def test_the_id_is_made_once_and_kept(self):
        made = cast.device_id()
        self.assertRegex(made, r"^[0-9a-f]{32}$")
        self.assertEqual(cast.device_id(), made)
        kodi.remember(cast.DEVICE, "not one of ours")
        self.assertNotEqual(cast.device_id(), "not one of ours", "one that isn't an id is made again")

    def test_the_stream_says_kodis_name(self):
        xbmc.info_labels["System.FriendlyName"] = " Living room\x07 "
        self.assertEqual(cast.query(DEVICE), {"cast_device": DEVICE, "cast_name": "Living room", "cast_platform": "kodi"})
        xbmc.info_labels["System.FriendlyName"] = ""
        self.assertEqual(cast.query(DEVICE)["cast_name"], "Kodi")
        xbmc.info_labels["System.FriendlyName"] = "é" * 70
        self.assertEqual(cast.query(DEVICE)["cast_name"], "é" * 64, "what the server takes")


class EventsTest(unittest.TestCase):
    def test_a_title_to_play(self):
        event = {"type": "cast_play", "device_id": DEVICE, "item_id": 7, "file_id": 3, "position_ms": 5000}
        self.assertEqual(play_of(event, DEVICE), (7, 3, 5000))
        self.assertEqual(play_of(dict(event, file_id=None, position_ms=None), DEVICE), (7, None, None))
        self.assertIsNone(play_of(dict(event, device_id="another-device"), DEVICE))
        for wrong in ({"item_id": "7"}, {"item_id": True}, {"item_id": 0}, {"file_id": "3"}, {"position_ms": -1}):
            self.assertIsNone(play_of(dict(event, **wrong), DEVICE), wrong)

    def test_a_command_for_the_player(self):
        event = {"type": "cast_control", "device_id": DEVICE, "action": "pause", "position_ms": None}
        self.assertEqual(control_of(event, DEVICE), ("pause", None))
        self.assertEqual(control_of(dict(event, action="stop", position_ms=9), DEVICE), ("stop", None))
        self.assertEqual(control_of(dict(event, action="seek", position_ms=12_500), DEVICE), ("seek", 12_500))
        self.assertIsNone(control_of(dict(event, action="seek"), DEVICE), "a seek needs a position")
        self.assertIsNone(control_of(dict(event, action="rewind"), DEVICE))
        self.assertIsNone(control_of(dict(event, device_id="another-device"), DEVICE))


class ReporterTest(unittest.TestCase):
    def setUp(self):
        self.now = 0.0
        self.told = []
        self.reporter = Reporter(self.told.append, clock=lambda: self.now)
        self.playing = {"item_id": 7, "duration_ms": 100_000, "kind": "movie"}

    def states(self):
        return [(body["state"], body["position_ms"]) if body["item_id"] else None for body in self.told]

    def test_a_playing_video_is_told_every_ten_seconds(self):
        self.reporter.started(self.playing, 1000)
        for second in range(1, 25):
            self.now = second
            self.reporter.at(self.playing, 1000 + second * 1000, paused=False)
        self.assertEqual(self.states(), [("playing", 1000), ("playing", 11_000), ("playing", 21_000)])
        self.assertEqual(self.told[0], {"item_id": 7, "position_ms": 1000, "duration_ms": 100_000, "state": "playing"})

    def test_pauses_waits_and_seeks_are_told_as_they_come(self):
        self.reporter.started(self.playing, 1000)
        self.now = 1
        self.reporter.at(self.playing, 2000, paused=True)
        for second in range(2, 40):
            self.now = second
            self.reporter.at(self.playing, 2000, paused=True)
        self.reporter.at(self.playing, 2000, paused=False)
        self.reporter.at(self.playing, 2500, paused=False, caching=True)
        self.reporter.moved(self.playing, 60_000)
        self.reporter.stopped(self.playing)
        self.assertEqual(
            self.states(),
            [("playing", 1000), ("paused", 2000), ("playing", 2000), ("buffering", 2500), ("buffering", 60_000), None],
        )
        self.assertEqual(self.told[-1], {"item_id": None})

    def test_a_title_loading_takes_over_without_a_stop(self):
        self.reporter.started(self.playing, 1000)
        loading = self.reporter.loading(9, 0)
        self.reporter.stopped(self.playing)
        nine = {"item_id": 9, "duration_ms": 0, "kind": "episode"}
        self.reporter.started(nine, 0)
        self.assertEqual(self.states(), [("playing", 1000), ("buffering", 0), ("playing", 0)])
        self.assertEqual(self.told[-1]["duration_ms"], None)
        self.reporter.stopped(loading)
        self.reporter.failed()
        self.assertEqual(len(self.told), 3, "what plays now isn't over")

    def test_music_isnt_told(self):
        track = {"item_id": 4, "duration_ms": 200_000, "kind": "track"}
        self.reporter.started(track, 0)
        self.reporter.at(track, 0, paused=True)
        self.reporter.stopped(track)
        self.assertEqual(self.told, [])

    def test_a_stream_opened_again_hears_it_again(self):
        self.reporter.again()
        self.reporter.started(self.playing, 1000)
        self.reporter.at(self.playing, 2000, paused=True)
        self.reporter.again()
        self.assertEqual(self.states()[-1], ("paused", 2000), "paused: as it was")
        self.reporter.at(self.playing, 2000, paused=False)
        self.now = 3
        self.reporter.again()
        self.reporter.at(self.playing, 5000, paused=False)
        self.assertEqual(self.states()[-2:], [("playing", 2000), ("playing", 5000)], "playing: where it is now")


class CastPlayer(KodiPlayer):
    """The service's player for this Kodi as a cast device: what it tells
    the server, and what it asks of Kodi's player, recorded."""

    def __init__(self):
        super().__init__()
        self.device = DEVICE
        self.told = []
        self.cast = Reporter(self.told.append)
        self.calls = []
        self.jobs = []
        self.worker = self
        self.played = []

    def put(self, job):
        self.jobs.append(job)

    def find_next(self, reports, moments):
        pass

    def work(self):
        while self.jobs:
            self.jobs.pop(0)()

    def stop(self):
        self.calls.append("stop")

    def pause(self):
        self.calls.append("pause")

    def seekTime(self, seconds):
        self.calls.append(("seek", seconds))

    def play(self, path, li=None):
        self.played.append((path, li))

    def starts(self, playing):
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.file = playing["path"]
        self.onPlayBackStarted()
        self.onAVStarted()
        self.work()

    def states(self):
        return [(b["item_id"], b["state"], b["position_ms"]) if b["item_id"] else None for b in self.told]


def video(item_id, session, kind="movie"):
    return {"item_id": item_id, "session_id": session, "duration_ms": 100_000, "path": "http://proxy/" + session,
            "kind": kind, "start_ms": 0}


class BeingCastToTest(unittest.TestCase):
    def setUp(self):
        from pley import playback

        self.playback = playback
        self.prepare = playback.prepare
        self.asked = []
        self.details = {"id": 9, "kind": "episode", "duration_ms": 100_000, "user_state": {"position_ms": 60_000}}
        self.server = support.FakeServer(lambda request: support.answer(200, self.details))
        kodi.set_property(kodi.PROXY, self.server.url + "/secret")

        def prepare(client, base, item_id, start_ms, detail=None, restart=None, file_id=None):
            if isinstance(self.prepared, Exception):
                raise self.prepared
            self.asked.append((item_id, start_ms, (detail or {}).get("id"), file_id))
            kodi.set_property(kodi.PLAYING, json.dumps(video(item_id, "cast")))
            return xbmcgui.ListItem(path="http://proxy/cast")

        self.prepared = None
        playback.prepare = prepare
        self.player = CastPlayer()
        xbmcgui.notified.clear()

    def tearDown(self):
        self.playback.prepare = self.prepare
        self.server.close()
        xbmc.conditions.discard("Player.Paused")
        kodi.clear_property(kodi.PLAYING)
        kodi.clear_property(kodi.PROXY)
        xbmcgui.notified.clear()

    def sent(self, **fields):
        event = {"type": "cast_play", "device_id": DEVICE, "item_id": 9, "file_id": None, "position_ms": None}
        self.player.heard(dict(event, **fields))

    def control(self, action, position_ms=None):
        self.player.heard({"type": "cast_control", "device_id": DEVICE, "action": action, "position_ms": position_ms})

    def test_a_title_sent_here_ends_what_plays_and_plays_from_where_it_was_sent(self):
        player = self.player
        player.starts(video(7, "s"))
        player.seconds = 20.0
        self.sent(file_id=3, position_ms=5000)
        player.work()
        self.assertEqual(player.sent, [("playing", 0), ("stopped", 20_000), ("ended", "/api/playback/s")])
        self.assertEqual(player.calls, ["stop"])
        self.assertEqual(self.asked, [(9, 5000, None, 3)])
        (path, li), = player.played
        self.assertEqual((path, li.properties["StartOffset"]), ("http://proxy/cast", "5.000"))
        player.file, player.seconds = "http://proxy/cast", 5.0
        player.onPlayBackStarted()
        player.onAVStarted()
        self.assertEqual(player.states()[1:], [(7, "playing", 0), (9, "buffering", 5000), (9, "playing", 5000)],
                         "no stop between them")

    def test_a_title_sent_without_a_position_starts_where_the_profile_left_off(self):
        self.sent()
        self.player.work()
        self.assertEqual(self.asked, [(9, 60_000, 9, None)], "its resume point, not asked")
        self.assertEqual(self.server.requests[0].path, "/secret/api/items/9")
        self.details["user_state"] = {"position_ms": 60_000, "watched": True}
        self.sent()
        self.player.work()
        self.assertEqual(self.asked[-1][:2], (9, 0), "watched: from the start")
        self.assertNotIn("StartOffset", self.player.played[-1][1].properties)

    def test_what_isnt_for_this_kodi_is_left_be(self):
        self.sent(device_id="another-device")
        self.sent(item_id="nine")
        self.player.heard({"type": "cast_state", "device_id": DEVICE, "state": None})
        self.assertEqual((self.player.jobs, self.player.told), ([], []))

    def test_stopped_before_it_plays_it_isnt_played(self):
        self.sent(position_ms=0)
        self.control("stop")
        self.player.work()
        self.assertEqual((self.asked, self.player.played), ([], []))
        self.assertEqual(self.player.states(), [(9, "buffering", 0), None])

    def test_the_last_title_sent_plays(self):
        self.sent(item_id=8, position_ms=0)
        self.sent(item_id=9, position_ms=0)
        self.player.work()
        self.assertEqual([asked[0] for asked in self.asked], [9])
        self.assertEqual(len(self.player.played), 1)

    def test_a_title_that_cant_be_played_says_why(self):
        from pley import api

        self.prepared = api.ApiError(404, "not_found", "item 9 not found")
        self.sent(position_ms=0)
        self.player.work()
        self.assertEqual(self.player.states(), [(9, "buffering", 0), None])
        self.assertEqual(xbmcgui.notified[-1][1], kodi.text("server_error") % "item 9 not found")

    def test_kodi_failing_to_play_it_says_nothing_plays(self):
        self.sent(position_ms=0)
        self.player.work()
        self.player.onPlayBackError()
        self.assertEqual(self.player.states(), [(9, "buffering", 0), None])

    def test_what_plays_before_a_title_sent_here_doesnt_end_it(self):
        # Kodi says the playback before stopped once the title is asked for.
        self.player.starts(video(7, "s"))
        self.sent(position_ms=0)
        self.player.file = ""
        self.player.onPlayBackStopped()
        self.player.work()
        self.assertEqual(self.player.states()[1:], [(7, "playing", 0), (9, "buffering", 0)])
        self.assertEqual(len(self.player.played), 1)

    def test_a_video_of_pleys_is_steered(self):
        player = self.player
        player.starts(video(7, "s"))
        self.control("pause")
        xbmc.conditions.add("Player.Paused")
        self.control("pause")
        self.control("seek", 12_500)
        self.control("resume")
        xbmc.conditions.discard("Player.Paused")
        self.control("resume")
        self.control("stop")
        self.assertEqual(player.calls, ["pause", ("seek", 12.5), "pause", "stop"])

    def test_anything_else_isnt(self):
        self.control("pause")
        self.player.starts(video(4, "t", kind="track"))
        self.control("pause")
        self.control("stop")
        self.assertEqual(self.player.calls, [])

    def test_what_kodi_does_is_told(self):
        player = self.player
        player.starts(video(7, "s"))
        player.seconds = 3.0
        player.onPlayBackPaused()
        player.onPlayBackResumed()
        player.onPlayBackSeek(30_000, 27_000)
        player.file = ""
        player.onPlayBackStopped()
        self.assertEqual(
            player.states(),
            [(7, "buffering", 0), (7, "playing", 0), (7, "paused", 3000), (7, "playing", 3000), (7, "playing", 30_000), None],
        )

    def test_the_next_episode_is_a_new_report(self):
        player = self.player
        player.starts(video(7, "s", kind="episode"))
        player.file = ""
        player.onPlayBackEnded()
        player.starts(video(8, "t", kind="episode"))
        self.assertEqual(player.states()[2:], [None, (8, "buffering", 0), (8, "playing", 0)])

    def test_automatic_playing_it_again_goes_on(self):
        player = self.player
        player.starts(video(7, "s"))
        player.restarting = (7, None, None)
        player.starts(video(7, "s2"))
        self.assertNotIn(None, player.states())
        self.assertEqual(player.states()[-1], (7, "playing", 0))


class TellTest(unittest.TestCase):
    """PUT /api/cast/devices/{id}/state, with the service's token, while the
    event stream lists this Kodi."""

    def setUp(self):
        self.server = support.FakeServer(lambda request: support.answer(204, {}))
        self.player = Player(Tokens(self.server.url), "pleyKodi/test", Now(), device=DEVICE, casts=Now())

    def tearDown(self):
        self.server.close()

    def test_what_plays_is_told_while_listed(self):
        body = {"item_id": 7, "position_ms": 0, "duration_ms": None, "state": "buffering"}
        self.player.tell_cast(body)
        self.assertEqual(self.server.requests, [], "not listed")
        self.player.listed = lambda: True
        self.player.tell_cast(body)
        self.player.tell_cast({"item_id": None})
        put, gone = self.server.requests
        self.assertEqual((put.method, put.path, put.json()), ("PUT", "/api/cast/devices/%s/state" % DEVICE, body))
        self.assertEqual(put.headers["authorization"], "Bearer t1")
        self.assertEqual(gone.json(), {"item_id": None})


if __name__ == "__main__":
    unittest.main()
