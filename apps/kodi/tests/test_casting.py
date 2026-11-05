import json
import os
import shutil
import tempfile
import unittest
import urllib.parse

import support
import xbmc
import xbmcaddon
import xbmcgui
from pley import api, casting, kodi, profiles, requests
from pley.service import Player, Reports, receiver_query

BASE = "http://127.0.0.1:5000/s3cret"
MINE = "kodi-1"


def cast(receiver=MINE, item_id=None, playlist_id=None, channel_id=None, start_ms=None, shuffle=False, device="Living room phone"):
    """A cast event, as the server sends it."""
    return {
        "type": "cast",
        "receiver_id": receiver,
        "item_id": item_id,
        "playlist_id": playlist_id,
        "channel_id": channel_id,
        "start_ms": start_ms,
        "shuffle": shuffle,
        "from": device,
    }


def from_profile(profile=4, ticket="t-1", **named):
    """A cast from a remote on another profile of the household."""
    return dict(cast(**named), profile_id=profile, switch_ticket=ticket)


def played():
    """What Kodi's player was given: a plugin address, or a queue's kind,
    its entries' addresses and where it starts."""
    found = []
    for item, _, start in xbmc.played:
        if isinstance(item, xbmc.PlayList):
            found.append((item.kind, [url for url, _ in xbmc.PlayList.entries.get(item.kind, [])], start))
        else:
            found.append(item)
    return found


def route(**params):
    return "plugin://plugin.video.pley/?" + urllib.parse.urlencode(params)


class Server:
    """The server through the service's proxy: GETs answered by path, each noted."""

    def __init__(self, answers):
        self.answers, self.asked = answers, []

    def get(self, path, **query):
        self.asked.append(path)
        if path not in self.answers:
            raise api.ApiError(404, "not_found", "not found")
        return self.answers[path]


def track(n, album=40):
    return {"id": n, "kind": "track", "title": "Song %d" % n, "parent_id": album}


class Clean(unittest.TestCase):
    def setUp(self):
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()
        xbmc.PlayList.shuffled.clear()
        del xbmc.played[:]

    def tearDown(self):
        xbmc.PlayList.entries.clear()
        xbmc.PlayList.positions.clear()
        xbmc.PlayList.shuffled.clear()
        del xbmc.played[:]


class RuleTest(unittest.TestCase):
    """Whose cast it is, what it names, and the words it shows."""

    def test_its_own_casts_while_it_takes_them(self):
        self.assertEqual(casting.action(MINE, True, cast(item_id=5, start_ms=60000)),
                         {"act": "item", "item_id": 5, "start_ms": 60000, "shuffle": False, "audio_index": None, "subtitle_index": None})
        self.assertEqual(casting.action(MINE, True, cast(item_id=5)),
                         {"act": "item", "item_id": 5, "start_ms": None, "shuffle": False, "audio_index": None, "subtitle_index": None})
        self.assertEqual(casting.action(MINE, True, cast(item_id=6, shuffle=True))["shuffle"], True)
        self.assertEqual(casting.action(MINE, True, cast(playlist_id=3, shuffle=True)), {"act": "playlist", "playlist_id": 3, "shuffle": True})
        self.assertEqual(casting.action(MINE, True, cast(channel_id=7)), {"act": "channel", "channel_id": 7})
        self.assertEqual(casting.action(MINE, True, cast(item_id=5, start_ms=-20))["start_ms"], 0)

    def test_anyone_elses_or_while_it_cant_take_them(self):
        none = {"act": "none"}
        self.assertEqual(casting.action(MINE, True, cast("kodi-2", item_id=5)), none, "another device's")
        self.assertEqual(casting.action(MINE, True, cast("kodi-2", channel_id=7)), none)
        self.assertEqual(casting.action(MINE, False, cast(item_id=5)), none, "signed out, or Who's watching? on screen")
        self.assertEqual(casting.action(MINE, True, cast()), none, "nothing named")
        self.assertEqual(casting.action(MINE, True, cast(item_id="5")), none)
        self.assertEqual(casting.action(MINE, True, cast(item_id=True)), none)
        self.assertEqual(casting.action("", True, cast("", item_id=5)), none)

    def test_a_cast_from_another_profile_switches_to_it_with_its_ticket(self):
        self.assertIsNone(casting.switch_to(cast(item_id=5), 2), "the profile's own remote")
        self.assertIsNone(casting.switch_to(dict(cast(item_id=5), profile_id=None, switch_ticket=None), 2))
        self.assertEqual(casting.switch_to(from_profile(item_id=5), 2), {"profile_id": 4, "ticket": "t-1"})
        self.assertEqual(casting.switch_to(from_profile(item_id=5), None), {"profile_id": 4, "ticket": "t-1"},
                         "on the profile picked as it signed in")
        self.assertIsNone(casting.switch_to(from_profile(2, item_id=5), 2), "on it already")
        self.assertIsNone(casting.switch_to(from_profile(2, None, item_id=5), 2))

    def test_another_profiles_cast_without_a_ticket_does_nothing(self):
        for ticket in (None, "", 7, "two words", "line\n", "x" * 257):
            self.assertEqual(casting.switch_to(from_profile(4, ticket, item_id=5), 2), casting.IGNORED, repr(ticket))
        for profile in ("4", True, 0, -4, 4.5):
            self.assertEqual(casting.switch_to(from_profile(profile, item_id=5), 2), casting.IGNORED, repr(profile))

    def test_where_it_came_from(self):
        self.assertEqual(casting.banner("Living room phone"), "Playing from Living room phone")
        self.assertEqual(casting.banner("  Firefox \n on   Windows "), "Playing from Firefox on Windows")
        self.assertEqual(casting.banner(None), "Playing from another device")
        self.assertEqual(casting.banner("  "), "Playing from another device")
        self.assertEqual(len(casting.banner("x" * 100)), len("Playing from ") + 64)

    def test_what_a_title_plays_by_its_kind(self):
        kinds = ("movie", "episode", "extra", "show", "season", "album", "artist", "track", "photo", None)
        self.assertEqual([casting.plays(k) for k in kinds],
                         ["video", "video", "video", "next_episode", "next_episode", "album", "artist", "track", "nothing", "nothing"])

    def test_the_receiver_query(self):
        self.addCleanup(xbmc.setting_values.pop, "services.devicename", None)
        xbmc.setting_values["services.devicename"] = "Den"
        self.assertEqual(casting.receiver_query(MINE, False), {"receiver": MINE, "device_kind": "tv", "device_name": "Den", "casts": "item"})
        self.assertEqual(casting.receiver_query(MINE, True)["casts"], "item,live")
        # Kodi's own name isn't sent: the server names it, as its plays.
        xbmc.setting_values["services.devicename"] = "Kodi"
        self.assertEqual(casting.receiver_query(MINE, True), {"receiver": MINE, "device_kind": "tv", "casts": "item,live"})


class PlayTest(Clean):
    """What a cast plays, as the add-on's own Play plays it."""

    def play(self, answers, what, current=lambda: True):
        self.server = Server(answers)
        return casting.play(self.server, BASE, what, xbmc.Player(), current)

    def item(self, item_id, start_ms=None, shuffle=False):
        return {"act": "item", "item_id": item_id, "start_ms": start_ms, "shuffle": shuffle}

    def test_a_movie_from_where_it_was_sent_or_where_play_starts_it(self):
        film = {"id": 5, "kind": "movie", "duration_ms": 7_200_000, "user_state": {"position_ms": 1_200_000, "watched": False}}
        answers = {"/api/items/5": film}
        self.assertEqual(self.play(answers, self.item(5, start_ms=60000)), casting.PLAYED)
        self.assertEqual(self.play(answers, self.item(5)), casting.PLAYED)
        answers["/api/items/5"] = dict(film, user_state={"position_ms": 1_200_000, "watched": True})
        self.play(answers, self.item(5))
        self.assertEqual(played(), [
            route(route="play", id=5, kind="movie", start=60000),
            route(route="play", id=5, kind="movie", start=1_200_000),
            route(route="play", id=5, kind="movie", start=0),
        ])

    def test_a_movie_with_the_tracks_picked_on_the_remotes_page(self):
        streams = [{"kind": "video", "index": 0}, {"kind": "audio", "index": 1}, {"kind": "audio", "index": 2}, {"kind": "subtitle", "index": 3}]
        other = [{"kind": "video", "index": 0}, {"kind": "audio", "index": 7}, {"kind": "subtitle", "index": 8}]
        film = {"id": 5, "kind": "movie", "duration_ms": 7_200_000, "files": [{"id": 50, "streams": streams}, {"id": 51, "streams": other}]}
        answers = {"/api/items/5": film}
        picked = lambda audio, subtitle: dict(self.item(5, start_ms=0), audio_index=audio, subtitle_index=subtitle)  # noqa: E731
        self.play(answers, picked(2, 3))
        self.play(answers, picked(1, -1))
        self.play(answers, picked(None, 3))
        # Another version's: the account's choice stays.
        self.play(answers, picked(7, 8))
        self.assertEqual(played(), [
            route(route="play", id=5, kind="movie", start=0, audio=2, subtitle=3),
            route(route="play", id=5, kind="movie", start=0, audio=1, subtitle=-1),
            route(route="play", id=5, kind="movie", start=0, subtitle=3),
            route(route="play", id=5, kind="movie", start=0),
        ])

    def test_the_cast_names_the_tracks_picked(self):
        found = casting.action(MINE, True, dict(cast(item_id=5), audio_index=2, subtitle_index=-1))
        self.assertEqual((found["audio_index"], found["subtitle_index"]), (2, -1))
        found = casting.action(MINE, True, dict(cast(item_id=5), audio_index=-1, subtitle_index=-3))
        self.assertEqual((found["audio_index"], found["subtitle_index"]), (None, None), "out of range: not named")
        found = casting.action(MINE, True, dict(cast(item_id=5), audio_index="2", subtitle_index=True))
        self.assertEqual((found["audio_index"], found["subtitle_index"]), (None, None))

    def test_a_show_or_a_season_plays_its_next_episode(self):
        episode = {"id": 8, "kind": "episode", "duration_ms": 2_400_000, "user_state": {"position_ms": 600_000}}
        answers = {"/api/items/3": {"id": 3, "kind": "show"}, "/api/items/3/on-deck": {"item": episode}}
        self.assertEqual(self.play(answers, self.item(3, start_ms=60000)), casting.PLAYED)
        self.assertEqual(played(), [route(route="play", id=8, kind="episode", start=600_000)], "where its Play starts it")
        answers["/api/items/3/on-deck"] = {"item": None}
        self.assertEqual(self.play(answers, self.item(3)), casting.NOTHING)
        self.assertEqual(len(xbmc.played), 1)

    def test_an_album_or_an_artist_as_kodis_music_queue(self):
        answers = {"/api/items/40": {"id": 40, "kind": "album"}, "/api/items/40/children": {"items": [track(41), track(42)]}}
        self.play(answers, self.item(40))
        songs = [route(route="play", id=41), route(route="play", id=42)]
        self.assertEqual(played(), [(xbmc.PLAYLIST_MUSIC, songs, 0)])
        self.assertFalse(xbmc.PlayList.shuffled[xbmc.PLAYLIST_MUSIC])
        self.play(answers, self.item(40, shuffle=True))
        self.assertTrue(xbmc.PlayList.shuffled[xbmc.PLAYLIST_MUSIC])
        # An artist album by album, the newest first, as its page lists them.
        answers.update({
            "/api/items/30": {"id": 30, "kind": "artist"},
            "/api/items/30/children": {"items": [{"id": 50, "kind": "album", "year": 2001}, {"id": 40, "kind": "album", "year": 2020}]},
            "/api/items/50/children": {"items": [track(51, 50)]},
        })
        del xbmc.played[:]
        self.play(answers, self.item(30))
        self.assertEqual(played(), [(xbmc.PLAYLIST_MUSIC, songs + [route(route="play", id=51)], 0)])
        answers["/api/items/40/children"] = {"items": []}
        self.assertEqual(self.play(answers, self.item(40)), casting.NOTHING)

    def test_a_track_plays_its_album_from_it(self):
        answers = {"/api/items/42": track(42), "/api/items/40/children": {"items": [track(41), track(42), track(43)]}}
        self.play(answers, self.item(42, start_ms=30000))
        songs = [route(route="play", id=41), route(route="play", id=42, start=30000), route(route="play", id=43)]
        self.assertEqual(played(), [(xbmc.PLAYLIST_MUSIC, songs, 1)])
        # Without an album: on its own.
        del xbmc.played[:]
        self.play({"/api/items/42": dict(track(42), parent_id=None)}, self.item(42))
        self.assertEqual(played(), [(xbmc.PLAYLIST_MUSIC, [route(route="play", id=42)], 0)])

    def test_a_playlist_plays_what_it_holds(self):
        songs = {"id": 3, "kind": "audio", "entries": [{"entry_id": 1, "item": track(41)}, {"entry_id": 2, "item": track(42)}]}
        self.assertEqual(self.play({"/api/playlists/3": songs}, {"act": "playlist", "playlist_id": 3, "shuffle": True}), casting.PLAYED)
        self.assertEqual(played(), [(xbmc.PLAYLIST_MUSIC, [route(route="play", id=41), route(route="play", id=42)], 0)])
        self.assertTrue(xbmc.PlayList.shuffled[xbmc.PLAYLIST_MUSIC])
        # Titles as Kodi's video queue, each asked of the plugin as Kodi comes to it.
        films = {"id": 4, "kind": "video", "entries": [{"entry_id": 1, "item": {"id": 5, "kind": "movie", "title": "A film"}}, {"entry_id": 2, "item": {"id": 8, "kind": "episode", "title": "An episode"}}]}
        del xbmc.played[:]
        self.play({"/api/playlists/4": films}, {"act": "playlist", "playlist_id": 4, "shuffle": False})
        self.assertEqual(played(), [(xbmc.PLAYLIST_VIDEO, [route(route="play", id=5), route(route="play", id=8)], 0)])
        self.assertFalse(xbmc.PlayList.shuffled[xbmc.PLAYLIST_VIDEO])
        empty = {"id": 6, "kind": "audio", "entries": []}
        self.assertEqual(self.play({"/api/playlists/6": empty}, {"act": "playlist", "playlist_id": 6, "shuffle": False}), casting.NOTHING)

    def test_a_channel_plays_in_kodis_queue_of_channels(self):
        lineup = {"total": 3, "offset": 0, "channels": [{"id": 3, "name": "Three"}, {"id": 5, "name": "Five"}, {"id": 8, "name": "Eight"}]}
        self.assertEqual(self.play({"/api/live/browse": lineup}, {"act": "channel", "channel_id": 5}), casting.PLAYED)
        channels = [route(route="live_play", id=n) for n in (3, 5, 8)]
        self.assertEqual(played(), [(xbmc.PLAYLIST_VIDEO, channels, 1)], "channel up and down go round the lineup")
        # One the lineup doesn't list any more: on its own.
        del xbmc.played[:]
        self.play({"/api/live/browse": lineup, "/api/live/channels/9": {"id": 9, "name": "Nine"}}, {"act": "channel", "channel_id": 9})
        self.assertEqual(played(), [(xbmc.PLAYLIST_VIDEO, [route(route="live_play", id=9)], 0)])

    def test_a_later_cast_drops_this_one(self):
        answers = {"/api/items/40": {"id": 40, "kind": "album"}, "/api/items/40/children": {"items": [track(41)]}}
        self.assertEqual(self.play(answers, self.item(40), current=lambda: False), casting.DROPPED)
        self.assertEqual((xbmc.played, self.server.asked), ([], ["/api/items/40"]))

    def test_a_kind_it_doesnt_know_plays_nothing(self):
        self.assertEqual(self.play({"/api/items/9": {"id": 9, "kind": "photo"}}, self.item(9)), casting.NOTHING)
        self.assertEqual(xbmc.played, [])


class Tokens:
    def __init__(self):
        self.signed_in = "http://pley.example:7539"
        # The profile switched to (None: the one picked as it signed in).
        self.profile = None
        self.switches = []
        # What a switch raises, when it fails.
        self.refusal = None
        self.doubts = 0

    def server(self):
        return self.signed_in

    def identity(self):
        return (self.signed_in, self.profile, None) if self.signed_in else None

    def switch_profile(self, profile_id, pin=None, ticket=None):
        self.switches.append((profile_id, pin, ticket))
        if self.refusal is not None:
            raise self.refusal
        self.profile = profile_id

    def doubt(self):
        self.doubts += 1


class Now:
    def put(self, job):
        job()


class KodiPlayer(Player):
    """The service's player, with what Kodi plays stood in for, and its
    requests to the server noted with the profile they go as."""

    def __init__(self, tokens):
        super().__init__(tokens, "pleyKodi/test", Now())
        self.file = ""
        self.stopped = 0
        self.calls = []

    def isPlaying(self):
        return bool(self.file)

    def getPlayingFile(self):
        return self.file

    def stop(self):
        self.stopped += 1
        self.file = ""

    def call(self, method, path, body=None, timeout=None, fine=()):
        self.calls.append((method, path, self.tokens.profile))


class ServiceTest(Clean):
    """The service takes its own casts, through its proxy (a stand-in
    server here)."""

    def setUp(self):
        super().setUp()
        self.profile = tempfile.mkdtemp()
        xbmcaddon.settings["_profile"] = self.profile
        with open(os.path.join(self.profile, "install_id"), "w", encoding="utf-8") as f:
            f.write(MINE)
        self.answers = {
            "/api/items/5": {"id": 5, "kind": "movie", "duration_ms": 7_200_000},
        }
        self.server = support.FakeServer(self.answer)
        kodi.set_property(kodi.PROXY, self.server.url + "/s3cret")
        kodi.clear_property(profiles.PICKING)
        del xbmcgui.notices[:]
        self.player = KodiPlayer(Tokens())
        del xbmc.builtins[:]
        for name in (profiles.GATE, kodi.WINDOW_OPEN, requests.SUMMARY):
            kodi.clear_property(name)

    def tearDown(self):
        self.server.close()
        kodi.clear_property(kodi.PROXY)
        kodi.clear_property(profiles.PICKING)
        for name in (profiles.GATE, kodi.WINDOW_OPEN, requests.SUMMARY):
            kodi.clear_property(name)
        xbmc.info_labels.pop("Container.FolderPath", None)
        del xbmc.builtins[:]
        xbmcaddon.settings.pop("_profile", None)
        shutil.rmtree(self.profile, ignore_errors=True)
        super().tearDown()

    def answer(self, request):
        path = urllib.parse.urlsplit(request.path).path[len("/s3cret"):]
        if path == "/api/live":
            return support.answer(200, self.live)
        if path in self.answers:
            return support.answer(200, self.answers[path])
        return support.answer(404, {"code": "not_found", "message": "Not found"})

    def test_its_own_cast_plays_and_says_where_it_came_from(self):
        self.player.heard(cast(item_id=5, start_ms=60000))
        self.assertEqual(played(), [route(route="play", id=5, kind="movie", start=60000)])
        self.assertEqual(xbmcgui.notices, [("Pley", "Playing from Living room phone", "./resources/icon.png", 4000)])
        self.player.heard(cast(item_id=5, device=None))
        self.assertEqual(xbmcgui.notices[-1][1], "Playing from another device")

    def test_another_devices_or_while_it_cant_take_them(self):
        self.player.heard(cast("kodi-2", item_id=5))
        # "Who's watching?" on screen as Kodi starts.
        kodi.set_property(profiles.PICKING, "1")
        self.player.heard(cast(item_id=5))
        kodi.clear_property(profiles.PICKING)
        self.player.tokens.signed_in = None
        self.player.heard(cast(item_id=5))
        self.assertEqual((xbmc.played, xbmcgui.notices, self.server.requests), ([], [], []))

    def test_only_the_last_cast_heard_plays(self):
        self.player.casts = 2
        self.player.play_cast({"act": "item", "item_id": 5, "start_ms": None, "shuffle": False}, "Den", 1)
        self.assertEqual((xbmc.played, xbmcgui.notices), ([], []))

    def test_what_the_server_refuses_is_said(self):
        self.player.heard(cast(item_id=6))
        self.assertEqual(xbmc.played, [])
        self.assertEqual(xbmcgui.notices, [("Pley", kodi.text("server_error") % "Not found", xbmcgui.NOTIFICATION_ERROR, 5000)])

    def test_nothing_to_play_is_said(self):
        self.answers["/api/items/3"] = {"id": 3, "kind": "season"}
        self.answers["/api/items/3/on-deck"] = {"item": None}
        self.player.heard(cast(item_id=3))
        self.assertEqual(xbmcgui.notices, [("Pley", "Nothing to play", "./resources/icon.png", 4000)])

    def test_another_profiles_cast_switches_to_it_then_plays(self):
        kodi.set_property(kodi.WINDOW_OPEN, "1")
        kodi.set_property(requests.SUMMARY, "{}")
        xbmc.info_labels["Container.FolderPath"] = "plugin://plugin.video.pley/?route=library&id=1"
        self.player.heard(from_profile(item_id=5, start_ms=60000))
        self.assertEqual(self.player.tokens.switches, [(4, None, "t-1")])
        self.assertEqual(played(), [route(route="play", id=5, kind="movie", start=60000)])
        self.assertEqual(xbmcgui.notices, [("Pley", "Playing from Living room phone", "./resources/icon.png", 4000)])
        # Pley's window, the listing on screen and requests are the profile's;
        # "Who's watching?" was answered by its remote's PIN.
        self.assertIn("NotifyAll(plugin.video.pley,pley.switched)", xbmc.builtins)
        self.assertIn("Container.Refresh", xbmc.builtins)
        self.assertEqual((kodi.get_property(profiles.GATE), kodi.get_property(requests.SUMMARY)), ("1", ""))
        # On it now: its next cast plays as it is.
        self.player.heard(from_profile(ticket="t-2", item_id=5))
        self.assertEqual(self.player.tokens.switches, [(4, None, "t-1")])
        self.assertEqual(len(xbmc.played), 2)

    def test_a_switch_without_the_window_or_a_listing_of_pleys(self):
        xbmc.info_labels["Container.FolderPath"] = "plugin://plugin.video.other/"
        self.player.heard(from_profile(item_id=5))
        self.assertEqual(len(xbmc.played), 1)
        self.assertEqual(xbmc.builtins, [])

    def test_another_profiles_cast_without_a_ticket_does_nothing(self):
        self.player.heard(from_profile(ticket=None, item_id=5))
        self.player.heard(from_profile("4", item_id=5))
        self.assertEqual((self.player.tokens.switches, xbmc.played, xbmcgui.notices, self.server.requests), ([], [], [], []))
        # Unless it is on that profile.
        self.player.tokens.profile = 4
        self.player.heard(from_profile(ticket=None, item_id=5))
        self.assertEqual((self.player.tokens.switches, len(xbmc.played)), ([], 1))

    def test_a_switch_that_fails_says_so_and_plays_nothing(self):
        self.player.tokens.refusal = api.ApiError(400, "ticket_spent", "this cast's switch has expired or was used: cast again")
        self.player.heard(from_profile(item_id=5))
        self.assertEqual((xbmc.played, self.server.requests), ([], []))
        self.assertEqual(xbmcgui.notices, [("Pley", "Couldn't switch to the remote's profile", xbmcgui.NOTIFICATION_ERROR, 4000)])
        self.assertEqual((self.player.tokens.profile, kodi.get_property(profiles.GATE), xbmc.builtins), (None, "", []))
        self.assertEqual(self.player.tokens.doubts, 0)
        # Nothing answered: the server proves itself again first.
        self.player.tokens.refusal = api.ApiError(0, "unreachable", "timed out")
        self.player.heard(from_profile(item_id=5))
        self.assertEqual((self.player.tokens.doubts, len(xbmcgui.notices), xbmc.played), (1, 2, []))

    def test_what_plays_ends_as_its_own_profile_before_the_switch(self):
        sent = []
        playing = {"path": "plugin://plugin.video.pley/?route=play&id=9", "item_id": 9, "session_id": "s1", "duration_ms": 600_000}
        self.player.reports = Reports(
            playing,
            lambda path, body: sent.append((body["state"], self.player.tokens.profile)),
            lambda path: sent.append((path, self.player.tokens.profile)),
        )
        self.player.live = {"session_id": "l1", "channel_id": 3}
        self.player.file = playing["path"]
        self.player.heard(from_profile(item_id=5))
        self.assertEqual(sent, [("stopped", None), ("/api/playback/s1", None)])
        self.assertEqual(self.player.calls, [("DELETE", "/api/live/sessions/l1", None)])
        self.assertEqual((self.player.reports, self.player.live, self.player.tokens.profile), (None, None, 4))
        self.assertEqual((len(xbmc.played), self.player.stopped), (1, 0), "the cast takes its place")

    def test_what_played_stops_when_the_switch_fails(self):
        playing = {"path": "plugin://plugin.video.pley/?route=play&id=9", "item_id": 9, "session_id": "s1", "duration_ms": 600_000}
        self.player.reports = Reports(playing, lambda path, body: None, lambda path: None)
        self.player.file = playing["path"]
        self.player.tokens.refusal = api.ApiError(400, "ticket_spent", "spent")
        self.player.heard(from_profile(item_id=5))
        self.assertEqual((self.player.stopped, xbmc.played), (1, []), "its session ended for the cast")

    def test_a_later_cast_drops_one_that_waits_for_its_switch(self):
        self.player.casts = 2
        self.player.play_cast({"act": "item", "item_id": 5, "start_ms": None, "shuffle": False}, "Den", 1, {"profile_id": 4, "ticket": "t-1"})
        self.assertEqual((self.player.tokens.switches, xbmc.played), ([], []))

    def test_the_event_stream_names_it_a_receiver(self):
        query = receiver_query("pleyKodi/test")
        server = self.server.url
        self.live = {"available": True, "allowed": True, "channels": 12}
        self.assertEqual(query(server + "/s3cret", "at-1"), {"receiver": MINE, "device_kind": "tv", "casts": "item,live"})
        self.assertEqual(self.server.requests[-1].headers["authorization"], "Bearer at-1")
        # Live TV not open to this account, or without channels: titles only.
        self.live = {"available": False, "allowed": True, "channels": 0}
        self.assertEqual(query(server + "/s3cret", "at-1")["casts"], "item")


if __name__ == "__main__":
    unittest.main()
