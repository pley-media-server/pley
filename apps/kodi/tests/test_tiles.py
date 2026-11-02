import unittest

import support  # noqa: F401
from pley import tiles

BASE = "http://127.0.0.1:5000/s"
EPISODE = {
    "id": 31,
    "kind": "episode",
    "title": "Pilot",
    "index": 1,
    "parent_index": 2,
    "parent_id": 30,
    "grandparent_id": 29,
    "grandparent_title": "The Show",
    "duration_ms": 1_800_000,
    "thumb_url": "/api/items/31/images/thumb?v=9",
    "user_state": {"position_ms": 600_000, "watched": False},
}
MOVIE = {
    "id": 5,
    "kind": "movie",
    "title": "Saltwind",
    "year": 1995,
    "duration_ms": 170 * 60_000,
    "poster_url": "/api/items/5/images/poster?v=1",
    "backdrop_url": "/api/items/5/images/backdrop?v=1",
    "thumb_url": "/api/items/5/images/thumb?v=1",
    "user_state": {"position_ms": 0, "watched": True},
}


class TilesTest(unittest.TestCase):
    """As the Android app's tiles (ArtworkTileModel.kt, ArtworkCandidates)."""

    def test_captions_by_shape(self):
        self.assertEqual(tiles.captions(EPISODE, "episode"), ("1. Pilot", "20 min left"))
        self.assertEqual(tiles.captions(EPISODE, "landscape"), ("The Show", "S2 · E1 · 20 min left"))
        self.assertEqual(tiles.captions(MOVIE, "poster"), ("Saltwind", "1995"))
        done = dict(EPISODE, user_state={"position_ms": 0, "watched": True})
        self.assertEqual(tiles.captions(done, "landscape"), ("The Show", "S2 · E1 · Pilot"))

    def test_pictures_by_shape(self):
        # A movie's still is its backdrop (its thumb is a frame grab).
        self.assertEqual(tiles.picture(BASE, MOVIE, "landscape"), BASE + "/api/items/5/images/backdrop?v=1&w=480")
        self.assertEqual(tiles.picture(BASE, MOVIE, "poster"), BASE + "/api/items/5/images/poster?v=1&w=320")
        self.assertEqual(tiles.picture(BASE, EPISODE, "episode"), BASE + "/api/items/31/images/thumb?v=9&w=480")
        # Without a still, an episode shows its show's backdrop; without a
        # poster, its season's.
        bare = dict(EPISODE, thumb_url=None)
        self.assertEqual(tiles.picture(BASE, bare, "landscape"), BASE + "/api/items/29/images/backdrop?w=480")
        self.assertEqual(tiles.picture(BASE, EPISODE, "poster"), BASE + "/api/items/30/images/poster?w=320")

    def test_a_tiles_marks(self):
        li = tiles.tile(BASE, EPISODE, "landscape")
        self.assertEqual((li.label, li.label2), ("The Show", "S2 · E1 · 20 min left"))
        self.assertEqual(li.video.values["setResumePoint"], (600.0, 1800.0))
        self.assertEqual(li.properties["id"], "31")
        self.assertNotIn("state", li.properties)
        watched = tiles.tile(BASE, MOVIE, "poster")
        self.assertEqual(watched.properties["state"], "watched")
        self.assertNotIn("setResumePoint", watched.video.values)
        show = {"id": 30, "kind": "show", "title": "The Show", "leaf_count": 120, "viewed_leaf_count": 3, "user_state": {"watched": False}}
        counted = tiles.tile(BASE, show, "poster")
        self.assertEqual((counted.properties["count"], counted.properties["countsize"]), ("117", "3"))
        self.assertTrue(counted.properties["ph"].startswith("ph-"))

    def test_rows(self):
        self.assertEqual(tiles.row_shape({"key": "continue_watching", "items": [MOVIE]}), "landscape")
        self.assertEqual(tiles.row_shape({"key": "recently_added:2", "items": [EPISODE]}), "landscape")
        self.assertEqual(tiles.row_shape({"key": "recently_added:1", "items": [MOVIE]}), "poster")
        self.assertEqual(tiles.row_shape({"key": "recently_added:3", "items": [MOVIE]}, "music"), "square")
        self.assertEqual(tiles.see_all_of({"key": "recently_added:1", "library_id": 1}), 1)
        self.assertIsNone(tiles.see_all_of({"key": "my_list", "library_id": None}))
        rows = [
            {"key": "my_list", "items": [dict(MOVIE, added_at=5)]},
            {"key": "continue_watching", "items": [EPISODE]},
        ]
        self.assertIs(tiles.hero_of(rows), EPISODE)
        self.assertEqual(tiles.hero_of(rows[:1])["id"], 5)
        self.assertIsNone(tiles.hero_of([]))
        li = tiles.see_all("See all")
        self.assertEqual((li.label, li.properties["seeall"]), ("See all", "1"))


if __name__ == "__main__":
    unittest.main()
