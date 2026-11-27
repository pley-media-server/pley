import unittest

import support  # noqa: F401
from pley import prompts

INTRO = {"file_id": 4, "kind": "intro", "start_ms": 5_000, "end_ms": 35_000, "source": "chapter"}
CREDITS = {"file_id": 4, "kind": "credits", "start_ms": 80_000, "end_ms": 90_000, "source": "chapter"}
NEXT = {"id": 6, "kind": "episode", "title": "Pilot", "index": 2, "parent_index": 1}


class MarkersTest(unittest.TestCase):
    def test_the_button_shows_until_a_second_before_the_end(self):
        markers = [INTRO, CREDITS]
        self.assertIsNone(prompts.active(markers, 4_999))
        self.assertEqual(prompts.active(markers, 5_000), INTRO)
        self.assertEqual(prompts.active(markers, 33_999), INTRO)
        self.assertIsNone(prompts.active(markers, 34_000))
        self.assertEqual(prompts.active(markers, 80_000), CREDITS)

    def test_skipping_lands_where_the_web_player_lands(self):
        self.assertEqual(prompts.target_ms(INTRO), 34_250, "the intro's last moment plays out")
        short = dict(INTRO, start_ms=10_000, end_ms=11_000)
        self.assertEqual(prompts.target_ms(short), 10_500, "half of a short intro at most")
        self.assertEqual(prompts.target_ms(CREDITS), 90_000)

    def test_episodes_are_written_as_the_android_app_writes_them(self):
        self.assertEqual(prompts.episode_line(NEXT), "S1 · E2 · Pilot")
        self.assertEqual(prompts.episode_line({"kind": "movie", "title": "Film"}), "Film")


class ReachesEndTest(unittest.TestCase):
    def test_credits_run_to_the_end_when_less_than_15_seconds_follow(self):
        self.assertTrue(prompts.reaches_end(CREDITS, 90_000))
        self.assertTrue(prompts.reaches_end(CREDITS, 100_000), "10 s after: a logo, not a scene")
        self.assertTrue(prompts.reaches_end(CREDITS, 105_000))
        self.assertFalse(prompts.reaches_end(CREDITS, 105_001))
        self.assertFalse(prompts.reaches_end(CREDITS, 150_000), "a scene in the last minute")
        self.assertFalse(prompts.reaches_end(CREDITS, 0), "a file of unknown length")
        self.assertFalse(prompts.reaches_end(CREDITS, None))
        self.assertFalse(prompts.reaches_end(dict(INTRO, end_ms=90_000), 90_000), "only credits")


class MomentsTest(unittest.TestCase):
    def setUp(self):
        self.now = 0.0
        self.moments = prompts.Moments([INTRO, CREDITS], NEXT, clock=lambda: self.now, duration_ms=90_000)

    def test_an_episodes_credits_bring_up_the_next_one(self):
        self.assertEqual(self.moments.offer(10_000), ("skip", INTRO))
        self.assertIsNone(self.moments.offer(50_000))
        self.assertEqual(self.moments.offer(81_000), ("next", CREDITS))
        self.assertEqual(self.moments.counting(), prompts.COUNTDOWN)
        self.now = 4.5
        self.assertEqual(self.moments.counting(), 6)
        self.now = 30
        self.assertEqual(self.moments.counting(), 0)

    def test_cancelled_up_next_leaves_skip_credits(self):
        self.moments.offer(81_000)
        self.moments.counting()
        self.moments.cancel_next()
        self.assertEqual(self.moments.offer(81_000), ("skip", CREDITS))
        self.assertIsNone(self.moments.next_since)

    def test_a_dismissed_button_stays_away_for_that_segment(self):
        self.moments.offer(6_000)
        self.moments.dismiss(INTRO)
        self.assertIsNone(self.moments.offer(10_000))
        self.assertEqual(self.moments.offer(81_000), ("next", CREDITS))

    def test_a_skip_button_shows_on_its_own_for_eight_seconds(self):
        self.assertEqual(prompts.SKIP_ALONE, 8)
        self.now = 100.0
        self.assertEqual(self.moments.offer(6_000), ("skip", INTRO))
        self.now = 103.0
        self.assertEqual(self.moments.offer(9_000), ("skip", INTRO), "its 8 s start when it appears")
        self.moments.appeared(INTRO)
        self.now = 110.5
        self.moments.appeared(INTRO)
        self.assertEqual(self.moments.offer(16_500), ("skip", INTRO), "and don't start again")
        self.now = 111.0
        self.assertIsNone(self.moments.offer(17_000))
        self.assertIsNone(self.moments.offer(25_000), "a seek inside the intro keeps it away")
        self.assertEqual(self.moments.credits_from(), 80_000)
        self.assertIsNone(prompts.Moments([INTRO]).credits_from())

    def test_entering_a_marker_again_gives_a_fresh_eight_seconds(self):
        self.moments.offer(6_000)
        self.moments.appeared(INTRO)
        self.now = prompts.SKIP_ALONE
        self.assertIsNone(self.moments.offer(14_000))
        self.assertIsNone(self.moments.offer(50_000), "out of the intro")
        self.assertEqual(self.moments.offer(10_000), ("skip", INTRO), "back in: a fresh 8 s")
        self.moments.appeared(INTRO)
        self.now += prompts.SKIP_ALONE - 0.5
        self.assertEqual(self.moments.offer(17_000), ("skip", INTRO))
        self.now += 0.5
        self.assertIsNone(self.moments.offer(18_000))

    def test_a_pressed_skip_comes_back_once_the_marker_is_entered_again(self):
        self.moments.offer(6_000)
        self.moments.dismiss(INTRO)
        self.assertIsNone(self.moments.offer(7_000))
        self.moments.offer(36_000)
        self.assertEqual(self.moments.offer(8_000), ("skip", INTRO), "seeking back into the intro")

    def test_skip_credits_after_a_cancelled_up_next_gets_its_own_eight_seconds(self):
        self.assertEqual(self.moments.offer(81_000), ("next", CREDITS))
        self.now = 30.0
        self.moments.cancel_next()
        self.assertEqual(self.moments.offer(82_000), ("skip", CREDITS))
        self.moments.appeared(CREDITS)
        self.now = 37.5
        self.assertEqual(self.moments.offer(89_000 - 1_500), ("skip", CREDITS))
        self.now = 38.0
        self.assertIsNone(self.moments.offer(88_500))

    def test_skip_credits_shown_before_the_next_episode_was_known_comes_back_after_cancel(self):
        late = prompts.Moments([CREDITS], None, clock=lambda: self.now, duration_ms=90_000)
        self.assertEqual(late.offer(81_000), ("skip", CREDITS))
        late.appeared(CREDITS)
        self.now = 2.0
        late.next_item = NEXT
        self.assertEqual(late.offer(83_000), ("next", CREDITS))
        self.now = 12.0
        late.cancel_next()
        self.assertEqual(late.offer(84_000), ("skip", CREDITS), "8 fresh seconds, though its first began 12 s ago")
        late.appeared(CREDITS)
        self.now = 19.5
        self.assertEqual(late.offer(85_000), ("skip", CREDITS))
        self.now = 20.0
        self.assertIsNone(late.offer(85_500))

    def test_without_a_next_episode_credits_are_skipped(self):
        movie = prompts.Moments([CREDITS], duration_ms=90_000)
        self.assertEqual(movie.offer(85_000), ("skip", CREDITS))

    def test_credits_something_follows_leave_up_next_to_the_end(self):
        scene_after = prompts.Moments([CREDITS], NEXT, duration_ms=150_000)
        self.assertEqual(scene_after.offer(81_000), ("skip", CREDITS), "60 s follow them: Skip Credits")
        self.assertIsNone(scene_after.offer(120_000))
        logo_after = prompts.Moments([CREDITS], NEXT, duration_ms=100_000)
        self.assertEqual(logo_after.offer(81_000), ("next", CREDITS), "10 s follow them")
        unknown = prompts.Moments([CREDITS], NEXT)
        self.assertEqual(unknown.offer(81_000), ("skip", CREDITS), "a file of unknown length")


if __name__ == "__main__":
    unittest.main()
