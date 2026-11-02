import os
import shutil
import tempfile
import threading
import unittest
from unittest import mock

import support
import xbmcgui
from pley import api, kodi, plugin, profiles, signin

PNG = b"\x89PNG\r\n\x1a\n" + b"\0" * 16
STATUS = {"name": "Den", "version": "1.0.0", "two_step_required": True}


class QrTest(unittest.TestCase):
    """The QR code of the sign-in, which the server draws."""

    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.temp = os.path.join(self.root, "temp")
        os.mkdir(self.temp)
        patch = mock.patch.object(kodi, "temp_dir", return_value=self.temp)
        patch.start()
        self.addCleanup(patch.stop)
        self.server = support.FakeServer(lambda request: (200, {"Content-Type": "image/png"}, PNG))
        self.addCleanup(self.server.close)
        self.addCleanup(shutil.rmtree, self.root, True)

    def test_the_code_names_the_file(self):
        path = plugin.fetch_qr(self.server.url, "BCDF-GHJK")
        self.assertEqual(path, os.path.join(self.temp, "pley-qr-BCDFGHJK.png"))
        with open(path, "rb") as f:
            self.assertEqual(f.read(), PNG)
        self.assertEqual(self.server.requests[0].path, "/api/oauth/device/qr?code=BCDFGHJK")

    def test_a_code_that_isnt_one_writes_nothing(self):
        # A server's code that would lead the file elsewhere.
        for code in ["../../escaped", "..\\..\\escaped", "AB/CD", "BCDF GHJK", "", "B" * 40]:
            self.assertEqual(plugin.fetch_qr(self.server.url, code), "", code)
        self.assertEqual(self.server.requests, [])
        self.assertEqual(os.listdir(self.root), ["temp"])
        self.assertEqual(os.listdir(self.temp), [])


class SwitchProfileTest(unittest.TestCase):
    """Switch profile, in the add-on's settings: Kodi's list and its number
    dialog, with the PINs asked as on "Who's watching?"."""

    def test_pins_are_asked_as_on_whos_watching(self):
        household = [{"id": 1, "name": "Anna", "current": True, "needs": "none"}, {"id": 2, "name": "Ben", "needs": "none"}]

        class Client:
            def get(self, path):
                return household if path == "/api/oauth/profiles" else {"id": 1, "has_pin": True}

        # Ben took nothing when the list was made and has a PIN since; then
        # a wrong one, then the right one.
        refusals = [api.ApiError(400, "wrong_pin", "this profile is protected by a PIN"), api.ApiError(400, "wrong_pin", "wrong PIN")]
        asked, shown, headings, typed = [], [], [], ["9999", "1234"]

        def switch(client, profile_id, pin=None):
            asked.append((profile_id, pin))
            if refusals:
                raise refusals.pop(0)

        class Dialog:
            def select(self, heading, entries, preselect=-1, useDetails=False):
                shown.append([(e.label, e.label2) for e in entries])
                return 1

            def numeric(self, kind, heading, defaultt="", hidden=False):
                headings.append(heading)
                return typed.pop(0)

        self.addCleanup(kodi.clear_property, profiles.GATE)
        for patch in (
            mock.patch.object(plugin.Plugin, "client", return_value=Client()),
            mock.patch.object(plugin.Plugin, "notify_service"),
            mock.patch.object(plugin.Plugin, "server_name", return_value="Den"),
            mock.patch.object(profiles, "switch", switch),
            mock.patch.object(plugin.xbmcgui, "Dialog", Dialog),
        ):
            patch.start()
            self.addCleanup(patch.stop)
        with mock.patch.object(kodi, "notify") as told:
            plugin.Plugin(["plugin://plugin.video.pley/", "-1", "?route=profiles"]).route_profiles()
        # The one in use says it has a PIN, as its lock does on "Who's watching?".
        self.assertEqual(shown, [[("Anna", kodi.text("pin")), ("Ben", "")]])
        heading = kodi.text("pin_of") % "Ben"
        self.assertEqual(headings, [heading, kodi.text("wrong_pin") + " " + heading])
        self.assertEqual(asked, [(2, None), (2, "9999"), (2, "1234")])
        told.assert_called_with(kodi.text("watching_as") % "Ben")


class TwoStepTest(unittest.TestCase):
    """Whether the server requires two-step sign-in, which the sign-in
    screen says: the phone that approves the code sets it up."""

    def setUp(self):
        self.status = support.answer(200, STATUS)
        # Cleared, the server holds its status back until it is set.
        self.answering = threading.Event()
        self.answering.set()
        self.server = support.FakeServer(self.handle)
        self.addCleanup(self.server.close)
        self.addCleanup(self.answering.set)

    def handle(self, request):
        if request.path == "/api/server":
            self.answering.wait(10)
            return self.status
        if request.path == "/.well-known/oauth-authorization-server":
            return support.answer(200, support.metadata(self.server.url))
        if request.path == "/api/oauth/device":
            code = {"device_code": "dc", "user_code": "BCDF-GHJK", "verification_uri": self.server.url + "/link"}
            return support.answer(200, dict(code, expires_in=600, interval=1))
        if request.path == "/api/oauth/token":
            return support.answer(400, {"error": "access_denied"})
        return support.answer(404, {"code": "not_found"})

    def test_the_server_says_whether_it_does(self):
        self.assertTrue(plugin.two_step_required(self.server.url))
        self.assertEqual(self.server.requests[-1].path, "/api/server")
        # It doesn't, a server from before it, or anything but true.
        for status in (
            dict(STATUS, two_step_required=False),
            {"name": "Den", "version": "0.9.0"},
            dict(STATUS, two_step_required="yes"),
            dict(STATUS, two_step_required=1),
            dict(STATUS, two_step_required=None),
            [STATUS],
        ):
            self.status = support.answer(200, status)
            self.assertFalse(plugin.two_step_required(self.server.url), status)

    def test_a_server_that_doesnt_answer_requires_nothing(self):
        # A refusal, a page that isn't Pley's, nothing there.
        for answer in (support.answer(404, {"code": "not_found"}), (200, {"Content-Type": "text/html"}, b"<html></html>")):
            self.status = answer
            self.assertFalse(plugin.two_step_required(self.server.url), answer)
        self.server.close()
        self.assertFalse(plugin.two_step_required(self.server.url))

    def test_the_code_waits_for_nothing(self):
        # The server is slow to say: the code shows, and is refused, without it.
        self.answering.clear()
        screens, told = [], threading.Event()

        class Screen(signin.SignIn):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, **kwargs)
                screens.append(self)

            def show_two_step(self):
                super().show_two_step()
                told.set()

        with mock.patch.object(signin, "SignIn", Screen):
            plugin.Plugin(["plugin://plugin.video.pley/", "-1", ""]).sign_in(self.server.url, "Den", "abc")
        screen = screens[0]
        self.assertEqual(screen.code, "BCDF-GHJK")
        self.assertEqual(xbmcgui.boxes[-1], ("Pley", kodi.text("code_refused")))
        self.assertFalse(screen.two_step)
        # Then it says so.
        self.answering.set()
        self.assertTrue(told.wait(10))
        self.assertTrue(screen.two_step)


if __name__ == "__main__":
    unittest.main()
