import tempfile
import unittest

import support  # noqa: F401
import xbmc
import xbmcaddon
from pley import api, autoquality, playback

BASE = "http://127.0.0.1:5000/s"
TRACKS = [
    {"index": 3, "language": "eng", "forced": False, "external": False, "text_based": True, "url": "/api/files/7/subtitles/3.vtt"},
    {"index": 4, "language": None, "forced": False, "external": False, "text_based": False, "url": None},
    {"index": 1000, "language": "fre", "forced": True, "external": True, "text_based": True, "url": "/api/files/7/subtitles/1000.vtt"},
]


class PlaybackTest(unittest.TestCase):
    def test_text_subtitles_beside_what_plays(self):
        # A converted stream carries no subtitles: every text track comes beside it.
        converted = playback.beside(BASE, {"hls": True, "subtitles": TRACKS})
        self.assertEqual(converted, [BASE + "/api/files/7/subtitles/3.eng.vtt", BASE + "/api/files/7/subtitles/1000.fre.forced.vtt"])
        # A file played as it is: Kodi reads its own tracks, and gets the files next to it.
        direct = playback.beside(BASE, {"hls": False, "subtitles": TRACKS})
        self.assertEqual(direct, [BASE + "/api/files/7/subtitles/1000.fre.forced.vtt"])
        self.assertEqual(playback.beside(BASE, {"hls": False, "subtitles": None}), [])

    def test_pgs_files_are_named_for_kodi_too(self):
        self.assertEqual(api.subtitle_name("/api/files/7/subtitles/4.sup", "eng", True), "/api/files/7/subtitles/4.eng.forced.sup")
        self.assertEqual(api.subtitle_name("/api/files/7/subtitles/4.sup", None, False), "/api/files/7/subtitles/4.sup")

    def test_subtitles_only_named_forced_are_named_forced_for_kodi(self):
        # Many files flag no forced track and only name it: the server counts
        # it forced, and Kodi would list it as a second "English".
        named = [
            {"index": 6, "language": "eng", "title": "English", "forced": False, "external": False, "text_based": True, "url": "/api/files/1/subtitles/6.vtt"},
            {"index": 7, "language": "eng", "title": "English (Forced)", "forced": False, "external": False, "text_based": True, "url": "/api/files/1/subtitles/7.vtt"},
            {"index": 4, "language": "eng", "title": "Signs & Songs", "forced": False, "external": False, "text_based": True, "url": "/api/files/1/subtitles/4.vtt"},
            {"index": 8, "language": "eng", "title": "Forcedly Bilingual", "forced": False, "external": False, "text_based": True, "url": "/api/files/1/subtitles/8.vtt"},
        ]
        converted = playback.beside(BASE, {"hls": True, "subtitles": named})
        self.assertEqual(
            [path.rsplit("/", 1)[-1] for path in converted],
            ["6.eng.vtt", "7.eng.forced.vtt", "4.eng.forced.vtt", "8.eng.vtt"],
        )


class QualityTest(unittest.TestCase):
    """The quality setting and how a playback is asked for (docs/QUALITY.md)."""

    def setUp(self):
        self.profile = tempfile.mkdtemp()
        self.saved = dict(xbmcaddon.settings)
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update({"_profile": self.profile, "server_id": "srv-1"})

    def tearDown(self):
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update(self.saved)

    def test_automatic_when_nothing_was_chosen(self):
        self.assertEqual(playback.chosen_quality(), "auto")
        xbmcaddon.settings["quality"] = 0
        self.assertEqual(playback.chosen_quality(), "auto", "Original or never changed: they can't be told apart")

    def test_a_quality_chosen_before_is_carried_over_once(self):
        xbmcaddon.settings["quality"] = 3  # 15 Mbps 4K in the old order
        self.assertEqual(playback.chosen_quality(), "15m")
        self.assertEqual((xbmcaddon.settings["video_quality"], xbmcaddon.settings["quality"]), ("15m", 0))
        xbmcaddon.settings["video_quality"] = "original"
        self.assertEqual(playback.chosen_quality(), "original", "a choice made since is kept")
        xbmcaddon.settings["quality"] = 99
        self.assertEqual(playback.chosen_quality(), "original", "nonsense isn't carried")
        self.assertEqual(xbmcaddon.settings["quality"], 0)

    def test_how_each_choice_is_asked_for(self):
        xbmcaddon.settings["video_quality"] = "original"
        self.assertEqual(playback.ask(), dict(automatic=False, quality="original", lowered=False, mode="auto", max_bitrate=None, max_height=None))
        # A quality: the server converts a file over it, and plays one under it as it is.
        xbmcaddon.settings["video_quality"] = "12m"
        self.assertEqual(playback.ask(), dict(automatic=False, quality="12m", lowered=False, mode="auto", max_bitrate=12000, max_height=1080))
        xbmcaddon.settings["video_quality"] = "auto"
        self.assertEqual(playback.ask(), dict(automatic=True, quality="original", lowered=False, mode="auto", max_bitrate=None, max_height=None))
        # The service's restarts.
        self.assertEqual(playback.ask("4m"), dict(automatic=True, quality="4m", lowered=False, mode="transcode", max_bitrate=4000, max_height=720))
        self.assertEqual(playback.ask("original")["mode"], "auto")

    def test_automatic_starts_under_a_ceiling_remembered_for_the_server(self):
        xbmcaddon.settings["video_quality"] = "auto"
        playback.remember_ceiling("srv-1", 9_450)
        self.assertEqual(playback.ask(), dict(automatic=True, quality="8m", lowered=True, mode="auto", max_bitrate=8000, max_height=1080))
        xbmcaddon.settings["server_id"] = "srv-2"
        self.assertEqual(playback.ask()["quality"], "original", "another server's network")
        xbmcaddon.settings["server_id"] = "srv-1"
        playback.remember_ceiling("srv-1", None)
        self.assertEqual(playback.ask()["quality"], "original", "forgotten")

    def test_a_server_that_converts_no_video_offers_the_original_alone(self):
        file = {"id": 3, "bitrate": 24_000_000, "streams": [{"kind": "video", "height": 2160}]}
        self.assertEqual(playback.qualities(file, True), ["original"])
        self.assertEqual(playback.qualities(None, True), ["original"])
        self.assertEqual(playback.qualities(file)[:2], ["original", "auto"])
        self.assertGreater(len(playback.qualities(file)), 2)

    def test_what_automatic_is_told_of_a_playback(self):
        file = {"id": 3, "bitrate": 24_000_000, "streams": [{"kind": "video", "height": 2160}]}
        asked = playback.ask("8m")
        info = {"method": "transcode", "url": "/api/playback/s1/master.m3u8", "auto_quality": None}
        found = playback.automatic(asked, info, file)
        self.assertEqual(
            found,
            {"quality": "8m", "playing_kbps": 8000, "file_kbps": 24_000, "file_lines": 2160, "cap": None, "lowered": False,
             "server": "srv-1", "prefix": "/api/playback/s1/", "converted": True, "lines": 1080, "live": False},
        )
        # A server whose conversions change their bitrate as they are sent.
        self.assertTrue(playback.automatic(asked, dict(info, live_bitrate=True), file)["live"])
        # Under a remembered ceiling the file fitted: played as it is.
        lowered = dict(playback.ask("8m"), lowered=True)
        direct = playback.automatic(lowered, {"method": "direct", "url": "/api/files/3/stream", "live_bitrate": False}, dict(file, bitrate=6_000_000))
        self.assertEqual((direct["quality"], direct["playing_kbps"], direct["lowered"]), ("original", 6000, False))
        self.assertEqual((direct["converted"], direct["lines"], direct["live"]), (False, 2160, False))
        # The original converted: at the server's own quality, which caps Automatic.
        converted = playback.automatic(playback.ask("original"), dict(info, auto_quality="15m", live_bitrate=True), file)
        self.assertEqual((converted["quality"], converted["playing_kbps"], converted["cap"]), ("original", 15000, "15m"))
        self.assertEqual((converted["converted"], converted["lines"], converted["live"]), (True, 2160, True))
        # A wide 1080p film is named 1080p, though 800 lines tall.
        wide = {"id": 4, "bitrate": 10_000_000, "width": 1920, "height": 800}
        self.assertEqual(playback.automatic(playback.ask("8m"), info, wide)["lines"], 1080)
        self.assertIsNone(playback.automatic(asked, info, None)["file_kbps"], "details that didn't come")

    def test_the_original_converted_after_the_player_gave_up(self):
        self.assertEqual(
            playback.ask("converted"),
            dict(automatic=True, quality="original", lowered=False, mode="transcode", max_bitrate=None, max_height=None),
            "at the server's own quality",
        )


class Network:
    """A client for the network's routes: GET's answer (or refusal), what
    the speed test brings, and PUT's answer."""

    def __init__(self, answer, pieces=(), put=None):
        self.answer, self.pieces, self.put_answer = answer, list(pieces), put
        self.puts, self.measured = [], []

    def get(self, path, **query):
        self.asked = (path, query)
        if isinstance(self.answer, api.ApiError):
            raise self.answer
        return self.answer

    def put(self, path, body=None):
        self.puts.append((path, body))
        if isinstance(self.put_answer, api.ApiError):
            raise self.put_answer
        return self.put_answer

    def measure(self, path, query, test, deadline, clock):
        self.measured.append((path, query, deadline))
        for at, size in self.pieces:
            test.chunk(at, size)
        return 200


class StartingQualityTest(unittest.TestCase):
    """Automatic's start from what the server knows of the network."""

    def setUp(self):
        self.profile = tempfile.mkdtemp()
        self.saved = dict(xbmcaddon.settings)
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update({"_profile": self.profile, "server_id": "srv-1", "video_quality": "auto"})
        self.now = 100.0

    def tearDown(self):
        xbmcaddon.settings.clear()
        xbmcaddon.settings.update(self.saved)

    def clock(self):
        return self.now

    def test_the_servers_ceiling_takes_the_place_of_this_kodis(self):
        playback.remember_ceiling("srv-1", 3000)
        client = Network({"local": False, "ceiling_kbps": 9000, "source": "playback", "since": 1, "probe": False})
        self.assertEqual(playback.ask(client=client)["quality"], "8m")
        self.assertEqual(client.asked, ("/api/playback/network", {"client_id": playback.install_id()}))
        client = Network({"local": True, "ceiling_kbps": None, "source": None, "since": None, "probe": False})
        self.assertEqual(playback.ask(client=client)["quality"], "original", "nothing known: the original")
        self.assertEqual(client.puts, [])

    def test_without_the_servers_word_this_kodis_memory(self):
        playback.remember_ceiling("srv-1", 3000)
        for refusal in (api.ApiError(404, "not_found", "no route"), api.ApiError(0, "unreachable", "timed out")):
            self.assertEqual(playback.ask(client=Network(refusal)), dict(automatic=True, quality="2m", lowered=True, mode="auto", max_bitrate=2000, max_height=480))

    def test_a_speed_test_first_where_the_server_asks_for_one(self):
        pieces = [(100.0, 262_144), (101.0, 1_000_000), (102.0, 1_000_000)]
        told = {"local": False, "ceiling_kbps": 5600, "source": "probe", "since": 2, "probe": False}
        client = Network({"local": False, "ceiling_kbps": None, "probe": True}, pieces, told)
        self.assertEqual(playback.starting_ceiling(client, self.clock), 5600)
        self.assertEqual(client.measured, [("/api/playback/speedtest", {"bytes": 33554432}, 103.5)])
        self.assertEqual(client.puts, [("/api/playback/network", {"probe_kbps": 8000, "probe_complete": False, "client_id": playback.install_id()})])
        self.assertIn("ActivateWindow(busydialognocancel)", xbmc.builtins)
        self.assertEqual(xbmc.builtins[-1], "Dialog.Close(busydialognocancel)")
        # The server didn't keep it: 70 % of it all the same.
        client = Network({"probe": True}, pieces, api.ApiError(500, "internal", "no"))
        self.assertEqual(playback.starting_ceiling(client, self.clock), 5600)

    def test_too_little_measured_is_as_if_nothing_were_known(self):
        playback.remember_ceiling("srv-1", 3000)
        client = Network({"probe": True, "ceiling_kbps": None}, [(100.0, 100_000)])
        self.assertIsNone(playback.starting_ceiling(client, self.clock))
        self.assertEqual(client.puts, [], "nothing is told")

    def test_original_and_chosen_qualities_never_ask(self):
        for chosen in ("original", "12m"):
            xbmcaddon.settings["video_quality"] = chosen
            client = Network(api.ApiError(500, "x", "asked"))
            playback.ask(client=client)
            self.assertFalse(hasattr(client, "asked"), chosen)
        self.assertFalse(hasattr(Network(None), "asked"))
        client = Network(None)
        playback.ask("4m", client)
        self.assertFalse(hasattr(client, "asked"), "a restart at a quality")


if __name__ == "__main__":
    unittest.main()
