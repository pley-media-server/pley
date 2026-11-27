"""Skip Intro, Skip Credits and Up Next, by the web player's rules
(apps/web/src/player/SkipButton.tsx): a button from a marker's start until
a second before its end, on its own for its first 8 seconds; an episode's
credits that run to the end of the
file bring up the next episode, which plays after a countdown unless it is
canceled. Without them, Up Next comes when the file ends (the service's
after_end)."""

from __future__ import annotations

import time

# The button goes this long before the segment ends.
TAIL_MS = 1000
# Credits ending this close to the end of their file run to it, as the
# server's Marker::reaches_end says: what is left is a studio's logo or
# black, not a scene.
CREDITS_TAIL_MS = 15_000
# Skip Intro lands this long before the intro ends, so it plays out.
INTRO_LEAD_MS = 750
COUNTDOWN = 10
# A skip button holds the remote while it shows: it goes this long after it
# appears (credits can last minutes). The other apps bring it back with
# their controls; Kodi's controls are the skin's OSD, which a button over
# it would take the remote from, so here it stays away until the playhead
# enters a marker again.
SKIP_ALONE = 8


def active(markers: list, position_ms: float):
    """The marker the playhead is in, if any."""
    return next((m for m in markers if m["start_ms"] <= position_ms < m["end_ms"] - TAIL_MS), None)


def reaches_end(marker: dict, duration_ms) -> bool:
    """Whether `marker` is credits that run to the end of their file,
    `duration_ms` long (unknown: none do). Only those bring up the next
    episode: credits that something follows (an after-credits scene, a
    show's last minutes) keep Skip Credits, so the countdown doesn't skip
    what comes after them."""
    return marker["kind"] == "credits" and (duration_ms or 0) > 0 and duration_ms - marker["end_ms"] <= CREDITS_TAIL_MS


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
    ("next", marker) or None. `next_item` is what plays next, if anything;
    `duration_ms` is the length of the file the markers belong to."""

    def __init__(self, markers: list, next_item=None, clock=time.monotonic, duration_ms=0):
        self.markers = markers
        self.next_item = next_item
        self.clock = clock
        self.duration_ms = duration_ms
        # The marker the playhead is in, Skip's for it that was pressed or
        # dismissed, and when it appeared: leaving the marker forgets them,
        # so entering one again starts afresh.
        self.inside = None
        self.dismissed = set()
        self.shown_since = {}
        self.next_cancelled = False
        self.next_since = None

    def offer(self, position_ms: float):
        marker = active(self.markers, position_ms)
        inside = key(marker) if marker else None
        if inside != self.inside:
            self.inside = inside
            self.dismissed.clear()
            self.shown_since.clear()
        if marker and reaches_end(marker, self.duration_ms) and self.next_item and not self.next_cancelled:
            # Up Next takes Skip Credits' place: if it comes back (Cancel), its 8 seconds start then.
            self.shown_since.pop(key(marker), None)
            return "next", marker
        if marker and key(marker) not in self.dismissed:
            since = self.shown_since.get(key(marker))
            if since is not None and self.clock() - since >= SKIP_ALONE:
                return None
            return "skip", marker
        return None

    def appeared(self, marker: dict) -> None:
        """Skip for `marker` is on screen: its 8 seconds start, the first
        time only. A seek inside the marker keeps them where they are."""
        self.shown_since.setdefault(key(marker), self.clock())

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
