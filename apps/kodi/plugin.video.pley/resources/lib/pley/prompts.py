"""Skip Intro, Skip Credits and Up Next, by the web player's rules
(apps/web/src/player/SkipButton.tsx): a button from a marker's start until
a second before its end; an episode's credits bring up the next episode,
which plays after a countdown unless it is canceled."""

from __future__ import annotations

import time

# The button goes this long before the segment ends.
TAIL_MS = 1000
# Skip Intro lands this long before the intro ends, so it plays out.
INTRO_LEAD_MS = 750
COUNTDOWN = 10
# A skip button holds the remote while it shows: it goes after this long,
# as streaming apps' do (credits can last minutes).
SKIP_SHOWN = 10


def active(markers: list, position_ms: float):
    """The marker the playhead is in, if any."""
    return next((m for m in markers if m["start_ms"] <= position_ms < m["end_ms"] - TAIL_MS), None)


def target_ms(marker: dict) -> float:
    """Where skipping lands: the last moment of an intro, the end of credits."""
    if marker["kind"] == "intro":
        return marker["end_ms"] - min(INTRO_LEAD_MS, (marker["end_ms"] - marker["start_ms"]) / 2)
    return marker["end_ms"]


def episode_line(item: dict) -> str:
    """"S1 · E2 · Pilot", as the Android app writes an episode."""
    if item.get("kind") == "episode" and item.get("parent_index") is not None and item.get("index") is not None:
        return "S%d · E%d · %s" % (item["parent_index"], item["index"], item["title"])
    return item.get("title", "")


class Moments:
    """What to offer at each position of one playback: ("skip", marker),
    ("next", None) or None. `next_item` is what plays next, if anything."""

    def __init__(self, markers: list, next_item=None, clock=time.monotonic):
        self.markers = markers
        self.next_item = next_item
        self.clock = clock
        self.dismissed = set()
        self.shown_since = {}
        self.next_cancelled = False
        self.next_since = None

    def offer(self, position_ms: float):
        marker = active(self.markers, position_ms)
        if marker and marker["kind"] == "credits" and self.next_item and not self.next_cancelled:
            return "next", marker
        if marker and key(marker) not in self.dismissed:
            since = self.shown_since.setdefault(key(marker), self.clock())
            if self.clock() - since >= SKIP_SHOWN:
                self.dismiss(marker)
                return None
            return "skip", marker
        return None

    def credits_from(self):
        """Where the credits start, if a marker says (ms)."""
        starts = [m["start_ms"] for m in self.markers if m["kind"] == "credits"]
        return min(starts) if starts else None

    def dismiss(self, marker: dict) -> None:
        self.dismissed.add(key(marker))

    def counting(self) -> int:
        """Seconds left of Up Next's countdown, which starts on first ask."""
        if self.next_since is None:
            self.next_since = self.clock()
        return max(COUNTDOWN - int(self.clock() - self.next_since), 0)

    def cancel_next(self) -> None:
        self.next_cancelled = True
        self.next_since = None


def key(marker: dict):
    return marker["file_id"], marker["kind"], marker["start_ms"]
