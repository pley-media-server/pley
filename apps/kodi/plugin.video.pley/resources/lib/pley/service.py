"""The service: it holds the sign-in, runs the proxy, tells the server
where playback is (docs/API.md, Playback): every 10 seconds while playing,
every 30 while paused (the server ends a session it hasn't heard of for 90
seconds), and when it stops, after which the session ends; and it obeys the
user's remote and plays what the remote casts here (remote.py, casting.py)."""

from __future__ import annotations

import json
import os
import queue
import re
import threading
import time
import urllib.parse

import xbmc

from . import api, auth, autoquality, kodi, listing, news, overlay, profiles, prompts, remote, requests, tracks
from .events import CHANGES, Events, Refresher
from .proxy import Proxy, new_secret

REPORT_EVERY = 10
PAUSED_EVERY = 30
# Requests while playing may take this long; when Kodi quits, the last
# report and the session's end get this together (Kodi kills a service
# that takes over 5 seconds to stop).
TIMEOUT = 5
LAST_TIMEOUT = 2
# A cast's switch to its remote's profile waits this long for what played to
# end as the profile it played as (its last report, its session's end): what
# hasn't gone by then is dropped, never sent as the profile switched to.
LEAVE_WAIT = 5
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
# A take-over's events (one for each viewer of the channel) come together,
# within milliseconds: within this long, they are the one. Watch again
# starts afresh, so the channel taken over again says so again.
TAKEN_ONCE = 3
FULLSCREEN = "Window.IsActive(fullscreenvideo) + !Window.IsVisible(videoosd) + !Window.IsVisible(seekbar)"


class Reports:
    """Progress reports for one playback, by the rules above. `send(path,
    body)` and `end(path)` reach the server. `tracks()` says the audio and
    subtitles a video plays (tracks.Follower.playing), which each report
    carries for the user's remotes, with the Quality menu's pick."""

    def __init__(self, playing: dict, send, end, clock=time.monotonic):
        self.playing = playing
        self.send = send
        self.end = end
        self.clock = clock
        self.position_ms = 0
        self.last = None
        self.paused = False
        self.credits_from = None
        self.tracks = None

    def body(self, state: str) -> dict:
        body = {
            "position_ms": int(self.position_ms),
            "duration_ms": int(self.playing.get("duration_ms") or 0),
            "state": state,
            "session_id": self.playing["session_id"],
        }
        # The server's stream indexes, -1 for no subtitles. One not known
        # yet is left out: the server keeps what the play asked for.
        audio, subtitle = (self.tracks() if self.tracks is not None else None) or (None, None)
        if audio is not None:
            body["audio_index"] = audio
        if subtitle is not None:
            body["subtitle_index"] = subtitle
        # The Quality menu's pick (Kodi's setting, as the file offers it).
        if self.playing.get("quality"):
            body["quality"] = self.playing["quality"]
        return body

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
        # The live channel's watch session Kodi plays ({session_id,
        # channel_id}), left when Kodi stops reading it (leave_live).
        self.live = None
        # Automatic (docs/QUALITY.md): the playback's watch and what it
        # was given (playback.automatic), what the proxy counts, and the
        # title being played again at another quality: (item id, watch,
        # what to say once it plays).
        self.watch = None
        self.auto = None
        self.clock = time.monotonic
        self.meter = autoquality.Meter()
        self.restarting = None
        # Casts heard: only the last one heard plays (play_cast).
        self.casts = 0
        # A cast's switch to its remote's profile is under way: casts heard
        # meanwhile are dropped (cast). Until when what the profile being
        # left still sends may go (leave_playing).
        self.switching = False
        self.leaving_until = None

    # ------------------------------------------------------------ requests

    def call(self, method: str, path: str, body=None, timeout: float = TIMEOUT, fine=(410,)):
        """One request to the server (request): its answer, or None when
        signed out or out of reach. While a cast's switch waits for the
        profile being left, it goes within that wait or not at all."""
        timeout = self.in_time(timeout)
        if timeout is None:
            kodi.log("%s %s dropped: the cast's switch waits no longer" % (method, path), xbmc.LOGWARNING)
            return None
        return self.request(method, path, body, timeout, fine)

    def in_time(self, timeout: float):
        """`timeout`, cut to what is left of a cast's switch's wait; None
        once that wait is over."""
        until = self.leaving_until
        if until is None:
            return timeout
        left = until - self.clock()
        return min(timeout, left) if left > 0 else None

    def request(self, method: str, path: str, body, timeout: float, fine):
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
                timeout = self.in_time(timeout)
                if timeout is None:
                    return None
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
        self.server_ended_live(channel)

        def taken():
            channel_taken, at = self.taken
            return channel is not None and channel_taken == channel and at > now - TAKEN_ONCE

        def say():
            if not taken():
                kodi.notify(kodi.text("stream_stopped"), error=True)

        # The take-over's event usually comes first and says why: decided
        # now, as Watch again picked within the wait forgets it.
        if taken():
            return
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
        self.server_ended_live(channel_id)
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
                self.taken = (None, float("-inf"))
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
                return self.live_stopped(event)
            by = "stopped_by_admin" if event.get("by_admin") else "stopped_elsewhere"
            self.stopped_by_server(event.get("message") or kodi.text(by))
        elif kind == "live_taken_over":
            self.taken_over(event.get("channel_id"), event.get("message"))
        elif kind == "player_command":
            try:
                self.obey(event)
            except Exception as e:  # noqa: BLE001 - one command never ends the stream
                # Kodi stopped between the check and the call, say (seekTime
                # raises then): out of here, it would drop the event stream,
                # which misses what happens while it connects again.
                kodi.log("the remote's %s failed: %r" % (event.get("command"), e), xbmc.LOGWARNING)
        elif kind == "cast":
            try:
                self.cast(event)
            except Exception as e:  # noqa: BLE001 - one cast never ends the stream
                kodi.log("the remote's cast failed: %r" % e, xbmc.LOGWARNING)

    def live_session(self):
        """The live channel's watch session (kodi.LIVE): {session_id,
        channel_id}, or None."""
        try:
            live = json.loads(kodi.get_property(kodi.LIVE) or "null")
        except ValueError:
            return None
        return live if isinstance(live, dict) and live.get("session_id") else None

    def live_stopped(self, event: dict) -> None:
        """The live channel's session stopped from another device or by an
        administrator: it stops as one taken over does, saying who stopped
        it, then their words on a line of their own."""
        live = self.live_session()
        if live is None or event.get("session_id") != live["session_id"]:
            return
        why = kodi.text("live_stopped_by_admin" if event.get("by_admin") else "live_stopped_elsewhere")
        message = event.get("message")
        self.taken_over(live.get("channel_id"), "%s\n%s" % (why, message) if message else why)

    def follow_live(self) -> None:
        """What Kodi plays now, for the live session it holds: another
        channel, or anything else, leaves the one before; the channel the
        plugin last watched (kodi.LIVE) is held once Kodi plays its stream."""
        from . import livetv

        channel = livetv.channel_of(self.playing_file())
        live = self.live_session()
        now = live if live is not None and channel is not None and live.get("channel_id") == channel else None
        with self.lock:
            before = self.live
            if before is not None and now is not None and before["session_id"] == now["session_id"]:
                return
            self.live = now
        if before is not None:
            self.leave_live(before)

    def stopped_live(self) -> None:
        """Kodi stopped, or its stream ended without the server ending it:
        the live session it held is left, unless Kodi still plays it (Kodi
        tells of a stop late)."""
        from . import livetv

        channel = livetv.channel_of(self.playing_file())
        with self.lock:
            before = self.live
            if before is None or before.get("channel_id") == channel:
                return
            self.live = None
        self.leave_live(before)

    def closing(self) -> None:
        """Kodi is exiting: the live session it holds is left, whether or
        not its stream still plays."""
        with self.lock:
            before, self.live = self.live, None
        if before is not None:
            self.leave_live(before)

    def server_ended_live(self, channel_id) -> None:
        """The server ended the channel's session (taken over, stopped from
        elsewhere, its stream ended): there is nothing to leave."""
        with self.lock:
            if self.live is not None and self.live.get("channel_id") == channel_id:
                self.live = None

    def leave_live(self, live: dict) -> None:
        """DELETE /api/live/sessions/{session}, on the worker: Kodi's stream
        keeps its session up to 45 s after Kodi stops reading it, which
        would hold a connection of the source (a one-connection account's
        only one) and stay in the user's remote. Best effort: a session the
        server ended meanwhile answers 404, and nothing else is tried."""
        path = "/api/live/sessions/%s" % urllib.parse.quote(live["session_id"], safe="")
        self.worker.put(lambda: self.call("DELETE", path, timeout=LAST_TIMEOUT, fine=(404, 410)))

    # ------------------------------------------------------------ the remote

    def obey(self, command: dict) -> None:
        """A command of the user's remote (remote.py), from the event
        stream's thread: for the title or the live channel playing here.
        Kodi stopped, or playing something else, obeys nothing."""
        with self.lock:
            reports = self.reports if self.restarting is None and self.tracked_playing() else None
        if reports is not None:
            return self.obey_title(reports, command)
        live = self.live_session()
        if live is not None:
            self.obey_live(live, command)

    def obey_title(self, reports: Reports, command: dict) -> None:
        if command.get("command") == "quality":
            return self.obey_quality(reports, command)
        playing = reports.playing
        paused = xbmc.getCondVisibility("Player.Paused")
        place = remote.queue_place(playing["item_id"])
        queued = place is not None and place[1] + 1 < place[2]
        # Nothing queued after an episode: the one after it, as Up Next
        # plays it (find_next), once the server has said which.
        with self.lock:
            moments = self.moments if self.reports is reports else None
        following = moments.next_item if moments is not None and playing.get("kind") == "episode" else None
        player = {
            "session_id": playing.get("session_id"),
            "state": "paused" if paused else "playing",
            "position_ms": self.position(),
            "duration_ms": playing.get("duration_ms") or None,
            "has_next": queued or following is not None,
        }
        music = (place is not None and place[0] == "music") or playing.get("kind") in listing.MUSIC
        if music:
            player["index"] = place[1] if place is not None else 0
        if command.get("command") in remote.TRACKS:
            # Switched in Kodi's player (tracks.Follower.obey); music, and a
            # title without tracks, have none to switch.
            with self.lock:
                follower = self.tracks if self.reports is reports else None
            action = follower.obey(playing.get("session_id"), command) if follower is not None else dict(remote.NONE)
        else:
            action = remote.obey(player, command)
        act = action["act"]
        if act in ("play", "pause"):
            # Kodi's pause turns it either way; said by onPlayBackPaused or
            # onPlayBackResumed, at once.
            self.pause()
        elif act == "seek":
            # Up Next goes first, as the TV apps' does, its countdown starting
            # again should the place still be in the credits: left counting, it
            # would play the next episode over the place the remote asked for.
            with self.ui:
                if moments is not None:
                    moments.next_since = None
                self.close_overlay()
            self.seekTime(action["position_ms"] / 1000.0)
        elif act == "next" and queued:
            self.playnext()
        elif act == "next":
            with self.ui:
                self.play_next(following)
        elif act == "previous":
            self.playprevious()
        # Music shows what plays where it plays, and is left as it was.
        if not music:
            self.remote_notice(remote.notice(command, action, kodi.text))
        if action["report"] and act not in ("play", "pause"):
            with self.lock:
                if self.reports is not reports:
                    return
                reports.position_ms = action["position_ms"] if act == "seek" else player["position_ms"]
                if paused != reports.paused:
                    # Kodi's word on a pause not heard yet: said with it.
                    (reports.pause if paused else reports.resume)()
                else:
                    reports.report("paused" if paused else "playing")

    def remote_notice(self, words: str) -> None:
        """What the user's remote did to a video, for a moment as Kodi's
        notification, without its sound. The add-on brings up nothing of
        Kodi's player for a command; what Kodi shows itself for a pause or
        a seek (the skin's seek bar, as for its own keys) is the skin's, and
        stays."""
        if words:
            kodi.notify(words, icon=kodi.addon_icon(), ms=remote.NOTICE_MS, sound=False)

    def obey_quality(self, reports: Reports, command: dict) -> None:
        """The remote's quality (remote.obey_quality), as picking it in
        Kodi's Quality setting, the add-on's Quality menu: the setting
        changes (Automatic picked again starts afresh, as QualitySetting
        sees), and the title plays again where it is at it, a new session
        that reports as it starts. The one picked already is said again."""
        from . import playback

        playing = reports.playing
        if "qualities" not in playing:
            # Music, which has no quality.
            return
        player = {"session_id": playing.get("session_id"), "qualities": playing["qualities"], "quality": playing.get("quality")}
        action = remote.obey_quality(player, command)
        if action["act"] == "none":
            if action["report"]:
                with self.lock:
                    if self.reports is reports:
                        reports.position_ms = self.position()
                        reports.report("paused" if reports.paused else "playing")
            return
        kodi.set_setting(playback.QUALITY, action["quality"])
        kodi.log("the remote picked the quality %s" % action["quality"])
        self.remote_notice(remote.notice(command, action, kodi.text))
        item_id, position = playing["item_id"], self.position()
        with self.lock:
            # Not Automatic's restart: the title gets a watch of its own.
            self.restarting = (item_id, None, None)
        self.worker.put(lambda: self.restart(item_id, position, None))

    def obey_live(self, live: dict, command: dict) -> None:
        """Channel up and down in Kodi's queue of channels, for the channel
        playing now: Kodi asks the plugin for it, which watches it as any
        channel is watched (a new session, which the remote then lists)."""
        from . import livetv

        channel = livetv.channel_of(self.playing_file())
        if channel is None or channel != live.get("channel_id"):
            return
        channels = livetv.queued_channels()
        action = remote.obey_live({"session_id": live["session_id"], "channel": channel, "channels": channels}, command)
        if action["act"] == "channel":
            # Kodi's video queue, which window.play_channels started: this
            # player's playselected would first switch to the playlist it last
            # played itself (music, unless it played one).
            self.play(xbmc.PlayList(xbmc.PLAYLIST_VIDEO), startpos=channels.index(action["channel"]))

    # --------------------------------------------------------------- casts

    def taking(self) -> bool:
        """Whether Kodi takes casts now: signed in, and "Who's watching?"
        not on screen, whose PINs a cast mustn't play past."""
        return bool(self.tokens.server()) and not kodi.get_property(profiles.PICKING)

    def cast(self, event: dict) -> None:
        """A cast of the user's remote (casting.py), from the event stream's
        thread: one for this Kodi plays on the worker, which asks the
        server what it needs first; of two close together, the later."""
        from . import casting, playback

        action = casting.action(playback.install_id(), self.taking(), event)
        if action["act"] == "none":
            return
        switch = casting.switch_to(event, self.profile_now())
        if switch == casting.IGNORED:
            return
        with self.lock:
            if self.switching:
                # Another cast switches profile: this one would play as
                # whichever profile it caught, so none plays it.
                return
            self.casts += 1
            mine = self.casts
        self.worker.put(lambda: self.play_cast(action, event.get("from"), mine, switch))

    def profile_now(self):
        """The profile the add-on acts as: its id, or None for the one
        picked as it signed in."""
        identity = self.tokens.identity()
        return identity[1] if identity else None

    def current_cast(self, mine: int) -> bool:
        """Whether cast `mine` may still play: no other came since, and Kodi
        takes casts."""
        return self.casts == mine and self.taking()

    def play_cast(self, action: dict, device, mine: int, switch=None) -> None:
        """Plays a cast as the add-on's own Play would, in place of what
        plays, through the service's proxy; then says where it came from,
        over what starts, as the add-on's other notices are said. `switch`
        (casting.switch_to): the remote's profile to switch to first."""
        if switch is not None and self.profile_now() != switch["profile_id"]:
            with self.lock:
                if not self.current_cast(mine):
                    return
                self.switching = True
                self.leaving_until = self.clock() + LEAVE_WAIT
            # What plays is the profile's being left: its last report, and
            # its live channel's end, go as that profile, on the worker
            # before the switch, within LEAVE_WAIT.
            try:
                left = self.leave_playing()
                self.worker.put(lambda: self.switch_for_cast(action, device, mine, switch, left))
            except BaseException:
                self.switch_done()
                raise
            return
        self.play_now(action, device, mine)

    def leave_playing(self) -> str:
        """Ends the playback reported, and the live channel's session held,
        as a switch to another profile nears: the file Kodi plays, which it
        goes on playing until the cast takes its place; "" for none."""
        with self.lock:
            left = self.reports.playing["path"] if self.reports is not None else ""
            if self.reports is not None:
                self._finish_locked(ended=False)
            live, self.live = self.live, None
        if live is not None:
            self.leave_live(live)
            left = left or self.playing_file()
        return left

    def switch_for_cast(self, action: dict, device, mine: int, switch: dict, left: str) -> None:
        """Switches to the remote's profile with the cast's ticket, then
        plays the cast; a switch that fails says so, plays nothing, and
        stops what played, which was left for the cast. Casts heard from
        then on play by the usual rules, as the profile switched to."""
        # What the profile left hasn't sent by now goes no more.
        self.leaving_until = None
        try:
            switched = self.current_cast(mine) and self.switch_profile_for_cast(switch, left)
        finally:
            self.switch_done()
        if switched:
            self.play_now(action, device, mine)

    def switch_profile_for_cast(self, switch: dict, left: str) -> bool:
        """The switch itself: whether the add-on acts as the remote's
        profile now."""
        from . import casting

        if self.profile_now() == switch["profile_id"]:
            return True
        try:
            self.tokens.switch_profile(switch["profile_id"], ticket=switch["ticket"])
        except auth.SignedOut:
            return False
        except api.ApiError as e:
            kodi.log("cast's switch to profile %d: %s %s" % (switch["profile_id"], e.code, e.message), xbmc.LOGWARNING)
            if e.status == 0:
                # Nothing answered: the server proves itself again before the next token.
                self.tokens.doubt()
            if left and self.playing_file() == left:
                self.stop()
            kodi.notify(kodi.text("cast_switch_failed"), error=True, ms=casting.BANNER_MS)
            return False
        self.switched_for_cast()
        return True

    def switch_done(self) -> None:
        """The cast's switch is over, done or not: casts play again."""
        with self.lock:
            self.switching = False
            self.leaving_until = None

    def switched_for_cast(self) -> None:
        """The add-on acts as the remote's profile now: the window, the
        listing on screen and requests' summary are that profile's, as after
        the plugin's switch."""
        profiles.tell_switched()
        # Requests' summary was the profile's before (news.py reads the new one).
        kodi.clear_property(requests.SUMMARY)
        if xbmc.getInfoLabel("Container.FolderPath").startswith("plugin://%s/" % kodi.ADDON_ID):
            xbmc.executebuiltin("Container.Refresh")

    def play_now(self, action: dict, device, mine: int) -> None:
        """Plays the cast as the profile the add-on acts as now."""
        from . import casting

        base = kodi.get_property(kodi.PROXY)
        if not base:
            return

        def current():
            return self.current_cast(mine)

        client = api.Client(base, self.user_agent, local=True)
        try:
            outcome = casting.play(client, base, action, self, current) if current() else casting.DROPPED
        except api.ApiError as e:
            kodi.log("cast %r: %s %s" % (action, e.code, e.message), xbmc.LOGWARNING)
            if current():
                kodi.notify(kodi.text("server_error") % e.message, error=True)
            return
        if outcome == casting.PLAYED:
            kodi.notify(casting.banner(device), icon=kodi.addon_icon(), ms=casting.BANNER_MS)
        elif outcome == casting.NOTHING:
            kodi.notify(kodi.text("nothing_to_play"), icon=kodi.addon_icon(), ms=casting.BANNER_MS)

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
        self.follow_live()
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
            reports.tracks = follower.playing
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
        if restarting is not None and restarting[1] is not None and restarting[0] == playing.get("item_id"):
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

    def restart(self, item_id: int, position_ms: float, quality) -> None:
        """Plays the title again where it is, at `quality` (Automatic's;
        None for the setting's, as the remote picked it), from the worker:
        a new session, which ends the one before. A cast's tracks stay."""
        from . import playback

        if not self.still_playing(item_id):
            self.restarting = None
            return
        base = kodi.get_property(kodi.PROXY)
        with self.lock:
            # What the remote may do stays as it was (playback.obeys).
            playing = self.reports.playing if self.reports is not None else {}
            kind, given = playing.get("kind"), playing.get("given")
        try:
            # The settings as they are now (the service's are read once).
            kodi.begin()
            client = api.Client(base, self.user_agent, timeout=60, local=True)
            li = playback.prepare(client, base, item_id, int(position_ms), restart=quality, kind=kind, given=given)
        except (api.ApiError, OSError, ValueError, KeyError) as e:
            kodi.log("couldn't play %d again at %s: %r" % (item_id, quality or "the quality setting", e), xbmc.LOGWARNING)
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
        self.stopped_live()

    def onPlayBackEnded(self) -> None:
        self.finish(ended=True)
        self.stopped_live()

    def onPlayBackError(self) -> None:
        self.stopped_live()
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
        self.play("plugin://%s/?route=play&id=%d&kind=%s" % (kodi.ADDON_ID, item["id"], item.get("kind") or "episode"))


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


def receiver_query(user_agent: str):
    """The event stream's query (events.Events' `query`) that makes this
    Kodi a receiver of the user's casts (casting.receiver_query), asked as
    the stream opens: channels too where the account has Live TV then."""

    def query(server: str, token: str) -> dict:
        from . import casting, livetv, playback

        try:
            live = livetv.available(livetv.summary(api.Client(server, user_agent, token, timeout=TIMEOUT)))
        except (api.ApiError, ValueError):
            # Out of reach: the stream won't open either, and asks again.
            live = False
        return casting.receiver_query(playback.install_id(), live)

    return query


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

    events = Events(tokens, user_agent, heard, opened=told.opened, query=receiver_query(user_agent))
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
    player.closing()
    kodi.clear_property(kodi.PROXY)
    kodi.clear_property(requests.SUMMARY)
    # Ends after what it is doing, unwaited for: the last report has the
    # seconds Kodi gives.
    news_worker.put(None)
    events.stop()
    refresher.cancel()
    proxy.stop()
    worker.stop()
