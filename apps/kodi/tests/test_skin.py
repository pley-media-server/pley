import collections
import os
import sys
import unittest
import xml.etree.ElementTree as ET

import support  # noqa: F401
from pley import layout as L

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
import skin  # noqa: E402

MEDIA = os.path.join(HERE, "..", "plugin.video.pley", "resources", "skins", "default", "media")
# Estuary's fonts (Kodi's default skin, LibreELEC's and CoreELEC's).
ESTUARY_FONTS = {
    "font10", "font12", "font13", "font14", "font27", "font32", "font37", "font45", "font60",
    "font20_title", "font25_title", "font30_title", "font32_title", "font36_title", "font40_title",
    "font45_title", "font52_title", "font_MainMenu", "Mono26",
}


def parse(name):
    return ET.parse(os.path.join(skin.SKIN, name)).getroot()


class SkinTest(unittest.TestCase):
    def test_the_windows_are_what_skin_py_writes(self):
        for path, text in skin.windows().items():
            with open(path, encoding="utf-8") as f:
                self.assertEqual(f.read(), text, "%s is out of date: python apps/kodi/skin.py" % path)

    def test_round_buttons_pass_focus_on(self):
        # Kodi moves focus by the focused control's own navigation: the button
        # inside a round button's group has its neighbours', which a grouplist
        # gives only the group.
        controls = {c.get("id"): c for c in parse(L.WINDOW).iter("control") if c.get("id")}

        def moves(control_id):
            return {tag: controls[str(control_id)].findtext(tag) for tag in ("onleft", "onright", "ondown")}

        self.assertEqual(moves(L.ITEM_WATCHED)["onleft"], str(L.button_box(L.ITEM_FROM_START)))
        self.assertEqual(moves(L.ITEM_WATCHED)["onright"], str(L.button_box(L.ITEM_UNWATCHED)))
        self.assertEqual(moves(L.ITEM_WATCHED)["ondown"], str(L.ITEM_SEASONS))
        self.assertEqual(moves(L.ITEM_FROM_START)["onleft"], str(L.button_box(L.ITEM_SHUFFLE)))
        self.assertEqual(moves(L.ITEM_SHUFFLE)["onleft"], str(L.ITEM_PLAY))
        self.assertIsNone(moves(L.ITEM_MORE)["onright"])
        self.assertEqual(moves(L.HOME_INFO)["onright"], str(L.button_box(L.HOME_WATCHED)))

    def test_each_id_once(self):
        for name in (L.WINDOW, L.SIGNIN_WINDOW):
            ids = [c.get("id") for c in parse(name).iter("control") if c.get("id")]
            self.assertEqual([i for i, n in collections.Counter(ids).items() if n > 1], [], name)

    def test_masks_keep_to_a_cropped_pictures_place(self):
        # Kodi stretches a mask over the whole picture by default, cropped
        # parts too: a portrait photo's circle came out a clipped oval.
        masked = [
            c for c in parse(L.WINDOW).iter("control")
            if c.find("texture") is not None and c.find("texture").get("diffuse") and c.findtext("aspectratio") == "scale"
        ]
        self.assertTrue(masked)
        for c in masked:
            self.assertEqual(c.find("aspectratio").get("scalediffuse"), "false", c.find("texture").text)

    def test_the_window_has_every_control_it_moves_and_fills(self):
        ids = {int(c.get("id")) for c in parse(L.WINDOW).iter("control") if c.get("id")}
        wanted = set(L.RAIL_ENTRIES) | set(L.SCREENS.values())
        for entry in L.RAIL_ENTRIES[1:]:
            wanted |= {L.rail_group(entry)} | set(L.rail_icons(entry)) | set(L.rail_labels(entry))
        wanted |= {
            L.BACKDROP, L.HOME_HERO, L.HOME_TITLE, L.HOME_CONTEXT, L.HOME_LEFT, L.HOME_SUMMARY, L.HOME_BUTTONS_BOX,
            L.HOME_PLAY, L.HOME_INFO_BLOCK, L.HOME_INFO_TITLE, L.HOME_INFO_SUMMARY,
            L.ITEM_HERO, L.ITEM_TITLE, L.ITEM_CONTEXT, L.ITEM_LEFT, L.ITEM_GENRES, L.ITEM_SUMMARY, L.ITEM_CREDITS,
            L.ITEM_BUTTONS_BOX, L.ITEM_PLAY, L.ITEM_INFO_BLOCK, L.ITEM_INFO_TITLE, L.ITEM_INFO_EPISODE,
            L.ITEM_INFO_LINE, L.ITEM_INFO_SUMMARY, L.ITEM_SEASONS,
            L.LIBRARY_TITLE, L.LIBRARY_SORT, L.LIBRARY_SORT_DOWN, L.LIBRARY_FILTER, L.LIBRARY_FILTER_ON, L.LIBRARY_UNWATCHED, L.LIBRARY_GENRE,
            L.LIBRARY_YEAR, L.LIBRARY_UNWATCHED_ON, L.LIBRARY_GENRE_ON, L.LIBRARY_YEAR_ON, L.LIBRARY_COUNT, L.LIBRARY_EMPTY, L.SEARCH_FIELD, L.SEARCH_EMPTY_TITLE, L.SEARCH_EMPTY_BODY,
            L.GRID_TITLE, L.GRID_SUBTITLE, L.GRID_EMPTY, L.NOTICE_TITLE, L.NOTICE_BODY, L.NOTICE_FIRST, L.NOTICE_SECOND,
            L.LIBRARIES_TITLE, L.LIBRARIES_BUTTONS, L.LIBRARIES_SORT, L.LIBRARIES_LIST, L.TOAST_LABEL,
        }
        wanted |= {L.rail_pill(entry) for entry in L.RAIL_ENTRIES[1:]}
        for button in (L.HOME_FROM_START, L.HOME_INFO, L.HOME_WATCHED, L.HOME_UNWATCHED, L.ITEM_FROM_START,
                       L.ITEM_WATCHED, L.ITEM_UNWATCHED, L.ITEM_ADD_TO_LIST, L.ITEM_IN_LIST, L.ITEM_TRAILER,
                       L.ITEM_ADD_TO_PLAYLIST, L.ITEM_MORE):
            wanted |= {button, L.button_box(button)}
        for tab in (L.LIBRARY_RECOMMENDED, L.LIBRARY_ALL, L.LIBRARY_COLLECTIONS):
            wanted |= {tab, tab + 100}
        for line in (L.HOME_META, L.HOME_INFO_META, L.ITEM_META, L.ITEM_INFO_META):
            wanted.add(line)
            for i in range(L.META_PIECES):
                wanted |= set(L.meta_piece(line, i))
        for rows, slots, shapes in (
            (L.HOME_ROWS, L.HOME_SLOTS, ("poster", "landscape", "square")),
            (L.ITEM_ROWS, L.ITEM_SLOTS, ("poster", "landscape", "episode", "person")),
            (L.LIBRARY_ROWS, L.LIBRARY_SLOTS, ("poster", "landscape", "square")),
            (L.SEARCH_ROWS, L.SEARCH_SLOTS, ("poster", "episode", "person", "square")),
        ):
            for i in range(slots):
                wanted |= {L.slot(rows, i), L.slot_header(rows, i)} | {L.slot_list(rows, i, s) for s in shapes}
        for grid in (L.LIBRARY_GRID, L.GRID_LISTS):
            wanted |= {L.grid_list(grid, s) for s in L.GRID_SHAPES}
        self.assertEqual(sorted(wanted - ids), [])

    def test_library_posters_draw_their_quality_badges(self):
        from pley import window

        grid = next(c for c in parse(L.WINDOW).iter("control") if c.get("id") == str(L.grid_list(L.LIBRARY_GRID, "grid")))
        poster = L.GRID["grid"]
        for layout in ("itemlayout", "focusedlayout"):
            drawn = [c for c in grid.find(layout).iter("control") if c.get("type") == "label" and "ListItem.Property(range)" in (c.findtext("visible") or "")]
            drawn += [c for c in grid.find(layout).iter("control") if c.get("type") == "label" and "Property(resolution),4k" in (c.findtext("visible") or "")]
            # "4K", then each dynamic range alone or after it.
            self.assertEqual(sorted(c.findtext("label") for c in drawn), sorted(["4K"] + [w for k, (w, _) in L.BADGES.items() if k != "4k"] * 2))
            for c in drawn:
                # Kodi reads + in a condition as "and": the conditions name the server's tokens.
                self.assertNotIn("HDR+", c.findtext("visible"))
                left, width = int(c.findtext("left")), int(c.findtext("width"))
                self.assertLessEqual(left + width, poster.width - L.BADGE_INSET, c.findtext("label"))
                self.assertEqual(int(c.findtext("top")) + L.BADGE_HEIGHT, poster.height - L.BADGE_INSET)
        # Each pill holds its word in the badge font, 12 either side (bold 20 px).
        for word, width in L.BADGES.values():
            self.assertGreaterEqual(width, window.text_width(word, 20, bold=True) + 24, word)
        # Rows never draw them.
        row = next(c for c in parse(L.WINDOW).iter("control") if c.get("id") == str(L.slot_list(L.HOME_ROWS, 0, "poster")))
        self.assertFalse([c for c in row.iter("control") if "ListItem.Property(range)" in (c.findtext("visible") or "")])

    def test_the_sign_in_has_what_signin_py_fills(self):
        ids = {int(c.get("id")) for c in parse(L.SIGNIN_WINDOW).iter("control") if c.get("id")}
        wanted = {
            L.SIGNIN_CHANGE, L.SIGNIN_ADDRESS, L.SIGNIN_STATUS, L.SIGNIN_SCAN, L.SIGNIN_QR, L.SIGNIN_CODE_A,
            L.SIGNIN_CODE_B, L.SIGNIN_STEP_SCAN, L.SIGNIN_STEP_GO, L.SIGNIN_STEP_ENTER, L.SIGNIN_NUMBER_GO,
            L.SIGNIN_NUMBER_ENTER, L.SIGNIN_QR_CARD, L.SIGNIN_TWO_STEP,
        }
        self.assertEqual(sorted(wanted - ids), [])

    def test_fonts_are_estuarys_and_pictures_are_there(self):
        media = set(os.listdir(MEDIA))
        for name in (L.WINDOW, L.SIGNIN_WINDOW, "pley-overlay.xml"):
            root = parse(name)
            for font in root.iter("font"):
                self.assertIn(font.text, ESTUARY_FONTS, name)
            for tag in ("texture", "texturefocus", "texturenofocus", "texturebg", "midtexture"):
                for texture in root.iter(tag):
                    for picture in (texture.text, texture.get("diffuse")):
                        if picture and not picture.startswith("$"):
                            self.assertIn(picture, media, "%s in %s" % (picture, name))

    def test_the_pictures_notice_ships_with_them(self):
        # Lucide's icons (ISC, the ones from Feather MIT) are in the pictures:
        # their notice goes in the add-on's folder, which package.py zips whole.
        addon = os.path.join(HERE, "..", "plugin.video.pley")
        with open(os.path.join(addon, "NOTICES.txt"), encoding="utf-8") as f:
            notices = f.read()
        for words in ("LICENSE.txt", "https://lucide.dev", "ISC License", "Cole Bemis", "arrow-right", "check"):
            self.assertIn(words, notices)
        self.assertTrue(os.path.exists(os.path.join(addon, "LICENSE.txt")))
        found = ET.parse(os.path.join(addon, "addon.xml")).getroot().find("extension/license")
        self.assertEqual(found.text, "GPL-2.0-or-later AND ISC AND MIT")


if __name__ == "__main__":
    unittest.main()
