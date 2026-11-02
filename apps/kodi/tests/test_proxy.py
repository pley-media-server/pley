import http.client
import json
import os
import shutil
import socket
import tempfile
import threading
import time
import unittest

import support
from pley import api, auth, profiles
from pley.api import path_in
from pley.proxy import Proxy

SECRET = "s3cret"
MOVIE = bytes(range(256)) * 12_000  # 3 MB


class ProxyTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.valid = {"at-1"}
        self.stopped = []
        self.network = {"local": False, "ceiling_kbps": 8000, "source": "playback", "since": 1, "probe": False}
        self.network_down = False
        self.server = support.FakeServer(self.handle)
        self.store = auth.Store(os.path.join(self.dir, "tokens.json"))
        self.store.save(
            {
                "server": self.server.url,
                "server_name": "Test",
                "server_id": "abc",
                "token_endpoint": self.server.url + "/api/oauth/token",
                "revocation_endpoint": self.server.url + "/api/oauth/revoke",
                "access_token": "at-1",
                "expires_at": 4_000_000_000,
                "refresh_token": "rt-1",
                "scope": "library play",
            }
        )
        self.tokens = auth.Tokens(self.store, "pleyKodi/test")
        self.proxy = Proxy(self.tokens, "pleyKodi/test", SECRET, on_stopped=self.stopped.append)
        self.port = self.proxy.start()

    def tearDown(self):
        self.proxy.stop()
        self.server.close()
        shutil.rmtree(self.dir)

    def handle(self, request):
        if request.path == "/api/oauth/token":
            if request.form().get("profile") == "5" and request.form().get("pin") != "2468":
                return support.answer(400, {"error": "invalid_grant", "reason": "wrong_pin", "error_description": "wrong PIN"})
            self.valid = {"at-2"}
            return support.answer(200, {"access_token": "at-2", "refresh_token": "rt-2", "expires_in": 3600})
        if request.path == "/api/oauth/revoke":
            return 200, {}, b""
        token = request.headers.get("authorization", "")[len("Bearer ") :]
        if token not in self.valid:
            return support.answer(401, {"code": "unauthorized", "message": "expired"})
        if request.path == "/api/files/7/stream":
            start, end = 0, len(MOVIE) - 1
            if "range" in request.headers:
                first, last = request.headers["range"][len("bytes=") :].split("-")
                start, end = int(first), int(last or end)
                part = MOVIE[start : end + 1]
                headers = {
                    "Content-Type": "video/x-matroska",
                    "Content-Range": "bytes %d-%d/%d" % (start, end, len(MOVIE)),
                    "Accept-Ranges": "bytes",
                    "Content-Length": str(len(part)),
                }
                return 206, headers, part
            return 200, {"Content-Type": "video/x-matroska", "Accept-Ranges": "bytes"}, MOVIE
        if request.path.startswith("/api/live/channels/3/stream.ts"):
            held = [{"channel_id": 2, "channel_name": "Sport", "watchers": [], "people": 1, "yours": True, "can_take_over": True}]
            return support.answer(409, {"code": "conflict", "message": "The one connection of Test Provider is in use, watching Sport.", "connections": held})
        if request.path == "/api/playback/s1/12.ts":
            return support.answer(410, {"code": "session_stopped", "message": "Maintenance tonight"})
        if request.path == "/api/collections/4/images/poster":
            # A collection without a poster sends to its first title's.
            return 307, {"Location": "/api/items/9/images/poster?v=3"}, b""
        if request.path == "/api/collections/5/images/poster":
            return 307, {"Location": self.server.url + "/api/items/10/images/poster"}, b""
        if request.path.startswith("/api/playback/speedtest"):
            return 200, {"Content-Type": "application/octet-stream", "Cache-Control": "no-store"}, MOVIE
        if request.path.startswith("/api/playback/network"):
            if self.network is None:
                return support.answer(404, {"code": "not_found", "message": "no route"})
            if request.method == "PUT" and self.network_down:
                return support.answer(500, {"code": "internal", "message": "busy"})
            if request.method == "PUT":
                self.network = dict(self.network, ceiling_kbps=request.json().get("ceiling_kbps"))
            return support.answer(200, self.network)
        if request.path.startswith("/api/progress"):
            return 200, {"Content-Type": "application/json"}, request.body
        return support.answer(200, {"path": request.path, "agent": request.headers.get("user-agent")})

    def ask(self, method, path, host=None, headers=None, body=None):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        sent = {"Host": host or "127.0.0.1:%d" % self.port}
        sent.update(headers or {})
        conn.request(method, path, body=body, headers=sent)
        response = conn.getresponse()
        data = response.read()
        conn.close()
        return response, data

    def test_requests_reach_the_server_with_the_token(self):
        response, data = self.ask("GET", "/%s/api/libraries?x=1" % SECRET)
        self.assertEqual(response.status, 200)
        self.assertEqual(json.loads(data), {"path": "/api/libraries?x=1", "agent": "pleyKodi/test"})
        self.assertEqual(self.server.requests[-1].headers["authorization"], "Bearer at-1")

    def test_a_refused_token_is_renewed_and_the_request_sent_again(self):
        self.valid = {"at-2"}
        response, data = self.ask("GET", "/%s/api/hubs/home" % SECRET)
        self.assertEqual(response.status, 200, data)
        self.assertEqual(self.store.load()["access_token"], "at-2")
        paths = [r.path for r in self.server.requests]
        self.assertEqual(paths, ["/api/hubs/home", "/api/oauth/token", "/api/hubs/home"])

    def test_bodies_go_through_as_they_are(self):
        body = json.dumps({"position_ms": 5000, "state": "playing"}).encode()
        response, data = self.ask(
            "POST", "/%s/api/progress" % SECRET, headers={"Content-Type": "application/json"}, body=body
        )
        self.assertEqual((response.status, data), (200, body))
        self.assertEqual(self.server.requests[-1].headers["content-type"], "application/json")

    def test_files_stream_whole_or_by_range(self):
        response, data = self.ask("GET", "/%s/api/files/7/stream" % SECRET)
        self.assertEqual((response.status, len(data)), (200, len(MOVIE)))
        self.assertEqual(data, MOVIE)
        self.assertEqual(response.getheader("Accept-Ranges"), "bytes")
        response, data = self.ask("GET", "/%s/api/files/7/stream" % SECRET, headers={"Range": "bytes=1000-1999"})
        self.assertEqual(response.status, 206)
        self.assertEqual(response.getheader("Content-Range"), "bytes 1000-1999/%d" % len(MOVIE))
        self.assertEqual(data, MOVIE[1000:2000])
        response, data = self.ask("HEAD", "/%s/api/files/7/stream" % SECRET)
        self.assertEqual((response.status, data), (200, b""))
        self.assertEqual(response.getheader("Content-Type"), "video/x-matroska")

    def test_streams_are_counted_for_automatic(self):
        from pley.autoquality import Meter

        self.proxy.meter = Meter()
        response, data = self.ask("GET", "/%s/api/files/7/stream" % SECRET)
        self.assertEqual(len(data), len(MOVIE))
        self.ask("GET", "/%s/api/libraries" % SECRET)
        self.assertEqual([(path, size) for _, path, size, _ in self.proxy.meter.answers], [("/api/files/7/stream", len(MOVIE))], "the API's answers aren't")
        self.assertAlmostEqual(self.proxy.meter.arriving_kbps("/api/files/7/stream"), len(MOVIE) * 8 / 1000.0 / 20)

    def test_a_stream_stopped_on_the_server_is_told(self):
        response, data = self.ask("GET", "/%s/api/playback/s1/12.ts" % SECRET)
        self.assertEqual(response.status, 410)
        self.assertEqual(self.stopped, ["Maintenance tonight"])

    def test_only_kodi_on_this_computer_gets_in(self):
        for host, path in [
            ("pley.attacker.example:%d" % self.port, "/%s/api/libraries" % SECRET),
            (None, "/wrong/api/libraries"),
            (None, "/%s/.well-known/oauth-authorization-server" % SECRET),
        ]:
            response, _ = self.ask("GET", path, host=host)
            self.assertIn(response.status, (403, 404), (host, path))
        self.assertEqual(self.server.requests, [])

    def test_a_negative_or_unreadable_length_is_refused_at_once(self):
        # A negative one would have it read until the connection ends.
        for length in ["-1", "1e3", "x"]:
            for path in ["/%s/api/progress" % SECRET, "/%s/switch-profile" % SECRET]:
                sock = socket.create_connection(("127.0.0.1", self.port), timeout=5)
                lines = ["POST %s HTTP/1.1" % path, "Host: 127.0.0.1:%d" % self.port, "Content-Length: " + length]
                sock.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("ascii"))
                answer = sock.recv(4096)
                sock.close()
                status = answer.split(b"\r\n", 1)[0]
                self.assertTrue(status.startswith(b"HTTP/1.") and b" 400 " in status, (length, path, answer))
        self.assertEqual(self.server.requests, [])

    def test_status_and_signing_out(self):
        response, data = self.ask("GET", "/%s/status" % SECRET)
        self.assertEqual(json.loads(data), {"signed_in": True, "server": self.server.url})
        response, data = self.ask("POST", "/%s/sign-out" % SECRET)
        self.assertEqual(json.loads(data)["signed_in"], False)
        self.assertEqual(self.server.requests[-1].path, "/api/oauth/revoke")
        response, data = self.ask("GET", "/%s/api/libraries" % SECRET)
        self.assertEqual((response.status, json.loads(data)["code"]), (401, "signed_out"))

    def test_switching_profiles_through_the_service(self):
        # The service alone holds the sign-in: the window and the plugin
        # switch through it, and a refusal comes back as the server's reason.
        client = api.Client(self.proxy.base, "pleyKodi/test", timeout=10, local=True)
        with self.assertRaises(api.ApiError) as refused:
            profiles.switch(client, 5, "1111")
        self.assertEqual((refused.exception.status, refused.exception.code), (400, "wrong_pin"))
        self.assertEqual(self.store.load()["refresh_token"], "rt-1")
        profiles.switch(client, 5, "2468")
        sent = self.server.requests[-1].form()
        self.assertEqual((sent["grant_type"], sent["profile"], sent["pin"]), (auth.SWITCH_GRANT, "5", "2468"))
        self.assertEqual((self.store.load()["access_token"], self.store.load()["profile"]), ("at-2", 5))
        response, data = self.ask("POST", "/%s/switch-profile" % SECRET)
        self.assertEqual((response.status, json.loads(data)["code"]), (400, "bad_request"))

    def test_the_plugins_own_client_reads_the_proxys_answers(self):
        # The plugin talks to the proxy over a socket of its own (no
        # http.client to load): JSON, bodies, big answers and refusals.
        client = api.Client(self.proxy.base, "pleyKodi/test", timeout=10, local=True)
        self.assertEqual(client.get("/status"), {"signed_in": True, "server": self.server.url})
        self.assertEqual(client.get("/api/libraries", sort="title"), {"path": "/api/libraries?sort=title", "agent": "pleyKodi/test"})
        body = {"position_ms": 5000, "state": "playing"}
        self.assertEqual(client.post("/api/progress", body), body)
        self.assertEqual(self.server.requests[-1].headers["content-type"], "application/json")
        movie = client.request("GET", "/api/files/7/stream")
        self.assertEqual((movie.status, movie.body), (200, MOVIE))
        self.assertEqual(movie.headers["content-type"], "video/x-matroska")
        self.store.clear()
        with self.assertRaises(api.ApiError) as refused:
            client.get("/api/libraries")
        self.assertEqual((refused.exception.status, refused.exception.code), (401, "signed_out"))
        self.proxy.stop()
        with self.assertRaises(api.ApiError) as down:
            client.get("/status")
        self.assertEqual((down.exception.status, down.exception.code), (0, "service_off"))

    def test_what_the_server_knows_of_the_network_is_kept_a_while(self):
        client = api.Client(self.proxy.base, "pleyKodi/test", timeout=10, local=True)
        asked = lambda: [r.method for r in self.server.requests if r.path.startswith("/api/playback/network")]
        self.assertEqual(client.get("/api/playback/network", client_id="kodi-1")["ceiling_kbps"], 8000)
        self.assertEqual(client.get("/api/playback/network", client_id="kodi-1")["ceiling_kbps"], 8000)
        self.assertEqual(asked(), ["GET"], "the second from the proxy: a play doesn't wait")
        client.get("/api/playback/network", client_id="kodi-2")
        self.assertEqual(asked(), ["GET", "GET"], "another client's own")
        # A ceiling told replaces what is kept.
        self.assertEqual(client.put("/api/playback/network", {"client_id": "kodi-1", "ceiling_kbps": 4000})["ceiling_kbps"], 4000)
        self.assertEqual(client.get("/api/playback/network", client_id="kodi-1")["ceiling_kbps"], 4000)
        self.assertEqual(asked(), ["GET", "GET", "PUT"])
        # One the server didn't take: asked again.
        self.network_down = True
        with self.assertRaises(api.ApiError):
            client.put("/api/playback/network", {"client_id": "kodi-1", "ceiling_kbps": None})
        self.assertEqual(client.get("/api/playback/network", client_id="kodi-1")["ceiling_kbps"], 4000)
        self.assertEqual(asked()[-1], "GET")
        # Another profile's is its own.
        data = self.store.load()
        self.store.save(dict(data, profile=7))
        before = len(asked())
        client.get("/api/playback/network", client_id="kodi-1")
        self.assertEqual(len(asked()), before + 1)
        # Kept no longer than ten minutes.
        self.proxy.network.clock = lambda: time.monotonic() + 601
        client.get("/api/playback/network", client_id="kodi-1")
        self.assertEqual(len(asked()), before + 2)

    def test_a_server_from_before_is_asked_once_a_while(self):
        self.network = None
        client = api.Client(self.proxy.base, "pleyKodi/test", timeout=10, local=True)
        for _ in range(2):
            with self.assertRaises(api.ApiError) as refused:
                client.get("/api/playback/network", client_id="kodi-1")
            self.assertEqual(refused.exception.status, 404)
        self.assertEqual(len([r for r in self.server.requests if r.path.startswith("/api/playback/network")]), 1)

    def test_a_speed_test_is_read_as_it_comes_and_let_go(self):
        from pley.autoquality import SpeedTest

        client = api.Client(self.proxy.base, "pleyKodi/test", timeout=10, local=True)
        test = SpeedTest(total=len(MOVIE))
        status = client.measure("/api/playback/speedtest", {"bytes": len(MOVIE)}, test, time.monotonic() + 10)
        self.assertEqual((status, test.size), (200, len(MOVIE)))
        self.assertTrue(test.result()["probe_complete"])
        self.assertIn("bytes=%d" % len(MOVIE), self.server.requests[-1].path)
        self.assertEqual(self.server.requests[-1].headers["authorization"], "Bearer at-1")
        # Stopped once enough came: the rest isn't waited for.
        test = SpeedTest(total=len(MOVIE) * 100)
        ticks = iter(range(100_000))
        status = client.measure("/api/playback/speedtest", {}, test, 50.0, clock=lambda: next(ticks) * 0.02)
        self.assertEqual(status, 200)
        self.assertLess(test.size, len(MOVIE))
        self.assertFalse(test.result(test.last_at)["probe_complete"])
        # A refusal measures nothing.
        self.store.clear()
        test = SpeedTest()
        self.assertEqual(client.measure("/api/playback/speedtest", {}, test, time.monotonic() + 5), 401)
        self.assertEqual(test.size, 0)

    def test_a_server_out_of_reach_is_said_within_seconds(self):
        # 192.0.2.1 is for documentation: nothing ever answers there, as a
        # server that is switched off doesn't.
        self.store.save(dict(self.store.load(), server="http://192.0.2.1:7539"))
        started = time.monotonic()
        response, data = self.ask("GET", "/%s/api/hubs/home" % SECRET)
        self.assertEqual((response.status, json.loads(data)["code"]), (502, "unreachable"))
        self.assertLess(time.monotonic() - started, 7)

    def test_another_device_at_the_address_is_said_and_gets_no_token(self):
        # A sign-in with a key, at an address where the server can't prove it
        # holds it (this stand-in answers /api/server like any other path).
        key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
        self.store.save(dict(self.store.load(), proof={"server_id": "abc", "proof_id": "17", "proof_key": key}))
        response, data = self.ask("GET", "/%s/api/hubs/home" % SECRET)
        self.assertEqual((response.status, json.loads(data)["code"]), (502, "another_device"))
        self.assertEqual([r.path.split("?")[0] for r in self.server.requests], ["/api/server"])
        self.assertNotIn("authorization", self.server.requests[0].headers)

    def test_stopping_ends_every_connection_at_once(self):
        # Kodi kills the service when its threads outlive 5 seconds.
        idle = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        idle.request("GET", "/%s/status" % SECRET, headers={"Host": "127.0.0.1:%d" % self.port})
        idle.getresponse().read()
        streaming = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        streaming.request("GET", "/%s/api/files/7/stream" % SECRET, headers={"Host": "127.0.0.1:%d" % self.port})
        response = streaming.getresponse()
        response.read(1024)
        started = time.monotonic()
        self.proxy.stop()
        self.assertLess(time.monotonic() - started, 2)
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline and self.proxy_threads():
            time.sleep(0.05)
        self.assertEqual(self.proxy_threads(), [])
        idle.close()
        streaming.close()

    def proxy_threads(self):
        return [t for t in threading.enumerate() if t.name.startswith("pley-proxy") and t.is_alive()]

    def test_a_live_channel_comes_as_it_is_sent(self):
        # A server that sends a little of a channel, then nothing for a
        # while: a radio channel takes a quarter of a minute to fill the
        # proxy's chunk, and Kodi mustn't wait for it.
        from pley.autoquality import Meter

        packets = b"\x47" + bytes(187)
        sent = packets * 10
        release = threading.Event()
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)

        def serve():
            conn, _ = listener.accept()
            conn.recv(65536)
            # A server that says it takes ranges.
            conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nAccept-Ranges: bytes\r\nTransfer-Encoding: chunked\r\n\r\n")
            conn.sendall(b"%x\r\n%s\r\n" % (len(sent), sent))
            release.wait(10)
            conn.sendall(b"0\r\n\r\n")
            conn.close()

        threading.Thread(target=serve, daemon=True).start()
        self.addCleanup(listener.close)
        self.addCleanup(release.set)
        self.store.save(dict(self.store.load(), server="http://127.0.0.1:%d" % listener.getsockname()[1]))
        self.proxy.meter = Meter()
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=5)
        self.addCleanup(sock.close)
        sock.sendall(b"GET /%s/api/live/channels/1/stream.ts?client_id=kodi-1 HTTP/1.1\r\nHost: 127.0.0.1:%d\r\n\r\n" % (SECRET.encode(), self.port))
        got = b""
        started = time.monotonic()
        while sent not in got:
            got += sock.recv(65536)
        self.assertLess(time.monotonic() - started, 3, "the packets come while the server holds the rest")
        head = got.split(b"\r\n\r\n", 1)[0].lower()
        self.assertIn(b"video/mp2t", head)
        # No length, no ranges: Kodi's player can neither pause nor seek it,
        # as Live TV has neither.
        self.assertNotIn(b"accept-ranges", head)
        self.assertNotIn(b"content-length", head)
        self.assertEqual((list(self.proxy.meter.answers), list(self.proxy.meter.chunks)), ([], []), "Automatic doesn't measure a channel")
        # The server ends the channel: said before Kodi reads the end.
        ended = []
        self.proxy.on_ended = lambda: ended.append(len(got))
        release.set()
        while True:
            piece = sock.recv(65536)
            if not piece:
                break
        self.assertEqual(len(ended), 1)

    def test_a_live_channel_kodi_lets_go_of_hasnt_ended(self):
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)

        def serve():
            conn, _ = listener.accept()
            conn.recv(65536)
            conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nTransfer-Encoding: chunked\r\n\r\n")
            try:
                while True:
                    conn.sendall(b"%x\r\n%s\r\n" % (188 * 100, bytes(188 * 100)))
                    time.sleep(0.02)
            except OSError:
                pass
            conn.close()

        threading.Thread(target=serve, daemon=True).start()
        self.addCleanup(listener.close)
        self.store.save(dict(self.store.load(), server="http://127.0.0.1:%d" % listener.getsockname()[1]))
        ended = []
        self.proxy.on_ended = lambda: ended.append(1)
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=5)
        sock.sendall(b"GET /%s/api/live/channels/1/stream.ts HTTP/1.1\r\nHost: 127.0.0.1:%d\r\n\r\n" % (SECRET.encode(), self.port))
        sock.recv(65536)
        # Kodi stops: the channel goes on for others, nothing is said.
        sock.close()
        time.sleep(1)
        self.assertEqual(ended, [])

    def test_a_live_channel_without_a_connection_free_is_told(self):
        busy = []
        self.proxy.on_busy = lambda *heard: busy.append(heard)
        response, data = self.ask("GET", "/%s/api/live/channels/3/stream.ts?client_id=kodi-1" % SECRET)
        self.assertEqual(response.status, 409)
        message, connections, path = busy[0]
        self.assertEqual(message, "The one connection of Test Provider is in use, watching Sport.")
        self.assertEqual([c["channel_name"] for c in connections], ["Sport"], "what holds each connection")
        self.assertEqual(path, "/api/live/channels/3/stream.ts?client_id=kodi-1")
        self.assertEqual(self.stopped, [])

    def test_subtitles_are_named_for_kodi(self):
        named = api.subtitle_name("/api/files/7/subtitles/3.vtt", "ENG", True)
        self.assertEqual(named, "/api/files/7/subtitles/3.eng.forced.vtt")
        self.assertEqual(api.subtitle_name("/api/files/7/subtitles/3.vtt", None, False), "/api/files/7/subtitles/3.vtt")
        self.assertEqual(api.subtitle_name("/api/files/7/subtitles/3.vtt", "x-1", False), "/api/files/7/subtitles/3.vtt")
        response, data = self.ask("GET", "/%s/api/files/7/subtitles/1003.fre.forced.vtt" % SECRET)
        self.assertEqual(json.loads(data)["path"], "/api/files/7/subtitles/1003.vtt", "the server's own name")
        response, data = self.ask("GET", "/%s/api/files/7/subtitles/4.eng.sup" % SECRET)
        self.assertEqual(json.loads(data)["path"], "/api/files/7/subtitles/4.sup", "PGS as .sup too")

    def test_the_servers_redirects_stay_behind_the_proxy(self):
        response, _ = self.ask("GET", "/%s/api/collections/4/images/poster" % SECRET)
        self.assertEqual(response.status, 307)
        self.assertEqual(response.getheader("Location"), "/%s/api/items/9/images/poster?v=3" % SECRET)
        response, _ = self.ask("GET", "/%s/api/collections/5/images/poster" % SECRET)
        self.assertEqual(response.getheader("Location"), "/%s/api/items/10/images/poster" % SECRET)

    def test_addresses_the_server_gives_become_the_proxys(self):
        base = "http://127.0.0.1:5000/s3cret"
        self.assertEqual(path_in(base, "/api/items/1/images/poster?v=2&w=500"), base + "/api/items/1/images/poster?v=2&w=500")
        self.assertEqual(path_in(base, "/api/playback/abc/master.m3u8"), base + "/api/playback/abc/master.m3u8")


if __name__ == "__main__":
    unittest.main()
