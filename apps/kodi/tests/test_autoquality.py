import unittest

import support  # noqa: F401
from pley import autoquality as aq
from pley.autoquality import Meter, Watch
from pley.kodi import text as T


class MenuTest(unittest.TestCase):
    def test_the_menu_in_its_order(self):
        # docs/QUALITY.md: the two 4K qualities first, then 1080p and below.
        self.assertEqual(aq.IDS, ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"])
        self.assertEqual(
            [aq.label(q, T) for q in aq.IDS],
            ["Original", "Automatic", "30 Mbps 4K", "15 Mbps 4K", "20 Mbps 1080p", "12 Mbps 1080p", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p"],
        )
        self.assertEqual(aq.limits("15m"), (15000, 2160))
        self.assertEqual(aq.limits("auto"), (None, None))
        self.assertEqual(aq.limits("original"), (None, None))

    def test_automatic_unless_a_quality_was_chosen(self):
        self.assertEqual(aq.from_setting(""), "auto")
        self.assertEqual(aq.from_setting("nonsense"), "auto")
        self.assertEqual(aq.from_setting("original"), "original")
        self.assertEqual(aq.from_setting("8m"), "8m")

    def test_a_choice_made_in_the_old_order_keeps_its_meaning(self):
        # The old order: Original, 30m, 20m, 15m, 12m, 8m, 4m, 2m.
        self.assertEqual([aq.from_old_index(i) for i in range(8)], [None, "30m", "20m", "15m", "12m", "8m", "4m", "2m"])
        self.assertIsNone(aq.from_old_index(8))
        self.assertIsNone(aq.from_old_index(-1))

    def test_a_file_is_offered_the_qualities_that_make_it_smaller(self):
        self.assertEqual(aq.offered(10_000, 1080), ["8m", "4m", "2m"])
        self.assertEqual(aq.offered(18_000, 2160), ["20m", "15m", "12m", "8m", "4m", "2m"], "1080p is fewer lines")
        self.assertEqual(aq.offered(20_000, 1080), ["15m", "12m", "8m", "4m", "2m"])
        self.assertEqual(aq.offered(None, None), ["30m", "20m", "15m", "12m", "8m", "4m", "2m"])
        self.assertEqual(aq.offered(1_500, 480), [])

    def test_the_menu_a_file_offers_as_every_app_does(self):
        # The server's cases (playback.rs, offered_qualities_are_the_players_menu),
        # which it lists to remotes.
        mbps = lambda m: m * 1_000_000  # noqa: E731
        self.assertEqual(aq.menu_for(1920, 1080, mbps(9)), ["original", "auto", "4m", "2m"])
        self.assertEqual(aq.menu_for(1920, 1080, mbps(10)), ["original", "auto", "8m", "4m", "2m"])
        self.assertEqual(aq.menu_for(1280, 720, mbps(3)), ["original", "auto", "2m"])
        self.assertEqual(aq.menu_for(3840, 2160, mbps(6)), ["original", "auto", "20m", "12m", "8m", "4m", "2m"])
        self.assertEqual(aq.menu_for(3840, 2160, mbps(60)), aq.IDS, "a 4K file keeps the table's order")
        self.assertEqual(aq.menu_for(3840, 2160, mbps(20)), ["original", "auto", "15m", "20m", "12m", "8m", "4m", "2m"])
        self.assertEqual(aq.menu_for(1920, 800, mbps(25)), ["original", "auto", "20m", "15m", "12m", "8m", "4m", "2m"])
        self.assertEqual(aq.menu_for(None, None, None), aq.IDS, "every one, when nothing is known")

    def test_the_pick_a_file_plays(self):
        offered = ["original", "auto", "4m", "2m"]
        self.assertEqual(aq.picked("auto", offered), "auto")
        self.assertEqual(aq.picked("4m", offered), "4m")
        self.assertEqual(aq.picked("30m", offered), "original", "a quality the file doesn't offer plays the original")

    def test_where_a_remembered_ceiling_starts(self):
        self.assertIsNone(aq.start_quality(None))
        self.assertEqual(aq.start_quality(9_000), "8m")
        self.assertEqual(aq.start_quality(22_000), "20m")
        self.assertEqual(aq.start_quality(500), "2m", "never below 2 Mbps 480p")

    def test_a_ceiling_is_remembered_for_30_minutes(self):
        value = aq.remember_ceiling(6_000, 1_000_000)
        self.assertEqual(aq.remembered_ceiling(value, 1_000_000 + 29 * 60), 6_000)
        self.assertIsNone(aq.remembered_ceiling(value, 1_000_000 + 31 * 60))
        self.assertIsNone(aq.remembered_ceiling("", 1_000_000))
        self.assertIsNone(aq.remembered_ceiling("junk", 1_000_000))

    def test_the_paths_of_a_playback(self):
        self.assertEqual(aq.media_prefix("/api/playback/abc/master.m3u8"), "/api/playback/abc/")
        self.assertEqual(aq.media_prefix("/api/files/7/stream?x=1"), "/api/files/7/stream")


class StepTest(unittest.TestCase):
    def test_down_from_the_original_by_what_the_network_carried(self):
        # 40 Mbps arriving at 0.5 of real time: 20 Mbps carried, 15 with the margin.
        self.assertEqual(aq.step_down("original", 40_000, 0.5, None, 40_000, 2160, None), ("15m", 15_000))
        # A 1080p file at 10 Mbps, delivered at 0.9: 6.75 Mbps.
        self.assertEqual(aq.step_down("original", 10_000, 0.9, None, 10_000, 1080, None), ("4m", 6_750))

    def test_never_above_the_servers_own_quality(self):
        quality, _ = aq.step_down("original", 60_000, 0.95, None, 60_000, 2160, "15m")
        self.assertEqual(quality, "15m")

    def test_always_lower_and_never_below_2_mbps(self):
        # Delivery that looks fine still goes a quality down.
        self.assertEqual(aq.step_down("8m", 8_000, 1.4, None, 20_000, 1080, None)[0], "4m")
        self.assertEqual(aq.step_down("4m", 4_000, 0.1, None, 20_000, 1080, None)[0], "2m")
        self.assertEqual(aq.step_down("2m", 2_000, 0.1, None, 20_000, 1080, None), (None, None))

    def test_without_a_delivery_measure(self):
        # What the proxy saw arrive.
        self.assertEqual(aq.step_down("original", 20_000, None, 9_000, 20_000, 1080, None)[0], "4m")
        self.assertEqual(aq.step_down("original", 20_000, None, None, 20_000, 1080, None), ("4m", 7_500))
        # Not even the file's bitrate: a middling quality.
        self.assertEqual(aq.step_down("original", None, None, None, None, None, None)[0], "8m")

    def test_up_one_quality_at_a_time_then_the_original(self):
        self.assertEqual(aq.step_up("4m", 25_000, 2160, None), ("8m", 8000))
        self.assertEqual(aq.step_up("20m", 25_000, 2160, None), ("original", 25_000))
        # Under the server's own quality, the original is what it converts to.
        self.assertEqual(aq.step_up("12m", 60_000, 2160, "15m"), ("15m", 15000))
        self.assertEqual(aq.step_up("15m", 60_000, 2160, "15m"), ("original", 15000))
        self.assertEqual(aq.step_up("original", 25_000, 2160, None), (None, None))
        self.assertEqual(aq.step_up("30m", None, None, None), (None, None), "unknown file: can't measure against it")


class Kodi:
    """A playback for a Watch: a second at a time, playing, waiting or seeking."""

    def __init__(self, watch, now=0.0, position=0.0):
        self.watch = watch
        self.now = now
        self.position = position
        self.ahead = None
        self.arriving = None
        self.carried = None
        self.shown = None
        # What the watch said: (when, the quality restarted at, or the
        # bitrate a stream goes on at, or "next <quality>"), and its steps.
        self.said = []
        self.steps = []

    def second(self, moves=1.0, caching=False, paused=False, seeking=False, ahead=None):
        self.now += 1
        self.position += moves
        if ahead is not None:
            self.ahead = ahead
        step = self.watch.tick(self.now, self.position, paused, seeking, caching, self.ahead, self.arriving, self.carried, self.shown)
        if step:
            self.steps.append(step)
            if step.kind == aq.BITRATE:
                self.said.append((self.now, step.kbps))
            elif step.kind == aq.NEXT:
                self.said.append((self.now, "next " + step.quality))
            else:
                self.said.append((self.now, step.quality))
        return step

    def play(self, seconds):
        for _ in range(seconds):
            self.second()

    def wait(self, seconds):
        for _ in range(seconds):
            self.second(0.0, caching=True)

    def short_waits(self):
        """Two waits of a second, 20 s apart."""
        self.second(0.0, caching=True)
        self.play(20)
        self.second(0.0, caching=True)
        self.play(1)


def watch(quality="original", playing=20_000, file=20_000, lines=1080, cap=None, lowered=False):
    return Watch(0.0, quality, playing, file, lines, cap, lowered)


def live(quality="8m", playing=8_000, file=20_000, file_lines=1080, lines=1080, cap=None, lowered=False):
    """A conversion whose bitrate changes as it is sent (live_bitrate)."""
    return Watch(0.0, quality, playing, file, file_lines, cap, lowered, live=True, lines=lines)


class WatchTest(unittest.TestCase):
    def test_a_long_wait_moves_the_original_to_a_conversion(self):
        kodi = Kodi(watch())
        # The proxy measured 16 Mbps over the last 2 minutes: under 1.5 times the file's 20.
        kodi.shown = 16_000
        kodi.play(30)
        kodi.wait(1)
        self.assertEqual(kodi.said, [])
        kodi.wait(1)
        self.assertEqual(kodi.said, [(32, "4m")], "2 s: half with the margin, without a measure")
        self.assertEqual(kodi.steps[0], aq.Step(aq.RESTART, "4m", None, True, 7_500))
        kodi.wait(5)
        self.assertEqual(len(kodi.said), 1, "one wait, one step")

    def test_the_original_plays_on_through_waits_on_a_network_that_carries_it(self):
        # 30 Mbps, 1.5 times the file's 20: the server is why it waits.
        w = watch()
        kodi = Kodi(w)
        kodi.shown = 30_000
        kodi.play(30)
        kodi.wait(2)
        kodi.play(20)
        kodi.wait(8)
        kodi.play(30)
        self.assertEqual(kodi.said, [])
        self.assertEqual(w.issue_at, 0, "neither wait an issue, nor the two of them short waits")

    def test_a_slow_answer_as_a_wait_ends_shows_the_network_to_blame(self):
        kodi = Kodi(watch())
        kodi.play(30)
        kodi.wait(3)
        self.assertEqual(kodi.said, [], "nothing measured yet")
        kodi.shown = 12_000
        kodi.play(1)
        self.assertEqual(kodi.said, [(34, "4m")])

    def test_a_direct_play_moves_at_the_second_wait_within_5_minutes(self):
        # Nothing measures the network: the proxy passes the file on in one answer.
        kodi = Kodi(watch())
        kodi.play(30)
        kodi.wait(4)
        kodi.play(60)
        self.assertEqual(kodi.said, [])
        kodi.wait(2)
        self.assertEqual(kodi.said, [(96, "4m")])
        apart = Kodi(watch())
        apart.play(30)
        apart.wait(3)
        apart.play(300)
        apart.wait(3)
        self.assertEqual(apart.said, [], "5 minutes apart")

    def test_the_original_converted_moves_on_any_long_wait(self):
        # Converted at the server's own quality: a lower one is less for it to make.
        kodi = Kodi(Watch(0.0, "original", 15_000, 40_000, 2160, "15m", converted=True))
        kodi.shown = 80_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said, [(32, "4m")])

    def test_two_short_waits_dont_move_the_original(self):
        # The picture plays on: another stream would blank the screen.
        w = watch()
        kodi = Kodi(w)
        kodi.play(30)
        kodi.short_waits()
        kodi.play(30)
        self.assertEqual(kodi.said, [])
        self.assertEqual(w.issue_at, 53, "still an issue: the calm starts again")

    def test_slow_delivery_doesnt_move_the_original(self):
        kodi = Kodi(watch())
        kodi.second(ahead=9.0)
        kodi.play(5)
        for _ in range(40):
            kodi.second(ahead=max(kodi.ahead - 0.2, 0.5))
        self.assertEqual(kodi.said, [])

    def test_the_start_a_seek_and_a_pause_are_not_the_network(self):
        kodi = Kodi(watch())
        kodi.wait(10)
        kodi.play(30)
        # A seek Kodi said it made, then its wait.
        kodi.second(0.0, seeking=True)
        kodi.wait(6)
        # A jump Kodi didn't say it made (a skip), then its wait.
        kodi.play(5)
        kodi.second(120.0)
        kodi.wait(6)
        # Paused.
        kodi.play(5)
        for _ in range(10):
            kodi.second(0.0, paused=True)
        kodi.wait(1)
        kodi.watch.seeked()
        kodi.wait(5)
        self.assertEqual(kodi.said, [])

    def test_plenty_ahead_or_steady_is_fine(self):
        kodi = Kodi(live())
        kodi.second(ahead=30.0)
        for _ in range(60):
            kodi.second(ahead=kodi.ahead - 0.2)
        self.assertEqual(kodi.said, [], "more than 10 s ahead")
        kodi = Kodi(live())
        for _ in range(60):
            kodi.second(ahead=4.0)
        self.assertEqual(kodi.said, [], "a full cache, read as it plays")

    def test_again_but_not_within_20_seconds(self):
        w = watch()
        kodi = Kodi(w)
        # 12 Mbps arriving through the proxy: 9 with the margin.
        kodi.arriving = 12_000
        kodi.shown = 12_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1][1], "8m")
        self.assertEqual(w.ceiling, 9_000)
        # A server whose conversions keep their bitrate: another stream again.
        w.restarted(kodi.now, "8m", 8_000)
        kodi.play(5)
        kodi.wait(3)
        self.assertEqual(len(kodi.said), 1, "within 20 s of the change")
        kodi.play(20)
        kodi.arriving = 100_000
        kodi.wait(2)
        self.assertEqual(kodi.said[-1][1], "4m", "one quality at least")

    def test_no_measure_stays_down(self):
        kodi = Kodi(watch("8m", 8_000, 20_000, 1080, lowered=True))
        kodi.play(1200)
        self.assertEqual(kodi.said, [])


class LiveBitrateTest(unittest.TestCase):
    """A conversion whose bitrate changes as it is sent: a step is a PUT
    of its bitrate, not another stream (docs/QUALITY.md, "Players that go
    black between streams")."""

    def test_a_long_wait_lowers_the_bitrate_and_keeps_the_size(self):
        w = live()
        kodi = Kodi(w)
        # 6 Mbps arriving: 4.5 with the margin, 4 in whole Mbps.
        kodi.arriving = 6_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.steps, [aq.Step(aq.BITRATE, "8m", 4_000, True, 4_500)])
        self.assertEqual((w.quality, w.lines, w.playing_kbps), ("8m", 1080, 4_000), "the same stream, the same size")
        self.assertEqual(w.label(T), "4 Mbps 1080p")
        self.assertTrue(w.lowered())

    def test_never_below_its_sizes_floor(self):
        w = live()
        kodi = Kodi(w)
        kodi.arriving = 6_000
        kodi.play(30)
        kodi.wait(2)
        kodi.play(20)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1][1], 3_000, "4 × 0.75 is 3: 3 Mbps for 1080p")
        kodi.play(30)
        kodi.wait(5)
        self.assertEqual(len(kodi.said), 2, "at the floor the network waits")
        # 4K goes no lower than 8 Mbps.
        uhd = live("15m", 15_000, 40_000, 2160, 2160)
        kodi = Kodi(uhd)
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.steps, [aq.Step(aq.BITRATE, "15m", 8_000, True, 8_000)], "half, then the margin: 5.6, under 8")
        self.assertEqual(uhd.label(T), "8 Mbps 4K")

    def test_two_short_waits_and_slow_delivery_lower_it(self):
        kodi = Kodi(live())
        kodi.play(30)
        kodi.short_waits()
        self.assertEqual(kodi.said, [(53, 3_000)])
        kodi = Kodi(live("12m", 12_000))
        kodi.second(ahead=9.0)
        kodi.play(5)
        # 0.8 s of video arriving each second: 0.2 s less ahead every second.
        for _ in range(30):
            kodi.second(ahead=kodi.ahead - 0.2)
            if kodi.said:
                break
        # 12 Mbps at 0.8 with the margin: 7.2.
        self.assertEqual(kodi.said[0][1], 7_000)
        self.assertLess(kodi.said[0][0], 30)

    def test_back_up_one_step_at_a_time_to_its_own(self):
        w = live("20m", 20_000, 25_000, 2160)
        kodi = Kodi(w)
        kodi.arriving = 8_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said, [(32, 6_000)])
        kodi.carried = 34_000
        kodi.play(299)
        self.assertEqual(len(kodi.said), 1, "not within 5 minutes of a step down")
        kodi.play(1)
        self.assertEqual(kodi.steps[-1], aq.Step(aq.BITRATE, "20m", 8_000, False, 8_000), "the menu's next bitrate up")
        kodi.play(180)
        self.assertEqual(kodi.said[-1], (512, 12_000), "3 minutes on")
        kodi.play(180)
        self.assertEqual(kodi.said[-1], (692, 15_000))
        kodi.play(180)
        self.assertEqual(kodi.said[-1], (872, 20_000), "34 Mbps carries 1.6 times 20 Mbps")
        self.assertEqual(kodi.steps[-1].memory, 20_000, "the quality's own")
        self.assertEqual(w.label(T), "20 Mbps 1080p")
        kodi.play(600)
        self.assertEqual(len(kodi.said), 5, "never above its own")

    def test_back_at_the_original_converted_forgets_the_ceiling(self):
        w = Watch(0.0, "original", 15_000, 40_000, 2160, "15m", live=True, lines=2160, converted=True)
        kodi = Kodi(w)
        kodi.arriving = 15_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said, [(32, 11_000)])
        self.assertEqual(w.label(T), "11 Mbps 4K")
        kodi.carried = 30_000
        kodi.play(300)
        self.assertEqual(kodi.steps[-1], aq.Step(aq.BITRATE, "original", 12_000, False, 12_000))
        kodi.play(180)
        self.assertEqual(kodi.steps[-1], aq.Step(aq.BITRATE, "original", 15_000, False, 0), "its own: no limit")
        self.assertEqual(w.label(T), "Original")
        self.assertFalse(w.lowered())

    def test_the_server_says_what_it_goes_on_at(self):
        w = live()
        kodi = Kodi(w)
        kodi.play(30)
        kodi.wait(2)
        w.answered(3_000, 3_500)
        self.assertEqual(w.playing_kbps, 3_500)

    def test_refused_it_moves_as_a_stream_that_cant_change(self):
        w = live()
        kodi = Kodi(w)
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said, [(32, 3_000)])
        # A transcoder from before: 400.
        w.answered(3_000, None)
        self.assertEqual((w.live, w.playing_kbps), (False, 8_000), "as it was")
        kodi.play(30)
        kodi.short_waits()
        self.assertEqual(len(kodi.said), 1, "the picture plays on")
        kodi.play(5)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1][1], "2m", "another stream once it stopped: 8 Mbps halved, with the margin")


class SettleTest(unittest.TestCase):
    """It settles (docs/QUALITY.md): a step up undone within 10 minutes
    isn't tried again during the title, and each such step doubles the
    calm the next one waits for."""

    def test_a_bitrate_that_didnt_hold_isnt_tried_again(self):
        w = live()
        kodi = Kodi(w)
        kodi.arriving = 6_000
        kodi.play(30)
        kodi.wait(2)
        kodi.carried = 13_000
        kodi.play(300)
        self.assertEqual(kodi.said, [(32, 4_000), (332, 8_000)])
        # Two minutes on, 8 Mbps doesn't hold after all.
        kodi.carried = None
        kodi.play(120)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1], (454, 4_000))
        self.assertEqual(w.quiet, 360)
        # However fast the network says it is, it stays where it held.
        kodi.carried = 80_000
        kodi.play(3_600)
        self.assertEqual(kodi.said[-1], (454, 4_000))

    def test_a_step_down_more_than_10_minutes_after_leaves_it_open(self):
        w = live()
        kodi = Kodi(w)
        kodi.arriving = 6_000
        kodi.play(30)
        kodi.wait(2)
        kodi.carried = 13_000
        kodi.play(300)
        kodi.carried = None
        kodi.play(605)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1][1], 4_000)
        self.assertEqual(w.quiet, 180)
        kodi.carried = 13_000
        kodi.play(300)
        self.assertEqual(kodi.said[-1][1], 8_000)

    def test_each_step_undone_doubles_the_calm(self):
        w = live("12m", 12_000)
        kodi = Kodi(w)
        kodi.arriving = 7_000
        kodi.play(30)
        kodi.wait(2)
        self.assertEqual(kodi.said, [(32, 5_000)])
        kodi.carried = 30_000
        kodi.play(300)
        self.assertEqual(kodi.said[-1], (332, 8_000))
        kodi.play(180)
        self.assertEqual(kodi.said[-1], (512, 12_000))
        # 12 Mbps doesn't hold: back down, and it's off for the title.
        kodi.carried = None
        kodi.arriving = 11_000
        kodi.play(20)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1], (534, 8_000))
        # Later 8 falters too, more than 10 minutes after it was reached: 8 stays open.
        kodi.play(620)
        kodi.arriving = 8_000
        kodi.wait(2)
        self.assertEqual(kodi.said[-1], (1156, 6_000))
        # 5 minutes after the step down 3 calm minutes would have done; now it takes 6.
        kodi.carried = 30_000
        kodi.play(300)
        self.assertEqual(kodi.said[-1], (1156, 6_000))
        kodi.play(60)
        self.assertEqual(kodi.said[-1], (1516, 8_000))
        kodi.play(1_200)
        self.assertEqual(kodi.said[-1], (1516, 8_000), "12 Mbps not again")

    def test_the_calm_doubles_up_to_12_minutes(self):
        w = live()
        for _ in range(4):
            w.last_up = (0.0, "x")
            w.went_down(1.0)
        self.assertEqual(w.quiet, 720)


class NextTitleTest(unittest.TestCase):
    """A stream that can't go higher without another stays, while the
    network it carries raises where the next title starts."""

    def test_where_the_next_title_starts_goes_up_one_quality_at_a_time(self):
        w = watch("4m", 4_000, 20_000, 1080, lowered=True)
        kodi = Kodi(w)
        kodi.carried = 13_000
        kodi.play(299)
        self.assertEqual(kodi.said, [], "not within 5 minutes of a step down")
        kodi.play(1)
        self.assertEqual(kodi.steps, [aq.Step(aq.NEXT, "8m", None, False, 8_000)], "13 Mbps carries 1.6 times 8")
        self.assertEqual(w.quality, "4m", "the stream plays on as it is")
        kodi.play(400)
        self.assertEqual(len(kodi.said), 1, "12 Mbps needs 19.2")
        kodi.carried = 40_000
        kodi.play(1)
        self.assertEqual(kodi.said[-1][1], "next 12m")
        kodi.play(179)
        self.assertEqual(len(kodi.said), 2, "one step every 3 minutes at most")
        kodi.play(1)
        # 15 Mbps makes the 20 Mbps 1080p file smaller; 20 Mbps 1080p doesn't.
        self.assertEqual(kodi.said[-1][1], "next 15m")
        kodi.play(180)
        self.assertEqual(kodi.steps[-1], aq.Step(aq.NEXT, "original", None, False, 0), "40 Mbps carries 1.6 times the file: forgotten")
        kodi.play(600)
        self.assertEqual(len(kodi.said), 4)

    def test_an_issue_restarts_the_quiet_time(self):
        w = watch("4m", 4_000, 20_000, 1080, lowered=True)
        kodi = Kodi(w)
        kodi.carried = 50_000
        kodi.play(250)
        # A wait: down to 2 Mbps, the lowest.
        kodi.wait(2)
        self.assertEqual(kodi.said, [(252, "2m")])
        w.restarted(kodi.now, "2m", 2_000)
        kodi.play(30)
        # Another, which has nowhere lower to go.
        kodi.wait(2)
        self.assertEqual(len(kodi.said), 1)
        kodi.play(179)
        self.assertEqual(len(kodi.said), 1, "3 minutes after the last issue, 5 after the step down")
        kodi.play(300 - 179 - 32)
        self.assertEqual(kodi.said[-1], (552, "next 4m"))

    def test_a_raise_a_step_down_undoes_isnt_made_again(self):
        w = watch("4m", 4_000, 20_000, 1080, lowered=True)
        kodi = Kodi(w)
        kodi.carried = 13_000
        kodi.play(300)
        self.assertEqual(kodi.said, [(300, "next 8m")])
        kodi.carried = None
        kodi.play(60)
        kodi.wait(2)
        self.assertEqual(kodi.said[-1], (362, "2m"))
        self.assertEqual(kodi.steps[-1].memory, 2_000)
        self.assertEqual(w.quiet, 360)
        w.restarted(kodi.now, "2m", 2_000)
        kodi.carried = 50_000
        kodi.play(359)
        self.assertEqual(kodi.said[-1][1], "2m", "6 calm minutes now")
        kodi.play(1)
        self.assertEqual(kodi.said[-1], (722, "next 4m"))
        kodi.play(2_000)
        self.assertEqual(kodi.said[-1], (722, "next 4m"), "8 Mbps didn't hold this title")

    def test_the_original_playing_has_nothing_to_raise(self):
        kodi = Kodi(watch())
        kodi.carried = 100_000
        kodi.play(1_200)
        self.assertEqual(kodi.said, [])


class FramesAndDecoderTest(unittest.TestCase):
    def test_dropped_frames_move_it_in_the_first_minute_only(self):
        w = watch()
        self.assertEqual(w.dropped(30.0), aq.Step(aq.RESTART, "converted", None, False, None), "the server's own quality")
        self.assertIsNone(w.dropped(40.0), "once")
        # A conversion: a quality lower, another stream.
        w = live()
        self.assertEqual(w.dropped(25.0), aq.Step(aq.RESTART, "4m", None, True, None))
        self.assertIsNone(live().dropped(10.0), "not within 20 s of the start")

    def test_later_dropped_frames_lower_where_the_next_title_starts(self):
        w = watch()
        self.assertEqual(w.dropped(61.0), aq.Step(aq.NEXT, "15m", None, False, 15_000))
        self.assertEqual(w.quality, "original", "it plays on")
        self.assertIsNone(w.dropped(90.0), "said once")
        self.assertEqual(live().dropped(300.0), aq.Step(aq.NEXT, "4m", None, False, 4_000))

    def test_the_player_giving_up_on_the_original_converts_it_once(self):
        w = watch()
        self.assertEqual(w.failed(12.0), aq.Step(aq.RESTART, "converted", None, False, None))
        self.assertIsNone(w.failed(13.0))
        w.restarted(14.0, "original", 15_000, live=True, lines=1080, converted=True)
        self.assertIsNone(w.failed(40.0), "a conversion that fails isn't converted again")
        self.assertIsNone(watch("8m", 8_000).failed(5.0))


class BitrateLabelTest(unittest.TestCase):
    def test_labels_floors_and_steps(self):
        self.assertEqual(aq.bitrate_label(4_500, 1080), "4.5 Mbps 1080p")
        self.assertEqual(aq.bitrate_label(8_000, 2160), "8 Mbps 4K")
        self.assertEqual(aq.bitrate_label(3_000, None), "3 Mbps")
        self.assertEqual([aq.floor_kbps(n) for n in (2160, 1600, 1080, 800, 720, 480, None)], [8_000, 8_000, 3_000, 3_000, 1_500, 1_000, 1_000])
        self.assertEqual(aq.named_lines(1920, 800), 1080)
        self.assertEqual(aq.named_lines(3840, 1600), 2160)
        self.assertEqual(aq.named_lines(None, 536), 480)
        self.assertIsNone(aq.named_lines(None, None))
        self.assertEqual(aq.stream_lines("8m", True, None, 2160), 1080)
        self.assertEqual(aq.stream_lines("8m", True, None, 720), 720, "no taller than the file")
        self.assertEqual(aq.stream_lines("original", True, "15m", 2160), 2160)
        self.assertEqual(aq.stream_lines("original", False, None, 1080), 1080)
        self.assertEqual(aq.rungs_up(4_500, 20_000), [8_000, 12_000, 15_000, 20_000])
        self.assertEqual(aq.rungs_up(3_000, 6_000), [4_000, 6_000])


class MeterTest(unittest.TestCase):
    def setUp(self):
        self.now = 0.0
        self.meter = Meter(clock=lambda: self.now)

    def test_what_arrives_over_20_seconds(self):
        for second in range(40):
            self.now = second
            self.meter.chunk("/api/playback/a/3.ts", 1_000_000)
            self.meter.chunk("/api/images/1", 5_000_000)
        self.assertAlmostEqual(self.meter.arriving_kbps("/api/playback/a/"), 21 * 8_000 / 20.0)
        self.assertIsNone(self.meter.arriving_kbps("/api/playback/b/"))

    def test_what_the_network_carried_needs_three_answers(self):
        self.meter.answered("/api/playback/a/1.ts", 4_000_000, 1.0)
        self.meter.answered("/api/playback/a/2.ts", 4_000_000, 2.0)
        self.assertIsNone(self.meter.carried_kbps("/api/playback/a/", 0))
        self.meter.answered("/api/playback/a/3.ts", 4_000_000, 4.0)
        self.meter.answered("/api/playback/a/4.ts", 100_000, 0.001)
        self.assertEqual(self.meter.carried_kbps("/api/playback/a/", 0), 8_000, "the third fastest, of 256 KB or more")
        self.now = 10
        self.assertIsNone(self.meter.carried_kbps("/api/playback/a/", 5), "since the last issue")


class StartTest(unittest.TestCase):
    """Where Automatic starts on a network the server knows (docs/QUALITY.md)."""

    def test_the_servers_ceiling(self):
        self.assertEqual(aq.network_ceiling({"local": False, "ceiling_kbps": 8000, "source": "playback"}), 8000)
        for answer in ({"ceiling_kbps": None}, {"ceiling_kbps": 0}, {"ceiling_kbps": True}, {}, None, []):
            self.assertIsNone(aq.network_ceiling(answer), answer)
        self.assertTrue(aq.wants_probe({"local": False, "probe": True}))
        self.assertFalse(aq.wants_probe({"local": True, "probe": True}), "never on the server's own network")
        self.assertFalse(aq.wants_probe({"probe": 1}))
        self.assertFalse(aq.wants_probe(None))

    def test_a_speed_test_that_comes_whole_in_time(self):
        test = aq.SpeedTest(total=1_000_000)
        test.chunk(10.0, 100_000)
        self.assertFalse(test.done(10.5))
        for i in range(9):
            test.chunk(10.1 + i * 0.1, 100_000)
        self.assertTrue(test.done(10.9))
        # The first piece held what came before the clock started.
        self.assertEqual(test.result(11.0), {"probe_kbps": 8000, "probe_complete": True})
        self.assertIsNone(aq.probe_ceiling(test.result()), "at least that fast: no limit")

    def test_a_speed_test_stopped_two_seconds_after_its_first_byte(self):
        test = aq.SpeedTest()
        test.chunk(0.0, 262_144)
        test.chunk(1.0, 500_000)
        test.chunk(1.9, 500_000)
        self.assertFalse(test.done(1.99))
        self.assertTrue(test.done(2.0))
        self.assertEqual(test.until(5.0), 2.0)
        self.assertEqual(test.result(2.0), {"probe_kbps": 4000, "probe_complete": False})
        self.assertEqual(aq.probe_ceiling(test.result(2.0)), 2800, "70 %, as the server keeps it")

    def test_too_little_measures_nothing(self):
        test = aq.SpeedTest()
        self.assertIsNone(test.result(3.0))
        self.assertEqual(test.until(3.5), 3.5, "the first byte awaited until the budget runs out")
        test.chunk(0.0, 200_000)
        test.chunk(1.0, 60_000)
        self.assertIsNone(test.result(2.0), "under 256 KiB")


if __name__ == "__main__":
    unittest.main()
