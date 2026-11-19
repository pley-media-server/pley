"""Collections by category: the Collections tab's groups and its Category
filter, a collection's page, a Home row of several collections
of one title, and the rows' See all, in Pley's window and in Kodi's lists."""

import unittest
from unittest import mock

import support  # noqa: F401
import xbmcplugin
from pley import api, categories, kodi, plugin, tiles, window
from pley import layout as L
from test_window import Dialogs, FakeWindow

HIGHLIGHTS = {"id": 1, "key": "highlights", "title": "Highlights", "from_label": None, "position": 0}
# Pley's Other, where a new server puts it: just before Box sets.
OTHER = {"id": 4, "key": "other", "title": "Other", "from_label": None, "position": 1}
BOX_SETS = {"id": 2, "key": "box_sets", "title": "Box sets", "from_label": None, "position": 2}
SEASONAL = {"id": 3, "key": None, "title": "Seasonal", "from_label": "Seasonal", "position": 3}
CATEGORIES = [HIGHLIGHTS, OTHER, BOX_SETS, SEASONAL]


def ref(category):
    return {"id": category["id"], "key": category["key"], "title": category["title"]}


def collection(id_, title, category=OTHER, automatic=False, smart_filter=None):
    return {
        "id": id_,
        "library_id": 4,
        "title": title,
        "summary": None,
        "automatic": automatic,
        "smart_filter": smart_filter,
        "category": ref(category),
        "item_count": 3,
        "poster_url": "/api/collections/%d/images/poster" % id_,
    }


RECENT = collection(10, "Recently Added", HIGHLIGHTS, smart_filter="sort=addedAt:desc")
SAGA = collection(11, "Starfall Saga", BOX_SETS, automatic=True)
MINE = collection(12, "Rainy Sundays")
# Of a category made since the categories were read: after the rest.
NEWER = collection(13, "Harvest", {"id": 9, "key": None, "title": "Autumn"})
COLLECTIONS = [RECENT, SAGA, MINE, NEWER]


class GroupsTest(unittest.TestCase):
    """categories.py: the groups and the filters, as the web's Collections tab."""

    def test_groups_follow_the_categories_order_other_included(self):
        found = categories.groups(COLLECTIONS, CATEGORIES)
        # Other where the list puts it, before Box sets; Seasonal has
        # none: left out; the unknown category after the rest.
        self.assertEqual([(categories.key(c), [x["id"] for x in of]) for c, of in found], [(1, [10]), (4, [12]), (2, [11]), (9, [13])])
        self.assertEqual([categories.title(c) for c, _ in found], ["Highlights", "Other", "Box sets", "Autumn"])
        self.assertEqual(categories.groups([], CATEGORIES), [])

    def test_the_category_filter(self):
        self.assertEqual([categories.key(c) for c, _ in categories.shown(COLLECTIONS, CATEGORIES, 2)], [2])
        self.assertEqual([categories.key(c) for c, _ in categories.shown(COLLECTIONS, CATEGORIES, "all")], [1, 4, 2, 9])
        # A category no longer among the groups filters nothing.
        every = categories.groups(COLLECTIONS, CATEGORIES)
        self.assertEqual((categories.chosen(3, every), categories.chosen(2, every), categories.chosen(4, every)), ("all", 2, 4))
        self.assertEqual([categories.parse(v) for v in (None, "all", "4", "x")], ["all", "all", 4, "all"])


class WindowTest(unittest.TestCase):
    def library(self, collections=COLLECTIONS, listed=CATEGORIES):
        win = FakeWindow()
        win.libraries = [{"id": 4, "kind": "movies", "name": "Movies"}, {"id": 8, "kind": "movies", "name": "Kids"}]
        win.me = {}
        answers = {"/api/libraries/4/collections": {"collections": collections}, "/api/collection-categories": {"categories": listed}}
        win.session.get = lambda path, **query: answers[path]
        page = window.Library(win, win.libraries[0], None)
        page.tab = "collections"
        page.load()
        page.draw()
        return win, page

    def test_several_categories_are_rows_with_their_headings(self):
        win, page = self.library()
        self.assertFalse(win.made[L.grid_list(L.LIBRARY_GRID, "grid")].visible)
        headings = [win.made[L.slot_header(L.LIBRARY_ROWS, i)].label for i in range(4)]
        muted = lambda n: "[COLOR %s]%s[/COLOR]" % (L.MUTED, n)  # noqa: E731
        self.assertEqual(headings, ["Highlights  " + muted(1), "Other  " + muted(1), "Box sets  " + muted(1), "Autumn  " + muted(1)])
        self.assertFalse(win.made[L.slot(L.LIBRARY_ROWS, 4)].visible)
        row = L.slot_list(L.LIBRARY_ROWS, 3, "poster")
        self.assertEqual([li.label for li in win.made[row].items], ["Harvest"])
        self.assertEqual(win.made[L.LIBRARY_COUNT].label, kodi.count("collections", 4))
        self.assertEqual(page.first(), L.slot_list(L.LIBRARY_ROWS, 0, "poster"))
        # The chips: Category, not the Library tab's.
        self.assertTrue(win.made[L.LIBRARY_CATEGORY].visible)
        self.assertFalse(win.made[L.LIBRARY_SORT].visible or win.made[L.LIBRARY_SORT_DOWN].visible or win.made[L.LIBRARY_GENRE].visible)
        # The headings stay shown.
        self.assertTrue(all(win.made[L.slot_header(L.LIBRARY_ROWS, i)].visible for i in range(4)))
        # A collection in a row opens its page.
        win.made[row].selected = 0
        opened = []
        win.open = opened.append
        page.click(row)
        self.assertEqual(opened[0].title, "Harvest")
        # Focus on a lower row brings it up.
        page.focused(L.slot_list(L.LIBRARY_ROWS, 2, "poster"))
        self.assertEqual(win.made[L.slot(L.LIBRARY_ROWS, 2)].position, (0, 272))
        self.assertEqual(win.made[L.slot(L.LIBRARY_ROWS, 0)].position[1], -2 * L.HEIGHT)

    def test_a_refresh_keeps_the_row_and_the_collection(self):
        rising = collection(15, "Starfall Rising", BOX_SETS)
        win, page = self.library([RECENT, MINE, SAGA, rising, NEWER])
        # Box sets, the third row: Starfall Saga, Starfall Rising.
        row = L.slot_list(L.LIBRARY_ROWS, 2, "poster")
        win.focused = row
        win.made[row].selected = 1
        page.focused(row)
        # A collection added before the one focused (Starfall Rising).
        origins = collection(14, "Starfall Origins", BOX_SETS)
        win.session.get = lambda path, **query: {"collections": [RECENT, MINE, origins, SAGA, rising, NEWER]} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        self.assertEqual([li.label for li in win.made[row].items], ["Starfall Origins", "Starfall Saga", "Starfall Rising"])
        self.assertEqual((win.focused, win.made[row].selected), (row, 2))
        # The row stays where focus brought it, the rows above out of sight.
        self.assertEqual(win.made[L.slot(L.LIBRARY_ROWS, 2)].position, (0, 272))
        self.assertEqual(win.made[L.slot(L.LIBRARY_ROWS, 0)].position[1], -2 * L.HEIGHT)
        # Its category gone, and one group left: the grid, not the hidden row.
        win.session.get = lambda path, **query: {"collections": [RECENT]} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        self.assertEqual(win.focused, L.grid_list(L.LIBRARY_GRID, "grid"))
        # On the grid, focus stays among the collections shown (not the
        # Library tab's titles).
        snow = collection(16, "Snow Days")
        win.session.get = lambda path, **query: {"collections": [MINE, snow]} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        grid.selected = 1
        win.session.get = lambda path, **query: {"collections": [MINE, snow, collection(17, "Tea Time")]} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        self.assertEqual((win.focused, grid.selected), (L.grid_list(L.LIBRARY_GRID, "grid"), 1))
        # Rows again: off the grid that went.
        win.session.get = lambda path, **query: {"collections": COLLECTIONS} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        self.assertEqual(win.focused, L.slot_list(L.LIBRARY_ROWS, 0, "poster"))

    def test_recommended_keeps_its_row_on_a_refresh(self):
        # Marking a title watched from a row's menu refreshes the page at once.
        win, page = self.library()
        films = [{"id": n, "kind": "movie", "title": "Film %d" % n} for n in range(1, 4)]
        hubs = [{"key": "collection:%d" % n, "title": "Row %d" % n, "library_id": 4, "collection_ids": [n], "items": films} for n in (10, 11, 12)]
        win.session.get = lambda path, **query: {"hubs": hubs}
        page.tab = "recommended"
        page.load()
        page.draw()
        row = L.slot_list(L.LIBRARY_ROWS, 2, "poster")
        win.focused = row
        win.made[row].selected = 3  # its See all
        page.focused(row)
        page.refresh()
        self.assertEqual((win.focused, win.made[row].selected), (row, 3))
        self.assertEqual(win.made[L.slot(L.LIBRARY_ROWS, 2)].position, (0, 200))

    def test_one_group_is_the_grid_without_a_heading(self):
        win, page = self.library([MINE, dict(NEWER, category=ref(OTHER))])
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        self.assertEqual(([li.label for li in grid.items], grid.visible), (["Rainy Sundays", "Harvest"], True))
        self.assertFalse(win.made[L.slot(L.LIBRARY_ROWS, 0)].visible)
        # One group: nothing to choose between, no Category chip.
        self.assertFalse(win.made[L.LIBRARY_CATEGORY].visible or win.made[L.LIBRARY_CATEGORY_ON].visible)
        opened = []
        win.open = opened.append
        grid.selected = 1
        page.click(L.grid_list(L.LIBRARY_GRID, "grid"))
        self.assertEqual(opened[0].title, "Harvest")

    def test_more_categories_than_rows_are_one_grid(self):
        many = [{"id": 100 + n, "key": None, "title": "Shelf %d" % n, "from_label": None, "position": n} for n in range(L.LIBRARY_SLOTS + 1)]
        found = [collection(200 + n, "Set %02d" % (L.LIBRARY_SLOTS - n), many[n]) for n in range(len(many))]
        win, page = self.library(found, many)
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        self.assertEqual(len(grid.items), len(many))
        self.assertEqual(grid.items[0].label, "Set 00")
        self.assertFalse(win.made[L.slot(L.LIBRARY_ROWS, 0)].visible)
        # A category picked: its own.
        with Dialogs(picks=[2]):
            page.click(L.LIBRARY_CATEGORY)
        self.assertEqual([li.label for li in grid.items], ["Set %02d" % (L.LIBRARY_SLOTS - 1)])

    def test_a_category_deleted_since_it_was_chosen_lets_go(self):
        win, page = self.library()
        with Dialogs(picks=[3]):
            page.click(L.LIBRARY_CATEGORY)
        self.assertEqual(win.made[L.LIBRARY_CATEGORY_ON].label, window.ICON_ROOM + "Box sets")
        # Box sets deleted: its collection is under Other now.
        moved = dict(SAGA, category=ref(OTHER))
        win.session.get = lambda path, **query: {"collections": [RECENT, moved, MINE, NEWER]} if "libraries" in path else {"categories": [HIGHLIGHTS, OTHER, SEASONAL]}
        page.refresh()
        self.assertEqual(page.category, categories.ALL)
        self.assertTrue(win.made[L.LIBRARY_CATEGORY].visible)
        self.assertFalse(win.made[L.LIBRARY_CATEGORY_ON].visible)
        self.assertEqual(win.made[L.slot_header(L.LIBRARY_ROWS, 1)].label.split("  ")[0], "Other")

    def test_all_again_with_one_category_left_moves_focus_to_the_collections(self):
        win, page = self.library()
        with Dialogs(picks=[1]):
            page.click(L.LIBRARY_CATEGORY)
        # Only Highlights has collections now; chosen, its chip stays.
        win.session.get = lambda path, **query: {"collections": [RECENT]} if "libraries" in path else {"categories": CATEGORIES}
        page.refresh()
        self.assertTrue(win.made[L.LIBRARY_CATEGORY_ON].visible)
        # All again: nothing left to choose between, the chip goes, and
        # focus goes to the collections rather than a hidden chip.
        with Dialogs(picks=[0]):
            page.click(L.LIBRARY_CATEGORY_ON)
        self.assertFalse(win.made[L.LIBRARY_CATEGORY].visible or win.made[L.LIBRARY_CATEGORY_ON].visible)
        self.assertEqual(win.focused, L.grid_list(L.LIBRARY_GRID, "grid"))
        self.assertTrue(win.made[win.focused].visible)

    def test_a_server_from_before_categories(self):
        # It has no categories (404) and its collections name none: one grid,
        # as before, and the tab stays open.
        old = [{k: v for k, v in c.items() if k != "category"} for c in (SAGA, MINE)]
        win = FakeWindow()
        win.libraries = [{"id": 4, "kind": "movies", "name": "Movies"}]
        win.me = {}

        def get(path, **query):
            if path == "/api/collection-categories":
                raise api.ApiError(404, "not_found", "no such endpoint")
            return {"collections": old}

        win.session.get = get
        page = window.Library(win, win.libraries[0], None)
        page.tab = "collections"
        page.load()
        page.draw()
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        self.assertEqual(([li.label for li in grid.items], grid.visible), (["Starfall Saga", "Rainy Sundays"], True))
        self.assertFalse(win.made[L.LIBRARY_CATEGORY].visible or win.made[L.LIBRARY_CATEGORY_ON].visible)
        # A collection's page names no category.
        self.assertEqual(window.collection_line(dict(old[0], summary="Every film.")), "Every film.")

        # Another refusal still says what went wrong.
        def broken(path, **query):
            raise api.ApiError(500, "internal", "broken")

        win.session.get = broken
        with self.assertRaises(api.ApiError):
            page.load()

    def test_no_collections_no_chips(self):
        win, page = self.library([])
        for chip in (L.LIBRARY_CATEGORY, L.LIBRARY_CATEGORY_ON):
            self.assertFalse(win.made[chip].visible, chip)
        self.assertEqual(win.made[L.LIBRARY_EMPTY].label, kodi.text("no_collections"))

    def test_category(self):
        win, page = self.library()
        # All categories, then each with collections, Other where it is.
        with Dialogs(picks=[1]) as dialogs:
            page.click(L.LIBRARY_CATEGORY)
        self.assertEqual(dialogs.shown[0], ([kodi.text("all_categories"), "Highlights", "Other", "Box sets", "Autumn"], 0))
        grid = win.made[L.grid_list(L.LIBRARY_GRID, "grid")]
        self.assertEqual(([li.label for li in grid.items], grid.visible), (["Recently Added"], True))
        self.assertEqual(win.made[L.LIBRARY_CATEGORY_ON].label, window.ICON_ROOM + "Highlights")
        self.assertEqual(win.focused, L.LIBRARY_CATEGORY_ON)
        self.assertEqual(win.made[L.LIBRARY_COUNT].label, kodi.count("collections", 1))
        # All again: the rows.
        with Dialogs(picks=[0]):
            page.click(L.LIBRARY_CATEGORY_ON)
        self.assertFalse(grid.visible)
        self.assertEqual(win.focused, L.LIBRARY_CATEGORY)
        # Back on the Library tab, its own chips.
        page.tab = "all"
        page.items, page.total = [], 0
        page.draw()
        self.assertFalse(win.made[L.LIBRARY_CATEGORY].visible or win.made[L.LIBRARY_CATEGORY_ON].visible)
        self.assertTrue(win.made[L.LIBRARY_GENRE].visible)


class PagesTest(unittest.TestCase):
    def test_a_collection_says_its_category(self):
        win = FakeWindow()
        found = dict(SAGA, summary="Every film of the saga.", items=[])
        win.session.get = lambda path, **query: found
        page = window.Grid.collection(win, {"id": 11})
        page.load()
        self.assertEqual((page.title, page.subtitle), ("Starfall Saga", "Box sets · Every film of the saga."))
        # Other says nothing.
        self.assertEqual(window.collection_line(MINE), "")

    def test_a_merged_row_opens_its_collections_together(self):
        win = FakeWindow()
        win.libraries = [{"id": 4, "kind": "movies", "name": "Movies"}, {"id": 8, "kind": "movies", "name": "Kids"}]
        asked = []
        items = [{"id": 1, "kind": "movie", "title": "A"}, {"id": 2, "kind": "movie", "title": "B"}]

        def get(path, **query):
            asked.append((path, query))
            return {"title": "Christmas", "collections": [{"id": 20, "library_id": 4}, {"id": 21, "library_id": 8}], "items": items}

        win.session.get = get
        opened = []
        win.open = opened.append
        # Its See all: the merged page.
        row = {"key": "collections:20,21", "title": "Christmas", "library_id": None, "collection_ids": [20, 21], "items": items}
        self.assertEqual(tiles.see_all_of(row), {"collection_ids": [20, 21]})
        rows = window.Rows(win, L.HOME_ROWS, L.HOME_SLOTS, ("poster",))
        rows.fill([{"key": row["key"], "title": "Christmas", "shape": "poster", "items": items, "see_all": tiles.see_all_of(row)}])
        list_id = L.slot_list(L.HOME_ROWS, 0, "poster")
        self.assertEqual(win.made[list_id].items[-1].label, window.T("see_all"))
        win.made[list_id].selected = 2
        win.open_item(rows.selected(list_id)[2])
        page = opened[0]
        page.load()
        self.assertEqual(asked, [("/api/collections/merged", {"ids": "20,21", "summaries": 1})])
        self.assertEqual((page.title, page.subtitle), ("Christmas", "%s · Movies · Kids" % kodi.count("titles", 2)))
        self.assertEqual([e["id"] for e in page.entries], [1, 2])
        # One collection's row: its page.
        one = {"key": "recently_added:4", "title": "Recently Added", "library_id": 4, "collection_ids": [10], "items": items}
        self.assertEqual(tiles.see_all_of(one), {"collection_ids": [10]})
        rows.fill([{"key": one["key"], "title": one["title"], "shape": "poster", "items": items, "see_all": tiles.see_all_of(one)}])
        win.open_item(rows.selected(list_id)[2])
        self.assertIsInstance(opened[1], window.Grid)
        self.assertEqual(opened[1].title, "Recently Added")
        win.session.get = lambda path, **query: dict(RECENT, items=[]) if path == "/api/collections/10" else None
        opened[1].load()
        self.assertEqual(opened[1].subtitle, "Highlights")


class ListsTest(unittest.TestCase):
    """Kodi's lists: the same, as folders."""

    def setUp(self):
        kodi.set_property(kodi.PROXY, "http://127.0.0.1:5000/s3cret")
        self.addCleanup(kodi.clear_property, kodi.PROXY)
        xbmcplugin.calls.clear()
        self.asked = []
        self.answers = answers = {
            "/api/libraries/4/collections": {"collections": COLLECTIONS},
            "/api/collection-categories": {"categories": CATEGORIES},
            "/api/collections/merged": {"title": "Christmas", "collections": [{"id": 20, "library_id": 4}, {"id": 21, "library_id": 8}], "items": []},
            "/api/collections/11": dict(SAGA, items=[]),
            "/api/collections/12": dict(MINE, items=[]),
            "/api/libraries": [{"id": 4, "name": "Movies"}, {"id": 8, "name": "Kids"}],
        }
        test = self

        class Client:
            def get(self, path, **query):
                test.asked.append((path, query))
                if path not in answers:
                    raise api.ApiError(404, "not_found", "no such endpoint")
                return answers[path]

        patch = mock.patch.object(plugin.Plugin, "client", return_value=Client())
        patch.start()
        self.addCleanup(patch.stop)

    def listed(self, query):
        xbmcplugin.calls.clear()
        listing = plugin.Plugin(["plugin://plugin.video.pley/", "1", "?" + query])
        listing.proxy_base = "http://127.0.0.1:5000/s3cret"
        listing.run()
        return [(c[2].label, c[1]) for c in xbmcplugin.calls if c[0] == "item"]

    def test_categories_are_folders(self):
        found = self.listed("route=collections&id=4")
        base = "plugin://plugin.video.pley/?route=collections&id=4&category="
        self.assertEqual(found, [("Highlights", base + "1"), ("Other", base + "4"), ("Box sets", base + "2"), ("Autumn", base + "9")])
        # A category: its collections.
        self.assertEqual([label for label, _ in self.listed("route=collections&id=4&category=4")], ["Rainy Sundays"])
        self.assertIn(("category", "Other"), xbmcplugin.calls)
        # A category gone since: every one's folders.
        self.assertEqual(len(self.listed("route=collections&id=4&category=3")), 4)

    def test_a_server_from_before_categories_lists_its_collections(self):
        del self.answers["/api/collection-categories"]
        self.answers["/api/libraries/4/collections"] = {"collections": [{k: v for k, v in c.items() if k != "category"} for c in COLLECTIONS]}
        self.assertEqual([label for label, _ in self.listed("route=collections&id=4")], ["Recently Added", "Starfall Saga", "Rainy Sundays", "Harvest"])
        self.assertIn(("end", True), xbmcplugin.calls)

    def test_a_collection_and_a_merged_row(self):
        self.listed("route=collection&id=11")
        self.assertIn(("category", "Starfall Saga · Box sets"), xbmcplugin.calls)
        # Other only says it has none: not named.
        self.listed("route=collection&id=12")
        self.assertIn(("category", "Rainy Sundays"), xbmcplugin.calls)
        found = self.listed("route=collection&ids=20,21")
        self.assertEqual(
            found,
            [
                (kodi.text("collection_in_library") % "Movies", "plugin://plugin.video.pley/?route=collection&id=20"),
                (kodi.text("collection_in_library") % "Kids", "plugin://plugin.video.pley/?route=collection&id=21"),
            ],
        )
        self.assertIn(("/api/collections/merged", {"ids": "20,21", "summaries": 1}), self.asked)
        self.assertIn(("category", "Christmas"), xbmcplugin.calls)


if __name__ == "__main__":
    unittest.main()
