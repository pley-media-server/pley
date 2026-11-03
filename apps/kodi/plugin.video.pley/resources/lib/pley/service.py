"""The service: it holds the sign-in, runs the proxy, and tells the server
where playback is (docs/API.md, Playback): every 10 seconds while playing,
every 30 while paused (the server ends a session it hasn't heard of for 90
seconds), and when it stops, after which the session ends."""

from __future__ import annotations

import json
import os
import queue
import re
import threading
import time
import urllib.parse

import xbmc

from . import api, auth, autoquality, kodi, news, overlay, prompts, requests, tracks
from .events import CHANGES, Events, Refresher
from .proxy import Proxy, new_secret

REPORT_EVERY = 10
PAUSED_EVERY = 30
# Requests while playing may take this long; when Kodi quits, the last
# report and the session's end get this together (Kodi kills a service
# that takes over 5 seconds to stop).
TIMEOUT = 5
LAST_TIMEOUT = 2
# Ended this close to the end (or in the credits), a title was watched to
# its end; ended earlier (a stream that broke off), it was not.
ENDED_FROM = 0.9
# Seconds a tracked playback may seem stopped before it counts as stopped,
# when Kodi didn't say so.
LOST_AFTER = 3
# Waits that aren't the network's: Kodi seeking, or playing faster or backwards.
MOVING = ("Player.Seeking", "Player.Forwarding", "Player.Rewinding")
# Someone taking a live channel's connection over says so with an event
# before the server ends its stream: an end waits this long for one before
# it says the server stopped the channel.
TAKEN_GRACE = 1
# A take-over's events (one for each viewer of the channel) come together:
# within this long, they are the one.
TAKEN_ONCE = 15
FULLSCREEN = "Window.IsActive(fullscreenvideo) + !Window.IsVisible(videoosd) + !Window.IsVisible(seekbar)"


class Reports:
    """Progress reports for one playback, by the rules above. `send(path,
    body)` and `end(path)` reach the server."""

    def __init__(self, playing: dict, send, end, clock=time.monotonic):
        self.playing = playing
        self.send = send
        self.end = end
        self.clock = clock
        self.position_ms = 0
        self.last = None
        self.paused = False
        self.credits_from = None

    def body(self, state: str) -> dict:
        return {
            "position_ms": int(self.position_ms),
            "duration_ms": int(self.playing.get("duration_ms") or 0),
            "state": state,
            "session_id": self.playing["session_id"],
        }

    def report(self, state: str) -> None:
        self.last = self.clock()
        self.send("/api/items/%d/progress" % self.playing["item_id"], self.body(state))

    def at(self, position_ms: float) -> None:
        """Playback is at `position_ms`: reported once 10 seconds went by,
        or 30 while paused."""
        if not self.paused:
            self.position_ms = position_ms
        every = PAUSED_EVERY if self.paused else REPORT_EVERY
        if self.last is None or self.clock() - self.last >= every:
            self.report("paused" if self.paused else "playing")

    def pause(self) -> None:
        self.paused = True
        self.report("paused")

    def resume(self) -> None:
        self.paused = False
        self.report("playing")

    def near_end(self) -> bool:
        duration = self.playing.get("duration_ms") or 0
        if self.credits_from is not None and self.position_ms >= self.credits_from:
            return True
        return bool(duration) and self.position_ms >= ENDED_FROM * duration

    def finish(self, ended: bool) -> None:
        """The last report: at the end for a title that ended near it, where
        it was otherwise; then the session ends."""
        if ended and self.near_end() and self.playing.get("duration_ms"):
            self.position_ms = self.playing["duration_ms"]
        try:
            self.report("stopped")
        finally:
            self.end("/api/playback/%s" % self.playing["session_id"])


class Worker:
    """The service's requests, one at a time, off the thread that Kodi's
    callbacks, the loop and the buttons share: a server that is slow or
    gone never holds them up."""

    def __init__(self):
        self.jobs = queue.Queue()
        self.thread = threading.Thread(target=self.run, name="pley-worker", daemon=True)
        self.thread.start()

    def put(self, job) -> None:
        self.jobs.put(job)

    def run(self) -> None:
        while True:
            job = self.jobs.get()
            if job is None:
                return
            try:
                job()
            except Exception as e:  # noqa: BLE001 - one job never ends the others
                kodi.log("request failed: %r" % e, xbmc.LOGWARNING)

    def stop(self) -> None:
        """Lets what is queued go (the last report), then ends."""
        self.jobs.put(None)
        self.thread.join(4)


class Player(xbmc.Player):
    def __init__(self, tokens: auth.Tokens, user_agent: str, worker: Worker, refresher: Refresher | None = None):
        super().__init__()
        self.tokens = tokens
        self.user_agent = user_agent
        self.worker = worker
        self.refresher = refresher
        self.lock = threading.Lock()
        self.reports = None
        self.moments = None
        # The playback's audio and subtitles (tracks.Follower).
        self.tracks = None
        # Up Next after an episode that ended on its own.
        self.after_end = None
        self.overlay = None
        self.shown = None
        # The button over the video, which the loop and a press both change.
        self.ui = threading.RLock()
        self.lost_since = None
        self.fullscreen = False
        # The playback last stopped by the server, and when.
        self.stopped_for = None
        self.stopped_at = float("-inf")
        # When a live channel was last refused for want of a connection.
        self.busy_at = float("-inf")
        # The live channel last taken over, and when; the one whose stream
        # the server last ended, and when.
        self.taken = (None, float("-inf"))
        self.live_end = (None, float("-inf"))
        # Automatic (docs/QUALITY.md): the playback's watch and what it
        # was given (playback.automatic), what the proxy counts, and the
        # title being played again at another quality: (item id, watch,
        # what to say once it plays).
        self.watch = None
        self.auto = None
        self.clock = time.monotonic
        self.meter = autoquality.Meter()
        self.restarting = None

    # ------------------------------------------------------------ requests

    def call(self, method: str, path: str, body=None, timeout: float = TIMEOUT, fine=(410,)):
        """One request to the server, the token renewed once if refused: its
        answer, or None when signed out or out of reach. Refusals but `fine`
        ones are logged."""
        try:
            if not self.tokens.server():
                return None
            server, token = self.tokens.session()
            for attempt in range(2):
                response = api.Client(server, self.user_agent, token, timeout=timeout).request(method, path, json_body=body)
                if response.status != 401 or attempt:
                    if response.status >= 400 and response.status not in fine:
                        kodi.log("%s %s: %d" % (method, path, response.status), xbmc.LOGWARNING)
                    return response
                token = self.tokens.refreshed(token, server)
        except (api.ApiError, auth.SignedOut) as e:
            if isinstance(e, api.ApiError) and e.status == 0:
                # Nothing answered: the server proves itself again before the next token.
                self.tokens.doubt()
            kodi.log("%s %s failed: %s" % (method, path, e), xbmc.LOGWARNING)
        return None

    def send(self, playing: dict, path: str, body: dict) -> None:
        """A progress report, from the worker. A stream an administrator
        stopped answers 410: a file played as it is hears it only here."""
        last = body.get("state") == "stopped"
        response = self.call("POST", path, body, LAST_TIMEOUT if last else TIMEOUT)
        if response is not None and response.status == 410 and not last:
            try:
                message = response.json().get("message")
            except (ValueError, AttributeError):
                message = None
            self.stopped_by_server(message)

    def end(self, path: str) -> None:
        # 404: over already, as when this Kodi asked to play the next episode
        # (the server ends a player's session when it starts another).
        self.call("DELETE", path, timeout=LAST_TIMEOUT, fine=(404, 410))

    def stopped_by_server(self, message) -> None:
        """A stream the server stopped: said once for each playback, however
        many of its requests are refused (and once in a while for requests
        refused after it ended)."""
        with self.lock:
            playing = self.reports
        now = time.monotonic()
        if playing is not None and playing is self.stopped_for:
            return
        if playing is None and now - self.stopped_at < 15:
            return
        self.stopped_for, self.stopped_at = playing, now
        kodi.notify(message or kodi.text("stream_stopped"), error=True)
        self.stop()

    def live_ended(self, path: str = "") -> None:
        """A live channel the server ended, from the proxy before Kodi reads
        the end: Kodi's queue of channels is let go, or Kodi would tune the
        next one, taking the connection again. Someone taking its connection
        over says so with an event, which comes first (taken_over); without
        one within a second, the server stopped it (an administrator, or its
        provider gave up), and that is said."""
        from . import livetv

        livetv.forget_queue()
        channel = livetv.channel_of(path)
        now = time.monotonic()
        self.live_end = (channel, now)

        def say():
            taken, at = self.taken
            if channel is not None and taken == channel and at > now - TAKEN_ONCE:
                return
            kodi.notify(kodi.text("stream_stopped"), error=True)

        later = threading.Timer(TAKEN_GRACE, say)
        later.daemon = True
        later.start()

    def taken_over(self, channel_id, message) -> None:
        """Someone took over the connection of a live channel this account
        watches (the server's live_taken_over, one for each viewer of the
        channel, by channel: Kodi's stream never learns its session). The
        channel playing here (or whose stream just ended) stops at once,
        Kodi's queue of channels is let go, and the server's words are said
        once, with Watch again, which plays the channel as any is played:
        every connection in use says so. It never takes a connection over
        by itself."""
        from . import livetv

        if not isinstance(channel_id, int):
            return
        now = time.monotonic()
        playing = livetv.channel_of(self.playing_file())
        ended, ended_at = self.live_end
        if playing != channel_id and not (ended == channel_id and now - ended_at < TAKEN_ONCE):
            return
        taken, at = self.taken
        if taken == channel_id and now - at < TAKEN_ONCE:
            return
        self.taken = (channel_id, now)
        livetv.forget_queue()
        if playing == channel_id:
            self.stop()

        def say():
            if livetv.taken_over(message):
                self.play(livetv.play_url(channel_id))

        threading.Thread(target=say, name="pley-live-taken", daemon=True).start()

    def busy(self, message, connections=(), path: str = "") -> None:
        """A live channel Kodi asked for when every connection of its source
        was in use (the plugin asked first, and one was free then): said as
        the plugin says it, once in a few seconds, off the proxy's thread;
        the queue of channels after it is let go. Watch here instead plays
        the channel again, taking that connection over."""
        now = time.monotonic()
        if now - self.busy_at < 5:
            return
        self.busy_at = now
        from . import livetv

        channel = livetv.channel_of(path)

        def say():
            livetv.forget_queue()
            take_over = livetv.busy(message, connections)
            if take_over is not None and channel is not None:
                self.play(livetv.play_url(channel, take_over=take_over))

        threading.Thread(target=say, name="pley-live-busy", daemon=True).start()

    def heard(self, event: dict) -> None:
        """An event of the server's stream, from its thread."""
        kind = event.get("type")
        if kind in CHANGES:
            if self.refresher is not None:
                self.refresher.later()
        elif kind == "session_stopped":
            with self.lock:
                playing = self.reports.playing if self.reports is not None else None
            if playing is None or event.get("session_id") != playing.get("session_id"):
                return
            by = "stopped_by_admin" if event.get("by_admin") else "stopped_elsewhere"
            self.stopped_by_server(event.get("message") or kodi.text(by))
        elif kind == "live_taken_over":
            self.taken_over(event.get("channel_id"), event.get("message"))

    # ----------------------------------------------------- what is playing

    def playing_file(self) -> str:
        try:
            return self.getPlayingFile().split("|")[0] if self.isPlaying() else ""
        except RuntimeError:
            return ""

    def position(self) -> float:
        try:
            # Kodi says a little before 0 as a stream starts.
            return max(self.getTime() * 1000, 0)
        except RuntimeError:
            return 0

    def onAVStarted(self) -> None:
        file = self.playing_file()
        with self.lock:
            if self.reports is not None and file != self.reports.playing["path"]:
                # Something else plays now: the one before is over.
                self._finish_locked(ended=False)
        raw = kodi.get_property(kodi.PLAYING)
        if not raw:
            return
        try:
            playing = json.loads(raw)
        except ValueError:
            return
        if not file or file != playing.get("path"):
            return
        kodi.clear_property(kodi.PLAYING)
        reports = Reports(
            playing,
            lambda path, body: self.worker.put(lambda: self.send(playing, path, body)),
            lambda path: self.worker.put(lambda: self.end(path)),
        )
        moments = prompts.Moments(playing.get("markers") or [])
        reports.credits_from = moments.credits_from()
        follower = None
        if playing.get("tracks"):
            follower = tracks.Follower(
                playing["tracks"], kodi.rpc, lambda body: self.worker.put(lambda: self.save_tracks(playing, body))
            )
        watch, notice = self.watching(playing)
        with self.lock:
            self.after_end = None
            self.lost_since = None
            self.reports = reports
            self.moments = moments
            self.tracks = follower
            self.watch = watch
            self.auto = playing.get("auto") if watch is not None else None
            reports.at(self.position())
        if notice:
            kodi.notify(notice)
        if playing.get("kind") == "episode":
            self.worker.put(lambda: self.find_next(reports, moments))
        if follower is not None:
            # The account's audio and subtitles, as soon as Kodi lists them
            # (else on the loop's next turns).
            follower.turn()

    def watching(self, playing: dict):
        """Automatic's watch over a playback that started, and what to say
        of it: the title's own, kept, when the service played it again at
        another quality (the notice of a step down); a new one otherwise."""
        restarting, self.restarting = self.restarting, None
        auto = playing.get("auto")
        if not auto:
            return None, None
        now = self.clock()
        stream = dict(live=bool(auto.get("live")), lines=auto.get("lines"), converted=auto.get("converted"))
        if restarting is not None and restarting[0] == playing.get("item_id"):
            _, watch, notice = restarting
            watch.restarted(now, auto["quality"], auto.get("playing_kbps"), **stream)
            return watch, notice
        watch = autoquality.Watch(
            now,
            auto["quality"],
            auto.get("playing_kbps"),
            auto.get("file_kbps"),
            auto.get("file_lines"),
            auto.get("cap"),
            lowered=bool(auto.get("lowered")),
            **stream,
        )
        return watch, None

    def remember(self, auto: dict, kbps) -> None:
        """The ceiling for the next titles on this server (None forgets
        it), kept here and told to the server, on the worker."""
        from . import playback

        server = auto.get("server") or ""

        def both():
            playback.remember_ceiling(server, kbps)
            playback.tell_ceiling(server, kbps, self.user_agent)

        self.worker.put(both)

    def read_ahead(self):
        """Seconds Kodi has read ahead of the position, where its player
        keeps a cache (None where it doesn't say)."""
        found = kodi.rpc("Player.GetProperties", {"playerid": 1, "properties": ["percentage", "cachepercentage", "totaltime"]})
        if not isinstance(found, dict):
            return None
        cached = found.get("cachepercentage") or 0
        at = found.get("percentage") or 0
        total = found.get("totaltime") or {}
        seconds = (
            total.get("hours", 0) * 3600 + total.get("minutes", 0) * 60 + total.get("seconds", 0) + total.get("milliseconds", 0) / 1000.0
        )
        if cached <= 0 or seconds <= 0 or cached >= 99.5:
            # No cache, or all of the rest read: nothing more to arrive.
            return None
        return max(cached - at, 0) / 100.0 * seconds

    def automatic(self, watch: autoquality.Watch, auto: dict, position: float, paused: bool) -> None:
        """Automatic's turn, from the loop: what Kodi says of the playback,
        and the step the watch says (take)."""
        if self.restarting is not None:
            return
        seeking = any(xbmc.getCondVisibility(c) for c in MOVING)
        caching = xbmc.getCondVisibility("Player.Caching")
        ahead = None if paused else self.read_ahead()
        prefix = auto.get("prefix") or ""
        arriving = self.meter.arriving_kbps(prefix)
        carried = None
        shown = None
        if watch.quality != autoquality.ORIGINAL or watch.converted:
            # Since the last issue, change or raise of where the next title starts.
            carried = self.meter.carried_kbps(prefix, max(watch.issue_at, watch.changed_at, watch.raised_at or 0))
        else:
            # Over the last 2 minutes: a wait of the file as it is counts only on a network too slow for it.
            shown = self.meter.carried_kbps(prefix, self.clock() - autoquality.ORIGINAL_NETWORK_WINDOW)
        with self.lock:
            # Under the lock the server's answers to a bitrate step take.
            step = watch.tick(self.clock(), position / 1000.0, paused, seeking, caching, ahead, arriving, carried, shown)
            reports = self.reports
            if step is None or reports is None or self.watch is not watch:
                return
            playing = reports.playing
        self.take(watch, auto, playing, step, position, carried)

    def take(self, watch: autoquality.Watch, auto: dict, playing: dict, step: autoquality.Step, position: float, carried=None) -> None:
        """Carries out Automatic's step: the stream's bitrate changed as it
        is sent, where the next title starts, or the title again at another
        quality. A step down says so; the network's memory follows each."""
        if step.memory is not None:
            # Up to a quality keeps it too: the next title starts there, not
            # under the older, lower ceiling.
            self.remember(auto, step.memory or None)
        why = "network" if step.down else "%s kbps carried" % int(carried or 0)
        if step.kind == autoquality.NEXT:
            kodi.log("automatic: the next title starts at %s" % step.quality)
            return
        if step.kind == autoquality.BITRATE:
            kodi.log("automatic: %s to %d kbps as it is sent at %d s (%s)" % ("down" if step.down else "up", step.kbps, position / 1000, why))
            if step.down:
                kodi.notify(kodi.text("smoother") % autoquality.bitrate_label(step.kbps, watch.lines))
            session = playing["session_id"]
            self.worker.put(lambda: self.set_bitrate(watch, session, step.kbps))
            return
        notice = kodi.text("smoother") % autoquality.label(step.quality) if step.down else None
        kodi.log("automatic: %s at %d s (%s)" % ("converted, as the player gave up" if step.quality == autoquality.CONVERTED else "down to " + step.quality, position / 1000, why))
        item_id = playing["item_id"]
        self.restarting = (item_id, watch, notice)
        self.worker.put(lambda: self.restart(item_id, position, step.quality))

    def set_bitrate(self, watch: autoquality.Watch, session: str, kbps: int) -> None:
        """PUT /api/playback/{session}/bitrate (from the worker): the stream
        goes on at `kbps` from the segments Kodi hasn't fetched yet, the same
        size and playlist. The watch hears what it goes on at, or that the
        server refused (one from before, a session gone)."""
        path = "/api/playback/%s/bitrate" % urllib.parse.quote(session, safe="")
        response = self.call("PUT", path, {"max_bitrate": int(kbps)}, fine=(400, 404, 410))
        got = None
        if response is not None and response.status < 300:
            try:
                got = (response.json() or {}).get("bitrate")
            except (ValueError, AttributeError):
                got = None
            if isinstance(got, bool) or not isinstance(got, (int, float)) or got <= 0:
                # Changed, without saying to what: as asked.
                got = kbps
        else:
            kodi.log("automatic: the bitrate of %s can't change as it plays" % session, xbmc.LOGWARNING)
        with self.lock:
            watch.answered(kbps, got)

    def restart(self, item_id: int, position_ms: float, quality: str) -> None:
        """Plays the title again where it is, at `quality` (from the
        worker): a new session, which ends the one before."""
        from . import playback

        if not self.still_playing(item_id):
            self.restarting = None
            return
        base = kodi.get_property(kodi.PROXY)
        try:
            # The settings as they are now (the service's are read once).
            kodi.begin()
            client = api.Client(base, self.user_agent, timeout=60, local=True)
            li = playback.prepare(client, base, item_id, int(position_ms), restart=quality)
        except (api.ApiError, OSError, ValueError, KeyError) as e:
            kodi.log("automatic: couldn't play %d at %s: %r" % (item_id, quality, e), xbmc.LOGWARNING)
            self.restarting = None
            return
        if not self.still_playing(item_id):
            # Stopped meanwhile: the new session isn't played, and ends.
            self.restarting = None
            try:
                session = json.loads(kodi.get_property(kodi.PLAYING))["session_id"]
            except (ValueError, KeyError, TypeError):
                return
            kodi.clear_property(kodi.PLAYING)
            self.end("/api/playback/%s" % session)
            return
        li.setProperty("StartOffset", "%.3f" % (position_ms / 1000.0))
        self.play(li.getPath(), li)

    def still_playing(self, item_id: int) -> bool:
        with self.lock:
            return self.reports is not None and self.reports.playing["item_id"] == item_id

    def onPlayBackSeek(self, time_ms, offset_ms) -> None:
        self.moved()

    def onPlayBackSeekChapter(self, chapter) -> None:
        self.moved()

    def onPlayBackSpeedChanged(self, speed) -> None:
        self.moved()

    def moved(self) -> None:
        """A seek, a chapter or a new speed: the wait that follows isn't the network's."""
        watch = self.watch
        if watch is not None:
            watch.seeked()

    def save_tracks(self, playing: dict, body: dict) -> None:
        """Audio or subtitles changed in Kodi's player: remembered for the
        show or the movie (from the worker)."""
        response = self.call("PUT", "/api/items/%d/streams" % playing["item_id"], body)
        if response is not None and response.status < 300:
            kodi.log("tracks of %d saved: audio %s, subtitles %s" % (playing["item_id"], body.get("audio_index", "as before"), body["subtitle_index"]))

    def find_next(self, reports: Reports, moments: prompts.Moments) -> None:
        """The episode after the one playing, for Up Next (from the worker)."""
        response = self.call("GET", "/api/items/%d/next" % reports.playing["item_id"])
        if response is None or response.status != 200:
            return
        try:
            following = (response.json() or {}).get("item")
        except (ValueError, AttributeError):
            return
        with self.lock:
            if self.reports is reports:
                moments.next_item = following

    def onPlayBackStarted(self) -> None:
        # Anything starting ends an Up Next still counting down.
        with self.lock:
            self.after_end = None

    def onPlayBackPaused(self) -> None:
        with self.lock:
            if self.reports and self.tracked_playing():
                self.reports.position_ms = self.position()
                self.reports.pause()

    def onPlayBackResumed(self) -> None:
        with self.lock:
            if self.reports and self.tracked_playing():
                self.reports.resume()

    def onPlayBackStopped(self) -> None:
        self.finish(ended=False)

    def onPlayBackEnded(self) -> None:
        self.finish(ended=True)

    def onPlayBackError(self) -> None:
        if self.fall_back():
            return
        self.finish(ended=False)

    def fall_back(self) -> bool:
        """Kodi's player gave up on the original in Automatic (a decoder
        failing): the title again where it was, converted at the server's
        own quality, once. Whether it is played again."""
        with self.lock:
            watch, reports = self.watch, self.reports
            if watch is None or reports is None or self.restarting is not None:
                return False
            step = watch.failed(self.clock())
            if step is None:
                return False
            # Kodi stopped: the loop waits for the restart rather than
            # ending the tracked playback (tick).
            self.lost_since = None
            playing, position, auto = reports.playing, reports.position_ms, self.auto
        self.take(watch, auto, playing, step, position)
        return True

    def tracked_playing(self) -> bool:
        """Whether the playback being reported is the one on screen. Kodi
        tells of a stop late, when something else may play already."""
        return self.reports is not None and self.playing_file() == self.reports.playing["path"]

    def finish(self, ended: bool) -> None:
        with self.lock:
            if self.reports is None or self.tracked_playing():
                return
            self._finish_locked(ended)

    def _finish_locked(self, ended: bool) -> None:
        reports, self.reports = self.reports, None
        moments, self.moments = self.moments, None
        follower, self.tracks = self.tracks, None
        self.watch = self.auto = None
        self.lost_since = None
        if follower is not None:
            # Before the last report, on the worker.
            follower.finish()
        if ended and reports.near_end() and moments and moments.next_item and not moments.next_cancelled:
            if self.fullscreen:
                self.after_end = moments
        reports.finish(ended)
        # Listings of pley show the new resume point and watched marks once
        # the server has the last report: after it, on the worker.
        if self.refresher is not None:
            self.worker.put(self.refresher.now)

    # ------------------------------------------------------------ the loop

    def tick(self) -> None:
        """Once a second, from the service's loop: progress, and the button
        over the video."""
        file = self.playing_file()
        # What Kodi shows counts: a pause made while a stream starts is told
        # of before the start, when there is nothing to report yet.
        paused = self.reports is not None and xbmc.getCondVisibility("Player.Paused")
        with self.lock:
            moments = None
            position = 0.0
            follower = None
            watch = auto = None
            if self.reports is not None:
                if file == self.reports.playing["path"]:
                    self.lost_since = None
                    follower = self.tracks
                    watch, auto = self.watch, self.auto
                    position = self.position()
                    if paused and not self.reports.paused:
                        self.reports.position_ms = position
                        self.reports.pause()
                    elif not paused and self.reports.paused:
                        self.reports.resume()
                    self.reports.at(position)
                    moments = self.moments
                elif file:
                    # Something else plays: the tracked one is over.
                    self._finish_locked(ended=False)
                elif self.restarting is not None and self.restarting[0] == self.reports.playing["item_id"]:
                    # Automatic plays it again after Kodi's player gave up
                    # (fall_back): over only if that fails (restart).
                    self.lost_since = None
                else:
                    # Nothing plays: Kodi's word on it is due; past a few
                    # seconds it was lost.
                    if self.lost_since is None:
                        self.lost_since = time.monotonic()
                    elif time.monotonic() - self.lost_since >= LOST_AFTER:
                        self._finish_locked(ended=False)
            elif self.after_end is not None and file:
                # Something else plays now: no Up Next over it.
                self.after_end = None
            if moments is None and self.reports is None:
                moments = self.after_end
            playing = self.reports is not None
        if follower is not None:
            if follower.following and self.subtitle_added():
                # Subtitles added from Kodi's dialog: saved as the choice
                # there, and Kodi's places don't say pley's tracks any more.
                follower.following = False
            # The account's choice shown, and changes made in Kodi's player
            # saved (Kodi asked outside the lock).
            follower.turn()
        if watch is not None:
            self.automatic(watch, auto, position, bool(paused))
        self.fullscreen = bool(playing) and xbmc.getCondVisibility(FULLSCREEN)
        with self.ui:
            self.offer(moments, playing, position)

    def subtitle_added(self) -> bool:
        """Whether the plugin added subtitles to this playback (subtitles.ADDED)."""
        from .subtitles import ADDED

        with self.lock:
            session = self.reports.playing.get("session_id") if self.reports is not None else None
        return bool(session) and kodi.get_property(ADDED) == session

    def offer(self, moments, playing: bool, position: float) -> None:
        if moments is None:
            return self.close_overlay()
        if self.overlay is not None and self.overlay.choice:
            return self.chosen(moments)
        offer = moments.offer(position) if playing else ("next", None)
        if offer is None:
            moments.next_since = None
            return self.close_overlay()
        if playing and not self.fullscreen:
            # The video isn't what's on screen (menus, the OSD): no button
            # takes the remote from them.
            return self.close_overlay()
        kind, marker = offer
        if kind == "skip":
            name = "skip_intro" if marker["kind"] == "intro" else "skip_credits"
            return self.show(("skip", prompts.key(marker), marker), "", kodi.text(name))
        left = moments.counting()
        if left == 0:
            return self.play_next(moments.next_item)
        title = kodi.text("up_next") % prompts.episode_line(moments.next_item)
        self.show(("next",), title, kodi.text("play_now") % left, kodi.text("cancel"))

    def pressed(self, which: overlay.Overlay) -> None:
        """A button was pressed: acted on now, not on the loop's next turn."""
        with self.ui:
            if which is not self.overlay:
                return
            with self.lock:
                moments = self.moments if self.reports is not None else self.after_end
            if moments is not None:
                self.chosen(moments)

    def chosen(self, moments: prompts.Moments) -> None:
        choice, shown = self.overlay.choice, self.shown
        self.close_overlay()
        if shown[0] == "skip":
            moments.dismiss(shown[2])
            if choice == "accept":
                self.seekTime(prompts.target_ms(shown[2]) / 1000.0)
        elif choice == "accept":
            self.play_next(moments.next_item)
        else:
            moments.cancel_next()
            with self.lock:
                self.after_end = None

    def show(self, what: tuple, title: str, accept: str, cancel: str = "") -> None:
        if self.overlay is not None and self.shown[:2] != what[:2]:
            self.close_overlay()
        if self.overlay is None:
            self.overlay = overlay.Overlay("pley-overlay.xml", kodi.addon_dir(), "default", "1080i")
            self.overlay.on_choice = self.pressed
            self.overlay.set_texts(title, accept, cancel)
            self.overlay.show()
        else:
            self.overlay.set_texts(title, accept, cancel)
        self.shown = what

    def close_overlay(self) -> None:
        if self.overlay is not None:
            self.overlay.close()
            self.overlay = None
            self.shown = None

    def play_next(self, item: dict) -> None:
        """Ends this playback where it is, and plays `item`."""
        self.close_overlay()
        with self.lock:
            if self.reports is not None:
                self._finish_locked(ended=False)
            self.moments = None
            self.after_end = None
        self.play("plugin://%s/?route=play&id=%d" % (kodi.ADDON_ID, item["id"]))


def proxy_config(profile: str) -> dict:
    """The proxy's port and secret, kept so that the addresses Kodi caches
    artwork by stay the same from one start to the next."""
    path = os.path.join(profile, "proxy.json")
    try:
        with open(path, encoding="utf-8") as f:
            config = json.load(f)
        if isinstance(config, dict) and config.get("secret") and isinstance(config.get("port"), int):
            return config
    except (OSError, ValueError):
        pass
    return {"port": 0, "secret": new_secret()}


def save_proxy_config(profile: str, config: dict) -> None:
    auth.write_private(os.path.join(profile, "proxy.json"), json.dumps(config))


class QualitySetting:
    """The quality setting as the service last read it (a choice made
    before Automatic carried over as it starts): Automatic chosen again,
    from another quality, starts afresh, here and on the server."""

    def __init__(self, user_agent: str):
        self.user_agent = user_agent
        self.last = None

    def read(self) -> None:
        from . import playback

        kodi.begin()
        now = playback.chosen_quality()
        was, self.last = self.last, now
        if was is not None and was != now and now == autoquality.AUTO:
            playback.picked_automatic(self.user_agent)


class Monitor(xbmc.Monitor):
    """Kodi's word that the add-on's settings changed, that it woke from
    sleep, and the plugin's that it did something to requests
    (requests.ASKING)."""

    def __init__(self, changed, asking=lambda: None):
        super().__init__()
        self.changed = changed
        self.asking = asking

    def onSettingsChanged(self) -> None:
        self.changed()

    def onNotification(self, sender, method, data) -> None:
        if sender == kodi.ADDON_ID and method.endswith(requests.ASKING):
            self.asking()
        elif sender == "xbmc" and method == "System.OnWake":
            # Maybe on another network: the server proves itself again.
            auth.network_may_have_changed()


def run() -> None:
    profile = kodi.profile_dir()
    user_agent = kodi.user_agent()
    quality = QualitySetting(user_agent)
    monitor = Monitor(lambda: worker.put(quality.read), asking=lambda: told.asked())
    tokens = auth.Tokens(auth.Store(os.path.join(profile, "tokens.json")), user_agent)
    config = proxy_config(profile)
    worker = Worker()
    refresher = Refresher(kodi.refresh_listing, busy=lambda: player.isPlaying())
    player = Player(tokens, user_agent, worker, refresher)
    proxy = Proxy(
        tokens,
        user_agent,
        config["secret"],
        config["port"],
        on_stopped=player.stopped_by_server,
        meter=player.meter,
        on_busy=player.busy,
        on_ended=player.live_ended,
    )
    port = proxy.start()
    if port != config["port"]:
        save_proxy_config(profile, {"port": port, "secret": config["secret"]})
    kodi.set_property(kodi.PROXY, proxy.base)
    kodi.log("proxy on 127.0.0.1:%d" % port)
    # Requests' news, on a worker of its own: playback's reports never
    # wait for it.
    news_worker = Worker()
    told = news.News(player.call, tokens.identity, news_worker.put, player.isPlayingVideo, lambda: proxy.base)

    def heard(event: dict) -> None:
        player.heard(event)
        told.heard(event)

    events = Events(tokens, user_agent, heard, opened=told.opened)
    events.start()
    # A quality chosen before Automatic, shown in the settings as it was.
    worker.put(quality.read)
    # Kodi's own address: another one, maybe on another network, has the
    # server prove itself again before its next token.
    address = xbmc.getIPAddress()
    while not monitor.waitForAbort(1):
        if xbmc.getIPAddress() != address:
            address = xbmc.getIPAddress()
            auth.network_may_have_changed()
        try:
            player.tick()
        except Exception as e:  # noqa: BLE001 - the loop outlives any one turn
            kodi.log("service turn failed: %r" % e, xbmc.LOGERROR)
        try:
            told.tick(events.open.is_set())
        except Exception as e:  # noqa: BLE001 - the loop outlives any one turn
            kodi.log("requests' news failed: %r" % e, xbmc.LOGERROR)
    player.close_overlay()
    kodi.clear_property(kodi.PROXY)
    kodi.clear_property(requests.SUMMARY)
    # Ends after what it is doing, unwaited for: the last report has the
    # seconds Kodi gives.
    news_worker.put(None)
    events.stop()
    refresher.cancel()
    proxy.stop()
    worker.stop()
