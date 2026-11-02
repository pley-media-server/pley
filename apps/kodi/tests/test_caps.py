import unittest

import support  # noqa: F401
from pley import caps

AUDIO_ON = {
    "audiooutput.passthrough": True,
    "audiooutput.ac3passthrough": True,
    "audiooutput.eac3passthrough": True,
    "audiooutput.dtspassthrough": False,
    "audiooutput.truehdpassthrough": True,
    "audiooutput.channels": 8,
}


class CapsTest(unittest.TestCase):
    def test_a_windows_kodi_on_an_sdr_screen(self):
        report = caps.build("", {}, (1920, 1080), "Kodi 21.3 on Windows")
        self.assertEqual(report["caps_version"], 2)
        # Kodi plays HDR10 and HLG anywhere, tone-mapping them itself.
        self.assertEqual(report["hdr_types"], ["hdr10", "hdr10plus", "hlg"])
        self.assertFalse(report["dolby_vision"])
        self.assertNotIn("dolby_vision_profiles", report)
        self.assertEqual(report["audio_passthrough"], [])
        self.assertEqual(report["audio_channels"], 2)
        self.assertTrue(report["audio_track_switching"])
        self.assertIn("hdmv_pgs_subtitle", report["subtitle_codecs"])
        # Every picture codec shown from the file, PGS alone beside a
        # stream (as a .sup file): VobSub and DVB are drawn onto it.
        self.assertIn("dvd_subtitle", report["subtitle_codecs"])
        self.assertIn("dvb_subtitle", report["subtitle_codecs"])
        self.assertTrue(report["picture_subtitles_sideload"])
        self.assertEqual(report["picture_subtitles_sideload_codecs"], ["hdmv_pgs_subtitle"])
        self.assertIn("mkv", report["containers"])
        self.assertEqual(report["display"], {"width": 1920, "height": 1080})
        self.assertEqual(report["device"], {"form": "tv", "model": "Kodi 21.3 on Windows"})
        self.assertEqual(report["hls"], {"master_playlist": False, "segments": ["ts"]})

    def test_passthrough_follows_kodis_audio_settings(self):
        report = caps.build("", AUDIO_ON, (0, 0), "Kodi")
        self.assertEqual(report["audio_passthrough"], ["ac3", "eac3", "truehd"])
        self.assertEqual(report["audio_channels"], 6)
        self.assertNotIn("display", report)
        off = dict(AUDIO_ON, **{"audiooutput.passthrough": False})
        self.assertEqual(caps.build("", off, (0, 0), "Kodi")["audio_passthrough"], [])

    def test_dolby_vision_where_the_screen_takes_it(self):
        android = caps.build("HDR10, HLG, Dolby Vision", {}, (3840, 2160), "Kodi 21.3 on Android")
        self.assertIn("dolby_vision", android["hdr_types"])
        self.assertTrue(android["dolby_vision"])
        self.assertEqual([p["profile"] for p in android["dolby_vision_profiles"]], [5, 8])
        coreelec = caps.build("HDR10, Dolby Vision", {}, (3840, 2160), "Kodi 21.3 on Linux", coreelec=True)
        self.assertEqual([p["profile"] for p in coreelec["dolby_vision_profiles"]], [5, 7, 8])
        # Dolby Vision with HDR10+ beside it: CoreELEC shows it, Android's decoders often don't.
        self.assertTrue(coreelec["dolby_vision_hdr10plus"])
        self.assertNotIn("dolby_vision_hdr10plus", android)


if __name__ == "__main__":
    unittest.main()
