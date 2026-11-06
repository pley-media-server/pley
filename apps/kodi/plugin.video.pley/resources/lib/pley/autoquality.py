"""The quality menu and how Automatic chooses (docs/QUALITY.md), without
Kodi: what the service sees of the playback goes in, decisions come out.

Automatic plays the original unless it doesn't play well here. Kodi tells
the service three things about that: whether it is buffering
(Player.Caching), where the playback is (the position not moving while
it plays), and how much it has read ahead (JSON-RPC's cachepercentage,
where the player has a cache). The proxy every stream goes through counts
what arrives (Meter). Kodi doesn't say how many frames it drops, so a
device that can't keep up isn't seen here; the server converts what this
Kodi can't decode before it plays, and Kodi's player giving up on the
original (onPlayBackError) plays it converted.

Kodi's player closes one stream before it opens the next, so the screen
goes black between them (docs/QUALITY.md, "Players that go black between
streams"). Automatic moves to another stream only once the picture has
stopped anyway: a wait of 2 s or more, or the player giving up. A
conversion whose bitrate can change as it is sent (live_bitrate) steps
its bitrate instead, keeping its size, and plays on.
"""

from __future__ import annotations

import collections
import threading
import time
from typing import List, Optional

# The menu, in its order: (id, label, kbps, lines). A bitrate's label is
# the same in every language; Original's and Automatic's are the names of
# the add-on's words for them (label()).
ORIGINAL = "original"
AUTO = "auto"
MENU = [
    (ORIGINAL, "original", None, None),
    (AUTO, "automatic", None, None),
    ("30m", "30 Mbps 4K", 30000, 2160),
    ("15m", "15 Mbps 4K", 15000, 2160),
    ("20m", "20 Mbps 1080p", 20000, 1080),
    ("12m", "12 Mbps 1080p", 12000, 1080),
    ("8m", "8 Mbps 1080p", 8000, 1080),
    ("4m", "4 Mbps 720p", 4000, 720),
    ("2m", "2 Mbps 480p", 2000, 480),
]
IDS = [m[0] for m in MENU]
DEFAULT = AUTO
# Automatic's restart of the original converted at the server's own
# quality for it (a decoder failing): not in the menu.
CONVERTED = "converted"
# The setting's order before Automatic (an index): what a choice made then means now.
OLD_ORDER = [ORIGINAL, "30m", "20m", "15m", "12m", "8m", "4m", "2m"]
# The lowest Automatic goes, and where it goes knowing nothing of the
# file or the network.
FLOOR = "2m"
BLIND = "8m"

# Rebuffering: one wait this long, or two shorter ones this close.
LONG_WAIT = 2.0
SHORT_WAITS_WITHIN = 60.0
# The original as the file is moves for such a wait only where the network
# carried less than this many times the file's bitrate over the last 2
# minutes: on one that carries more, the server is why it waits (its disk,
# a remux catching up), and a conversion reads the same file and does more.
# Where nothing measured it, the second such wait this close moves it.
ORIGINAL_NETWORK = 1.5
ORIGINAL_NETWORK_WINDOW = 120.0
ORIGINAL_WAITS_WITHIN = 300.0
# Delivery: seconds of video arriving per second, over this long; under
# SLOW for SLOW_FOR seconds with under LOW_AHEAD seconds read ahead, and
# falling, it is a play issue before a wait.
DELIVERY_WINDOW = 20.0
SLOW = 1.1
SLOW_FOR = 15.0
LOW_AHEAD = 10.0
# What a step down keeps of what the network carried.
MARGIN = 0.75
# Without a measure of delivery, a step down halves.
UNKNOWN_DELIVERY = 0.5
# No change this soon after the last.
SETTLE = 20.0
# Back up: after this long without an issue and since the last step (at
# most one step this often), not this soon after a step down, when the
# network carried this much more than the step up.
QUIET = 180.0
AFTER_DOWN = 300.0
HEADROOM = 1.6
# It settles: a step up a step down undoes this soon isn't tried again
# during the title, and doubles the calm the next step up waits for, up
# to MAX_QUIET.
SETTLE_UP = 600.0
MAX_QUIET = 4 * QUIET
# Dropped frames move the stream in the title's first minute only (a guess
# that the device can't keep up with it); later they lower where the next
# title starts.
FIRST_MINUTE = 60.0
# What Automatic remembers of a network found slow, for the next title.
REMEMBER_FOR = 30 * 60.0
# The position moving this much less or more than the time is a seek.
JUMP = 3.0

# A conversion's bitrate stepped as it is sent (live_bitrate): never under
# its size's floor, (lines above, kbps) from the tallest, nor above the
# stream's own; in whole Mbps, rounded down, as on Samsung and LG.
FLOORS = ((1080, 8000), (720, 3000), (480, 1500))
LEAST_KBPS = 1000
BITRATE_STEP = 1000

# Step.kind: how a decision is carried out.
RESTART = "restart"  # the title again where it is, at `quality`: a new stream
BITRATE = "bitrate"  # the stream goes on at `kbps` (PUT /api/playback/{id}/bitrate)
NEXT = "next"  # nothing changes now: only where the next title starts (`memory`)


class Step(collections.namedtuple("Step", "kind quality kbps down memory")):
    """A decision of Automatic's. `quality` is what the stream plays (for a
    restart, what it plays next; CONVERTED for the original converted),
    `kbps` the bitrate a BITRATE step goes on at, `down` whether it is a
    step down to say, and `memory` what the network's memory keeps for the
    next title: kbps, 0 to forget it, None to leave it."""

    __slots__ = ()

# Where Automatic starts (docs/QUALITY.md): what the server knows of this
# Kodi's network is asked for at most this long, and a speed test is
# 32 MiB stopped 2 s after its first byte, measuring nothing under 256 KiB.
# The whole of it, asking and testing, holds the play back no longer than
# START_BUDGET. A speed test the server couldn't keep counts for 70 % of
# what it measured, as the server counts it.
NETWORK_TIMEOUT = 1.5
PROBE_BYTES = 32 * 1024 * 1024
PROBE_SECONDS = 2.0
PROBE_LEAST = 256 * 1024
PROBE_SHARE = 0.7
START_BUDGET = 3.5


def label(quality_id: str, text) -> str:
    """A quality as the menu names it; `text(name)` is the add-on's words."""
    for id_, name, kbps, _ in MENU:
        if id_ == quality_id:
            return name if kbps else text(name)
    return text(MENU[0][1])


def decimal(value: float, digits: int) -> str:
    """A number as English writes it, "4.5": bitrate_label's own."""
    return "%.*f" % (digits, value)


def limits(quality_id: str):
    """(max_bitrate, max_height) for a quality; (None, None) for Original and Automatic."""
    for id_, _, kbps, lines in MENU:
        if id_ == quality_id:
            return kbps, lines
    return None, None


def kbps_of(quality_id: Optional[str]) -> Optional[int]:
    return limits(quality_id)[0] if quality_id else None


def from_setting(value: str) -> str:
    """The quality a setting's value names (Automatic for anything else)."""
    return value if value in IDS else DEFAULT


def from_old_index(index: int) -> Optional[str]:
    """The quality a choice made before Automatic named, None for none.
    Index 0 was both the default and Original, which can't be told apart:
    it isn't carried over, and Automatic is played."""
    if 0 < index < len(OLD_ORDER):
        return OLD_ORDER[index]
    return None


def offered(file_kbps: Optional[float], file_lines: Optional[int]) -> List[str]:
    """The menu's converted qualities that make the file smaller (every
    one when its bitrate and size are unknown), best first by bitrate."""
    found = []
    for id_, _, kbps, lines in MENU:
        if kbps is None:
            continue
        if file_kbps is None and file_lines is None:
            found.append(id_)
        elif (file_kbps is not None and kbps < file_kbps) or (file_lines is not None and lines < file_lines):
            found.append(id_)
    return sorted(found, key=lambda q: -kbps_of(q))


def menu_for(width: Optional[int], height: Optional[int], bitrate: Optional[float]) -> List[str]:
    """The ids of the Quality menu as every app offers it for a file of
    `width` × `height` and `bitrate` (bits per second), docs/QUALITY.md, "The
    menu", and as the server lists them to remotes: Original, Automatic,
    then each quality with fewer lines than the file or at most 80 % of its
    bitrate (every one when neither is known), tallest first as the file
    names them, then by bitrate. The setting offers the whole menu, which
    keeps the table's order."""
    lines = named_lines(width, height)
    kbps = bitrate / 1000.0 if bitrate and bitrate > 0 else None
    height = height if height and height > 0 else None
    copies = []
    for id_, _, cap_kbps, cap_lines in MENU:
        if cap_kbps is None:
            continue
        if kbps is not None or height is not None:
            fewer = height is not None and cap_lines < height
            lighter = kbps is not None and cap_kbps <= kbps * 0.8
            if not fewer and not lighter:
                continue
        shown = min(cap_lines, lines) if lines else cap_lines
        copies.append((id_, shown, cap_kbps))
    copies.sort(key=lambda c: (-c[1], -c[2]))
    return [ORIGINAL, AUTO] + [c[0] for c in copies]


def picked(choice: str, offered_ids: List[str]) -> str:
    """What the Quality menu has picked for a file that offers
    `offered_ids` (menu_for), as progress reports and the user's remotes
    say it: the setting's choice, else Original, which a file that doesn't
    offer it plays."""
    return choice if choice in offered_ids else ORIGINAL


def under_cap(qualities: List[str], cap: Optional[str]) -> List[str]:
    """Those no higher than what the server would convert the file to
    (auto_quality, which is at the file's lines or fewer)."""
    if not cap or cap not in IDS:
        return qualities
    most = kbps_of(cap)
    return [q for q in qualities if kbps_of(q) <= most]


def start_quality(ceiling_kbps: Optional[float]) -> Optional[str]:
    """Where Automatic starts under a remembered ceiling: the best quality
    at or under it (2 Mbps 480p at least); None without one. The server
    plays the file as it is when it is already under it."""
    if not ceiling_kbps:
        return None
    for q in offered(None, None):
        if kbps_of(q) <= ceiling_kbps:
            return q
    return FLOOR


def step_down(
    current: str,
    playing_kbps: Optional[float],
    delivery: Optional[float],
    network_kbps: Optional[float],
    file_kbps: Optional[float],
    file_lines: Optional[int],
    cap: Optional[str],
):
    """The quality after a play issue, and the ceiling it came from:
    (quality, ceiling kbps), or (None, None) when there is nothing lower.

    The ceiling is the bitrate playing times the delivery seen times 0.75
    (what the network carried, when the bitrate playing is unknown), never
    above the server's own quality for the file; the quality is the best
    under it, always lower than the one playing, 2 Mbps 480p at least."""
    if delivery is not None and playing_kbps:
        carried = playing_kbps * min(max(delivery, 0.05), 1.0)
    elif network_kbps and playing_kbps:
        carried = min(network_kbps, playing_kbps)
    elif network_kbps:
        carried = network_kbps
    elif playing_kbps:
        carried = playing_kbps * UNKNOWN_DELIVERY
    else:
        carried = None
    ceiling = carried * MARGIN if carried else None
    candidates = under_cap(offered(file_kbps, file_lines), cap)
    below = playing_kbps if current == ORIGINAL else kbps_of(current)
    if below is not None:
        candidates = [q for q in candidates if kbps_of(q) < below]
    if not candidates:
        return None, None
    if ceiling is None:
        # Nothing known of the file or the network: a middling quality.
        quality = BLIND if BLIND in candidates else candidates[0]
        return quality, kbps_of(quality)
    for q in candidates:
        if kbps_of(q) <= ceiling:
            return q, ceiling
    return candidates[-1], ceiling


def step_up(current: str, file_kbps: Optional[float], file_lines: Optional[int], cap: Optional[str]):
    """The next quality up from `current` and the bitrate it needs the
    network to carry it at (before the headroom): (quality, kbps). The
    original needs the file's bitrate, or the server's own quality's when
    it converts it; unknown, it can't be measured against: (None, None)."""
    if current == ORIGINAL:
        return None, None
    up = [q for q in under_cap(offered(file_kbps, file_lines), cap) if kbps_of(q) > kbps_of(current)]
    if up:
        return up[-1], kbps_of(up[-1])
    need = kbps_of(cap) if cap in IDS else file_kbps
    if not need:
        return None, None
    return ORIGINAL, need


def named_lines(width: Optional[int], height: Optional[int]) -> Optional[int]:
    """The lines a picture of width × height is named by: 2160, 1080, 720
    or 480, by its width too (a 1920 × 800 film is 1080p); None when
    neither is known."""
    width, height = width or 0, height or 0
    if width <= 0 and height <= 0:
        return None
    if width >= 3800 or height >= 2100:
        return 2160
    if width >= 1800 or height >= 1000:
        return 1080
    if width >= 1200 or height >= 700:
        return 720
    return 480


def stream_lines(quality: str, converted: bool, cap: Optional[str], file_named: Optional[int]) -> Optional[int]:
    """The lines a stream is named by: a conversion's quality's (the
    server's own for the original converted), no more than the file's; the
    file's for the file as it is."""
    if not converted:
        return file_named
    lines = limits(cap)[1] if quality == ORIGINAL else limits(quality)[1]
    if lines and file_named:
        return min(lines, file_named)
    return lines or file_named


def floor_kbps(lines: Optional[int]) -> int:
    """The least a stream of `lines` is stepped down to as it is sent: 8 Mbps
    for 4K, 3 Mbps for 1080p, 1.5 for 720p and 1 below (docs/QUALITY.md)."""
    for above, kbps in FLOORS:
        if (lines or 0) > above:
            return kbps
    return LEAST_KBPS


def bitrate_label(kbps: float, lines: Optional[int], number=decimal) -> str:
    """A bitrate stepped as it is sent, as the menu names qualities: "4.5
    Mbps 1080p", "8 Mbps 4K". `number(value, digits)` writes the number in
    the language shown (kodi.decimal)."""
    mbps = int(kbps) // 100 / 10.0
    words = number(mbps, 0 if mbps == int(mbps) else 1) + " Mbps"
    if not lines:
        return words
    return words + (" 4K" if lines >= 2160 else " %dp" % lines)


def rungs_up(kbps: float, own: float) -> List[float]:
    """The bitrates a stream stepped down as it is sent climbs back by, one
    at a time: the menu's, then its own."""
    return sorted({k for _, _, k, _ in MENU if k and kbps < k < own} | {own})


class Watch:
    """One title in Automatic, from its first playback to its last
    restart: `tick` once a second with what Kodi says, and it answers a
    Step, or None. `restarted` after a restart, `answered` with the
    server's answer to a BITRATE step, `failed` when Kodi's player gives up,
    `dropped` for frames dropped (where Kodi would say)."""

    def __init__(
        self,
        now: float,
        quality: str,
        playing_kbps: Optional[float],
        file_kbps: Optional[float],
        file_lines: Optional[int],
        cap: Optional[str] = None,
        lowered: bool = False,
        live: bool = False,
        lines: Optional[int] = None,
        converted: Optional[bool] = None,
    ):
        self.file_kbps = file_kbps
        self.file_lines = file_lines
        self.cap = cap
        self.title_at = now
        self.changed_at = now
        # Started under a remembered ceiling counts as a step down.
        self.down_at = now if lowered else None
        self.issue_at = now
        # The ceiling of the last step down, for the next titles.
        self.ceiling = None
        # It settles: the last step up, until a step down or SETTLE_UP says
        # whether it held, what didn't hold this title, and the calm a step
        # up waits for.
        self.last_up = None
        self.too_high = set()
        self.quiet = QUIET
        # Where the next title starts, raised while this stream can't go
        # higher (None: where this one plays), and when it last was.
        self.next_title = None
        self.raised_at = None
        self.fell_back = False
        self.dropped_told = False
        # When the original's long waits came on a network nothing measured.
        self.original_waits = collections.deque()
        self.stream(quality, playing_kbps, live, lines, converted)
        self.reset(now)

    def stream(self, quality: str, playing_kbps: Optional[float], live: bool, lines: Optional[int], converted: Optional[bool]) -> None:
        """What plays now: `quality` at `playing_kbps`, `lines` tall, its
        bitrate changing as it is sent when `live`."""
        self.quality = quality
        self.playing_kbps = playing_kbps
        self.converted = quality != ORIGINAL if converted is None else bool(converted)
        self.live = bool(live and self.converted and playing_kbps)
        # Its own bitrate, which a step up goes back to at most, and the
        # last the server said it goes on at.
        self.own_kbps = playing_kbps
        self.sent_kbps = playing_kbps
        self.lines = lines

    def floor(self) -> float:
        return min(floor_kbps(self.lines), self.own_kbps or LEAST_KBPS)

    def lowered(self) -> bool:
        """Below the original: a quality, or a bitrate stepped down."""
        return self.quality != ORIGINAL or bool(self.live and self.playing_kbps < self.own_kbps)

    def label(self, text, number=decimal) -> str:
        """What plays, as the menu would say it after "Automatic · "."""
        if self.live and self.playing_kbps < self.own_kbps:
            return bitrate_label(self.playing_kbps, self.lines, number)
        return label(self.quality, text)

    def reset(self, now: float) -> None:
        self.last = None
        self.flowing = 0
        self.stall_since = None
        self.stall_counted = False
        # The wait under way lasted 2 s (`long_wait`): what's left of it is
        # no short wait. It came on a network nothing measured, and is among
        # `original_waits`.
        self.stall_long = False
        self.stall_unmeasured = False
        self.short_waits = collections.deque()
        self.history = collections.deque()
        self.slow_since = None

    def seeked(self) -> None:
        """Kodi was told to seek: the wait that follows isn't the network's."""
        self.flowing = 0
        self.stall_since = None
        self.stall_long = False
        self.stall_unmeasured = False
        self.history.clear()
        self.slow_since = None

    def restarted(
        self,
        now: float,
        quality: str,
        playing_kbps: Optional[float],
        live: bool = False,
        lines: Optional[int] = None,
        converted: Optional[bool] = None,
    ) -> None:
        """Playing again at `quality` (a new session, the same title)."""
        self.stream(quality, playing_kbps, live, lines, converted)
        self.changed_at = now
        self.reset(now)

    def delivery(self) -> Optional[float]:
        """Seconds of video that arrived per second over the window, from
        the position and what is read ahead; None without both."""
        if len(self.history) < 2:
            return None
        t0, p0, a0 = self.history[0]
        t1, p1, a1 = self.history[-1]
        if a0 is None or a1 is None or t1 - t0 < 5:
            return None
        return max((p1 - p0) + (a1 - a0), 0.0) / (t1 - t0)

    def tick(
        self,
        now: float,
        position: float,
        paused: bool = False,
        seeking: bool = False,
        caching: bool = False,
        ahead: Optional[float] = None,
        arriving_kbps: Optional[float] = None,
        carried_kbps: Optional[float] = None,
        shown_kbps: Optional[float] = None,
    ) -> Optional[Step]:
        """What Kodi says this second: `position` and `ahead` in seconds,
        `arriving_kbps` what the proxy passed on over the last 20 s,
        `carried_kbps` what it measured the network carrying since the
        last issue, `shown_kbps` over the last 2 minutes (None when it
        can't tell)."""
        last, self.last = self.last, (now, position)
        if paused or seeking:
            # Waits while paused or seeking aren't the network's.
            self.seeked()
            return None
        issue = False
        # A wait of 2 s or more: the picture stopped anyway.
        stopped = False
        if last is not None:
            elapsed = now - last[0]
            moved = position - last[1]
            if moved < -0.5 or (moved > 0.2 and abs(moved - elapsed) > JUMP):
                # A seek Kodi didn't say it made.
                self.seeked()
                return None
            stalled = caching or moved <= 0.2
            if stalled:
                if self.stall_since is None and self.flowing >= 2:
                    # Stopped somewhere since the last look: counted from it.
                    self.stall_since = last[0]
                    self.stall_counted = False
                    self.stall_long = False
                    self.stall_unmeasured = False
                if self.stall_since is not None and not self.stall_counted and now - self.stall_since >= LONG_WAIT:
                    if self.long_wait(now, shown_kbps):
                        self.stall_counted = True
                        issue = stopped = True
            else:
                if self.stall_since is not None and not self.stall_counted:
                    if self.stall_long:
                        if self.long_wait(now, shown_kbps):
                            # At the original, what arrived as it ended shows the network to blame after all.
                            issue = stopped = True
                    else:
                        self.short_waits.append(now)
                        while self.short_waits and now - self.short_waits[0] > SHORT_WAITS_WITHIN:
                            self.short_waits.popleft()
                        if len(self.short_waits) >= 2:
                            self.short_waits.clear()
                            issue = True
                self.stall_since = None
                self.stall_long = False
                self.stall_unmeasured = False
                self.flowing += 1
        if self.flowing >= 2 or self.stall_since is not None:
            self.history.append((now, position, ahead))
            while self.history and now - self.history[0][0] > DELIVERY_WINDOW:
                self.history.popleft()
        delivery = self.delivery()
        if not issue and delivery is not None and ahead is not None:
            falling = self.falling()
            if delivery < SLOW and ahead < LOW_AHEAD and falling:
                if self.slow_since is None:
                    self.slow_since = now
                elif now - self.slow_since >= SLOW_FOR:
                    self.slow_since = None
                    issue = True
            else:
                self.slow_since = None
        if issue:
            return self.on_issue(now, stopped, delivery, arriving_kbps)
        return self.maybe_up(now, carried_kbps)

    def long_wait(self, now: float, shown_kbps: Optional[float]) -> bool:
        """Whether the wait under way, 2 s long by `now`, is a play issue.
        At the original as the file is, only on a network not shown
        (`shown_kbps`) carrying ORIGINAL_NETWORK times the file's bitrate;
        where nothing measured it (a direct play, whose answer the proxy
        passes on whole), the second such wait within 5 minutes. Asked
        again as the wait goes on and as it ends."""
        self.stall_long = True
        if self.quality != ORIGINAL or self.converted:
            return True
        if shown_kbps and self.file_kbps:
            return shown_kbps < ORIGINAL_NETWORK * self.file_kbps
        if not self.stall_unmeasured:
            self.stall_unmeasured = True
            while self.original_waits and now - self.original_waits[0] >= ORIGINAL_WAITS_WITHIN:
                self.original_waits.popleft()
            self.original_waits.append(now)
        return len(self.original_waits) >= 2

    def falling(self) -> bool:
        """Less read ahead now than at the start of the window."""
        aheads = [a for _, _, a in self.history if a is not None]
        return len(aheads) >= 2 and aheads[-1] < aheads[0] - 1.0

    def on_issue(self, now: float, stopped: bool, delivery: Optional[float], arriving_kbps: Optional[float]) -> Optional[Step]:
        """A play issue: a step down, as the stream is sent when it can be;
        else another stream, but only once the picture has stopped anyway
        (`stopped`: a wait of 2 s or more), as the screen goes black
        between streams."""
        self.issue_at = now
        if now - self.changed_at < SETTLE:
            return None
        if self.live:
            return self.lower_bitrate(now, delivery, arriving_kbps)
        if not stopped:
            # Two short waits, or the delivery warning: the picture plays on.
            return None
        quality, ceiling = step_down(
            self.quality, self.playing_kbps, delivery, arriving_kbps, self.file_kbps, self.file_lines, self.cap
        )
        if quality is None:
            return None
        self.went_down(now)
        self.ceiling = ceiling
        # The lowest quality may be above the ceiling: that is where the next title starts.
        return Step(RESTART, quality, None, True, max(ceiling or 0, kbps_of(quality)))

    def lower_bitrate(self, now: float, delivery: Optional[float], arriving_kbps: Optional[float]) -> Optional[Step]:
        """The stream goes on lower, its size kept: the bitrate playing times
        the delivery seen times 0.75, no lower than its size's floor (where
        the network waits)."""
        kbps = self.playing_kbps
        if delivery is not None:
            carried = kbps * min(max(delivery, 0.05), 1.0)
        elif arriving_kbps:
            carried = min(arriving_kbps, kbps)
        else:
            carried = kbps * UNKNOWN_DELIVERY
        ceiling = carried * MARGIN
        target = max(self.floor(), min(self.own_kbps, ceiling // BITRATE_STEP * BITRATE_STEP))
        if target >= kbps:
            return None
        self.went_down(now)
        self.ceiling = ceiling
        self.playing_kbps = target
        return Step(BITRATE, self.quality, int(target), True, max(ceiling, target))

    def went_down(self, now: float) -> None:
        """A step down: one soon after a step up says that step was one too
        far, not tried again this title, and the calm before the next one
        doubles. Where the next title starts goes down with it."""
        if self.last_up is not None and now - self.last_up[0] < SETTLE_UP:
            self.too_high.add(self.last_up[1])
            self.quiet = min(MAX_QUIET, self.quiet * 2)
        self.last_up = None
        self.next_title = None
        self.changed_at = self.down_at = now

    def calm(self, now: float, since: float) -> bool:
        """Long enough without an issue, since `since`, and since a step down."""
        if now - self.issue_at < self.quiet or now - since < self.quiet:
            return False
        return self.down_at is None or now - self.down_at >= AFTER_DOWN

    def maybe_up(self, now: float, carried_kbps: Optional[float]) -> Optional[Step]:
        """After a calm spell, when the network carries 1.6 times it: a
        bitrate stepped down climbs back one step, as it is sent. A stream
        that can't go higher without another stream (which would blank the
        screen while it plays well) stays; the next title starts higher."""
        if carried_kbps is None:
            return None
        if self.live and self.playing_kbps < self.own_kbps:
            if not self.calm(now, self.changed_at):
                return None
            up = rungs_up(self.playing_kbps, self.own_kbps)[0]
            key = "%d kbps" % up
            if key in self.too_high or carried_kbps < HEADROOM * up:
                return None
            self.playing_kbps = up
            self.changed_at = now
            self.last_up = (now, key)
            if up < self.own_kbps:
                memory = up
            else:
                # Back at its own: the quality's, or no limit for the original.
                memory = 0 if self.quality == ORIGINAL else kbps_of(self.quality) or up
            return Step(BITRATE, self.quality, int(up), False, memory)
        if self.dropped_told or not self.calm(now, max(self.changed_at, self.raised_at or self.changed_at)):
            # Frames dropped lowered where the next title starts: not raised again.
            return None
        quality, need = step_up(self.next_title or self.quality, self.file_kbps, self.file_lines, self.cap)
        if quality is None or quality in self.too_high or carried_kbps < HEADROOM * need:
            return None
        self.next_title = quality
        self.raised_at = now
        self.last_up = (now, quality)
        return Step(NEXT, quality, None, False, 0 if quality == ORIGINAL else kbps_of(quality))

    def answered(self, asked: float, kbps: Optional[float]) -> None:
        """The server's answer to a BITRATE step to `asked`: what the stream
        goes on at, or None when it refused (a server or transcoder from
        before, a session gone). Refused, the stream is as it was, and moves
        as one whose bitrate can't change."""
        if kbps is None:
            self.live = False
            self.playing_kbps = self.sent_kbps
            return
        self.sent_kbps = kbps
        if self.playing_kbps == asked:
            self.playing_kbps = kbps

    def failed(self, now: float) -> Optional[Step]:
        """Kodi's player gave up on the original (a decoder failing): the
        title again converted, at the server's own quality for the file,
        once."""
        if self.quality != ORIGINAL or self.converted or self.fell_back:
            return None
        self.fell_back = True
        self.issue_at = self.changed_at = now
        return Step(RESTART, CONVERTED, None, False, None)

    def dropped(self, now: float) -> Optional[Step]:
        """The device dropped frames. In the title's first minute, a guess
        that it can't keep up with this stream: another (the original
        converted at the server's own quality, a conversion a quality
        lower). Later, only where the next title starts goes a quality
        lower, once. (Kodi tells add-ons nothing of dropped frames: for an
        app that counts them.)"""
        if self.quality == ORIGINAL and not self.converted:
            quality = CONVERTED
            lower, _ = step_down(ORIGINAL, self.playing_kbps, 1.0, None, self.file_kbps, self.file_lines, self.cap)
        else:
            quality, _ = step_down(self.quality, self.playing_kbps, 1.0, None, self.file_kbps, self.file_lines, self.cap)
            lower = quality
        if now - self.title_at < FIRST_MINUTE:
            if quality is None or now - self.changed_at < SETTLE or self.fell_back:
                return None
            self.issue_at = now
            self.fell_back = quality == CONVERTED
            self.went_down(now)
            return Step(RESTART, quality, None, quality != CONVERTED, None)
        if lower is None or self.dropped_told:
            return None
        self.dropped_told = True
        self.next_title = None
        return Step(NEXT, lower, None, False, kbps_of(lower))


class Meter:
    """What the proxy passes on of each stream, for Automatic: bytes as
    they arrive, and each answer's size over the time it took. Paths are
    the server's; the service asks about those of the playback."""

    def __init__(self, clock=time.monotonic, keep: float = 240.0):
        self.clock = clock
        self.keep = keep
        self.lock = threading.Lock()
        self.chunks = collections.deque()
        self.answers = collections.deque()

    def prune(self, now: float) -> None:
        while self.chunks and now - self.chunks[0][0] > self.keep:
            self.chunks.popleft()
        while self.answers and now - self.answers[0][0] > self.keep:
            self.answers.popleft()

    def chunk(self, path: str, size: int) -> None:
        now = self.clock()
        with self.lock:
            self.chunks.append((now, path, size))
            self.prune(now)

    def answered(self, path: str, size: int, seconds: float) -> None:
        """A whole answer: `size` bytes in `seconds`, from its first byte
        to its last, counting the time Kodi took to take them (so a
        measure is never more than the network carried)."""
        now = self.clock()
        with self.lock:
            self.answers.append((now, path, size, seconds))
            self.prune(now)

    def arriving_kbps(self, prefix: str, window: float = DELIVERY_WINDOW) -> Optional[float]:
        """What arrived of `prefix`'s answers over the last `window` seconds, in kbps."""
        now = self.clock()
        with self.lock:
            found = [(t, n) for t, path, n in self.chunks if path.startswith(prefix) and now - t <= window]
        if not found:
            return None
        return sum(n for _, n in found) * 8 / 1000.0 / window

    def carried_kbps(self, prefix: str, since: float, least: int = 3, smallest: int = 256 * 1024) -> Optional[float]:
        """What the network carried since `since`, in kbps: the `least`-th
        fastest answer of `prefix` of 256 KB or more (a 2 s segment at
        2 Mbps is 500 KB), so one lucky answer doesn't count; None with
        fewer such answers."""
        with self.lock:
            rates = sorted(
                (n * 8 / 1000.0 / s for t, path, n, s in self.answers if t >= since and path.startswith(prefix) and n >= smallest and s > 0),
                reverse=True,
            )
        if len(rates) < least:
            return None
        return rates[least - 1]


def remembered_ceiling(value: str, now: float) -> Optional[float]:
    """A remembered ceiling ("kbps@time") still in force at `now` (wall clock)."""
    try:
        kbps, at = value.split("@")
        kbps, at = float(kbps), float(at)
    except ValueError:
        return None
    if kbps <= 0 or now - at > REMEMBER_FOR or at - now > 60:
        return None
    return kbps


def remember_ceiling(kbps: float, now: float) -> str:
    return "%d@%d" % (kbps, now)


def network_ceiling(answer) -> Optional[float]:
    """The ceiling in the server's word on this Kodi's network
    (GET /api/playback/network), None for no limit."""
    if not isinstance(answer, dict):
        return None
    kbps = answer.get("ceiling_kbps")
    if isinstance(kbps, bool) or not isinstance(kbps, (int, float)) or kbps <= 0:
        return None
    return float(kbps)


def wants_probe(answer) -> bool:
    return isinstance(answer, dict) and answer.get("probe") is True and not answer.get("local")


class SpeedTest:
    """A speed test's body as it arrives: `chunk` with each piece read,
    `done` says when to stop (all of it, or PROBE_SECONDS after the first
    byte), `result` is what to tell the server. The first piece holds what
    arrived before the clock started (through the proxy, up to 256 KB), so
    the speed is of the bytes after it over the time since it."""

    def __init__(self, total: int = PROBE_BYTES):
        self.total = total
        self.size = 0
        self.first_at = None
        self.first_size = 0
        self.last_at = None

    def chunk(self, now: float, size: int) -> None:
        if size <= 0:
            return
        if self.first_at is None:
            self.first_at = now
            self.first_size = size
        self.size += size
        self.last_at = now

    def until(self, deadline: float) -> float:
        """When to stop reading at the latest."""
        if self.first_at is None:
            return deadline
        return min(deadline, self.first_at + PROBE_SECONDS)

    def complete(self) -> bool:
        return self.size >= self.total and self.first_at is not None and self.last_at - self.first_at <= PROBE_SECONDS

    def done(self, now: float) -> bool:
        return self.size >= self.total or (self.first_at is not None and now - self.first_at >= PROBE_SECONDS)

    def result(self, now: Optional[float] = None) -> Optional[dict]:
        """{probe_kbps, probe_complete}, or None when under 256 KiB came
        (nothing is known then)."""
        if self.size < PROBE_LEAST or self.first_at is None:
            return None
        complete = self.complete()
        end = self.last_at if complete or now is None else max(now, self.last_at)
        seconds = end - self.first_at
        counted = self.size - self.first_size
        if seconds <= 0 or counted <= 0:
            # All of it in one piece: as fast as can be told.
            seconds, counted = 0.001, self.size
        kbps = int(round(counted * 8 / seconds / 1000))
        return {"probe_kbps": min(max(kbps, 1), 10_000_000), "probe_complete": complete}


def probe_ceiling(result: dict) -> Optional[float]:
    """The ceiling a speed test gives when the server can't say: none for
    one that came whole in time, else 70 % of what it measured."""
    if result.get("probe_complete"):
        return None
    return result["probe_kbps"] * PROBE_SHARE


def media_prefix(url: str) -> str:
    """The paths a playback's answers come from: an HLS stream's folder,
    or the file played as it is."""
    path = url.split("?")[0]
    if path.endswith(".m3u8"):
        return path.rsplit("/", 1)[0] + "/"
    return path
