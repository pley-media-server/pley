import json
import threading
import time
import unittest
import urllib.parse
from unittest import mock

import support
import test_window
import xbmc
import xbmcgui
import xbmcplugin
from pley import kodi, livetv, plugin, window
from pley import layout as L

BASE = "http://127.0.0.1:5000/s"
# A quarter past an hour, the programmes on from the hour to the next.
NOW = (int(time.time()) // 3600) * 3600 + 900
HOUR = 3600


def program(channel_id, start, stop, title, **more):
    found = {
        "channel_id": channel_id, "start": start, "stop": stop, "title": title, "subtitle": None, "description": None,
        "categories": [], "image": None, "season": None, "episode": None, "is_new": False, "is_live": False, "rating": None,
    }
    found.update(more)
    return found


NEWS = {
    "id": 1, "source_id": 1, "name": "Pley News HD", "number": "101", "group": "News", "logo": "/api/live/channels/1/logo",
    "favorite": False, "has_guide": True,
    "now": program(1, NOW - 900, NOW + 2700, "Morning Briefing", subtitle="Part 7", description="Everything you need to know.",
                   categories=["News"], season=1, episode=7, is_live=True, image="/api/live/channels/1/programs/1/image"),
    "next": program(1, NOW + 2700, NOW + 5400, "Midday Bulletin"),
}
SPORT = {
    "id": 2, "source_id": 1, "name": "Sport 1080i", "number": "102", "group": "Sport", "logo": None, "favorite": True,
    "has_guide": True, "now": program(2, NOW - 900, NOW + 2700, "Weekend Highlights"), "next": None,
}
RADIO = {"id": 4, "source_id": 1, "name": "Radio Jazz", "number": "104", "group": "Radio", "logo": None, "favorite": False,
         "has_guide": False, "now": None, "next": None}
WEATHER = dict(RADIO, id=5, name="Weather Now", number="105", group="News")
SKY = dict(RADIO, id=7, name="UK| SKY SPORTS 1", number="201", group="UK| Sport")
SKY2 = dict(RADIO, id=8, name="UK| SKY SPORTS 2", number="202", group="UK| Sport")
BBC = dict(RADIO, id=9, name="UK| BBC NEWS", number="301", group="UK| News")
CHANNELS = [NEWS, SPORT, RADIO, WEATHER, SKY, SKY2, BBC]
RECENT = [4, 1]
GROUPS = [
    {"name": "News", "region": None, "label": "News", "count": 2},
    {"name": "UK| Sport", "region": "UK", "label": "Sport", "count": 2},
    {"name": "Sport", "region": None, "label": "Sport", "count": 1},
    {"name": "UK| News", "region": "UK", "label": "News", "count": 1},
    {"name": "Radio", "region": None, "label": "Radio", "count": 1},
    {"name": "US| Films", "region": "US", "label": "Films", "count": 0},
    {"name": "DE| Sport", "region": "DE", "label": "Sport", "count": 0},
    {"name": "DE| Kino", "region": "DE", "label": "Kino", "count": 0},
]


def lineup(path, query, channels):
    """The Live TV API of a server with `channels`, as far as the add-on asks it."""
    if path == "/api/live":
        return {"available": True, "allowed": True, "channels": len(channels), "sources": 0, "recent": RECENT}
    if path == "/api/live/groups":
        return {"total": len(channels), "favorites": sum(1 for c in channels if c["favorite"]), "recent": len(RECENT), "groups": GROUPS}
    if path == "/api/live/browse":
        category = query.get("category", "all")
        found = list(channels)
        if category == "favorites":
            found = [c for c in found if c["favorite"]]
        elif category == "recent":
            found = [c for c in channels if c["id"] in RECENT]
            found.sort(key=lambda c: RECENT.index(c["id"]))
        elif category == "group":
            found = [c for c in found if c["group"] == query.get("group")]
        q = (query.get("q") or "").lower()
        if q:
            found = [c for c in found if q in c["name"].lower() or (c["number"] or "").startswith(q)]
        limit = int(query.get("limit", 100))
        offset = int(query.get("offset", 0))
        if "around" in query:
            at = [c["id"] for c in found].index(int(query["around"]))
            offset = at // limit * limit
        return {"total": len(found), "offset": offset, "channels": found[offset : offset + limit]}
    if path.startswith("/api/live/channels/") and path.count("/") == 4:
        return next(c for c in channels if c["id"] == int(path.rsplit("/", 1)[1]))
    if path == "/api/live/guide":
        ids = query.get("ids")
        assert ids, "the guide is asked for one channel, never every one"
        return {"from": 0, "to": 0, "programs": {"1": [NEWS["now"], NEWS["next"]]} if ids == "1" else {}}
    raise AssertionError(path)


def clock(ts):
    return time.strftime("%H:%M", time.localtime(ts))


class WhatsOnTest(unittest.TestCase):
    def test_now_and_next(self):
        self.assertIs(livetv.on_now(NEWS, NOW), NEWS["now"])
        self.assertIs(livetv.up_next(NEWS, NOW), NEWS["next"])
        # Listed a while ago: once the next has begun, it is what's on.
        later = NOW + 3000
        self.assertIs(livetv.on_now(NEWS, later), NEWS["next"])
        self.assertIsNone(livetv.up_next(NEWS, later))
        self.assertIsNone(livetv.on_now(RADIO, NOW))
        self.assertAlmostEqual(livetv.progress(NEWS["now"], NOW), 0.25)

    def test_times_as_kodi_writes_them(self):
        self.assertEqual(livetv.time_format(), "%H:%M", "no seconds")
        self.assertEqual(livetv.span(NEWS["now"]), "%s – %s" % (clock(NOW - 900), clock(NOW + 2700)))
        xbmc.region["time"] = "%I:%M:%S %p"
        self.addCleanup(xbmc.region.update, {"time": "%H:%M:%S"})
        self.assertEqual(livetv.time_format(), "%I:%M %p")
        self.assertFalse(livetv.clock(NOW).startswith("0"), "8:15 PM, as the web writes it")
        self.assertEqual(livetv.left(NEWS["now"], NOW), "45 min left")
        self.assertEqual(livetv.episode(NEWS["now"]), "S1 · E7")
        self.assertEqual(livetv.badges(NEWS["now"]), ["Live"])
        self.assertEqual(livetv.headline(NEWS["now"]), "Morning Briefing: Part 7")
        self.assertEqual(livetv.name(NEWS), "101 Pley News HD")
        self.assertEqual(livetv.name(dict(NEWS, number=None)), "Pley News HD")

    def test_days(self):
        # Times on local dates, not NOW ± 24 hours, so the test holds in any
        # zone on any day: a day is 23 or 25 hours when daylight saving
        # starts or ends. Every day of a year covers the zone's changes.
        today = time.localtime(NOW)

        def at(days, hour, minute):
            return time.mktime((today.tm_year, today.tm_mon, today.tm_mday + days, hour, minute, 0, 0, 0, -1))

        self.assertEqual(livetv.day(NOW, NOW), "Today")
        for days in range(366):
            for now in (at(days, 0, 30), at(days, 23, 30)):
                with self.subTest(now=time.ctime(now)):
                    self.assertEqual(livetv.day(at(days, 0, 1), now), "Today")
                    self.assertEqual(livetv.day(at(days, 23, 59), now), "Today")
                    self.assertEqual(livetv.day(at(days + 1, 0, 1), now), "Tomorrow")
                    self.assertEqual(livetv.day(at(days + 1, 23, 59), now), "Tomorrow")
                    self.assertEqual(livetv.day(at(days - 1, 0, 1), now), "Yesterday")
                    self.assertEqual(livetv.day(at(days - 1, 23, 59), now), "Yesterday")
                    self.assertNotIn(livetv.day(at(days + 2, 0, 1), now), ("Today", "Tomorrow"))
                    self.assertNotIn(livetv.day(at(days - 2, 23, 59), now), ("Today", "Yesterday"))
        later = time.localtime(at(3, 0, 1))
        self.assertEqual(livetv.day(at(3, 0, 1), NOW), "%s %d %s" % (livetv.ENGLISH_DAYS[later.tm_wday], later.tm_mday, livetv.ENGLISH_MONTHS[later.tm_mon - 1]))

    def test_what_kodis_info_says(self):
        text = livetv.plot(NEWS, NOW)
        self.assertTrue(text.startswith("[B]%s – %s[/B]  45 min left · S1 · E7 · Live" % (clock(NOW - 900), clock(NOW + 2700))), text)
        self.assertIn("Everything you need to know.", text)
        self.assertIn("[B]Next[/B]  %s  Midday Bulletin" % clock(NOW + 2700), text)
        self.assertEqual(livetv.plot(RADIO, NOW), "No guide")
        self.assertEqual(livetv.plot(dict(RADIO, has_guide=True), NOW), "Nothing in the guide now")

    def test_regions(self):
        # As the server splits group names (and the web channel names).
        self.assertEqual(livetv.split_region("UK| Sport"), ("UK", "Sport"))
        self.assertEqual(livetv.split_region("[DE] Kino"), ("DE", "Kino"))
        self.assertEqual(livetv.split_region("FR: Sport"), ("FR", "Sport"))
        self.assertEqual(livetv.split_region("|UK| SKY SPORTS 1"), ("UK", "SKY SPORTS 1"))
        self.assertIsNone(livetv.split_region("Pley News HD"))
        self.assertIsNone(livetv.split_region("Movies: Action"), "a word, not a region")
        # In a group, a name leaves out the region the group says; elsewhere it's whole.
        self.assertEqual(livetv.name(SKY, "UK| Sport"), "201 SKY SPORTS 1")
        self.assertEqual(livetv.name(SKY), "201 UK| SKY SPORTS 1")
        self.assertEqual(livetv.short_name(SKY, "News"), "UK| SKY SPORTS 1")
        self.assertEqual(livetv.short_name(dict(SKY, name="US| CNN"), "UK| Sport"), "US| CNN", "another region's stays")

    def test_regions_first_then_the_other_groups(self):
        tree = livetv.group_tree(GROUPS)
        self.assertEqual(
            [(e["kind"], e.get("region") or e["group"]["name"]) for e in tree],
            [("region", "DE"), ("region", "UK"), ("group", "News"), ("group", "Sport"), ("group", "Radio"), ("group", "US| Films")],
            "regions in alphabetical order, then the rest in the lineup's; a region with one group is among the rest",
        )
        self.assertEqual(tree[1]["count"], 3)
        self.assertEqual([g["label"] for g in tree[1]["groups"]], ["Sport", "News"])
        # Fewer than two regions: one list, in the lineup's order.
        one = [g for g in GROUPS if g.get("region") != "DE"]
        self.assertEqual([e["kind"] for e in livetv.group_tree(one)], ["group"] * len(one))
        self.assertEqual([g["name"] for g in livetv.matching_groups(GROUPS, "SPORT")], ["UK| Sport", "Sport", "DE| Sport"])
        self.assertEqual(livetv.matching_groups(GROUPS, " "), [])
        self.assertEqual(livetv.group_name(GROUPS[1]), "UK · Sport")
        self.assertEqual(livetv.group_name(GROUPS[0]), "News")

    def test_a_number(self):
        self.assertIs(livetv.by_number(CHANNELS, "104"), RADIO)
        self.assertIs(livetv.by_number(CHANNELS, "10"), NEWS, "the first whose number starts so")
        self.assertIsNone(livetv.by_number(CHANNELS, "7"))


class ItemsTest(unittest.TestCase):
    def setUp(self):
        xbmc.PlayList.entries.clear()

    def test_a_channel_in_kodis_lists(self):
        li = livetv.list_item(BASE, NEWS, NOW)
        self.assertEqual((li.label, li.label2), ("101 Pley News HD", "Morning Briefing: Part 7"))
        self.assertEqual(li.art["thumb"], BASE + "/api/live/channels/1/logo?w=320")
        self.assertEqual(li.art["fanart"], BASE + "/api/live/channels/1/programs/1/image?w=640")
        tag = li.video.values
        self.assertEqual((tag["setMediaType"], tag["setTitle"], tag["setTagLine"], tag["setGenres"]), ("video", "101 Pley News HD", "Morning Briefing: Part 7", ["News"]))
        # A stream without an end: Kodi counts one it stopped as played, and
        # keeps that by its address; a channel is never watched, nor resumed.
        self.assertEqual((tag["setPlaycount"], tag["setResumePoint"]), (0, (0.0, 0.0)))
        self.assertNotIn("setDuration", tag)
        self.assertEqual(li.properties["IsPlayable"], "true")
        self.assertEqual(livetv.list_item(BASE, SKY, NOW, "UK| Sport").label, "201 SKY SPORTS 1")

    def test_a_channel_plays_as_the_provider_sends_it(self):
        li = livetv.play_item(BASE, NEWS, "kodi-1", NOW)
        # Read for the device's watch session, which the remote reaches.
        self.assertEqual(li.path, BASE + "/api/live/channels/1/stream.ts?client_id=kodi-1-stream&device_kind=tv")
        self.assertEqual((li.mime, li.lookup), ("video/mp2t", False))

    def test_the_queue_holds_the_channels_around_the_one_played(self):
        many = [dict(RADIO, id=i, number=str(i)) for i in range(1, 501)]
        playlist, at = livetv.queue(BASE, many, 300, NOW)
        entries = xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO]
        self.assertEqual(len(entries), 2 * livetv.QUEUED + 1)
        self.assertEqual(entries[at][0], "plugin://plugin.video.pley/?route=live_play&id=301")
        playlist, at = livetv.queue(BASE, many, 3, NOW)
        self.assertEqual((at, len(xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO])), (3, livetv.QUEUED + 4))
        # Opened from a group: its names, and the player's, are the group's.
        livetv.queue(BASE, [SKY, SKY2], 0, NOW, "UK| Sport")
        url, li = xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO][1]
        self.assertEqual((url, li.label), ("plugin://plugin.video.pley/?route=live_play&id=8&group=UK%7C+Sport", "202 SKY SPORTS 2"))

    def test_only_a_queue_of_channels_is_let_go(self):
        mine = xbmc.PlayList(xbmc.PLAYLIST_VIDEO)
        mine.add("plugin://plugin.video.other/?play=1")
        livetv.forget_queue()
        self.assertEqual(mine.size(), 1, "someone else's queue stays")
        livetv.queue(BASE, CHANNELS, 0, NOW)
        livetv.forget_queue()
        self.assertEqual(mine.size(), 0)


class ServiceTest(unittest.TestCase):
    """What the service's proxy hears of a channel Kodi plays."""

    def setUp(self):
        from pley import service

        self.service = service
        xbmc.PlayList.entries.clear()
        del xbmcgui.notified[:]
        del xbmcgui.boxes[:]
        del xbmcgui.picks[:]
        del xbmcgui.questions[:]
        del xbmcgui.answers[:]
        self.player = service.Player(None, "pleyKodi/test", None)
        self.stopped, self.played = [], []
        self.player.stop = lambda: self.stopped.append(1)
        self.player.play = self.played.append
        self.playing = ""
        self.player.playing_file = lambda: self.playing

    def wait(self, check, seconds=1.0):
        for _ in range(int(seconds / 0.02)):
            if check():
                return
            time.sleep(0.02)

    def taken(self, channel, message="Alex is watching Two now, on the connection this channel used."):
        self.player.heard({"type": "live_taken_over", "session_id": "s1", "channel_id": channel, "message": message})

    def test_a_channel_the_server_ended_doesnt_tune_the_next(self):
        livetv.queue(BASE, CHANNELS, 0, NOW)
        with mock.patch.object(self.service, "TAKEN_GRACE", 0.05):
            self.player.live_ended("/api/live/channels/1/stream.ts?client_id=kodi-1")
            self.assertEqual(xbmc.PlayList(xbmc.PLAYLIST_VIDEO).size(), 0)
            # Said once no take-over's event came for it.
            self.wait(lambda: xbmcgui.notified)
        self.assertEqual(xbmcgui.notified, [("Pley", "The server stopped this stream.")])

    def test_a_channel_taken_over_stops_at_once_and_says_why_once(self):
        livetv.queue(BASE, CHANNELS, 0, NOW)
        self.playing = "http://127.0.0.1:5000/s3cret/api/live/channels/1/stream.ts?client_id=kodi-1"
        self.taken(1)
        # Its HLS session and its stream's reader each have an event, as other devices' do.
        self.taken(1)
        self.wait(lambda: xbmcgui.questions)
        time.sleep(0.1)
        self.assertEqual(self.stopped, [1])
        self.assertEqual(xbmc.PlayList(xbmc.PLAYLIST_VIDEO).size(), 0, "Kodi tunes nothing after it")
        self.assertEqual(xbmcgui.questions, [("Live TV stopped", "Alex is watching Two now, on the connection this channel used.", "OK", "Watch again")])
        self.assertEqual(self.played, [], "OK leaves it stopped")

    def test_watch_again_plays_the_channel_as_any_is_played(self):
        self.playing = "http://127.0.0.1:5000/s3cret/api/live/channels/1/stream.ts?client_id=kodi-1"
        xbmcgui.answers.append(True)
        self.taken(1, None)
        self.wait(lambda: self.played)
        self.assertEqual(xbmcgui.questions[0][1], "This channel isn't being watched any more.")
        # Never taking a connection over by itself: every connection in use says so.
        self.assertEqual(self.played, ["plugin://plugin.video.pley/?route=live_play&id=1"])

    def test_a_channel_watched_again_and_taken_over_again_says_so_again(self):
        self.playing = "http://127.0.0.1:5000/s3cret/api/live/channels/1/stream.ts?client_id=kodi-1"
        asked = livetv.taken_over
        picked = threading.Event()

        def answered(message):
            picked.wait(1)
            return asked(message)

        xbmcgui.answers.extend([True, False])
        with mock.patch.object(self.service, "TAKEN_GRACE", 0.1), mock.patch.object(livetv, "taken_over", answered):
            self.taken(1, "Alex is watching Two now, on the connection this channel used.")
            # Its stream ends, then Watch again is picked within the end's wait: the end says nothing.
            self.player.live_ended("/api/live/channels/1/stream.ts?client_id=kodi-1")
            picked.set()
            self.wait(lambda: self.played)
            time.sleep(0.2)
            self.assertEqual(xbmcgui.notified, [])
            # Playing again, the channel is taken over again within seconds.
            self.taken(1, "Sam is watching Three now, on the connection this channel used.")
            self.wait(lambda: len(xbmcgui.questions) == 2)
            self.player.live_ended("/api/live/channels/1/stream.ts?client_id=kodi-1")
            time.sleep(0.2)
        self.assertEqual(self.stopped, [1, 1])
        self.assertEqual(
            [q[1] for q in xbmcgui.questions],
            ["Alex is watching Two now, on the connection this channel used.", "Sam is watching Three now, on the connection this channel used."],
        )
        self.assertEqual(self.played, ["plugin://plugin.video.pley/?route=live_play&id=1"], "OK leaves it stopped")
        self.assertEqual(xbmcgui.notified, [], "the take-overs say why, not the ends")

    def test_another_channels_take_over_changes_nothing(self):
        self.playing = "http://127.0.0.1:5000/s3cret/api/live/channels/1/stream.ts?client_id=kodi-1"
        self.taken(2)
        time.sleep(0.1)
        self.assertEqual((self.stopped, xbmcgui.questions), ([], []))

    def test_a_take_over_heard_after_its_stream_ended_is_said_and_the_end_says_nothing(self):
        with mock.patch.object(self.service, "TAKEN_GRACE", 0.2):
            self.player.live_ended("/api/live/channels/1/stream.ts?client_id=kodi-1")
            # Kodi read the end already: nothing plays.
            self.taken(1)
            self.wait(lambda: xbmcgui.questions)
            time.sleep(0.4)
        self.assertEqual(len(xbmcgui.questions), 1)
        self.assertEqual(self.stopped, [], "nothing plays to stop")
        self.assertEqual(xbmcgui.notified, [], "the take-over says why, not the end")

    def test_a_channel_refused_on_its_way_to_kodi_can_be_watched_here_instead(self):
        played = []
        self.player.play = played.append
        xbmcgui.picks.append(0)
        held = [{"channel_id": 2, "channel_name": "Sport 1080i", "watchers": [], "people": 1, "yours": True, "can_take_over": True}]
        self.player.busy("The one connection is in use.", held, "/api/live/channels/1/stream.ts?client_id=kodi-1")
        for _ in range(50):
            if played:
                break
            time.sleep(0.02)
        self.assertEqual(played, ["plugin://plugin.video.pley/?route=live_play&id=1&take_over=2"])

    def test_a_channel_refused_on_its_way_to_kodi_is_said_once(self):
        livetv.queue(BASE, CHANNELS, 0, NOW)
        self.player.busy("The one connection is in use.")
        self.player.busy("The one connection is in use.")
        for _ in range(50):
            if xbmcgui.boxes:
                break
            time.sleep(0.02)
        time.sleep(0.1)
        self.assertEqual(xbmcgui.boxes, [("All connections are in use", "The one connection is in use.")])
        self.assertEqual(xbmc.PlayList(xbmc.PLAYLIST_VIDEO).size(), 0)


class LeavingTest(unittest.TestCase):
    """The service leaves the watch session of a channel Kodi stops reading,
    rather than the server keeping it 45 s; not one the server ended."""

    def setUp(self):
        from pley import service

        class Now:
            def put(self, job):
                job()

        xbmc.PlayList.entries.clear()
        del xbmcgui.questions[:]
        self.player = service.Player(None, "pleyKodi/test", Now())
        self.deleted = []
        self.player.call = lambda method, path, body=None, timeout=0, fine=(): self.deleted.append((method, path, fine))
        self.player.stop = lambda: None
        self.playing = ""
        self.player.playing_file = lambda: self.playing
        self.addCleanup(kodi.clear_property, kodi.LIVE)
        # The TAKEN_ONCE window would keep a take-over of one test from the next.
        self.player.taken = (None, float("-inf"))

    def plays(self, channel, session):
        """The plugin watched `channel` (kodi.LIVE), then Kodi plays its stream."""
        kodi.set_property(kodi.LIVE, json.dumps({"session_id": session, "channel_id": channel}))
        self.playing = "http://127.0.0.1:5000/s3cret/api/live/channels/%d/stream.ts?client_id=kodi-1-stream" % channel
        self.player.onAVStarted()

    def test_stopping_leaves_the_session_at_once(self):
        self.plays(1, "live-1")
        self.assertEqual(self.deleted, [])
        self.playing = ""
        self.player.onPlayBackStopped()
        self.player.onPlayBackStopped()
        self.assertEqual(self.deleted, [("DELETE", "/api/live/sessions/live-1", (404, 410))])

    def test_a_stop_told_late_leaves_nothing_still_playing(self):
        self.plays(1, "live-1")
        self.player.onPlayBackStopped()
        self.assertEqual(self.deleted, [])

    def test_another_channel_leaves_the_one_before(self):
        self.plays(1, "live-1")
        self.plays(2, "live-2")
        self.assertEqual([d[1] for d in self.deleted], ["/api/live/sessions/live-1"])
        self.playing = ""
        self.player.onPlayBackEnded()
        self.assertEqual([d[1] for d in self.deleted], ["/api/live/sessions/live-1", "/api/live/sessions/live-2"])

    def test_a_title_after_a_channel_leaves_it(self):
        self.plays(1, "live-1")
        self.playing = "http://127.0.0.1:5000/s3cret/api/playback/s/index.m3u8"
        self.player.onAVStarted()
        self.assertEqual([d[1] for d in self.deleted], ["/api/live/sessions/live-1"])

    def test_kodi_exiting_leaves_it(self):
        self.plays(1, "live-1")
        self.player.closing()
        self.player.closing()
        self.assertEqual([d[1] for d in self.deleted], ["/api/live/sessions/live-1"])

    def test_a_session_the_server_ended_isnt_left(self):
        self.plays(1, "live-1")
        with mock.patch("threading.Thread"):
            self.player.heard({"type": "live_taken_over", "session_id": "s9", "channel_id": 1, "message": None})
        self.playing = ""
        self.player.onPlayBackStopped()
        self.plays(1, "live-2")
        with mock.patch("threading.Thread"):
            self.player.heard({"type": "session_stopped", "session_id": "live-2", "by_admin": True, "message": None})
        self.playing = ""
        self.player.onPlayBackStopped()
        self.plays(1, "live-3")
        with mock.patch("threading.Timer"):
            self.player.live_ended("/api/live/channels/1/stream.ts?client_id=kodi-1-stream")
        self.playing = ""
        self.player.onPlayBackEnded()
        self.assertEqual(self.deleted, [])

    def test_a_channel_kodi_never_played_isnt_held(self):
        kodi.set_property(kodi.LIVE, json.dumps({"session_id": "live-1", "channel_id": 1}))
        self.player.onPlayBackError()
        self.player.closing()
        self.assertEqual(self.deleted, [])


class RoutesTest(unittest.TestCase):
    """Live TV in Kodi's lists and the play route, against a stand-in server."""

    def setUp(self):
        clock_patch = mock.patch("time.time", return_value=NOW)
        clock_patch.start()
        self.addCleanup(clock_patch.stop)
        self.busy = False
        self.held = []
        self.server = support.FakeServer(self.handle)
        self.addCleanup(self.server.close)
        kodi.set_property(kodi.PROXY, self.server.url + "/s3cret")
        self.addCleanup(kodi.clear_property, kodi.PROXY)
        self.addCleanup(kodi.xbmcaddon.settings.pop, "_profile", None)
        import tempfile

        kodi.xbmcaddon.settings["_profile"] = tempfile.mkdtemp()
        del xbmcplugin.calls[:]
        del xbmcgui.boxes[:]
        del xbmcgui.lists[:]
        del xbmcgui.picks[:]
        xbmc.PlayList.entries.clear()

    def handle(self, request):
        path, _, query = request.path[len("/s3cret") :].partition("?")
        if path == "/api/live/channels/1/watch":
            if self.busy and request.json().get("take_over") != 2:
                return support.answer(409, {"code": "conflict", "message": "The one connection of Test Provider is in use, watching Sport 1080i.", "connections": self.held})
            return support.answer(200, {"session_id": "w1", "url": "/api/live/sessions/w1/index.m3u8", "kind": "hls", "channel": NEWS})
        if path == "/api/live/channels/2/favorite":
            return 204, {}, b""
        if path == "/api/live/channels":
            raise AssertionError("every channel is never asked for")
        if path == "/api/search":
            return support.answer(200, {"items": [], "people": [], "channels": [NEWS]})
        return support.answer(200, lineup(path, dict(urllib.parse.parse_qsl(query)), CHANNELS))

    def run_route(self, query):
        plugin.Plugin(["plugin://plugin.video.pley/", "1", "?" + query]).run()
        return [c for c in xbmcplugin.calls if c[0] == "item"]

    def test_live_tv_lists_its_categories(self):
        items = self.run_route("route=live")
        labels = [c[2].label.split("  [COLOR")[0] for c in items]
        self.assertEqual(labels, ["Favourites", "Recently watched", "All channels", "Search channels", "DE", "UK", "News", "Sport", "Radio", "US · Films"])
        self.assertIn("(7)", items[2][2].label)
        self.assertIn("category=favorites", items[0][1])
        self.assertIn("route=live_region&region=UK", items[5][1])
        del xbmcplugin.calls[:]
        items = self.run_route("route=live_region&region=UK")
        self.assertEqual([c[2].label.split("  [COLOR")[0] for c in items], ["Sport", "News"])
        self.assertIn("group=UK%7C+Sport", items[0][1])

    def test_a_groups_channels_play_and_have_a_menu(self):
        items = self.run_route("route=live_channels&category=group&group=UK%7C+Sport")
        self.assertEqual([c[2].label for c in items], ["201 SKY SPORTS 1", "202 SKY SPORTS 2"])
        url, li, folder = items[0][1:]
        self.assertEqual((url, folder), ("plugin://plugin.video.pley/?route=live_play&id=7&group=UK%7C+Sport", False))
        self.assertEqual([name for name, _ in li.menu], ["Add to favourites", "Schedule"])
        self.assertIn(("content", "videos"), xbmcplugin.calls)
        asked = [r.path for r in self.server.requests]
        self.assertEqual(asked, ["/s3cret/api/live/browse?category=group&offset=0&limit=300&group=UK%7C+Sport"])
        del xbmcplugin.calls[:]
        self.assertEqual([c[2].label for c in self.run_route("route=live_channels&category=recent")], ["104 Radio Jazz", "101 Pley News HD"])

    def test_a_category_comes_a_page_at_a_time(self):
        with mock.patch.object(plugin, "PAGE", 3):
            items = self.run_route("route=live_channels&category=all")
            self.assertEqual([c[2].label for c in items], ["101 Pley News HD", "102 Sport 1080i", "104 Radio Jazz", "Next page"])
            self.assertIn("offset=3", items[-1][1])
            del xbmcplugin.calls[:]
            items = self.run_route("route=live_channels&category=all&offset=6")
        self.assertEqual([c[2].label for c in items], ["301 UK| BBC NEWS"])

    def test_searching_every_channel_and_the_groups(self):
        items = self.run_route("route=live_channels&category=all&q=sky")
        self.assertEqual([c[2].label for c in items], ["201 UK| SKY SPORTS 1", "202 UK| SKY SPORTS 2"])
        del xbmcplugin.calls[:]
        # Whatever group it was asked from, a search looks at every channel,
        # the groups whose names match first.
        items = self.run_route("route=live_channels&category=group&group=Radio&q=news")
        self.assertEqual([c[2].label.split("  [COLOR")[0] for c in items], ["News", "UK · News", "101 Pley News HD", "301 UK| BBC NEWS"])
        self.assertTrue(items[0][3], "a group opens")
        self.assertIn("group=UK%7C+News", items[1][1])

    def test_a_channels_schedule(self):
        items = self.run_route("route=live_schedule&id=1")
        on, later, more = items
        self.assertEqual(on[2].label, "On now  Morning Briefing: Part 7")
        self.assertEqual((on[1], on[3]), ("plugin://plugin.video.pley/?route=live_play&id=1", False))
        # "Today", or "Tomorrow" run in the hour before midnight.
        self.assertEqual(later[2].label, "%s %s  Midday Bulletin" % (livetv.day(NOW + 2700, NOW), clock(NOW + 2700)))
        self.assertNotIn("IsPlayable", later[2].properties)
        self.assertEqual(more[2].label, "Later")
        self.assertIn("from=%d" % (NOW + 5400), more[1])
        guide = next(r.path for r in self.server.requests if "/guide" in r.path)
        self.assertIn("ids=1", guide)

    def test_playing_a_channel(self):
        self.run_route("route=live_play&id=1")
        resolved = [c for c in xbmcplugin.calls if c[0] == "resolved"]
        self.assertEqual(len(resolved), 1)
        _, ok, li = resolved[0]
        self.assertTrue(ok)
        self.assertTrue(li.path.startswith(self.server.url + "/s3cret/api/live/channels/1/stream.ts?client_id=kodi-"))
        watch = next(r for r in self.server.requests if r.path.endswith("/watch"))
        reader = urllib.parse.parse_qs(urllib.parse.urlsplit(li.path).query)["client_id"][0]
        self.assertEqual(reader, watch.json()["client_id"] + "-stream", "the stream is read for the device's session")
        self.assertNotIn(("end", False), xbmcplugin.calls, "nothing listed: Kodi plays")

    def test_every_connection_in_use_is_said_and_the_queue_let_go(self):
        self.busy = True
        livetv.queue(BASE, CHANNELS, 0, NOW)
        self.run_route("route=live_play&id=1")
        self.assertEqual([c[:2] for c in xbmcplugin.calls if c[0] == "resolved"], [("resolved", False)])
        self.assertEqual(xbmcgui.boxes, [("All connections are in use", "The one connection of Test Provider is in use, watching Sport 1080i.")])
        self.assertEqual(xbmc.PlayList(xbmc.PLAYLIST_VIDEO).size(), 0, "Kodi would try each channel after it")
        # What holds each connection, when nothing can be taken over.
        del xbmcgui.boxes[:]
        self.held = [{"channel_id": 2, "channel_name": "Sport 1080i", "watchers": [], "people": 3, "yours": False, "can_take_over": False}]
        self.run_route("route=live_play&id=1")
        self.assertTrue(xbmcgui.boxes[0][1].endswith("\n\nSport 1080i  ·  3 people"), xbmcgui.boxes)

    def test_watching_here_instead_takes_a_connection_over(self):
        self.busy = True
        self.held = [
            {"channel_id": 3, "channel_name": "Film Four", "watchers": ["ana"], "people": 1, "yours": False, "can_take_over": False},
            {"channel_id": 2, "channel_name": "Sport 1080i", "watchers": [], "people": 1, "yours": True, "can_take_over": True},
        ]
        xbmcgui.picks.append(1)
        self.run_route("route=live_play&id=1")
        self.assertEqual(xbmcgui.lists, [("All connections are in use", [("Film Four", "ana"), ("Sport 1080i", "You, on another device  ·  Watch here instead")])])
        resolved = [c for c in xbmcplugin.calls if c[0] == "resolved"]
        self.assertEqual(len(resolved), 1)
        _, ok, li = resolved[0]
        self.assertTrue(ok)
        self.assertIn("take_over=2", li.path, "the stream takes it too")
        asked = [r.json().get("take_over") for r in self.server.requests if r.path.endswith("/watch")]
        self.assertEqual(asked, [None, 2])
        # Chosen elsewhere in the list (one that can't be taken), or let be: nothing plays.
        del xbmcplugin.calls[:]
        xbmcgui.picks.append(0)
        self.run_route("route=live_play&id=1")
        self.assertEqual([c[:2] for c in xbmcplugin.calls if c[0] == "resolved"], [("resolved", False)])

    def test_who_holds_a_connection(self):
        self.assertEqual(livetv.who({"yours": True, "people": 1}), "You, on another device")
        self.assertEqual(livetv.who({"watchers": ["ana", "ben"], "people": 2}), "ana, ben")
        self.assertEqual(livetv.who({"watchers": [], "people": 1}), "Someone else")
        self.assertEqual(livetv.who({"watchers": [], "people": 4}), "4 people")

    def test_search_finds_channels_too(self):
        items = self.run_route("route=search&q=news")
        labels = [c[2].label for c in items]
        self.assertEqual(labels, ["101 Pley News HD"])
        self.assertEqual(items[0][1], "plugin://plugin.video.pley/?route=live_play&id=1")
        self.assertFalse(items[0][3])

    def test_a_favourite_from_the_menu(self):
        del xbmc.builtins[:]
        self.run_route("route=live_favourite&id=2&value=0")
        self.assertEqual([(r.method, r.path) for r in self.server.requests], [("DELETE", "/s3cret/api/live/channels/2/favorite")])
        self.assertIn("Container.Refresh", xbmc.builtins)

    def test_a_server_without_live_tv_has_none(self):
        class Refusing:
            def get(self, path):
                raise livetv.api.ApiError(404, "not_found", "no route")

        self.assertEqual(livetv.summary(Refusing()), {})
        self.assertFalse(livetv.available({}))


class LiveScreenTest(unittest.TestCase):
    """Live TV in pley's window."""

    def setUp(self):
        clock_patch = mock.patch("time.time", return_value=NOW)
        clock_patch.start()
        self.addCleanup(clock_patch.stop)
        xbmc.PlayList.entries.clear()
        del xbmc.played[:]
        self.win = test_window.FakeWindow()
        self.win.me = {"id": 3, "is_admin": False}
        self.win.live = {"available": True}
        # Work handed to other threads runs at once.
        self.win.later = lambda work, then, failed=None: then(work())
        self.win.post = lambda work: work()
        self.win.rest = lambda then: then()
        self.channels = [dict(c) for c in CHANNELS]
        self.favourites = []
        self.asked = []

        class Server:
            def get(inner, path, **query):
                self.asked.append((path, query))
                return lineup(path, {k: str(v) for k, v in query.items()}, self.channels)

            def put(inner, path, body=None):
                self.favourites.append(("PUT", path))
                next(c for c in self.channels if path.endswith("/%d/favorite" % c["id"]))["favorite"] = True

            def delete(inner, path):
                self.favourites.append(("DELETE", path))
                next(c for c in self.channels if path.endswith("/%d/favorite" % c["id"]))["favorite"] = False

        self.win.session.client = Server()
        self.screen = window.Live(self.win)
        self.win.screen = self.screen
        self.addCleanup(lambda: self.screen.timer and self.screen.timer.cancel())
        self.screen.load()

    def rows(self):
        return self.win.made[L.LIVE_LIST].items

    def categories(self):
        return [(li.label, li.label2, li.properties["chosen"]) for li in self.win.made[L.LIVE_CATEGORIES].items]

    def test_categories_and_the_channels_with_whats_on(self):
        self.screen.show(False)
        self.assertEqual(self.categories(), [
            ("Search channels", "", ""), ("Favourites", "1", ""), ("Recently watched", "2", ""), ("All channels", "7", "1"),
            ("DE", "0", ""), ("UK", "3", ""), ("News", "2", ""), ("Sport", "1", ""), ("Radio", "1", ""), ("US · Films", "0", ""),
        ])
        rows = self.rows()
        self.assertEqual([r.label for r in rows][:3], ["101 Pley News HD", "102 Sport 1080i", "104 Radio Jazz"])
        news = rows[0]
        self.assertEqual(news.label2, "Morning Briefing: Part 7")
        self.assertEqual(news.properties["when"].split(" · ")[1], "45 min left")
        self.assertEqual(news.properties["airs"], "Today · %s – %s · S1 · E7 · Live" % (clock(NOW - 900), clock(NOW + 2700)))
        self.assertEqual(news.properties["next"], "%s  Midday Bulletin" % clock(NOW + 2700))
        self.assertTrue(news.properties["picture"].endswith("/image?w=640"))
        # How far into it: the progress bar, as a tile's.
        self.assertEqual(news.video.values["setResumePoint"][1], 3600.0)
        self.assertEqual((rows[1].properties["favorite"], rows[1].properties["short"]), ("1", "102"))
        self.assertEqual((rows[2].label2, rows[2].properties["when"]), ("No guide", ""))
        self.assertEqual(self.win.made[L.LIVE_HEADING].label, "All channels")
        self.assertEqual(self.win.made[L.LIVE_COUNT].label, "7 channels")
        self.assertEqual(self.win.focused, L.LIVE_LIST)
        # Never every channel at once.
        self.assertNotIn("/api/live/channels", [p for p, _ in self.asked])

    def test_a_region_opens_and_its_groups_drop_the_region(self):
        self.screen.show(False)
        stored = {}
        orig = (window.kodi.remember, window.kodi.setting)
        window.kodi.remember = lambda key, value: stored.update({key: value})
        window.kodi.setting = lambda name: "srv" if name == "server_id" else ""
        self.addCleanup(lambda: (setattr(window.kodi, "remember", orig[0]), setattr(window.kodi, "setting", orig[1])))
        found = self.win.made[L.LIVE_CATEGORIES]
        found.selected = 5
        self.screen.click(L.LIVE_CATEGORIES)
        labels = [c[0] for c in self.categories()]
        self.assertEqual(labels[5:8], ["UK", "Sport", "News"])
        self.assertEqual(self.win.made[L.LIVE_CATEGORIES].items[5].properties["region"], "open")
        self.assertEqual(self.win.made[L.LIVE_CATEGORIES].items[6].properties["inner"], "1")
        # Focus resting on a category shows its channels.
        found.selected = 6
        self.win.focused = L.LIVE_CATEGORIES
        self.screen.moved(L.LIVE_CATEGORIES, window.DOWN)
        self.assertEqual([r.label for r in self.rows()], ["201 SKY SPORTS 1", "202 SKY SPORTS 2"])
        self.assertEqual(self.win.made[L.LIVE_HEADING].label, "UK · Sport")
        self.assertEqual(self.categories()[6][2], "1")
        self.assertEqual(stored, {"live_tab/srv/3": "group:UK| Sport"})
        # Played from it: Next and Previous stay in the group.
        self.screen.click(L.LIVE_LIST)
        self.assertEqual([u for u, _ in xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO]], [
            "plugin://plugin.video.pley/?route=live_play&id=7&group=UK%7C+Sport", "plugin://plugin.video.pley/?route=live_play&id=8&group=UK%7C+Sport",
        ])
        # Closed again.
        found.selected = 5
        self.screen.click(L.LIVE_CATEGORIES)
        self.assertEqual([c[0] for c in self.categories()][5:7], ["UK", "News"])

    def test_pages_come_as_the_list_is_scrolled(self):
        self.channels = [dict(RADIO, id=i, number=str(1000 + i), name="Channel %d" % i, group="Radio") for i in range(1, 251)]
        self.screen.load()
        self.screen.show(False)
        self.assertEqual((len(self.rows()), self.screen.total), (livetv.PAGE, 250))
        self.win.made[L.LIVE_LIST].selected = livetv.PAGE - 10
        self.screen.moved(L.LIVE_LIST, window.DOWN)
        self.assertEqual(len(self.rows()), 2 * livetv.PAGE)
        self.assertEqual(self.win.made[L.LIVE_LIST].selected, livetv.PAGE - 10, "focus stays where it was")
        self.assertEqual(self.rows()[livetv.PAGE].label, "%d Channel %d" % (1000 + livetv.PAGE + 1, livetv.PAGE + 1))
        browses = [q for p, q in self.asked if p == "/api/live/browse"]
        self.assertEqual(browses[-1]["offset"], livetv.PAGE)

    def test_a_list_opened_further_on_gets_the_page_before(self):
        self.channels = [dict(RADIO, id=i, number=str(1000 + i), name="Channel %d" % i, group="Radio") for i in range(1, 251)]
        self.screen.load()
        self.screen.show(False)
        for digit in "1220":
            self.screen.number(int(digit))
        self.screen.typed_timer.cancel()
        self.screen.go_to_typed()
        self.assertEqual((self.screen.offset, self.win.made[L.LIVE_LIST].selected), (200, 19))
        self.win.made[L.LIVE_LIST].selected = 5
        self.screen.moved(L.LIVE_LIST, window.UP)
        self.assertEqual((self.screen.offset, len(self.rows()), self.win.made[L.LIVE_LIST].selected), (100, 150, 105))

    def test_a_favourite_from_the_menu(self):
        self.screen.show(False)
        self.win.made[L.LIVE_LIST].selected = 0
        with test_window.Dialogs(picks=[1]) as asked:
            self.screen.menu(L.LIVE_LIST)
        self.assertEqual(asked.shown[0][0], ["Watch", "Add to favourites", "Schedule"])
        self.assertEqual(self.favourites, [("PUT", "/api/live/channels/1/favorite")])
        self.assertEqual(self.rows()[0].properties["favorite"], "1")
        self.assertEqual(self.categories()[1][:2], ("Favourites", "2"))
        # The last favourite removed on Favourites: All channels again, and no Favourites.
        self.screen.choose(("favorites", None))
        for _ in range(2):
            self.win.made[L.LIVE_LIST].selected = 0
            with test_window.Dialogs(picks=[1]):
                self.screen.menu(L.LIVE_LIST)
        self.assertEqual(self.screen.category, ("all", None))
        self.assertNotIn("Favourites", [c[0] for c in self.categories()])
        self.assertEqual(self.win.focused, L.LIVE_LIST)

    def test_a_channels_schedule(self):
        self.screen.show(False)
        # Info: the schedule; a programme still to come says what it is,
        # and back from it the schedule is as it was, not asked for again.
        with test_window.Dialogs(picks=[1, -1]) as asked:
            self.screen.info(L.LIVE_LIST)
        labels, preselect = asked.shown[0]
        upcoming = "%s %s  ·  Midday Bulletin" % (livetv.day(NOW + 2700, NOW), clock(NOW + 2700))
        self.assertEqual(labels, ["On now  ·  Morning Briefing: Part 7", upcoming, "Later"])
        self.assertEqual(asked.shown[1][1], 1)
        self.assertEqual(asked.asked[0][0], "Midday Bulletin")
        self.assertIn("101 Pley News HD", asked.asked[0][1])
        guides = [q for p, q in self.asked if p == "/api/live/guide"]
        self.assertEqual(guides, [{"from": NOW, "to": NOW + 6 * HOUR, "ids": 1}])
        # What's on watches the channel.
        with test_window.Dialogs(picks=[0]):
            self.screen.info(L.LIVE_LIST)
        self.assertEqual(xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO][xbmc.played[-1][2]][0], "plugin://plugin.video.pley/?route=live_play&id=1")

    def test_number_keys_go_to_a_channel(self):
        self.screen.show(False)
        self.screen.choose(("favorites", None))
        for digit in (1, 0, 4):
            self.screen.number(digit)
        self.screen.typed_timer.cancel()
        self.assertEqual(self.win.toasts[-1], "104")
        self.screen.go_to_typed()
        # Not among the favourites: in All channels.
        self.assertEqual(self.screen.category, ("all", None))
        self.assertEqual(self.win.made[L.LIVE_LIST].selected, 2)
        self.screen.number(9)
        self.screen.typed_timer.cancel()
        self.screen.go_to_typed()
        self.assertEqual(self.win.toasts[-1], "No channel 9")

    def test_searching(self):
        self.screen.show(False)
        with test_window.Dialogs(typed=["sky"]):
            self.screen.search()
        self.assertEqual([r.label for r in self.rows()], ["201 UK| SKY SPORTS 1", "202 UK| SKY SPORTS 2"])
        self.assertEqual(self.categories()[0], ("“sky”", "2", "1"))
        self.assertEqual(self.win.made[L.LIVE_HEADING].label, "“sky”")
        # From a group too, every channel; the groups that match follow it.
        self.screen.choose(("group", "Radio"))
        with test_window.Dialogs(typed=["news"]):
            self.screen.search()
        self.assertEqual([r.label for r in self.rows()], ["101 Pley News HD", "301 UK| BBC NEWS"])
        self.assertEqual([c[0] for c in self.categories()][:3], ["“news”", "News", "UK · News"])
        found = self.win.made[L.LIVE_CATEGORIES]
        found.selected = 2
        self.screen.click(L.LIVE_CATEGORIES)
        self.assertEqual([r.label for r in self.rows()], ["301 BBC NEWS"])
        self.assertEqual(self.win.made[L.LIVE_HEADING].label, "UK · News")

    def test_the_apps_search_finds_channels(self):
        screen = window.Search(self.win)
        self.win.session.get = lambda path, **query: {"items": [], "people": [], "channels": [NEWS, SPORT]}
        screen.search("news")
        row = screen.rows.rows[-1]
        self.assertEqual((row["title"], row["shape"]), ("Live TV", "channel"))
        tiles = self.win.made[L.slot_list(L.SEARCH_ROWS, 0, "channel")].items
        self.assertEqual([(t.label, t.label2) for t in tiles], [("101 Pley News HD", "Morning Briefing: Part 7"), ("102 Sport 1080i", "Weekend Highlights")])
        self.assertEqual(tiles[0].art["tile"], self.win.session.base + "/api/live/channels/1/logo?w=320")
        # Chosen, it plays, the others found queued.
        list_id = L.slot_list(L.SEARCH_ROWS, 0, "channel")
        self.win.made[list_id].selected = 1
        screen.click(list_id)
        self.assertEqual(xbmc.played[-1][2], 1)
        self.assertEqual([u for u, _ in xbmc.PlayList.entries[xbmc.PLAYLIST_VIDEO]][1], "plugin://plugin.video.pley/?route=live_play&id=2")

    def test_without_channels_it_says_so(self):
        self.channels = []
        self.screen.load()
        notice = self.screen.instead()
        self.assertEqual((notice.title, notice.body, notice.rail), ("No channels yet", window.T("no_channels_body"), "live"))
        self.win.me["is_admin"] = True
        self.assertIn("Settings → Live TV", self.screen.instead().body)

    def test_the_rail_has_live_tv_where_there_are_channels(self):
        self.win.libraries = [{"id": 1, "name": "Movies", "kind": "movies"}]
        self.win.setup_rail()
        self.assertTrue(self.win.made[L.rail_group(L.RAIL_LIVE)].visible)
        self.assertEqual(window.rail_entries(1, True), [L.RAIL_SEARCH, L.RAIL_HOME, L.RAIL_LIVE, L.RAIL_LIBRARIES[0], L.RAIL_PLAYLISTS, L.RAIL_COLLECTIONS])
        self.assertEqual(self.win.rail_entry("live"), L.RAIL_LIVE)
        self.win.live = {"available": False, "allowed": True}
        self.win.setup_rail()
        self.assertFalse(self.win.made[L.rail_group(L.RAIL_LIVE)].visible)
        self.assertEqual(self.win.rail_entry("live"), L.RAIL_HOME)

    def test_the_rail_follows_live_tv_set_up_or_emptied(self):
        movies = {"id": 1, "name": "Movies", "kind": "movies"}
        summary = {"available": False, "allowed": True, "channels": 0}

        class Server:
            def get(inner, path, **query):
                return [movies] if path == "/api/libraries" else dict(summary)

        class Page:
            rail = "home"

            def refresh(inner):
                pass

        win = test_window.FakeWindow()
        win.session.client = Server()
        win.session.get = win.session.client.get
        win.session.details, win.session.lock = {}, window.threading.Lock()
        win.libraries, win.live, win.screen = [movies], dict(summary), Page()
        win.setup_rail()
        self.assertFalse(win.made[L.rail_group(L.RAIL_LIVE)].visible)
        # An administrator adds a source: live_channels_changed, and the rail has Live TV.
        summary.update(available=True, channels=5)
        win.changed()
        self.assertTrue(win.made[L.rail_group(L.RAIL_LIVE)].visible)
        # Its last source removed: the rail loses it again.
        summary.update(available=False, channels=0)
        win.changed()
        self.assertFalse(win.made[L.rail_group(L.RAIL_LIVE)].visible)


if __name__ == "__main__":
    unittest.main()
