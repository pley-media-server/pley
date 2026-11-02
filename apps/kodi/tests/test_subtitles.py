import json
import os
import shutil
import tempfile
import unittest
from unittest import mock

import support
import xbmcaddon
import xbmcgui
import xbmcplugin
from pley import api, kodi, plugin, subtitles, tracks

RESULTS = [
    {"id": "opensubtitles:1", "provider": "opensubtitles", "language": "en", "name": "Saltwind.1995.1080p.BluRay.x264", "format": "srt",
     "hearing_impaired": False, "forced": False, "downloads": 12, "matched": "hash"},
    {"id": "opensubtitles:2", "provider": "opensubtitles", "language": "pt-BR", "name": "Saltwind.1995.WEB", "format": "srt",
     "hearing_impaired": True, "forced": False, "downloads": 2500, "matched": "title"},
]
TRACK = {"index": 10001, "codec": "subrip", "language": "eng", "title": None, "default": False, "forced": False, "external": True,
         "text_based": True, "added": True, "url": "/api/files/7/subtitles/10001.vtt"}
VTT = b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n"
PLAYING = {"item_id": 9, "file_id": 7, "session_id": "s1", "tracks": {}}


class SubtitlesTest(unittest.TestCase):
    """Find subtitles from Kodi's player, pley as a subtitle service."""

    def test_only_what_pley_plays(self):
        self.assertEqual(subtitles.playing(json.dumps(PLAYING))["item_id"], 9)
        self.assertIsNone(subtitles.playing(""), "Kodi plays something else")
        self.assertIsNone(subtitles.playing(json.dumps({"item_id": 9, "session_id": "s1"})), "music has no tracks")
        self.assertIsNone(subtitles.playing("{"))

    def test_results_as_kodis_dialog_lists_them(self):
        found = subtitles.entries(RESULTS, "plugin://plugin.video.pley/", 9, 7, {"en": "English", "pt": "Portuguese"}.get)
        url, label, label2, art, props = found[0]
        self.assertEqual(url, "plugin://plugin.video.pley/?action=download&result=opensubtitles%3A1&item=9&file=7")
        self.assertEqual((label, label2), ("English", "Saltwind.1995.1080p.BluRay.x264"))
        self.assertEqual(art, {"icon": "5", "thumb": "en"}, "made for this very file: five stars")
        self.assertEqual(props, {"sync": "true", "hearing_imp": "false"})
        _, label, _, art, props = found[1]
        self.assertEqual((label, art, props), ("Portuguese", {"icon": "4", "thumb": "pt"}, {"sync": "false", "hearing_imp": "true"}))
        self.assertEqual(subtitles.language_name("xx", lambda code: ""), "xx", "a code Kodi doesn't know")
        self.assertNotIn("file", subtitles.entries(RESULTS, "p://", 9, None)[0][0])

    def test_what_is_said_when_it_cant_look(self):
        text = kodi.text
        self.assertEqual(subtitles.problem(api.ApiError(503, "unavailable", "no key"), text),
                         "Subtitle search isn't set up on this server. An administrator sets it up on the web, in Settings → Server.")
        self.assertEqual(subtitles.problem(api.ApiError(404, "not_found", "nothing at"), text), "Finding subtitles needs a newer Pley server.")
        self.assertIn("busy", subtitles.problem(api.ApiError(502, "upstream", "busy"), text))


class RouteTest(unittest.TestCase):
    """The plugin's part in Kodi's subtitle dialog, against a stand-in server."""

    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root, True)
        patch = mock.patch.object(kodi, "temp_dir", return_value=self.root)
        patch.start()
        self.addCleanup(patch.stop)
        self.search = (200, {"results": RESULTS})
        self.server = support.FakeServer(self.handle)
        self.addCleanup(self.server.close)
        kodi.set_property(kodi.PROXY, self.server.url + "/s3cret")
        kodi.set_property(kodi.PLAYING, json.dumps(PLAYING))
        self.addCleanup(kodi.clear_property, kodi.PROXY)
        self.addCleanup(kodi.clear_property, kodi.PLAYING)
        self.addCleanup(kodi.clear_property, subtitles.ADDED)
        del xbmcplugin.calls[:]
        del xbmcgui.notified[:]

    def handle(self, request):
        if request.path.startswith("/s3cret/api/items/9/subtitles/search"):
            return support.answer(*self.search)
        if request.path == "/s3cret/api/items/9/subtitles" and request.method == "POST":
            return support.answer(201, TRACK)
        if request.path == "/s3cret/api/files/7/subtitles/10001.vtt":
            return 200, {"Content-Type": "text/vtt"}, VTT
        if request.path == "/s3cret/api/items/9/streams":
            return support.answer(200, {})
        return support.answer(404, {"code": "not_found", "message": request.path})

    def run_plugin(self, query):
        plugin.Plugin(["plugin://plugin.video.pley/", "3", "?" + query]).run()
        return [c for c in xbmcplugin.calls if c[0] == "item"], xbmcplugin.calls[-1]

    def test_the_results_for_the_title_playing(self):
        items, last = self.run_plugin("action=search&languages=English&preferredlanguage=English")
        self.assertEqual(last, ("end", True))
        self.assertEqual([i[2].label2 for i in items], ["Saltwind.1995.1080p.BluRay.x264", "Saltwind.1995.WEB"])
        self.assertEqual(self.server.requests[0].path, "/s3cret/api/items/9/subtitles/search?file_id=7", "the profile's language, the server's choice")

    def test_nothing_for_what_pley_doesnt_play(self):
        kodi.clear_property(kodi.PLAYING)
        items, last = self.run_plugin("action=search&languages=English")
        self.assertEqual((items, last, self.server.requests), ([], ("end", True), []))

    def test_search_not_set_up_on_the_server(self):
        self.search = (503, {"code": "unavailable", "message": "OpenSubtitles isn't set up", "setup": False})
        items, last = self.run_plugin("action=search")
        self.assertEqual((items, last), ([], ("end", True)))
        self.assertEqual(xbmcgui.notified, [("Pley", "Subtitle search isn't set up on this server. An administrator sets it up on the web, in Settings → Server.")])

    def test_the_one_picked_is_added_saved_and_handed_to_kodi(self):
        items, last = self.run_plugin("action=download&result=opensubtitles%3A1&item=9&file=7")
        path = os.path.join(self.root, "pley-subtitles", "10001.eng.vtt")
        self.assertEqual([i[1] for i in items], [path])
        with open(path, "rb") as f:
            self.assertEqual(f.read(), VTT)
        sent = [(r.method, r.path, r.json() if r.body else None) for r in self.server.requests]
        self.assertEqual(sent[0], ("POST", "/s3cret/api/items/9/subtitles", {"result": "opensubtitles:1", "file_id": 7}))
        self.assertEqual(sent[-1], ("PUT", "/s3cret/api/items/9/streams", {"file_id": 7, "subtitle_index": 10001}))
        self.assertEqual(kodi.get_property(subtitles.ADDED), "s1", "the service stops following Kodi's places")


class FollowerTest(unittest.TestCase):
    def test_a_follower_that_stopped_following_saves_nothing(self):
        saved = []
        layout = {"hls": False, "file_id": 7, "audio": [], "subtitles": [{"index": 3, "language": "eng", "forced": False}],
                  "audio_index": None, "known": True, "subtitle_index": None, "burned": None}
        follower = tracks.Follower(layout, lambda method, params: None, saved.append)
        follower.following = False
        follower._turn({"subtitles": [{"index": 0}], "currentsubtitle": {"index": 1}, "subtitleenabled": True})
        self.assertEqual((follower.shown, saved), (False, []))


if __name__ == "__main__":
    unittest.main()
