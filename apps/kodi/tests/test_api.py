import unittest

import support
from pley import api


class NormalizeTest(unittest.TestCase):
    def test_addresses_are_written_as_pley_writes_its_issuer(self):
        cases = {
            "192.168.1.5": "http://192.168.1.5:7539",
            "192.168.1.5:7539": "http://192.168.1.5:7539",
            "HTTP://NAS.local:8096/": "http://nas.local:8096",
            "http://nas.local:80": "http://nas.local",
            "https://pley.example.com": "https://pley.example.com",
            "https://pley.example.com:443/": "https://pley.example.com",
            "http://[FE80::1]:7539": "http://[fe80::1]:7539",
            " nas:7539 ": "http://nas:7539",
        }
        for typed, written in cases.items():
            self.assertEqual(api.normalize(typed), written, typed)

    def test_anything_else_is_refused(self):
        for typed in ["ftp://nas", "http://", "http://nas/pley", "http://user@nas", "http://nas:99999", "nas?x=1"]:
            with self.assertRaises(ValueError, msg=typed):
                api.normalize(typed)


class NotPleyTest(unittest.TestCase):
    def test_a_web_page_is_said_to_be_something_else(self):
        server = support.FakeServer(lambda request: (200, {"Content-Type": "text/html"}, b"<html>router</html>"))
        try:
            with self.assertRaises(api.ApiError) as refused:
                api.Client(server.url, "pleyKodi/test").get("/.well-known/oauth-authorization-server")
            self.assertEqual(refused.exception.code, "not_pley")
        finally:
            server.close()


class UnencryptedTest(unittest.TestCase):
    def test_only_this_network_is_private(self):
        for private in ["127.0.0.1", "10.1.2.3", "172.20.0.5", "192.168.1.5", "169.254.9.9", "100.101.102.103",
                        "::1", "fd12::5", "fe80::1%eth0", "::ffff:192.168.1.5"]:
            self.assertTrue(api.is_private_address(private), private)
        for public in ["81.2.69.160", "8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:db8::1", "::ffff:81.2.69.160",
                       "", "nas"]:
            self.assertFalse(api.is_private_address(public), public)

    def test_plain_http_to_the_internet_sends_nothing(self):
        server = support.FakeServer(lambda request: support.answer(200, {}))
        real = api.is_private_address
        # The stand-in server, as if it were on the internet.
        api.is_private_address = lambda address: False
        try:
            client = api.Client(server.url, "pleyKodi/test", token="pley_at_secret")
            with self.assertRaises(api.ApiError) as refused:
                client.get("/api/me")
            self.assertEqual(refused.exception.code, "unencrypted")
            with self.assertRaises(api.Unencrypted):
                api.connect(server.url)
            self.assertEqual(server.requests, [], "no token, no request")
        finally:
            api.is_private_address = real
        # On this network it goes through.
        self.assertEqual(api.Client(server.url, "pleyKodi/test").get("/api/me"), {})
        server.close()

    def test_https_is_never_held_to_it(self):
        class Sock:
            def getpeername(self):
                return ("81.2.69.160", 443)

        api.check_unencrypted("https", Sock())
        with self.assertRaises(api.Unencrypted):
            api.check_unencrypted("http", type("S", (Sock,), {"close": lambda self: None})())


class ErrorsTest(unittest.TestCase):
    def test_both_kinds_of_refusal_are_read(self):
        pley = api.error_of(api.Response(404, {}, b'{"code": "not_found", "message": "item 42 not found"}'))
        self.assertEqual((pley.status, pley.code, pley.message), (404, "not_found", "item 42 not found"))
        oauth = api.error_of(api.Response(400, {}, b'{"error": "invalid_grant", "error_description": "used"}'))
        self.assertEqual((oauth.code, oauth.message), ("invalid_grant", "used"))
        bare = api.error_of(api.Response(502, {}, b"<html>bad gateway</html>"))
        self.assertEqual((bare.code, bare.message), ("http_502", "the server answered 502"))
        self.assertEqual(bare.body, {})

    def test_what_a_refusal_says_besides_is_kept(self):
        # When to ask again, which a sentence alone would hide.
        body = b'{"code": "declined", "message": "declined", "retry_at": 1700000000}'
        refusal = api.error_of(api.Response(409, {}, body))
        self.assertEqual(refusal.body["retry_at"], 1700000000)
        self.assertEqual(api.ApiError(0, "unreachable", "down").body, {})


if __name__ == "__main__":
    unittest.main()
