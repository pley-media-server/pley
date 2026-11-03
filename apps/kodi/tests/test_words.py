import unittest

import support  # noqa: F401
from pley import words

FILE = {
    "id": 7,
    "width": 3840,
    "height": 1608,
    "edition": None,
    "streams": [
        {"kind": "video", "dynamic_range": "dolby_vision", "dv_profile": 8, "dv_bl_compat": 1},
        {"kind": "audio", "default": True, "channels": 8, "profile": "Dolby TrueHD + Dolby Atmos"},
    ],
}


class WordsTest(unittest.TestCase):
    """As the Android app and the web say it (Durations.kt, Labels.kt, Media.kt)."""

    def test_runtimes_and_what_is_left(self):
        self.assertEqual(words.runtime(29_000), "29s")
        self.assertEqual(words.runtime(58 * 60_000), "58 min")
        self.assertEqual(words.runtime(112 * 60_000), "1h 52m")
        self.assertEqual(words.runtime(120 * 60_000), "2h")
        self.assertEqual(words.runtime(None), "")
        self.assertEqual(words.time_left(30 * 60_000, 56 * 60_000), "26 min left")
        self.assertEqual(words.time_left(73_000, 90_000), "17s left")
        self.assertEqual(words.time_left(0, None), "")

    def test_card_lines_by_kind(self):
        self.assertEqual(words.card_subtitle({"kind": "movie", "year": 2024}), "2024")
        self.assertEqual(words.card_subtitle({"kind": "show", "child_count": 1}), "1 season")
        self.assertEqual(words.card_subtitle({"kind": "show", "child_count": 3}), "3 seasons")
        self.assertEqual(words.card_subtitle({"kind": "season", "leaf_count": 10}), "10 episodes")
        self.assertEqual(words.card_subtitle({"kind": "episode", "parent_index": 2, "index": 5}), "S2 · E5")
        self.assertEqual(words.card_subtitle({"kind": "album", "parent_title": "Daft Punk", "year": 2001}), "Daft Punk · 2001")
        self.assertEqual(words.episode_long_label(0, 3), "Specials · Episode 3")
        self.assertEqual(words.season_title(None, 2), "Season 2")
        self.assertEqual(words.plural(12345, "movie"), "12,345 movies")

    def test_progress_and_counts(self):
        playing = {"kind": "movie", "duration_ms": 100, "user_state": {"position_ms": 25, "watched": False}}
        self.assertTrue(words.in_progress(playing))
        self.assertEqual(words.progress(playing), 0.25)
        watched = {"kind": "movie", "duration_ms": 100, "user_state": {"position_ms": 25, "watched": True}}
        self.assertFalse(words.in_progress(watched))
        self.assertEqual(words.unwatched({"kind": "show", "leaf_count": 10, "viewed_leaf_count": 4}), 6)
        self.assertEqual(words.unwatched({"kind": "movie", "leaf_count": 10}), 0)

    def test_quality_badges(self):
        self.assertEqual(words.quality_badges(FILE), ["4K", "DV P8.1", "Atmos"])
        stereo = {"width": 1280, "height": 720, "streams": [{"kind": "video"}, {"kind": "audio", "channels": 6}]}
        self.assertEqual(words.quality_badges(stereo), ["720p", "5.1"])
        self.assertEqual(words.quality_badges(None), [])

    def test_a_library_posters_badges(self):
        # The web's mediaBadges: "4K" of the resolutions only, the dynamic range short, no codec.
        uhd = {"width": 3840, "height": 1608, "video_codec": "hevc", "dynamic_range": "dolby_vision"}
        self.assertEqual(words.media_badges(uhd), ["4K", "DV"])
        self.assertEqual(words.media_badges(dict(uhd, width=1920, height=1080, dynamic_range="hdr10plus")), ["HDR+"])
        self.assertEqual(words.media_badges(dict(uhd, dynamic_range="hlg")), ["4K", "HLG"])
        self.assertEqual(words.media_badges(dict(uhd, dynamic_range="sdr")), ["4K"])
        self.assertEqual(words.media_badges({"width": 1280, "height": 720, "dynamic_range": None}), [])
        self.assertEqual(words.media_badges(None), [])

    def test_bitrate_and_size_under_a_poster(self):
        # As the web's mediaLine, formatBitrate and formatBytes write them.
        self.assertEqual(words.media_line({"bitrate": 42_000_000, "size": 25_125_558_682}), "42 Mbps · 23.4 GB")
        self.assertEqual(words.media_line({"bitrate": 8_200_000, "size": None}), "8.2 Mbps")
        self.assertEqual(words.media_line({"bitrate": None, "size": 157_286_400}), "150 MB")
        self.assertEqual(words.media_line({"bitrate": None, "size": None}), "Bitrate unknown")
        self.assertEqual(words.media_line(None), "")
        self.assertEqual(words.bitrate(320_400), "320 kbps")
        self.assertEqual(words.bitrate(12_400_000), "12 Mbps")
        self.assertEqual(words.size(1536), "1.5 KB")
        self.assertEqual(words.size(512), "512 B")
        self.assertEqual(words.size(3 * 1024 ** 4), "3.0 TB")

    def test_dolby_vision_as_the_server_names_it(self):
        dv = {"kind": "video", "dynamic_range": "dolby_vision"}
        # Profile 7's base layer (Blu-ray, compatibility 6) isn't named: never "P7.6".
        self.assertEqual(words.dynamic_range_short(dict(dv, dv_profile=7, dv_bl_compat=6)), "DV P7")
        self.assertEqual(words.dynamic_range_short(dict(dv, dv_profile=7, dv_bl_compat=6, dv_el="fel")), "DV P7 FEL")
        self.assertEqual(words.dynamic_range_short(dict(dv, dv_profile=8, dv_bl_compat=4)), "DV P8.4")
        self.assertEqual(words.dynamic_range_short(dict(dv, dv_profile=5, dv_bl_compat=0)), "DV P5")
        hybrid = {"width": 3840, "height": 2160, "streams": [dict(dv, dv_profile=8, dv_bl_compat=1, hdr10plus=True)]}
        self.assertEqual(words.quality_badges(hybrid), ["4K", "DV P8.1", "HDR10+"])

    def test_an_items_line(self):
        movie = {"kind": "movie", "year": 2024, "duration_ms": 118 * 60_000, "content_rating": "PG-13", "rating": 7.44, "files": [FILE]}
        self.assertEqual(
            words.item_meta(movie),
            [("text", "2024"), ("text", "1h 58m"), ("badge", "PG-13"), ("rating", "7.4"), ("quality", "4K"), ("quality", "DV P8.1"), ("quality", "Atmos")],
        )
        show = {"kind": "show", "year": 2021, "child_count": 2}
        self.assertEqual(words.item_meta(show, [{"index": 1, "year": 2021}, {"index": 2, "year": 2023}]), [("text", "2 seasons"), ("text", "2021-2023")])
        episode = {"kind": "episode", "release_date": "2024-03-05", "duration_ms": 45 * 60_000}
        self.assertEqual(words.item_meta(episode), [("text", "Mar 5, 2024"), ("text", "45 min")])

    def test_homes_line_takes_details_when_in(self):
        item = {"id": 1, "kind": "movie", "year": 2024, "duration_ms": 58 * 60_000}
        self.assertEqual(words.meta(item)["badges"], [])
        found = words.meta(item, {"id": 1, "content_rating": "R", "files": [FILE]})
        self.assertEqual((found["year"], found["content_rating"], found["runtime"]), ("2024", "R", "58 min"))
        self.assertEqual(found["badges"], ["4K", "DV P8.1", "Atmos"])
        # Another item's details don't count.
        self.assertIsNone(words.meta(item, {"id": 2, "content_rating": "R"})["content_rating"])

    def test_placeholders_hue_as_the_web(self):
        # h = h * 31 + code, unsigned, mod 360: "ab" → 97 * 31 + 98 = 3105 → 225.
        self.assertEqual(words.hue("ab"), 225)
        self.assertEqual(words.placeholder("ab"), "ph-15.png")
        self.assertEqual(words.placeholder(""), "ph-00.png")


# 2026-11-01 12:00 UTC: days are whole in any time zone near it.
NOW = 1793534400
DAY = 86400


class WhenTest(unittest.TestCase):
    """Times as the apps say them, which Kodi can't."""

    def test_how_far_away_it_is(self):
        self.assertEqual(words.relative(NOW + 20 * 60, NOW), "in 20 min")
        self.assertEqual(words.relative(NOW + 10, NOW), "in 1 min")
        self.assertEqual(words.relative(NOW + 3600, NOW), "in 1 hour")
        self.assertEqual(words.relative(NOW + 5 * 3600, NOW), "in 5 hours")
        self.assertEqual(words.relative(NOW + DAY, NOW), "tomorrow")
        self.assertEqual(words.relative(NOW + 12 * DAY, NOW), "in 12 days")
        self.assertEqual(words.relative(NOW - 30, NOW), "just now")
        self.assertEqual(words.relative(NOW - 20 * 60, NOW), "20 min ago")
        self.assertEqual(words.relative(NOW - 2 * 3600, NOW), "2 hours ago")
        self.assertEqual(words.relative(NOW - DAY, NOW), "yesterday")
        self.assertEqual(words.relative(NOW - 3 * DAY, NOW), "3 days ago")
        self.assertEqual(words.relative(NOW - 40 * DAY, NOW), "Sep 22, 2026")
        self.assertEqual(words.relative(None, NOW), "")

    def test_days(self):
        self.assertEqual(words.day(NOW + 3 * DAY, NOW), "Nov 4")
        self.assertEqual(words.day("2026-11-04", NOW), "Nov 4")
        self.assertEqual(words.day("2027-01-05", NOW), "Jan 5, 2027")
        self.assertEqual(words.day("soon", NOW), "soon")

    def test_seasons_as_the_web_names_them(self):
        self.assertEqual(words.seasons_label([2]), "Season 2")
        self.assertEqual(words.seasons_label([5, 1, 2, 3]), "Seasons 1–3, 5")
        self.assertEqual(words.seasons_label([3, 1]), "Seasons 1, 3")
        self.assertEqual(words.seasons_label([0, 1, 2]), "Seasons 1–2 and specials")
        self.assertEqual(words.seasons_label([0, 3]), "Season 3 and specials")
        self.assertEqual(words.seasons_label([0]), "Specials")
        self.assertEqual(words.seasons_label(None), "")


if __name__ == "__main__":
    unittest.main()
