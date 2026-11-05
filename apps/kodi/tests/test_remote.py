import json
import os
import shutil
import tempfile
import unittest
from unittest import mock

import support
import test_tracks
import xbmc
import xbmcaddon
import xbmcgui
from pley import api, kodi, livetv, playback, remote, tracks
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

    def test_a_video_offers_the_tracks_its_file_has(self):
        video = ["play", "pause", "seek", "skip"]
        self.assertEqual(remote.controls("video"), video)
        self.assertEqual(remote.controls("video", audio=1), video + ["audio"])
        self.assertEqual(remote.controls("video", subtitles=2), video + ["subtitles"])
        self.assertEqual(remote.controls("video", has_next=True, audio=2, subtitles=3), video + ["next", "audio", "subtitles"])
        # Music and live channels change no track.
        self.assertEqual(remote.controls("music", audio=2, subtitles=2), ["play", "pause", "seek", "skip", "next", "previous"])
        self.assertEqual(remote.controls("live", has_order=True, audio=1, subtitles=1), ["next", "previous"])

    def test_a_video_offers_its_quality(self):
        video = ["play", "pause", "seek", "skip"]
        self.assertEqual(remote.controls("video", audio=1, quality=True), video + ["audio", "quality"])
        # Music and live channels have no quality.
        self.assertEqual(remote.controls("music", quality=True), ["play", "pause", "seek", "skip", "next", "previous"])
        self.assertEqual(remote.controls("live", has_order=True, quality=True), ["next", "previous"])


class NoticeRuleTest(unittest.TestCase):
    """What Kodi says for a moment of a command it obeyed on a video."""

    PLAYING = {"session_id": "s1", "state": "playing", "position_ms": 600_000, "duration_ms": 7_200_000, "has_next": False}

    def say(self, asked, state="playing"):
        return remote.notice(asked, remote.obey(dict(self.PLAYING, state=state), asked), kodi.text)

    def test_a_pause_a_play_a_skip_or_a_seek(self):
        self.assertEqual(self.say(command("s1", "pause")), "Paused")
        self.assertEqual(self.say(command("s1", "play"), "paused"), "Playing")
        self.assertEqual(self.say(command("s1", "play")), "", "playing already")
        self.assertEqual(self.say(command("s1", "pause"), "paused"), "", "paused already")
        self.assertEqual(self.say(command("s1", "skip", 30_000)), "+30 s")
        self.assertEqual(self.say(command("s1", "skip", -10_000)), "−10 s", "a real minus")
        self.assertEqual(self.say(command("s1", "skip", 1_500)), "+2 s", "whole seconds")
        self.assertEqual(self.say(command("s1", "skip", -200)), "−1 s", "never 0 for a step there is")
        self.assertEqual(self.say(command("s1", "skip", 0)), "", "no step")
        self.assertEqual(self.say(command("s1", "seek", 754_000)), "Jumped to 12:34")
        self.assertEqual(self.say(command("s1", "seek", 3_723_000)), "Jumped to 1:02:03", "from an hour")
        self.assertEqual(self.say(command("s1", "seek", 0)), "Jumped to 0:00")
        self.assertEqual(self.say(command("s1", "seek", 9_000_000)), "Jumped to 2:00:00", "no further than the end")
        self.assertEqual(self.say(command("s1", "seek")), "", "no place")
        self.assertEqual(self.say(command("s2", "pause")), "", "another session's")
        self.assertEqual(self.say(command("s1", "next")), "", "the next title shows as Kodi plays it")

    def test_the_track_or_quality_picked(self):
        def notice(act, index=None, track=None, quality=None):
            action = {"act": act, "report": True}
            if index is not None:
                action["stream_index"] = index
            if track is not None:
                action["track"] = track
            if quality is not None:
                action["quality"] = quality
            return remote.notice(command("s1", act), action, kodi.text)

        french = {"index": 1, "language": "fre", "name": "Commentary"}
        with mock.patch.object(xbmc, "convertLanguage", lambda code, form: {"fre": "French"}.get(code, ""), create=True):
            self.assertEqual(notice("audio", 2, french), "Audio: French - Commentary")
            self.assertEqual(notice("subtitles", 3, {"index": 1, "language": "fre"}), "Subtitles: French")
        self.assertEqual(notice("subtitles", -1), "Subtitles off")
        self.assertEqual(notice("audio", 2), "", "a track Kodi doesn't list")
        self.assertEqual(notice("none"), "", "the one playing already")
        self.assertEqual(notice("quality", quality="8m"), "Quality: 8 Mbps 1080p")
        self.assertEqual(notice("quality", quality="auto"), "Quality: Automatic")

    def test_a_track_as_kodis_menus_name_it(self):
        with mock.patch.object(xbmc, "convertLanguage", lambda code, form: {"fre": "French"}.get(code, ""), create=True):
            self.assertEqual(remote.track_name({"language": "fre", "name": " Commentary "}), "French - Commentary")
            self.assertEqual(remote.track_name({"language": "fre", "name": ""}), "French")
            self.assertEqual(remote.track_name({"language": "qaa"}), "qaa", "a code Kodi has no name for")
            self.assertEqual(remote.track_name({"language": "", "name": "Signs"}), "Unknown - Signs")
            self.assertEqual(remote.track_name({}), "Unknown")
        self.assertEqual(remote.track_name({"language": "fre"}), "fre", "a Kodi that can't name languages")


class QualityRuleTest(unittest.TestCase):
    """The remote's quality: a film whose menu offers Original, Automatic
    and 8 Mbps 1080p, at Automatic."""

    PLAYER = {"session_id": "s1", "qualities": ["original", "auto", "8m"], "quality": "auto"}

    def obey(self, quality, session="s1", what="quality", **player):
        return remote.obey_quality(dict(self.PLAYER, **player), {"session_id": session, "command": what, "quality": quality})

    def test_a_quality_the_menu_offers(self):
        self.assertEqual(self.obey("8m"), {"act": "quality", "quality": "8m", "report": True})
        self.assertEqual(self.obey("original"), {"act": "quality", "quality": "original", "report": True})
        self.assertEqual(self.obey("auto"), {"act": "none", "report": True}, "the one picked: said again")
        unknown = remote.obey_quality(dict(self.PLAYER, quality=None), {"session_id": "s1", "command": "quality", "quality": "auto"})
        self.assertEqual(unknown, {"act": "quality", "quality": "auto", "report": True}, "the pick not known")

    def test_one_it_doesnt_offer_another_sessions_or_another_command(self):
        for quality in ("30m", "best", None, 8, ""):
            with self.subTest(quality=quality):
                self.assertEqual(self.obey(quality), {"act": "none", "report": False})
        self.assertEqual(self.obey("8m", session="s2"), {"act": "none", "report": False})
        self.assertEqual(self.obey("8m", session_id=None), {"act": "none", "report": False})
        self.assertEqual(self.obey("8m", what="audio"), {"act": "none", "report": False})

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


class TrackRuleTest(unittest.TestCase):
    """The remote's audio and subtitles: a player that can play audio 1 and
    2 and subtitles 3 and 4, playing audio 1 without subtitles."""

    PLAYER = {"session_id": "s1", "audio": [1, 2], "subtitles": [3, 4], "audio_index": 1, "subtitle_index": -1}
    NONE = {"act": "none", "report": False}
    AGAIN = {"act": "none", "report": True}

    def obey(self, what, index, session="s1", **player):
        return remote.obey_track(dict(self.PLAYER, **player), {"session_id": session, "command": what, "stream_index": index})

    def test_audio(self):
        self.assertEqual(self.obey("audio", 2), {"act": "audio", "stream_index": 2, "report": True})
        self.assertEqual(self.obey("audio", 1, audio_index=None), {"act": "audio", "stream_index": 1, "report": True})
        self.assertEqual(self.obey("audio", 1), self.AGAIN, "what plays already: said again")
        for index in (9, 3, -1, None, True, "2"):
            with self.subTest(index=index):
                self.assertEqual(self.obey("audio", index), self.NONE, "a track it can't play, or none")

    def test_subtitles(self):
        self.assertEqual(self.obey("subtitles", 3), {"act": "subtitles", "stream_index": 3, "report": True})
        self.assertEqual(self.obey("subtitles", 3, subtitle_index=4), {"act": "subtitles", "stream_index": 3, "report": True})
        self.assertEqual(self.obey("subtitles", -1, subtitle_index=4), {"act": "subtitles", "stream_index": -1, "report": True})
        self.assertEqual(self.obey("subtitles", 4, subtitle_index=4), self.AGAIN)
        self.assertEqual(self.obey("subtitles", -1), self.AGAIN)
        self.assertEqual(self.obey("subtitles", -1, subtitles=[]), self.AGAIN, "none, without any to pick")
        for index in (9, 2, -2, None):
            with self.subTest(index=index):
                self.assertEqual(self.obey("subtitles", index), self.NONE)

    def test_another_sessions_or_another_command(self):
        self.assertEqual(self.obey("audio", 2, session="s2"), self.NONE)
        self.assertEqual(self.obey("subtitles", -1, session="s2"), self.NONE)
        self.assertEqual(self.obey("audio", 2, session_id=None), self.NONE)
        self.assertEqual(self.obey("pause", None), self.NONE)


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
        for shown in (xbmcgui.notified, xbmcgui.notices, xbmcgui.silent):
            del shown[:]
        self.player = KodiPlayer()

    def said(self):
        """The notifications shown, by their message."""
        return [message for _, message in xbmcgui.notified]

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

    def test_what_a_command_did_shows_for_a_moment(self):
        self.start()
        self.player.heard(command("s1", "pause"))
        xbmc.conditions.add("Player.Paused")
        self.player.onPlayBackPaused()
        self.player.heard(command("s1", "pause"))
        self.player.heard(command("s1", "play"))
        xbmc.conditions.discard("Player.Paused")
        self.player.onPlayBackResumed()
        self.player.heard(command("s1", "play"))
        self.player.heard(command("s1", "skip", 30_000))
        self.player.heard(command("s1", "skip", -10_000))
        self.player.heard(command("s1", "seek", 754_000))
        self.player.heard(command("s1", "next"))
        self.player.heard(command("other", "pause"))
        self.assertEqual(self.said(), ["Paused", "Playing", "+30 s", "−10 s", "Jumped to 12:34"], "nothing of what changes nothing")
        self.assertEqual({(h, i, t) for h, _, i, t in xbmcgui.notices}, {("Pley", kodi.addon_icon(), remote.NOTICE_MS)})
        self.assertEqual(xbmcgui.silent, self.said(), "without Kodi's sound")

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


class QualityTest(Wiring):
    """The remote's quality: Kodi's Quality setting changes, as picking it
    there does, and the title plays again where it is at it."""

    def setUp(self):
        super().setUp()
        self.addCleanup(xbmcaddon.settings.pop, playback.QUALITY, None)
        xbmcaddon.settings[playback.QUALITY] = "auto"
        self.restarted = []
        self.player.restart = lambda item_id, position, quality: self.restarted.append((item_id, position, quality))

    def play(self, kind="movie"):
        playing = {
            "item_id": 7,
            "session_id": "s1",
            "duration_ms": 7_200_000,
            "path": "http://proxy/7",
            "kind": kind,
            "qualities": ["original", "auto", "8m", "4m", "2m"],
            "quality": "auto",
        }
        if kind == "track":
            del playing["qualities"], playing["quality"]
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]
        self.player.seconds = 600.0
        self.player.onAVStarted()
        self.bodies = []
        self.player.send = lambda playing, path, body: self.bodies.append(body)

    def quality(self, quality, session="s1"):
        return dict(command(session, "quality"), quality=quality)

    def test_a_pick_changes_the_setting_and_plays_the_title_again_there(self):
        self.play()
        self.player.heard(self.quality("8m"))
        self.assertEqual(xbmcaddon.settings[playback.QUALITY], "8m", "kept, as a pick in the setting is")
        self.assertEqual(self.restarted, [(7, 600_000, None)], "at the setting, where it was")
        self.assertEqual(self.player.restarting, (7, None, None), "not Automatic's: a watch of its own")
        self.assertEqual(self.bodies, [], "the new session reports as it starts")
        # Played again, it gets a watch of its own (Automatic picked again).
        self.assertEqual(self.player.watching({"item_id": 7, "auto": None}), (None, None))

    def test_the_quality_picked_shows_as_the_setting_names_it(self):
        self.play()
        self.player.heard(self.quality("8m"))
        self.assertEqual(self.said(), ["Quality: 8 Mbps 1080p"])

    def test_the_quality_picked_already_is_said_again(self):
        self.play()
        self.player.heard(self.quality("auto"))
        self.assertEqual(self.said(), [], "nothing changes")
        self.assertEqual((self.restarted, xbmcaddon.settings[playback.QUALITY]), ([], "auto"))
        self.assertEqual([(b["state"], b["position_ms"], b["quality"]) for b in self.bodies], [("playing", 600_000, "auto")])

    def test_one_the_file_doesnt_offer_another_sessions_or_music_does_nothing(self):
        self.play()
        self.player.heard(self.quality("30m"))
        self.player.heard(self.quality("8m", "s2"))
        self.play("track")
        self.player.heard(self.quality("8m"))
        self.assertEqual((self.restarted, self.bodies, xbmcaddon.settings[playback.QUALITY]), ([], [], "auto"))

    def test_every_report_of_a_video_says_the_quality_picked(self):
        self.play()
        self.player.reports.report("playing")
        self.assertEqual(self.bodies[-1]["quality"], "auto")
        self.play("track")
        self.player.reports.report("playing")
        self.assertNotIn("quality", self.bodies[-1], "music has none")


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

    def test_music_says_nothing_of_a_command(self):
        queue(xbmc.PLAYLIST_MUSIC, [5, 7, 9], 1)
        self.start("track", seconds=10.0)
        for asked in (command("s1", "pause"), command("s1", "skip", 30_000), command("s1", "seek", 0), command("s1", "next")):
            self.player.heard(asked)
        self.assertEqual(len(xbmc.asked), 4)
        self.assertEqual(self.said(), [], "the music is left as it was")


def track(what, index, session="s1"):
    """The remote's audio or subtitles command."""
    return dict(command(session, what), stream_index=index)


class TracksTest(Wiring):
    """The remote's audio and subtitles in Kodi's player: the film of
    test_tracks, its audio 1 and 2 and its subtitles 1000 (the Spanish file
    next to it, given to Kodi first), 3, 4 and 5, as Kodi lists them."""

    def setUp(self):
        super().setUp()
        self.kodi = test_tracks.Kodi(subtitles=4)
        self.real_rpc = xbmc.executeJSONRPC

        def rpc(request):
            asked = json.loads(request)
            return json.dumps({"result": self.kodi.rpc(asked["method"], asked["params"])})

        xbmc.executeJSONRPC = rpc
        self.bodies = []
        self.saved = []
        self.player.send = lambda playing, path, body: self.bodies.append(body)
        self.player.call = lambda method, path, body=None, **kw: self.saved.append((method, path, body)) or api.Response(204, {}, b"")

    def tearDown(self):
        xbmc.executeJSONRPC = self.real_rpc
        super().tearDown()

    def play(self, info=None, file=None):
        """The film playing, past the seconds Kodi takes to switch to the
        account's choice; the follower's clock."""
        layout = tracks.layout(info or test_tracks.direct(), file or test_tracks.a_file())
        playing = {"item_id": 7, "session_id": "s1", "duration_ms": 7_200_000, "path": "http://proxy/7", "kind": "movie", "tracks": layout}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]
        self.player.seconds = 600.0
        self.player.onAVStarted()
        clock = test_tracks.Clock()
        self.player.tracks.clock = clock
        self.player.tracks.quiet_until = 0
        self.player.tick()
        del self.kodi.calls[:]
        del self.bodies[:]
        return clock

    def told(self):
        """What each report said of the tracks."""
        return [(b.get("audio_index"), b.get("subtitle_index")) for b in self.bodies]

    def test_a_pick_switches_kodis_player_by_its_place_there(self):
        clock = self.play()
        self.player.heard(track("audio", 2))
        self.player.heard(track("subtitles", 3))
        # The file's English subtitles are Kodi's second: the Spanish file comes first.
        self.assertEqual(self.kodi.calls, [
            ("Player.SetAudioStream", {"stream": 1}),
            ("Player.SetSubtitle", {"subtitle": 1, "enable": True}),
        ])
        self.assertEqual(self.told(), [(2, -1), (2, 3)], "reported at once, by the server's indexes")
        self.assertEqual(self.bodies[0]["position_ms"], 600_000)
        self.player.heard(track("subtitles", 1000))
        self.player.heard(track("subtitles", -1))
        self.assertEqual(self.kodi.calls[2:], [
            ("Player.SetSubtitle", {"subtitle": 0, "enable": True}),
            ("Player.SetSubtitle", {"subtitle": "off"}),
        ])
        self.assertEqual(self.told()[2:], [(2, 1000), (2, -1)])
        # Saved as a pick in Kodi's own menu is, once it held.
        clock.now += tracks.QUIET
        self.player.tick()
        clock.now += tracks.SETTLE
        self.player.tick()
        self.assertEqual(self.saved, [("PUT", "/api/items/7/streams", {"file_id": 7, "audio_index": 2, "subtitle_index": None})])

    def test_a_pick_shows_the_track_as_kodis_menu_names_it(self):
        self.play()
        self.kodi.audios[1].update(language="fre", name="Commentary")
        self.kodi.subtitles[1].update(language="eng", name="SDH")
        with mock.patch.object(xbmc, "convertLanguage", lambda code, form: {"fre": "French", "eng": "English"}.get(code, ""), create=True):
            self.player.heard(track("audio", 2))
            self.player.heard(track("subtitles", 3))
            self.player.heard(track("subtitles", -1))
            self.player.heard(track("subtitles", 9))
        self.assertEqual(self.said(), ["Audio: French - Commentary", "Subtitles: English - SDH", "Subtitles off"])

    def test_what_plays_already_is_said_again(self):
        self.play()
        self.player.heard(track("audio", 1))
        self.player.heard(track("subtitles", -1))
        self.assertEqual(self.kodi.calls, [])
        self.assertEqual(self.told(), [(1, -1), (1, -1)])
        self.assertEqual(self.said(), [], "nothing changes")

    def test_a_track_kodi_cant_play_or_another_sessions_does_nothing(self):
        self.play()
        for asked in (track("audio", 9), track("subtitles", 1001), track("audio", 2, "s2"), track("subtitles", None)):
            self.player.heard(asked)
        self.assertEqual((self.kodi.calls, self.bodies), ([], []))

    def test_nothing_until_the_accounts_choice_is_shown(self):
        # Kodi hasn't listed the subtitles yet: showing the account's choice
        # once it does would undo a pick.
        self.kodi.subtitles = []
        self.play()
        self.player.heard(track("audio", 2))
        self.assertEqual((self.kodi.calls, self.bodies), ([], []))

    def test_every_report_says_what_kodi_plays(self):
        clock = self.play()
        xbmc.conditions.add("Player.Paused")
        self.player.onPlayBackPaused()
        # Changed in Kodi's player: the French audio, the file's French forced subtitles.
        self.kodi.audio, self.kodi.subtitle, self.kodi.enabled = 1, 3, True
        clock.now += 1
        self.player.tick()
        xbmc.conditions.discard("Player.Paused")
        self.player.onPlayBackResumed()
        self.assertEqual(self.told(), [(1, -1), (2, 5)])
        # Captions Kodi found in the video aren't the server's: left out.
        self.kodi.subtitles = test_tracks.kodi_lists(subtitles=4, captions=1)[1]
        self.kodi.subtitle = 4
        self.player.tick()
        self.player.reports.report("playing")
        self.assertEqual(self.told()[-1], (2, None))

    def test_a_converted_stream(self):
        # Its only audio is the server's French; the pictures are drawn on.
        self.kodi = test_tracks.Kodi(audio=1, subtitles=3)
        self.play(test_tracks.streamed(audio=2, burned=4), test_tracks.a_file(audio=2, subtitle=4))
        self.player.reports.report("playing")
        self.assertEqual(self.told(), [(2, 4)])
        for asked in (track("audio", 1), track("subtitles", -1), track("subtitles", 3)):
            self.player.heard(asked)
        self.assertEqual((self.kodi.calls, len(self.bodies)), ([], 1), "Kodi can't switch them")
        self.player.heard(track("audio", 2))
        self.player.heard(track("subtitles", 4))
        self.assertEqual(self.told()[1:], [(2, 4), (2, 4)])

    def test_subtitles_added_in_kodi_end_the_remotes_picks(self):
        self.play()
        self.player.tracks.following = False
        self.player.heard(track("audio", 2))
        self.player.reports.report("playing")
        self.assertEqual(self.kodi.calls, [])
        self.assertEqual(self.told(), [(None, None)], "Kodi's places don't say the server's tracks any more")

    def test_music_changes_no_track(self):
        queue(xbmc.PLAYLIST_MUSIC, [7], 0)
        self.start("track")
        del self.bodies[:]
        self.player.heard(track("audio", 1))
        self.player.reports.report("playing")
        self.assertEqual(self.kodi.calls, [])
        self.assertEqual(self.told(), [(None, None)])


BASE = "http://127.0.0.1:5000/s3cret"
CHANNELS =[{"id": 3, "name": "Three"}, {"id": 5, "name": "Five"}, {"id": 8, "name": "Eight"}]


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
        self.assertEqual(self.said(), [], "the channel shows as Kodi plays it")

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
        # Its details load while the play request goes: audio and subtitles
        # are offered without them.
        asked = self.play()
        self.assertEqual(asked["controls"], ["play", "pause", "seek", "skip", "audio", "subtitles", "quality"])
        self.assertEqual((asked["device_name"], asked["device_kind"]), ("Den", "tv"))
        queue(xbmc.PLAYLIST_VIDEO, [9, 10], 0)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip", "next", "audio", "subtitles", "quality"])
        queue(xbmc.PLAYLIST_VIDEO, [8, 9], 1)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip", "audio", "subtitles", "quality"], "the last of the queue")
        queue(xbmc.PLAYLIST_MUSIC, [9], 0)
        self.assertEqual(self.play()["controls"], ["play", "pause", "seek", "skip", "next", "previous"])

    def test_a_kind_known_from_the_listing_says_what_it_obeys(self):
        # The details load while the play request goes: the listing's kind decides.
        self.assertEqual(self.play(kind="track")["controls"], ["play", "pause", "seek", "skip", "next", "previous"])
        self.assertEqual(self.play(kind="episode")["controls"], ["play", "pause", "seek", "skip", "next", "audio", "subtitles", "quality"])

    def test_the_details_say_which_tracks_are_offered(self):
        film = {"id": 9, "kind": "movie", "title": "Saltwind", "duration_ms": 1000, "files": [test_tracks.a_file()]}
        self.assertEqual(self.play(film)["controls"], ["play", "pause", "seek", "skip", "audio", "subtitles", "quality"])
        no_subtitles = dict(test_tracks.a_file(), streams=[s for s in test_tracks.STREAMS if s["kind"] != "subtitle"])
        self.assertEqual(self.play(dict(film, files=[no_subtitles]))["controls"], ["play", "pause", "seek", "skip", "audio", "quality"])

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
