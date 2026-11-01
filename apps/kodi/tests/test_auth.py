import os
import shutil
import tempfile
import threading
import time
import unittest

import support
from pley import api, auth


class TokenServer:
    """A token endpoint that rotates refresh tokens as pley does: each one
    works once."""

    def __init__(self):
        self.issued = 1
        self.refreshes = 0
        self.down = False
        # What happens while a refresh is asked (another sign-in, say).
        self.during_refresh = None
        self.lock = threading.Lock()
        self.server = support.FakeServer(self.handle)
        self.url = self.server.url

    def handle(self, request):
        if request.path == "/.well-known/oauth-authorization-server":
            return support.answer(200, support.metadata(self.url))
        if request.path == "/api/oauth/revoke":
            return 200, {}, b""
        if request.path != "/api/oauth/token":
            return support.answer(404, {"code": "not_found"})
        if self.down:
            return support.answer(503, {"error": "temporarily_unavailable"})
        form = request.form()
        assert form["client_id"] == auth.CLIENT_ID
        if self.during_refresh:
            self.during_refresh()
        with self.lock:
            if form.get("refresh_token") != "rt-%d" % self.issued:
                return support.answer(400, {"error": "invalid_grant", "error_description": "used again"})
            if form["grant_type"] == auth.SWITCH_GRANT:
                # Profile 3 has the PIN 1234; profile 9 needs its password.
                if form["profile"] == "9":
                    return support.answer(400, {"error": "invalid_grant", "reason": "browser", "error_description": "sign in again"})
                if form["profile"] == "3" and form.get("pin") != "1234":
                    return support.answer(400, {"error": "invalid_grant", "reason": "wrong_pin", "error_description": "wrong PIN"})
            time.sleep(0.05)
            self.issued += 1
            self.refreshes += 1
            n = self.issued
        return support.answer(
            200,
            {"access_token": "at-%d" % n, "refresh_token": "rt-%d" % n, "expires_in": 3600, "scope": "library play"},
        )


class TokensTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.server = TokenServer()
        self.store = auth.Store(os.path.join(self.dir, "tokens.json"))
        self.now = 1_000_000.0
        self.store.save(self.record(expires_at=self.now + 3600))
        self.tokens = auth.Tokens(self.store, "pleyKodi/test", clock=lambda: self.now)

    def tearDown(self):
        self.server.server.close()
        shutil.rmtree(self.dir)

    def record(self, **changes):
        data = {
            "server": self.server.url,
            "server_name": "Test",
            "server_id": "abc",
            "token_endpoint": self.server.url + "/api/oauth/token",
            "revocation_endpoint": self.server.url + "/api/oauth/revoke",
            "access_token": "at-1",
            "expires_at": self.now + 3600,
            "refresh_token": "rt-1",
            "scope": "library play",
        }
        data.update(changes)
        return data

    def test_a_fresh_token_is_used_as_it_is(self):
        self.assertEqual(self.tokens.access(), "at-1")
        self.assertEqual(self.server.refreshes, 0)

    def test_a_token_about_to_expire_is_renewed_and_saved_first(self):
        self.now += 3600 - auth.REFRESH_MARGIN + 1
        self.assertEqual(self.tokens.access(), "at-2")
        saved = self.store.load()
        self.assertEqual((saved["access_token"], saved["refresh_token"]), ("at-2", "rt-2"))
        self.assertEqual(saved["expires_at"], self.now + 3600)
        self.assertEqual(self.tokens.access(), "at-2")
        self.assertEqual(self.server.refreshes, 1)

    def test_refused_tokens_are_renewed_once_however_many_ask(self):
        got = []
        threads = [threading.Thread(target=lambda: got.append(self.tokens.refreshed("at-1"))) for _ in range(6)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(got, ["at-2"] * 6)
        self.assertEqual(self.server.refreshes, 1, "two refreshes with one token would end the sign-in")
        # A token refused after it was renewed already is answered with the
        # new one.
        self.assertEqual(self.tokens.refreshed("at-1"), "at-2")
        self.assertEqual(self.server.refreshes, 1)

    def test_a_sign_in_the_server_ended_is_forgotten(self):
        self.store.save(self.record(refresh_token="rt-0"))
        with self.assertRaises(auth.SignedOut):
            self.tokens.refreshed("at-1")
        self.assertIsNone(self.store.load())
        with self.assertRaises(auth.SignedOut):
            self.tokens.access()

    def test_another_sign_in_is_another_identity(self):
        # Someone else may have approved it, on the same server, with no
        # profile switched to.
        self.store.save(self.record(sign_in="a"))
        first = self.tokens.identity()
        self.assertEqual(first, (self.server.url, None, "a"))
        self.store.save(self.record(sign_in="bb", access_token="at-9", refresh_token="rt-9"))
        self.assertNotEqual(self.tokens.identity(), first)

    def test_switching_profiles(self):
        self.assertEqual(self.tokens.identity(), (self.server.url, None, None))
        # A wrong PIN spends nothing.
        with self.assertRaises(api.ApiError) as refused:
            self.tokens.switch_profile(3, "9999")
        self.assertEqual(refused.exception.code, "wrong_pin")
        self.assertEqual(self.store.load()["refresh_token"], "rt-1")
        with self.assertRaises(api.ApiError) as refused:
            self.tokens.switch_profile(9)
        self.assertEqual(refused.exception.code, "browser")
        # The right one: new tokens acting as the profile, saved before use.
        self.tokens.switch_profile(3, "1234")
        saved = self.store.load()
        self.assertEqual((saved["access_token"], saved["refresh_token"], saved["profile"]), ("at-2", "rt-2", 3))
        self.assertEqual(self.tokens.access(), "at-2")
        self.assertEqual(self.tokens.identity(), (self.server.url, 3, None))
        # A refresh keeps the profile.
        self.assertEqual(self.tokens.refreshed("at-2"), "at-3")
        self.assertEqual(self.store.load()["profile"], 3)
        # A refresh token that doesn't work any more ends the sign-in.
        self.store.save(self.record(refresh_token="rt-0"))
        with self.assertRaises(auth.SignedOut):
            self.tokens.switch_profile(4)
        self.assertIsNone(self.store.load())

    def test_a_server_that_doesnt_answer_keeps_the_tokens(self):
        self.server.down = True
        with self.assertRaises(api.ApiError):
            self.tokens.refreshed("at-1")
        self.assertEqual(self.store.load()["refresh_token"], "rt-1")

    def test_a_sign_in_the_plugin_wrote_since_is_read_again(self):
        self.assertEqual(self.tokens.access(), "at-1")
        self.store.save(self.record(access_token="at-from-the-plugin", server_name="Another name"))
        self.assertEqual(self.tokens.access(), "at-from-the-plugin")
        self.store.clear()
        self.assertIsNone(self.tokens.server())

    def test_a_sign_in_made_during_a_refresh_is_kept(self):
        # The plugin signs in again while the service refreshes: the new
        # sign-in stays, and what the refresh got is ended.
        self.server.during_refresh = lambda: self.store.save(self.record(access_token="at-new", refresh_token="rt-new"))
        self.assertEqual(self.tokens.refreshed("at-1"), "at-new")
        self.assertEqual(self.store.load()["refresh_token"], "rt-new")
        revoked = [r.form()["token"] for r in self.server.server.requests if r.path == "/api/oauth/revoke"]
        self.assertEqual(revoked, ["rt-2"])

    @unittest.skipIf(os.name == "nt", "file modes are POSIX")
    def test_tokens_are_readable_by_this_user_only(self):
        self.assertEqual(os.stat(self.store.path).st_mode & 0o777, 0o600)

    def test_signing_out_ends_the_sign_in_on_the_server(self):
        self.tokens.sign_out()
        revoke = [r for r in self.server.server.requests if r.path == "/api/oauth/revoke"]
        self.assertEqual(revoke[0].form(), {"token": "rt-1", "client_id": auth.CLIENT_ID})
        self.assertIsNone(self.store.load())


class DeviceSignInTest(unittest.TestCase):
    def setUp(self):
        self.answers = []
        self.meta = None
        self.asked = []
        self.server = support.FakeServer(self.handle)

    def tearDown(self):
        self.server.close()

    def handle(self, request):
        if request.path == "/.well-known/oauth-authorization-server":
            return support.answer(200, self.meta or support.metadata(self.server.url))
        if request.path == "/api/oauth/device":
            self.asked.append(request.form())
            self.assertTrue(request.headers["user-agent"].startswith("pleyKodi/"))
            return support.answer(
                200,
                {
                    "device_code": "dc",
                    "user_code": "BCDF-GHJK",
                    "verification_uri": self.server.url + "/link",
                    "expires_in": 600,
                    "interval": 5,
                },
            )
        self.assertEqual(request.form()["grant_type"], auth.DEVICE_GRANT)
        return self.answers.pop(0)

    def test_the_code_is_polled_until_someone_answers(self):
        flow = auth.DeviceSignIn(self.server.url, "pleyKodi/0.1.0 (Kodi 21.3; Windows)")
        self.assertEqual(flow.start()["user_code"], "BCDF-GHJK")
        self.assertEqual(self.asked, [{"client_id": auth.CLIENT_ID, "scope": "library play playlists profiles"}])
        tokens = {"access_token": "at", "refresh_token": "rt", "expires_in": 3600, "scope": "library play"}
        self.answers = [
            support.answer(400, {"error": "authorization_pending"}),
            support.answer(400, {"error": "slow_down"}),
            support.answer(400, {"error": "access_denied"}),
            support.answer(400, {"error": "expired_token"}),
            support.answer(200, tokens),
        ]
        self.assertEqual(flow.poll("dc"), ("pending", None))
        self.assertEqual(flow.poll("dc"), ("slow_down", None))
        self.assertEqual(flow.interval, 10)
        self.assertEqual(flow.poll("dc"), ("denied", None))
        self.assertEqual(flow.poll("dc"), ("expired", None))
        self.assertEqual(flow.poll("dc"), ("allowed", tokens))
        record = flow.record(tokens, "Living room", "abc", now=100)
        self.assertEqual(record["expires_at"], 3700)
        self.assertEqual(record["token_endpoint"], self.server.url + "/api/oauth/token")
        self.assertEqual(record["server"], self.server.url)
        # Each sign-in is named anew: whoever approves the code picks the account.
        self.assertRegex(record["sign_in"], "^[0-9a-f]{16}$")
        self.assertNotEqual(flow.record(tokens, "Living room", "abc")["sign_in"], record["sign_in"])

    def test_requests_are_asked_for_where_the_server_offers_them(self):
        # A server that lists them, one that lists only `requests`, and one
        # from before them (asking for a scope it doesn't know would fail
        # the whole sign-in).
        offered = ["library", "play", "playlists", "profiles", "requests", "approvals", "admin"]
        for supported, scope in (
            (offered, "library play playlists profiles requests approvals"),
            (offered[:5], "library play playlists profiles requests"),
            (offered[:4], "library play playlists profiles"),
        ):
            self.meta = dict(support.metadata(self.server.url), scopes_supported=supported)
            flow = auth.DeviceSignIn(self.server.url, "pleyKodi/test")
            flow.start()
            self.assertEqual(self.asked.pop()["scope"], scope)
            # A token answer without its scope keeps what was asked.
            record = flow.record({"access_token": "at", "refresh_token": "rt"}, "Living room", "abc")
            self.assertEqual(record["scope"], scope)

    def test_a_server_that_isnt_the_one_asked_is_refused(self):
        self.meta = support.metadata("http://elsewhere.example:7539")
        with self.assertRaises(api.ApiError):
            auth.DeviceSignIn(self.server.url, "pleyKodi/test")
        self.meta = dict(support.metadata(self.server.url), token_endpoint="http://elsewhere.example/token")
        with self.assertRaises(api.ApiError):
            auth.DeviceSignIn(self.server.url, "pleyKodi/test")


if __name__ == "__main__":
    unittest.main()
