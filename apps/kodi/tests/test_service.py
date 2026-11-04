import json
import unittest

import support  # noqa: F401
import xbmc
import xbmcaddon
import xbmcgui
from pley import kodi
from pley.autoquality import Meter
from pley.service import PAUSED_EVERY, REPORT_EVERY, Player, Reports


class ReportsTest(unittest.TestCase):
    def setUp(self):
        self.now = 0.0
        self.sent = []
        self.ended = []
        playing = {"item_id": 7, "session_id": "abc", "duration_ms": 100_000}
        self.reports = Reports(playing, lambda path, body: self.sent.append((path, body)), self.ended.append,
                               clock=lambda: self.now)

    def states(self):
        return [(body["state"], body["position_ms"]) for _, body in self.sent]

    def test_playing_is_reported_every_ten_seconds(self):
        self.reports.at(0)
        for second in range(1, 25):
            self.now = second
            self.reports.at(second * 1000)
        self.assertEqual(self.states(), [("playing", 0), ("playing", 10_000), ("playing", 20_000)])
        path, body = self.sent[0]
        self.assertEqual(path, "/api/items/7/progress")
        self.assertEqual((body["session_id"], body["duration_ms"]), ("abc", 100_000))

    def test_pause_resume_and_stop(self):
        self.reports.at(1000)
        self.now = 3
        self.reports.position_ms = 4000
        self.reports.pause()
        self.now = 30
        self.reports.at(4000)
        self.reports.resume()
        self.now = 31
        self.reports.at(5000)
        self.reports.finish(ended=False)
        self.assertEqual(self.states(), [("playing", 1000), ("paused", 4000), ("playing", 4000), ("stopped", 5000)])
        self.assertEqual(self.ended, ["/api/playback/abc"])

    def test_a_paused_playback_is_reported_every_30_seconds(self):
        # The server ends a session it hasn't heard of for 90 seconds.
        self.reports.at(5000)
        self.now = 1
        self.reports.position_ms = 5000
        self.reports.pause()
        for second in range(2, 70):
            self.now = second
            self.reports.at(9_999_999)
        self.assertEqual(self.states(), [("playing", 5000), ("paused", 5000), ("paused", 5000), ("paused", 5000)])
        self.assertEqual(PAUSED_EVERY, 30)

    def test_a_stream_that_broke_off_early_isnt_watched(self):
        self.reports.at(40_000)
        self.reports.finish(ended=True)
        self.assertEqual(self.states()[-1], ("stopped", 40_000), "40 % in: where it was")
        reports = Reports({"item_id": 7, "session_id": "s", "duration_ms": 100_000}, lambda p, b: self.sent.append((p, b)), self.ended.append)
        reports.credits_from = 80_000
        reports.at(81_000)
        reports.finish(ended=True)
        self.assertEqual(self.states()[-1], ("stopped", 100_000), "in the credits: the whole length")

    def test_the_end_reports_the_whole_length(self):
        self.reports.at(98_000)
        self.reports.finish(ended=True)
        self.assertEqual(self.states()[-1], ("stopped", 100_000))
        self.assertEqual(REPORT_EVERY, 10)


class Now:
    """The service's worker, doing each job at once."""

    def put(self, job):
        job()


class KodiPlayer(Player):
    """The service's player, with what Kodi plays stood in for."""

    def __init__(self):
        super().__init__(tokens=None, user_agent="pleyKodi/test", worker=Now())
        self.file = ""
        self.seconds = 0.0
        self.sent = []

    def isPlaying(self):
        return bool(self.file)

    def getPlayingFile(self):
        return self.file

    def getTime(self):
        return self.seconds

    def send(self, playing, path, body):
        self.sent.append((body["state"], body["position_ms"]))

    def end(self, path):
        self.sent.append(("ended", path))


class PlayerTest(unittest.TestCase):
    def setUp(self):
        self.player = KodiPlayer()
        playing = {"item_id": 7, "session_id": "s", "duration_ms": 100_000, "path": "http://proxy/7"}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]

    def tearDown(self):
        xbmc.conditions.discard("Player.Paused")
        kodi.clear_property(kodi.PLAYING)

    def test_a_pause_kodi_told_of_too_early_is_reported(self):
        # Paused while the stream started: Kodi said so before it said the
        # stream started, when there was nothing to report.
        self.player.seconds = 3.0
        xbmc.conditions.add("Player.Paused")
        self.player.onAVStarted()
        self.player.tick()
        self.player.tick()
        xbmc.conditions.discard("Player.Paused")
        self.player.seconds = 4.0
        self.player.tick()
        self.assertEqual(self.player.sent, [("playing", 3000), ("paused", 3000), ("playing", 3000)])

    def test_a_stop_ends_the_session(self):
        self.player.seconds = -0.099
        self.player.onAVStarted()
        self.player.seconds = 20.0
        self.player.tick()
        self.player.file = ""
        self.player.onPlayBackStopped()
        self.assertEqual(self.player.sent, [("playing", 0), ("stopped", 20_000), ("ended", "/api/playback/s")])


def automatic(session, quality="original", kbps=20_000, prefix=None, live=False, converted=None):
    """A playback in Automatic, as playback.prepare tells the service of it."""
    return {
        "item_id": 7,
        "session_id": session,
        "duration_ms": 3_600_000,
        "path": "http://proxy/" + session,
        "auto": {
            "quality": quality,
            "playing_kbps": kbps,
            "file_kbps": 20_000,
            "file_lines": 1080,
            "cap": None,
            "lowered": False,
            "server": "srv-1",
            "prefix": prefix or "/api/playback/%s/" % session,
            "converted": quality != "original" if converted is None else converted,
            "lines": 1080,
            "live": live,
        },
    }


class AutoPlayer(KodiPlayer):
    """The service's player with its clock stood in for, and its restarts
    recorded rather than made."""

    def __init__(self):
        super().__init__()
        self.now = 0.0
        self.clock = lambda: self.now
        self.meter = Meter(clock=lambda: self.now)
        self.restarts = []
        self.played = []
        self.bitrates = []

    def restart(self, item_id, position_ms, quality):
        self.restarts.append((item_id, position_ms, quality))

    def set_bitrate(self, watch, session, kbps):
        self.bitrates.append((session, kbps))

    def play(self, path, li=None):
        self.played.append((path, li))

    def starts(self, playing):
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.file = playing["path"]
        self.onAVStarted()

    def second(self, moves=1.0, caching=False, arriving=0):
        self.now += 1
        self.seconds += moves
        if arriving:
            self.meter.chunk(self.auto["prefix"] + "1.ts", arriving)
        if caching:
            xbmc.conditions.add("Player.Caching")
        try:
            self.tick()
        finally:
            xbmc.conditions.discard("Player.Caching")


class AutomaticTest(unittest.TestCase):
    def setUp(self):
        import tempfile

        self.profile = tempfile.mkdtemp()
        xbmcaddon.settings["_profile"] = self.profile
        del xbmcgui.notified[:]
        self.player = AutoPlayer()

    def tearDown(self):
        xbmcaddon.settings.pop("_profile", None)
        kodi.clear_property(kodi.PLAYING)

    def test_a_wait_plays_the_title_again_lower_where_it_was(self):
        player = self.player
        player.starts(automatic("s1"))
        first = player.watch
        self.assertEqual(first.quality, "original")
        # 1.5 MB a second through the proxy: 12 Mbps, its answers too, under 1.5 times the file's 20.
        for n in range(30):
            player.second(arriving=1_500_000)
            if n % 10 == 0:
                player.meter.answered(player.auto["prefix"] + "%d.ts" % n, 1_500_000, 1.0)
        player.second(0.0, caching=True)
        player.second(0.0, caching=True)
        self.assertEqual(player.restarts, [(7, 30_000, "8m")])
        self.assertEqual(kodi.remembered("auto_ceiling:srv-1").split("@")[0], "8550", "12 Mbps for 19 of the last 20 s, with the margin")
        self.assertEqual(xbmcgui.notified, [], "said once it plays")
        player.second(0.0, caching=True)
        self.assertEqual(len(player.restarts), 1, "nothing more while it restarts")

        # The service's restart started playing it at 8 Mbps.
        player.starts(automatic("s2", "8m", 8_000))
        self.assertEqual(player.sent[-3:], [("stopped", 30_000), ("ended", "/api/playback/s1"), ("playing", 30_000)])
        self.assertIs(player.watch, first, "the title's watch, kept")
        self.assertEqual(first.quality, "8m")
        self.assertEqual(xbmcgui.notified, [("Pley", "Quality lowered to 8 Mbps 1080p")])

    def test_the_next_title_starts_higher_and_back_at_the_original_forgets(self):
        player = self.player
        kodi.remember("auto_ceiling:srv-1", "6000@%d" % 10**9)
        player.starts(dict(automatic("s1", "4m", 4_000), duration_ms=10_000_000))
        watch = player.watch
        # The network carries 40 Mbps all along.
        watch.next_title = "15m"
        watch.down_at = None
        for second in range(181):
            player.now += 1
            player.meter.answered(player.auto["prefix"] + "%d.ts" % second, 5_000_000, 1.0)
            player.second()
        self.assertEqual(kodi.remembered("auto_ceiling:srv-1"), "", "no limit for the next title")
        self.assertEqual((player.restarts, player.bitrates), ([], []), "the stream plays on")
        self.assertEqual(watch.quality, "4m")
        self.assertEqual(xbmcgui.notified, [], "silently")

    def test_a_restart_converted_keeps_the_ceiling(self):
        player = self.player
        kodi.remember("auto_ceiling:srv-1", "6000@%d" % 10**9)
        player.starts(automatic("s1"))
        player.restarting = (7, player.watch, None)
        player.starts(automatic("s2", "original", 15_000, live=True, converted=True))
        self.assertEqual(kodi.remembered("auto_ceiling:srv-1"), "6000@%d" % 10**9)
        self.assertTrue(player.watch.live and player.watch.converted)

    def test_a_conversion_steps_its_bitrate_as_it_is_sent(self):
        player = self.player
        player.starts(automatic("s1", "8m", 8_000, live=True))
        watch = player.watch
        # 750 KB a second through the proxy: 6 Mbps.
        for _ in range(30):
            player.second(arriving=750_000)
        player.second(0.0, caching=True)
        player.second(0.0, caching=True)
        self.assertEqual(player.restarts, [], "no new stream: the screen stays")
        # 6 Mbps for 19 of the last 20 s, with the margin: 4.3, in steps of 0.5.
        self.assertEqual(player.bitrates, [("s1", 4_000)])
        self.assertEqual((watch.quality, watch.playing_kbps, watch.lines), ("8m", 4_000, 1080))
        self.assertEqual(xbmcgui.notified, [("Pley", "Quality lowered to 4 Mbps 1080p")], "said at once: it plays on")
        self.assertEqual(kodi.remembered("auto_ceiling:srv-1").split("@")[0], "4275")

    def test_a_short_wait_pair_leaves_the_original_be(self):
        player = self.player
        player.starts(automatic("s1"))
        for _ in range(30):
            player.second()
        player.second(0.0, caching=True)
        for _ in range(20):
            player.second()
        player.second(0.0, caching=True)
        for _ in range(10):
            player.second()
        self.assertEqual((player.restarts, player.bitrates), ([], []))

    def test_the_player_giving_up_on_the_original_plays_it_converted(self):
        player = self.player
        player.starts(automatic("s1"))
        for _ in range(30):
            player.second()
        player.file = ""
        player.onPlayBackError()
        self.assertEqual(player.restarts, [(7, 30_000, "converted")])
        # Nothing plays while the restart comes: the playback isn't over.
        for _ in range(10):
            player.second(0.0)
        self.assertNotIn("stopped", [state for state, _ in player.sent])
        self.assertIsNotNone(player.reports)
        # The conversion starts, as one of Automatic's.
        player.starts(automatic("s2", "original", 15_000, live=True, converted=True))
        self.assertEqual(player.sent[-3:], [("stopped", 30_000), ("ended", "/api/playback/s1"), ("playing", 30_000)])
        self.assertEqual(xbmcgui.notified, [], "nothing to say")
        # It fails too: over.
        player.restarting = None
        player.file = ""
        player.onPlayBackError()
        self.assertEqual(len(player.restarts), 1)
        self.assertIsNone(player.reports)

    def test_a_failed_restart_after_the_player_gave_up_ends_it(self):
        player = self.player
        player.starts(automatic("s1"))
        player.second()
        player.file = ""
        player.onPlayBackError()
        player.restarting = None
        player.second(0.0)
        # Kodi's word on it is due for 3 s.
        player.lost_since -= 5
        player.second(0.0)
        self.assertIsNone(player.reports)

    def test_a_seek_and_its_wait_arent_the_network(self):
        player = self.player
        player.starts(automatic("s1"))
        for _ in range(30):
            player.second()
        player.onPlayBackSeek(600_000, 570_000)
        for _ in range(8):
            player.second(0.0, caching=True)
        player.second(570.0)
        for _ in range(5):
            player.second()
        self.assertEqual(player.restarts, [])

    def test_a_chosen_quality_isnt_watched(self):
        player = self.player
        playing = automatic("s1")
        del playing["auto"]
        player.starts(playing)
        self.assertIsNone(player.watch)
        for _ in range(10):
            player.second()
        for _ in range(5):
            player.second(0.0, caching=True)
        self.assertEqual(player.restarts, [])

    def test_another_title_gets_its_own_watch(self):
        player = self.player
        player.starts(automatic("s1", "4m", 4_000))
        first = player.watch
        player.restarting = (8, first, "said")
        player.starts(automatic("s2"))
        self.assertIsNot(player.watch, first)
        self.assertEqual(xbmcgui.notified, [])


class Tokens:
    """A sign-in to the stand-in server: its token, renewed once refused."""

    def __init__(self, server):
        self.url = server
        self.token = "t1"
        self.doubts = 0

    def server(self):
        return self.url

    def doubt(self):
        self.doubts += 1

    def session(self):
        return self.url, self.token

    def refreshed(self, token, server):
        assert server == self.url, "a token renewed for its own server only"
        self.token = "t2"
        return self.token


class BitrateTest(unittest.TestCase):
    """PUT /api/playback/{session}/bitrate, with the service's token."""

    def setUp(self):
        self.answers = []
        self.server = support.FakeServer(lambda request: self.answers.pop(0))
        self.player = Player(Tokens(self.server.url), "pleyKodi/test", Now())

    def tearDown(self):
        self.server.close()

    def stepped(self, kbps=4_500):
        from pley.autoquality import Watch

        watch = Watch(0.0, "8m", 8_000, 20_000, 1080, live=True, lines=1080)
        watch.playing_kbps = kbps
        return watch

    def test_the_stream_goes_on_at_what_the_server_says(self):
        self.answers = [support.answer(401, {"code": "unauthorized"}), support.answer(200, {"bitrate": 5_000})]
        watch = self.stepped()
        self.player.set_bitrate(watch, "s 1", 4_500)
        put = self.server.requests[-1]
        self.assertEqual((put.method, put.path, put.json()), ("PUT", "/api/playback/s%201/bitrate", {"max_bitrate": 4_500}))
        self.assertEqual(put.headers["authorization"], "Bearer t2", "the token renewed once refused")
        self.assertEqual((watch.live, watch.playing_kbps, watch.sent_kbps), (True, 5_000, 5_000))

    def test_refused_the_stream_is_as_it_was(self):
        self.answers = [support.answer(400, {"code": "bad_request", "message": "this stream's bitrate can't change as it plays"})]
        watch = self.stepped()
        self.player.set_bitrate(watch, "s1", 4_500)
        self.assertEqual((watch.live, watch.playing_kbps), (False, 8_000))

    def test_a_session_gone_too(self):
        self.answers = [support.answer(404, {"code": "not_found", "message": "playback session expired"})]
        watch = self.stepped()
        self.player.set_bitrate(watch, "s1", 4_500)
        self.assertEqual((watch.live, watch.playing_kbps), (False, 8_000))


class NetworkTest(unittest.TestCase):
    """Automatic's ceiling told to the server (docs/QUALITY.md, "The server")."""

    def setUp(self):
        import tempfile

        self.profile = tempfile.mkdtemp()
        self.saved = dict(xbmcaddon.settings)
        xbmcaddon.settings.update({"_profile": self.profile, "server_id": "srv-1", "video_quality": "auto"})
        self.server = support.FakeServer(lambda request: support.answer(200, {"ceiling_kbps": None}))
        kodi.set_property(kodi.PROXY, self.server.url + "/s3cret")

    def tearDown(self):
        self.server.close()
        kodi.clear_property(kodi.PROXY)
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update(self.saved)

    def told(self):
        return [(r.method, r.path, r.json()) for r in self.server.requests]

    def test_each_ceiling_is_told_through_the_proxy(self):
        from pley import playback

        playback.tell_ceiling("srv-1", 8549.6, "pleyKodi/test")
        playback.tell_ceiling("srv-1", None, "pleyKodi/test")
        me = playback.install_id()
        self.assertEqual(
            self.told(),
            [("PUT", "/s3cret/api/playback/network", {"ceiling_kbps": 8550, "client_id": me}),
             ("PUT", "/s3cret/api/playback/network", {"ceiling_kbps": None, "client_id": me})],
        )
        playback.tell_ceiling("srv-2", 4000, "pleyKodi/test")
        self.assertEqual(len(self.told()), 2, "another server's ceiling isn't this one's")

    def test_automatic_chosen_again_starts_afresh(self):
        from pley import playback
        from pley.service import QualitySetting

        playback.remember_ceiling("srv-1", 3000)
        setting = QualitySetting("pleyKodi/test")
        setting.read()
        self.assertEqual(self.told(), [], "as the service starts: nothing changed")
        xbmcaddon.settings["video_quality"] = "8m"
        setting.read()
        self.assertEqual(self.told(), [])
        xbmcaddon.settings["video_quality"] = "auto"
        setting.read()
        self.assertEqual([body["ceiling_kbps"] for _, _, body in self.told()], [None])
        self.assertIsNone(playback.ceiling())


class RestartTest(unittest.TestCase):
    """The service playing a title again at another quality."""

    def setUp(self):
        from pley import playback

        self.playback = playback
        self.prepare = playback.prepare
        self.asked = []
        kodi.set_property(kodi.PROXY, "http://127.0.0.1:1/secret")

        def prepare(client, base, item_id, start_ms, detail=None, restart=None, kind=None):
            self.asked.append((base, item_id, start_ms, restart, kind))
            kodi.set_property(kodi.PLAYING, json.dumps({"session_id": "new", "path": "http://proxy/new"}))
            return xbmcgui.ListItem(path="http://proxy/new")

        playback.prepare = prepare
        self.player = AutoPlayer()
        self.player.restart = Player.restart.__get__(self.player)
        self.player.starts(dict(automatic("s1"), kind="movie"))

    def tearDown(self):
        self.playback.prepare = self.prepare
        kodi.clear_property(kodi.PLAYING)
        kodi.clear_property(kodi.PROXY)

    def test_it_plays_from_where_it_was(self):
        self.player.restart(7, 754_321, "8m")
        # A movie still, for what the user's remote may do (playback.obeys).
        self.assertEqual(self.asked, [("http://127.0.0.1:1/secret", 7, 754_321, "8m", "movie")])
        (path, li), = self.player.played
        self.assertEqual((path, li.properties["StartOffset"]), ("http://proxy/new", "754.321"))

    def test_stopped_meanwhile_it_isnt_played(self):
        self.player.file = ""
        self.player.onPlayBackStopped()
        self.player.restarting = (7, None, None)
        self.player.restart(7, 1000, "8m")
        self.assertEqual((self.asked, self.player.played), ([], []))
        self.assertIsNone(self.player.restarting)


if __name__ == "__main__":
    unittest.main()
