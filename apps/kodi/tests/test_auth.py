import json
import os
import shutil
import tempfile
import threading
import time
import unittest
import urllib.parse

import support
from pley import api, auth


class TokenServer:
    """A token endpoint that rotates refresh tokens as pley does: each one
    works once."""

    def __init__(self):
        self.issued = 1
        self.refreshes = 0
        self.down = False
        # More in each token answer (a proof key).
        self.extra = {}
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
            dict({"access_token": "at-%d" % n, "refresh_token": "rt-%d" % n, "expires_in": 3600, "scope": "library play"}, **self.extra),
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
        self.assertEqual(self.tokens.session()[1], "at-1")
        self.assertEqual(self.server.refreshes, 0)

    def test_a_token_about_to_expire_is_renewed_and_saved_first(self):
        self.now += 3600 - auth.REFRESH_MARGIN + 1
        self.assertEqual(self.tokens.session()[1], "at-2")
        saved = self.store.load()
        self.assertEqual((saved["access_token"], saved["refresh_token"]), ("at-2", "rt-2"))
        self.assertEqual(saved["expires_at"], self.now + 3600)
        self.assertEqual(self.tokens.session()[1], "at-2")
        self.assertEqual(self.server.refreshes, 1)

    def test_refused_tokens_are_renewed_once_however_many_ask(self):
        got = []
        threads = [threading.Thread(target=lambda: got.append(self.tokens.refreshed("at-1", self.server.url))) for _ in range(6)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(got, ["at-2"] * 6)
        self.assertEqual(self.server.refreshes, 1, "two refreshes with one token would end the sign-in")
        # A token refused after it was renewed already is answered with the
        # new one.
        self.assertEqual(self.tokens.refreshed("at-1", self.server.url), "at-2")
        self.assertEqual(self.server.refreshes, 1)

    def test_a_sign_in_the_server_ended_is_forgotten(self):
        self.store.save(self.record(refresh_token="rt-0"))
        with self.assertRaises(auth.SignedOut):
            self.tokens.refreshed("at-1", self.server.url)
        self.assertIsNone(self.store.load())
        with self.assertRaises(auth.SignedOut):
            self.tokens.session()

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
        self.assertEqual(self.tokens.session()[1], "at-2")
        self.assertEqual(self.tokens.identity(), (self.server.url, 3, None))
        # A refresh keeps the profile.
        self.assertEqual(self.tokens.refreshed("at-2", self.server.url), "at-3")
        self.assertEqual(self.store.load()["profile"], 3)
        # A refresh token that doesn't work any more ends the sign-in.
        self.store.save(self.record(refresh_token="rt-0"))
        with self.assertRaises(auth.SignedOut):
            self.tokens.switch_profile(4)
        self.assertIsNone(self.store.load())

    def test_a_server_that_doesnt_answer_keeps_the_tokens(self):
        self.server.down = True
        with self.assertRaises(api.ApiError):
            self.tokens.refreshed("at-1", self.server.url)
        self.assertEqual(self.store.load()["refresh_token"], "rt-1")

    def test_a_sign_in_the_plugin_wrote_since_is_read_again(self):
        self.assertEqual(self.tokens.session()[1], "at-1")
        self.store.save(self.record(access_token="at-from-the-plugin", server_name="Another name"))
        self.assertEqual(self.tokens.session()[1], "at-from-the-plugin")
        self.store.clear()
        self.assertIsNone(self.tokens.server())

    def test_a_sign_in_made_during_a_refresh_is_kept(self):
        # The plugin signs in again while the service refreshes: the new
        # sign-in stays, and what the refresh got is ended.
        self.server.during_refresh = lambda: self.store.save(self.record(access_token="at-new", refresh_token="rt-new"))
        self.assertEqual(self.tokens.refreshed("at-1", self.server.url), "at-new")
        self.assertEqual(self.store.load()["refresh_token"], "rt-new")
        revoked = [r.form()["token"] for r in self.server.server.requests if r.path == "/api/oauth/revoke"]
        self.assertEqual(revoked, ["rt-2"])

    def test_a_token_never_goes_to_another_server(self):
        # Signed in to another server while this one held its answer (a
        # server that isn't to be trusted): its 401 gets nothing, not the
        # other server's token.
        other = "http://other.example:7539"

        def sign_in_there():
            self.store.save(self.record(server=other, access_token="at-other", refresh_token="rt-other"))

        sign_in_there()
        with self.assertRaises(auth.SignedOut):
            self.tokens.refreshed("at-1", self.server.url)
        self.assertEqual(self.tokens.session(), (other, "at-other"))
        # Nor when the other sign-in comes while the refresh is asked.
        self.store.save(self.record())
        self.server.during_refresh = sign_in_there
        with self.assertRaises(auth.SignedOut):
            self.tokens.refreshed("at-1", self.server.url)
        self.assertEqual(self.tokens.session(), (other, "at-other"))

    def test_a_server_left_behind_cant_sign_the_add_on_out_of_the_new_one(self):
        # The old server ends the refresh it held once the plugin signed in
        # elsewhere (it learns when: the add-on revokes its grant there).
        other = "http://other.example:7539"
        self.store.save(self.record(refresh_token="rt-0"))
        self.server.during_refresh = lambda: self.store.save(
            self.record(server=other, access_token="at-other", refresh_token="rt-other")
        )
        with self.assertRaises(auth.SignedOut):
            self.tokens.refreshed("at-1", self.server.url)
        self.assertEqual(self.store.load()["server"], other, "the new sign-in stays")
        self.assertEqual(self.tokens.session(), (other, "at-other"))

    @unittest.skipIf(os.name == "nt", "file modes are POSIX")
    def test_tokens_are_readable_by_this_user_only(self):
        self.assertEqual(os.stat(self.store.path).st_mode & 0o777, 0o600)

    def test_signing_out_ends_the_sign_in_on_the_server(self):
        self.tokens.sign_out()
        revoke = [r for r in self.server.server.requests if r.path == "/api/oauth/revoke"]
        self.assertEqual(revoke[0].form(), {"token": "rt-1", "client_id": auth.CLIENT_ID})
        self.assertIsNone(self.store.load())


ID = "0123456789abcdef0123456789abcdef01234567"
KEY = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
OTHER_KEY = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk"
CHALLENGE = "AAECAwQFBgcICQoLDA0ODw"
# The sign-in answers every app reads the same (the web TV app's and the
# native Samsung app's tests read them too).
FIXTURE = os.path.join(support.HERE, "..", "..", "..", "packages", "tv", "test", "fixtures", "auth.json")


class ProvingServer(TokenServer):
    """The token server, which proves it holds KEY when asked, at its own
    address, unless `answer(query)` answers instead."""

    def __init__(self):
        super().__init__()
        self.answer = None

    def handle(self, request):
        parts = urllib.parse.urlsplit(request.path)
        if parts.path != "/api/server":
            return super().handle(request)
        query = dict(urllib.parse.parse_qsl(parts.query))
        if self.answer is not None:
            return self.answer(query)
        assert query["proof_id"] == "17"
        return support.answer(200, {"id": ID, "name": "Test", "proof": auth.proof(auth.key_of(KEY), ID, self.url, query["challenge"])})


class ProofTest(unittest.TestCase):
    """No token goes to the server's address before the server there proves
    it is the one signed in to (docs/API.md, "Telling servers apart")."""

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.server = ProvingServer()
        self.store = auth.Store(os.path.join(self.dir, "tokens.json"))
        self.now = 1_000_000.0
        self.store.save(self.record())
        self.tokens = auth.Tokens(self.store, "pleyKodi/test", clock=lambda: self.now, random=lambda n: bytes(range(n)))

    def tearDown(self):
        self.server.server.close()
        shutil.rmtree(self.dir)

    def record(self, **changes):
        data = {
            "server": self.server.url,
            "server_name": "Test",
            "server_id": ID,
            "token_endpoint": self.server.url + "/api/oauth/token",
            "revocation_endpoint": self.server.url + "/api/oauth/revoke",
            "access_token": "at-1",
            "expires_at": self.now + 3600,
            "refresh_token": "rt-1",
            "scope": "library play",
            "sign_in": "a",
            "proof": {"server_id": ID, "proof_id": "17", "proof_key": KEY},
        }
        data.update(changes)
        return data

    def refuse(self):
        """Another device at the address: its answer proves nothing."""
        self.server.answer = lambda query: support.answer(200, {"id": ID, "proof": "AAAA"})

    def asked(self):
        return [urllib.parse.urlsplit(r.path).path for r in self.server.server.requests]

    def test_the_server_proves_itself_once_before_the_token_goes(self):
        self.assertEqual(self.tokens.session()[1], "at-1")
        first = self.server.server.requests[0]
        self.assertEqual(first.path, "/api/server?challenge=%s&proof_id=17" % CHALLENGE)
        self.assertNotIn("authorization", first.headers)
        self.assertEqual(self.tokens.session()[1], "at-1")
        self.assertEqual(self.asked(), ["/api/server"])

    def test_it_proves_itself_again_after_half_an_hour_a_request_without_an_answer_or_a_wake(self):
        self.tokens.session()
        self.now += auth.PROOF_TTL - 1
        self.tokens.session()
        self.assertEqual(self.asked().count("/api/server"), 1)
        self.now += 1
        self.tokens.session()
        self.assertEqual(self.asked().count("/api/server"), 2)
        self.tokens.doubt()
        self.tokens.session()
        self.assertEqual(self.asked().count("/api/server"), 3)
        auth.network_may_have_changed()
        self.tokens.session()
        self.assertEqual(self.asked().count("/api/server"), 4)
        # A clock set back.
        self.now -= 60
        self.tokens.session()
        self.assertEqual(self.asked().count("/api/server"), 5)

    def test_another_device_at_the_address_gets_no_token(self):
        another = auth.proof(auth.key_of(KEY), "another", self.server.url, CHALLENGE)
        for status, body in (
            (200, {"id": ID, "proof": "AAAA"}),
            (200, {"id": "another", "proof": another}),
            (200, {"id": ID, "proof": None}),
            (200, {"id": ID}),
            (403, {"message": "a proof for a plain-http address is made for the home network only"}),
            (404, {"code": "not_found"}),
        ):
            with self.subTest(status=status, body=body):
                self.server.answer = lambda query, status=status, body=body: support.answer(status, body)
                with self.assertRaises(auth.NotYourServer) as refused:
                    self.tokens.session()
                self.assertEqual((refused.exception.status, refused.exception.code), (0, "another_device"))
                with self.assertRaises(auth.NotYourServer):
                    self.tokens.refreshed("at-1", self.server.url)
        # A redirect, which could lead to the real server from anywhere, and a router's page prove nothing.
        for answer in ((302, {"Location": self.server.url + "/api/server"}, b""), (200, {"Content-Type": "text/html"}, b"<html>a router</html>")):
            with self.subTest(answer=answer):
                self.server.answer = lambda query, answer=answer: answer
                with self.assertRaises(auth.NotYourServer):
                    self.tokens.session()
        self.assertEqual(set(self.asked()), {"/api/server"})

    def test_a_server_error_is_out_of_reach_not_another_device(self):
        self.server.answer = lambda query: support.answer(502, {"code": "bad_gateway"})
        with self.assertRaises(api.ApiError) as refused:
            self.tokens.session()
        self.assertNotIsInstance(refused.exception, auth.NotYourServer)
        self.assertEqual((refused.exception.status, refused.exception.code), (0, "unreachable"))

    def test_a_refresh_waits_for_a_proof_made_then_and_keeps_the_key(self):
        self.tokens.session()
        # An answer bringing another key doesn't replace the sign-in's.
        self.server.extra = {"server_id": ID, "proof_id": "18", "proof_key": OTHER_KEY}
        self.assertEqual(self.tokens.refreshed("at-1", self.server.url), "at-2")
        self.assertEqual(self.asked(), ["/api/server", "/api/server", "/api/oauth/token"])
        self.assertEqual(auth.proof_of(self.store.load())["proof_key"], KEY)

    def test_a_token_about_to_expire_waits_for_one_proof_its_refreshs(self):
        self.now += 3600 - auth.REFRESH_MARGIN + 1
        self.assertEqual(self.tokens.session()[1], "at-2")
        self.assertEqual(self.asked(), ["/api/server", "/api/oauth/token"])

    def test_a_token_another_refresh_gave_goes_out_only_after_a_proof(self):
        self.assertEqual(self.tokens.refreshed("at-0", self.server.url), "at-1")
        self.assertEqual(self.asked(), ["/api/server"])

    def test_a_sign_in_made_during_a_refresh_waits_for_its_own_proof(self):
        # The plugin signs in again while the service refreshes, and the
        # device at the address can't prove the new sign-in's server: its
        # token goes nowhere, whether it was about to expire or refused.
        def sign_in_again():
            self.store.save(self.record(sign_in="bb", access_token="at-new", refresh_token="rt-new"))
            self.refuse()

        self.server.during_refresh = sign_in_again
        self.now += 3600 - auth.REFRESH_MARGIN + 1
        with self.assertRaises(auth.NotYourServer):
            self.tokens.session()
        self.assertEqual(self.asked(), ["/api/server", "/api/oauth/token", "/api/oauth/revoke", "/api/server"])
        self.server.answer = None
        self.store.save(self.record(sign_in="c", access_token="at-3", refresh_token="rt-%d" % self.server.issued))
        with self.assertRaises(auth.NotYourServer):
            self.tokens.refreshed("at-3", self.server.url)
        self.assertEqual(self.asked()[4:], ["/api/server", "/api/oauth/token", "/api/oauth/revoke", "/api/server"])

    def test_a_sign_in_from_before_gets_its_key_at_its_next_refresh(self):
        self.store.save(self.record(proof=None))
        self.server.extra = {"server_id": ID, "proof_id": "17", "proof_key": KEY}
        self.assertEqual(self.tokens.refreshed("at-1", self.server.url), "at-2")
        self.assertEqual(self.asked(), ["/api/oauth/token"])
        self.assertEqual(auth.proof_of(self.store.load())["proof_key"], KEY)

    def test_a_profile_switch_and_its_pin_wait_for_a_proof_made_then(self):
        self.tokens.session()
        self.tokens.switch_profile(3, "1234")
        self.assertEqual(self.asked(), ["/api/server", "/api/server", "/api/oauth/token"])
        self.refuse()
        with self.assertRaises(auth.NotYourServer):
            self.tokens.switch_profile(3, "1234")
        self.assertEqual(self.asked()[3:], ["/api/server"])

    def test_signing_out_sends_another_device_nothing(self):
        self.refuse()
        self.tokens.sign_out()
        self.assertEqual(self.asked(), ["/api/server"])
        self.assertIsNone(self.store.load())

    def test_the_sign_in_before_ends_only_where_the_server_proves_its_key(self):
        self.assertTrue(auth.proves(self.record(), "pleyKodi/test"))
        self.refuse()
        self.assertFalse(auth.proves(self.record(), "pleyKodi/test"))
        self.assertTrue(auth.proves(self.record(proof=None), "pleyKodi/test"), "nothing to check")

    def test_another_sign_in_has_its_server_prove_itself_anew(self):
        self.tokens.session()
        self.store.save(self.record(sign_in="bb", access_token="at-99", refresh_token="rt-99"))
        self.assertEqual(self.tokens.session()[1], "at-99")
        self.assertEqual(self.asked().count("/api/server"), 2)

    def test_a_sign_in_keeps_the_key_its_tokens_came_with(self):
        flow = auth.DeviceSignIn(self.server.url, "pleyKodi/test")
        tokens ={"access_token": "at", "refresh_token": "rt", "server_id": ID, "proof_id": "17", "proof_key": KEY}
        self.assertEqual(flow.record(tokens, "Test", ID)["proof"], {"server_id": ID, "proof_id": "17", "proof_key": KEY})
        self.assertNotIn("proof", flow.record(dict(tokens, proof_key=KEY + "="), "Test", ID))

    def test_answers_and_proofs_as_the_shared_fixture_has_them(self):
        with open(FIXTURE, encoding="utf-8") as f:
            fixture = json.load(f)
        for case in fixture["tokenResponse"]:
            given = (case["out"].get("ok") or {}).get("proof")
            expected = {"server_id": given["serverId"], "proof_id": given["proofId"], "proof_key": given["proofKey"]} if given else {}
            self.assertEqual(auth.proof_from(case["in"]["body"]), expected, case["in"])
        for case in fixture["serverProof"]:
            c = case["in"]
            self.assertEqual(auth.proof(auth.key_of(c["key"]), c["serverId"], c["address"], c["challenge"]), case["out"])


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
