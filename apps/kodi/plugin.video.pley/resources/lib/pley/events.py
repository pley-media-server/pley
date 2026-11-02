"""The server's event stream (docs/API.md, Activity & events), which the
service holds open: what the person's other devices change shows here too,
and a stream stopped from elsewhere stops at once."""

from __future__ import annotations

import json
import threading
import time

from . import api, auth, kodi
from .proxy import hard_close

# The server sends a comment every 15 seconds: with nothing for this long,
# the connection is gone.
READ_TIMEOUT = 45
# Seconds between tries while the server can't be reached, the last one
# repeated.
RETRY = (1, 2, 5, 10, 30)
# A connection that held this long was a good one: tries count again.
HELD = 60
# Events that change what pley's listings show (Live TV's: its channels
# or guide changed, an administrator's lineup among them).
CHANGES = ("user_state_changed", "watchlist_changed", "library_changed", "libraries_changed", "live_channels_changed")
# Events come in bursts (a play reaching 90 % sends user_state_changed and
# library_changed): the listing is asked for once, this long after the
# first, and no more often than every MIN_GAP seconds (a scan).
GATHER = 0.3
MIN_GAP = 2.0


def events_of(lines):
    """The JSON events of an event stream's lines (Server-Sent Events: an
    event's `data:` lines, then an empty line)."""
    data = []
    for line in lines:
        if not line:
            if data:
                try:
                    event = json.loads("\n".join(data))
                except ValueError:
                    event = None
                if isinstance(event, dict):
                    yield event
            data = []
        elif line.startswith("data:"):
            value = line[5:]
            data.append(value[1:] if value.startswith(" ") else value)


class Events:
    """`GET /api/events` of the server signed in to, held open by a thread
    of its own, which calls `heard(event)` for each event, and `opened()`
    each time the stream opens (nothing it missed is replayed)."""

    def __init__(self, tokens: auth.Tokens, user_agent: str, heard, retry=RETRY, read_timeout=READ_TIMEOUT, opened=None):
        self.tokens = tokens
        self.user_agent = user_agent
        self.heard = heard
        self.opened = opened
        self.retry = retry
        self.read_timeout = read_timeout
        self.stopping = threading.Event()
        # Set while the server's stream is open.
        self.open = threading.Event()
        self.lock = threading.Lock()
        self.sock = None
        self.thread = threading.Thread(target=self.run, name="pley-events", daemon=True)

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        """Ends the stream at once: Kodi gives a service 5 seconds to stop."""
        self.stopping.set()
        with self.lock:
            sock, self.sock = self.sock, None
        if sock is not None:
            hard_close(sock)
        self.thread.join(2)

    def run(self) -> None:
        tries = 0
        while not self.stopping.is_set():
            server = self.tokens.server()
            if not server:
                # Signed out: the plugin may sign in again.
                self.stopping.wait(5)
                continue
            began = time.monotonic()
            try:
                self.listen(server)
            except auth.SignedOut:
                continue
            except Exception as e:  # noqa: BLE001 - whatever broke it, it is tried again
                if isinstance(e, OSError) or (isinstance(e, api.ApiError) and e.status == 0):
                    # Nothing answered: the server proves itself again before the next token.
                    self.tokens.doubt()
                if tries == 0 and not self.stopping.is_set():
                    kodi.log("event stream: %s" % e)
            if time.monotonic() - began >= HELD:
                tries = 0
            self.stopping.wait(self.retry[min(tries, len(self.retry) - 1)])
            tries += 1

    def listen(self, server: str) -> None:
        """One connection, until the stream ends (the token expired, the
        server stopped) or another sign-in, server or profile takes over:
        then its next line (the server's comment every 15 seconds at the
        latest) ends it, unread."""
        who = self.tokens.identity()
        token = self.tokens.access()
        for attempt in range(2):
            conn = api.connect(server, self.read_timeout)
            try:
                with self.lock:
                    if self.stopping.is_set():
                        return
                    self.sock = conn.sock
                headers = {
                    "User-Agent": self.user_agent,
                    "Accept": "text/event-stream",
                    "Authorization": "Bearer " + token,
                }
                conn.request("GET", "/api/events", headers=headers)
                answer = conn.getresponse()
                if answer.status == 401 and not attempt:
                    token = self.tokens.refreshed(token)
                    continue
                if answer.status != 200:
                    raise api.ApiError(answer.status, "events", "the server answered %d" % answer.status)
                self.open.set()
                if self.opened is not None:
                    self.opened()
                for event in events_of(self.lines(answer, who)):
                    self.heard(event)
                return
            finally:
                self.open.clear()
                with self.lock:
                    self.sock = None
                conn.close()

    def lines(self, answer, who):
        while not self.stopping.is_set():
            raw = answer.readline(65536)
            if not raw:
                return
            yield raw.decode("utf-8", "replace").rstrip("\r\n")
            if self.tokens.identity() != who:
                return


class Refresher:
    """Refreshes the pley listing on screen (`refresh()`): for events, once
    for a burst of them, unless it was refreshed since they came, which
    showed what they said."""

    def __init__(self, refresh, busy=lambda: False, gather=GATHER, min_gap=MIN_GAP, clock=time.monotonic):
        self.refresh = refresh
        # While a video plays, the listing behind it waits: playback's end
        # refreshes it.
        self.busy = busy
        self.gather = gather
        self.min_gap = min_gap
        self.clock = clock
        self.lock = threading.Lock()
        self.timer = None
        self.asked = None
        self.refreshed = None

    def later(self) -> None:
        """An event says what the listings show changed."""
        with self.lock:
            if self.timer is not None:
                return
            if self.asked is None:
                self.asked = self.clock()
            wait = self.gather
            if self.refreshed is not None:
                wait = max(wait, self.refreshed + self.min_gap - self.clock())
            self.timer = threading.Timer(wait, self.fire)
            self.timer.daemon = True
            self.timer.start()

    def fire(self) -> None:
        with self.lock:
            self.timer = None
            asked, self.asked = self.asked, None
            if asked is None or (self.refreshed is not None and self.refreshed >= asked):
                return
        if not self.busy():
            self.now()

    def now(self) -> None:
        """Refreshes at once (playback ended)."""
        with self.lock:
            self.refreshed = self.clock()
        self.refresh()

    def cancel(self) -> None:
        with self.lock:
            if self.timer is not None:
                self.timer.cancel()
                self.timer = None
