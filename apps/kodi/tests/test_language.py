"""The add-on's languages: its strings.po files, which language it shows
(packages/tv/test/fixtures/language.json, as the Samsung apps do), its own
lookup of the words, and the profile's own language."""

import collections
import glob
import json
import os
import re
import shutil
import tempfile
import unittest
from unittest import mock
import xml.etree.ElementTree as ET

import support
import xbmc
import xbmcaddon
import xbmcgui
from pley import api, i18n, kodi, language, livetv, words
from pley import layout as L

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.join(HERE, "..", "plugin.video.pley")
FIXTURE = os.path.join(HERE, "..", "..", "..", "packages", "tv", "test", "fixtures", "language.json")
# A word's values: %s and %d (%% is a percent sign), and {names}.
VALUES = re.compile(r"%[sd]|\{[a-z]+\}")


def catalog(language_):
    with open(os.path.join(i18n.ROOT, i18n.FOLDERS[language_], "strings.po"), encoding="utf-8") as f:
        return i18n.parse(f.read(), english=language_ == i18n.ENGLISH)


class CatalogTest(unittest.TestCase):
    """Every language has every word, with the same values in it."""

    def test_every_language_has_every_word_with_the_same_values(self):
        english = catalog("en")
        self.assertGreater(len(english), 400)
        for language_ in ("fr", "es"):
            found = catalog(language_)
            self.assertEqual(sorted(found), sorted(english), language_)
            for id_, text in found.items():
                self.assertTrue(text.strip(), "%s #%d is empty" % (language_, id_))
                self.assertEqual(
                    sorted(VALUES.findall(text)), sorted(VALUES.findall(english[id_])), "%s #%d: %s" % (language_, id_, text)
                )

    def test_the_files_keep_kodis_layout(self):
        # Kodi reads them too (the settings): the same entries, in the same
        # order, each msgid the English words.
        def entries(language_):
            with open(os.path.join(i18n.ROOT, i18n.FOLDERS[language_], "strings.po"), encoding="utf-8") as f:
                return re.findall(r'msgctxt "(#\d+)"\nmsgid "(.*)"', f.read())

        english = entries("en")
        for language_ in ("fr", "es"):
            self.assertEqual(entries(language_), english, language_)
        with open(os.path.join(i18n.ROOT, i18n.FOLDERS["en"], "strings.po"), encoding="utf-8") as f:
            self.assertNotRegex(f.read(), r'msgstr "[^"]', "English's words are its msgids")

    def test_every_name_the_code_has_is_a_word(self):
        english = catalog("en")
        for name, id_ in kodi.TEXT.items():
            self.assertIn(id_, english, name)
        self.assertEqual(len(set(kodi.TEXT.values())), len(kodi.TEXT))
        for name, first in kodi.COUNTS.items():
            for form in range(len(i18n.FORMS)):
                self.assertIn(first + form, english, "%s, %s" % (name, i18n.FORMS[form]))
                self.assertNotIn(first + form, kodi.TEXT.values(), name)
        for id_ in kodi.MONTHS + kodi.WEEKDAYS + kodi.BYTES + tuple(kodi.LANGUAGE_NAMES.values()) + L.WORDS:
            self.assertIn(id_, english)
        self.assertEqual(len(kodi.MONTHS), 12)
        self.assertEqual(len(kodi.WEEKDAYS), 7)

    def test_every_word_the_sources_name_is_there(self):
        # kodi.text("name"), T("name"), text("name"), and the counted ones.
        named = re.compile(r'\b(?:T|text|kodi\.text)\(\s*"([a-z0-9_]+)"(?=\s*[),])')
        counted = re.compile(r'\bkodi\.(?:count|counted)\(\s*"([a-z0-9_]+)"(?=\s*[),])')
        missing, found = [], 0
        for path in glob.glob(os.path.join(ADDON, "resources", "lib", "pley", "*.py")):
            with open(path, encoding="utf-8") as f:
                source = f.read()
            for name in named.findall(source):
                found += 1
                if name not in kodi.TEXT:
                    missing.append("%s: %s" % (os.path.basename(path), name))
            for name in counted.findall(source):
                found += 1
                if name not in kodi.COUNTS:
                    missing.append("%s: %s" % (os.path.basename(path), name))
        self.assertGreater(found, 300, "the sources weren't found")
        self.assertEqual(missing, [])

    def test_the_settings_labels_are_words(self):
        english = catalog("en")
        tree = ET.parse(os.path.join(ADDON, "resources", "settings.xml"))
        labels = [e.get("label") for e in tree.iter() if e.get("label")] + [e.text for e in tree.iter("heading")]
        self.assertGreater(len(labels), 20)
        for label in labels:
            self.assertIn(int(label), english, label)

    def test_the_windows_draw_their_words_from_the_language_shown(self):
        # Kodi's $ADDON[] would be in Kodi's language: each word is a Home
        # property the add-on fills (kodi.show_words).
        drawn = set()
        for path in glob.glob(os.path.join(ADDON, "resources", "skins", "default", "1080i", "*.xml")):
            with open(path, encoding="utf-8") as f:
                text = f.read()
            self.assertNotIn("$ADDON[", text, path)
            self.assertNotIn("$LOCALIZE[", text, path)
            drawn |= {int(n) for n in re.findall(r"Window\(Home\)\.Property\(pley\.word\.(\d+)\)", text)}
        self.assertEqual(drawn, set(L.WORDS))

    def test_french_has_no_break_spaces_before_its_tall_marks_and_curly_apostrophes(self):
        for id_, text in catalog("fr").items():
            self.assertNotIn("'", text, "fr #%d has a straight apostrophe (’): %s" % (id_, text))
            self.assertNotIn("...", text, "fr #%d has three dots (…): %s" % (id_, text))
            # A no-break space (U+00A0) before : ; ? ! » and after «, never an ordinary one or none.
            self.assertIsNone(re.search(r" [:;?!»]|« ", text), "fr #%d has an ordinary space: %s" % (id_, text))
            self.assertIsNone(re.search(r"[\w}][;?!»]|[\w}]:(\s|$)|«[\w{%]", text), "fr #%d needs a no-break space: %s" % (id_, text))

    def test_spanish_opens_its_questions_and_exclamations(self):
        for id_, text in catalog("es").items():
            self.assertNotIn("'", text, "es #%d has a straight apostrophe (’): %s" % (id_, text))
            self.assertNotIn("...", text, "es #%d has three dots (…): %s" % (id_, text))
            self.assertLessEqual(text.count("?"), text.count("¿"), "es #%d: %s" % (id_, text))
            self.assertLessEqual(text.count("!"), text.count("¡"), "es #%d: %s" % (id_, text))


class ParseTest(unittest.TestCase):
    def test_a_po_file_as_kodi_reads_it(self):
        text = "\n".join([
            "# A comment",
            'msgid ""',
            'msgstr ""',
            '"Language: fr_FR\\n"',
            "",
            "#. one",
            'msgctxt "#30001"',
            'msgid "Sign in to %s"',
            'msgstr "Se connecter à %s"',
            "",
            'msgctxt "#30002"',
            'msgid ""',
            '"Two "',
            '"lines"',
            'msgstr ""',
            '"Deux "',
            '"lignes \\"citées\\""',
            "",
            'msgctxt "#30003"',
            'msgid "Not written yet"',
            'msgstr ""',
        ])
        self.assertEqual(i18n.parse(text), {30001: "Se connecter à %s", 30002: 'Deux lignes "citées"'})
        self.assertEqual(i18n.parse(text, english=True), {30001: "Sign in to %s", 30002: "Two lines", 30003: "Not written yet"})

    def test_a_word_a_language_lacks_is_in_english(self):
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root)
        for language_, body in (("en", 'msgid "Hello"\nmsgstr ""\n'), ("fr", 'msgid "Hello"\nmsgstr ""\n')):
            folder = os.path.join(root, i18n.FOLDERS[language_])
            os.makedirs(folder)
            with open(os.path.join(folder, "strings.po"), "w", encoding="utf-8") as f:
                f.write('msgctxt "#30001"\n' + body)
        found = i18n.Words("fr", root)
        self.assertEqual(found.text(30001), "Hello")
        self.assertEqual(found.text(39999), "")
        self.assertEqual(i18n.Words("de", root).language, "en")


class PickTest(unittest.TestCase):
    """Which language the add-on shows: the account's, else the one chosen
    in its settings, else Kodi's, else the server's, else English, as the
    Samsung apps pick it (the shared fixture)."""

    @classmethod
    def setUpClass(cls):
        with open(FIXTURE, encoding="utf-8") as f:
            cls.fixture = json.load(f)

    def test_picks_the_language_as_the_samsung_apps_do(self):
        for case in self.fixture["pickLanguage"]:
            with self.subTest(case=case):
                self.assertEqual(i18n.pick(case["account"], case["server"], case["device"]), case["out"])

    def test_the_one_chosen_here_comes_after_the_accounts(self):
        for case in self.fixture["pickLanguageWithChoice"]:
            with self.subTest(case=case):
                self.assertEqual(i18n.pick(case["account"], case["server"], case["device"], case["choice"]), case["out"])

    def test_tags_match_by_their_language(self):
        self.assertEqual(i18n.supported("fr-CA"), "fr")
        self.assertEqual(i18n.supported("ES_mx"), "es")
        self.assertEqual(i18n.supported("en_US.UTF-8"), "en")
        self.assertEqual(i18n.supported("fr"), "fr")
        self.assertIsNone(i18n.supported("de-DE"))
        self.assertIsNone(i18n.supported("auto"))
        self.assertIsNone(i18n.supported(""))
        self.assertIsNone(i18n.supported(None))


class FormsTest(unittest.TestCase):
    def test_plural_forms_as_cldr_has_them(self):
        self.assertEqual([i18n.plural_form("en", n) for n in (0, 1, 1.5, 2, 1_000_000)], ["other", "one", "other", "other", "other"])
        self.assertEqual([i18n.plural_form("fr", n) for n in (0, 1, 1.5, 2, 1_000_000, 2_000_000, 1_000_001)],
                         ["one", "one", "one", "other", "many", "many", "other"])
        self.assertEqual([i18n.plural_form("es", n) for n in (0, 1, 1.5, 2, 1_000_000, 1_000_001)],
                         ["other", "one", "other", "other", "many", "other"])

    def test_numbers_as_each_language_writes_them(self):
        self.assertEqual(i18n.number(12345, "en"), "12,345")
        self.assertEqual(i18n.number(12345, "fr"), "12 345")
        self.assertEqual(i18n.number(1234, "fr"), "1 234")
        self.assertEqual(i18n.number(12345, "es"), "12.345")
        self.assertEqual(i18n.number(1234, "es"), "1234", "Spanish groups from five digits")
        self.assertEqual(i18n.decimal(7.43, 1, "en"), "7.4")
        self.assertEqual(i18n.decimal(7.43, 1, "fr"), "7,4")
        self.assertEqual(i18n.decimal(7.43, 1, "es"), "7,4")


class Shown(unittest.TestCase):
    """The add-on in a language: Kodi's, the setting, the account's and the
    server's as the test sets them; English again after."""

    def setUp(self):
        self.profile = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.profile)
        xbmcaddon.settings["_profile"] = self.profile
        self.addCleanup(xbmcaddon.settings.pop, "_profile", None)
        self.addCleanup(xbmcaddon.settings.pop, "language", None)
        self.addCleanup(setattr, xbmc, "language", xbmc.language)
        self.addCleanup(kodi.begin)
        kodi.begin()

    def show(self, account=None, choice=None, kodi_language="en", server=None):
        kodi.heard_account({"language": account})
        kodi.heard_server({"language": server})
        xbmcaddon.settings["language"] = choice or "auto"
        xbmc.language = kodi_language
        kodi.forget_language()
        return kodi.language()


class LanguageTest(Shown):
    def test_the_order_the_add_on_follows(self):
        self.assertEqual(self.show(), "en")
        self.assertEqual(self.show(kodi_language="fr"), "fr", "Kodi's own")
        self.assertEqual(self.show(kodi_language="de", server="es"), "es", "Kodi in a language Pley doesn't have: the server's")
        self.assertEqual(self.show(kodi_language="de"), "en")
        self.assertEqual(self.show(choice="es", kodi_language="fr"), "es", "the setting before Kodi's")
        self.assertEqual(self.show(account="fr", choice="es"), "fr", "the account's before all")
        self.assertEqual(self.show(account="de", choice="es"), "es", "an account's Pley isn't in counts as none")
        self.assertEqual(kodi.remembered(kodi.ACCOUNT_LANGUAGE), "")

    def test_forgotten_on_another_thread_as_it_is_found(self):
        class Forgetful(dict):
            # Another thread's forget_language() right after each lookup keeps
            # what it found.
            def __setitem__(self, key, value):
                dict.__setitem__(self, key, value)
                self.pop(key, None)

        self.show(account="fr")
        with mock.patch.object(kodi, "_call", Forgetful()):
            self.assertEqual(kodi.language(), "fr")
            self.assertEqual(kodi.text("search"), kodi.text("search"))
            self.assertIsNotNone(kodi.addon())

    def test_signed_out_the_device_chooses(self):
        self.show(account="fr")
        kodi.heard_account(None)
        kodi.forget_language()
        self.assertEqual(kodi.language(), "en")

    def test_words_counts_and_numbers_in_the_language_shown(self):
        self.show(account="fr")
        self.assertEqual(kodi.text("sign_out"), "Se déconnecter")
        self.assertEqual(kodi.count("movies", 1), "1 film")
        self.assertEqual(kodi.count("movies", 0), "0 film", "French counts 0 as one")
        self.assertEqual(kodi.count("movies", 12345), "12 345 films")
        self.assertEqual(kodi.count("movies", 2_000_000), "2 000 000 de films")
        self.assertEqual(words.runtime(112 * 60_000), "1 h 52 min")
        self.assertEqual(words.time_left(30 * 60_000, 56 * 60_000), "Reste 26 min")
        self.assertEqual(words.date("2024-03-05"), "5 mars 2024")
        self.assertEqual(words.rating(7.43), "7,4")
        self.assertEqual(words.size(24_000_000_000), "22,4 Go")
        self.assertEqual(words.stars(3), "1½ étoile")
        self.assertEqual(words.episode_label(1, 3), "S1 · É3")
        self.assertEqual(words.seasons_label([0, 1, 2]), "Saisons 1–2 et épisodes spéciaux")
        self.show(account="es")
        self.assertEqual(kodi.count("movies", 0), "0 películas")
        self.assertEqual(kodi.count("movies", 1), "1 película")
        self.assertEqual(words.episode_label(1, 3), "T1 · E3")
        self.assertEqual(words.date("2024-03-05"), "5 mar 2024")
        self.show()
        self.assertEqual(kodi.count("movies", 0), "0 movies")
        self.assertEqual(words.date("2024-03-05"), "Mar 5, 2024")

    def test_the_windows_words_change_with_it(self):
        self.addCleanup(xbmcgui.Window.properties.clear)
        xbmcgui.Window.properties.clear()
        self.show()
        told = "NotifyAll(%s,%s)" % (kodi.ADDON_ID, kodi.LANGUAGE_CHANGED)
        before = len(xbmc.builtins)
        self.assertTrue(kodi.follow_language())
        self.assertEqual(xbmcgui.Window.properties["pley.word.30003"], "Search")
        self.assertEqual(xbmcgui.Window.properties[kodi.SHOWN_LANGUAGE], "en")
        self.assertEqual(set(k for k in xbmcgui.Window.properties if k.startswith("pley.word.")), {"pley.word.%d" % i for i in L.WORDS})
        # The first time says nothing: no part of the add-on drew words yet.
        self.assertNotIn(told, xbmc.builtins[before:])
        self.assertFalse(kodi.follow_language(), "the same language")
        kodi.heard_account({"language": "es"})
        self.assertTrue(kodi.follow_language())
        self.assertEqual(xbmcgui.Window.properties["pley.word.30003"], "Buscar")
        self.assertIn(told, xbmc.builtins[before:])

    def test_the_service_draws_the_windows_words_again_as_it_starts(self):
        # Kodi still running, the add-on updated: Home's properties hold the
        # version before's words, in the language shown now.
        self.addCleanup(xbmcgui.Window.properties.clear)
        xbmcgui.Window.properties.clear()
        self.show()
        xbmcgui.Window.properties[kodi.SHOWN_LANGUAGE] = "en"
        xbmcgui.Window.properties["pley.word.30003"] = "Look for"
        kodi.show_language_at_start()
        self.assertEqual(xbmcgui.Window.properties["pley.word.30003"], "Search")
        self.assertEqual(set(k for k in xbmcgui.Window.properties if k.startswith("pley.word.")), {"pley.word.%d" % i for i in L.WORDS})

    def test_requests_say_the_language_shown(self):
        self.show(account="fr")
        server = support.FakeServer(lambda request: support.answer(200, {}))
        self.addCleanup(server.close)
        api.Client(server.url, "pleyKodi/test").get("/api/hubs/home")
        self.assertEqual(server.requests[0].headers.get("accept-language"), "fr")

    def test_the_guide_keeps_kodis_clock_unless_kodi_is_in_another_of_pleys_languages(self):
        self.addCleanup(xbmc.region.update, {"time": xbmc.region["time"]})
        xbmc.region["time"] = "%H:%M:%S"
        self.show(kodi_language="de")
        self.assertEqual(livetv.time_format(), "%H:%M", "Kodi in German, the add-on in English: Kodi's region")
        self.show(kodi_language="fr", choice="en")
        self.assertEqual(livetv.time_format(), "%I:%M %p", "Kodi in French, the add-on in English: English's clock")
        xbmc.region["time"] = "%I:%M:%S %p"
        self.show(kodi_language="en", choice="fr")
        self.assertEqual(livetv.time_format(), "%H:%M", "Kodi in English, the add-on in French: French's clock")
        self.show(kodi_language="de", server="es")
        self.assertEqual(livetv.time_format(), "%I:%M %p", "Kodi in German, the add-on in Spanish: Kodi's region")


class ProfileLanguageTest(Shown):
    """Profile language: the profile's own, on all its devices."""

    class Client:
        def __init__(self, error=None):
            self.puts = []
            self.error = error

        def put(self, path, body):
            self.puts.append((path, body))
            if self.error:
                raise self.error
            return {"id": 1, "name": "Ann", "language": body["language"]}

    def choose(self, client, picked, me=None):
        asked, said = [], []

        def select(heading, labels, preselect):
            asked.append((heading, labels, preselect))
            return picked

        found = language.choose(client, me or {"id": 1, "language": None}, select, lambda words_, error: said.append((words_, error)))
        return found, asked, said

    def test_automatic_says_what_it_is_here_then_each_language_in_itself(self):
        self.show(kodi_language="fr")
        self.assertEqual([label for _, label in language.choices()], ["Automatique (Français)", "English", "Français", "Español"])

    def test_a_choice_is_saved_and_shown(self):
        self.show()
        xbmcgui.Window.properties[kodi.SHOWN_LANGUAGE] = "en"
        self.addCleanup(xbmcgui.Window.properties.clear)
        client = self.Client()
        found, asked, _ = self.choose(client, 2)
        self.assertEqual(asked[0][0], "Profile language")
        self.assertEqual(asked[0][2], 0, "Automatic is the profile's now")
        self.assertEqual(client.puts, [("/api/me/language", {"language": "fr"})])
        self.assertEqual(found["language"], "fr")
        self.assertEqual(kodi.language(), "fr")
        self.assertEqual(kodi.text("settings"), "Paramètres")
        # Automatic again: null, the server's.
        found, asked, _ = self.choose(client, 0, me=found)
        self.assertEqual(asked[0][2], 2)
        self.assertEqual(client.puts[-1], ("/api/me/language", {"language": None}))
        self.assertEqual(kodi.language(), "en")

    def test_the_one_in_force_changes_nothing(self):
        client = self.Client()
        self.assertIsNone(self.choose(client, 1, me={"language": "en"})[0])
        self.assertIsNone(self.choose(client, -1)[0])
        self.assertEqual(client.puts, [])

    def test_on_a_pley_from_before_languages_the_choice_is_this_devices(self):
        # Its GET /api/server names no language, and it keeps no account's.
        self.show(kodi_language="fr")
        kodi.heard_server({"id": "s", "name": "Old"})
        self.assertFalse(kodi.keeps_account_language())
        client = self.Client(api.ApiError(404, "not_found", "no"))
        self.assertEqual([label for _, label in language.choices(on_device=True)][0], "Automatique (Français)")
        found, asked, said = self.choose(client, 3)
        self.assertEqual(asked[0][2], 0, "this device's Automatic")
        self.assertEqual(client.puts, [], "nothing asked of the server")
        self.assertEqual(said, [])
        self.assertIsNotNone(found)
        self.assertEqual(xbmcaddon.settings["language"], "es", "kept on this device")
        self.assertEqual(kodi.language(), "es", "shown at once")
        # The one kept is the one in force; Automatic forgets it.
        _, asked, _ = self.choose(client, 0)
        self.assertEqual(asked[0][2], 3)
        self.assertEqual(xbmcaddon.settings["language"], "auto")
        self.assertEqual(kodi.language(), "fr")
        # A server with languages again: the account's.
        kodi.heard_server({"id": "s", "language": "en"})
        self.assertTrue(kodi.keeps_account_language())

    def test_a_choice_answered_404_is_this_devices(self):
        # The server named a language, yet keeps no account's.
        self.show()
        client = self.Client(api.ApiError(404, "not_found", "no"))
        found, _, said = self.choose(client, 2)
        self.assertEqual(client.puts, [("/api/me/language", {"language": "fr"})])
        self.assertEqual(said, [], "no error: the choice holds")
        self.assertIsNotNone(found)
        self.assertEqual(xbmcaddon.settings["language"], "fr")
        self.assertEqual(kodi.language(), "fr")
        self.assertFalse(kodi.keeps_account_language())
        # The next choice stays on this device.
        self.choose(client, 3)
        self.assertEqual(len(client.puts), 1)
        self.assertEqual(kodi.language(), "es")

    def test_a_404_holds_until_another_server_answers(self):
        # The stream opening again asks GET /api/server again: the same
        # server still keeps no account's language, and Automatic forgets
        # this device's choice.
        self.show()
        kodi.heard_server({"id": "s", "version": "0.2.0", "language": "en"})
        client = self.Client(api.ApiError(404, "not_found", "no"))
        self.choose(client, 2)
        self.assertEqual(kodi.language(), "fr")
        kodi.heard_server({"id": "s", "version": "0.2.0", "language": "en"})
        self.assertFalse(kodi.keeps_account_language())
        _, asked, _ = self.choose(client, 0)
        self.assertEqual(asked[0][2], 2, "this device's choice, checked")
        self.assertEqual(len(client.puts), 1, "nothing asked of the server")
        self.assertEqual(xbmcaddon.settings["language"], "auto")
        self.assertEqual(kodi.language(), "en")
        # The server updated, or another one: it is asked again.
        kodi.heard_server({"id": "s", "version": "0.3.0", "language": "en"})
        self.assertTrue(kodi.keeps_account_language())

    def test_a_later_pley_in_a_language_the_add_on_hasnt_keeps_the_accounts(self):
        self.show()
        kodi.heard_server({"id": "s", "version": "0.4.0", "language": "de"})
        self.assertTrue(kodi.keeps_account_language())
        kodi.heard_server({"id": "s", "version": "0.4.0"})
        self.assertFalse(kodi.keeps_account_language(), "one that names none")

    def test_a_choice_answered_405_is_this_devices_too(self):
        self.show()
        client = self.Client(api.ApiError(405, "method_not_allowed", "no"))
        found, _, said = self.choose(client, 2)
        self.assertEqual(said, [], "no error: the choice holds")
        self.assertIsNotNone(found)
        self.assertEqual(xbmcaddon.settings["language"], "fr")
        self.assertEqual(kodi.language(), "fr")
        self.assertFalse(kodi.keeps_account_language())

    def test_a_403_is_said_and_isnt_an_older_server(self):
        self.show()
        found, _, said = self.choose(self.Client(api.ApiError(403, "forbidden", "not allowed")), 2)
        self.assertIsNone(found)
        self.assertEqual(said, [(kodi.text("server_error") % "not allowed", True)])
        self.assertEqual(kodi.language(), "en")
        self.assertTrue(kodi.keeps_account_language())

    def test_another_refusal_is_said_and_changes_nothing(self):
        self.show()
        found, _, said = self.choose(self.Client(api.ApiError(500, "internal", "it broke")), 2)
        self.assertIsNone(found)
        self.assertEqual(said, [(kodi.text("server_error") % "it broke", True)])
        self.assertEqual(kodi.language(), "en")
        self.assertTrue(kodi.keeps_account_language())

    def test_the_service_hears_the_accounts_and_the_servers(self):
        answers = {"/api/auth/me": {"id": 1, "language": None}, "/api/server": {"id": "s", "language": "es"}}

        class Response:
            def __init__(self, body):
                self.status, self.body = 200, body

            def json(self):
                return self.body

        self.show(kodi_language="de")
        language.hear(lambda method, path: Response(answers[path]))
        self.assertEqual(kodi.language(), "es", "Kodi in German: the server's")
        answers["/api/auth/me"]["language"] = "fr"
        language.hear(lambda method, path: Response(answers[path]))
        self.assertEqual(kodi.language(), "fr")
        # Out of reach: what was heard stays.
        language.hear(lambda method, path: None)
        self.assertEqual(kodi.language(), "fr")


if __name__ == "__main__":
    unittest.main()
