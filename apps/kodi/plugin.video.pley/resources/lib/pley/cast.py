"""Being cast to (docs/API.md, "Casting to another device"): a phone or a
browser of the profile sends this Kodi a movie, an episode or an extra,
then pauses, seeks and stops it, and sees what it plays. The service's
event stream says this Kodi may be cast to (`query`) and hears what it is
sent; what plays here is told back (`Reporter`). Kodi doesn't cast itself."""

from __future__ import annotations

import os
import re
import time

import xbmc

from . import kodi
from .listing import MUSIC

PLATFORM = "kodi"
# This Kodi's id among the profile's devices, made once (remembered.json).
DEVICE = "cast_device"
# What the server takes of a name, in characters.
NAME_MAX = 64
ACTIONS = ("pause", "resume", "seek", "stop")
# While a video plays, what it plays is told this often.
REPORT_EVERY = 10


def device_id() -> str:
    """This Kodi as the profile's devices know it: the same from one start
    to the next."""
    found = kodi.remembered(DEVICE)
    if re.fullmatch(r"[0-9a-f]{32}", found):
        return found
    made = os.urandom(16).hex()
    kodi.remember(DEVICE, made)
    return made


def device_name() -> str:
    """What people call this Kodi: its name in Settings → Services →
    General, as the server takes it."""
    name = "".join(c for c in xbmc.getInfoLabel("System.FriendlyName") if c.isprintable()).strip()
    return name[:NAME_MAX].strip() or "Kodi"


def query(device: str) -> dict:
    """The event stream's query that lists this Kodi to cast to while the
    stream is open."""
    return {"cast_device": device, "cast_name": device_name(), "cast_platform": PLATFORM}


def whole(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def play_of(event: dict, device: str):
    """What a `cast_play` for `device` asks to play: (item id, file id or
    None, position or None for where the profile left off); None for
    anything else."""
    if event.get("device_id") != device:
        return None
    item_id, file_id, position_ms = event.get("item_id"), event.get("file_id"), event.get("position_ms")
    if not whole(item_id) or not item_id:
        return None
    if file_id is not None and not whole(file_id):
        return None
    if position_ms is not None and not whole(position_ms):
        return None
    return item_id, file_id, position_ms


def control_of(event: dict, device: str):
    """What a `cast_control` for `device` asks of the player: (action,
    position for a seek, else None); None for anything else."""
    if event.get("device_id") != device or event.get("action") not in ACTIONS:
        return None
    action, position_ms = event["action"], event.get("position_ms")
    if action != "seek":
        return action, None
    return (action, position_ms) if whole(position_ms) else None


class Loading:
    """A title sent here, being asked for: it buffers until it plays."""


class Reporter:
    """What this Kodi plays, told to the server for the devices that cast
    to it (`send(body)`, never waited for): a video of pley's as it loads,
    starts, pauses, resumes and seeks, every 10 seconds while it plays,
    and nothing once it stops. Music and Live TV aren't told."""

    def __init__(self, send, clock=time.monotonic):
        self.send = send
        self.clock = clock
        # What is told of: a playback (the service's `playing`), a Loading,
        # or None.
        self.current = None
        self.body = None
        self.last = None

    def report(self, body: dict) -> None:
        self.body = body
        self.last = self.clock()
        self.send(body)

    def told(self, item_id: int, duration_ms, position_ms: float, state: str) -> None:
        self.report(
            {
                "item_id": item_id,
                "position_ms": int(max(position_ms, 0)),
                "duration_ms": int(duration_ms) if duration_ms else None,
                "state": state,
            }
        )

    def loading(self, item_id: int, position_ms: float) -> Loading:
        """A title sent here, or picked here, starts loading: what plays
        before it is over, without being told it stopped."""
        self.current = Loading()
        self.told(item_id, None, position_ms, "buffering")
        return self.current

    def started(self, playing: dict, position_ms: float, paused: bool = False) -> None:
        if playing.get("kind") in MUSIC:
            return
        self.current = playing
        self.told(playing["item_id"], playing.get("duration_ms"), position_ms, "paused" if paused else "playing")

    def at(self, playing: dict, position_ms: float, paused: bool, caching: bool = False) -> None:
        """Where `playing` is: told as it pauses, resumes or waits for the
        network, and every 10 seconds while it plays."""
        if self.current is not playing:
            return
        state = "paused" if paused else "buffering" if caching else "playing"
        if state != self.body["state"] or (state != "paused" and self.clock() - self.last >= REPORT_EVERY):
            self.told(playing["item_id"], playing.get("duration_ms"), position_ms, state)

    def moved(self, playing: dict, position_ms: float) -> None:
        """`playing` was moved to `position_ms`."""
        if self.current is playing:
            self.told(playing["item_id"], playing.get("duration_ms"), position_ms, self.body["state"])

    def stopped(self, what) -> None:
        """`what` (a playback or a Loading) is over: nothing plays, unless
        something else took over meanwhile."""
        if what is None or self.current is not what:
            return
        self.current = None
        self.report({"item_id": None})

    def failed(self) -> None:
        """Kodi couldn't play what was loading."""
        if isinstance(self.current, Loading):
            self.stopped(self.current)

    def again(self) -> None:
        """The event stream opened again, and the server knows nothing of
        what plays here: told again."""
        if self.current is None or self.body is None:
            return
        if isinstance(self.current, Loading) or self.body["state"] == "paused":
            self.report(self.body)
        else:
            # Moving on: told on the loop's next turn, where it is then.
            self.last = float("-inf")
