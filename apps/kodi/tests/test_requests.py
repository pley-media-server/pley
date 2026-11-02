import json
import unittest

import support  # noqa: F401
import xbmcaddon
from pley import api, kodi, requests

T = kodi.text
# 2026-11-01 12:00 UTC: days are whole in any time zone near it.
NOW = 1793534400
DAY = 86400


def season(number, state, **more):
    return dict({"number": number, "name": "Specials" if number == 0 else "Season %d" % number, "state": state,
                 "episodes": 10, "aired": 10, "in_library": 0, "declined": False, "request_id": None}, **more)


def show(*seasons, **more):
    return dict({"kind": "show", "tmdb_id": 1396, "title": "Breaking Bad", "status": "none", "item_id": None,
                 "seasons": list(seasons), "requests": [], "watching": False, "seasons_left": None,
                 "movies_left": None}, **more)


def request(**more):
    return dict({"id": 42, "kind": "movie", "tmdb_id": 949, "title": "Heat", "stage": "pending", "status": "pending",
                 "mine": True, "watching": False, "actions": [], "requested_by": {"id": 1, "name": "Ann"},
                 "created_at": NOW - 3 * DAY, "available_seasons": [], "item_id": None}, **more)


ON = {"enabled": True, "access": "ask", "manage": False, "pending": 0, "failed": 0, "unseen": 0, "own": 0,
      "sign_in_again": None, "movies_limit": 3, "seasons_limit": 10}


class StringsTest(unittest.TestCase):
    def test_every_name_has_its_words(self):
        for name, number in kodi.TEXT.items():
            self.assertTrue(xbmcaddon.localized(number), "no string #%d for %s" % (number, name))
        self.assertEqual(len(set(kodi.TEXT.values())), len(kodi.TEXT))


class SummaryTest(unittest.TestCase):
    def client(self, error=None, answer=None):
        class Client:
            def get(self, path, **query):
                assert path == "/api/requests/summary"
                if error:
                    raise error
                return answer

        return Client()

    def test_servers_without_requests_for_apps_have_them_off(self):
        self.assertIsNone(requests.summary(self.client(api.ApiError(404, "not_found", "no"))))
        # A build from before apps could ask.
        self.assertIsNone(requests.summary(self.client(api.ApiError(403, "insufficient_scope", "apps can't"))))
        with self.assertRaises(api.ApiError):
            requests.summary(self.client(api.ApiError(0, "unreachable", "down")))
        self.assertEqual(requests.summary(self.client(answer=ON)), ON)

    def test_what_shows_where(self):
        self.assertTrue(requests.discovers(ON) and requests.may_ask(ON) and requests.lists(ON))
        self.assertFalse(requests.discovers(None) or requests.lists(None) or requests.manages(None))
        self.assertFalse(requests.discovers(dict(ON, enabled=False)))
        # Off for this account: its own requests still show.
        off = dict(ON, access="off")
        self.assertFalse(requests.discovers(off))
        self.assertFalse(requests.lists(off))
        self.assertTrue(requests.lists(dict(off, own=2)))
        # Signed in before apps could ask: Discover says so, and nothing else shows.
        before = dict(ON, sign_in_again="requests")
        self.assertTrue(requests.discovers(before))
        self.assertFalse(requests.may_ask(before) or requests.lists(before))

    def test_the_number_on_the_requests_entry(self):
        self.assertEqual(requests.count(dict(ON, unseen=2, pending=5)), (2, False))
        manager = dict(ON, manage=True, pending=3, failed=1, unseen=2)
        self.assertEqual(requests.count(manager), (6, True))
        self.assertEqual(requests.count(dict(manager, failed=0)), (5, False))
        self.assertEqual(requests.count(None), (0, False))
        # Kodi's lists and pley's window name the entry alike.
        self.assertEqual(requests.entry_label(ON, T), "Your requests")
        self.assertEqual(requests.entry_label(dict(ON, unseen=1200), T), "Your requests (1,200)")
        self.assertEqual(requests.entry_label(manager, T), "Requests [COLOR %s](6)[/COLOR]" % requests.RED)

    def test_listings_that_show_requests(self):
        counted, listed = requests.COUNTED, requests.LISTED
        self.assertTrue(requests.showing("plugin://plugin.video.pley/", counted))
        self.assertTrue(requests.showing("plugin://plugin.video.pley/?route=discover", counted))
        self.assertFalse(requests.showing("plugin://plugin.video.pley/?route=requests&view=all", counted))
        self.assertTrue(requests.showing("plugin://plugin.video.pley/?route=requests&view=all", listed))
        self.assertTrue(requests.showing("plugin://plugin.video.pley/?route=seasons&tmdb=1396", listed))
        for path in ["plugin://plugin.video.pley/?route=discover_list&list=trending", "plugin://plugin.video.pley/?route=library&id=1",
                     "plugin://plugin.video.other/"]:
            self.assertFalse(requests.showing(path, counted + listed), path)

    def test_words_for_tmdb_are_what_the_server_takes(self):
        self.assertEqual(requests.search_text("  dune  "), "dune")
        self.assertEqual(requests.search_text("   "), "")
        # 200 characters at most, counted as the server counts them.
        self.assertEqual(requests.search_text("é" * 250), "é" * 200)
        self.assertEqual(requests.search_text("x" * 199 + " dune"), "x" * 199)


class TitleTest(unittest.TestCase):
    def test_badges(self):
        self.assertEqual(requests.badge({"status": "library"}, T), ("In library", requests.GREEN))
        self.assertEqual(requests.badge({"status": "partial", "item_id": 7}, T), ("Partly in library", requests.GREEN))
        self.assertEqual(requests.badge({"status": "partial", "item_id": None}, T), ("Requested", requests.BLUE))
        self.assertEqual(requests.badge({"status": "requested"}, T), ("Requested", requests.BLUE))
        self.assertEqual(requests.badge({"status": "declined"}, T), ("Declined", requests.RED))
        self.assertEqual(requests.badge({"status": "restricted"}, T), ("", None))
        self.assertEqual(requests.badge({"status": "none"}, T), ("", None))

    def test_the_main_action_is_the_first_that_applies(self):
        movie = {"kind": "movie", "status": "none", "item_id": None, "requests": [], "movies_left": 2}
        act = requests.main_action
        self.assertEqual(act(movie, dict(ON, sign_in_again="requests")), "sign_in")
        self.assertEqual(act(dict(movie, status="restricted"), ON), "restricted")
        self.assertEqual(act(dict(movie, status="library", item_id=7), ON), "open")
        self.assertEqual(act(dict(movie, status="library"), ON), "in_library")
        self.assertEqual(act(dict(movie, status="declined"), ON), "declined")
        self.assertEqual(act(dict(movie, status="requested", requests=[request()]), ON), "requested")
        self.assertEqual(act(dict(movie, status="requested", watching=True), ON), "following")
        self.assertEqual(act(dict(movie, status="requested"), ON), "tell_me_too")
        self.assertEqual(act(show(status="partial", item_id=7), ON), "open")
        self.assertEqual(act(show(status="partial"), ON), "more_seasons")
        self.assertEqual(act(movie, ON), "request")
        self.assertEqual(act(movie, dict(ON, access="off")), "no_access")
        self.assertEqual(act(dict(movie, status="requested"), dict(ON, access="off")), "no_access")
        self.assertEqual(act(dict(movie, movies_left=0), ON), "used_up")
        self.assertEqual(act(show(seasons_left=0), ON), "used_up")
        # Following costs nothing.
        self.assertEqual(act(dict(movie, status="requested", movies_left=0), ON), "tell_me_too")

    def test_a_declined_title_says_why_and_until_when(self):
        detail = {"declined": {"reason": "We have it on disc", "at": NOW, "again_at": NOW + 12 * DAY}}
        self.assertEqual(requests.declined_line(detail, T, NOW), "Declined: “We have it on disc”. You can ask again in 12 days.")
        detail["declined"]["reason"] = None
        self.assertEqual(requests.declined_line(detail, T, NOW), "Declined. You can ask again in 12 days.")


class SeasonsTest(unittest.TestCase):
    def test_the_picker_ticks_the_missing_seasons_lowest_first(self):
        detail = show(
            season(0, "missing"),
            season(1, "library"),
            season(2, "partial", in_library=4),
            season(3, "missing"),
            season(4, "requested", request_id=9),
            season(5, "missing", declined=True),
            season(6, "missing"),
            season(7, "unaired", aired=0),
            seasons_left=2,
        )
        listed, ticked = requests.picker(detail)
        self.assertEqual([s["number"] for s in listed], [2, 3, 6, 7, 0])
        self.assertEqual(ticked, [0, 1])
        listed, ticked = requests.picker(dict(detail, seasons_left=None))
        self.assertEqual([listed[i]["number"] for i in ticked], [2, 3, 6])
        self.assertEqual(requests.picker(dict(detail, seasons_left=0))[1], [])
        # A season picked on its own.
        listed, ticked = requests.picker(detail, only=7)
        self.assertEqual([listed[i]["number"] for i in ticked], [7])
        # Nothing may be asked of a title the account is kept from.
        self.assertEqual(requests.picker(dict(detail, status="restricted"))[0], [])

    def test_more_seasons_needs_an_aired_one(self):
        self.assertTrue(requests.more_seasons(show(season(1, "library"), season(2, "partial"))))
        self.assertFalse(requests.more_seasons(show(season(0, "missing"), season(1, "library"), season(2, "unaired"))))
        self.assertFalse(requests.more_seasons(show(season(1, "missing", declined=True))))

    def test_each_season_says_its_state(self):
        asked = request(id=9, kind="show", stage="downloading", status="approved", download={"progress": 0.42, "state": "downloading", "eta": None})
        detail = show(requests=[asked])

        def state(s):
            return requests.season_state(s, detail, T, False, NOW)[0]

        self.assertEqual(state(season(1, "library")), "In library")
        self.assertEqual(state(season(2, "requested", request_id=9)), "Downloading 42%")
        self.assertEqual(state(season(3, "requested", request_id=10)), "Requested")
        self.assertEqual(state(season(4, "missing", declined=True)), "Declined")
        self.assertEqual(state(season(5, "partial", in_library=4)), "4 of 10 episodes in library")
        self.assertEqual(state(season(6, "unaired", air_date="2026-11-04")), "Airs Nov 4")
        self.assertEqual(state(season(7, "unaired", air_date=None)), "Not out yet")
        self.assertEqual(state(season(8, "missing", episodes=1)), "1 episode")


class StageTest(unittest.TestCase):
    def pill(self, manage=False, **more):
        return requests.stage(request(**more), T, manage, NOW)[0]

    def test_pills(self):
        self.assertEqual(requests.stage(request(), T, False, NOW), ("Waiting for approval", requests.AMBER))
        self.assertEqual(self.pill(stage="approved"), "Approved")
        self.assertEqual(self.pill(stage="waiting", stage_at=NOW + 3 * DAY), "Out Nov 4")
        self.assertEqual(self.pill(stage="waiting", kind="show", stage_at=NOW + 3 * DAY), "Airs Nov 4")
        self.assertEqual(self.pill(stage="waiting", kind="show", stage_at=NOW + 5 * 3600), "Airs in 5 hours")
        self.assertEqual(self.pill(stage="waiting", stage_at=None), "Not out yet")
        self.assertEqual(self.pill(stage="searching"), "Searching")
        download = {"progress": 0.42, "state": "downloading", "eta": NOW + 20 * 60, "episodes": 1}
        self.assertEqual(self.pill(stage="downloading", download=download), "Downloading 42% · in 20 min")
        self.assertEqual(self.pill(stage="downloading", download=dict(download, state="queued")), "Waiting to download")
        self.assertEqual(self.pill(stage="downloading", download=dict(download, state="paused")), "Download paused")
        self.assertEqual(self.pill(stage="downloading", download=dict(download, state="warning")), "The download has a problem")
        self.assertEqual(self.pill(stage="importing"), "Adding to the library")
        self.assertEqual(self.pill(stage="partial", kind="show", available_seasons=[1, 2]), "Seasons 1–2 ready")
        self.assertEqual(
            self.pill(stage="partial", kind="show", available_seasons=[1], download=dict(download, eta=None)),
            "Season 1 ready · Downloading 42%",
        )
        self.assertEqual(requests.stage(request(stage="available"), T, False, NOW), ("Ready to watch", requests.GREEN))
        self.assertEqual(requests.stage(request(stage="gone"), T, False, NOW), ("No longer in a library", requests.GRAY))
        self.assertEqual(requests.stage(request(stage="declined"), T, False, NOW), ("Declined", requests.RED))
        self.assertEqual(self.pill(stage="failed", reason="Radarr isn't answering"), "Not sent yet")
        self.assertEqual(self.pill(True, stage="failed", reason="Radarr isn't answering"), "Couldn't send: Radarr isn't answering")

    def test_what_a_request_says_under_its_pill(self):
        def lines(manage=False, **more):
            return requests.details(request(**more), T, manage, NOW)

        self.assertEqual(lines(stage="failed"), ["It couldn't be sent to download yet: an administrator will look at it."])
        self.assertEqual(lines(stage="failed", retry_at=NOW + 5 * 60), ["Trying again in 5 min."])
        self.assertEqual(lines(True, stage="failed", note="Added by hand"), ["Added by hand"])
        self.assertEqual(lines(stage="declined", reason="We have it on disc"), ["“We have it on disc”"])
        self.assertEqual(lines(stage="available"), ["Ready, in a library you can't open: ask an administrator"])
        self.assertEqual(lines(stage="available", item_id=7, note="for managers"), [])
        self.assertEqual(requests.asked_line(request(), T, False, NOW), "Asked for 3 days ago")
        self.assertEqual(requests.asked_line(request(mine=False), T, True, NOW), "Ann asked for it 3 days ago")
        self.assertEqual(requests.asked_line(request(requested_by=None), T, False, NOW), "Asked for by someone else")

    def test_actions_in_the_servers_order(self):
        found = requests.actions(request(actions=["approve", "decline", "handled", "delete"]), T)
        self.assertEqual(
            [label for _, label in found],
            ["Approve", "Decline", "Decline with a reason…", "Mark as added by hand", "Delete"],
        )
        self.assertEqual([a for a, _ in found][2], "decline_reason")
        self.assertEqual(requests.actions(request(actions=["withdraw"]), T), [("withdraw", "Withdraw")])
        self.assertEqual(requests.actions(request(stage="declined", actions=["withdraw"]), T), [("withdraw", "Remove")])
        self.assertEqual(
            requests.actions(request(actions=["unwatch", "retry", "ask_again", "something_new"]), T),
            [("unwatch", "Stop following"), ("retry", "Try again"), ("ask_again", "Request again")],
        )

    def test_lists_with_downloads_are_moving(self):
        self.assertTrue(requests.moving([request(stage="downloading")]))
        self.assertTrue(requests.moving([request(stage="partial", download={"progress": 0.1})]))
        self.assertFalse(requests.moving([request(stage="partial"), request(stage="searching")]))


class AskingTest(unittest.TestCase):
    def client(self, status, answer):
        sent = []

        class Client:
            def request(self, method, path, json_body=None, **kwargs):
                sent.append((method, path, json_body))
                return api.Response(status, {}, json.dumps(answer).encode("utf-8"))

        return Client(), sent

    def test_asking_and_following(self):
        client, sent = self.client(201, request(status="approved"))
        found, following = requests.ask(client, "show", 1396, [3, 1])
        self.assertEqual(sent, [("POST", "/api/requests", {"kind": "show", "tmdb_id": 1396, "seasons": [1, 3]})])
        self.assertFalse(following)
        self.assertEqual(requests.asked(found, following, T), "“Heat” is approved and on its way.")
        client, sent = self.client(200, request(watching=True, mine=False))
        found, following = requests.ask(client, "movie", 949)
        self.assertEqual(sent[0][2], {"kind": "movie", "tmdb_id": 949})
        self.assertTrue(following)
        self.assertEqual(requests.asked(found, following, T), "You'll hear when “Heat” is ready.")
        self.assertEqual(requests.asked(request(), False, T), "Asked for “Heat”: an administrator decides.")
        self.assertEqual(requests.asked(request(status="failed"), False, T), "Asked for “Heat”, but it couldn't be sent to download yet.")
        client, _ = self.client(429, {"code": "too_many_requests", "message": "no", "retry_at": NOW + 3 * DAY})
        with self.assertRaises(api.ApiError) as refused:
            requests.ask(client, "movie", 949)
        self.assertEqual(refused.exception.body["retry_at"], NOW + 3 * DAY)

    def test_the_allowance_in_the_webs_words(self):
        def lines(**left):
            return requests.allowance(dict({"movies_left": None, "seasons_left": None}, **left), ON, T, NOW)

        self.assertEqual(lines(), [])
        self.assertEqual(
            lines(movies_left=2, seasons_left=5),
            ["You can ask for 2 more movies and 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days."],
        )
        self.assertEqual(lines(movies_left=1, seasons_left=None), ["You can ask for 1 more movie.", "Limits: 3 movies per 7 days."])
        self.assertEqual(
            lines(movies_left=0, movies_next_at=NOW + 3 * DAY, seasons_left=4)[:2],
            ["You've asked for all the movies you can for now: you can ask for more in 3 days.", "You can ask for 4 more seasons."],
        )
        self.assertEqual(
            lines(movies_left=0, movies_next_at=NOW + 3 * DAY, seasons_left=0, seasons_next_at=NOW + 5 * DAY)[0],
            "You've asked for all you can for now: more movies in 3 days, more seasons in 5 days.",
        )
        self.assertEqual(
            lines(movies_left=None, seasons_left=0)[0],
            "You've asked for all the seasons you can for now: you can ask for more later.",
        )

    def test_refusals(self):
        def said(status, code, manage=False, kind="movie", scope="requests", **body):
            error = api.ApiError(status, code, body.pop("message", "refused"), dict(body, code=code))
            return requests.problem(error, T, manage, ON, kind, NOW, scope)

        self.assertEqual(said(503, "unavailable", setup=False), ("Requests are off", "This server doesn't take requests for movies and shows.", False))
        self.assertEqual(said(503, "tmdb_key", setup=True, retry=False), ("TMDB refused this server's key", "An administrator needs to check it.", False))
        self.assertEqual(said(503, "tmdb_key", True, setup=True, retry=False)[1], "Check it on the web, in Settings → Server.")
        self.assertEqual(said(503, "tmdb_down", setup=True), ("TMDB isn't answering", "", True))
        # A server from before the codes said only `setup`.
        self.assertEqual(said(503, "unavailable", setup=True)[0], "TMDB isn't answering")
        self.assertEqual(said(403, "no_access"), ("Requests aren't open to this account", "An administrator can let it ask for movies and shows.", False))
        self.assertEqual(said(403, "insufficient_scope")[0], "Sign in again to ask for movies and shows")
        self.assertEqual(said(403, "insufficient_scope", scope="approvals")[0], "Sign in again to approve and decline here.")
        self.assertEqual(said(403, "restricted", message="rated R: above this account's age limit")[0], "Rated R: above this account's age limit.")
        self.assertEqual(said(409, "in_library", message="a library has this movie already")[0], "A library has this movie already.")
        self.assertEqual(said(409, "declined", retry_at=NOW + 12 * DAY)[0], "You can ask again in 12 days.")
        self.assertEqual(said(429, "too_many_requests", retry_at=NOW + 3 * DAY)[0], "You can ask for more in 3 days.")
        self.assertEqual(said(429, "too_many_requests", kind="show", retry_at=None)[0], "You can ask for at most 10 seasons per 7 days: pick fewer.")
        self.assertEqual(said(500, "internal", message="oops")[0], "The server answered: oops")
        self.assertEqual(
            requests.problem_text(said(503, "unavailable", setup=False)),
            "Requests are off. This server doesn't take requests for movies and shows.",
        )
        self.assertEqual(requests.problem_text(said(503, "tmdb_down", setup=True)), "TMDB isn't answering")


class NoticesTest(unittest.TestCase):
    def test_ready_to_watch(self):
        event = {"type": "request_available", "title": "Heat", "kind": "movie", "seasons": None, "complete": True}
        self.assertEqual(requests.ready(event, T), "“Heat” is ready to watch.")
        event = {"title": "Severance", "kind": "show", "seasons": [2], "complete": False}
        self.assertEqual(requests.ready(event, T), "Season 2 of “Severance” is ready to watch.")
        self.assertEqual(requests.ready(dict(event, seasons=[1, 2]), T), "Seasons 1–2 of “Severance” are ready to watch.")
        self.assertEqual(requests.ready(dict(event, complete=True), T), "“Severance” is ready to watch.")
        self.assertEqual(requests.news(3, T), "3 of your requests have news")

    def test_requests_ready_for_a_notice(self):
        self.assertTrue(requests.ready_now(request(stage="available", item_id=7)))
        self.assertFalse(requests.ready_now(request(stage="available")))
        self.assertFalse(requests.ready_now(request(stage="declined", item_id=7)))
        partial = requests.as_event(request(kind="show", stage="partial", available_seasons=[1], item_id=7))
        self.assertEqual(requests.ready(partial, T), "Season 1 of “Heat” is ready to watch.")


if __name__ == "__main__":
    unittest.main()
