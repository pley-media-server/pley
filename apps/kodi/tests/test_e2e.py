"""The add-on against a real pley server: signing in with a code, browsing
and playing through the proxy, progress reports and the events they send,
a renewed token and signing out.

Needs a built server and FFmpeg, so it runs only when asked:

    PLEY_BIN=target/debug/pley.exe PLEY_FFMPEG_DIR=<dir with ffmpeg and ffprobe> \
        python -m unittest test_e2e
"""

import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

import support  # noqa: F401
from pley import api, auth, autoquality, caps, kodi, profiles, requests, service
from pley.api import path_in
from pley.events import Events
from pley.proxy import Proxy

PLEY_BIN = os.environ.get("PLEY_BIN")
FFMPEG_DIR = os.environ.get("PLEY_FFMPEG_DIR")
EXE = ".exe" if sys.platform == "win32" else ""
AGENT = "pleyKodi/0.1.0 (Kodi 21.3; Windows)"
PASSWORD = "kodi-test-pass-1"


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_for(check, seconds, what):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        found = check()
        if found:
            return found
        time.sleep(0.25)
    raise AssertionError("waited %ds for %s" % (seconds, what))


@unittest.skipUnless(PLEY_BIN and FFMPEG_DIR, "set PLEY_BIN and PLEY_FFMPEG_DIR to run against a real server")
class EndToEndTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp(prefix="pley-kodi-e2e-")
        movies = os.path.join(cls.dir, "media", "Movies", "Test Pattern (2020)")
        os.makedirs(movies)
        cls.movie = os.path.join(movies, "Test Pattern (2020).mkv")
        subprocess.run(
            [
                os.path.join(FFMPEG_DIR, "ffmpeg" + EXE), "-v", "error", "-y",
                "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=24", "-f", "lavfi", "-i", "sine=frequency=440",
                "-t", "40", "-c:v", "libx264", "-preset", "ultrafast", "-g", "48", "-c:a", "aac", "-shortest", cls.movie,
            ],
            check=True,
        )
        port = free_port()
        cls.server = "http://127.0.0.1:%d" % port
        cls.process = subprocess.Popen(
            [
                PLEY_BIN, "--data-dir", os.path.join(cls.dir, "data"), "--bind", "127.0.0.1:%d" % port,
                "--no-tray", "--no-discovery",
                "--ffmpeg", os.path.join(FFMPEG_DIR, "ffmpeg" + EXE),
                "--ffprobe", os.path.join(FFMPEG_DIR, "ffprobe" + EXE),
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        cls.log = []
        threading.Thread(target=cls.read_log, daemon=True).start()
        code = wait_for(cls.setup_code, 60, "the setup code in the server's log")
        plain = api.Client(cls.server, "e2e")
        set_up = plain.post("/api/setup", {"username": "admin", "password": PASSWORD, "setup_code": code})
        cls.admin = api.Client(cls.server, "e2e", set_up["token"])
        library = cls.admin.post(
            "/api/libraries", {"name": "Movies", "kind": "movies", "paths": [os.path.dirname(movies)]}
        )
        cls.library = library["id"]
        wait_for(lambda: cls.admin.get("/api/libraries/%d/items" % cls.library)["total"], 90, "the library's scan")
        cls.item = cls.admin.get("/api/libraries/%d/items" % cls.library)["items"][0]

    @classmethod
    def read_log(cls):
        for line in cls.process.stdout:
            cls.log.append(line.decode("utf-8", "replace"))

    @classmethod
    def setup_code(cls):
        for line in cls.log:
            found = re.search(r"setup[?#]code=([A-Za-z0-9_-]+)", line)
            if found:
                return found.group(1)
        return None

    @classmethod
    def tearDownClass(cls):
        if sys.platform == "win32":
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(cls.process.pid)], capture_output=True)
        else:
            cls.process.terminate()
        cls.process.wait(30)
        shutil.rmtree(cls.dir, ignore_errors=True)

    def browser(self, method, path, body=None):
        """A call from pley's own pages, signed in as the administrator."""
        headers = {
            "Cookie": "pley_token=" + self.admin.token,
            "Origin": self.server,
            "Sec-Fetch-Site": "same-origin",
        }
        client = api.Client(self.server, "Mozilla/5.0 e2e")
        return client.json(method, path, json_body=body, headers=headers)

    def sign_in(self, store):
        flow = auth.DeviceSignIn(self.server, AGENT)
        # This server offers asking for titles and deciding on them: the
        # add-on asks for both.
        self.assertEqual(flow.scope, "library play playlists profiles requests approvals")
        started = flow.start()
        shown = self.browser("POST", "/api/oauth/device/verify", {"user_code": started["user_code"]})
        self.assertEqual(shown["client"]["name"], "Pley for Kodi")
        self.assertCountEqual(shown["scopes"], ["library", "play", "playlists", "profiles", "requests", "approvals"])
        self.assertEqual(self.browser("POST", "/api/oauth/requests/%s" % shown["id"], {"allow": True}), {"done": True})
        time.sleep(flow.interval)
        state, tokens = flow.poll(started["device_code"])
        self.assertEqual(state, "allowed")
        store.save(flow.record(tokens, "e2e", ""))
        return tokens

    def test_the_add_on_against_a_real_server(self):
        store = auth.Store(os.path.join(self.dir, "tokens.json"))
        tokens = self.sign_in(store)
        self.assertCountEqual(tokens["scope"].split(), ["library", "play", "playlists", "profiles", "requests", "approvals"])
        holder = auth.Tokens(store, AGENT)
        proxy = Proxy(holder, AGENT, "e2e-secret")
        proxy.start()
        try:
            self.through_the_proxy(proxy, holder, store)
        finally:
            proxy.stop()

    def through_the_proxy(self, proxy, holder, store):
        client = api.Client(proxy.base, AGENT, timeout=120)
        self.assertEqual(client.get("/status"), {"signed_in": True, "server": self.server})
        libraries = client.get("/api/libraries")
        self.assertEqual([(l["name"], l["paths"]) for l in libraries], [("Movies", [])], "apps don't see paths")
        self.assertEqual(client.get("/api/libraries/%d/items" % self.library)["total"], 1)

        # Requests are off on a server without a TMDB key: the summary says
        # so, and Discover answers in pley's words, not as a server away.
        summary = requests.summary(client)
        self.assertEqual((summary["enabled"], summary["sign_in_again"]), (False, None))
        self.assertFalse(requests.discovers(summary) or requests.lists(summary))
        with self.assertRaises(api.ApiError) as off:
            client.get("/api/discover/trending")
        self.assertEqual(off.exception.status, 503)
        self.assertEqual(requests.problem(off.exception, kodi.text)[0], "Requests are off")
        item = self.item["id"]
        report = caps.build("", {}, (1920, 1080), "Kodi 21.3 on Windows")

        # Kodi plays the file as it is.
        info = client.post("/api/playback", {"item_id": item, "client_id": "kodi-e2e", "caps": report})
        self.assertEqual((info["method"], info["hls"]), ("direct", False), info.get("reasons"))
        stream = api.Client(path_in(proxy.base, info["url"]), AGENT)
        head = stream.request("GET", "", headers={"Range": "bytes=0-1023"})
        self.assertEqual((head.status, len(head.body)), (206, 1024))
        with open(self.movie, "rb") as f:
            self.assertEqual(head.body, f.read(1024))

        # The service's reports land on the item, and its event stream hears
        # what changed: the resume point the report started, then the
        # session stopped from elsewhere.
        heard = []
        listening = Events(holder, AGENT, heard.append, retry=(0.2,))
        listening.start()
        try:
            wait_for(listening.open.is_set, 10, "the event stream")
            player = service.Player(holder, AGENT, service.Worker())
            body = {"position_ms": 20_000, "duration_ms": info["duration_ms"], "state": "paused", "session_id": info["session_id"]}
            player.call("POST", "/api/items/%d/progress" % item, body)
            self.assertEqual(client.get("/api/items/%d" % item)["user_state"]["position_ms"], 20_000)
            changed = wait_for(lambda: [e for e in heard if e.get("type") == "user_state_changed"], 10, "user_state_changed")
            self.assertEqual(changed[0]["item_ids"][0], item)
            self.assertEqual(self.admin.request("DELETE", "/api/sessions/%s" % info["session_id"]).status, 204)
            stopped = wait_for(lambda: [e for e in heard if e.get("type") == "session_stopped"], 10, "session_stopped")
            self.assertEqual((stopped[0]["session_id"], stopped[0]["by_admin"]), (info["session_id"], False))
        finally:
            listening.stop()
        player.call("DELETE", "/api/playback/%s" % info["session_id"])

        # A lower quality is converted, as HLS through the proxy.
        request = {"item_id": item, "client_id": "kodi-e2e", "caps": report, "max_bitrate": 2000, "max_height": 480}
        info = client.post("/api/playback", request)
        self.assertEqual((info["method"], info["hls"]), ("transcode", True))
        self.assertTrue(info["url"].endswith("/index.m3u8"), info["url"])
        playlist = client.request("GET", info["url"]).body.decode()
        first = next(line for line in playlist.splitlines() if line.endswith(".ts"))
        segment = client.request("GET", info["url"].rsplit("/", 1)[0] + "/" + first)
        self.assertEqual(segment.status, 200)
        self.assertEqual(segment.body[:1], b"\x47", "an MPEG-TS packet")

        # Automatic steps its bitrate as it is sent, with the service's
        # token: the same stream goes on lower, and back.
        self.assertIs(info["live_bitrate"], True)
        watch = autoquality.Watch(0.0, "2m", 2000, None, 720, live=True, lines=480)
        watch.playing_kbps = 1500
        player.set_bitrate(watch, info["session_id"], 1500)
        # The server keeps it within the session's own bitrate.
        self.assertTrue(watch.live)
        self.assertTrue(1000 <= watch.sent_kbps <= 1500, watch.sent_kbps)
        self.assertEqual(watch.playing_kbps, watch.sent_kbps)
        lowered = watch.sent_kbps
        watch.playing_kbps = 2000
        player.set_bitrate(watch, info["session_id"], 2000)
        self.assertTrue(watch.live and lowered <= watch.sent_kbps <= 2000, watch.sent_kbps)
        player.call("DELETE", "/api/playback/%s" % info["session_id"])

        # A file played as it is can't: the watch hears the refusal.
        info = client.post("/api/playback", {"item_id": item, "client_id": "kodi-e2e", "caps": report})
        self.assertEqual((info["method"], info["live_bitrate"]), ("direct", False))
        watch = autoquality.Watch(0.0, "2m", 2000, None, 720, live=True, lines=480)
        player.set_bitrate(watch, info["session_id"], 1500)
        self.assertEqual((watch.live, watch.playing_kbps), (False, 2000))
        player.call("DELETE", "/api/playback/%s" % info["session_id"])

        # An access token the server no longer takes is renewed once.
        saved = store.load()
        store.save(dict(saved, access_token="pley_at_" + "x" * 43))
        self.assertEqual(len(client.get("/api/libraries")), 1)
        renewed = store.load()
        self.assertNotIn(renewed["access_token"], (saved["access_token"], "pley_at_" + "x" * 43))
        self.assertNotEqual(renewed["refresh_token"], saved["refresh_token"])

        # Who's watching?: the household, a wrong PIN said as such and
        # spending nothing, then the kid's profile with its own.
        kid = self.admin.post("/api/users", {"username": "kid", "display_name": "Kid", "managed": True, "home": True, "pin": "2468"})
        household = {p["id"]: p for p in profiles.household(client)}
        self.assertEqual(len(household), 2)
        self.assertTrue(household[client.get("/api/auth/me")["id"]]["current"])
        self.assertEqual((household[kid["id"]]["current"], household[kid["id"]]["needs"]), (False, "pin"))
        with self.assertRaises(api.ApiError) as refused:
            profiles.switch(client, kid["id"], "1111")
        self.assertEqual(refused.exception.code, "wrong_pin")
        profiles.switch(client, kid["id"], "2468")
        self.assertEqual(client.get("/api/auth/me")["id"], kid["id"])
        self.assertEqual(holder.identity(), (self.server, kid["id"], saved["sign_in"]))
        self.assertEqual(client.get("/api/libraries/%d/items" % self.library)["total"], 1)

        # Listed with the account's apps, then signed out.
        apps = self.admin.get("/api/me/apps")
        ours = [a for a in apps if a["client_id"] == auth.CLIENT_ID]
        self.assertEqual((ours[0]["name"], ours[0]["device"]), ("Pley for Kodi", "Kodi 21.3; Windows"))
        client.request("POST", "/sign-out")
        self.assertEqual([a for a in self.admin.get("/api/me/apps") if a["client_id"] == auth.CLIENT_ID], [])
        self.assertEqual(client.request("GET", "/api/libraries").status, 401)


if __name__ == "__main__":
    unittest.main()
