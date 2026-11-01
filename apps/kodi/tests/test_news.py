import json
import shutil
import tempfile
import unittest

import support  # noqa: F401
import xbmc
import xbmcaddon
import xbmcgui
from pley import api, kodi, news, requests, service

SERVER = "https://pley.example"
BASE = "http://127.0.0.1:5000/s3cret"
HOME = "plugin://plugin.video.pley/"
CATCH_UP = "/api/requests?unseen=1&limit=20"
ON = {"enabled": True, "access": "ask", "manage": False, "pending": 0, "failed": 0, "unseen": 0, "own": 1, "sign_in_again": None,
      "changed_at": None}


def ready(id, title, updated_at, **more):
    return dict({"id": id, "title": title, "kind": "movie", "stage": "available", "item_id": id + 100, "poster_url": None,
                 "available_seasons": [], "updated_at": updated_at}, **more)


class Server:
    """The server as the service asks it: answers by path (its query too);
    a path it doesn't know is out of reach."""

    def __init__(self):
        self.answers = {"/api/requests/summary": (200, ON), "/api/auth/me": (200, {"id": 1, "name": "Ann"})}
        self.asked = []

    def call(self, method, path, body=None, timeout=5, fine=(410,)):
        self.asked.append((method, path))
        found = self.answers.get(path)
        if found is None:
            return None
        return api.Response(found[0], {}, json.dumps(found[1]).encode("utf-8"))


class NewsTest(unittest.TestCase):
    def setUp(self):
        self.profile = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.profile, True)
        xbmcaddon.settings["_profile"] = self.profile
        self.addCleanup(xbmcaddon.settings.pop, "_profile", None)
        for name in (requests.SUMMARY, requests.LIVE):
            kodi.clear_property(name)
            self.addCleanup(kodi.clear_property, name)
        del xbmcgui.notified[:], xbmc.builtins[:]
        self.addCleanup(xbmc.conditions.clear)
        self.addCleanup(xbmc.info_labels.clear)
        self.server = Server()
        self.who = (SERVER, None, "a")
        self.playing = False
        self.now = 1000.0
        self.jobs = []
        self.news = self.started()

    def started(self):
        """The service's news, as Kodi starts."""
        return news.News(self.server.call, lambda: self.who, self.jobs.append, lambda: self.playing, lambda: BASE, clock=lambda: self.now)

    def turn(self, seconds=0, stream_open=False):
        """A turn of the service's loop, `seconds` after the last, and the
        jobs it gave the worker."""
        self.now += seconds
        self.news.tick(stream_open)
        while self.jobs:
            self.jobs.pop(0)()

    def reads(self):
        return len([path for _, path in self.server.asked if path == "/api/requests/summary"])

    def test_the_summary_is_read_as_kodi_starts_and_kept(self):
        self.turn()
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), ON)
        self.turn(30)
        self.assertEqual(self.reads(), 1)
        self.turn(31)
        self.assertEqual(self.reads(), 2, "every minute while the stream is down")
        self.turn(120, stream_open=True)
        self.assertEqual(self.reads(), 2, "the stream's events say when")

    def test_a_summary_that_cant_be_read_is_read_again_in_a_minute(self):
        self.server.answers["/api/requests/summary"] = (500, {"code": "internal", "message": "database busy"})
        self.turn(stream_open=True)
        self.turn(30, stream_open=True)
        self.assertEqual(self.reads(), 1)
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))
        self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [ready(42, "Heat", 2000)]})
        self.turn(31, stream_open=True)
        self.assertEqual(self.reads(), 2, "the stream open, though no event came")
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), dict(ON, unseen=1))
        self.turn(stream_open=True)
        self.assertEqual(xbmcgui.notified, [("Pley", "“Heat” is ready to watch.")], "the news from before Kodi started")
        self.turn(120, stream_open=True)
        self.assertEqual(self.reads(), 2)

    def test_news_that_cant_be_read_is_looked_at_again_in_a_minute(self):
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))
        self.server.answers[CATCH_UP] = (503, {"code": "unavailable", "message": "starting"})
        self.turn(stream_open=True)
        self.turn(61, stream_open=True)
        self.assertEqual((self.reads(), self.server.asked.count(("GET", CATCH_UP))), (2, 2))
        self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [ready(42, "Heat", 2000)]})
        self.turn(61, stream_open=True)
        self.turn(stream_open=True)
        self.assertEqual(xbmcgui.notified, [("Pley", "“Heat” is ready to watch.")])
        self.turn(120, stream_open=True)
        self.assertEqual(self.server.asked.count(("GET", CATCH_UP)), 3)

    def test_a_server_without_requests_for_apps_is_asked_again_when_the_stream_opens(self):
        self.server.answers["/api/requests/summary"] = (404, {"code": "not_found", "message": "no"})
        self.turn()
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), {"enabled": False})
        self.turn(600)
        self.assertEqual(self.reads(), 1)
        self.news.opened()
        self.turn(1)
        self.assertEqual(self.reads(), 2)

    def test_events_read_the_summary_once_soon_after(self):
        self.turn(stream_open=True)
        for _ in range(3):
            self.news.heard({"type": "requests_changed", "request_id": 42})
        self.turn(0.5, stream_open=True)
        self.assertEqual(self.reads(), 1)
        self.turn(0.6, stream_open=True)
        self.assertEqual(self.reads(), 2)

    def test_ready_to_watch_is_said_with_its_poster(self):
        self.turn(stream_open=True)
        self.news.heard({"type": "request_available", "request_id": 42, "title": "Heat", "kind": "movie", "tmdb_id": 949, "item_id": 7,
                         "poster_url": "/api/discover/images/poster/h.jpg", "seasons": None, "complete": True})
        while self.jobs:
            self.jobs.pop(0)()
        self.assertEqual(self.news.waiting, [("“Heat” is ready to watch.", BASE + "/api/discover/images/poster/h.jpg?w=185")])
        self.turn(stream_open=True)
        self.assertEqual(xbmcgui.notified, [("Pley", "“Heat” is ready to watch.")])
        self.assertEqual(news.announced(news.ANNOUNCED + SERVER + "|1"), ["42"], "never again as news from before")
        self.news.heard({"type": "request_available", "request_id": 43, "title": "Ronin", "kind": "movie", "item_id": None})
        self.assertEqual(self.jobs, [], "nothing for a library this account can't open")

    def test_nothing_shows_over_playing_video(self):
        self.turn()
        self.playing = True
        for n in range(4):
            self.news.notice(self.who, "Notice %d" % n)
        self.turn()
        self.assertEqual(xbmcgui.notified, [])
        self.playing = False
        self.turn()
        self.assertEqual([message for _, message in xbmcgui.notified], ["Notice 1", "Notice 2", "Notice 3"], "the last three")

    def test_news_from_before_is_said_once(self):
        declined = dict(ready(43, "Ronin", 1900), stage="declined", item_id=None)
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=2))
        self.server.answers[CATCH_UP] = (200, {"total": 2, "requests": [ready(42, "Heat", 2000), declined]})
        self.turn()
        self.turn()
        self.assertEqual(xbmcgui.notified, [("Pley", "“Heat” is ready to watch.")], "a TV pops up ready ones only")
        # Kodi started again.
        self.news = self.started()
        self.turn()
        self.turn()
        self.assertEqual(len(xbmcgui.notified), 1, "never twice")

    def test_a_show_ready_season_by_season_is_said_once_each(self):
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))

        def kodi_starts(request):
            self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [request]})
            self.news = self.started()
            self.turn()
            self.turn()

        show = ready(42, "Dark", 2000, kind="show", stage="partial", seasons=[1, 2, 3], available_seasons=[1])
        kodi_starts(show)
        # Followed, unfollowed, matched again: it moves, and has nothing new.
        kodi_starts(dict(show, updated_at=2500))
        kodi_starts(dict(show, available_seasons=[1, 2], updated_at=3000))
        kodi_starts(dict(show, stage="available", available_seasons=[1, 2, 3], updated_at=4000))
        kodi_starts(dict(show, stage="available", available_seasons=[1, 2, 3], updated_at=4500))
        self.assertEqual([message for _, message in xbmcgui.notified], [
            "Season 1 of “Dark” is ready to watch.", "Season 2 of “Dark” is ready to watch.", "“Dark” is ready to watch."])

    def test_what_an_event_said_isnt_said_again_as_kodi_starts(self):
        self.turn(stream_open=True)
        self.news.heard({"type": "request_available", "request_id": 42, "title": "Dark", "kind": "show", "item_id": 7, "seasons": [1],
                         "complete": False})
        self.turn(stream_open=True)
        self.turn(stream_open=True)
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))
        self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [
            ready(42, "Dark", 2500, kind="show", stage="partial", seasons=[1, 2], available_seasons=[1])]})
        self.news = self.started()
        self.turn()
        self.turn()
        self.assertEqual(xbmcgui.notified, [("Pley", "Season 1 of “Dark” is ready to watch.")])

    def test_what_was_announced_is_kept_to_the_last_few_hundred(self):
        key = news.ANNOUNCED + SERVER + "|1"
        news.announce(key, [str(n) for n in range(250)])
        news.announce(key, ["7", "300", "301/1"] + [str(n) for n in range(1000, 1100)])
        kept = news.announced(key)
        self.assertEqual(len(kept), news.ANNOUNCED_KEEP)
        self.assertEqual(kept[-103:], ["7", "300", "301/1"] + [str(n) for n in range(1000, 1100)], "the newest last, each once")
        self.assertNotIn("0", kept)

    def test_news_of_several_says_how_many(self):
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=2))
        self.server.answers[CATCH_UP] = (200, {"total": 2, "requests": [ready(42, "Heat", 2000), ready(44, "Ronin", 1990)]})
        self.turn()
        self.turn()
        self.assertEqual(xbmcgui.notified, [("Pley", "2 of your requests have news")])

    def test_unseen_rising_with_no_event_looks_at_the_news(self):
        self.turn()
        self.assertNotIn(("GET", CATCH_UP), self.server.asked)
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))
        self.server.answers[CATCH_UP] = (200, {"total": 0, "requests": []})
        self.turn(61)
        self.assertIn(("GET", CATCH_UP), self.server.asked)
        del self.server.asked[:]
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=2))
        self.news.heard({"type": "requests_changed", "request_id": 44})
        self.turn(1.5, stream_open=True)
        self.assertEqual(self.reads(), 1)
        self.assertNotIn(("GET", CATCH_UP), self.server.asked, "the event explained it")

    def test_listings_are_shown_again_when_they_change(self):
        xbmc.info_labels["Container.FolderPath"] = HOME
        self.turn(stream_open=True)
        self.server.answers["/api/requests/summary"] = (200, dict(ON, own=2))
        self.news.asked()
        self.turn(stream_open=True)
        self.turn(stream_open=True)
        self.assertEqual(xbmc.builtins, ["Container.Refresh"], "the root says the number")
        del xbmc.builtins[:]
        self.server.answers["/api/requests/summary"] = (200, dict(ON, own=2, changed_at=1234))
        self.news.asked()
        self.turn(stream_open=True)
        self.turn(stream_open=True)
        self.assertEqual(xbmc.builtins, [], "the same number")
        xbmc.info_labels["Container.FolderPath"] = HOME + "?route=requests"
        xbmc.conditions.add(news.DIALOG)
        self.news.heard({"type": "requests_changed", "request_id": 42})
        self.turn(stream_open=True)
        self.assertEqual(xbmc.builtins, [], "not under a dialog")
        xbmc.conditions.clear()
        self.turn(stream_open=True)
        self.assertEqual(xbmc.builtins, ["Container.Refresh"])
        del xbmc.builtins[:]
        xbmc.info_labels["Container.FolderPath"] = HOME + "?route=library&id=1"
        self.news.heard({"type": "requests_changed", "request_id": 42})
        self.turn(stream_open=True)
        xbmc.info_labels["Container.FolderPath"] = HOME + "?route=requests"
        self.turn(stream_open=True)
        self.assertEqual(xbmc.builtins, [], "a listing that shows later is read afresh")

    def test_your_requests_while_something_downloads(self):
        live = HOME + "?route=requests"
        xbmc.info_labels["Container.FolderPath"] = live
        kodi.set_property(requests.LIVE, live)
        self.turn(stream_open=True)
        self.turn(5, stream_open=True)
        self.assertEqual(xbmc.builtins, [])
        self.turn(5, stream_open=True)
        self.assertEqual(xbmc.builtins, ["Container.Refresh"], "every 10 seconds")
        self.playing = True
        self.turn(10, stream_open=True)
        self.assertEqual(len(xbmc.builtins), 1, "never behind a video")
        self.playing = False
        kodi.clear_property(requests.LIVE)
        self.turn(10, stream_open=True)
        self.assertEqual(len(xbmc.builtins), 2, "the one that waited")
        self.turn(25, stream_open=True)
        self.assertEqual(len(xbmc.builtins), 2, "events say when, with the stream")
        self.turn(31, stream_open=False)
        self.assertEqual(len(xbmc.builtins), 3, "every 30 seconds without it")

    def test_another_sign_in_starts_afresh(self):
        self.turn()
        self.assertTrue(kodi.get_property(requests.SUMMARY))
        self.who = (SERVER, 3, "a")
        self.news.tick(False)
        self.assertEqual(kodi.get_property(requests.SUMMARY), "", "the profile's own")
        self.turn()
        self.assertEqual(self.reads(), 2)
        self.who = None
        self.turn(120)
        self.assertEqual((kodi.get_property(requests.SUMMARY), self.reads()), ("", 2), "signed out")

    def test_someone_elses_sign_in_has_their_own_news(self):
        self.server.answers["/api/requests/summary"] = (200, dict(ON, unseen=1))
        self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [ready(42, "Heat", 2000)]})
        self.turn(stream_open=True)
        self.turn(stream_open=True)
        self.assertEqual(xbmcgui.notified, [("Pley", "“Heat” is ready to watch.")])
        # Ann's ready-to-watch, heard just before Bo signed in with a code on
        # the same server, isn't said to him.
        self.news.heard({"type": "request_available", "request_id": 43, "title": "Ronin", "kind": "movie", "item_id": 8})
        self.server.answers["/api/auth/me"] = (200, {"id": 2, "name": "Bo"})
        self.server.answers[CATCH_UP] = (200, {"total": 1, "requests": [ready(50, "Up", 1000)]})
        self.who = (SERVER, None, "b")
        self.turn(stream_open=True)
        self.turn(stream_open=True)
        self.assertEqual([message for _, message in xbmcgui.notified], ["“Heat” is ready to watch.", "“Up” is ready to watch."],
                         "his news from before, under his own account")
        self.assertTrue(kodi.remembered(news.ANNOUNCED + SERVER + "|2"))

    def test_the_plugin_is_heard(self):
        asked = []
        monitor = service.Monitor(lambda: None, asking=lambda: asked.append(True))
        monitor.onNotification("plugin.video.pley", "Other.pley.asking", "")
        monitor.onNotification("script.other", "Other.pley.asking", "")
        monitor.onNotification("plugin.video.pley", "Other.changed", "")
        self.assertEqual(asked, [True])


if __name__ == "__main__":
    unittest.main()
