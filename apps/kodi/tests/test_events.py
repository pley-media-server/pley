import json
import socket
import threading
import time
import unittest

import support  # noqa: F401
from pley import auth, events, kodi
from pley.events import Events, Refresher, events_of
from pley.proxy import hard_close
from test_service import KodiPlayer


class StreamServer:
    """/api/events as pley answers it: a chunked stream of events that stays
    open, with a comment every `ping` seconds. `answers` has a (status,
    events, hold) for each connection, in turn; hold false ends the stream
    after its events."""

    def __init__(self, answers, ping=0.1):
        self.answers = list(answers)
        self.ping = ping
        self.requests = []
        self.conns = []
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(8)
        self.url = "http://127.0.0.1:%d" % self.listener.getsockname()[1]
        threading.Thread(target=self.serve, daemon=True).start()

    def serve(self):
        while True:
            try:
                conn, _ = self.listener.accept()
            except OSError:
                return
            self.conns.append(conn)
            threading.Thread(target=self.answer, args=(conn,), daemon=True).start()

    def answer(self, conn):
        head = b""
        try:
            while b"\r\n\r\n" not in head:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                head += chunk
            self.requests.append(head.decode("latin-1"))
            status, sent, hold = self.answers.pop(0) if self.answers else (200, [], True)
            if status != 200:
                conn.sendall(b"HTTP/1.1 %d No\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" % status)
                return
            conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
            for event in sent:
                self.chunk(conn, "data: %s\n\n" % json.dumps(event))
            if not hold:
                conn.sendall(b"0\r\n\r\n")
                return
            while True:
                time.sleep(self.ping)
                self.chunk(conn, ": ping\n\n")
        except OSError:
            pass
        finally:
            conn.close()

    @staticmethod
    def chunk(conn, text):
        body = text.encode("utf-8")
        conn.sendall(b"%x\r\n%s\r\n" % (len(body), body))

    def close(self):
        hard_close(self.listener)
        for conn in self.conns:
            hard_close(conn)


class Tokens:
    def __init__(self, server):
        self.at = server
        self.profile = None
        self.token = "at-1"
        self.doubts = 0
        # (server, token) signed in to while a refused token is renewed.
        self.chosen_meanwhile = None

    def doubt(self):
        self.doubts += 1

    def server(self):
        return self.at

    def identity(self):
        return (self.at, self.profile) if self.at else None

    def session(self):
        if not self.at:
            raise auth.SignedOut()
        return self.at, self.token

    def refreshed(self, stale, server):
        if self.chosen_meanwhile:
            (self.at, self.token), self.chosen_meanwhile = self.chosen_meanwhile, None
        if server != self.at:
            raise auth.SignedOut()
        self.token = "at-%d" % (int(stale.split("-")[1]) + 1)
        return self.token


def wait_for(check, seconds=5.0):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if check():
            return True
        time.sleep(0.01)
    return False


class EventsOfTest(unittest.TestCase):
    def test_each_events_data_lines_are_read(self):
        lines = [
            ": keep-alive",
            "",
            'data: {"type": "user_state_changed", "item_ids": [7, 6, 5], "children": false}',
            "",
            'data:{"type":',
            'data: "library_changed"}',
            "",
            "data: not json",
            "",
            "event: unnamed",
            'data: ["a list"]',
            "",
        ]
        self.assertEqual(
            list(events_of(lines)),
            [{"type": "user_state_changed", "item_ids": [7, 6, 5], "children": False}, {"type": "library_changed"}],
        )


class EventsTest(unittest.TestCase):
    def setUp(self):
        self.heard = []
        self.servers = []
        self.stream = None

    def tearDown(self):
        if self.stream is not None:
            self.stream.stop()
        for server in self.servers:
            server.close()

    def server(self, answers):
        server = StreamServer(answers)
        self.servers.append(server)
        return server

    def listen(self, tokens):
        self.stream = Events(tokens, "pleyKodi/test", self.heard.append, retry=(0.05,))
        self.stream.start()

    def test_events_are_heard_with_a_token_renewed_when_refused(self):
        changed = {"type": "user_state_changed", "item_ids": [7], "children": False}
        server = self.server([(401, [], False), (200, [changed, {"type": "sessions_changed"}], True)])
        self.listen(Tokens(server.url))
        self.assertTrue(wait_for(lambda: len(self.heard) == 2), self.heard)
        self.assertEqual(self.heard[0], changed)
        first, second = server.requests
        self.assertTrue(first.startswith("GET /api/events HTTP/1.1"))
        self.assertIn("Authorization: Bearer at-1", first)
        self.assertIn("Authorization: Bearer at-2", second)
        self.assertIn("Accept: text/event-stream", second)
        self.assertIn("User-Agent: pleyKodi/test", second)

    def test_a_stream_that_ends_is_opened_again(self):
        # The server ends a stream whose token expired.
        server = self.server([(200, [{"type": "a"}], False), (200, [{"type": "b"}], True)])
        self.listen(Tokens(server.url))
        self.assertTrue(wait_for(lambda: len(self.heard) == 2), self.heard)
        self.assertEqual([e["type"] for e in self.heard], ["a", "b"])

    def test_each_opening_is_said(self):
        # Nothing it missed is replayed: the service reads requests' summary again.
        server = self.server([(200, [{"type": "a"}], False), (200, [], True)])
        opened = []
        self.stream = Events(Tokens(server.url), "pleyKodi/test", self.heard.append, retry=(0.05,),
                             opened=lambda: opened.append(len(self.heard)))
        self.stream.start()
        self.assertTrue(wait_for(lambda: len(opened) == 2), opened)
        self.assertEqual(opened, [0, 1], "each before its stream's events")

    def test_the_stream_makes_this_kodi_a_receiver(self):
        # Asked afresh each time it opens, with the token it opens with.
        server = self.server([(200, [], False), (200, [], True)])
        asked = []

        def query(at, token):
            asked.append((at, token))
            return {"receiver": "kodi-1", "device_kind": "tv", "casts": "item,live"}

        self.stream = Events(Tokens(server.url), "pleyKodi/test", self.heard.append, retry=(0.05,), query=query)
        self.stream.start()
        self.assertTrue(wait_for(lambda: len(server.requests) == 2), server.requests)
        self.assertTrue(server.requests[0].startswith("GET /api/events?receiver=kodi-1&device_kind=tv&casts=item,live HTTP/1.1"))
        self.assertEqual(asked, [(server.url, "at-1")] * 2)

    def test_another_server_chosen_is_listened_to_instead(self):
        first = self.server([(200, [], True)])
        second = self.server([(200, [{"type": "from_the_second"}], True)])
        tokens = Tokens(first.url)
        self.listen(tokens)
        self.assertTrue(wait_for(lambda: first.requests))
        tokens.at = second.url
        self.assertTrue(wait_for(lambda: self.heard), "the second server's events")
        self.assertEqual(len(first.requests), 1)

    def test_a_server_left_meanwhile_never_gets_the_new_ones_token(self):
        # Another server was chosen while the first held its answer, then
        # refused the token: the first isn't asked again, with the second's.
        first = self.server([(401, [], False)])
        second = self.server([(200, [{"type": "from_the_second"}], True)])
        tokens = Tokens(first.url)
        tokens.chosen_meanwhile = (second.url, "at-9")
        self.listen(tokens)
        self.assertTrue(wait_for(lambda: self.heard), "the second server's events")
        self.assertEqual(len(first.requests), 1)
        self.assertIn("Authorization: Bearer at-9", second.requests[0])

    def test_another_profile_is_listened_to_as_itself(self):
        # Switched profiles: the stream opened as the profile before goes.
        server = self.server([(200, [], True), (200, [{"type": "as_the_new_profile"}], True)])
        tokens = Tokens(server.url)
        self.listen(tokens)
        self.assertTrue(wait_for(lambda: server.requests))
        tokens.profile, tokens.token = 3, "at-9"
        self.assertTrue(wait_for(lambda: self.heard), "the new profile's events")
        self.assertIn("Authorization: Bearer at-9", server.requests[1])

    def test_stopping_ends_a_stream_waiting_for_events_at_once(self):
        server = self.server([(200, [], True)])
        server.ping = 60
        self.listen(Tokens(server.url))
        self.assertTrue(wait_for(lambda: server.requests))
        time.sleep(0.1)
        started = time.monotonic()
        self.stream.stop()
        self.assertLess(time.monotonic() - started, 1.0)
        self.assertFalse(self.stream.thread.is_alive())
        self.stream = None

    def test_nobody_signed_in_asks_nothing(self):
        self.listen(Tokens(None))
        time.sleep(0.2)
        self.assertTrue(self.stream.thread.is_alive())
        started = time.monotonic()
        self.stream.stop()
        self.assertLess(time.monotonic() - started, 1.0)
        self.stream = None


class RefresherTest(unittest.TestCase):
    def setUp(self):
        self.refreshed = []
        self.busy = False
        self.refresher = Refresher(lambda: self.refreshed.append(time.monotonic()), busy=lambda: self.busy,
                                   gather=0.05, min_gap=0.4)

    def tearDown(self):
        self.refresher.cancel()

    def test_a_burst_of_events_refreshes_once(self):
        for _ in range(3):
            self.refresher.later()
        self.assertTrue(wait_for(lambda: self.refreshed, 1))
        time.sleep(0.2)
        self.assertEqual(len(self.refreshed), 1)

    def test_refreshes_for_events_are_spaced_out(self):
        self.refresher.later()
        self.assertTrue(wait_for(lambda: self.refreshed, 1))
        self.refresher.later()
        self.assertTrue(wait_for(lambda: len(self.refreshed) == 2, 2))
        self.assertGreaterEqual(self.refreshed[1] - self.refreshed[0], 0.35)

    def test_a_refresh_after_the_event_showed_it_already(self):
        # Playback's end refreshed the listing after the server had the
        # report the event came of.
        self.refresher.later()
        self.refresher.now()
        time.sleep(0.2)
        self.assertEqual(len(self.refreshed), 1)

    def test_nothing_is_refreshed_behind_a_playing_video(self):
        self.busy = True
        self.refresher.later()
        time.sleep(0.2)
        self.assertEqual(self.refreshed, [])
        self.assertEqual(events.GATHER, 0.3)


class HeardTest(unittest.TestCase):
    def setUp(self):
        self.later = []
        self.player = KodiPlayer()
        self.player.refresher = type("Later", (), {"later": lambda _: self.later.append(1), "now": lambda _: None})()
        self.player.stopped = []
        self.player.stop = lambda: self.player.stopped.append(True)
        self.notified = []
        self.notify, kodi.notify = kodi.notify, lambda message, error=False: self.notified.append(message)
        playing = {"item_id": 7, "session_id": "s", "duration_ms": 100_000, "path": "http://proxy/7"}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]
        self.player.onAVStarted()

    def tearDown(self):
        kodi.notify = self.notify
        kodi.clear_property(kodi.PLAYING)

    def test_changes_ask_for_a_refresh(self):
        for kind in ("user_state_changed", "watchlist_changed", "library_changed", "libraries_changed"):
            self.player.heard({"type": kind})
        self.player.heard({"type": "sessions_changed"})
        self.player.heard({"type": "scan_progress"})
        self.assertEqual(len(self.later), 4)

    def test_a_stream_stopped_elsewhere_stops_at_once(self):
        self.player.heard({"type": "session_stopped", "session_id": "another", "message": None, "by_admin": True})
        self.assertEqual(self.player.stopped, [])
        self.player.heard({"type": "session_stopped", "session_id": "s", "message": None, "by_admin": False})
        self.assertEqual(self.player.stopped, [True])
        self.assertEqual(self.notified, [kodi.text("stopped_elsewhere")])

    def test_an_administrators_words_are_shown(self):
        self.player.heard({"type": "session_stopped", "session_id": "s", "message": "Maintenance", "by_admin": True})
        self.assertEqual(self.notified, ["Maintenance"])

    def test_each_playback_stopped_is_said_once(self):
        stop = {"type": "session_stopped", "session_id": "s", "message": None, "by_admin": True}
        self.player.heard(stop)
        # Its other requests refused after the event (410).
        self.player.stopped_by_server("The server stopped this stream.")
        self.player.file = ""
        self.player.onPlayBackStopped()
        self.player.stopped_by_server("The server stopped this stream.")
        self.assertEqual(len(self.notified), 1)
        # The next playback, stopped moments later, stops too.
        playing = {"item_id": 8, "session_id": "t", "duration_ms": 100_000, "path": "http://proxy/8"}
        kodi.set_property(kodi.PLAYING, json.dumps(playing))
        self.player.file = playing["path"]
        self.player.onAVStarted()
        self.player.heard(dict(stop, session_id="t"))
        self.assertEqual(len(self.notified), 2)
        self.assertEqual(self.player.stopped, [True, True])


if __name__ == "__main__":
    unittest.main()
