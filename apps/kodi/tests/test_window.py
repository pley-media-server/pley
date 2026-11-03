import json
import os
import shutil
import tempfile
import unittest

import support  # noqa: F401
from pley import layout as L
from pley import window

BASE = "http://127.0.0.1:5000/s"
EPISODE = {
    "id": 31,
    "kind": "episode",
    "title": "Glasshouse",
    "index": 5,
    "parent_index": 2,
    "grandparent_title": "Driftline",
    "duration_ms": 56 * 60_000,
    "year": 2025,
    "summary": "Ida gets a lead.",
    "user_state": {"position_ms": 30 * 60_000, "watched": False},
}


class Control:
    def __init__(self):
        self.label = None
        self.visible = True
        self.position = None
        self.items = []

    def setLabel(self, text, font=None, textColor=None):
        self.label = text
        self.text_color = textColor

    def setColorDiffuse(self, color):
        self.diffuse = color

    def setVisible(self, shown):
        self.visible = shown

    def setPosition(self, x, y):
        self.position = (x, y)

    def setHeight(self, height):
        self.height = height

    def reset(self):
        self.items = []

    def addItems(self, items):
        self.items += items

    def getSelectedPosition(self):
        return getattr(self, "selected", 0)

    def selectItem(self, at):
        self.selected = at

    def getListItem(self, at):
        return self.items[at]

    def setImage(self, path, use_cache=True):
        self.image = path

    def setText(self, text):
        self.label = text

    def setEnabled(self, enabled):
        self.enabled = enabled


class FakeWindow(window.Window):
    """The window's drawing, on controls that only remember."""

    def __init__(self):
        super().__init__()
        self.session = window.Session.__new__(window.Session)
        self.session.base = BASE
        self.made = {}
        self.toasts = []
        self.focused = None
        self.opened = []

    def getControl(self, control_id):
        return self.made.setdefault(control_id, Control())

    def toast(self, text, seconds=3.5):
        self.toasts.append(text)

    def setFocusId(self, control_id):
        self.focused = control_id

    def getFocusId(self):
        return self.focused or 0

    def setProperty(self, name, value):
        pass

    def getProperty(self, name):
        return ""

    def open(self, screen):
        self.opened.append(screen)


ASKS = {"enabled": True, "access": "ask", "manage": False, "pending": 0, "failed": 0, "unseen": 0, "own": 0, "sign_in_again": None}


def keep_summary(test, s):
    """The requests' summary as the service keeps it (news.py), until the
    test ends."""
    window.kodi.set_property(window.requests.SUMMARY, json.dumps(s))
    test.addCleanup(window.kodi.clear_property, window.requests.SUMMARY)


def requests_off(test):
    keep_summary(test, {"enabled": False})


def at_once(work, then, failed=None):
    """Window.later, with no thread: the answer at once."""
    try:
        answer = work()
    except window.api.ApiError:
        return failed and failed()
    then(answer)


class Client:
    """The server, as far as these tests ask it: posts are noted."""

    def __init__(self, answers=None):
        self.answers = answers or {}
        self.posts = []

    def get(self, path, **query):
        return self.answers[path]

    def post(self, path, body=None):
        self.posts.append((path, body))
        return {"id": 77, "title": (body or {}).get("title")}


class Dialogs:
    """xbmcgui.Dialog, answering as the test says: `picks` for select,
    `typed` for input, `yes` for yesno; what select offered is kept in
    `shown`, what yesno asked in `asked`."""

    def __init__(self, picks=(), typed=(), yes=()):
        self.picks, self.typed, self.yes, self.shown, self.asked = list(picks), list(typed), list(yes), [], []

    def __enter__(self):
        self.real = window.xbmcgui.Dialog
        outer = self

        class Dialog:
            def select(self, heading, entries, preselect=-1, useDetails=False):
                outer.shown.append(([getattr(e, "label", e) for e in entries], preselect))
                return outer.picks.pop(0)

            def input(self, heading, *args, **kwargs):
                return outer.typed.pop(0)

            def numeric(self, kind, heading, defaultt="", hidden=False):
                outer.asked.append((heading, hidden))
                return outer.typed.pop(0)

            def textviewer(self, heading, text):
                outer.asked.append((heading, text))

            def contextmenu(self, entries):
                outer.shown.append((list(entries), -1))
                return outer.picks.pop(0)

            def yesno(self, heading, message, nolabel="", yeslabel="", autoclose=0):
                outer.asked.append((heading, message, nolabel, yeslabel))
                return outer.yes.pop(0)

        window.xbmcgui.Dialog = Dialog
        return self

    def __exit__(self, *exc):
        window.xbmcgui.Dialog = self.real


class WindowTest(unittest.TestCase):
    def test_text_as_wide_as_kodi_draws_it(self):
        # As Kodi's screenshots measure them (Estuary at 1080p).
        line = "A burnt-out chef inherits a failing flower farm and a stubborn goat,"
        self.assertAlmostEqual(window.text_width(line, 30), 887, delta=9)
        self.assertAlmostEqual(window.text_width("Continue Watching", 36, bold=True), 350, delta=8)
        self.assertEqual(window.wrap(line + " and finds out she's better at one of them.", 30, L.TEXT_WIDTH)[0][-1], "goat,")
        self.assertEqual(window.lines("A storm keeps the fleet ashore, and old grudges come out at the pub.", 30, L.TEXT_WIDTH, 3), 1)
        self.assertEqual(window.advance("é"), window.advance("e"))
        self.assertEqual(window.lines("", 30, L.TEXT_WIDTH, 3), 0)

    def test_summaries_are_cut_at_a_word(self):
        text = " ".join(["lighthouse"] * 60)
        cut = window.clamp(text, 30, L.TEXT_WIDTH, 2)
        self.assertTrue(cut.endswith("…"))
        self.assertLessEqual(window.lines(cut, 30, L.TEXT_WIDTH, 9), 2)
        self.assertEqual(window.clamp("Short.", 30, L.TEXT_WIDTH, 2), "Short.")

    def test_what_a_hero_says_of_an_episode(self):
        info = window.about(EPISODE)
        self.assertEqual(info["title"], "Driftline")
        self.assertEqual(info["context"], "S2 · E5 · Glasshouse")
        self.assertEqual(info["left"], "26 min left")
        self.assertTrue(info["progress"])
        self.assertEqual(info["summary"], "Ida gets a lead.")

    def test_the_metadata_line(self):
        detail = {"id": 31, "content_rating": "TV-MA", "files": [{"width": 1920, "height": 1080, "streams": []}]}
        self.assertEqual(
            window.meta_parts(EPISODE, detail, lead="S2 · E5"),
            [("lead", "S2 · E5"), ("text", "2025"), ("badge", "TV-MA"), ("text", "56 min"), ("quality", "1080p")],
        )
        win = FakeWindow()
        win.meta(L.HOME_INFO_META, [("text", "2025"), ("rating", "7.4"), ("quality", "4K"), ("quality", "HDR10")])
        dots = [win.made[L.meta_piece(L.HOME_INFO_META, i)[0]].visible for i in range(4)]
        # No dot first, nor between quality badges.
        self.assertEqual(dots, [False, True, True, False])
        self.assertTrue(win.made[L.meta_piece(L.HOME_INFO_META, 1)[1]].visible)
        self.assertEqual(win.made[L.meta_piece(L.HOME_INFO_META, 2)[3]].label, "4K")
        self.assertFalse(win.made[L.meta_piece(L.HOME_INFO_META, 5)[2]].visible)

    def test_credits(self):
        detail = {
            "cast": [{"id": 1, "name": "Idris Vale", "character": "Ash"}, {"id": 2, "name": "Maren Holt"}],
            "crew": [{"id": 3, "name": "Lena Okafor", "role": "director"}, {"id": 4, "name": "A. Producer", "role": "producer"}],
        }
        people = window.cast_and_crew(detail)
        self.assertEqual([p["name"] for p in people], ["Idris Vale", "Maren Holt", "Lena Okafor"])
        self.assertEqual(people[-1]["job"], "Director")
        line = window.credits_line(detail)
        self.assertIn("Lena Okafor", line)
        self.assertIn("Idris Vale, Maren Holt", line)

    def test_adding_to_a_playlist(self):
        win = FakeWindow()
        playlists = [
            {"id": 1, "title": "Road trip", "kind": "video", "item_count": 3},
            {"id": 2, "title": "Comedies", "kind": "video", "smart_filter": "genre=Comedy"},
            {"id": 3, "title": "Running", "kind": "audio", "item_count": 9},
        ]
        win.session.client = Client({"/api/playlists": {"playlists": playlists}})
        win.session.get = win.session.client.get
        with Dialogs(picks=[1]) as asked:
            win.add_to_playlist({"id": 5, "kind": "movie"})
        # A new one first, then the profile's own of the kind: smart ones fill themselves.
        self.assertEqual(asked.shown[0][0], [window.T("new_video_playlist"), "Road trip"])
        self.assertEqual(win.session.client.posts, [("/api/playlists/1/items", {"item_ids": [5]})])
        self.assertEqual(win.toasts, [window.T("added_to") % "Road trip"])
        with Dialogs(picks=[0], typed=[" Late albums "]) as asked:
            win.add_to_playlist({"id": 9, "kind": "album"})
        self.assertEqual(asked.shown[0][0], [window.T("new_music_playlist"), "Running"])
        self.assertEqual(win.session.client.posts[-1], ("/api/playlists", {"title": "Late albums", "kind": "audio", "item_ids": [9]}))
        # Backing out of naming it adds nothing.
        with Dialogs(picks=[0], typed=[""]):
            win.add_to_playlist({"id": 9, "kind": "album"})
        self.assertEqual(len(win.session.client.posts), 2)

    def test_an_album_and_its_tracks(self):
        album = {"id": 40, "kind": "album", "title": "Blue Hour", "parent_id": 39, "parent_title": "Nadia Rose", "year": 2021}
        tracks = [
            {"id": 41, "kind": "track", "title": "Opening", "index": 1, "duration_ms": 225_000, "parent_title": "Blue Hour", "grandparent_title": "Nadia Rose"},
            {"id": 42, "kind": "track", "title": "Long Night", "index": 2, "duration_ms": 3_723_000, "parent_title": "Blue Hour", "grandparent_title": "Nadia Rose"},
        ]
        win = FakeWindow()
        win.session.client = Client({"/api/items/40": album, "/api/items/40/children": {"items": tracks}})
        win.session.get = win.session.client.get
        win.session.details, win.session.lock = {}, window.threading.Lock()
        page = window.Item(win, 40, {"id": 40, "kind": "album"})
        page.load()
        page.show(False)
        # The tracks, numbered with their length, under the album's line.
        self.assertEqual([row["key"] for row in page.model], ["tracks"])
        shown = win.made[L.slot_list(L.ITEM_ROWS, 0, "episode")].items
        self.assertEqual([(li.label, li.label2) for li in shown], [("1. Opening", "3:45"), ("2. Long Night", "1:02:03")])
        self.assertEqual(win.made[L.ITEM_CONTEXT].label, "Nadia Rose")
        self.assertEqual(page.meta_line(), [("text", "2021"), ("text", "2 tracks"), ("text", "1h 6m")])
        # Play and Shuffle; music isn't marked watched.
        self.assertTrue(win.made[L.button_box(L.ITEM_SHUFFLE)].visible)
        self.assertFalse(win.made[L.button_box(L.ITEM_WATCHED)].visible)
        self.assertFalse(win.made[L.button_box(L.ITEM_UNWATCHED)].visible)
        self.assertEqual(win.focused, L.ITEM_PLAY)
        self.assertIn((window.T("go_to_artist")), [name for name, _ in page.more_actions()])
        # A track plays the album on from it, each asked of the plugin as Kodi comes to it.
        queue = window.xbmc.PlayList.entries
        del window.xbmc.played[:]
        win.made[L.slot_list(L.ITEM_ROWS, 0, "episode")].selected = 1
        page.click(L.slot_list(L.ITEM_ROWS, 0, "episode"))
        self.assertEqual([url for url, _ in queue[window.xbmc.PLAYLIST_MUSIC]], ["plugin://plugin.video.pley/?route=play&id=41", "plugin://plugin.video.pley/?route=play&id=42"])
        self.assertEqual(window.xbmc.played[-1][2], 1)
        self.assertFalse(window.xbmc.PlayList.shuffled[window.xbmc.PLAYLIST_MUSIC])
        page.click(L.ITEM_SHUFFLE)
        self.assertTrue(window.xbmc.PlayList.shuffled[window.xbmc.PLAYLIST_MUSIC])
        self.assertEqual(window.xbmc.played[-1][2], 0)

    def test_your_rating(self):
        self.assertEqual([window.words.stars(v) for v in (1, 2, 3, 7, 10)], ["½ star", "1 star", "1½ stars", "3½ stars", "5 stars"])

        class Server:
            def __init__(self):
                self.puts = []

            def put(self, path, body=None):
                self.puts.append((path, body))

        win = FakeWindow()
        win.session.client = Server()
        page = window.Item(win, 5)
        page.detail = {"id": 5, "kind": "movie", "user_state": {"rating": 6.0}}
        with Dialogs(picks=[8]) as asked:
            page.rate()
        # Not rated, then half stars; the saved one chosen first.
        self.assertEqual((len(asked.shown[0][0]), asked.shown[0][1]), (11, 6))
        self.assertEqual(win.session.client.puts, [("/api/items/5/rating", {"rating": 8})])
        with Dialogs(picks=[0]):
            page.rate()
        self.assertEqual(win.session.client.puts[-1], ("/api/items/5/rating", {"rating": None}))

    def test_an_artist_plays_album_by_album(self):
        albums = [{"id": 50, "kind": "album", "title": "Early", "year": 2001}, {"id": 51, "kind": "album", "title": "Late", "year": 2020}]
        answers = {
            "/api/items/49/children": {"items": albums},
            "/api/items/50/children": {"items": [{"id": 60, "kind": "track", "title": "One"}]},
            "/api/items/51/children": {"items": [{"id": 61, "kind": "track", "title": "Two"}]},
        }
        session = Client(answers)
        # Newest first, as the artist's page lists them.
        self.assertEqual([t["id"] for t in window.music_tracks(session, {"id": 49, "kind": "artist"})], [61, 60])
        win = FakeWindow()
        win.session.get = Client({"/api/items/52/children": {"items": []}}).get
        win.play({"id": 52, "kind": "album"})
        self.assertEqual(win.toasts, [window.T("no_tracks")])

    def test_whos_watching(self):
        household = [
            {"id": 1, "name": "Anna", "current": True, "needs": "none"},
            {"id": 2, "name": "Ben", "needs": "none", "avatar_url": "/api/users/2/avatar"},
            {"id": 3, "name": "Kid", "needs": "pin"},
            {"id": 4, "name": "Dad", "needs": "password"},
        ]
        asked, refusals = [], []

        def switch(client, profile_id, pin=None):
            asked.append((profile_id, pin))
            if refusals:
                raise refusals.pop(0)

        real = window.profiles.switch
        window.profiles.switch = switch
        self.addCleanup(setattr, window.profiles, "switch", real)
        self.addCleanup(window.kodi.clear_property, window.profiles.GATE)
        win = FakeWindow()
        win.me = {"id": 1, "has_pin": True}
        win.session.client = Client()
        switched = []
        win.switched = lambda: switched.append(True)
        page = window.Profiles(win, household, gate=True)
        page.show(False)
        faces = win.made[L.PROFILES_LIST]
        self.assertEqual([li.label for li in faces.items], ["Anna", "Ben", "Kid", "Dad"])
        self.assertEqual(faces.items[2].properties.get("locked"), "1")
        # The one in use shows its PIN too, which the server's list doesn't.
        self.assertEqual(faces.items[0].properties.get("locked"), "1")
        self.assertIsNone(faces.items[1].properties.get("locked"))
        self.assertIn("/api/users/2/avatar", faces.items[1].art["tile"])
        # The one in use first, and the row centred.
        self.assertEqual(faces.selected, 0)
        self.assertEqual(faces.position[0], L.profiles_left(4) - L.PROFILE_MARGIN)
        # Another without a PIN: at once.
        page.pick(household[1])
        self.assertEqual((asked[-1], switched), ((2, None), [True]))
        self.assertEqual(window.kodi.get_property(window.profiles.GATE), "1")
        # One with a PIN: Kodi's number dialog, hidden; a wrong one asks
        # again, saying so, and so does one too many.
        refusals.append(window.api.ApiError(400, "wrong_pin", "wrong PIN"))
        refusals.append(window.api.ApiError(400, "throttled", "too many attempts, try again in 15 min"))
        with Dialogs(typed=["9999", "8888", ""]) as dialogs:
            page.pick(household[2])
        heading = window.T("pin_of") % "Kid"
        self.assertEqual(
            dialogs.asked,
            [(heading, True), (window.T("wrong_pin") + " " + heading, True), ("Too many attempts, try again in 15 min. " + heading, True)],
        )
        self.assertEqual(asked[-2:], [(3, "9999"), (3, "8888")])
        # A locked one says so, and isn't asked again.
        refusals.append(window.api.ApiError(400, "pin_locked", "locked"))
        with Dialogs(typed=["7777"]) as dialogs:
            page.pick(household[2])
        self.assertEqual(win.made[L.PROFILES_NOTE].label, window.T("pin_locked"))
        # One that took nothing when the list was made, with a PIN since: asked for.
        refusals.append(window.api.ApiError(400, "wrong_pin", "this profile is protected by a PIN"))
        with Dialogs(typed=[""]) as dialogs:
            page.pick(household[1])
        self.assertEqual(dialogs.asked, [(window.T("pin_of") % "Ben", True)])
        with Dialogs(typed=["12"]):
            page.pick(household[2])
        self.assertEqual(win.made[L.PROFILES_NOTE].label, window.T("pin_digits"))
        self.assertEqual(asked[-1], (2, None))
        # One that needs its password: signing in again with a code.
        with Dialogs(yes=[True]):
            page.pick(household[3])
        self.assertEqual(win.after, "sign_in")
        # Going on as the one in use, at Kodi's start, asks its PIN; a
        # server that can't check it lets it through.
        win.after = None
        refusals.append(window.api.ApiError(400, "invalid_request", "already"))
        with Dialogs(typed=["1234"]):
            page.pick(household[0])
        self.assertEqual(asked[-1], (1, "1234"))
        self.assertIsInstance(win.opened[-1], window.Home)
        # Not at the start: no PIN for the one in use, and Back goes back.
        later = window.Profiles(win, household)
        later.pick(household[0])
        self.assertEqual(len(asked), 6)
        self.assertFalse(later.back())

    def test_editing_a_playlist(self):
        class Server:
            """A playlist's routes, noted; answers as pley's."""

            def __init__(self):
                self.calls, self.title, self.smart = [], "Road trip", None
                self.entries = [{"entry_id": 10 + n, "item": {"id": n, "kind": "movie", "title": "Film %d" % n}} for n in (1, 2, 3)]

            def detail(self):
                return {"id": 7, "title": self.title, "kind": "video", "smart_filter": self.smart, "entries": list(self.entries)}

            def get(self, path, **query):
                return self.detail()

            def put(self, path, body=None):
                self.calls.append(("PUT", path, body))
                order = {entry_id: at for at, entry_id in enumerate(body["entry_ids"])}
                self.entries.sort(key=lambda e: order[e["entry_id"]])
                return self.detail()

            def patch(self, path, body=None):
                self.calls.append(("PATCH", path, body))
                self.title = body["title"]
                return self.detail()

            def delete(self, path):
                self.calls.append(("DELETE", path))
                self.entries = [e for e in self.entries if not path.endswith("/%d" % e["entry_id"])]

        server = Server()
        win = FakeWindow()
        win.session.client, win.session.get = server, server.get
        win.session.details, win.session.lock = {}, window.threading.Lock()
        page = window.Playlist(win, {"id": 7, "title": "Road trip"})
        page.load()
        page.show(False)
        grid = win.made[L.grid_list(L.GRID_LISTS, page.shape)]
        self.assertEqual([li.label for li in grid.items], ["Film 1", "Film 2", "Film 3"])
        # A title's menu: its own actions, then the playlist's.
        grid.selected = 0
        with Dialogs(picks=[3]) as asked:
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        names = asked.shown[0][0]
        self.assertEqual(names[3:], [window.T("move_down"), window.T("remove_from_playlist"), window.T("rename_playlist"), window.T("delete_playlist")])
        self.assertEqual(server.calls[-1], ("PUT", "/api/playlists/7/order", {"entry_ids": [12, 11, 13]}))
        self.assertEqual(([li.label for li in grid.items], grid.selected), (["Film 2", "Film 1", "Film 3"], 1))
        # Remove from playlist: focus on the one before, past the end.
        grid.selected = 2
        with Dialogs(picks=[4]) as asked:
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        self.assertEqual(asked.shown[0][0][3:5], [window.T("move_up"), window.T("remove_from_playlist")])
        self.assertEqual(server.calls[-1], ("DELETE", "/api/playlists/7/items/13"))
        self.assertEqual((len(grid.items), grid.selected), (2, 1))
        # Rename playlist.
        grid.selected = 0
        with Dialogs(picks=[5], typed=["Long trip"]):
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        self.assertEqual(server.calls[-1], ("PATCH", "/api/playlists/7", {"title": "Long trip"}))
        self.assertEqual(win.made[L.GRID_TITLE].label, "Long trip")
        # A smart playlist fills itself: renamed and deleted only.
        server.smart = "genre=Comedy"
        page.refresh()
        with Dialogs(picks=[-1]) as asked:
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        self.assertEqual(asked.shown[0][0][3:], [window.T("rename_playlist"), window.T("delete_playlist")])
        # Delete playlist asks first.
        with Dialogs(picks=[4], yes=[False]):
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        self.assertEqual(server.calls[-1][0], "PATCH")
        with Dialogs(picks=[4], yes=[True]) as asked:
            page.menu(L.grid_list(L.GRID_LISTS, page.shape))
        self.assertEqual(asked.asked[0][0], window.T("delete_playlist_ask"))
        self.assertEqual(server.calls[-1], ("DELETE", "/api/playlists/7"))
        self.assertEqual(win.toasts[-1], window.T("playlist_deleted"))

    def test_a_music_playlist_plays_on_from_a_track(self):
        tracks = [{"entry_id": 20 + n, "item": {"id": 30 + n, "kind": "track", "title": "Song %d" % n}} for n in (1, 2)]
        win = FakeWindow()
        win.session.get = Client({"/api/playlists/8": {"id": 8, "title": "Running", "kind": "audio", "entries": tracks}}).get
        page = window.Playlist(win, {"id": 8, "title": "Running"})
        page.load()
        self.assertEqual(page.shape, "square")
        win.made[L.grid_list(L.GRID_LISTS, "square")] = grid = Control()
        grid.selected = 1
        page.click(L.grid_list(L.GRID_LISTS, "square"))
        self.assertEqual(len(window.xbmc.PlayList.entries[window.xbmc.PLAYLIST_MUSIC]), 2)
        self.assertEqual(window.xbmc.played[-1][2], 1)

    def test_library_filters(self):
        win = FakeWindow()
        win.libraries = [{"id": 4, "kind": "movies", "name": "Movies", "item_count": 342}]
        library = window.Library(win, win.libraries[0], None)
        library.items, library.total = [], 23
        library.genre, library.watched = "Science Fiction", False
        library.sort, library.descending = "year", True
        # A server from before the quality filters: no Resolution or Dynamic range.
        library.filters = {"genres": ["Drama", "Science Fiction"], "years": [1999]}
        self.assertEqual(library.query(0)["watched"], "false")
        # Save as…: Plex's syntax, as SmartFilter.of writes it.
        self.assertEqual(library.smart_filter(), "genre=Science+Fiction&unwatched=1&sort=year%3Adesc")
        library.draw()
        # A filter that is on shows its twin, which leads with a check.
        self.assertFalse(win.made[L.LIBRARY_GENRE].visible)
        self.assertTrue(win.made[L.LIBRARY_GENRE_ON].visible)
        self.assertTrue(win.made[L.LIBRARY_GENRE_ON].label.endswith("Science Fiction"))
        self.assertTrue(win.made[L.LIBRARY_YEAR].visible)
        self.assertFalse(win.made[L.LIBRARY_YEAR_ON].visible)
        self.assertEqual(win.made[L.LIBRARY_COUNT].label, window.T("count_of") % ("23", "342 movies"))
        # Clear filters, from the Filter menu (All, Unwatched, Watched, Genre, Year, Clear, Save as).
        library.reload = lambda: None
        with Dialogs(picks=[5]) as asked:
            library.filter_menu()
        self.assertEqual(asked.shown[0][1], 1)
        self.assertEqual(len(asked.shown[0][0]), 7)
        self.assertFalse(library.filtered)
        self.assertEqual(library.smart_filter(), "sort=year%3Adesc")
        # Watched, then Save as… makes the smart playlist and opens it.
        with Dialogs(picks=[2]):
            library.filter_menu()
        self.assertIs(library.watched, True)
        win.session.client = Client()
        with Dialogs(picks=[6], typed=["Seen"]):
            library.filter_menu()
        self.assertEqual(win.session.client.posts, [("/api/playlists", {"title": "Seen", "smart_filter": "unwatched=0&sort=year%3Adesc", "library_id": 4})])
        self.assertEqual(len(win.opened), 1)

    def test_library_quality_filters(self):
        uhd = {"bitrate": 42_000_000, "size": 25_125_558_682, "width": 3840, "height": 2160, "video_codec": "hevc", "dynamic_range": "dolby_vision"}
        hd = {"bitrate": 8_200_000, "size": 4_294_967_296, "width": 1920, "height": 1080, "video_codec": "h264", "dynamic_range": "sdr"}
        items = [
            {"id": 1, "kind": "movie", "title": "Saltwind", "year": 1995, "user_state": {}, "media": uhd},
            {"id": 2, "kind": "movie", "title": "Low Orbit", "year": 2019, "user_state": {}, "media": hd},
        ]
        filters = {
            "genres": ["Drama"],
            "years": [1999],
            "resolutions": ["4k", "1080"],
            # Best first, "hdr" leading; a token this add-on has no word for is left out.
            "dynamic_ranges": ["hdr", "dolby_vision", "hdr12", "sdr"],
        }
        asked = []

        def get(path, **query):
            asked.append((path, query))
            return filters if path.endswith("/filters") else {"items": items, "total": 2}

        win = FakeWindow()
        win.session.get = get
        win.libraries = [{"id": 4, "kind": "movies", "name": "Movies", "item_count": 342}]
        library = window.Library(win, win.libraries[0], None)
        library.load()
        library.draw()
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        # Sorted by title and unfiltered, the posters say nothing of their quality.
        self.assertEqual([(li.label2, li.properties.get("resolution"), li.properties.get("range")) for li in grid.items], [("1995", None, None), ("2019", None, None)])
        self.assertTrue(win.made[L.LIBRARY_FILTER].visible)
        self.assertFalse(win.made[L.LIBRARY_FILTER_ON].visible)
        # Filter: Resolution and Dynamic range after Year, with nothing chosen.
        with Dialogs(picks=[5, 1]) as dialogs:
            library.click(L.LIBRARY_FILTER)
        names = dialogs.shown[0][0]
        self.assertEqual(names[3:], ["Genre: All genres", "Year: All years", "Resolution: All resolutions", "Dynamic range: All dynamic ranges"])
        # The choices are the server's, in its order, by their names.
        self.assertEqual(dialogs.shown[1], (["All resolutions", "4K", "1080p"], 0))
        self.assertEqual(asked[-1], ("/api/libraries/4/items", {"sort": "title", "order": "asc", "offset": 0, "limit": window.PAGE, "resolution": "4k"}))
        # A resolution chosen: the posters carry their badges, their line stays the year.
        self.assertEqual([(li.label2, li.properties.get("resolution"), li.properties.get("range")) for li in grid.items], [("1995", "4k", "dolby_vision"), ("2019", None, None)])
        # Filter's twin says what is chosen, and keeps focus.
        self.assertFalse(win.made[L.LIBRARY_FILTER].visible)
        self.assertTrue(win.made[L.LIBRARY_FILTER_ON].visible)
        self.assertEqual(win.made[L.LIBRARY_FILTER_ON].label, window.ICON_ROOM + "4K")
        self.assertEqual(win.focused, L.LIBRARY_FILTER_ON)
        self.assertEqual(win.made[L.LIBRARY_COUNT].label, window.T("count_of") % ("2", "342 movies"))
        with Dialogs(picks=[6, 2]) as dialogs:
            library.click(L.LIBRARY_FILTER_ON)
        self.assertEqual(dialogs.shown[0][0][5:], ["Resolution: 4K", "Dynamic range: All dynamic ranges", window.T("clear_filters"), window.T("save_as")])
        self.assertEqual(dialogs.shown[1], (["All dynamic ranges", "Any HDR", "Dolby Vision", "SDR"], 0))
        self.assertEqual(asked[-1][1]["dynamic_range"], "dolby_vision")
        self.assertEqual(win.made[L.LIBRARY_FILTER_ON].label, window.ICON_ROOM + "4K · Dolby Vision")
        # Save as…: after the year, before unwatched, in Plex's syntax (Pley's dynamicRange).
        library.genre, library.year, library.watched = "Drama", 1999, False
        self.assertEqual(library.smart_filter(), "genre=Drama&year=1999&resolution=4k&dynamicRange=dolby_vision&unwatched=1&sort=titleSort")
        # A value chosen stays offered when the server no longer lists its kind, to change or clear it.
        library.genre, library.year, library.watched = None, None, None
        filters["resolutions"], filters["dynamic_ranges"] = [], []
        library.filters = None
        with Dialogs(picks=[-1]) as dialogs:
            library.click(L.LIBRARY_FILTER_ON)
        self.assertEqual(dialogs.shown[0][0][5:7], ["Resolution: 4K", "Dynamic range: Dolby Vision"])
        # Clear filters clears them too.
        with Dialogs(picks=[7]):
            library.click(L.LIBRARY_FILTER_ON)
        self.assertEqual((library.resolution, library.dynamic_range, library.filtered), (None, None, False))
        self.assertNotIn("resolution", asked[-1][1])
        self.assertEqual(win.focused, L.LIBRARY_FILTER)
        # Nothing to choose from and nothing chosen: no rows for them.
        with Dialogs(picks=[-1]) as dialogs:
            library.click(L.LIBRARY_FILTER)
        self.assertEqual(len(dialogs.shown[0][0]), 5)
        # The filters' answer missing: the menu goes on without them.
        library.filters = None
        win.session.get = lambda path, **query: (_ for _ in ()).throw(window.api.ApiError(500, "internal", "down"))
        with Dialogs(picks=[-1]) as dialogs:
            library.click(L.LIBRARY_FILTER)
        self.assertEqual(len(dialogs.shown[0][0]), 5)

    def test_library_sorts_and_the_bitrate_line(self):
        win = FakeWindow()
        asked = []
        media = {"bitrate": 42_000_000, "size": 25_125_558_682, "width": 3840, "height": 2160, "dynamic_range": "hdr10plus"}
        items = [
            {"id": 1, "kind": "movie", "title": "Saltwind", "year": 1995, "user_state": {}, "media": media},
            {"id": 2, "kind": "movie", "title": "Low Orbit", "year": 2019, "user_state": {}, "media": {"bitrate": None, "size": None, "width": 720, "height": 576}},
        ]

        def get(path, **query):
            asked.append(query)
            return {"items": list(items), "total": 4}

        win.session.get = get
        movies = window.Library(win, {"id": 4, "kind": "movies", "name": "Movies"}, None)
        # The web's sorts, in its order: Your rating and Bitrate too.
        with Dialogs(picks=[8]) as dialogs:
            movies.click(L.LIBRARY_SORT)
        self.assertEqual(dialogs.shown[0][0], ["Title", "Year", "Date added", "Rating", "Your rating", "Release date", "Duration", "Last played", "Bitrate"])
        self.assertEqual((asked[-1]["sort"], asked[-1]["order"]), ("bitrate", "desc"))
        self.assertEqual(win.focused, L.LIBRARY_SORT_DOWN)
        # Sorted by bitrate: the line under each poster is its bitrate and size, and the posters
        # carry their badges.
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        self.assertEqual([li.label2 for li in grid.items], ["42 Mbps · 23.4 GB", "Bitrate unknown"])
        self.assertEqual((grid.items[0].properties["resolution"], grid.items[0].properties["range"]), ("4k", "hdr10plus"))
        self.assertNotIn("resolution", grid.items[1].properties)
        self.assertEqual(movies.smart_filter(), "sort=mediaBitrate%3Adesc")
        # The next page's posters too.
        win.later = at_once
        movies.more()
        self.assertEqual(asked[-1]["offset"], 2)
        self.assertEqual([li.label2 for li in grid.items[2:]], ["42 Mbps · 23.4 GB", "Bitrate unknown"])
        with Dialogs(picks=[4]):
            movies.click(L.LIBRARY_SORT_DOWN)
        self.assertEqual(asked[-1]["sort"], "user_rating")
        self.assertEqual(movies.smart_filter(), "sort=userRating%3Adesc")
        self.assertEqual([(li.label2, li.properties.get("resolution")) for li in grid.items], [("1995", None), ("2019", None)])
        # Shows have no one duration, and artists sort as the web's music does.
        shows = window.Library(win, {"id": 5, "kind": "shows", "name": "TV Shows"}, None)
        with Dialogs(picks=[-1]) as dialogs:
            shows.click(L.LIBRARY_SORT)
        self.assertNotIn("Duration", dialogs.shown[0][0])
        self.assertEqual(dialogs.shown[0][0][-1], "Bitrate")
        music = window.Library(win, {"id": 6, "kind": "music", "name": "Music"}, None)
        with Dialogs(picks=[-1]) as dialogs:
            music.click(L.LIBRARY_SORT)
        self.assertEqual(dialogs.shown[0][0], ["Title", "Date added", "Your rating", "Last played"])

    def test_a_library_opens_on_the_tab_it_was_left_on(self):
        settings = window.kodi.xbmcaddon.settings
        profile = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, profile, True)
        before = dict(settings)
        self.addCleanup(lambda: (settings.clear(), settings.update(before)))
        settings.update({"_profile": profile, "server_id": "srv-1"})
        movies, shows = {"id": 4, "kind": "movies", "name": "Movies"}, {"id": 5, "kind": "shows", "name": "TV Shows"}
        music = {"id": 6, "kind": "music", "name": "Music"}

        def opened(library, me=7):
            win = FakeWindow()
            win.libraries, win.me = [movies, shows, music], {} if me is None else {"id": me}
            page = window.Library(win, library, None)
            page.load = lambda: None
            page.show = lambda filled: None
            return page

        # Library the first time.
        self.assertEqual(opened(movies).tab, "all")
        opened(movies).switch("recommended")
        self.assertEqual(opened(movies).tab, "recommended")
        # Each library keeps its own, and each profile and server theirs.
        self.assertEqual(opened(shows).tab, "all")
        self.assertEqual(opened(movies, me=8).tab, "all")
        settings["server_id"] = "srv-2"
        self.assertEqual(opened(movies).tab, "all")
        settings["server_id"] = "srv-1"
        opened(movies).switch("collections")
        self.assertEqual(opened(movies).tab, "collections")
        # Music has no Collections: Library instead.
        opened(music).switch("recommended")
        self.assertEqual(opened(music).tab, "recommended")
        with open(os.path.join(profile, window.kodi.REMEMBERED), encoding="utf-8") as f:
            kept = json.load(f)
        kept["library_tab/srv-1/7/6"] = "collections"
        with open(os.path.join(profile, window.kodi.REMEMBERED), "w", encoding="utf-8") as f:
            json.dump(kept, f)
        self.assertEqual(opened(music).tab, "all")
        # Without a profile, nothing is read or kept.
        self.assertEqual(opened(movies, me=None).tab, "all")
        opened(movies, me=None).switch("recommended")
        opened(movies).switch("all")
        self.assertEqual(opened(movies).tab, "all")
        # Kept in the add-on's data folder, so a restarted Kodi has it.
        with open(os.path.join(profile, window.kodi.REMEMBERED), encoding="utf-8") as f:
            self.assertEqual(json.load(f), {"library_tab/srv-1/7/4": "all", "library_tab/srv-1/7/6": "collections"})
        # A file that isn't one opens on Library.
        with open(os.path.join(profile, window.kodi.REMEMBERED), "w", encoding="utf-8") as f:
            f.write("{")
        self.assertEqual(opened(movies).tab, "all")
        opened(movies).switch("recommended")
        self.assertEqual(opened(movies).tab, "recommended")

    def test_sorting_the_libraries(self):
        movies, shows, music = ({"id": 1, "name": "Movies", "kind": "movies"}, {"id": 2, "name": "TV Shows", "kind": "shows"}, {"id": 3, "name": "Music", "kind": "music"})
        every = {lib["id"]: lib for lib in (movies, shows, music)}

        class Server:
            """PUT and DELETE /api/me/libraries/order, noted; the answer is
            the libraries in their new order."""

            def __init__(self, missing=False):
                self.missing, self.calls = missing, []

            def get(self, path, **query):
                return {}

            def put(self, path, body=None):
                self.calls.append(("PUT", path, body))
                if self.missing:
                    raise window.api.ApiError(404, "not_found", "no such route")
                return [every[i] for i in body["ids"]]

            def delete(self, path):
                self.calls.append(("DELETE", path, None))
                return [movies, shows, music]

        win = FakeWindow()
        win.libraries = [movies, shows, music]
        win.session.client = Server()
        shows_page = window.Library(win, shows, None)
        self.assertEqual(shows_page.rail, "lib1")
        win.screen = shows_page
        # Sort libraries: TV Shows, then First; Back when done.
        with Dialogs(picks=[1, 0, -1]) as asked:
            win.sort_libraries()
        self.assertEqual(asked.shown[0][0], ["Movies", "TV Shows", "Music", window.T("server_order")])
        self.assertEqual(asked.shown[1], ([window.T("first"), window.T("after") % "Movies", window.T("after") % "Music"], 1))
        self.assertEqual(win.session.client.calls, [("PUT", "/api/me/libraries/order", {"ids": [2, 1, 3]})])
        self.assertEqual([lib["id"] for lib in win.libraries], [2, 1, 3])
        self.assertEqual(win.made[L.rail_labels(L.RAIL_LIBRARIES[0])[0]].label, "TV Shows")
        # The rail's mark stays on the library open.
        self.assertEqual(shows_page.rail, "lib0")
        # A rail library's menu: the first has no Move up.
        with Dialogs(picks=[0]) as asked:
            win.library_menu(L.RAIL_LIBRARIES[0])
        self.assertEqual(asked.shown[0][0], [window.T("move_down"), window.T("server_order")])
        self.assertEqual(win.session.client.calls[-1][2], {"ids": [1, 2, 3]})
        self.assertEqual(win.focused, L.RAIL_LIBRARIES[1])
        with Dialogs(picks=[2]):
            win.library_menu(L.RAIL_LIBRARIES[1])
        self.assertEqual(win.session.client.calls[-1], ("DELETE", "/api/me/libraries/order", None))
        # An older server has no such route: it says so.
        win.session.client = Server(missing=True)
        with Dialogs(picks=[0]):
            win.library_menu(L.RAIL_LIBRARIES[0])
        self.assertEqual(win.toasts[-1], window.T("sort_needs_newer_server"))
        self.assertEqual([lib["id"] for lib in win.libraries], [1, 2, 3])
        # Sorted on another device: libraries_changed refreshes the window,
        # and the rail follows.
        win.session.get = lambda path, **query: [music, movies, shows]
        win.session.details = {}
        win.session.lock = window.threading.Lock()
        win.screen = shows_page
        # The page redrawn takes focus; someone in the rail stays there.
        shows_page.refresh = lambda: win.setFocusId(L.LIBRARY_SORT)
        win.setFocusId(L.RAIL_SEARCH)
        win.changed()
        self.assertEqual(win.made[L.rail_labels(L.RAIL_LIBRARIES[0])[0]].label, "Music")
        self.assertEqual(shows_page.rail, "lib2")
        self.assertEqual(win.focused, L.RAIL_SEARCH)

    def test_signing_out_asks_first(self):
        win = FakeWindow()
        win.libraries = []
        left = []
        win.leave_for = left.append
        window.kodi.xbmcaddon.settings["server_name"] = "Living room"
        self.addCleanup(window.kodi.xbmcaddon.settings.pop, "server_name", None)
        requests_off(self)
        # The profile's menu: Choose a server, Audio and subtitles, Sign out.
        with Dialogs(picks=[2], yes=[False]) as asked:
            win.account()
        # Short: the question, the server, and Sign out to say yes.
        self.assertEqual(asked.asked, [("Sign out?", "You'll be signed out of Living room.", window.T("cancel"), window.T("sign_out"))])
        self.assertEqual(left, [])
        with Dialogs(picks=[2], yes=[True]):
            win.account()
        self.assertEqual(left, ["sign_out"])

    def test_the_rail_fits_every_library_above_settings(self):
        # Entries 80 tall and 8 apart (the other TV apps' 40 and 4), nine of them over Settings.
        self.assertEqual((L.RAIL_ROW, L.RAIL_PITCH, L.RAIL_FIT), (80, 88, 9))
        avatar_foot = L.RAIL_AVATAR_Y + 50
        for live in (False, True):
            for count in range(9):
                win = FakeWindow()
                win.libraries = [{"id": i, "name": "Library %d" % i, "kind": "movies"} for i in range(count)]
                win.live = {"available": live}
                win.setup_rail()
                # As on Android TV (RailItems.kt), the libraries, then Libraries in Collections' place
                # when the rail can't list them all: at least four, and five while nine entries fit
                # (Search, Home, Libraries and Playlists around them); no Collections without a library.
                shown = min(count, 4 if live else 5)
                more = count > shown
                top = [L.RAIL_SEARCH, L.RAIL_HOME] + ([L.RAIL_LIVE] if live else [])
                if more:
                    entries = top + [*L.RAIL_LIBRARIES[:shown], L.RAIL_ALL_LIBRARIES, L.RAIL_PLAYLISTS]
                else:
                    entries = top + [*L.RAIL_LIBRARIES[:count], L.RAIL_PLAYLISTS] + ([L.RAIL_COLLECTIONS] if count else [])
                self.assertEqual(window.rail_entries(count, live), entries)
                self.assertEqual(window.rail_shown(count, live), shown)
                for i, entry in enumerate(L.RAIL_LIBRARIES):
                    self.assertEqual(win.made[L.rail_group(entry)].visible, i < shown, (live, count, i))
                self.assertEqual(win.made[L.rail_group(L.RAIL_ALL_LIBRARIES)].visible, more)
                self.assertEqual(win.made[L.rail_group(L.RAIL_COLLECTIONS)].visible, 0 < count == shown)
                # Each pill's top and bottom on the screen.
                pills = []
                for entry in entries + [L.RAIL_SETTINGS]:
                    group, pill = win.made[L.rail_group(entry)], win.made[entry]
                    top_ = group.position[1] + pill.position[1]
                    pills.append((top_, top_ + pill.height))
                    self.assertEqual(win.made[L.rail_pill(entry)].height, pill.height)
                self.assertGreaterEqual(pills[0][0] - avatar_foot, L.RAIL_GAP, "under the avatar")
                for (_, above), (below, _) in zip(pills, pills[1:]):
                    self.assertGreaterEqual(below - above, L.RAIL_GAP, "%d libraries: %s" % (count, pills))
                # Settings stays where skin.py puts it, its foot on the safe area's.
                self.assertEqual(win.made[L.rail_group(L.RAIL_SETTINGS)].position, (0, L.RAIL_SETTINGS_Y - L.RAIL_ROW // 2))
                self.assertEqual(pills[-1][1], L.HEIGHT - L.SAFE_Y)
                # Every entry at full height: none smaller than the others.
                self.assertEqual({bottom - top_ for top_, bottom in pills}, {L.RAIL_ROW}, pills)
                # From Search down, a pitch apart.
                self.assertEqual(win.made[L.rail_group(entries[-1])].position[1], L.RAIL_TOP + (len(entries) - 1) * L.RAIL_PITCH - L.RAIL_ROW // 2)
        # Too many entries for their places close up, as before, the pills still apart.
        self.assertEqual(L.rail_pitch(L.RAIL_FIT + 1), (L.RAIL_SETTINGS_Y - L.RAIL_TOP) // (L.RAIL_FIT + 1))
        self.assertEqual(L.rail_row(L.rail_pitch(L.RAIL_FIT + 1)), L.rail_pitch(L.RAIL_FIT + 1) - L.RAIL_GAP)

    def test_a_library_past_the_rail_marks_libraries(self):
        libraries = [{"id": i, "name": "Library %d" % i, "kind": "movies"} for i in range(6)]
        self.assertEqual(window.library_rail(libraries, 4), "lib4")
        self.assertEqual(window.library_rail(libraries, 5), "libraries")
        # With Live TV in the rail, four libraries.
        self.assertEqual(window.library_rail(libraries, 3, live=True), "lib3")
        self.assertEqual(window.library_rail(libraries, 4, live=True), "libraries")
        self.assertEqual(window.library_rail(libraries, 9), "home")
        # Live TV coming takes the fifth library's place: its page marks Libraries, focus goes there.
        win = FakeWindow()
        win.libraries = libraries
        page = window.Library(win, libraries[4], None)
        win.screen = page
        self.assertEqual(page.rail, "lib4")
        self.assertEqual(win.rail_entry("lib4"), L.RAIL_LIBRARIES[4])
        win.take_libraries(libraries, live={"available": True})
        self.assertEqual(page.rail, "libraries")
        self.assertEqual(win.rail_entry("lib4"), L.RAIL_HOME)
        self.assertFalse(win.made[L.rail_group(L.RAIL_LIBRARIES[4])].visible)
        self.assertTrue(win.made[L.rail_group(L.RAIL_ALL_LIBRARIES)].visible)

    def test_the_libraries_page(self):
        # Six libraries: the rail has five, and Libraries all of them (TvLibrariesScreen).
        listed = [
            {"id": 1, "name": "Movies", "kind": "movies", "item_count": 1234},
            {"id": 2, "name": "TV Shows", "kind": "shows", "item_count": 1},
            {"id": 3, "name": "Music", "kind": "music", "item_count": 40},
            {"id": 4, "name": "Documentaries", "kind": "movies", "item_count": 12, "scanning": True},
            {"id": 5, "name": "Anime", "kind": "shows"},
            {"id": 6, "name": "Concerts", "kind": "movies", "item_count": 3},
        ]

        class Server:
            """GET /api/libraries, PUT and DELETE /api/me/libraries/order."""

            def __init__(self):
                self.listed, self.orders = list(listed), []

            def get(self, path, **query):
                return list(self.listed)

            def put(self, path, body=None):
                self.orders.append(body["ids"])
                self.listed = [next(lib for lib in listed if lib["id"] == i) for i in body["ids"]]
                return list(self.listed)

            def delete(self, path):
                self.orders.append(None)
                self.listed = list(listed)
                return list(self.listed)

        server = Server()
        win = FakeWindow()
        win.session.client = server
        win.session.get = server.get
        win.libraries = []
        page = window.Libraries(win)
        page.load()
        self.assertEqual(page.rail, "libraries")
        self.assertTrue(win.made[L.rail_group(L.RAIL_ALL_LIBRARIES)].visible)
        win.screen = page
        page.show(False)
        cards = win.made[L.LIBRARIES_LIST]
        self.assertEqual([(c.label, c.label2) for c in cards.items], [
            ("Movies", "1,234 movies"),
            ("TV Shows", "1 show"),
            ("Music", "40 artists"),
            ("Documentaries", "12 movies · " + window.T("scanning_now")),
            ("Anime", ""),
            ("Concerts", "3 movies"),
        ])
        self.assertEqual(cards.items[2].properties["icon"], "icon-music.png")
        self.assertEqual(win.focused, L.LIBRARIES_LIST)
        # A card opens its library; past the rail's five, its page marks Libraries.
        cards.selectItem(5)
        page.click(L.LIBRARIES_LIST)
        self.assertEqual((win.opened[-1].library["name"], win.opened[-1].rail), ("Concerts", "libraries"))
        cards.selectItem(3)
        page.click(L.LIBRARIES_LIST)
        self.assertEqual(win.opened[-1].rail, "lib3")
        cards.selectItem(1)
        page.click(L.LIBRARIES_LIST)
        self.assertEqual(win.opened[-1].rail, "lib1")
        # A card's menu: Open, Move up, Move down, the server's order. Focus goes with the card.
        with Dialogs(picks=[2]) as asked:
            page.menu(L.LIBRARIES_LIST)
        self.assertEqual(asked.shown[0][0], [window.T("open"), window.T("move_up"), window.T("move_down"), window.T("server_order")])
        self.assertEqual(server.orders[-1], [1, 3, 2, 4, 5, 6])
        self.assertEqual([c.label for c in cards.items[:3]], ["Movies", "Music", "TV Shows"])
        self.assertEqual(cards.getSelectedPosition(), 2)
        self.assertEqual(win.made[L.rail_labels(L.RAIL_LIBRARIES[1])[0]].label, "Music")
        # The first card has no Move up.
        cards.selectItem(0)
        with Dialogs(picks=[2]) as asked:
            page.menu(L.LIBRARIES_LIST)
        self.assertEqual(asked.shown[0][0], [window.T("open"), window.T("move_down"), window.T("server_order")])
        self.assertIsNone(server.orders[-1])
        # Sort libraries, as in the profile's menu.
        with Dialogs(picks=[-1]) as asked:
            page.click(L.LIBRARIES_SORT)
        self.assertEqual(asked.shown[0][0], [lib["name"] for lib in listed] + [window.T("server_order")])
        # The rail's fifth library moved down goes to Libraries, and focus with it.
        with Dialogs(picks=[1]):
            win.library_menu(L.RAIL_LIBRARIES[4])
        self.assertEqual(server.orders[-1], [1, 2, 3, 4, 6, 5])
        self.assertEqual(win.focused, L.RAIL_ALL_LIBRARIES)
        self.assertEqual(cards.items[5].label, "Anime")
        # A scan's counts reach the page; the rail stays as it is.
        win.session.details = {}
        win.session.lock = window.threading.Lock()
        server.listed = [dict(server.listed[0], item_count=1235)] + server.listed[1:]
        win.changed()
        self.assertEqual(cards.items[0].label2, "1,235 movies")
        # Down to five libraries: Collections is back, Libraries gone, and the page marks Home.
        server.listed = server.listed[:5]
        win.changed()
        self.assertEqual(len(cards.items), 5)
        self.assertEqual(page.rail, "home")
        self.assertFalse(win.made[L.rail_group(L.RAIL_ALL_LIBRARIES)].visible)
        self.assertTrue(win.made[L.rail_group(L.RAIL_COLLECTIONS)].visible)
        # Focus never goes to the entry the rail doesn't show.
        self.assertEqual((win.rail_entry("libraries"), win.rail_entry("collections")), (L.RAIL_HOME, L.RAIL_COLLECTIONS))
        win.libraries = listed
        self.assertEqual((win.rail_entry("libraries"), win.rail_entry("collections")), (L.RAIL_ALL_LIBRARIES, L.RAIL_HOME))

    def test_rows_fill_and_place(self):
        win = FakeWindow()
        rows = window.Rows(win, L.HOME_ROWS, L.HOME_SLOTS, ("poster", "landscape", "square"))
        movie = {"id": 5, "kind": "movie", "title": "Saltwind", "user_state": {}}
        rows.fill([
            {"key": "continue_watching", "title": "Continue Watching", "shape": "landscape", "items": [EPISODE]},
            {"key": "recently_added:1", "title": "Recently Added", "shape": "poster", "items": [movie], "see_all": 1},
        ])
        self.assertEqual(win.made[L.slot_header(L.HOME_ROWS, 0)].label, "Continue Watching")
        self.assertEqual(len(win.made[L.slot_list(L.HOME_ROWS, 0, "landscape")].items), 1)
        self.assertFalse(win.made[L.slot_list(L.HOME_ROWS, 0, "poster")].visible)
        # A library's row ends with See all.
        self.assertEqual(len(win.made[L.slot_list(L.HOME_ROWS, 1, "poster")].items), 2)
        self.assertFalse(win.made[L.slot(L.HOME_ROWS, 2)].visible)
        # Under the hero, the rows go down from where it ends…
        rows.place(None, 768)
        self.assertEqual(win.made[L.slot(L.HOME_ROWS, 0)].position, (0, 768))
        landscape = L.row_height(L.SHAPES["landscape"]) + L.ROW_GAP
        self.assertEqual(win.made[L.slot(L.HOME_ROWS, 1)].position, (0, 768 + landscape))
        # …and with a row focused, it sits under the info block, the ones above out of sight.
        rows.place(1, 0)
        self.assertEqual(win.made[L.slot(L.HOME_ROWS, 1)].position, (0, L.FOCUSED_ROW_Y))
        self.assertLess(win.made[L.slot(L.HOME_ROWS, 0)].position[1], 0)
        self.assertEqual(rows.find(L.slot_list(L.HOME_ROWS, 1, "poster"))[0], 1)
        self.assertIsNone(rows.find(L.slot_header(L.HOME_ROWS, 1)))

    def test_the_profiles_menu_opens_discover_and_the_requests(self):
        win = FakeWindow()
        win.libraries = []
        left = []
        win.leave_for = left.append
        # A manager, one of whose requests couldn't be sent: the number in red.
        keep_summary(self, dict(ASKS, access="auto", manage=True, pending=2, failed=1))
        with Dialogs(picks=[1]) as asked:
            win.account()
        red = "[COLOR %s](3)[/COLOR]" % window.requests.RED
        self.assertEqual(asked.shown[0][0], [
            window.T("discover"), window.T("requests_page") + " " + red, window.T("choose_server"), window.T("tracks"), window.T("sign_out"),
        ])
        # Kodi's own list of them, once the window has closed.
        self.assertEqual((left, win.listing), (["listing"], "plugin://plugin.video.pley/?route=requests"))
        # The service has no summary yet: the server is asked, and its answer kept.
        window.kodi.clear_property(window.requests.SUMMARY)
        win.session.client = Client({"/api/requests/summary": dict(ASKS, unseen=2, own=3)})
        with Dialogs(picks=[0]) as asked:
            win.account()
        self.assertEqual(asked.shown[0][0][:2], [window.T("discover"), window.T("your_requests") + " (2)"])
        self.assertEqual(win.listing, "plugin://plugin.video.pley/?route=discover")
        self.assertEqual(json.loads(window.kodi.get_property(window.requests.SUMMARY))["unseen"], 2)
        # An account that may no longer ask keeps its requests.
        keep_summary(self, dict(ASKS, access="off", own=1))
        with Dialogs(picks=[-1]) as asked:
            win.account()
        self.assertEqual(asked.shown[0][0][:2], [window.T("your_requests"), window.T("choose_server")])
        # A server without requests: the menu as it was.
        requests_off(self)
        with Dialogs(picks=[-1]) as asked:
            win.account()
        self.assertEqual(asked.shown[0][0][0], window.T("choose_server"))

    def test_a_show_asks_for_more_seasons_from_its_page(self):
        win = FakeWindow()
        win.later = at_once
        found = {"kind": "show", "tmdb_id": 1396, "status": "partial", "request": None, "askable": [3]}
        asked_of = []

        def get(path, **query):
            asked_of.append(path)
            return found

        win.session.get = get
        page = window.Item(win, 7)
        page.detail = {"id": 7, "kind": "show", "title": "Low Orbit"}
        keep_summary(self, ASKS)
        page.ask_about()
        self.assertEqual(asked_of, ["/api/items/7/request"])
        # More's Request more seasons: the plugin's picker, over the window.
        del window.xbmc.builtins[:]
        dict(page.more_actions())[window.T("request_more_seasons")]()
        self.assertEqual(window.xbmc.builtins, ["RunPlugin(plugin://plugin.video.pley/?route=more_seasons&id=7&title=Low+Orbit)"])
        # Nothing more to ask for: no entry.
        found = dict(found, askable=[])
        page.ask_about()
        self.assertNotIn(window.T("request_more_seasons"), dict(page.more_actions()))
        # A profile that may not ask, or a movie: not even asked.
        found = dict(found, askable=[3])
        keep_summary(self, dict(ASKS, access="off", own=2))
        page.ask_about()
        self.assertNotIn(window.T("request_more_seasons"), dict(page.more_actions()))
        keep_summary(self, ASKS)
        page.detail = {"id": 8, "kind": "movie", "title": "Saltwind"}
        page.ask_about()
        self.assertEqual(len(asked_of), 2)

    def test_search_shows_tmdb_after_the_libraries(self):
        win = FakeWindow()
        later = []
        win.later = lambda work, then, failed=None: later.append((work, then, failed))
        movie = {"id": 5, "kind": "movie", "title": "Saltwind", "user_state": {}}
        tmdb = {"results": [
            {"kind": "movie", "tmdb_id": 949, "title": "Saltwind 2", "year": 2027, "poster_url": "/api/discover/images/poster/h2.jpg", "status": "none"},
            {"kind": "show", "tmdb_id": 1, "title": "Overgrowth", "year": None, "poster_url": None, "status": "requested"},
            {"kind": "show", "tmdb_id": 2, "title": "Parallel Coast", "year": 2019, "poster_url": None, "status": "partial", "item_id": 70},
        ]}
        asked_of = []

        def get(path, **query):
            asked_of.append((path, query))
            return {"items": [movie], "people": []} if path == "/api/search" else tmdb

        win.session.get = get
        keep_summary(self, ASKS)
        page = window.Search(win)
        win.show(page)
        page.search("saltwind")
        # The libraries' rows at once; TMDB is looked at beside them, never waited for.
        self.assertEqual([row["key"] for row in page.rows.rows], [window.T("movies")])
        # Every title, person and channel the server finds, up to 200 of each.
        self.assertEqual(asked_of, [("/api/search", {"q": "saltwind", "limit": 200, "summaries": 1})])
        work, then, _ = later.pop()
        then(work())
        self.assertEqual(asked_of[-1], ("/api/discover/search", {"q": "saltwind", "missing": 1}))
        self.assertEqual([row["title"] for row in page.rows.rows], [window.T("movies"), window.T("discover")])
        row = L.slot_list(L.SEARCH_ROWS, 1, "poster")
        shown = win.made[row].items
        self.assertEqual([(li.label, li.label2) for li in shown], [
            ("Saltwind 2", "2027 · " + window.T("movie")),
            ("Overgrowth", "[COLOR %s]%s[/COLOR]" % (window.requests.BLUE, window.T("requested"))),
            ("Parallel Coast", "[COLOR %s]%s[/COLOR]" % (window.requests.GREEN, window.T("partly_in_library"))),
            # See all, opening Discover's results for the words.
            (window.T("see_all"), ""),
        ])
        # Posters through the server, as library artwork.
        self.assertEqual(shown[0].art["tile"], window.image(BASE, "/api/discover/images/poster/h2.jpg", 320))
        self.assertEqual(win.made[L.slot(L.SEARCH_ROWS, 1)].position, (0, 200 + L.row_height(L.SHAPES["poster"]) + L.ROW_GAP))
        # OK: the title's main action, the plugin's; one partly in a library opens there.
        del window.xbmc.builtins[:]
        page.click(row)
        self.assertEqual(window.xbmc.builtins, ["RunPlugin(plugin://plugin.video.pley/?route=title&kind=movie&tmdb=949)"])
        win.made[row].selectItem(2)
        page.click(row)
        self.assertEqual(win.opened[-1].item_id, 70)
        # Its menu asks for more of it.
        with Dialogs(picks=[0]) as asked:
            page.menu(row)
        self.assertEqual(asked.shown[0][0], [window.T("request_more_seasons")])
        self.assertEqual(window.xbmc.builtins[-1], "RunPlugin(plugin://plugin.video.pley/?route=title&kind=show&tmdb=2&more=1)")
        # See all: Discover's results for the words, in Kodi's lists.
        left = []
        win.leave_for = left.append
        win.made[row].selectItem(3)
        page.click(row)
        self.assertEqual((left, win.listing), (["listing"], "plugin://plugin.video.pley/?route=discover_search&q=saltwind"))
        # Shown again (a title was asked for): the row stays until its titles come, focus
        # on the tile it was on.
        win.setFocusId(row)
        win.made[row].selectItem(1)
        tmdb["results"][0] = dict(tmdb["results"][0], status="requested")
        page.refresh()
        self.assertEqual([row_["key"] for row_ in page.rows.rows], [window.T("movies"), "tmdb"])
        work, then, _ = later.pop()
        then(work())
        self.assertEqual(win.made[row].items[0].label2, "[COLOR %s]%s[/COLOR]" % (window.requests.BLUE, window.T("requested")))
        self.assertEqual(win.made[row].selected, 1)
        # Focused, the row moves up under the field.
        self.assertEqual(win.made[L.slot(L.SEARCH_ROWS, 1)].position, (0, 200))

    def test_on_tmdb_has_a_row_whatever_the_libraries_found(self):
        win = FakeWindow()
        later = []
        win.later = lambda work, then, failed=None: later.append((work, then, failed))
        kinds = ("movie", "show", "episode", "album")
        radio = {"id": 4, "source_id": 1, "name": "Radio Jazz", "number": "104", "group": "Radio", "logo": None, "favorite": False,
                 "has_guide": False, "now": None, "next": None}
        answers = {
            "/api/search": {"items": [{"id": n, "kind": kind, "title": "Saltwind", "user_state": {}} for n, kind in enumerate(kinds, 1)],
                            "people": [{"id": 9, "name": "Lena Okafor"}], "channels": [radio]},
            "/api/discover/search": {"results": [{"kind": "movie", "tmdb_id": 949, "title": "Saltwind 2", "status": "none"}]},
        }
        win.session.get = lambda path, **query: answers[path]
        keep_summary(self, ASKS)
        page = window.Search(win)
        win.show(page)
        page.search("saltwind")
        self.assertEqual(len(page.rows.rows), 6, "a row of each kind the libraries have")
        work, then, _ = later.pop()
        then(work())
        # The titles' rows, then Live TV's and people's, then Discover.
        self.assertEqual([row["key"] for row in page.rows.rows][-3:], ["channels", "people", "tmdb"])
        self.assertEqual(win.made[L.slot_list(L.SEARCH_ROWS, 6, "poster")].items[0].label, "Saltwind 2")

    def test_search_rows_hold_everything_found(self):
        win = FakeWindow()
        requests_off(self)
        movies = [{"id": n, "kind": "movie", "title": "Harbor %d" % n, "user_state": {}} for n in range(150)]
        people = [{"id": n, "name": "Harbor Person %d" % n} for n in range(30)]
        channels = [{"id": n, "source_id": 1, "name": "Harbor %d" % n, "number": str(n), "logo": None, "favorite": False, "has_guide": False, "now": None, "next": None}
                    for n in range(20)]
        win.session.get = lambda path, **query: {"items": movies, "people": people, "channels": channels}
        page = window.Search(win)
        win.show(page)
        page.search("harbor")
        # A row a group, each with all of its results, nothing cut at 12.
        self.assertEqual([(row["key"], len(row["items"])) for row in page.rows.rows], [(window.T("movies"), 150), ("channels", 20), ("people", 30)])
        self.assertEqual(len(win.made[L.slot_list(L.SEARCH_ROWS, 2, "person")].items), 30)
        self.assertEqual(len(win.made[L.slot_list(L.SEARCH_ROWS, 1, "channel")].items), 20)

    def test_search_when_the_libraries_have_nothing(self):
        win = FakeWindow()
        later = []
        win.later = lambda work, then, failed=None: later.append((work, then, failed))
        answers = {"/api/search": {"items": [], "people": []}, "/api/discover/search": {"results": [{"kind": "movie", "tmdb_id": 9, "title": "Halcyon", "status": "none"}]}}
        win.session.get = lambda path, **query: answers[path]
        keep_summary(self, ASKS)
        page = window.Search(win)
        win.show(page)
        page.search("halcyon")
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_in_libraries"))
        work, then, _ = later.pop()
        then(work())
        # Discover under the sentence, focused too.
        self.assertEqual([row["key"] for row in page.rows.rows], ["tmdb"])
        self.assertEqual(win.made[L.slot(L.SEARCH_ROWS, 0)].position, (0, 320))
        page.focused(L.slot_list(L.SEARCH_ROWS, 0, "poster"))
        self.assertEqual(win.made[L.slot(L.SEARCH_ROWS, 0)].position, (0, 320))
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_in_libraries"))
        # An older search's titles come too late.
        page.search("halcyon returns")
        older = later.pop()
        page.search("mask")
        older[1](older[0]())
        self.assertEqual(page.rows.rows, [])
        # Nothing on TMDB either, or TMDB away: nothing found.
        answers["/api/discover/search"] = {"results": []}
        work, then, _ = later.pop()
        then(work())
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_found_for") % "mask")
        page.search("mask")
        later.pop()[2]()
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_found_for") % "mask")
        # A profile that may not ask: TMDB isn't looked at.
        keep_summary(self, dict(ASKS, access="off"))
        page.search("mask")
        self.assertEqual(later, [])
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_found_for") % "mask")

    def test_on_tmdb_says_why_tmdb_refused(self):
        win = FakeWindow()
        later = []
        win.later = lambda work, then, failed=None: later.append((work, then, failed))
        refusal = [window.api.ApiError(503, "tmdb_down", "TMDB timed out", {"setup": True})]
        asked_of = []

        def get(path, **query):
            asked_of.append((path, query))
            if path == "/api/search":
                return {"items": [], "people": []}
            if refusal:
                raise refusal[0]
            return {"results": [{"kind": "movie", "tmdb_id": 9, "title": "Halcyon", "status": "none"}]}

        win.session.get = get
        keep_summary(self, ASKS)
        page = window.Search(win)
        win.show(page)
        page.search("  halcyon " + "x" * 300)
        work, then, _ = later.pop()
        then(work())
        self.assertEqual(asked_of[-1], ("/api/discover/search", {"q": ("halcyon " + "x" * 300)[:200], "missing": 1}), "as the server takes them")
        # The row says why, under "Nothing in your libraries matches", with Try again.
        down = "%s · [COLOR %s]%s[/COLOR]" % (window.T("discover"), L.DANGER_TEXT, window.T("tmdb_down"))
        self.assertEqual([row["title"] for row in page.rows.rows], [down])
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_in_libraries"))
        row = L.slot_list(L.SEARCH_ROWS, 0, "poster")
        self.assertEqual([li.label for li in win.made[row].items], [window.T("try_again")])
        # Try again asks TMDB again; its titles take the row's place.
        refusal.clear()
        page.click(row)
        work, then, _ = later.pop()
        then(work())
        self.assertEqual([row_["title"] for row_ in page.rows.rows], [window.T("discover")])
        self.assertEqual([li.label for li in win.made[row].items], ["Halcyon", window.T("see_all")])
        # A key TMDB refuses: trying again won't help, See all's page says what to do.
        refusal.append(window.api.ApiError(503, "tmdb_key", "401", {"setup": True, "retry": False}))
        page.search("halcyon")
        work, then, _ = later.pop()
        then(work())
        self.assertEqual(page.rows.rows[0]["title"], "%s · [COLOR %s]%s[/COLOR]" % (window.T("discover"), L.DANGER_TEXT, window.T("tmdb_key")))
        self.assertEqual([li.label for li in win.made[row].items], [window.T("see_all")])
        left = []
        win.leave_for = left.append
        page.click(row)
        self.assertEqual((left, win.listing), (["listing"], "plugin://plugin.video.pley/?route=discover_search&q=halcyon"))
        # No answer at all (the service away): the libraries' rows alone.
        page.search("mask")
        later.pop()[2]()
        self.assertEqual(page.rows.rows, [])
        self.assertEqual(win.made[L.SEARCH_EMPTY_TITLE].label, window.T("nothing_found_for") % "mask")


if __name__ == "__main__":
    unittest.main()
