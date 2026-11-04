import json
import os
import shutil
import tempfile
import unittest

import support
import test_tracks
import xbmc
import xbmcaddon
import xbmcgui
from pley import kodi, livetv, playback, remote
from pley.service import Player

# The shared remote-control rule's cases, as every app runs them: each
# group's {in, out}, a player and a command in, what the player does out.
FIXTURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "packages", "tv", "test", "fixtures", "remote.json")
with open(FIXTURE, encoding="utf-8") as f:
    CASES = json.load(f)


def command(session, what, ms=None):
    """A player_command event, its place or offset null when not given."""
    return {
        "type": "player_command",
        "session_id": session,
        "command": what,
        "position_ms": ms if what == "seek" else None,
        "offset_ms": ms if what == "skip" else None,
    }


class RuleTest(unittest.TestCase):
    def test_every_group_has_cases(self):
        self.assertEqual(sorted(CASES), ["controls", "live", "music", "video"])
        for group, cases in CASES.items():
            self.assertTrue(cases, group)

    def test_what_each_player_obeys(self):
        for case in CASES["controls"]:
            with self.subTest(**case["in"]):
                self.assertEqual(remote.controls(**case["in"]), case["out"])

    def test_a_video(self):
        for case in CASES["video"]:
            player, asked = case["in"]["player"], case["in"]["command"]
            with self.subTest(player=player, command=asked):
                self.assertEqual(remote.obey(player, asked), case["out"])

    def test_music(self):
        for case in CASES["music"]:
            player, asked = case["in"]["player"], case["in"]["command"]
            with self.subTest(player=player, command=asked):
                self.assertEqual(remote.obey(player, asked), case["out"])

    def test_a_live_channel(self):
        for case in CASES["live"]:
            player, asked = case["in"]["player"], case["in"]["command"]
            with self.subTest(player=player, command=asked):
                self.assertEqual(remote.obey_live(player, asked), case["out"])

    def test_a_place_left_out_or_not_a_number_is_none(self):
        player = {"session_id": "s1", "state": "playing", "position_ms": 600000, "duration_ms": 7200000, "has_next": False}
        none = {"act": "none", "report": False}
        self.assertEqual(remote.obey(player, {"session_id": "s1", "command": "seek"}), none)
        self.assertEqual(remote.obey(player, {"session_id": "s1", "command": "skip"}), none)
        self.assertEqual(remote.obey(player, {"session_id": "s1", "command": "seek", "position_ms": "60000"}), none)
        self.assertEqual(remote.obey(player, {"session_id": "s1", "command": "skip", "offset_ms": True}), none)
        self.assertEqual(remote.obey(dict(player, session_id=None), {"session_id": None, "command": "pause"}), none)

    def test_kodi_is_named_by_the_name_it_was_given(self):
        self.addCleanup(xbmc.setting_values.pop, "services.devicename", None)
        self.addCleanup(xbmc.info_labels.pop, "System.FriendlyName", None)
        # Kodi says its own name with the computer's host name: not for remotes.
        xbmc.info_labels["System.FriendlyName"] = "Kodi (DESKTOP-1A2B3C4)"
        xbmc.setting_values["services.devicename"] = "Kodi"
        self.assertEqual(remote.device(), {"device_kind": "tv"}, "the server names it from the User-Agent")
        xbmc.setting_values["services.devicename"] = "  Living   room "
        self.assertEqual(remote.device(), {"device_kind": "tv", "device_name": "Living room"})
        xbmc.setting_values["services.devicename"] = "K" * 80
        self.assertEqual(len(remote.device()["device_name"]), 64)
        xbmc.setting_values["services.devicename"] = ""
        self.assertEqual(remote.device(), {"device_kind": "tv"})
        del xbmc.setting_values["services.devicename"]
        self.assertEqual(remote.device(), {"device_kind": "tv"}, "no such setting")


def queue(kind, ids, at):
    """Kodi's queue of `kind`, the plugin's play route for each of `ids`,
    playing the one at `at`."""
    playlist = xbmc.PlayList(kind)
    playlist.clear()
    for item_id in ids:
        playlist.add("plugin://plugin.video.pley/?route=play&id=%d" % item_id)
    xbmc.PlayList.positions[kind] = at


class Now:
    def put(self, job):
        job()


class KodiPlayer(Player):
    """The service's player, with what Kodi plays stood in for."""

    def __init__(self):
        super().__init__(tokens=None, user_agent="pleyKodi/test", worker=Now())
        self.file = ""
        self.seconds = 0.0
        self.sent = []
        self.following = None

    def find_next(self, reports, moments):
        moments.next_item = self.following

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


class Wiring(unittest.TestCase):
    def setUp(self):
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()
        del xbmc.asked[:]
        del xbmc.played[:]
        self.player = KodiPlayer()

    def tearDown(self):
        xbmc.conditions.discard("Player.Paused")
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()
        kodi.clear_property(kodi.PLAYING)
        kodi.clear_property(kodi.LIVE)

    def start(self, kind="movie", item_id=7, seconds=600.0):
        playing = {"item_id": item_id, "session_id": "s1", "duration_ms": 7_200_000, "path": "http://proxy/%d" % item_id, "kind": kind}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]
        self.player.seconds = seconds
        self.player.onAVStarted()
        del self.player.sent[:]


class VideoTest(Wiring):
    def test_pause_and_play_turn_kodis_pause_once(self):
        self.start()
        self.player.heard(command("s1", "pause"))
        self.assertEqual(xbmc.asked, [("pause",)])
        # Paused already: nothing turns, where it is is said again.
        xbmc.conditions.add("Player.Paused")
        self.player.onPlayBackPaused()
        del self.player.sent[:]
        self.player.heard(command("s1", "pause"))
        self.assertEqual(xbmc.asked, [("pause",)])
        self.assertEqual(self.player.sent, [("paused", 600_000)])
        self.player.heard(command("s1", "play"))
        self.assertEqual(xbmc.asked, [("pause",), ("pause",)])
        xbmc.conditions.discard("Player.Paused")
        del self.player.sent[:]
        self.player.heard(command("s1", "play"))
        self.assertEqual(len(xbmc.asked), 2)
        self.assertEqual(self.player.sent, [("playing", 600_000)])

    def test_a_seek_or_skip_is_reported_at_once(self):
        self.start()
        self.player.heard(command("s1", "skip", -30_000))
        self.assertEqual(xbmc.asked, [("seekTime", 570.0)])
        self.assertEqual(self.player.sent, [("playing", 570_000)])
        self.player.heard(command("s1", "seek", 9_000_000))
        self.assertEqual(xbmc.asked[-1], ("seekTime", 7200.0), "no further than its end")

    def test_a_seek_puts_up_next_away(self):
        self.start("episode")
        moments = self.player.moments
        moments.next_since = 0.0
        closed = []

        class Shown:
            choice = None

            def close(self):
                closed.append(True)

        self.player.overlay, self.player.shown = Shown(), ("next",)
        self.player.heard(command("s1", "skip", -30_000))
        self.assertEqual((closed, self.player.overlay), ([True], None))
        self.assertIsNone(moments.next_since, "its countdown starts again should the place still be in the credits")
        self.assertEqual(xbmc.asked, [("seekTime", 570.0)])

    def test_a_command_that_fails_leaves_the_event_stream_alone(self):
        self.start()

        def stopped(seconds):
            raise RuntimeError("Kodi is not playing any media file")

        self.player.seekTime = stopped
        self.player.heard(command("s1", "seek", 0))
        self.assertTrue(any("failed" in message for _, message in xbmc.logged))

    def test_another_sessions_command_or_another_playback_is_left_alone(self):
        self.start()
        self.player.heard(command("other", "pause"))
        self.player.file = "http://elsewhere/1"
        self.player.heard(command("s1", "pause"))
        self.player.file = ""
        self.player.heard(command("s1", "seek", 0))
        self.assertEqual(xbmc.asked, [], "Kodi stopped, or playing something else, obeys nothing")
        self.assertEqual(self.player.sent, [])

    def test_next_is_kodis_queue(self):
        self.start()
        self.player.heard(command("s1", "next"))
        self.player.heard(command("s1", "previous"))
        self.assertEqual(xbmc.asked, [], "nothing queued after it; previous isn't a video's")
        queue(xbmc.PLAYLIST_VIDEO, [7, 8], 0)
        self.player.heard(command("s1", "next"))
        self.assertEqual(xbmc.asked, [("playnext",)])
        self.assertEqual(self.player.sent, [], "the next title reports with its own session")

    def test_next_on_an_episode_plays_the_one_after_it(self):
        self.start("episode")
        self.player.heard(command("s1", "next"))
        self.assertEqual((xbmc.asked, xbmc.played), ([], []), "the show's last: nothing after it")
        self.player.following = {"id": 8, "kind": "episode"}
        self.start("episode")
        self.player.heard(command("s1", "next"))
        self.assertEqual(xbmc.asked, [])
        self.assertEqual([item for item, _, _ in xbmc.played], ["plugin://plugin.video.pley/?route=play&id=8&kind=episode"])
        self.assertEqual(self.player.sent[0], ("stopped", 600_000), "this one is over, where it was")
        del xbmc.played[:]
        queue(xbmc.PLAYLIST_VIDEO, [7, 9], 0)
        self.start("episode")
        self.player.heard(command("s1", "next"))
        self.assertEqual((xbmc.asked, xbmc.played), ([("playnext",)], []), "Kodi's queue first")


class MusicTest(Wiring):
    def test_previous_starts_the_track_again_or_plays_the_one_before(self):
        queue(xbmc.PLAYLIST_MUSIC, [5, 7, 9], 1)
        self.start("track", seconds=2.0)
        self.player.heard(command("s1", "previous"))
        self.assertEqual(xbmc.asked, [("playprevious",)])
        self.player.seconds = 10.0
        self.player.heard(command("s1", "previous"))
        self.assertEqual(xbmc.asked[-1], ("seekTime", 0.0))
        self.assertEqual(self.player.sent, [("playing", 0)])
        self.player.heard(command("s1", "next"))
        self.assertEqual(xbmc.asked[-1], ("playnext",))

    def test_the_first_track_starts_again_and_the_last_has_no_next(self):
        queue(xbmc.PLAYLIST_MUSIC, [7], 0)
        self.start("track", seconds=1.0)
        self.player.heard(command("s1", "previous"))
        self.player.heard(command("s1", "next"))
        self.assertEqual(xbmc.asked, [("seekTime", 0.0)])


BASE = "http://127.0.0.1:5000/s3cret"
CHANNELS = [{"id": 3, "name": "Three"}, {"id": 5, "name": "Five"}, {"id": 8, "name": "Eight"}]


class LiveTest(Wiring):
    def setUp(self):
        super().setUp()
        livetv.queue(BASE, CHANNELS, 1, 0)
        kodi.set_property(kodi.LIVE, json.dumps({"session_id": "w1", "channel_id": 5}))
        self.player.file = BASE + "/api/live/channels/5/stream.ts?client_id=kodi-1-stream&device_kind=tv"

    def test_next_and_previous_change_channel_in_the_queue(self):
        self.assertEqual(livetv.queued_channels(), [3, 5, 8])
        self.player.heard(command("w1", "next"))
        self.player.heard(command("w1", "previous"))
        # In Kodi's video queue of channels, whatever this player played before.
        played = [(item.kind, start) for item, _, start in xbmc.played]
        self.assertEqual(played, [(xbmc.PLAYLIST_VIDEO, 2), (xbmc.PLAYLIST_VIDEO, 0)])
        self.assertEqual(xbmc.asked, [])

    def test_only_the_channel_playing_obeys(self):
        self.player.heard(command("w0", "next"))
        self.player.heard(command("w1", "pause"))
        self.player.file = BASE + "/api/live/channels/8/stream.ts?client_id=kodi-1-stream&device_kind=tv"
        self.player.heard(command("w1", "next"))
        self.player.file = ""
        self.player.heard(command("w1", "next"))
        self.assertEqual(xbmc.asked, [])

    def test_without_a_queue_nothing_changes_channel(self):
        livetv.forget_queue()
        self.assertIsNone(livetv.queued_channels())
        self.player.heard(command("w1", "next"))
        self.assertEqual(xbmc.asked, [])

    def test_stopped_from_another_device_it_stops_as_one_taken_over(self):
        said = []
        self.player.taken_over = lambda channel, message: said.append((channel, message))
        self.player.heard({"type": "session_stopped", "session_id": "w0", "message": None, "by_admin": False})
        self.assertEqual(said, [], "another session's")
        self.player.heard({"type": "session_stopped", "session_id": "w1", "message": None, "by_admin": False})
        self.player.heard({"type": "session_stopped", "session_id": "w1", "message": "Maintenance tonight", "by_admin": True})
        self.assertEqual(said, [
            (5, "Live TV was stopped from another device."),
            (5, "Live TV was stopped by the server's administrator.\nMaintenance tonight"),
        ])


class RequestsTest(unittest.TestCase):
    """What play and watch requests tell the server for the remote."""

    def setUp(self):
        self.profile = tempfile.mkdtemp()
        xbmcaddon.settings["_profile"] = self.profile
        xbmc.setting_values["services.devicename"] = "Den"
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()

    def tearDown(self):
        xbmcaddon.settings.pop("_profile", None)
        xbmc.setting_values.pop("services.devicename", None)
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()
        kodi.clear_property(kodi.PLAYING)
        kodi.clear_property(kodi.LIVE)
        shutil.rmtree(self.profile, ignore_errors=True)

    def play(self, detail=None, kind=None):
        item = {"id": 9, "kind": "movie", "title": "Saltwind", "duration_ms": 1000, "files": [test_tracks.a_file()]}
        server = test_tracks.Server(item, [test_tracks.streamed()])
        playback.prepare(server, test_tracks.BASE, 9, 0, detail, kind=kind)
        return server.posts[0]

    def test_a_title_says_what_it_obeys_and_where_it_plays(self):
        asked = self.play()
        self.assertEqual(asked["controls"], ["play", "pause", "seek", "skip"])
        self.assertEqual((asked["device_name"], asked["device_kind"]), ("Den", "tv"))
        queue(xbmc.PLAYLIST_VIDEO, [9, 10], 0)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip", "next"])
        queue(xbmc.PLAYLIST_VIDEO, [8, 9], 1)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip"], "the last of the queue")
        queue(xbmc.PLAYLIST_MUSIC, [9], 0)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip", "next", "previous"])

    def test_a_kind_known_from_the_listing_says_what_it_obeys(self):
        # The details load while the play request goes: the listing's kind decides.
        self.assertEqual(self.play(kind="track")["controls"], ["play", "pause", "seek", "skip", "next", "previous"])
        self.assertEqual(self.play(kind="episode")["controls"], ["play", "pause", "seek", "skip", "next"])

    def test_a_channel_obeys_channel_up_and_down_in_a_queue(self):
        class Server:
            def __init__(self):
                self.asked = []

            def post(self, path, body=None):
                self.asked.append(body)
                return {"session_id": "w1", "url": "/api/live/sessions/w1/index.m3u8", "kind": "hls", "channel": CHANNELS[1]}

        server = Server()
        livetv.watch(server, 5, "kodi-1")
        self.assertEqual(server.asked[0], {"client_id": "kodi-1", "controls": [], "device_kind": "tv", "device_name": "Den"})
        self.assertEqual(json.loads(kodi.get_property(kodi.LIVE)), {"session_id": "w1", "channel_id": 5})
        livetv.queue(BASE, CHANNELS, 1, 0)
        livetv.watch(server, 5, "kodi-1")
        self.assertEqual(server.asked[1]["controls"], ["next", "previous"])


if __name__ == "__main__":
    unittest.main()
