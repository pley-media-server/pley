import json
import time
import unittest
import urllib.parse

import support
import xbmc
import xbmcaddon
import xbmcgui
import xbmcplugin
from pley import asking, kodi, plugin, profiles, requests, words

DAY = 86400
BASE = "plugin://plugin.video.pley/"
ON = {"enabled": True, "access": "ask", "manage": False, "pending": 0, "failed": 0, "unseen": 0, "own": 1, "sign_in_again": None,
      "movies_left": 2, "seasons_left": 5, "movies_limit": 3, "seasons_limit": 10, "movies_next_at": None, "seasons_next_at": None,
      "changed_at": None}
MANAGER = dict(ON, access="auto", manage=True, pending=2, failed=1, movies_left=None, seasons_left=None, movies_limit=None,
               seasons_limit=None)
ACTIVE = "GET /api/requests"


def title(tmdb, kind="movie", **more):
    return dict({"kind": kind, "tmdb_id": tmdb, "title": "Title %d" % tmdb, "year": 2020, "overview": "Something happens.",
                 "poster_url": "/api/discover/images/poster/p%d.jpg" % tmdb, "backdrop_url": None, "rating": 7.5, "status": "none",
                 "item_id": None, "request_id": None}, **more)


def detail(base, **more):
    return dict(base, **dict({"imdb_id": None, "declined": None, "restriction": None, "watching": False, "requests": [], "movies_left": 2,
                              "seasons_left": 5, "movies_next_at": None, "seasons_next_at": None}, **more))


def season(number, state, **more):
    return dict({"number": number, "name": "Specials" if number == 0 else "Season %d" % number, "state": state, "episodes": 10,
                 "aired": 10, "in_library": 0, "air_date": None, "poster_url": None, "declined": False, "request_id": None}, **more)


MOVIE = detail(title(949), title="Saltwind", year=1995)
SHOW = detail(title(1396, "show"), title="Driftline", year=2008, seasons=[
    season(0, "missing"), season(1, "library", in_library=10), season(2, "missing"), season(3, "unaired", episodes=8, aired=0,
                                                                                        air_date="2027-01-05")])


def request(id=42, **more):
    return dict({"id": id, "kind": "movie", "tmdb_id": 949, "title": "Saltwind", "year": 1995, "poster_url": "/api/discover/images/poster/h.jpg",
                 "backdrop_url": None, "seasons": None, "available_seasons": [], "status": "pending", "stage": "pending",
                 "stage_at": None, "reason": None, "note": None, "mine": True, "watching": False, "watchers": 0, "unseen": False,
                 "actions": ["withdraw"], "requested_by": {"id": 1, "name": "Ann"}, "created_at": time.time() - 3 * DAY,
                 "retry_at": None, "item_id": None, "download": None}, **more)


class Dialogs:
    """xbmcgui.Dialog answering as a test says: each yesno, select,
    multiselect and input takes the next of `answers`; what each dialog
    showed is kept in `shown`. Notices go where the stand-in keeps them."""

    def __init__(self, *answers):
        self.answers = list(answers)
        self.shown = []

    def __enter__(self):
        self.real = xbmcgui.Dialog
        outer = self

        class Dialog:
            def notification(self, heading, message, icon="", time=0, sound=True):
                xbmcgui.notified.append((heading, message))

            def ok(self, heading, message):
                outer.shown.append(("ok", heading, message))
                return True

            def yesno(self, heading, message, nolabel="", yeslabel="", autoclose=0):
                outer.shown.append(("yesno", heading, message, yeslabel))
                return outer.answers.pop(0)

            def select(self, heading, entries, autoclose=0, preselect=-1, useDetails=False):
                outer.shown.append(("select", heading, [getattr(e, "label", e) for e in entries], preselect))
                return outer.answers.pop(0)

            def multiselect(self, heading, entries, autoclose=0, preselect=None, useDetails=False):
                outer.shown.append(("multiselect", heading, [(e.label, e.label2) for e in entries], list(preselect or [])))
                return outer.answers.pop(0)

            def input(self, heading, defaultt="", type=0, option=0, autoclose=0):
                outer.shown.append(("input", heading, defaultt))
                return outer.answers.pop(0)

        xbmcgui.Dialog = Dialog
        return self

    def __exit__(self, *exc):
        xbmcgui.Dialog = self.real


class Case(unittest.TestCase):
    """The plugin against a stand-in of the service's proxy: answers by
    "METHOD /path" (a function of the query, or (status, body))."""

    def setUp(self):
        self.answers = {"GET /api/requests/summary": (200, ON), ACTIVE: (200, {"total": 0, "requests": [], "people": []})}
        self.waits = {}
        self.server = support.FakeServer(self.handle)
        self.addCleanup(self.server.close)
        kodi.set_property(kodi.PROXY, self.server.url + "/s3cret")
        for name in (kodi.PROXY, requests.SUMMARY, requests.LIVE, asking.FOUND):
            self.addCleanup(kodi.clear_property, name)
        for name in (requests.SUMMARY, requests.LIVE, asking.FOUND):
            kodi.clear_property(name)
        del xbmcplugin.calls[:], xbmcplugin.sorts[:], xbmc.builtins[:], xbmcgui.notified[:]
        self.addCleanup(xbmc.info_labels.clear)
        # One of pley's listings on screen: what was done there shows it again.
        xbmc.info_labels["Container.FolderPath"] = BASE + "?route=discover"

    def handle(self, request):
        path, _, query = request.path[len("/s3cret"):].partition("?")
        key = "%s %s" % (request.method, path)
        time.sleep(self.waits.get(key, 0))
        found = self.answers.get(key, (404, {"code": "not_found", "message": "nothing at " + path}))
        status, body = found(dict(urllib.parse.parse_qsl(query))) if callable(found) else found
        return support.answer(status, body)

    def sent(self, method=None, path=None):
        """What the plugin asked, as (method, path and query, body)."""
        found = []
        for r in self.server.requests:
            target = r.path[len("/s3cret"):]
            if (method is None or r.method == method) and (path is None or target.partition("?")[0] == path):
                found.append((r.method, target, r.json() if r.body else None))
        return found

    def run_plugin(self, query, handle="3"):
        """The plugin called with `query`: the entries it listed."""
        plugin.Plugin([BASE, handle, "?" + query]).run()
        return [(c[1], c[2], c[3]) for c in xbmcplugin.calls if c[0] == "item"]

    def labels(self, entries):
        return [li.label for _, li, _ in entries]

    def summary(self, s):
        """The summary as the service keeps it."""
        kodi.set_property(requests.SUMMARY, json.dumps(s))


class DiscoverTest(Case):
    def test_the_lists_then_the_requests(self):
        entries = self.run_plugin("route=discover")
        self.assertEqual(self.labels(entries), ["Trending this week", "Popular movies", "Popular shows",
                                                "Coming to cinemas", "Popular anime", "Your requests"])
        self.assertEqual(entries[0][0], BASE + "?route=discover_list&list=trending")
        self.assertEqual(entries[0][1].video.values["setPlot"], "Details and pictures from TMDB.")
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), ON, "kept for the listings that follow")

    def test_the_number_on_the_requests(self):
        self.answers["GET /api/requests/summary"] = (200, dict(ON, unseen=2))
        self.assertEqual(self.labels(self.run_plugin("route=discover"))[-1], "Your requests (2)")
        del xbmcplugin.calls[:]
        self.answers["GET /api/requests/summary"] = (200, MANAGER)
        self.assertEqual(self.labels(self.run_plugin("route=discover"))[-1], "Requests [COLOR %s](3)[/COLOR]" % requests.RED,
                         "a request that couldn't be sent")

    def test_a_sign_in_from_before_says_to_sign_in_again(self):
        self.answers["GET /api/requests/summary"] = (200, dict(ON, sign_in_again="requests"))
        entries = self.run_plugin("route=discover")
        self.assertEqual(self.labels(entries), ["Sign in again to ask for movies and shows"])
        self.assertEqual((entries[0][0], entries[0][2]), (BASE + "?route=sign_in", False))
        self.assertEqual(entries[0][1].label2, "Sign in again")

    def test_requests_off(self):
        for answer in [(200, dict(ON, enabled=False, access="ask")), (404, {"code": "not_found", "message": "no"})]:
            del xbmcplugin.calls[:]
            self.answers["GET /api/requests/summary"] = answer
            entries = self.run_plugin("route=discover")
            self.assertEqual(self.labels(entries), ["Requests are off"], answer)
            self.assertEqual(entries[0][1].video.values["setPlot"], "This server doesn't take requests for movies and shows.")

    def test_an_account_that_may_not_ask_sees_its_own(self):
        self.answers["GET /api/requests/summary"] = (200, dict(ON, access="off", own=2))
        self.assertEqual(self.labels(self.run_plugin("route=discover")), ["Requests aren't open to this account", "Your requests"])

    def test_the_root(self):
        settings = dict(xbmcaddon.settings)
        self.addCleanup(lambda: (xbmcaddon.settings.clear(), xbmcaddon.settings.update(settings)))
        xbmcaddon.settings.update({"server_url": "https://pley.example", "look": 1})
        self.answers.update({"GET /status": (200, {"signed_in": True, "server": "https://pley.example"}),
                             "GET /api/hubs/home": (200, {"hubs": []}), "GET /api/libraries": (200, [])})
        self.answers["GET /api/requests/summary"] = (200, dict(ON, unseen=1))
        self.assertEqual(self.labels(self.run_plugin("")), ["Playlists", "Search", "Discover", "Your requests (1)", "Settings"])
        del xbmcplugin.calls[:]
        self.run_plugin("")
        self.assertEqual(len(self.sent("GET", "/api/requests/summary")), 1, "kept: the service reads it again on its events")
        del xbmcplugin.calls[:]
        kodi.clear_property(requests.SUMMARY)
        self.answers["GET /api/requests/summary"] = (404, {"code": "not_found", "message": "an older server"})
        self.assertEqual(self.labels(self.run_plugin("")), ["Playlists", "Search", "Settings"])
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), {"enabled": False}, "off until asked again")


class ListTest(Case):
    def test_pages_are_read_until_a_screenful(self):
        pages = {"1": [title(n) for n in range(1, 6)], "2": [], "3": [title(5)] + [title(n) for n in range(10, 29)]}
        self.answers["GET /api/discover/trending"] = lambda query: (200, {"page": int(query["page"]), "total_pages": 4,
                                                                           "results": pages[query["page"]]})
        entries = self.run_plugin("route=discover_list&list=trending")
        self.assertEqual(len(entries), 5 + 19 + 1, "a short page and an empty one are not the end; each title once")
        self.assertEqual(entries[-1][0], BASE + "?route=discover_list&list=trending&page=4")
        self.assertEqual([t for _, t, _ in self.sent("GET", "/api/discover/trending")],
                         ["/api/discover/trending?page=1", "/api/discover/trending?page=2", "/api/discover/trending?page=3"])
        self.assertEqual(self.sent("GET", "/api/requests")[0][1], "/api/requests?status=pending%2Capproved%2Cfailed&limit=200")
        self.assertEqual(xbmcplugin.sorts, [(0, "%L", "%P"), (1, "%L", "%P"), (2, "", "")])

    def test_titles_say_what_they_are(self):
        following = request(stage="downloading", status="approved", mine=False, watching=True, actions=["unwatch"],
                            download={"progress": 0.42, "state": "downloading", "eta": None})
        self.answers[ACTIVE] = (200, {"total": 1, "requests": [following], "people": []})
        self.answers["GET /api/discover/movies"] = (200, {"page": 1, "total_pages": 1, "results": [
            title(1), title(2, status="library", item_id=7), title(1396, "show", status="partial", item_id=8),
            title(949, status="requested", request_id=42)]})
        asked, library, partial, mine = self.run_plugin("route=discover_list&list=movies")
        self.assertEqual((asked[0], asked[2]), (BASE + "?route=title&kind=movie&tmdb=1", False), "OK asks for it")
        self.assertEqual(asked[1].label2, "")
        self.assertEqual(asked[1].art["poster"], self.server.url + "/s3cret/api/discover/images/poster/p1.jpg?w=500")
        self.assertEqual(asked[1].video.values["setPlot"], "Something happens.[CR][CR]Details and pictures from TMDB.")
        self.assertEqual((library[0], library[2], library[1].properties["IsPlayable"]), (BASE + "?route=play&id=7", False, "true"))
        badge = "[COLOR %s]In library[/COLOR]" % requests.GREEN
        self.assertEqual((library[1].label2, library[1].video.values["setProductionCode"]), (badge, badge))
        self.assertTrue(library[1].video.values["setPlot"].startswith(badge + "[CR][CR]"))
        self.assertEqual((partial[0], partial[2]), (BASE + "?route=item&id=8", True))
        self.assertEqual(partial[1].menu, [("Seasons", "Container.Update(%s?route=seasons&tmdb=1396)" % BASE),
                                           ("Request more seasons", "RunPlugin(%s?route=title&kind=show&tmdb=1396&more=1)" % BASE)])
        self.assertEqual(mine[1].label2, "[COLOR %s]Downloading 42%%[/COLOR]" % requests.BLUE, "its request's stage")
        self.assertEqual(mine[1].menu, [("Stop following", "RunPlugin(%s?route=act&id=42&do=unwatch)" % BASE)])
        self.assertIn(("content", "movies"), xbmcplugin.calls)

    def test_a_list_that_cant_be_read_says_why(self):
        self.answers["GET /api/discover/trending"] = (503, {"code": "tmdb_down", "message": "TMDB timed out", "setup": True})
        self.assertEqual(self.labels(self.run_plugin("route=discover_list&list=trending")), ["TMDB isn't answering", "Try again"])
        del xbmcplugin.calls[:]
        self.answers["GET /api/discover/trending"] = (503, {"code": "tmdb_key", "message": "401", "setup": True, "retry": False})
        entries = self.run_plugin("route=discover_list&list=trending")
        self.assertEqual(self.labels(entries), ["TMDB refused this server's key"])
        self.assertEqual(entries[0][1].video.values["setPlot"], "An administrator needs to check it.")

    def test_results_for_searchs_words(self):
        # The See all of Search's Discover entry: all TMDB has for the words, with
        # their badges, a screenful at a time.
        def page(query):
            first = 20 * int(query["page"]) - 19
            found = [title(n, status="library", item_id=7) if n == 1 else title(n) for n in range(first, first + 20)]
            return 200, {"page": int(query["page"]), "total_pages": 3, "results": found}

        self.answers["GET /api/discover/search"] = page
        entries = self.run_plugin("route=discover_search&q=saltwind")
        self.assertIn(("category", "Results for “saltwind”"), xbmcplugin.calls)
        self.assertEqual(len(entries), 20 + 1)
        self.assertEqual(entries[0][1].label2, "[COLOR %s]In library[/COLOR]" % requests.GREEN)
        self.assertEqual(entries[-1][0], BASE + "?route=discover_search&q=saltwind&page=2")
        self.assertEqual(self.sent("GET", "/api/discover/search")[0][1], "/api/discover/search?q=saltwind&page=1")
        del xbmcplugin.calls[:]
        self.assertEqual(self.labels(self.run_plugin("route=discover_search&q=saltwind&page=2"))[0], "Title 21")
        self.assertIn(("category", "Results for “saltwind”"), xbmcplugin.calls)

    def test_nothing_for_the_words(self):
        self.answers["GET /api/discover/search"] = (200, {"page": 1, "total_pages": 0, "results": []})
        entries = self.run_plugin("route=discover_search&q=zzz")
        self.assertEqual(self.labels(entries), ["Nothing on TMDB for “zzz”"])
        self.assertEqual(entries[0][0], BASE + "?route=search", "OK searches again, in Search")

    def test_words_as_the_server_takes_them(self):
        self.answers["GET /api/discover/search"] = (200, {"page": 1, "total_pages": 0, "results": []})
        self.run_plugin("route=discover_search&q=" + urllib.parse.quote("  " + "é" * 250))
        self.assertEqual(self.sent("GET", "/api/discover/search")[0][1], "/api/discover/search?" + urllib.parse.urlencode({"q": "é" * 200, "page": 1}))
        # No words (a favourite of Discover's search from before): Search asks
        # for them, on Kodi's keyboard.
        with Dialogs("saltwind") as dialogs:
            self.run_plugin("route=discover_search")
        self.assertEqual(dialogs.shown, [("input", "Search", "")])
        self.assertEqual(xbmc.builtins[-1], "Container.Update(%s?route=search&q=saltwind)" % BASE)

    def test_results_tmdb_cant_give_say_why(self):
        self.answers["GET /api/discover/search"] = (503, {"code": "tmdb_down", "message": "TMDB timed out", "setup": True})
        self.assertEqual(self.labels(self.run_plugin("route=discover_search&q=saltwind")), ["TMDB isn't answering", "Try again"])
        self.assertIn(("category", "Results for “saltwind”"), xbmcplugin.calls)


class TitleTest(Case):
    def ask(self, query, *answers, status=201, answer=None):
        self.answers["POST /api/requests"] = (status, answer if answer is not None else {"status": "pending", "title": "Saltwind"})
        with Dialogs(*answers) as dialogs:
            self.run_plugin(query, handle="-1")
        return dialogs.shown

    def test_a_movie_is_asked_for_once_its_dialog_says_so(self):
        self.answers["GET /api/discover/movie/949"] = (200, MOVIE)
        shown = self.ask("route=title&kind=movie&tmdb=949", True)
        self.assertEqual(shown, [("yesno", "Request", "Saltwind (1995)[CR]You can ask for 2 more movies and 5 more seasons.[CR]"
                                  "Limits: 3 movies and 10 seasons per 7 days.", "Request")])
        self.assertEqual(self.sent("POST", "/api/requests"), [("POST", "/api/requests", {"kind": "movie", "tmdb_id": 949})])
        self.assertEqual(xbmcgui.notified, [("Pley", "Asked for “Saltwind”: an administrator decides.")])
        self.assertEqual(xbmc.builtins, ["NotifyAll(plugin.video.pley,pley.asking)", "Container.Refresh"])
        self.assertEqual(json.loads(kodi.get_property(requests.SUMMARY)), ON, "kept for the listing shown again; the service reads it anew")
        del self.server.requests[:]
        self.ask("route=title&kind=movie&tmdb=949", False)
        self.assertEqual(self.sent("POST"), [], "cancelled")

    def test_a_show_offers_the_seasons_it_may_be_asked_for(self):
        self.answers["GET /api/discover/show/1396"] = (200, SHOW)
        airs = "Airs " + words.day("2027-01-05", time.time())
        shown = self.ask("route=title&kind=show&tmdb=1396", [], [0, 1], answer={"status": "approved", "title": "Driftline"})
        rows = [("Season 2", "10 episodes"), ("Season 3", airs), ("Specials", "10 episodes")]
        self.assertEqual(shown, [("multiselect", "Which seasons?", rows, [0]), ("ok", "Which seasons?", "Pick a season."),
                                 ("multiselect", "Which seasons?", rows, [])])
        self.assertEqual(self.sent("POST", "/api/requests")[0][2], {"kind": "show", "tmdb_id": 1396, "seasons": [2, 3]})
        self.assertEqual(xbmcgui.notified, [("Pley", "“Driftline” is approved and on its way.")])

    def test_more_seasons_than_may_be_asked_for(self):
        self.answers["GET /api/discover/show/1396"] = (200, dict(SHOW, seasons_left=1))
        shown = self.ask("route=title&kind=show&tmdb=1396", [0, 1], [1])
        self.assertEqual(shown[1], ("ok", "Which seasons?", "You can ask for 2 more movies and 1 more season.[CR]"
                                    "Limits: 3 movies and 10 seasons per 7 days."))
        self.assertEqual(shown[2][3], [0, 1], "as they were picked")
        self.assertEqual(self.sent("POST", "/api/requests")[0][2]["seasons"], [3])

    def test_a_season_picked_from_its_list(self):
        self.answers["GET /api/discover/show/1396"] = (200, SHOW)
        shown = self.ask("route=title&kind=show&tmdb=1396&season=3", None)
        self.assertEqual(shown[0][3], [1], "that season alone")
        self.assertEqual(self.sent("POST"), [])

    def test_what_ok_does_on_each_title(self):
        now = time.time()
        cases = [
            (dict(MOVIE, status="declined", declined={"reason": "Not on this server", "at": now, "again_at": now + 12 * DAY}),
             ("ok", "Saltwind (1995)", "Declined: “Not on this server”. You can ask again in 12 days.")),
            (dict(MOVIE, status="requested", requests=[request()]),
             ("ok", "Saltwind (1995)", "Requested[CR][COLOR %s]Waiting for approval[/COLOR][CR]Asked for 3 days ago" % requests.AMBER)),
            (dict(MOVIE, status="restricted", restriction="Rated R: above this account's age limit"),
             ("ok", "Saltwind (1995)", "Rated R: above this account's age limit")),
            (dict(MOVIE, movies_left=0, movies_next_at=now + 3 * DAY),
             ("ok", "Saltwind (1995)", "You've asked for all the movies you can for now: you can ask for more in 3 days.[CR]"
                                   "You can ask for 5 more seasons.[CR]Limits: 3 movies and 10 seasons per 7 days.")),
        ]
        for found, expected in cases:
            self.answers["GET /api/discover/movie/949"] = (200, found)
            self.assertEqual(self.ask("route=title&kind=movie&tmdb=949"), [expected], found["status"])
        self.assertEqual(self.sent("POST"), [])

    def test_ok_on_a_title_in_a_library_opens_it(self):
        self.answers["GET /api/discover/movie/949"] = (200, dict(MOVIE, status="library", item_id=7))
        self.assertEqual(self.ask("route=title&kind=movie&tmdb=949"), [])
        self.assertEqual(xbmc.builtins, ["PlayMedia(%s?route=play&id=7)" % BASE])

    def test_following_and_tell_me_too(self):
        followed = request(mine=False, watching=True, actions=["unwatch"], requested_by=None)
        self.answers["GET /api/discover/movie/949"] = (200, dict(MOVIE, status="requested", watching=True, requests=[followed]))
        self.answers["DELETE /api/requests/42/watch"] = (200, {})
        shown = self.ask("route=title&kind=movie&tmdb=949", True)
        self.assertEqual(shown, [("yesno", "Saltwind (1995)", "Following[CR][COLOR %s]Waiting for approval[/COLOR][CR]Asked for by someone else"
                                  % requests.AMBER, "Stop following")])
        self.assertEqual(self.sent("DELETE"), [("DELETE", "/api/requests/42/watch", None)])
        self.answers["GET /api/discover/movie/949"] = (200, dict(MOVIE, status="requested"))
        shown = self.ask("route=title&kind=movie&tmdb=949", True, status=200, answer={"status": "pending", "watching": True})
        self.assertEqual(shown, [("yesno", "Saltwind (1995)", "Someone asked for it already.", "Tell me too")])
        self.assertEqual(xbmcgui.notified[-1], ("Pley", "You'll hear when “Saltwind” is ready."))

    def test_an_account_that_may_not_ask(self):
        self.answers["GET /api/requests/summary"] = (200, dict(ON, access="off"))
        self.answers["GET /api/discover/movie/949"] = (200, MOVIE)
        self.assertEqual(self.ask("route=title&kind=movie&tmdb=949"),
                         [("ok", "Saltwind (1995)", "Requests aren't open to this account. An administrator can let it ask for movies and shows.")])

    def test_refusals_are_said(self):
        self.answers["GET /api/discover/movie/949"] = (200, MOVIE)
        refused = {"code": "too_many_requests", "message": "slow down", "retry_at": time.time() + 3 * DAY}
        self.ask("route=title&kind=movie&tmdb=949", True, status=429, answer=refused)
        self.assertEqual(xbmcgui.notified, [("Pley", "You can ask for more in 3 days.")])
        shown = self.ask("route=title&kind=movie&tmdb=949", True, False, status=403,
                         answer={"code": "insufficient_scope", "message": "needs requests"})
        self.assertEqual(shown[1][1], "Sign in again to ask for movies and shows")
        self.answers["GET /api/discover/movie/949"] = (503, {"code": "unavailable", "message": "off", "setup": False})
        self.ask("route=title&kind=movie&tmdb=949")
        self.assertEqual(xbmcgui.notified[-1], ("Pley", "Requests are off. This server doesn't take requests for movies and shows."))


class SeasonsTest(Case):
    def test_a_shows_seasons_under_its_main_action(self):
        self.answers["GET /api/discover/show/1396"] = (200, SHOW)
        entries = self.run_plugin("route=seasons&tmdb=1396")
        self.assertEqual([(li.label, li.label2) for _, li, _ in entries], [
            ("Request", "You can ask for 2 more movies and 5 more seasons."),
            ("Season 1", "[COLOR %s]In library[/COLOR]" % requests.GREEN), ("Season 2", "10 episodes"),
            ("Season 3", "Airs " + words.day("2027-01-05", time.time())), ("Specials", "10 episodes")])
        self.assertEqual(entries[0][1].properties["SpecialSort"], "top")
        self.assertEqual([url for url, _, _ in entries[1:3]], [BASE + "?route=title&kind=show&tmdb=1396&season=1",
                                                               BASE + "?route=title&kind=show&tmdb=1396&season=2"])
        self.assertEqual(entries[3][1].video.values["setPlot"], entries[3][1].label2 + "[CR][CR]8 episodes[CR][CR]Details and pictures from TMDB.")
        self.assertIn(("content", "seasons"), xbmcplugin.calls)

    def test_a_show_in_a_library_opens_and_offers_more(self):
        self.answers["GET /api/discover/show/1396"] = (200, dict(SHOW, status="partial", item_id=8))
        entries = self.run_plugin("route=seasons&tmdb=1396")
        self.assertEqual(self.labels(entries)[:2], ["Open", "Request more seasons"])
        self.assertEqual(entries[1][0], BASE + "?route=title&kind=show&tmdb=1396&more=1")


class RequestsTest(Case):
    def test_your_requests(self):
        downloading = request(43, kind="show", tmdb_id=1396, title="Driftline", seasons=[1, 2], status="approved", stage="downloading",
                              actions=[], download={"progress": 0.5, "state": "downloading", "eta": None})
        self.answers[ACTIVE] = (200, {"total": 2, "requests": [request(unseen=True), downloading], "people": []})
        self.answers["POST /api/requests/seen"] = (200, {})
        entries = self.run_plugin("route=requests")
        self.assertEqual(self.sent("GET", "/api/requests")[0][1], "/api/requests?offset=0&limit=50")
        movie, show = entries
        self.assertEqual((movie[0], movie[2]), (BASE + "?route=act&id=42&do=choose", False))
        self.assertEqual(movie[1].label2, "[COLOR %s]Waiting for approval[/COLOR]" % requests.AMBER)
        self.assertEqual(movie[1].menu, [("Withdraw", "RunPlugin(%s?route=act&id=42&do=withdraw)" % BASE)])
        self.assertEqual(show[1].video.values["setPlot"], "[COLOR %s]Downloading 50%%[/COLOR][CR]Seasons 1–2[CR]Asked for 3 days ago"
                         % requests.BLUE)
        self.assertEqual(show[1].menu, [("Seasons", "Container.Update(%s?route=seasons&tmdb=1396)" % BASE)])
        self.assertEqual(self.sent("POST"), [("POST", "/api/requests/seen", {})], "its news is seen")
        self.assertEqual(kodi.get_property(requests.LIVE), BASE + "?route=requests", "asked for again while it downloads")

    def test_requests_for_managers(self):
        self.summary(MANAGER)
        self.answers[ACTIVE] = (200, {"total": 1, "requests": [request(mine=False, actions=["approve", "decline", "handled", "delete"])],
                                      "people": [{"id": 1, "name": "Ann"}]})
        entries = self.run_plugin("route=requests")
        self.assertEqual(self.sent("GET", "/api/requests")[0][1], "/api/requests?offset=0&limit=50&attention=1&order=oldest")
        self.assertEqual(self.labels(entries), ["Needs attention", "Movies and shows", "Everyone", "Search", "Saltwind"])
        self.assertEqual(entries[0][0], BASE + "?route=request_filter&pick=view")
        self.assertEqual([label for label, _ in entries[4][1].menu], ["Approve", "Decline", "Decline with a reason…",
                                                                      "Mark as added by hand", "Delete"])
        self.assertIn("Ann asked for it 3 days ago", entries[4][1].video.values["setPlot"])
        self.assertEqual(self.sent("POST"), [])
        self.assertEqual(kodi.get_property(requests.LIVE), "")

    def test_a_managers_news_is_seen_in_yours_alone(self):
        self.summary(dict(MANAGER, unseen=1))
        ready = request(stage="available", status="available", unseen=True, actions=["delete"])
        self.answers[ACTIVE] = (200, {"total": 1, "requests": [ready], "people": [{"id": 1, "name": "Ann"}]})
        self.answers["POST /api/requests/seen"] = (200, {})
        for view in ("", "&view=available", "&view=all"):
            self.run_plugin("route=requests" + view)
        self.assertEqual(self.sent("POST"), [], "everyone's requests, the account's among them")
        del xbmcplugin.calls[:], self.server.requests[:]
        entries = self.run_plugin("route=requests&view=mine")
        self.assertEqual(self.labels(entries)[0], "Yours")
        self.assertEqual(self.sent("GET", "/api/requests")[0][1], "/api/requests?offset=0&limit=50&mine=1")
        self.assertEqual(self.sent("POST"), [("POST", "/api/requests/seen", {})])
        with Dialogs(-1) as dialogs:
            self.run_plugin("route=request_filter&pick=view&view=mine", handle="-1")
        self.assertEqual((dialogs.shown[0][2][-1], dialogs.shown[0][3]), ("Yours", 7), "last, and preselected while shown")

    def test_empty_lists(self):
        self.assertEqual(self.labels(self.run_plugin("route=requests")), ["You haven't asked for anything yet."])
        self.summary(MANAGER)
        for query, said in [("route=requests", "Nothing needs attention."), ("route=requests&view=pending", "Nothing matches these filters."),
                            ("route=requests&kind=show", "Nothing matches these filters."),
                            ("route=requests&view=mine", "You haven't asked for anything yet.")]:
            del xbmcplugin.calls[:]
            self.assertEqual(self.labels(self.run_plugin(query))[-1], said, query)

    def test_a_manager_signed_in_before_approvals(self):
        self.summary(dict(ON, sign_in_again="approvals"))
        self.assertEqual(self.labels(self.run_plugin("route=requests"))[0], "Sign in again to approve and decline here.")

    def test_filters(self):
        self.summary(MANAGER)
        with Dialogs(2):
            self.run_plugin("route=request_filter&pick=kind&view=pending", handle="-1")
        self.assertEqual(xbmc.builtins, ["Container.Update(%s?route=requests&view=pending&kind=show,replace)" % BASE])
        self.answers[ACTIVE] = (200, {"total": 1, "requests": [], "people": [{"id": 1, "name": "Ann"}, {"id": 2, "name": "Bo"}]})
        with Dialogs(1) as dialogs:
            self.run_plugin("route=request_filter&pick=user_id", handle="-1")
        self.assertEqual(dialogs.shown, [("select", "Filter", ["Everyone", "Ann", "Bo"], 0)])
        self.assertEqual(self.sent("GET", "/api/requests")[0][1], "/api/requests?limit=1&attention=1&order=oldest")
        self.assertEqual(xbmc.builtins[-1], "Container.Update(%s?route=requests&user_id=1,replace)" % BASE)

    def test_the_search_filter(self):
        self.summary(MANAGER)
        self.addCleanup(lambda: (xbmc.Keyboard.typed.clear(), xbmc.Keyboard.shown.clear()))
        xbmc.Keyboard.typed[:] = [" saltwind ", None, ""]
        filtered = "Container.Update(%s?route=requests&view=pending&q=saltwind,replace)" % BASE
        self.run_plugin("route=request_filter&pick=q&view=pending", handle="-1")
        self.assertEqual(xbmc.builtins, [filtered])
        self.run_plugin("route=request_filter&pick=q&view=pending&q=saltwind", handle="-1")
        self.assertEqual(xbmc.builtins, [filtered], "cancelled: the list stays as it is")
        self.run_plugin("route=request_filter&pick=q&view=pending&q=saltwind", handle="-1")
        self.assertEqual(xbmc.builtins[-1], "Container.Update(%s?route=requests&view=pending,replace)" % BASE, "emptied: none")
        self.assertEqual(xbmc.Keyboard.shown, [("Search", ""), ("Search", "saltwind"), ("Search", "saltwind")])


class ActTest(Case):
    def act(self, do, *answers, found=None):
        self.summary(MANAGER)
        self.answers["GET /api/requests/42"] = (200, found or request(mine=False, actions=["approve", "decline", "handled", "delete"]))
        with Dialogs(*answers) as dialogs:
            self.run_plugin("route=act&id=42&do=" + do, handle="-1")
        return dialogs.shown

    def test_decline_with_a_reason(self):
        self.answers["POST /api/requests/42/decline"] = (200, {})
        shown = self.act("decline_reason", "  Not on this server ")
        self.assertEqual(shown, [("input", "Decline with a reason", "")])
        self.assertEqual(self.sent("POST"), [("POST", "/api/requests/42/decline", {"reason": "Not on this server"})])
        self.assertEqual(xbmc.builtins[-1], "Container.Refresh")

    def test_ok_on_a_request_offers_what_may_be_done(self):
        self.answers["POST /api/requests/42/approve"] = (200, {})
        shown = self.act("choose", 0)
        self.assertEqual(shown, [("select", "Saltwind · Waiting for approval", ["Approve", "Decline", "Decline with a reason…",
                                                                            "Mark as added by hand", "Delete"], -1)])
        self.assertEqual(self.sent("POST"), [("POST", "/api/requests/42/approve", {})])

    def test_what_cant_be_undone_is_asked_first(self):
        self.assertEqual(self.act("delete", False)[0][2], "Delete this request? Radarr and Sonarr keep what they have.")
        self.answers["DELETE /api/requests/42"] = (200, {})
        shown = self.act("withdraw", True, found=request())
        self.assertEqual(shown, [("yesno", "Withdraw", "Withdraw your request for “Saltwind”?", "Withdraw")])
        self.assertEqual(self.sent("DELETE"), [("DELETE", "/api/requests/42", None)])

    def test_a_race_shows_the_request_as_it_is(self):
        self.answers["POST /api/requests/42/approve"] = (409, {"code": "conflict", "message": "it was declined meanwhile",
                                                               "request": request(stage="declined")})
        self.act("approve")
        self.assertEqual(xbmcgui.notified, [("Pley", "It was declined meanwhile.")])
        self.assertEqual(xbmc.builtins[-1], "Container.Refresh")

    def test_a_request_gone_or_changed(self):
        self.answers["GET /api/requests/42"] = (404, {"code": "not_found", "message": "no such request"})
        with Dialogs():
            self.run_plugin("route=act&id=42&do=approve", handle="-1")
        self.assertEqual(xbmc.builtins, ["Container.Refresh"])
        self.act("approve", found=request(actions=[]))
        self.assertEqual(self.sent("POST"), [], "no longer offered")
        self.assertEqual(self.act("choose", found=request(actions=[])), [
            ("ok", "Saltwind", "[COLOR %s]Waiting for approval[/COLOR][CR]Asked for 3 days ago" % requests.AMBER)])

    def test_done_from_pleys_window(self):
        # Asked over pley's window (window.run_plugin): the window shows it, not
        # the list of add-ons under it.
        xbmc.info_labels["Container.FolderPath"] = "addons://sources/video/"
        kodi.set_property(kodi.WINDOW_OPEN, "1")
        self.addCleanup(kodi.clear_property, kodi.WINDOW_OPEN)
        self.answers["POST /api/requests/42/approve"] = (200, {})
        self.act("approve")
        self.assertEqual(xbmc.builtins[-2:], ["NotifyAll(plugin.video.pley,pley.asking)", "NotifyAll(plugin.video.pley,changed)"])


class SearchTest(Case):
    def setUp(self):
        super().setUp()
        self.summary(ON)
        self.answers["GET /api/search"] = (200, {"items": [{"id": 5, "kind": "movie", "title": "Saltwind", "year": 1995}]})
        self.answers["GET /api/discover/search"] = (200, {"page": 1, "total_pages": 1, "results": [title(1), title(2)]})

    def test_titles_from_tmdb_after_the_libraries(self):
        self.waits["GET /api/search"] = 0.3
        entries = self.run_plugin("route=search&q=saltwind")
        self.assertEqual(self.labels(entries), ["Saltwind", "Discover", "Title 1", "Title 2", "See all"])
        self.assertEqual(entries[-1][0], BASE + "?route=discover_search&q=saltwind")
        self.assertEqual(self.sent("GET", "/api/discover/search")[0][1], "/api/discover/search?q=saltwind&missing=1")

    def test_titles_from_tmdb_once_they_come(self):
        self.waits["GET /api/discover/search"] = 0.5
        self.answers["GET /api/search"] = (200, {"items": []})
        xbmc.info_labels["Container.FolderPath"] = BASE + "?route=search&q=saltwind"
        entries = self.run_plugin("route=search&q=saltwind")
        self.assertEqual(self.labels(entries), ["Nothing in your libraries matches", "Discover", "See all"])
        self.assertEqual(entries[1][1].label2, "Searching…")
        self.assertEqual(xbmc.builtins, ["Container.Refresh"], "shown again once they came")
        del xbmcplugin.calls[:]
        self.assertEqual(self.labels(self.run_plugin("route=search&q=saltwind"))[1:4], ["Discover", "Title 1", "Title 2"])
        self.assertEqual(len(self.sent("GET", "/api/discover/search")), 1, "kept")

    def test_tmdb_slower_than_the_wait_is_left_to_on_tmdb(self):
        self.addCleanup(setattr, asking, "FOUND_WAIT", asking.FOUND_WAIT)
        asking.FOUND_WAIT = 0.3
        self.waits["GET /api/discover/search"] = 1.5
        self.answers["GET /api/search"] = (200, {"items": []})
        xbmc.info_labels["Container.FolderPath"] = BASE + "?route=search&q=saltwind"
        started = time.monotonic()
        entries = self.run_plugin("route=search&q=saltwind")
        self.assertLess(time.monotonic() - started, 1.2, "Kodi's Python isn't held until TMDB answers")
        self.assertEqual((entries[1][0], entries[1][1].label2), (BASE + "?route=discover_search&q=saltwind", "Searching…"))
        self.assertEqual(xbmc.builtins, [])
        self.assertEqual(kodi.get_property(asking.FOUND), "", "nothing kept: the next search asks again")

    def test_nothing_in_the_libraries_nor_on_tmdb(self):
        self.answers["GET /api/search"] = (200, {"items": []})
        self.answers["GET /api/discover/search"] = (200, {"page": 1, "total_pages": 0, "results": []})
        self.waits["GET /api/discover/search"] = 0.3
        xbmc.info_labels["Container.FolderPath"] = BASE + "?route=search&q=zzz"
        self.assertEqual(self.labels(self.run_plugin("route=search&q=zzz")), ["Nothing in your libraries matches", "Discover", "See all"])
        self.assertEqual(xbmc.builtins, ["Container.Refresh"], "shown again once TMDB answered")
        del xbmcplugin.calls[:]
        entries = self.run_plugin("route=search&q=zzz")
        self.assertEqual(self.labels(entries), ["Nothing in your libraries matches", "Nothing found for “zzz”"])
        self.assertEqual(entries[1][0], BASE + "?route=search", "OK searches again")

    def test_tmdb_refusing_says_why_in_its_place(self):
        self.waits["GET /api/search"] = 0.3
        self.answers["GET /api/discover/search"] = (503, {"code": "tmdb_down", "message": "TMDB timed out", "setup": True})
        entries = self.run_plugin("route=search&q=saltwind")
        self.assertEqual(self.labels(entries), ["Saltwind", "TMDB isn't answering", "Try again"])
        self.assertEqual(entries[-1][0], BASE + "?route=retry")
        self.assertEqual(entries[-1][1].properties["SpecialSort"], "bottom")
        del xbmcplugin.calls[:]
        self.answers["GET /api/discover/search"] = (503, {"code": "tmdb_key", "message": "401", "setup": True, "retry": False})
        entries = self.run_plugin("route=search&q=saltwind")
        self.assertEqual(self.labels(entries), ["Saltwind", "TMDB refused this server's key"], "trying again won't help")
        self.assertEqual(entries[1][1].video.values["setPlot"], "An administrator needs to check it.")
        del xbmcplugin.calls[:]
        self.answers["GET /api/discover/search"] = (403, {"code": "no_access", "message": "no"})
        self.assertEqual(self.labels(self.run_plugin("route=search&q=saltwind")), ["Saltwind", "Requests aren't open to this account"])

    def test_a_refusal_after_the_listing_shows_once_it_came(self):
        self.waits["GET /api/discover/search"] = 0.5
        self.answers["GET /api/search"] = (200, {"items": []})
        self.answers["GET /api/discover/search"] = (503, {"code": "tmdb_down", "message": "TMDB timed out", "setup": True})
        xbmc.info_labels["Container.FolderPath"] = BASE + "?route=search&q=saltwind"
        self.assertEqual(self.labels(self.run_plugin("route=search&q=saltwind")), ["Nothing in your libraries matches", "Discover", "See all"])
        self.assertEqual(xbmc.builtins, ["Container.Refresh"], "shown again once it came")
        del xbmcplugin.calls[:]
        self.assertEqual(self.labels(self.run_plugin("route=search&q=saltwind")), ["Nothing in your libraries matches", "TMDB isn't answering", "Try again"])
        self.assertEqual(len(self.sent("GET", "/api/discover/search")), 1, "kept for the listing shown again")
        # Try again: TMDB is asked again.
        del xbmcplugin.calls[:]
        self.answers["GET /api/discover/search"] = (200, {"page": 1, "total_pages": 1, "results": [title(1)]})
        self.waits["GET /api/discover/search"] = 0
        self.waits["GET /api/search"] = 0.3
        self.assertEqual(self.labels(self.run_plugin("route=search&q=saltwind")), ["Nothing in your libraries matches", "Discover", "Title 1", "See all"])
        self.assertEqual(len(self.sent("GET", "/api/discover/search")), 2)

    def test_words_cut_to_what_the_server_takes(self):
        self.waits["GET /api/search"] = 0.3
        typed = "  " + "x" * 250
        entries = self.run_plugin("route=search&q=" + urllib.parse.quote(typed))
        # Every title and channel found, up to 200 of each, the words as typed.
        self.assertEqual(self.sent("GET", "/api/search")[0][1], "/api/search?" + urllib.parse.urlencode({"q": typed, "limit": 200, "summaries": 1}))
        self.assertEqual(self.sent("GET", "/api/discover/search")[0][1], "/api/discover/search?q=%s&missing=1" % ("x" * 200))
        self.assertEqual(entries[-1][0], BASE + "?route=discover_search&q=" + "x" * 200)
        # Nothing but spaces: TMDB isn't asked.
        del self.server.requests[:]
        self.run_plugin("route=search&q=%20%20")
        self.assertEqual(self.sent("GET", "/api/discover/search"), [])

    def test_a_title_asked_for_from_search_leaves_tmdb_there(self):
        # The listing is shown again at once, before the service has read
        # the summary anew.
        self.answers["GET /api/discover/movie/1"] = (200, detail(title(1)))
        self.answers["POST /api/requests"] = (201, {"status": "pending", "title": "Title 1"})
        with Dialogs(True):
            self.run_plugin("route=title&kind=movie&tmdb=1", handle="-1")
        self.assertEqual(xbmc.builtins[-1], "Container.Refresh")
        del xbmcplugin.calls[:]
        self.assertIn("Discover", self.labels(self.run_plugin("route=search&q=saltwind")))

    def test_no_tmdb_where_this_sign_in_cant_ask(self):
        self.summary(dict(ON, access="off"))
        self.answers["GET /api/search"] = (200, {"items": []})
        self.assertEqual(self.run_plugin("route=search&q=saltwind"), [])
        self.assertEqual(xbmcgui.notified, [("Pley", "Nothing found")])
        self.assertEqual(self.sent("GET", "/api/discover/search"), [])


class ProfileTest(Case):
    def test_another_profile_keeps_nothing_of_the_one_before(self):
        self.summary(MANAGER)
        self.addCleanup(kodi.clear_property, profiles.GATE)
        self.answers["GET /api/oauth/profiles"] = (200, [{"id": 1, "name": "Ann", "current": True, "needs": "none"},
                                                         {"id": 2, "name": "Bo", "current": False, "needs": "none"}])
        self.answers["POST /switch-profile"] = (200, {})
        with Dialogs(1):
            self.run_plugin("route=profiles", handle="-1")
        self.assertEqual(self.sent("POST", "/switch-profile")[0][2], {"profile": 2, "pin": None})
        self.assertEqual(kodi.get_property(requests.SUMMARY), "", "Bo's is read")
        self.assertEqual(xbmc.builtins, ["NotifyAll(plugin.video.pley,pley.asking)", "Container.Refresh"])


class MoreSeasonsTest(Case):
    SHOW = {"id": 8, "kind": "show", "title": "Low Orbit", "user_state": {}}

    def test_library_shows_offer_more_seasons(self):
        lister = plugin.Plugin([BASE, "3", "?route=library&id=1"])
        self.assertEqual(lister.menu(self.SHOW)[-1][0], "Add to My List", "the service hasn't read the summary yet")
        self.summary(ON)
        lister = plugin.Plugin([BASE, "3", "?route=library&id=1"])
        self.assertEqual(lister.menu(self.SHOW)[-1],
                         ("Request more seasons", "RunPlugin(%s?route=more_seasons&id=8&title=Low+Orbit)" % BASE))
        self.assertNotIn("Request more seasons", [label for label, _ in lister.menu(dict(self.SHOW, kind="movie"))])
        self.assertEqual(self.sent(), [], "listings ask nothing more")

    def test_nothing_more_to_ask_for(self):
        self.answers["GET /api/items/8/request"] = (200, {"kind": "show", "tmdb_id": 1396, "status": "library", "request": None, "askable": []})
        self.answers["GET /api/items/8"] = (200, {"id": 8, "title": "Low Orbit"})
        with Dialogs() as dialogs:
            self.run_plugin("route=more_seasons&id=8&title=Low+Orbit", handle="-1")
        self.assertEqual(dialogs.shown, [("ok", "Request more seasons", "There's nothing more of “Low Orbit” to ask for.")])

    def test_the_picker_of_a_library_show(self):
        self.answers["GET /api/items/8/request"] = (200, {"kind": "show", "tmdb_id": 1396, "status": "partial", "request": None, "askable": [2]})
        self.answers["GET /api/discover/show/1396"] = (200, dict(SHOW, status="partial", item_id=8))
        with Dialogs(None) as dialogs:
            self.run_plugin("route=more_seasons&id=8&title=Low+Orbit", handle="-1")
        self.assertEqual([row[0] for row in dialogs.shown[0][2]], ["Season 2", "Season 3", "Specials"])
        self.assertEqual(self.sent("POST"), [])

    def test_a_show_tmdb_doesnt_know(self):
        self.answers["GET /api/items/8/request"] = (404, {"code": "not_found", "message": "no TMDB match"})
        with Dialogs():
            self.run_plugin("route=more_seasons&id=8&title=Low+Orbit", handle="-1")
        self.assertEqual(xbmcgui.notified, [("Pley", "Nothing on TMDB for “Low Orbit”")])


if __name__ == "__main__":
    unittest.main()
