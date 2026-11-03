"""pley's own screens in Kodi, drawn as its Android TV app draws them
(apps/android/ui/tv): the rail, Home's hero and rows, an item's page,
libraries, search, playlists and collections. Kodi's player plays, with
the service's Skip Intro and Up Next.

One window (layout.WINDOW) holds every screen; this shows one at a time and
keeps the ones under it, so Back goes to them as they were. What the
screens show comes through the service's proxy, as the plugin's listings.
Discover and the requests are the plugin's (asking.py): the profile's menu
opens them in Kodi's lists, and what is asked for from here (search's On
TMDB, a show's Request more seasons) is the plugin's too, in Kodi's dialogs
over the window.
"""

from __future__ import annotations

import json
import threading
import time
import unicodedata
from urllib.parse import quote_plus, urlencode

import xbmc
import xbmcgui

from . import api, kodi, livetv, playback, profiles, requests, tiles, tracks, words
from . import layout as L
from .asking import AWAY, FOUND_SHOWN
from .listing import image, list_item

T = kodi.text

# Kodi's actions.
LEFT, RIGHT, UP, DOWN, PAGE_UP, PAGE_DOWN = 1, 2, 3, 4, 5, 6
PREVIOUS_MENU, NAV_BACK, CONTEXT_MENU, MENU, SHOW_INFO = 10, 92, 117, 163, 11
BACK = (PREVIOUS_MENU, NAV_BACK)
MOVES = (LEFT, RIGHT, UP, DOWN, PAGE_UP, PAGE_DOWN)
# The remote's number keys, 0 to 9.
NUMBERS = tuple(range(58, 68))

# Focus rests this long on a tile before the backdrop and the info block
# follow it (TvHome's rememberRested).
REST = 0.4
UI = kodi.WINDOW_OPEN
# Room for a pill's icon before its label (pill-*.png keep it unstretched).
ICON_ROOM = " " * 8

SORTS = ("title", "added", "year", "release", "rating", "duration", "last_played")
SORT_NAMES = {
    "title": "sort_title",
    "added": "sort_added",
    "year": "sort_year",
    "release": "sort_release",
    "rating": "sort_rating",
    "duration": "sort_duration",
    "last_played": "sort_last_played",
}
LIBRARY_ICONS = {"movies": "icon-clapperboard.png", "shows": "icon-tv.png", "music": "icon-music.png"}
PAGE = 120
# What a playlist takes (shows, seasons, artists and albums add their
# episodes or tracks), and of which kind.
PLAYLIST_KINDS = {"movie": "video", "show": "video", "season": "video", "episode": "video", "artist": "audio", "album": "audio", "track": "audio"}
# Music's own pages (the web's ArtistView and AlbumView): an artist's
# albums, an album's tracks, and Play and Shuffle for them.
MUSIC_PAGES = ("artist", "album")
# A library's sort as a smart filter says it (SmartFilter.kt's, the web's).
PLEX_SORTS = {
    "title": "titleSort",
    "year": "year",
    "added": "addedAt",
    "rating": "rating",
    "release": "originallyAvailableAt",
    "duration": "duration",
    "last_played": "lastViewedAt",
}
HERO_BUTTONS = (
    L.ITEM_PLAY,
    L.ITEM_SHUFFLE,
    L.ITEM_FROM_START,
    L.ITEM_WATCHED,
    L.ITEM_UNWATCHED,
    L.ITEM_ADD_TO_LIST,
    L.ITEM_IN_LIST,
    L.ITEM_TRAILER,
    L.ITEM_ADD_TO_PLAYLIST,
    L.ITEM_MORE,
)


def color(text: str, argb: str) -> str:
    return "[COLOR %s]%s[/COLOR]" % (argb, text)


# Noto Sans's advance widths (Estuary's font) from " " to "~", in 2048ths of
# an em. Kodi draws them at 0.943 of that, and its bold 0.075 em wider a
# character: so its screenshots measure.
ADVANCES = (
    532, 550, 835, 1323, 1171, 1701, 1499, 460, 614, 614, 1128, 1171, 548, 659, 548, 761,
    1171, 1171, 1171, 1171, 1171, 1171, 1171, 1171, 1171, 1171, 548, 548, 1171, 1171, 1171, 888,
    1841, 1308, 1331, 1294, 1495, 1138, 1062, 1490, 1517, 694, 559, 1267, 1073, 1857, 1556, 1599,
    1239, 1599, 1273, 1124, 1138, 1497, 1228, 1904, 1200, 1159, 1171, 673, 761, 673, 1171, 909,
    575, 1148, 1259, 983, 1259, 1155, 704, 1259, 1265, 528, 528, 1093, 528, 1914, 1265, 1239,
    1259, 1259, 845, 980, 739, 1265, 1040, 1609, 1083, 1044, 962, 778, 1128, 778, 1171,
)
DRAWN, BOLDER = 0.943, 0.075


def advance(ch: str) -> int:
    """How far `ch` moves the pen in Noto Sans, in 2048ths of an em."""
    code = ord(ch)
    if 32 <= code < 127:
        return ADVANCES[code - 32]
    if ch == "…":
        return 1619
    if code >= 0x2E80:  # CJK, kana, hangul: square
        return 2048
    # An accented letter as wide as its letter, anything else as a digit.
    base = ord(unicodedata.normalize("NFD", ch)[0])
    return ADVANCES[base - 32] if 32 <= base < 127 else 1171


def text_width(text: str, size: int, bold: bool = False) -> float:
    """How wide Kodi draws `text` in Estuary's Noto Sans at `size` px."""
    em = sum(advance(ch) for ch in text) / 2048 * DRAWN
    if bold:
        em += BOLDER * len(text)
    return em * size


def wrap(text: str, size: int, width: int, bold: bool = False) -> list:
    """`text`'s words, line by line, as Kodi wraps them in a column `width` wide."""
    space = text_width(" ", size, bold)
    rows, line, used = [], [], 0.0
    for word in text.split():
        w = text_width(word, size, bold)
        if line and used + space + w > width:
            rows.append(line)
            line, used = [], 0.0
        used += (space if line else 0) + w
        line.append(word)
    if line:
        rows.append(line)
    return rows


def lines(text: str, size: int, width: int, most: int, bold: bool = False) -> int:
    """How many lines `text` takes in a column `width` wide, up to `most`."""
    return min(len(wrap(text or "", size, width, bold)), most)


def clamp(text: str, size: int, width: int, most: int) -> str:
    """`text` cut at a word with "…" where it runs past `most` lines (Kodi's
    text boxes stop at a line without saying more was there)."""
    text = " ".join((text or "").split())
    rows = wrap(text, size, width)
    if len(rows) <= most:
        return text
    last = " ".join(rows[most - 1])
    while last and text_width(last + "…", size) > width:
        last = last.rsplit(" ", 1)[0] if " " in last else last[:-1]
    kept = [" ".join(row) for row in rows[: most - 1]] + [last]
    return " ".join(kept).rstrip(" ,.;:") + "…"


# Work posted to the window's thread (Window.post) is announced by this.
POSTED = "pley.posted"
# The add-on acts as another profile (profiles.SWITCHED).
SWITCHED = profiles.SWITCHED


class Monitor(xbmc.Monitor):
    """Hears the service say what the server changed, and the window's own
    threads hand it work: Kodi calls this on the window's thread."""

    def __init__(self, window):
        super().__init__()
        self.window = window

    def onNotification(self, sender, method, data):
        if sender != kodi.ADDON_ID:
            return
        if method.endswith(POSTED):
            self.window.run_posted()
        elif method.endswith(SWITCHED):
            # Switched from the add-on's settings, over the window.
            with self.window.lock:
                if not self.window.closing:
                    self.window.switched()
        elif method.endswith("changed"):
            self.window.changed()


class Session:
    """The server, through the service's proxy, and the details asked of it."""

    def __init__(self, base: str):
        self.base = base
        self.client = api.Client(base, kodi.user_agent(), timeout=60, local=True)
        self.details = {}
        self.lock = threading.Lock()

    def get(self, path: str, **query):
        return self.client.get(path, **query)

    def detail(self, item_id: int, fresh: bool = False) -> dict:
        with self.lock:
            found = None if fresh else self.details.get(item_id)
        if found is None:
            found = self.client.get("/api/items/%d" % item_id)
            with self.lock:
                if len(self.details) > 80:
                    self.details.clear()
                self.details[item_id] = found
        return found

    def cached(self, item_id: int):
        with self.lock:
            return self.details.get(item_id)

    def forget(self) -> None:
        with self.lock:
            self.details.clear()


class Window(xbmcgui.WindowXML):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.session = None
        self.plugin = None
        self.lock = threading.RLock()
        self.controls = {}
        self.stack = []
        self.screen = None
        self.ready = False
        self.libraries = []
        self.me = {}
        # The household's profiles (None: this sign-in can't switch).
        self.household = []
        # GET /api/live: whether the rail has Live TV.
        self.live = {}
        self.rest_token = 0
        self.rest_timer = None
        self.toast_timer = None
        self.posted = []
        self.posted_lock = threading.Lock()
        self.owner = {}
        self.monitor = None
        self.closing = False
        self.focus = None
        # What to do once the window is closed: "choose_server", "sign_in";
        # "listing": open the plugin's listing at self.listing.
        self.after = None
        self.listing = None

    # ------------------------------------------------------------ controls

    def c(self, control_id: int):
        """A control, its Python object kept (Kodi makes a new one each ask)."""
        found = self.controls.get(control_id)
        if found is None:
            found = self.controls[control_id] = self.getControl(control_id)
        return found

    def visible(self, control_id: int, shown: bool) -> None:
        self.c(control_id).setVisible(shown)

    def put(self, control_id: int, text: str) -> None:
        self.c(control_id).setLabel(text or "")

    def place(self, control_id: int, x: int, y: int) -> None:
        self.c(control_id).setPosition(int(x), int(y))

    def text(self, control_id: int, text: str) -> None:
        box = self.c(control_id)
        box.reset()
        box.setText(text or "")

    def meta(self, line: int, parts: list) -> None:
        """Fills a metadata line with `parts`: ("text"|"lead"|"rating"|"badge"|
        "quality", text). Quality badges sit together, without dots."""
        for i in range(L.META_PIECES):
            dot, star, text, badge = L.meta_piece(line, i)
            if i < len(parts):
                kind, value = parts[i]
                together = kind == "quality" and parts[i - 1][0] == "quality"
                self.visible(dot, i > 0 and not together)
                self.visible(star, kind == "rating")
                if kind in ("badge", "quality"):
                    self.visible(text, False)
                    self.put(badge, value)
                    self.visible(badge, True)
                else:
                    self.put(text, color(value, L.FG) if kind in ("lead", "rating") else value)
                    self.visible(text, True)
                    self.visible(badge, False)
            else:
                for c in (dot, star, text, badge):
                    self.visible(c, False)

    def prop(self, name: str, value: str) -> None:
        self.setProperty(name, value or "")

    # ----------------------------------------------------------- lifecycle

    def onInit(self) -> None:
        with self.lock:
            if self.ready:
                # Back from playing, or a dialog: what changed shows.
                if self.screen is not None:
                    try:
                        self.screen.resume()
                    except api.ApiError as e:
                        # The page stays as it was; the next change shows.
                        kodi.log("back on %s: %s" % (self.screen.name, e.message), xbmc.LOGWARNING)
                return
            self.ready = True
            kodi.set_property(UI, "1")
            self.monitor = Monitor(self)
            started = time.perf_counter()
            # What the server knows of this network, kept by the proxy by the
            # time something is played (playback.starting_ceiling).
            threading.Thread(target=playback.look_ahead, args=(self.session.client,), daemon=True).start()
            self.start()
            kodi.log("Pley's window ready in %d ms" % ((time.perf_counter() - started) * 1000))

    def start(self) -> None:
        try:
            self.me, self.libraries, self.household, self.live = api.together(
                lambda: self.session.get("/api/auth/me"),
                lambda: self.session.get("/api/libraries"),
                lambda: profiles.household(self.session.client),
                lambda: livetv.summary(self.session.client),
            )
        except api.ApiError as e:
            return self.failed(e, Home(self))
        self.setup_rail()
        if self.household and len(self.household) > 1 and not kodi.get_property(profiles.GATE):
            # A household's Kodi asks who is watching as it starts, as the
            # TV apps do: once each time Kodi starts.
            return self.go(Profiles(self, self.household, gate=True))
        self.go(Home(self))

    def switched(self) -> None:
        """Acting as another profile: the rail and Home are its own."""
        self.session.forget()
        try:
            # Live TV may not be open to this profile (an age limit).
            self.me, self.libraries, self.live = api.together(
                lambda: self.session.get("/api/auth/me"),
                lambda: self.session.get("/api/libraries"),
                lambda: livetv.summary(self.session.client),
            )
        except api.ApiError as e:
            return self.failed(e, Home(self))
        self.setup_rail()
        self.go(Home(self))

    def close(self) -> None:
        self.closing = True
        for timer in (self.rest_timer, self.toast_timer):
            if timer is not None:
                timer.cancel()
        kodi.clear_property(UI)
        super().close()

    def changed(self) -> None:
        """The server said something changed (the watch state, My List, a
        library, Live TV's channels): the screen shows it."""
        with self.lock:
            if self.closing or self.screen is None:
                return
            self.session.forget()
            rail = self.getFocusId() if self.focus_in_rail() else None
            try:
                # A library added, renamed, moved (on any of the profile's devices) or scanned,
                # or Live TV set up or emptied.
                libraries, live = api.together(
                    lambda: self.session.get("/api/libraries"),
                    lambda: livetv.summary(self.session.client),
                )
                self.take_libraries(libraries, live)
                self.screen.refresh()
            except api.ApiError as e:
                kodi.log("refreshing %s: %s" % (self.screen.name, e.message), xbmc.LOGWARNING)
            if rail is not None and self.getFocusId() != rail:
                # Someone in the rail stays there, whatever the page redrew.
                self.setFocusId(rail)

    # ---------------------------------------------------------- navigation

    def go(self, screen) -> None:
        """Opens a page of the rail's: what was open goes."""
        self.stack = []
        self.screen = None
        self.open(screen)

    def open(self, screen) -> None:
        """Opens `screen` over the one showing."""
        try:
            screen.load()
        except api.ApiError as e:
            return self.failed(e, screen)
        instead = screen.instead()
        if instead is not None:
            self.stack = []
            self.screen = None
            screen = instead
        if self.screen is not None:
            self.screen.leave()
            self.stack.append(self.screen)
        self.screen = screen
        self.show(screen)

    def show(self, screen, back: bool = False) -> None:
        for name, group in L.SCREENS.items():
            if name != screen.name:
                self.visible(group, False)
        self.prop("pley.screen", screen.name)
        self.visible(L.SCREENS[screen.name], True)
        self.prop("pley.rail", screen.rail or (self.stack[0].rail if self.stack else "home"))
        self.prop("pley.backdrop.on", "1" if screen.backdrop else "")
        if not screen.backdrop:
            self.prop("pley.backdrop", "")
        filled = self.owner.get(screen.name) is screen
        self.owner[screen.name] = screen
        screen.show(back and filled)

    def back(self) -> None:
        if self.screen is not None and self.screen.back():
            return
        if self.stack:
            self.screen.leave()
            self.screen = self.stack.pop()
            self.show(self.screen, back=True)
            return
        if self.focus_in_rail():
            self.close()
        else:
            self.focus_rail()

    def failed(self, error: api.ApiError, screen) -> None:
        """What a page says when the server didn't answer it (or another
        device did)."""
        if error.code in ("unreachable", "another_device"):
            name = kodi.setting("server_name") or kodi.setting("server_url")
            title = T("cant_reach" if error.code == "unreachable" else error.code) % name
            notice = Notice(self, title, T("cant_reach_hint"), retry=screen)
        elif error.code == "signed_out":
            notice = Notice(self, T("signed_out"), "", retry=screen, sign_in=True)
        else:
            notice = Notice(self, T("page_failed") if not isinstance(screen, Home) else T("home_failed"), error.message, retry=screen)
        if self.screen is None or isinstance(screen, Home):
            self.stack = []
            self.screen = None
        self.open(notice)

    # ---------------------------------------------------------------- rail

    def setup_rail(self) -> None:
        name = self.me.get("name") or self.me.get("username") or ""
        self.prop("pley.avatar.name", name)
        self.prop("pley.avatar.server", kodi.setting("server_name") or "")
        tint = (self.me.get("avatar_color") or "").lstrip("#")
        # Text on a light colour is dark, as the Android app's avatar draws it.
        light = len(tint) == 6 and luminance(tint) > 0.45
        self.c(L.RAIL_COLOR).setColorDiffuse("FF" + tint.upper() if len(tint) == 6 else L.SURFACE3)
        self.c(L.RAIL_INITIAL).setLabel(name[:1].upper(), textColor=L.ON_ACCENT if light else L.FG)
        # None: the initial shows (another profile's picture mustn't stay).
        self.prop("pley.avatar.picture", image(self.session.base, self.me.get("avatar_url"), 128))
        live = livetv.available(self.live)
        shown = rail_shown(len(self.libraries), live)
        for i, entry in enumerate(L.RAIL_LIBRARIES):
            if i < shown:
                library = self.libraries[i]
                icon = LIBRARY_ICONS.get(library.get("kind"), "icon-folder.png")
                for control in L.rail_icons(entry):
                    self.c(control).setImage(icon, False)
                for control in L.rail_labels(entry):
                    self.put(control, library.get("name"))
            self.visible(L.rail_group(entry), i < shown)
        entries = rail_entries(len(self.libraries), live)
        self.visible(L.rail_group(L.RAIL_LIVE), L.RAIL_LIVE in entries)
        self.visible(L.rail_group(L.RAIL_ALL_LIBRARIES), L.RAIL_ALL_LIBRARIES in entries)
        self.visible(L.rail_group(L.RAIL_COLLECTIONS), L.RAIL_COLLECTIONS in entries)
        pitch = L.rail_pitch(len(entries))
        for at, entry in enumerate(entries):
            self.place_rail_entry(entry, L.RAIL_TOP + at * pitch, pitch)
        # Settings stays at the foot, its pill as tall as the others'.
        self.place_rail_entry(L.RAIL_SETTINGS, L.RAIL_SETTINGS_Y, pitch)

    def place_rail_entry(self, entry: int, y: int, pitch: int) -> None:
        """An entry's group centred on `y`, its pills as tall as `pitch`
        leaves room for (the icon and name stay centred in the group)."""
        self.place(L.rail_group(entry), 0, y - L.RAIL_ROW // 2)
        row = L.rail_row(pitch)
        for control in (entry, L.rail_pill(entry)):
            self.place(control, 8, (L.RAIL_ROW - row) // 2)
            self.c(control).setHeight(row)

    def focus_in_rail(self) -> bool:
        return self.getFocusId() in L.RAIL_ENTRIES

    def focus_rail(self) -> None:
        """To the rail's current entry (D-pad rule 3)."""
        self.setFocusId(self.rail_entry(self.getProperty("pley.rail")))

    def rail_entry(self, key: str) -> int:
        if key.startswith("lib") and key[3:].isdigit():
            entry = L.RAIL_LIBRARIES[int(key[3:])] if int(key[3:]) < len(L.RAIL_LIBRARIES) else L.RAIL_HOME
        else:
            entry = {
                "profile": L.RAIL_AVATAR,
                "search": L.RAIL_SEARCH,
                "live": L.RAIL_LIVE,
                "libraries": L.RAIL_ALL_LIBRARIES,
                "playlists": L.RAIL_PLAYLISTS,
                "collections": L.RAIL_COLLECTIONS,
                "settings": L.RAIL_SETTINGS,
            }.get(key, L.RAIL_HOME)
        # Collections or Libraries as the page's, while the rail shows the other: Home.
        shown = rail_entries(len(self.libraries), livetv.available(self.live)) + [L.RAIL_AVATAR, L.RAIL_SETTINGS]
        return entry if entry in shown else L.RAIL_HOME

    def rail_click(self, control_id: int) -> None:
        if control_id == L.RAIL_AVATAR:
            return self.account()
        if control_id == L.RAIL_SETTINGS:
            kodi.addon().openSettings()
            return
        if control_id == L.RAIL_SEARCH:
            return self.go(Search(self))
        if control_id == L.RAIL_HOME:
            return self.go(Home(self))
        if control_id == L.RAIL_LIVE:
            return self.go(Live(self))
        if control_id == L.RAIL_ALL_LIBRARIES:
            return self.go(Libraries(self))
        if control_id == L.RAIL_PLAYLISTS:
            return self.go(Grid.playlists(self))
        if control_id == L.RAIL_COLLECTIONS:
            return self.go(Grid.collections(self))
        if control_id in L.RAIL_LIBRARIES:
            i = L.RAIL_LIBRARIES.index(control_id)
            if i < len(self.libraries):
                self.go(Library(self, self.libraries[i], "lib%d" % i))

    def account(self) -> None:
        actions = []
        s = self.requests_summary()
        if requests.discovers(s):
            # Kodi's own lists (asking.py), with the window closed.
            actions.append((T("discover"), lambda: self.leave_for_listing(route="discover")))
        if requests.lists(s):
            actions.append((requests.entry_label(s, T), lambda: self.leave_for_listing(route="requests")))
        if self.household is None:
            # Signed in before the add-on asked for `profiles`.
            actions.append((T("switch_profile"), self.sign_in_to_switch))
        elif len(self.household) > 1:
            actions.append((T("switch_profile"), self.switch_profile))
        actions.append((T("choose_server"), lambda: self.leave_for("choose_server")))
        actions.append((T("tracks"), self.track_defaults))
        if len(self.libraries) > 1:
            actions.append((T("sort_libraries"), self.sort_libraries))
        actions.append((T("sign_out"), self.sign_out))
        chosen = xbmcgui.Dialog().contextmenu([name for name, _ in actions])
        if chosen >= 0:
            actions[chosen][1]()

    def switch_profile(self) -> None:
        """"Who's watching?", the household as it is now."""
        try:
            found = profiles.household(self.session.client)
        except api.ApiError as e:
            return self.toast(T("server_error") % e.message, 6)
        if found is None:
            return self.sign_in_to_switch()
        self.household = found
        self.open(Profiles(self, found))

    def sign_in_to_switch(self) -> None:
        """This sign-in can't switch profiles: signing in again with a code
        allows it."""
        if xbmcgui.Dialog().yesno(T("switch_profile"), T("cant_switch"), nolabel=T("cancel"), yeslabel=T("sign_in")):
            self.leave_for("sign_in")

    def sign_out(self) -> None:
        """Sign out, once the person says so: the window closes first."""
        if kodi.ask_sign_out():
            self.leave_for("sign_out")

    def track_defaults(self) -> None:
        """Audio and subtitles: the profile's defaults for titles it hasn't
        picked tracks for (the TV apps' rows in Settings)."""
        try:
            me = self.session.get("/api/auth/me")
        except api.ApiError as e:
            return self.toast(T("server_error") % e.message, 6)

        def select(heading, labels, preselect):
            return xbmcgui.Dialog().select(heading, labels, preselect=preselect)

        def say(words, error):
            self.toast(words, 6 if error else 3.5)

        self.me = tracks.edit_defaults(self.session.client, me, select, say, T, api.ApiError)

    # ---------------------------------------------- the libraries' order

    def sort_libraries(self) -> None:
        """Sort libraries (the Android TV app's, in Settings): a library,
        then where it goes; each move is saved at once, for the profile on
        all its devices."""
        while True:
            names = [lib.get("name") or "" for lib in self.libraries]
            chosen = xbmcgui.Dialog().select(T("sort_libraries"), names + [T("server_order")])
            if chosen < 0:
                return
            if chosen == len(names):
                return self.use_server_order()
            moving = self.libraries[chosen]
            others = [lib for lib in self.libraries if lib is not moving]
            places = [T("first")] + [T("after") % (lib.get("name") or "") for lib in others]
            to = xbmcgui.Dialog().select(names[chosen], places, preselect=chosen)
            if to < 0 or to == chosen:
                continue
            ids = [lib["id"] for lib in others]
            ids.insert(to, moving["id"])
            if not self.save_order(ids):
                return

    def library_menu(self, control_id: int) -> None:
        """A rail library's menu: Move up, Move down, the server's order."""
        i = L.RAIL_LIBRARIES.index(control_id)
        if i >= len(self.libraries) or len(self.libraries) < 2:
            return
        ids = [lib["id"] for lib in self.libraries]
        actions = []
        if i > 0:
            actions.append((T("move_up"), i - 1))
        if i + 1 < len(ids):
            actions.append((T("move_down"), i + 1))
        actions.append((T("server_order"), None))
        chosen = xbmcgui.Dialog().contextmenu([name for name, _ in actions])
        if chosen < 0:
            return
        to = actions[chosen][1]
        if to is None:
            return self.use_server_order()
        ids.insert(to, ids.pop(i))
        if self.save_order(ids):
            # Moved past the libraries the rail lists, it is on the Libraries page.
            shown = rail_shown(len(self.libraries), livetv.available(self.live))
            self.setFocusId(L.RAIL_LIBRARIES[to] if to < shown else L.RAIL_ALL_LIBRARIES)

    def save_order(self, ids: list) -> bool:
        """The profile's own order (PUT /api/me/libraries/order): whether it
        was saved."""
        return self.reorder(lambda: self.session.client.put("/api/me/libraries/order", {"ids": ids}))

    def use_server_order(self) -> bool:
        return self.reorder(lambda: self.session.client.delete("/api/me/libraries/order"))

    def reorder(self, change) -> bool:
        try:
            libraries = change()
        except api.ApiError as e:
            # A server from before libraries had an order of the profile's own.
            self.toast(T("sort_needs_newer_server") if e.status in (404, 405) else T("server_error") % e.message, 6)
            if e.status == 409:
                # A library came or went since they were listed: the rail as they are now.
                try:
                    self.place_libraries(self.session.get("/api/libraries"))
                except api.ApiError:
                    pass
            return False
        self.place_libraries(libraries)
        if isinstance(self.screen, (Home, Libraries)):
            # Home's rows and the Libraries page follow the libraries' order.
            self.screen.refresh()
        return True

    def take_libraries(self, libraries: list, live: dict = None) -> None:
        """The libraries as the server lists them now, and Live TV's summary
        when it was asked for too: the rail redrawn if what it shows of them
        changed (the rest is the Libraries page's)."""
        was_live = livetv.available(self.live)
        if live is not None:
            self.live = live
        redraw = [rail_view(lib) for lib in libraries] != [rail_view(lib) for lib in self.libraries]
        if redraw or livetv.available(self.live) != was_live:
            self.place_libraries(libraries)
        else:
            self.libraries = libraries

    def place_libraries(self, libraries: list) -> None:
        """The rail in the libraries' order, its mark staying on the library
        open."""
        self.libraries = libraries
        self.setup_rail()
        for screen in self.stack + ([self.screen] if self.screen is not None else []):
            if isinstance(screen, Library) and (screen.rail or "").startswith("lib"):
                screen.rail = library_rail(libraries, screen.library["id"], livetv.available(self.live))
        if self.screen is not None:
            self.prop("pley.rail", self.screen.rail or (self.stack[0].rail if self.stack else "home"))

    def leave_for(self, after: str) -> None:
        """Closes the window, and does `after` (run())."""
        self.after = after
        self.close()

    def leave_for_listing(self, **params) -> None:
        """Closes the window for one of the plugin's listings, in Kodi's own
        window (Discover, the requests)."""
        self.listing = plugin_url(**params)
        self.leave_for("listing")

    def requests_summary(self, ask: bool = True):
        """The requests' summary as the service keeps it (news.py), asked of
        the server (`ask`) only while it has none: {"enabled": false} where
        requests are off, None where it can't be read."""
        try:
            found = json.loads(kodi.get_property(requests.SUMMARY) or "null")
        except ValueError:
            found = None
        if isinstance(found, dict) or not ask:
            return found if isinstance(found, dict) else None
        try:
            found = requests.summary(self.session.client) or {"enabled": False}
        except api.ApiError as e:
            kodi.log("requests' summary: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
            return None
        kodi.set_property(requests.SUMMARY, json.dumps(found))
        return found

    # -------------------------------------------------------------- events

    def onAction(self, action) -> None:
        with self.lock:
            if self.closing:
                return
            code = action.getId()
            if code in BACK:
                return self.back()
            focus = self.getFocusId()
            if code in (CONTEXT_MENU, MENU) and focus in L.RAIL_LIBRARIES:
                return self.library_menu(focus)
            if code in (CONTEXT_MENU, MENU) and self.screen is not None:
                return self.screen.menu(focus)
            if code == SHOW_INFO and self.screen is not None and focus not in L.RAIL_ENTRIES:
                return self.screen.info(focus)
            if code in NUMBERS and self.screen is not None and focus not in L.RAIL_ENTRIES:
                return self.screen.number(code - NUMBERS[0])
            if code in MOVES and self.screen is not None:
                if focus in L.RAIL_ENTRIES:
                    if code == RIGHT:
                        self.screen.enter()
                    return
                self.screen.moved(focus, code)

    def onClick(self, control_id: int) -> None:
        with self.lock:
            if self.closing:
                return
            if control_id in L.RAIL_ENTRIES:
                return self.rail_click(control_id)
            if self.screen is not None:
                self.screen.click(control_id)

    def onFocus(self, control_id: int) -> None:
        with self.lock:
            previous, self.focus = self.focus, control_id
            if self.closing or self.screen is None:
                return
            if control_id in L.RAIL_ENTRIES:
                current = self.rail_entry(self.getProperty("pley.rail"))
                if previous not in L.RAIL_ENTRIES and control_id != current:
                    # Into the rail from the page: to the page's entry.
                    self.setFocusId(current)
                return
            self.screen.focused(control_id)

    # ---------------------------------------------------- rest and toasts

    def post(self, work) -> None:
        """Runs `work` on the window's thread, soon: what other threads
        (timers, answers from the server) change on screen goes through here,
        as Kodi's controls are changed partly at once and partly by messages
        to its GUI thread, which threads racing each other could reorder."""
        with self.posted_lock:
            self.posted.append(work)
        xbmc.executebuiltin("NotifyAll(%s,%s)" % (kodi.ADDON_ID, POSTED))

    def run_posted(self) -> None:
        with self.posted_lock:
            work, self.posted = self.posted, []
        with self.lock:
            for job in work:
                if self.closing:
                    return
                try:
                    job()
                except api.ApiError as e:
                    kodi.log("posted work: %s" % e.message, xbmc.LOGWARNING)

    def rest(self, then) -> None:
        """Calls `then` once focus rested REST seconds where it is now."""
        self.rest_token += 1
        token = self.rest_token
        if self.rest_timer is not None:
            self.rest_timer.cancel()

        def fire():
            if token == self.rest_token and not self.closing:
                self.post(lambda: token == self.rest_token and then())

        self.rest_timer = threading.Timer(REST, fire)
        self.rest_timer.daemon = True
        self.rest_timer.start()

    def later(self, work, then, failed=None) -> None:
        """Runs `work` away from the window's thread, then `then(answer)`
        on it (not once it closed); `failed()` there when the server
        refused or didn't answer."""

        def run():
            try:
                answer = work()
            except api.ApiError as e:
                kodi.log("in the background: %s" % e.message, xbmc.LOGWARNING)
                if failed is not None:
                    self.post(failed)
                return
            self.post(lambda: then(answer))

        threading.Thread(target=run, daemon=True).start()

    def toast(self, text: str, seconds: float = 3.5) -> None:
        self.put(L.TOAST_LABEL, text)
        self.prop("pley.toast", "1")
        if self.toast_timer is not None:
            self.toast_timer.cancel()
        self.toast_timer = threading.Timer(seconds, lambda: self.post(lambda: self.prop("pley.toast", "")))
        self.toast_timer.daemon = True
        self.toast_timer.start()

    # ---------------------------------------------------------- things to do

    def play(self, item: dict, from_start: bool = False) -> None:
        """Plays `item` (a show or season: the episode to watch next; an
        artist or album: its tracks)."""
        if item.get("kind") in MUSIC_PAGES:
            return self.play_music(item)
        try:
            if item.get("kind") in ("show", "season"):
                found = self.session.get("/api/items/%d/on-deck" % item["id"]).get("item")
                if not found:
                    return self.toast(T("nothing_found"))
                item = found
            start = 0 if from_start else (words.state(item).get("position_ms") or 0) if words.in_progress(item) else 0
            li = playback.prepare(self.session.client, self.session.base, item["id"], start, self.session.cached(item["id"]))
        except api.ApiError as e:
            return self.toast(T("server_error") % e.message, 6)
        if start:
            li.setProperty("StartOffset", "%.3f" % (start / 1000.0))
        xbmc.Player().play(li.getPath(), li)

    def play_music(self, item: dict, at: int = 0, shuffle: bool = False, tracks=None) -> None:
        """An artist's or album's tracks in Kodi's music playlist, from
        track `at` (the web's audio queue). Kodi asks the plugin's play
        route for each as it comes to it, so each is reported as it plays."""
        try:
            if tracks is None:
                tracks = music_tracks(self.session, item)
        except api.ApiError as e:
            return self.toast(T("server_error") % e.message, 6)
        if not tracks:
            return self.toast(T("no_tracks"))
        queue = xbmc.PlayList(xbmc.PLAYLIST_MUSIC)
        queue.clear()
        for track in tracks:
            queue.add(play_url(track["id"]), list_item(self.session.base, track))
        if shuffle:
            queue.shuffle()
            at = 0
        else:
            queue.unshuffle()
        xbmc.Player().play(queue, startpos=at)

    def set_watched(self, item: dict, watched: bool) -> None:
        self.session.client.post("/api/items/%d/%s" % (item["id"], "watched" if watched else "unwatched"))
        self.changed()

    def set_listed(self, item: dict, listed: bool) -> None:
        path = "/api/watchlist/%d" % item["id"]
        if listed:
            self.session.client.put(path)
        else:
            self.session.client.delete(path)
        self.changed()

    def tile_menu(self, item: dict, continue_watching: bool = False, extra=()) -> None:
        """A tile's actions (TvItemActionsScreen), in Kodi's context menu;
        then the page's own (`extra`: a playlist's)."""
        kind = item.get("kind")
        actions = []
        if kind in ("movie", "show", "season", "episode", "extra"):
            watched = words.state(item).get("watched")
            actions.append((T("mark_unwatched") if watched else T("mark_watched"), lambda: self.set_watched(item, not watched)))
        if kind in ("movie", "show"):
            try:
                listed = self.session.detail(item["id"]).get("in_watchlist")
            except api.ApiError:
                listed = False
            actions.append((T("remove_from_list") if listed else T("add_to_list"), lambda: self.set_listed(item, not listed)))
        if kind in PLAYLIST_KINDS:
            actions.append((T("add_to_playlist"), lambda: self.add_to_playlist(item)))
        if continue_watching:
            actions.append((T("remove_from_continue"), lambda: self.forget_progress(item)))
        if kind == "episode" and item.get("grandparent_id"):
            actions.append((T("go_to_show"), lambda: self.open(Item(self, item["grandparent_id"]))))
        self.choose(actions + list(extra))

    def choose(self, actions: list) -> None:
        """(name, action) pairs in Kodi's context menu: the one chosen is
        done, and what the server refused said."""
        if not actions:
            return
        chosen = xbmcgui.Dialog().contextmenu([name for name, _ in actions])
        if chosen >= 0:
            try:
                actions[chosen][1]()
            except api.ApiError as e:
                self.toast(T("server_error") % e.message, 6)

    def rename_playlist(self, playlist: dict):
        """Rename playlist: the new title, once saved (None: unchanged)."""
        old = playlist.get("title") or ""
        title = (xbmcgui.Dialog().input(T("rename_playlist"), defaultt=old) or "").strip()
        if not title or title == old:
            return None
        self.session.client.patch("/api/playlists/%d" % playlist["id"], {"title": title})
        return title

    def delete_playlist(self, playlist: dict) -> bool:
        """Delete playlist, once the person says so: whether it went."""
        asked = T("delete_playlist_body") % (playlist.get("title") or "")
        if not xbmcgui.Dialog().yesno(T("delete_playlist_ask"), asked, nolabel=T("cancel"), yeslabel=T("delete")):
            return False
        self.session.client.delete("/api/playlists/%d" % playlist["id"])
        self.toast(T("playlist_deleted"))
        return True

    def forget_progress(self, item: dict) -> None:
        self.session.client.delete("/api/continue-watching/%d" % item["id"])
        self.changed()

    def add_to_playlist(self, item: dict) -> None:
        """Add to playlist (TvAddToPlaylist): into a new playlist named
        here, or one of the profile's of the item's kind (smart ones fill
        themselves)."""
        kind = PLAYLIST_KINDS.get(item.get("kind"), "video")
        playlists = self.session.get("/api/playlists").get("playlists", [])
        mine = [p for p in playlists if not p.get("smart_filter") and p.get("kind", kind) == kind]
        new = T("new_music_playlist" if kind == "audio" else "new_video_playlist")
        entries = [xbmcgui.ListItem(new, offscreen=True)]
        entries += [xbmcgui.ListItem(p.get("title") or "", T("playlist_items") % p.get("item_count", 0), offscreen=True) for p in mine]
        chosen = xbmcgui.Dialog().select(T("add_to_playlist"), entries)
        if chosen < 0:
            return
        if chosen == 0:
            title = xbmcgui.Dialog().input(new).strip()
            if not title:
                return
            self.session.client.post("/api/playlists", {"title": title, "kind": kind, "item_ids": [item["id"]]})
        else:
            playlist = mine[chosen - 1]
            title = playlist.get("title") or ""
            self.session.client.post("/api/playlists/%d/items" % playlist["id"], {"item_ids": [item["id"]]})
        self.toast(T("added_to") % title)

    def open_item(self, item: dict) -> None:
        """What a tile opens: the item's page; a person's, a library's."""
        kind = item.get("kind")
        if kind == "person":
            return self.open(Grid.person(self, item))
        if kind == "seeall":
            library = next((lib for lib in self.libraries if lib["id"] == item.get("library_id")), None)
            if library is not None:
                self.open(Library(self, library, None))
            return
        if kind == "extra":
            return self.play(item, from_start=True)
        if kind == "track":
            return self.play(item, from_start=True)
        self.open(Item(self, item["id"], item))


def play_url(item_id: int) -> str:
    """The plugin's play route for `item_id`, which Kodi asks as it comes
    to it in a playlist."""
    return plugin_url(route="play", id=item_id)


def plugin_url(**params) -> str:
    """One of the plugin's routes: a listing, or an action."""
    return "plugin://%s/?%s" % (kodi.ADDON_ID, urlencode(params))


def run_plugin(**params) -> None:
    """One of the plugin's actions (asking.py's), its dialogs over the
    window; what it did shows once it is done (kodi.refresh_listing)."""
    xbmc.executebuiltin("RunPlugin(%s)" % plugin_url(**params))


def newest_first(albums: list) -> list:
    """An artist's albums as the web lists them: newest first, then by title."""
    return sorted(albums, key=lambda a: (-(a.get("year") or 0), (a.get("sort_title") or a.get("title") or "").lower()))


def music_tracks(session, item: dict) -> list:
    """An album's tracks, or an artist's, album by album (newest first)."""
    children = lambda id_: session.get("/api/items/%d/children" % id_).get("items", [])  # noqa: E731
    if item.get("kind") == "album":
        return children(item["id"])
    albums = newest_first(children(item["id"]))
    lists = api.together(*[lambda a=a: children(a["id"]) for a in albums]) if albums else []
    return [track for found in lists for track in found or []]


def rail_view(library: dict) -> tuple:
    """What the rail shows of a library."""
    return library.get("id"), library.get("name"), library.get("kind")


def rail_shown(libraries: int, live: bool = False) -> int:
    """How many of `libraries` the rail lists: every one while all its
    entries fit over Settings at full height, else as many as fit with
    Libraries and Playlists after them, and never fewer than four (the other
    TV apps' rails scroll to theirs; Kodi's entries have fixed places)."""
    top = 3 if live else 2
    if libraries <= len(L.RAIL_LIBRARIES) and top + libraries + 2 <= L.RAIL_FIT:
        return libraries
    room = min(len(L.RAIL_LIBRARIES), L.RAIL_FIT - top - 2)
    return min(libraries, max(L.RAIL_LEAST_LIBRARIES, room))


def rail_entries(libraries: int, live: bool = False) -> list:
    """The rail's entries from Search down, Settings apart (RailItems.kt):
    Live TV after Home where the server has channels (the web's sidebar),
    the libraries it lists, and with more, Libraries in Collections' place
    (collections stay on each library's Collections tab); without a
    library, no Collections."""
    shown = rail_shown(libraries, live)
    top = [L.RAIL_SEARCH, L.RAIL_HOME] + ([L.RAIL_LIVE] if live else [])
    if libraries > shown:
        return [*top, *L.RAIL_LIBRARIES[:shown], L.RAIL_ALL_LIBRARIES, L.RAIL_PLAYLISTS]
    return [*top, *L.RAIL_LIBRARIES[:shown], L.RAIL_PLAYLISTS] + ([L.RAIL_COLLECTIONS] if libraries else [])


def library_rail(libraries: list, library_id, live: bool = False) -> str:
    """The rail's entry a library's page marks: its own among those the
    rail lists, else Libraries (Home, once the library has gone)."""
    at = next((i for i, lib in enumerate(libraries) if lib["id"] == library_id), None)
    if at is None:
        return "home"
    return "lib%d" % at if at < rail_shown(len(libraries), live) else "libraries"


LIBRARY_NOUNS = {"movies": ("movie", "movies"), "shows": ("show", "shows"), "music": ("artist", "artists")}


def library_count(library: dict) -> str:
    """How much a library holds: "12 movies", "1 show", "40 artists"."""
    if library.get("item_count") is None:
        return ""
    return words.plural(library["item_count"], *LIBRARY_NOUNS.get(library.get("kind"), ("item", "items")))


def luminance(hex_rgb: str) -> float:
    r, g, b = (int(hex_rgb[i : i + 2], 16) / 255.0 for i in (0, 2, 4))
    lin = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in (r, g, b)]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


# ----------------------------------------------------------------- rows


class Rows:
    """A screen's rows of tiles: slots the skin holds, filled and placed
    here. A row: {"key", "title", "shape", "items", "see_all"}, and "tile"
    (an item's tile) where its tiles aren't the library's, "see_all_label"
    where its last tile says something else (search's Discover row's Try again)."""

    def __init__(self, win: Window, base: int, slots: int, shapes: tuple):
        self.win = win
        self.base = base
        self.slots = slots
        self.shapes = shapes
        self.rows = []

    def fill(self, rows: list) -> None:
        self.rows = rows[: self.slots]
        for i in range(self.slots):
            self.fill_slot(i)

    def append(self, row: dict) -> bool:
        """A row that came after the others (search's Discover row) in the next
        slot, theirs left as they are: False when none is free."""
        if len(self.rows) >= self.slots:
            return False
        self.rows.append(row)
        self.fill_slot(len(self.rows) - 1)
        return True

    def fill_slot(self, i: int) -> None:
        win = self.win
        base = win.session.base
        group = L.slot(self.base, i)
        if i >= len(self.rows):
            win.visible(group, False)
            return
        row = self.rows[i]
        win.put(L.slot_header(self.base, i), row["title"])
        for shape in self.shapes:
            found = win.c(L.slot_list(self.base, i, shape))
            found.reset()
            if shape == row["shape"]:
                if row.get("tile") is not None:
                    items = [row["tile"](item) for item in row["items"]]
                elif shape == "person":
                    items = [tiles.person(base, credit) for credit in row["items"]]
                elif shape == "channel":
                    now = time.time()
                    items = [channel_tile(base, channel, now) for channel in row["items"]]
                else:
                    items = [tiles.tile(base, item, shape) for item in row["items"]]
                if row.get("see_all") is not None:
                    items.append(tiles.see_all(row.get("see_all_label") or T("see_all")))
                found.addItems(items)
                found.setVisible(True)
            else:
                found.setVisible(False)
        win.visible(group, True)

    def place(self, focused, top: int) -> None:
        """Places the rows: from `top`, or with row `focused` under the info
        block and the rows above it out of sight."""
        y = top if focused is None else L.FOCUSED_ROW_Y
        for i, row in enumerate(self.rows):
            group = L.slot(self.base, i)
            if focused is not None and i < focused:
                self.win.place(group, 0, -2 * L.HEIGHT)
                continue
            self.win.place(group, 0, y)
            y += L.row_height(L.SHAPES[row["shape"]]) + L.ROW_GAP

    def find(self, control_id: int):
        """(row index, row) of a row list's id, or None."""
        found = L.find_slot(self.base, self.slots, control_id)
        if found is None or found[0] >= len(self.rows):
            return None
        return found[0], self.rows[found[0]]

    def selected(self, control_id: int):
        """(row index, row, item) under focus in a row list, or None. A row's
        "See all" is {"kind": "seeall", "library_id": …}."""
        found = self.find(control_id)
        if found is None:
            return None
        i, row = found
        at = self.win.c(control_id).getSelectedPosition()
        if at < 0:
            return None
        if at >= len(row["items"]):
            return i, row, {"kind": "seeall", "library_id": row.get("see_all")}
        return i, row, row["items"][at]

    def focus(self, i: int, at: int = 0) -> None:
        row = self.rows[i]
        list_id = L.slot_list(self.base, i, row["shape"])
        self.win.setFocusId(list_id)
        if at:
            self.win.c(list_id).selectItem(at)


# --------------------------------------------------------------- screens


class Screen:
    """A page of pley's window."""

    name = ""
    rail = None
    backdrop = False

    def __init__(self, win: Window):
        self.win = win
        self.session = win.session
        self.last_focus = None

    def load(self) -> None:
        """Asks the server for what the page shows (may raise ApiError)."""

    def instead(self):
        """A page to show instead of this one, once loaded (None: this one)."""
        return None

    def show(self, filled: bool) -> None:
        """Draws the page; `filled`: its controls still hold it (Back)."""

    def leave(self) -> None:
        self.last_focus = self.win.getFocusId()

    def resume(self) -> None:
        """The window shows again (after playing): what changed shows."""
        self.refresh()

    def refresh(self) -> None:
        """What the page shows changed on the server."""

    def back(self) -> bool:
        return False

    def enter(self) -> None:
        """Right from the rail: back to where focus was on the page."""
        self.win.setFocusId(self.last_focus or self.first())

    def first(self) -> int:
        return 0

    def focused(self, control_id: int) -> None:
        pass

    def moved(self, control_id: int, action: int) -> None:
        pass

    def click(self, control_id: int) -> None:
        pass

    def menu(self, control_id: int) -> None:
        pass

    def info(self, control_id: int) -> None:
        """The remote's Info key."""

    def number(self, digit: int) -> None:
        """A number key."""


def hero_lines(win: Window, ids: dict, info: dict, gaps: dict) -> int:
    """Places a hero's pieces down its column as the Android app stacks
    them (a missing one closes its gap), and returns the buttons' top.
    `ids`/`gaps`: by piece; `info`: the texts."""
    title = info.get("title") or ""
    count = lines(title, 60, L.TEXT_WIDTH, 2, bold=True)
    # The title sits on the foot of its box, which ends at 248.
    win.place(ids["title"], L.GUTTER, 248 - max(count, 1) * 82)
    win.put(ids["title"], title)
    y = 248
    for piece in ("context", "meta", "left", "genres", "summary", "credits"):
        if piece not in ids:
            continue
        value = info.get(piece)
        shown = bool(value)
        if piece == "summary":
            win.visible(ids[piece], shown)
            if shown:
                count = lines(value, 30, L.TEXT_WIDTH, info.get("summary_lines", 3))
                y += gaps[piece]
                win.place(ids[piece], L.GUTTER, y)
                win.c(ids[piece]).setHeight(count * 42)
                win.text(ids[piece], clamp(value, 30, L.TEXT_WIDTH, info.get("summary_lines", 3)))
                y += count * 42
            continue
        if piece == "meta":
            win.meta(ids[piece], value or [])
            if not value:
                win.visible(ids[piece], False)
                continue
            win.visible(ids[piece], True)
            y += gaps[piece]
            win.place(ids[piece], L.GUTTER, y)
            y += 36
            continue
        win.visible(ids[piece], shown)
        if shown:
            height = {"context": 44}.get(piece, 34)
            y += gaps[piece]
            win.place(ids[piece], L.GUTTER, y)
            win.put(ids[piece], value)
            y += height
    return y + gaps["buttons"]


def meta_parts(item: dict, detail=None, lead: str = "") -> list:
    """The metadata line's pieces (TvMetaLine)."""
    found = words.meta(item, detail)
    parts = []
    if lead:
        parts.append(("lead", lead))
    if found["year"]:
        parts.append(("text", found["year"]))
    if found["content_rating"]:
        parts.append(("badge", found["content_rating"]))
    if found["runtime"]:
        parts.append(("text", found["runtime"]))
    if found["rating"]:
        parts.append(("rating", found["rating"]))
    parts += [("quality", b) for b in found["badges"]]
    return parts[: L.META_PIECES]


def about(item: dict, detail=None) -> dict:
    """What a hero or an info block says of `item` (HomeInfo)."""
    detail = detail if detail and detail.get("id") == item.get("id") else None
    episode = item.get("kind") == "episode"
    show = item.get("grandparent_title") if episode else None
    context = ""
    if episode:
        context = " · ".join(p for p in (words.episode_label(item.get("parent_index"), item.get("index")), item.get("title")) if p)
    progress = words.in_progress(item)
    return {
        "title": show or item.get("title"),
        "context": context,
        "meta": meta_parts(item, detail),
        "left": words.time_left(words.state(item).get("position_ms"), item.get("duration_ms")) if progress else "",
        "summary": ((detail or {}).get("summary") or item.get("summary") or "").strip(),
        "progress": progress,
        "watched": bool(words.state(item).get("watched")),
    }


class Home(Screen):
    """Home (TvHome): the hero for what to watch next over its backdrop,
    then the server's rows; in the rows, an info block about the tile focus
    rests on, and its backdrop."""

    name = "home"
    rail = "home"
    backdrop = True

    def __init__(self, win: Window):
        super().__init__(win)
        self.rows = Rows(win, L.HOME_ROWS, L.HOME_SLOTS, ("poster", "landscape", "square"))
        self.hero = None
        self.model = []
        self.in_rows = False
        self.about_id = None

    def load(self) -> None:
        hubs = self.session.get("/api/hubs/home", summaries=1).get("hubs", [])
        kinds = {lib["id"]: lib.get("kind") for lib in self.win.libraries}
        self.model = [
            {
                "key": hub["key"],
                "title": hub["title"],
                "shape": tiles.row_shape(hub, kinds.get(hub.get("library_id"))),
                "items": hub["items"],
                "see_all": tiles.see_all_of(hub),
            }
            for hub in hubs
            if hub.get("items")
        ]
        self.hero = tiles.hero_of(self.model)

    def instead(self):
        if self.model:
            return None
        win = self.win
        if not win.libraries and livetv.available(win.live):
            # Live TV alone on this server, or for this profile: it is Home.
            live = Live(win)
            try:
                live.load()
            except api.ApiError as e:
                kodi.log("Live TV as Home: %s" % e.message, xbmc.LOGWARNING)
            if live.lineup.get("total"):
                return live
        if not win.libraries:
            return Notice(win, T("no_libraries"), T("no_libraries_body"), retry=self)
        scanning = any(lib.get("scanning") for lib in win.libraries)
        return Notice(win, T("scanning") if scanning else T("nothing_yet"), T("nothing_yet_body"), retry=self)

    def show(self, filled: bool) -> None:
        win = self.win
        if not filled:
            self.rows.fill(self.model)
            self.draw_hero()
        if self.in_rows and filled and self.last_focus:
            found = self.rows.find(self.last_focus)
            if found:
                self.rows_mode(found[0])
                win.setFocusId(self.last_focus)
                return
        self.hero_mode()

    def draw_hero(self) -> None:
        if self.hero is None:
            return
        win = self.win
        detail = self.session.cached(self.hero["id"])
        info = about(self.hero, detail)
        ids = {"title": L.HOME_TITLE, "context": L.HOME_CONTEXT, "meta": L.HOME_META, "left": L.HOME_LEFT, "summary": L.HOME_SUMMARY}
        gaps = {"context": 20, "meta": 16, "left": 20, "summary": 20, "buttons": 40}
        top = hero_lines(win, ids, info, gaps)
        win.place(L.HOME_BUTTONS_BOX, L.GUTTER, top)
        win.put(L.HOME_PLAY, ICON_ROOM + (T("resume") if info["progress"] else T("play")))
        win.visible(L.button_box(L.HOME_FROM_START), info["progress"])
        win.visible(L.button_box(L.HOME_WATCHED), not info["watched"])
        win.visible(L.button_box(L.HOME_UNWATCHED), info["watched"])
        if detail is None:
            hero = self.hero
            win.later(lambda: self.session.detail(hero["id"]), lambda _: self.hero is hero and self.draw_hero())

    def hero_mode(self, take_focus: bool = True) -> None:
        win = self.win
        self.in_rows = False
        win.prop("pley.mode", "hero")
        win.prop("pley.scrim", "hero")
        win.visible(L.HOME_HERO, True)
        if self.hero is not None:
            win.prop("pley.backdrop", tiles.backdrop(self.session.base, self.hero))
        self.rows.place(None, 768)
        if take_focus:
            win.setFocusId(L.HOME_PLAY)

    def rows_mode(self, i: int) -> None:
        self.in_rows = True
        self.win.prop("pley.mode", "rows")
        self.win.prop("pley.scrim", "rows")
        self.rows.place(i, 0)

    def first(self) -> int:
        return L.HOME_PLAY

    def focused(self, control_id: int) -> None:
        found = self.rows.find(control_id)
        if found is not None:
            entering = not self.in_rows
            self.rows_mode(found[0])
            if entering:
                # From the hero, the info block says at once what focus is on.
                self.rested(control_id)
            else:
                self.rested_later(control_id)
        elif control_id in (L.HOME_PLAY, L.HOME_FROM_START, L.HOME_INFO, L.HOME_WATCHED, L.HOME_UNWATCHED) and self.in_rows:
            self.hero_mode()

    def moved(self, control_id: int, action: int) -> None:
        found = self.rows.find(control_id)
        if found is None:
            return
        if action == UP and found[0] == 0 and self.win.getFocusId() == control_id:
            # Up from the first row: back to the hero (its group was hidden,
            # so Kodi couldn't move there itself).
            self.win.visible(L.HOME_HERO, True)
            return self.hero_mode()
        self.rested_later(control_id)

    def rested_later(self, control_id: int) -> None:
        self.win.rest(lambda: self.rested(control_id))

    def rested(self, control_id: int) -> None:
        found = self.rows.selected(control_id)
        if found is None or self.win.getFocusId() != control_id:
            return
        _, _, item = found
        if item.get("kind") == "seeall":
            return
        self.about_id = item["id"]
        self.draw_info(item, self.session.cached(item["id"]))
        self.win.prop("pley.backdrop", tiles.backdrop(self.session.base, item))
        if self.session.cached(item["id"]) is None:
            self.win.later(lambda: self.session.detail(item["id"]), lambda detail: self.about_id == item["id"] and self.draw_info(item, detail))

    def draw_info(self, item: dict, detail) -> None:
        """The info block over the rows (TvInfoBlock)."""
        win = self.win
        info = about(item, detail)
        win.put(L.HOME_INFO_TITLE, info["title"])
        win.meta(L.HOME_INFO_META, meta_parts(item, detail, lead=info["context"]))
        win.text(L.HOME_INFO_SUMMARY, clamp(info["summary"], 27, L.TEXT_WIDTH, 2))

    def back(self) -> bool:
        if self.in_rows:
            # Back from the rows: to the hero (TvHome's BackHandler).
            self.win.visible(L.HOME_HERO, True)
            self.hero_mode()
            return True
        return False

    def click(self, control_id: int) -> None:
        win = self.win
        if control_id == L.HOME_PLAY and self.hero:
            return win.play(self.hero)
        if control_id == L.HOME_FROM_START and self.hero:
            return win.play(self.hero, from_start=True)
        if control_id == L.HOME_INFO and self.hero:
            return win.open_item(self.hero)
        if control_id in (L.HOME_WATCHED, L.HOME_UNWATCHED) and self.hero:
            try:
                win.set_watched(self.hero, control_id == L.HOME_WATCHED)
            except api.ApiError as e:
                win.toast(T("server_error") % e.message, 6)
            return
        found = self.rows.selected(control_id)
        if found is not None:
            win.open_item(found[2])

    def menu(self, control_id: int) -> None:
        found = self.rows.selected(control_id)
        if found is not None and found[2].get("kind") != "seeall":
            self.win.tile_menu(found[2], continue_watching=found[1]["key"].startswith("continue"))

    def refresh(self) -> None:
        focus = self.win.getFocusId()
        found = self.rows.selected(focus)
        self.load()
        if not self.model:
            return
        self.rows.fill(self.model)
        self.draw_hero()
        if self.in_rows and found is not None:
            # Focus stays on the tile, or its neighbour when it went.
            i, row, item = found
            at = next((n for n, r in enumerate(self.model) if r["key"] == row["key"]), None)
            if at is None:
                return self.hero_mode()
            ids = [x.get("id") for x in self.model[at]["items"]]
            position = ids.index(item.get("id")) if item.get("id") in ids else min(self.win.c(focus).getSelectedPosition(), len(ids) - 1)
            self.rows_mode(at)
            self.rows.focus(at, max(position, 0))
        elif not self.in_rows:
            # Focus in the rail stays there.
            self.hero_mode(take_focus=not self.win.focus_in_rail())


class Item(Screen):
    """An item's page (TvItemScreen): the title over its backdrop, its
    buttons, then a show's or season's episodes, Extras, Cast & Crew and
    More Like This; once focus leaves the buttons, an info block about the
    page, or the focused episode."""

    name = "item"
    backdrop = True

    def __init__(self, win: Window, item_id: int, summary=None):
        super().__init__(win)
        self.item_id = item_id
        self.summary = summary
        self.rows = Rows(win, L.ITEM_ROWS, L.ITEM_SLOTS, ("poster", "landscape", "episode", "person", "square"))
        self.detail = {}
        # An artist's albums, an album's tracks.
        self.children = []
        self.seasons = []
        self.season = None
        self.episodes = []
        self.on_deck = None
        self.related = []
        self.area = "hero"
        self.episode_about = None
        self.playable = True
        self.model = []
        self.first_y = 800
        self.first_button = L.ITEM_PLAY
        # A show's TMDB title, as GET /api/items/{id}/request says it
        # (asked once the page has drawn): More's Request more seasons.
        self.asking = None

    # -- loading

    def load(self) -> None:
        s = self.session
        known = (self.summary or {}).get("kind")
        if known:
            # The tile said what it is: its details come with the rest.
            detail = None
            kind = known
            parent = (self.summary or {}).get("parent_id")
        else:
            detail = s.detail(self.item_id, fresh=True)
            kind = detail["kind"]
            parent = detail.get("parent_id")
        calls = []
        if detail is None:
            calls.append(("detail", lambda: s.detail(self.item_id, fresh=True)))
        if kind == "show":
            calls.append(("seasons", lambda: s.get("/api/items/%d/children" % self.item_id, summaries=1).get("items", [])))
            calls.append(("on_deck", lambda: s.get("/api/items/%d/on-deck" % self.item_id).get("item")))
        elif kind == "season":
            calls.append(("episodes", lambda: s.get("/api/items/%d/children" % self.item_id, summaries=1).get("items", [])))
            calls.append(("on_deck", lambda: s.get("/api/items/%d/on-deck" % self.item_id).get("item")))
        elif kind == "episode" and parent:
            calls.append(("episodes", lambda: s.get("/api/items/%d/children" % parent, summaries=1).get("items", [])))
        elif kind in MUSIC_PAGES:
            calls.append(("children", lambda: s.get("/api/items/%d/children" % self.item_id).get("items", [])))
        if kind in ("movie", "show"):
            calls.append(("related", lambda: s.get("/api/items/%d/related" % self.item_id).get("items", [])))
        answers = dict(zip([name for name, _ in calls], api.together(*[call for _, call in calls]))) if calls else {}
        self.detail = detail or answers["detail"]
        self.related = answers.get("related") or []
        self.on_deck = answers.get("on_deck")
        self.children = answers.get("children") or []
        if kind == "artist":
            self.children = newest_first(self.children)
        if kind == "show":
            self.seasons = answers.get("seasons") or []
            wanted = (self.on_deck or {}).get("parent_id")
            self.season = next((x for x in self.seasons if x["id"] == wanted), self.seasons[0] if self.seasons else None)
            self.episodes = s.get("/api/items/%d/children" % self.season["id"], summaries=1).get("items", []) if self.season else []
        else:
            self.episodes = answers.get("episodes") or []
        self.model = self.rows_model()

    def rows_model(self) -> list:
        d = self.detail
        rows = []
        if d["kind"] == "artist" and self.children:
            rows.append({"key": "albums", "title": T("albums"), "shape": "square", "items": self.children})
        if d["kind"] == "album" and self.children:
            rows.append({"key": "tracks", "title": T("tracks_row"), "shape": "episode", "items": self.children})
        if self.episodes:
            title = words.season_title((self.season or {}).get("title"), (self.season or {}).get("index"))
            if d["kind"] in ("season", "episode"):
                title = words.season_title(None, self.episodes[0].get("parent_index"))
            rows.append({"key": "episodes", "title": "" if len(self.seasons) > 1 else title, "shape": "episode", "items": self.episodes})
        extras = d.get("extras") or []
        if extras:
            rows.append({"key": "extras", "title": T("extras"), "shape": "landscape", "items": extras})
        credits = cast_and_crew(d)
        if credits:
            crew = any(c.get("job") for c in credits)
            cast = any(not c.get("job") for c in credits)
            episode = d["kind"] == "episode"
            if not crew:
                title = T("guest_stars") if episode else T("cast")
            elif not cast:
                title = T("crew")
            else:
                title = T("cast_and_crew")
            rows.append({"key": "cast", "title": title, "shape": "person", "items": credits})
        if self.related:
            rows.append({"key": "related", "title": T("more_like_this"), "shape": "poster", "items": self.related})
        return rows

    # -- drawing

    def primary(self) -> tuple:
        """(label, item to play, resumes): Resume, Play, "Resume S1 · E3"."""
        d = self.detail
        if d["kind"] in ("movie", "episode", "extra"):
            return (T("resume") if words.in_progress(d) else T("play")), d, words.in_progress(d)
        if d["kind"] in MUSIC_PAGES:
            return (T("play"), d, False) if self.children else (T("unavailable"), None, False)
        target = self.on_deck
        if not target:
            return T("unavailable"), None, False
        label = words.episode_label(target.get("parent_index"), target.get("index"))
        if words.in_progress(target):
            return T("resume_episode") % label, target, True
        return T("play_episode") % label, target, False

    def show(self, filled: bool) -> None:
        if not filled:
            self.rows.fill(self.model)
            self.draw()
        self.win.prop("pley.backdrop", tiles.backdrop(self.session.base, self.detail))
        if filled and self.last_focus and self.area != "hero":
            self.restore()
        else:
            self.hero_mode()
        if not filled:
            self.ask_about()

    def ask_about(self) -> None:
        """Once the page has drawn, never before: what of a show may still
        be asked for, where this profile may ask."""
        d = self.detail
        if d.get("kind") != "show" or not requests.may_ask(self.win.requests_summary(ask=False)):
            self.asking = None
            return

        def took(found):
            if self.detail is d:
                self.asking = found if isinstance(found, dict) else None

        # 404: the show has no TMDB match, and nothing to ask for.
        self.win.later(lambda: self.session.get("/api/items/%d/request" % d["id"]), took)

    def draw(self) -> None:
        win, d = self.win, self.detail
        kind = d["kind"]
        show_title = d.get("grandparent_title") if kind == "episode" else d.get("parent_title") if kind == "season" else None
        context = ""
        if kind == "episode":
            context = " · ".join(p for p in (words.episode_label(d.get("parent_index"), d.get("index")), d.get("title")) if p)
        elif kind == "season" and show_title:
            context = d.get("title") or words.season_title(None, d.get("index"))
        elif kind == "album":
            context = d.get("parent_title") or ""
        music = kind in MUSIC_PAGES
        label, target, resumes = self.primary()
        genres = list(d.get("genres") or [])
        info = {
            "title": show_title or d.get("title"),
            "context": context,
            "meta": self.meta_line(),
            "left": words.time_left(words.state(target).get("position_ms"), target.get("duration_ms")) if resumes and target else "",
            "genres": " · ".join(genres),
            "summary": (d.get("summary") or "").strip(),
            "credits": credits_line(d),
        }
        ids = {
            "title": L.ITEM_TITLE,
            "context": L.ITEM_CONTEXT,
            "meta": L.ITEM_META,
            "left": L.ITEM_LEFT,
            "genres": L.ITEM_GENRES,
            "summary": L.ITEM_SUMMARY,
            "credits": L.ITEM_CREDITS,
        }
        gaps = {"context": 16, "meta": 24, "left": 16, "genres": 12, "summary": 20, "credits": 16, "buttons": 44}
        top = hero_lines(win, ids, info, gaps)
        win.place(L.ITEM_BUTTONS_BOX, L.GUTTER, top)
        self.first_y = top + L.BUTTON + L.BUTTON_LABEL_ROOM + 16 + L.ROW_GAP
        win.put(L.ITEM_PLAY, ICON_ROOM + label)
        self.playable = target is not None
        win.c(L.ITEM_PLAY).setEnabled(self.playable)
        watched = bool(words.state(d).get("watched"))
        listable = kind in ("movie", "show")
        # Music is played and added to playlists; it isn't marked watched.
        shown = {
            L.ITEM_PLAY: self.playable,
            L.ITEM_SHUFFLE: music and self.playable,
            L.ITEM_FROM_START: resumes,
            L.ITEM_WATCHED: not music and not watched,
            L.ITEM_UNWATCHED: not music and watched,
            L.ITEM_ADD_TO_LIST: listable and not d.get("in_watchlist"),
            L.ITEM_IN_LIST: listable and bool(d.get("in_watchlist")),
            L.ITEM_TRAILER: self.trailer() is not None,
            L.ITEM_ADD_TO_PLAYLIST: kind in PLAYLIST_KINDS,
            L.ITEM_MORE: bool(self.more_actions()),
        }
        for button, on in shown.items():
            if button != L.ITEM_PLAY:
                win.visible(L.button_box(button), on)
        # Where focus goes on the hero: Play, else the first button there is.
        self.first_button = next((b for b in HERO_BUTTONS if shown.get(b)), L.ITEM_PLAY)
        # Season tabs, for a show of more than one season.
        tabs = win.c(L.ITEM_SEASONS)
        tabs.reset()
        if len(self.seasons) > 1:
            entries = []
            for season in self.seasons:
                li = xbmcgui.ListItem(words.season_title(season.get("title"), season.get("index")), offscreen=True)
                li.select(season is self.season)
                entries.append(li)
            tabs.addItems(entries)
            tabs.selectItem(self.seasons.index(self.season) if self.season in self.seasons else 0)
            tabs.setVisible(True)
        else:
            tabs.setVisible(False)
        # The episodes row starts on the one to watch (or this episode).
        if self.episodes:
            current = self.item_id if kind == "episode" else (self.on_deck or {}).get("id")
            ids_ = [e["id"] for e in self.episodes]
            if current in ids_:
                win.c(L.slot_list(L.ITEM_ROWS, 0, "episode")).selectItem(ids_.index(current))

    def meta_line(self) -> list:
        d = self.detail
        found = words.music_meta(d, self.children) if d["kind"] in MUSIC_PAGES else words.item_meta(d, self.seasons)
        return found[: L.META_PIECES]

    def trailer(self):
        return next((x for x in self.detail.get("extras") or [] if x.get("extra_type") == "trailer"), None)

    def more_actions(self) -> list:
        d = self.detail
        actions = []
        summary = (d.get("summary") or "").strip()
        if summary and lines(summary, 30, L.TEXT_WIDTH, 4) > 3 or d.get("tagline"):
            actions.append((T("full_summary"), self.full_summary))
        if d["kind"] == "episode" and d.get("grandparent_id"):
            actions.append((T("go_to_show"), lambda: self.win.open(Item(self.win, d["grandparent_id"]))))
        if d["kind"] == "episode" and d.get("parent_id"):
            actions.append((T("go_to_season"), lambda: self.win.open(Item(self.win, d["parent_id"]))))
        if d["kind"] == "season" and d.get("parent_id"):
            actions.append((T("go_to_show"), lambda: self.win.open(Item(self.win, d["parent_id"]))))
        if d["kind"] == "album" and d.get("parent_id"):
            actions.append((T("go_to_artist"), lambda: self.win.open(Item(self.win, d["parent_id"]))))
        if self.asking is not None and self.asking.get("askable") != []:
            # The plugin's picker (null: TMDB's seasons weren't known yet).
            title = d.get("title") or ""
            actions.append((T("request_more_seasons"), lambda: run_plugin(route="more_seasons", id=d["id"], title=title)))
        actions.append((T("your_rating"), self.rate))
        return actions

    def rate(self) -> None:
        """Your rating (the web's StarRating): five stars, halves allowed,
        0 to 10 on the server; Not rated clears it."""
        d = self.detail
        saved = words.state(d).get("rating")
        values = [None] + list(range(1, 11))
        labels = [T("not_rated")] + [words.stars(v) for v in values[1:]]
        at = values.index(round(saved)) if saved and round(saved) in values else 0
        chosen = xbmcgui.Dialog().select(T("your_rating"), labels, preselect=at)
        if chosen < 0 or values[chosen] == (round(saved) if saved else None):
            return
        self.session.client.put("/api/items/%d/rating" % d["id"], {"rating": values[chosen]})
        d.setdefault("user_state", {})["rating"] = values[chosen]
        self.win.toast(T("saved"))

    def full_summary(self) -> None:
        d = self.detail
        text = "\n\n".join(p for p in (d.get("tagline"), d.get("summary")) if p)
        xbmcgui.Dialog().textviewer(d.get("title") or "", text)

    def hero_mode(self) -> None:
        win = self.win
        self.area = "hero"
        win.prop("pley.mode", "hero")
        win.prop("pley.scrim", "item")
        win.visible(L.ITEM_HERO, True)
        self.rows.place(None, self.first_y)
        self.place_tabs(self.first_y)
        win.setFocusId(self.first_button)

    def place_tabs(self, episodes_y: int) -> None:
        if len(self.seasons) > 1 and self.rows.rows and self.rows.rows[0]["key"] == "episodes":
            self.win.place(L.ITEM_SEASONS, L.GUTTER, episodes_y - 32)

    def rows_mode(self, i: int) -> None:
        win = self.win
        win.prop("pley.mode", "rows")
        win.prop("pley.scrim", "rows")
        row = self.rows.rows[i]
        if row["key"] == "episodes":
            self.area = "episodes"
            # The episodes under the info block, as the spec puts them (y = 272 dp).
            self.rows.place(None, 468)
            self.place_tabs(468)
        else:
            self.area = "rows"
            self.rows.place(i, 0)
            self.win.place(L.ITEM_SEASONS, L.GUTTER, -2 * L.HEIGHT)
            self.draw_info(None)

    def draw_info(self, episode) -> None:
        """The info block: the page's title and metadata, then the focused
        episode's title, its line and summary, or the page's summary."""
        win, d = self.win, self.detail
        kind = d["kind"]
        show_title = d.get("grandparent_title") if kind == "episode" else d.get("parent_title") if kind == "season" else None
        win.put(L.ITEM_INFO_TITLE, show_title or d.get("title"))
        if episode is not None:
            win.meta(L.ITEM_INFO_META, self.meta_line())
            win.put(L.ITEM_INFO_EPISODE, episode.get("title"))
            line = " · ".join(p for p in (words.episode_long_label(episode.get("parent_index"), episode.get("index")), words.runtime(episode.get("duration_ms"))) if p)
            win.put(L.ITEM_INFO_LINE, line)
            win.visible(L.ITEM_INFO_EPISODE, True)
            win.visible(L.ITEM_INFO_LINE, True)
            win.place(L.ITEM_INFO_SUMMARY, L.GUTTER, 326)
            win.text(L.ITEM_INFO_SUMMARY, clamp(episode.get("summary") or "", 30, L.TEXT_WIDTH, 2))
        else:
            win.meta(L.ITEM_INFO_META, self.meta_line())
            win.visible(L.ITEM_INFO_EPISODE, False)
            win.visible(L.ITEM_INFO_LINE, False)
            win.place(L.ITEM_INFO_SUMMARY, L.GUTTER, 216)
            win.text(L.ITEM_INFO_SUMMARY, clamp(d.get("summary") or "", 30, L.TEXT_WIDTH, 2))

    def restore(self) -> None:
        found = self.rows.find(self.last_focus)
        if found is None:
            return self.hero_mode()
        self.rows_mode(found[0])
        self.win.setFocusId(self.last_focus)

    def first(self) -> int:
        return L.ITEM_PLAY

    def focused(self, control_id: int) -> None:
        if control_id == L.ITEM_SEASONS:
            self.rows_mode(0)
            self.draw_info(None)
            return
        found = self.rows.find(control_id)
        if found is not None:
            self.rows_mode(found[0])
            if found[1]["key"] == "episodes":
                self.win.rest(lambda: self.rested(control_id))
            return
        if control_id in HERO_BUTTONS:
            if self.area != "hero":
                self.hero_mode()
                self.win.setFocusId(control_id)

    def rested(self, control_id: int) -> None:
        found = self.rows.selected(control_id)
        if found is not None and found[1]["key"] == "episodes" and self.win.getFocusId() == control_id:
            self.draw_info(found[2])

    def moved(self, control_id: int, action: int) -> None:
        if control_id == L.ITEM_SEASONS and action in (LEFT, RIGHT):
            return self.win.rest(self.pick_season)
        found = self.rows.find(control_id)
        if found is None:
            if control_id == L.ITEM_SEASONS and action == UP:
                self.up_to_hero()
            return
        if action == UP and self.win.getFocusId() == control_id:
            # Up from the first row there is (the hero's group was hidden).
            if found[0] == 0 and len(self.seasons) <= 1:
                return self.up_to_hero()
        if found[1]["key"] == "episodes":
            self.win.rest(lambda: self.rested(control_id))

    def up_to_hero(self) -> None:
        self.win.visible(L.ITEM_HERO, True)
        self.hero_mode()

    def pick_season(self) -> None:
        tabs = self.win.c(L.ITEM_SEASONS)
        at = tabs.getSelectedPosition()
        if not (0 <= at < len(self.seasons)) or self.seasons[at] is self.season:
            return
        self.season = self.seasons[at]
        for n in range(tabs.size()):
            tabs.getListItem(n).select(n == at)
        self.episodes = self.session.get("/api/items/%d/children" % self.season["id"], summaries=1).get("items", [])
        self.model = self.rows_model()
        self.rows.fill(self.model)
        self.place_tabs(468)
        self.rows.place(None, 468)

    def back(self) -> bool:
        if self.area != "hero":
            self.up_to_hero()
            return True
        return False

    def click(self, control_id: int) -> None:
        win, d = self.win, self.detail
        try:
            if control_id == L.ITEM_PLAY:
                _, target, _ = self.primary()
                if target is not None:
                    win.play(target)
            elif control_id == L.ITEM_SHUFFLE:
                win.play_music(d, shuffle=True, tracks=self.children if d["kind"] == "album" else None)
            elif control_id == L.ITEM_FROM_START:
                _, target, _ = self.primary()
                if target is not None:
                    win.play(target, from_start=True)
            elif control_id in (L.ITEM_WATCHED, L.ITEM_UNWATCHED):
                win.set_watched(d, control_id == L.ITEM_WATCHED)
            elif control_id in (L.ITEM_ADD_TO_LIST, L.ITEM_IN_LIST):
                win.set_listed(d, control_id == L.ITEM_ADD_TO_LIST)
            elif control_id == L.ITEM_TRAILER and self.trailer():
                win.play(self.trailer(), from_start=True)
            elif control_id == L.ITEM_ADD_TO_PLAYLIST:
                win.add_to_playlist(d)
            elif control_id == L.ITEM_MORE:
                actions = self.more_actions()
                chosen = xbmcgui.Dialog().contextmenu([name for name, _ in actions]) if actions else -1
                if chosen >= 0:
                    actions[chosen][1]()
            elif control_id == L.ITEM_SEASONS:
                self.pick_season()
            else:
                found = self.rows.selected(control_id)
                if found is not None:
                    _, row, item = found
                    if row["key"] == "episodes":
                        win.open(Item(win, item["id"], item))
                    elif row["key"] == "tracks":
                        # The album plays on from the track.
                        win.play_music(d, at=self.children.index(item), tracks=self.children)
                    else:
                        win.open_item(item)
        except api.ApiError as e:
            win.toast(T("server_error") % e.message, 6)

    def menu(self, control_id: int) -> None:
        found = self.rows.selected(control_id)
        if found is not None and found[1]["key"] != "cast":
            self.win.tile_menu(found[2])

    def refresh(self) -> None:
        focus = self.win.getFocusId()
        area = self.area
        self.load()
        self.rows.fill(self.model)
        self.draw()
        if area == "hero":
            self.hero_mode()
            if focus and focus != L.ITEM_PLAY:
                self.win.setFocusId(focus)
        else:
            self.last_focus = focus
            self.restore()
        self.ask_about()


def cast_and_crew(detail: dict) -> list:
    """Cast, then the directors, creators and writers (as people with a job)."""
    jobs = {"director": "Director", "creator": "Creator", "writer": "Writer"}
    found = [dict(c) for c in detail.get("cast") or []]
    for credit in detail.get("crew") or []:
        if credit.get("role") in jobs:
            entry = dict(credit)
            entry["job"] = jobs[credit["role"]]
            found.append(entry)
    return found[:40]


def credits_line(detail: dict) -> str:
    """"Directed by Lena Okafor · Starring Idris Vale, Maren Holt"."""
    crew = detail.get("crew") or []
    makers = [c for c in crew if c.get("role") == "director"] or [c for c in crew if c.get("role") == "creator"]
    stars = (detail.get("cast") or [])[:3]
    parts = []
    if makers:
        made = T("directed_by") if makers[0].get("role") == "director" else T("created_by")
        parts.append("%s %s" % (color(made, L.MUTED), ", ".join(c["name"] for c in makers[:2])))
    if stars:
        parts.append("%s %s" % (color(T("starring"), L.MUTED), ", ".join(c["name"] for c in stars)))
    return color(" · ", L.SUBTLE).join(parts)


class Library(Screen):
    """A library (TvLibraryScreen): Recommended (its rows), Library (every
    title, sorted and filtered, in a grid) and Collections."""

    name = "library"
    backdrop = False

    def __init__(self, win: Window, library: dict, rail_key):
        super().__init__(win)
        self.library = library
        self.rail = rail_key or library_rail(win.libraries, library["id"], livetv.available(win.live))
        # It opens on the tab it was last left on, on this Kodi, for this
        # server and profile (as on Android TV, Samsung and LG); on Library
        # the first time, and when that tab isn't the library's.
        key = self.remembered_as()
        kept = kodi.remembered(key, "all") if key else "all"
        self.tab = kept if kept in self.tabs() else "all"
        self.sort = "title"
        self.descending = False
        self.watched = None  # all, False: unwatched, True: watched
        self.genre = None
        self.year = None
        self.items = []
        self.total = 0
        self.collections = None
        self.hubs = None
        self.rows = Rows(win, L.LIBRARY_ROWS, L.LIBRARY_SLOTS, ("poster", "landscape", "square"))
        self.filters = None
        self.loading = False

    def tabs(self) -> tuple:
        """Recommended, Library and Collections; music has no Collections."""
        if self.library.get("kind") == "music":
            return ("recommended", "all")
        return ("recommended", "all", "collections")

    def remembered_as(self):
        """Where this library's tab is remembered: the server, the profile
        and the library (None without a profile: nothing is kept)."""
        server = kodi.setting("server_id") or kodi.setting("server_url")
        profile = self.win.me.get("id")
        if not server or profile is None:
            return None
        return "library_tab/%s/%s/%s" % (server, profile, self.library["id"])

    @property
    def shape(self) -> str:
        return "square" if self.library.get("kind") == "music" else "grid"

    def query(self, offset: int) -> dict:
        q = {"sort": self.sort, "order": "desc" if self.descending else "asc", "offset": offset, "limit": PAGE}
        if self.watched is not None:
            q["watched"] = "true" if self.watched else "false"
        if self.genre:
            q["genre"] = self.genre
        if self.year:
            q["year"] = self.year
        return q

    @property
    def filtered(self) -> bool:
        return self.watched is not None or bool(self.genre) or bool(self.year)

    def chips(self) -> list:
        """(chip, whether it is on, its label when on) for Unwatched, Genre
        and Year."""
        return [
            (L.LIBRARY_UNWATCHED, self.watched is False, T("unwatched")),
            (L.LIBRARY_GENRE, bool(self.genre), self.genre or ""),
            (L.LIBRARY_YEAR, bool(self.year), str(self.year or "")),
        ]

    def refocus(self, chip: int) -> None:
        """Focus back on [chip], or its twin where that is the one shown."""
        on = next(on for c, on, _ in self.chips() if c == chip)
        self.win.setFocusId(chip + 100 if on else chip)

    def smart_filter(self) -> str:
        """The filters and sort as a smart playlist's filter, in Plex's
        syntax (SmartFilter.of): what Save as… saves."""
        parts = []
        if self.genre:
            parts.append(("genre", self.genre))
        if self.year:
            parts.append(("year", str(self.year)))
        if self.watched is not None:
            parts.append(("unwatched", "0" if self.watched else "1"))
        field = PLEX_SORTS[self.sort]
        parts.append(("sort", field + ":desc" if self.descending else field))
        return "&".join("%s=%s" % (quote_plus(k), quote_plus(v)) for k, v in parts)

    def load(self) -> None:
        lib = self.library["id"]
        if self.tab == "all":
            page = self.session.get("/api/libraries/%d/items" % lib, **self.query(0))
            self.items = page.get("items", [])
            self.total = page.get("total", len(self.items))
        elif self.tab == "recommended":
            hubs = self.session.get("/api/libraries/%d/hubs" % lib).get("hubs", [])
            self.hubs = [
                {"key": h["key"], "title": h["title"], "shape": tiles.row_shape(h, self.library.get("kind")), "items": h["items"], "see_all": None}
                for h in hubs
                if h.get("items")
            ]
        else:
            self.collections = self.session.get("/api/libraries/%d/collections" % lib).get("collections", [])

    def show(self, filled: bool) -> None:
        win = self.win
        win.put(L.LIBRARY_TITLE, self.library.get("name"))
        win.prop("pley.library.tab", self.tab)
        for tab, id_ in (("recommended", L.LIBRARY_RECOMMENDED), ("all", L.LIBRARY_ALL), ("collections", L.LIBRARY_COLLECTIONS)):
            offered = tab in self.tabs()
            win.visible(id_, offered and tab != self.tab)
            win.visible(id_ + 100, offered and tab == self.tab)
        if not filled:
            self.draw()
        if filled and self.last_focus:
            win.setFocusId(self.last_focus)
        else:
            win.setFocusId(self.first())

    def draw(self) -> None:
        win = self.win
        for shape in L.GRID_SHAPES:
            win.visible(L.grid_list(L.LIBRARY_GRID, shape), False)
        win.put(L.LIBRARY_EMPTY, "")
        if self.tab == "all":
            sort_name = T(SORT_NAMES[self.sort])
            win.visible(L.LIBRARY_SORT, not self.descending)
            win.visible(L.LIBRARY_SORT_DOWN, self.descending)
            for control in (L.LIBRARY_SORT, L.LIBRARY_SORT_DOWN):
                win.put(control, ICON_ROOM + sort_name)
            # Music only sorts (TvLibraryScreen's chips); a filter that is on
            # shows its twin, with a check.
            music = self.library.get("kind") == "music"
            win.visible(L.LIBRARY_FILTER, not music)
            win.put(L.LIBRARY_FILTER, ICON_ROOM + T("filter"))
            for chip, on, text in self.chips():
                win.visible(chip, not music and not on)
                win.visible(chip + 100, not music and on)
                win.put(chip + 100, ICON_ROOM + text)
            noun = LIBRARY_NOUNS.get(self.library.get("kind"), ("item", "items"))
            every = self.library.get("item_count")
            if self.filtered and every is not None and every >= self.total:
                # "23 of 342 movies"
                win.put(L.LIBRARY_COUNT, T("count_of") % ("{:,}".format(self.total), words.plural(every, *noun)))
            else:
                win.put(L.LIBRARY_COUNT, words.plural(self.total, *noun))
            grid = win.c(L.grid_list(L.LIBRARY_GRID, self.shape))
            grid.reset()
            grid.addItems([tiles.tile(self.session.base, item, self.shape) for item in self.items])
            win.visible(L.grid_list(L.LIBRARY_GRID, self.shape), bool(self.items))
            if not self.items:
                win.put(L.LIBRARY_EMPTY, T("no_matches") if self.filtered else T("library_empty"))
        elif self.tab == "recommended":
            self.rows.fill(self.hubs or [])
            self.rows.place(None, 200)
            if not self.hubs:
                win.put(L.LIBRARY_EMPTY, T("nothing_yet"))
        else:
            grid = win.c(L.grid_list(L.LIBRARY_GRID, "grid"))
            grid.reset()
            grid.addItems([collection_tile(self.session.base, c) for c in self.collections or []])
            win.visible(L.grid_list(L.LIBRARY_GRID, "grid"), bool(self.collections))
            if not self.collections:
                win.put(L.LIBRARY_EMPTY, T("no_collections"))

    def first(self) -> int:
        if self.tab == "recommended" and self.hubs:
            return L.slot_list(L.LIBRARY_ROWS, 0, self.hubs[0]["shape"])
        grid = L.grid_list(L.LIBRARY_GRID, self.shape if self.tab == "all" else "grid")
        if (self.tab == "all" and self.items) or (self.tab == "collections" and self.collections):
            return grid
        return L.LIBRARY_ALL + (100 if self.tab == "all" else 0)

    def switch(self, tab: str) -> None:
        if tab == self.tab:
            return
        self.tab = tab
        key = self.remembered_as()
        if key:
            kodi.remember(key, tab)
        try:
            self.load()
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        self.show(False)

    def reload(self) -> None:
        try:
            self.load()
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        self.draw()

    def click(self, control_id: int) -> None:
        win = self.win
        tabs = {L.LIBRARY_RECOMMENDED: "recommended", L.LIBRARY_ALL: "all", L.LIBRARY_COLLECTIONS: "collections"}
        if control_id in tabs or control_id - 100 in tabs:
            return self.switch(tabs.get(control_id) or tabs[control_id - 100])
        if control_id in (L.LIBRARY_SORT, L.LIBRARY_SORT_DOWN):
            names = [T(SORT_NAMES[s]) for s in SORTS]
            chosen = xbmcgui.Dialog().select(T("sort_by"), names, preselect=SORTS.index(self.sort))
            if chosen < 0:
                return
            if SORTS[chosen] == self.sort:
                self.descending = not self.descending
            else:
                self.sort = SORTS[chosen]
                self.descending = self.sort not in ("title",)
            self.reload()
            return win.setFocusId(L.LIBRARY_SORT_DOWN if self.descending else L.LIBRARY_SORT)
        if control_id == L.LIBRARY_FILTER:
            return self.filter_menu()
        if control_id in (L.LIBRARY_UNWATCHED, L.LIBRARY_UNWATCHED_ON):
            self.watched = None if self.watched is False else False
            self.reload()
            return self.refocus(L.LIBRARY_UNWATCHED)
        if control_id in (L.LIBRARY_GENRE, L.LIBRARY_GENRE_ON):
            self.pick_genre()
            return self.refocus(L.LIBRARY_GENRE)
        if control_id in (L.LIBRARY_YEAR, L.LIBRARY_YEAR_ON):
            self.pick_year()
            return self.refocus(L.LIBRARY_YEAR)
        if self.tab == "recommended":
            found = self.rows.selected(control_id)
            if found is not None:
                win.open_item(found[2])
            return
        grid = win.c(control_id) if control_id in [L.grid_list(L.LIBRARY_GRID, s) for s in L.GRID_SHAPES] else None
        if grid is None:
            return
        at = grid.getSelectedPosition()
        if self.tab == "all" and 0 <= at < len(self.items):
            win.open_item(self.items[at])
        elif self.tab == "collections" and self.collections and 0 <= at < len(self.collections):
            win.open(Grid.collection(win, self.collections[at]))

    def filter_menu(self) -> None:
        """Filter (TvLibraryPanel's): which titles to show, the genre and
        the year; then Clear filters and Save as…"""
        shows = ((None, T("all")), (False, T("unwatched")), (True, T("mark_watched_short")))
        entries = []
        for watched, name in shows:
            li = xbmcgui.ListItem(name, offscreen=True)
            li.select(self.watched is watched)
            entries.append(li)
        # Kodi's select list draws one label: the value goes in it.
        entries.append(xbmcgui.ListItem("%s: %s" % (T("genre"), self.genre or T("any_genre")), offscreen=True))
        entries.append(xbmcgui.ListItem("%s: %s" % (T("year"), self.year or T("any_year")), offscreen=True))
        if self.filtered:
            entries.append(xbmcgui.ListItem(T("clear_filters"), offscreen=True))
            entries.append(xbmcgui.ListItem(T("save_as"), offscreen=True))
        current = next(i for i, (watched, _) in enumerate(shows) if self.watched is watched)
        chosen = xbmcgui.Dialog().select(T("filter"), entries, preselect=current)
        if chosen < 0:
            return
        if chosen < len(shows):
            self.watched = shows[chosen][0]
            return self.reload()
        if chosen == len(shows):
            return self.pick_genre()
        if chosen == len(shows) + 1:
            return self.pick_year()
        if chosen == len(shows) + 2:
            self.watched, self.genre, self.year = None, None, None
            return self.reload()
        self.save_as()

    def filter_options(self) -> dict:
        if self.filters is None:
            self.filters = self.session.get("/api/libraries/%d/filters" % self.library["id"])
        return self.filters

    def pick_genre(self) -> None:
        try:
            options = self.filter_options().get("genres") or []
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        current = options.index(self.genre) + 1 if self.genre in options else 0
        chosen = xbmcgui.Dialog().select(T("genre"), [T("any_genre")] + options, preselect=current)
        if chosen >= 0:
            self.genre = options[chosen - 1] if chosen else None
            self.reload()

    def pick_year(self) -> None:
        try:
            options = [str(y) for y in self.filter_options().get("years") or []]
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        current = options.index(str(self.year)) + 1 if str(self.year) in options else 0
        chosen = xbmcgui.Dialog().select(T("year"), [T("any_year")] + options, preselect=current)
        if chosen >= 0:
            self.year = int(options[chosen - 1]) if chosen else None
            self.reload()

    def save_as(self) -> None:
        """Save these filters: a smart playlist of this library that keeps
        up with it by itself; it opens once made."""
        title = xbmcgui.Dialog().input(T("save_filters")).strip()
        if not title:
            return
        body = {"title": title, "smart_filter": self.smart_filter(), "library_id": self.library["id"]}
        try:
            made = self.session.client.post("/api/playlists", body)
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        self.win.open(Playlist(self.win, made))

    def moved(self, control_id: int, action: int) -> None:
        if self.tab != "all" or control_id != L.grid_list(L.LIBRARY_GRID, self.shape) or self.loading:
            return
        at = self.win.c(control_id).getSelectedPosition()
        if at >= len(self.items) - 3 * 7 and len(self.items) < self.total:
            self.more()

    def more(self) -> None:
        """The next titles, before focus reaches the grid's end."""
        self.loading = True
        offset = len(self.items)
        lib = self.library["id"]

        def done(page):
            self.loading = False
            found = page.get("items", [])
            if not found or len(self.items) != offset:
                return
            self.items += found
            self.win.c(L.grid_list(L.LIBRARY_GRID, self.shape)).addItems([tiles.tile(self.session.base, item, self.shape) for item in found])

        def failed():
            # Asked again as focus moves on.
            self.loading = False

        self.win.later(lambda: self.session.get("/api/libraries/%d/items" % lib, **self.query(offset)), done, failed)

    def menu(self, control_id: int) -> None:
        if self.tab == "all" and control_id == L.grid_list(L.LIBRARY_GRID, self.shape):
            at = self.win.c(control_id).getSelectedPosition()
            if 0 <= at < len(self.items):
                self.win.tile_menu(self.items[at])
        elif self.tab == "recommended":
            found = self.rows.selected(control_id)
            if found is not None:
                self.win.tile_menu(found[2], continue_watching=found[1]["key"].startswith("continue"))

    def refresh(self) -> None:
        focus = self.win.getFocusId()
        at = self.win.c(focus).getSelectedPosition() if focus in [L.grid_list(L.LIBRARY_GRID, s) for s in L.GRID_SHAPES] else -1
        self.load()
        self.draw()
        if at >= 0:
            self.win.setFocusId(focus)
            self.win.c(focus).selectItem(min(at, max(len(self.items) - 1, 0)))


def collection_tile(base: str, collection: dict) -> xbmcgui.ListItem:
    li = xbmcgui.ListItem(collection.get("title") or "", words.plural(collection["item_count"], "title") if collection.get("item_count") is not None else "", offscreen=True)
    # Without a poster of its own, the server sends its titles' (a redirect).
    url = collection.get("poster_url") or "/api/collections/%d/images/poster" % collection["id"]
    found = image(base, url, tiles.WIDTHS["grid"])
    if found:
        li.setArt({"tile": found})
    li.setProperty("ph", words.placeholder(collection.get("title") or ""))
    return li


class Search(Screen):
    """Search (TvSearchScreen): the field, then what was found, by kind;
    where this profile may ask, Discover after them: titles
    the libraries don't have, looked for beside their search and never
    waited for, their row coming once they do, or saying why TMDB refused."""

    name = "search"
    rail = "search"

    def __init__(self, win: Window):
        super().__init__(win)
        self.query = ""
        self.found = []
        # The Discover row's titles (None: not looked for, or on their way), why TMDB
        # refused (requests.problem), whether they are on their way, and the
        # searches made (an older one's titles come too late).
        self.tmdb = None
        self.refused = None
        self.looking = False
        self.asked = 0
        self.rows = Rows(win, L.SEARCH_ROWS, L.SEARCH_SLOTS, ("poster", "episode", "person", "square", "channel"))

    def show(self, filled: bool) -> None:
        win = self.win
        win.put(L.SEARCH_FIELD, self.query or color(T("search_hint"), L.MUTED))
        if not filled:
            self.draw()
        win.setFocusId(self.last_focus if filled and self.last_focus else L.SEARCH_FIELD)

    def model(self) -> list:
        """The libraries' rows, then Discover (in a slot of its own)."""
        return self.found + ([self.tmdb_row()] if self.tmdb or self.refused else [])

    def tmdb_row(self) -> dict:
        """The Discover row, its See all opening Discover's results for the
        words. TMDB refused: why, in its
        title, and Try again where it may help; else See all, where the
        results' page says more."""
        base = self.session.base
        row = {
            "key": "tmdb",
            "title": T("discover"),
            "shape": "poster",
            "items": self.tmdb or [],
            "tile": lambda title: tiles.discover(base, title, tmdb_caption(title)),
            "see_all": self.query,
        }
        if self.refused:
            title, _, retry = self.refused
            row["title"] += " · " + color(title, L.DANGER_TEXT)
            if retry:
                row["see_all_label"] = T("try_again")
        return row

    def draw(self) -> None:
        self.rows.fill(self.model())
        self.place(self.focused_row())
        self.say()

    def focused_row(self):
        found = self.rows.find(self.win.getFocusId())
        return found[0] if found is not None else None

    def say(self) -> None:
        """What the page says over its rows: what to look for; that nothing
        was found; that the libraries have nothing, over Discover."""
        if not self.query:
            title, body = T("search_empty"), T("search_empty_body")
        elif self.found:
            title, body = "", ""
        elif self.tmdb or self.refused or self.looking:
            title, body = T("nothing_in_libraries"), ""
        else:
            title, body = T("nothing_found_for") % self.query, ""
        self.win.put(L.SEARCH_EMPTY_TITLE, title)
        self.win.put(L.SEARCH_EMPTY_BODY, body)

    def top(self) -> int:
        """Where the rows start: under the field (not an info block), or
        under "Nothing in your libraries matches"."""
        return 320 if self.query and not self.found else 200

    def place(self, focused=None) -> None:
        """The rows down from the top; with row `focused` focused, from it,
        the ones above out of sight."""
        y = self.top()
        for i, row in enumerate(self.rows.rows):
            group = L.slot(L.SEARCH_ROWS, i)
            if focused is not None and i < focused:
                self.win.place(group, 0, -2 * L.HEIGHT)
                continue
            self.win.place(group, 0, y)
            y += L.row_height(L.SHAPES[row["shape"]]) + L.ROW_GAP

    def search(self, query: str, again: bool = False) -> None:
        """What `query` finds; `again`: the same search, the Discover row kept
        until its titles come again."""
        self.query = query.strip()
        self.found = []
        self.asked += 1
        self.looking = False
        asks = bool(self.query) and requests.may_ask(self.win.requests_summary(ask=False))
        if not again or not asks:
            self.tmdb = None
            self.refused = None
        if self.query:
            if asks:
                self.look_on_tmdb()
            try:
                answer = self.session.get("/api/search", q=self.query, limit=100, summaries=1)
            except api.ApiError:
                # The page stays as it was: the Discover row's titles aren't put under it.
                self.asked += 1
                raise
            items = answer.get("items", [])
            by_kind = lambda kinds: [i for i in items if i.get("kind") in kinds]  # noqa: E731
            for title, kinds, shape in (
                (T("movies"), ("movie",), "poster"),
                (T("shows"), ("show", "season"), "poster"),
                (T("episodes"), ("episode",), "episode"),
                (T("music"), ("artist", "album", "track"), "square"),
            ):
                found = by_kind(kinds)
                if found:
                    self.found.append({"key": title, "title": title, "shape": shape, "items": found})
            # After the titles, Live TV's channels, by name, number or what's
            # on (a server with Live TV open to this profile), then people,
            # as on the other apps; the closest matches first in each.
            channels = answer.get("channels") or []
            if channels:
                self.found.append({"key": "channels", "title": T("live_tv"), "shape": "channel", "items": channels})
            people = answer.get("people") or []
            if people:
                self.found.append({"key": "people", "title": T("people"), "shape": "person", "items": people})
        self.win.put(L.SEARCH_FIELD, self.query or color(T("search_hint"), L.MUTED))
        self.draw()

    def look_on_tmdb(self) -> None:
        """GET /api/discover/search?missing=1, beside the libraries' search,
        for the words as the server takes them."""
        asked, query = self.asked, requests.search_text(self.query)
        self.looking = True

        def look():
            try:
                return self.session.get("/api/discover/search", q=query, missing=1)
            except api.ApiError as e:
                if e.code in AWAY:
                    raise
                # Refused (TMDB away, its key, no access): said in the Discover row.
                return e

        def took(answer, found=None):
            if asked != self.asked:
                return
            refused = answer if isinstance(answer, api.ApiError) else None
            if found is None:
                found = [] if refused else [t for t in (answer or {}).get("results") or [] if isinstance(t, dict)]
            self.looking = False
            self.tmdb = found[:FOUND_SHOWN]
            self.refused = None
            if refused is not None:
                manage = requests.manages(self.win.requests_summary(ask=False))
                self.refused = requests.problem(refused, T, manage, now=time.time())
            if self.win.owner.get(self.name) is self:
                # Drawn even under another page: Back finds it there.
                self.came()

        # No answer (the server or the service away): the libraries' rows alone.
        self.win.later(look, took, lambda: took(None, []))

    def came(self) -> None:
        """The Discover row's titles (or its refusal) came: their row after the
        libraries', theirs and focus left as they are."""
        rows = self.rows.rows
        at = next((i for i, row in enumerate(rows) if row["key"] == "tmdb"), None)
        if at is not None and self.tmdb:
            # Searched again (something was asked for): its badges anew.
            listed = self.win.c(L.slot_list(L.SEARCH_ROWS, at, "poster"))
            position = listed.getSelectedPosition()
            rows[at] = self.tmdb_row()
            self.rows.fill_slot(at)
            if position > 0:
                listed.selectItem(min(position, len(self.tmdb)))
        elif at is not None:
            return self.draw()
        elif not self.tmdb and not self.refused:
            # Nothing found there.
            return self.say()
        else:
            # The last slot is kept for it: the libraries fill the others at most.
            self.rows.append(self.tmdb_row())
        self.place(self.focused_row())
        self.say()

    def click(self, control_id: int) -> None:
        win = self.win
        if control_id == L.SEARCH_FIELD:
            typed = xbmcgui.Dialog().input(T("search"), defaultt=self.query)
            if typed is None:
                return
            try:
                self.search(typed)
            except api.ApiError as e:
                return win.toast(T("server_error") % e.message, 6)
            if self.found:
                self.rows.focus(0)
            return
        found = self.rows.selected(control_id)
        if found is not None and found[1]["key"] == "channels":
            # A channel plays, the others found queued: Next and Previous.
            channels = found[1]["items"]
            return play_channels(self.session.base, channels, next(i for i, c in enumerate(channels) if c is found[2]))
        if found is not None and found[1]["key"] == "tmdb":
            return self.open_title(found[2])
        if found is not None:
            item = dict(found[2])
            if found[1]["key"] == "people":
                item["kind"] = "person"
            win.open_item(item)

    def open_title(self, title: dict) -> None:
        """A title found on TMDB: the library's page where one has it, else
        its main action (the plugin's); See all: Discover's
        results for the same words, in Kodi's lists; Try again: TMDB is
        asked again, the row as it is until it answers."""
        if title.get("kind") == "seeall":
            if self.refused and self.refused[2]:
                return self.look_on_tmdb()
            return self.win.leave_for_listing(route="discover_search", q=self.query)
        if title.get("item_id") and title.get("status") in ("library", "partial"):
            return self.win.open(Item(self.win, title["item_id"]))
        run_plugin(route="title", kind="show" if title.get("kind") == "show" else "movie", tmdb=title.get("tmdb_id"))

    def focused(self, control_id: int) -> None:
        found = self.rows.find(control_id)
        if found is not None:
            self.place(found[0])
        elif control_id == L.SEARCH_FIELD:
            self.place()

    def menu(self, control_id: int) -> None:
        found = self.rows.selected(control_id)
        if found is None or found[1]["key"] in ("people", "channels"):
            return
        title = found[2]
        if found[1]["key"] != "tmdb":
            return self.win.tile_menu(title)
        if title.get("kind") == "show" and title.get("item_id") and title.get("status") == "partial":
            # OK opens the library's seasons; more of them are asked for here.
            tmdb = title.get("tmdb_id")
            self.win.choose([(T("request_more_seasons"), lambda: run_plugin(route="title", kind="show", tmdb=tmdb, more=1))])

    def refresh(self) -> None:
        if not self.query:
            return
        focus = self.win.getFocusId()
        at = self.win.c(focus).getSelectedPosition() if self.rows.find(focus) is not None else 0
        self.search(self.query, again=True)
        if at > 0 and self.rows.find(focus) is not None:
            # Focus stays on the tile it was on (a title just asked for).
            self.win.c(focus).selectItem(at)


def tmdb_caption(title: dict) -> str:
    """Under a TMDB title's poster: its badge, else its year and kind."""
    badge = requests.colored(*requests.badge(title, T))
    if badge:
        return badge
    year = str(title["year"]) if title.get("year") else ""
    return " · ".join(p for p in (year, T("show") if title.get("kind") == "show" else T("movie")) if p)


class Grid(Screen):
    """A page of tiles: playlists, a playlist, collections, a collection,
    a person's titles."""

    name = "grid"

    def __init__(self, win: Window, title: str, loader, shape: str = "grid", rail=None, opener=None, empty: str = "", subtitle: str = "", menu_for=None):
        super().__init__(win)
        self.title = title
        self.loader = loader
        self.shape = shape
        self.rail = rail
        self.opener = opener
        self.empty = empty
        self.subtitle = subtitle
        # An entry's actions, when they aren't a title's (playlists').
        self.menu_for = menu_for
        self.entries = []
        self.items = []

    @classmethod
    def playlists(cls, win: Window):
        def load():
            return win.session.get("/api/playlists").get("playlists", [])

        def tile(playlist):
            li = xbmcgui.ListItem(playlist.get("title") or "", T("playlist_items") % playlist.get("item_count", 0), offscreen=True)
            artwork = playlist.get("artwork") or []
            if artwork:
                li.setArt({"tile": image(win.session.base, artwork[0], tiles.WIDTHS["wide"])})
            li.setProperty("ph", words.placeholder(playlist.get("title") or ""))
            return li

        def menu_for(playlist):
            return [(T("rename_playlist"), lambda: win.rename_playlist(playlist)), (T("delete_playlist"), lambda: win.delete_playlist(playlist))]

        return cls(win, T("playlists"), (load, tile), "wide", "playlists", lambda p: win.open(Playlist(win, p)), T("no_playlists"), menu_for=menu_for)

    @classmethod
    def collections(cls, win: Window):
        def load():
            found = []
            videos = [lib for lib in win.libraries if lib.get("kind") in ("movies", "shows")]
            answers = api.together(*[lambda lib=lib: win.session.get("/api/libraries/%d/collections" % lib["id"]) for lib in videos]) if videos else []
            for answer in answers:
                found += answer.get("collections", [])
            return sorted(found, key=lambda c: (c.get("title") or "").lower())

        return cls(win, T("collections"), (load, lambda c: collection_tile(win.session.base, c)), "grid", "collections", lambda c: win.open(Grid.collection(win, c)), T("no_collections"))

    @classmethod
    def collection(cls, win: Window, collection: dict):
        def load():
            return win.session.get("/api/collections/%d" % collection["id"], summaries=1).get("items", [])

        return cls(win, collection.get("title") or "", (load, None), "grid", opener=win.open_item, subtitle=collection.get("summary") or "")

    @classmethod
    def person(cls, win: Window, person: dict):
        def load():
            found = win.session.get("/api/people/%d" % int(person["id"]))
            return [credit["item"] for credit in found.get("credits", [])]

        return cls(win, person.get("name") or person.get("title") or "", (load, None), "grid", opener=win.open_item, subtitle=T("known_for"))

    def load(self) -> None:
        self.entries = self.loader[0]()
        if self.shape == "wide" and self.loader[1] is None and self.entries:
            kinds = {e.get("kind") for e in self.entries}
            self.shape = "wide" if kinds <= {"episode", "extra"} else "square" if kinds <= {"track", "album", "artist"} else "grid"

    def show(self, filled: bool) -> None:
        win = self.win
        win.put(L.GRID_TITLE, self.title)
        win.put(L.GRID_SUBTITLE, clamp(self.subtitle, 27, 1500, 1))
        if not filled:
            self.draw()
        win.setFocusId(self.last_focus if filled and self.last_focus else (L.grid_list(L.GRID_LISTS, self.shape) if self.entries else L.RAIL_HOME))

    def draw(self) -> None:
        win = self.win
        for shape in L.GRID_SHAPES:
            win.visible(L.grid_list(L.GRID_LISTS, shape), False)
        grid = win.c(L.grid_list(L.GRID_LISTS, self.shape))
        grid.reset()
        make = self.loader[1] or (lambda item: tiles.tile(self.session.base, item, self.shape))
        grid.addItems([make(entry) for entry in self.entries])
        grid.setVisible(bool(self.entries))
        win.put(L.GRID_EMPTY, "" if self.entries else self.empty)

    def click(self, control_id: int) -> None:
        if control_id != L.grid_list(L.GRID_LISTS, self.shape):
            return
        at = self.win.c(control_id).getSelectedPosition()
        if 0 <= at < len(self.entries) and self.opener:
            self.opener(self.entries[at])

    def menu(self, control_id: int) -> None:
        if control_id != L.grid_list(L.GRID_LISTS, self.shape):
            return
        at = self.win.c(control_id).getSelectedPosition()
        if not 0 <= at < len(self.entries):
            return
        if self.menu_for is not None:
            self.win.choose(self.menu_for(self.entries[at]))
            try:
                self.refresh()
            except api.ApiError as e:
                self.win.toast(T("server_error") % e.message, 6)
        elif self.loader[1] is None:
            self.win.tile_menu(self.entries[at])

    def refresh(self) -> None:
        focus = self.win.getFocusId()
        grid = L.grid_list(L.GRID_LISTS, self.shape)
        at = self.win.c(grid).getSelectedPosition() if focus == grid else -1
        self.load()
        self.draw()
        if at >= 0 and self.entries:
            self.win.setFocusId(grid)
            self.win.c(grid).selectItem(min(at, len(self.entries) - 1))
        elif at >= 0:
            # The last one went: focus mustn't stay on the hidden grid.
            self.win.focus_rail()


class Playlist(Grid):
    """A playlist (the web's PlaylistPage): its titles, a music playlist's
    tracks playing on from the one chosen. A title's menu edits the
    playlist too: Move up, Move down and Remove from playlist (not a smart
    playlist's, which fills itself), Rename playlist and Delete playlist."""

    def __init__(self, win: Window, playlist: dict):
        super().__init__(win, playlist.get("title") or "", (self.fetch, None), "wide", opener=win.open_item)
        self.playlist = playlist
        self.listed = []

    def fetch(self) -> list:
        found = self.session.get("/api/playlists/%d" % self.playlist["id"], summaries=1)
        self.take(found)
        return self.entries

    def take(self, found: dict) -> None:
        """The playlist as the server answered it."""
        self.playlist = found
        self.title = found.get("title") or self.title
        self.listed = found.get("entries") or []
        self.entries = [e["item"] for e in self.listed]
        self.empty = T("smart_playlist_empty") if found.get("smart_filter") else T("playlist_empty")

    def click(self, control_id: int) -> None:
        if control_id != L.grid_list(L.GRID_LISTS, self.shape):
            return
        at = self.win.c(control_id).getSelectedPosition()
        if not 0 <= at < len(self.entries):
            return
        if self.playlist.get("kind") == "audio":
            return self.win.play_music(self.playlist, at=at, tracks=self.entries)
        self.win.open_item(self.entries[at])

    def menu(self, control_id: int) -> None:
        if control_id != L.grid_list(L.GRID_LISTS, self.shape):
            return
        at = self.win.c(control_id).getSelectedPosition()
        if not 0 <= at < len(self.entries):
            return
        edits = []
        if not self.playlist.get("smart_filter"):
            if at > 0:
                edits.append((T("move_up"), lambda: self.move(at, at - 1)))
            if at + 1 < len(self.entries):
                edits.append((T("move_down"), lambda: self.move(at, at + 1)))
            edits.append((T("remove_from_playlist"), lambda: self.remove(at)))
        edits += [(T("rename_playlist"), self.rename), (T("delete_playlist"), self.delete)]
        self.win.tile_menu(self.entries[at], extra=edits)

    def move(self, at: int, to: int) -> None:
        ids = [e["entry_id"] for e in self.listed]
        ids.insert(to, ids.pop(at))
        self.redraw(self.session.client.put("/api/playlists/%d/order" % self.playlist["id"], {"entry_ids": ids}), to)

    def remove(self, at: int) -> None:
        self.session.client.delete("/api/playlists/%d/items/%d" % (self.playlist["id"], self.listed[at]["entry_id"]))
        self.redraw(None, at)

    def redraw(self, found, at: int) -> None:
        """The playlist as it is now (`found`, else asked again), focus on
        `at` (or the one before, past its end)."""
        if found and "entries" in found:
            self.take(found)
        else:
            self.load()
        self.draw()
        grid = L.grid_list(L.GRID_LISTS, self.shape)
        if self.entries:
            self.win.setFocusId(grid)
            self.win.c(grid).selectItem(min(at, len(self.entries) - 1))
        else:
            self.win.focus_rail()

    def rename(self) -> None:
        title = self.win.rename_playlist(self.playlist)
        if title:
            self.title = self.playlist["title"] = title
            self.win.put(L.GRID_TITLE, title)

    def delete(self) -> None:
        if self.win.delete_playlist(self.playlist):
            self.win.back()
            if isinstance(self.win.screen, Grid):
                # The playlists, without this one.
                self.win.screen.refresh()


class Live(Screen):
    """Live TV (livetv.py), as the web lays it out: the
    categories (Search, Favourites, Recently watched, All channels, then
    the groups, those of a region gathered under it), the channels of the
    one focus rests on by number with what's on now, and beside them the
    focused channel's programme now and next. Channels come a page at a
    time as the list is scrolled: a lineup can have tens of thousands. A
    channel plays in Kodi's player with its category's channels in the
    queue, so Next and Previous change channel within it. Menu and Info:
    its favourite and schedule; number keys go to a channel."""

    name = "live"
    rail = "live"
    # What's on is redrawn this often near focus; a channel whose
    # programme ended is asked for again.
    TICK = 30
    # Number keys typed this close together make one number.
    TYPING = 1.5
    # Rows either side of focus kept up to date.
    AROUND = 20
    # The next page is asked for this close to the end of those in.
    AHEAD = 30

    def __init__(self, win: Window):
        super().__init__(win)
        self.lineup = {}
        # The category shown: ("all"|"favorites"|"recent"|"group"|"search", group or query).
        key = self.remembered_as()
        kept = kodi.remembered(key, "all") if key else "all"
        kind, _, group = kept.partition(":")
        self.category = (kind, group or None) if kind in ("all", "favorites", "recent", "group") else ("all", None)
        self.entries = []
        self.open_regions = set()
        # The channels in the list: those from `offset` in the category, of `total`.
        self.offset = 0
        self.total = 0
        self.shown = []
        self.asked = 0
        self.loading = False
        self.timer = None
        self.typed = ""
        self.typed_timer = None

    def remembered_as(self):
        """Where the category is remembered: the server and the profile, as
        a library's tab (the web keeps its own in the browser)."""
        server = kodi.setting("server_id") or kodi.setting("server_url")
        profile = self.win.me.get("id")
        if not server or profile is None:
            return None
        return "live_tab/%s/%s" % (server, profile)

    # ------------------------------------------------------------ the server

    @property
    def group(self):
        """The group shown, when it is one: names in it leave out its region."""
        return self.category[1] if self.category[0] == "group" else None

    def page(self, offset: int = 0, around=None, category=None) -> dict:
        kind, value = category or self.category
        if kind == "search":
            return livetv.browse(self.session.client, "all", query=value, offset=offset, around=around)
        return livetv.browse(self.session.client, kind, value if kind == "group" else None, offset=offset, around=around)

    def load(self) -> None:
        self.lineup = livetv.groups(self.session.client)
        self.settle()
        self.take(self.page())

    def take(self, found: dict) -> None:
        self.offset, self.total, self.shown = found["offset"], found["total"], list(found["channels"])

    def settle(self) -> None:
        """All channels where the category went (the last favourite gone, a
        group hidden)."""
        kind, group = self.category
        names = {g["name"] for g in self.lineup.get("groups") or []}
        if (kind == "group" and group not in names) or (kind in ("favorites", "recent") and not self.lineup.get(kind)):
            self.category = ("all", None)

    def instead(self):
        if self.lineup.get("total"):
            return None
        admin = bool(self.win.me.get("is_admin"))
        notice = Notice(self.win, T("no_channels"), T("no_channels_admin" if admin else "no_channels_body"), retry=self)
        notice.rail = "live"
        return notice

    # --------------------------------------------------------------- drawing

    def show(self, filled: bool) -> None:
        win = self.win
        if not filled:
            self.draw_categories(select=True)
            self.draw()
        self.wait()
        if filled and self.last_focus:
            win.setFocusId(self.last_focus)
        else:
            win.setFocusId(self.first())

    def first(self) -> int:
        return L.LIVE_LIST if self.shown else L.LIVE_CATEGORIES

    def category_entries(self) -> list:
        """The categories as the list shows them: a region's groups after
        it while it is open (and while one of them is shown)."""
        lineup = self.lineup
        out = [{"key": ("search", None), "label": T("search_channels"), "count": None}]
        if self.category[0] == "search":
            # What was looked for, marked as the category shown.
            out[0].update(key=self.category, label="“%s”" % self.category[1], count=self.total)
            # The groups whose names match, to go to (the web's).
            for group in livetv.matching_groups(lineup.get("groups") or [], self.category[1]):
                out.append({"key": ("group", group["name"]), "label": livetv.group_name(group), "count": group.get("count"), "inner": True})
        if lineup.get("favorites"):
            out.append({"key": ("favorites", None), "label": T("favourites"), "count": lineup["favorites"]})
        if lineup.get("recent"):
            out.append({"key": ("recent", None), "label": T("recently_watched"), "count": lineup["recent"]})
        out.append({"key": ("all", None), "label": T("all_channels"), "count": lineup.get("total") or 0})
        for entry in livetv.group_tree(lineup.get("groups") or []):
            if entry["kind"] == "group":
                group = entry["group"]
                out.append({"key": ("group", group["name"]), "label": livetv.group_name(group), "count": group.get("count")})
                continue
            region = entry["region"]
            opened = region in self.open_regions
            out.append({"key": ("region", region), "label": region, "count": entry["count"], "region": True, "open": opened})
            if opened:
                for group in entry["groups"]:
                    out.append({"key": ("group", group["name"]), "label": group.get("label") or group["name"], "count": group.get("count"), "inner": True})
        return out

    def draw_categories(self, select: bool = False) -> None:
        """The categories' list, the one shown marked; `select`: focus on it."""
        win = self.win
        found = win.c(L.LIVE_CATEGORIES)
        at = found.getSelectedPosition()
        if select and self.group:
            # The region of the group shown opens with it.
            region = next((g.get("region") for g in self.lineup.get("groups") or [] if g["name"] == self.group), None)
            if region:
                self.open_regions.add(region)
        self.entries = self.category_entries()
        items = []
        for entry in self.entries:
            li = xbmcgui.ListItem(entry["label"], "{:,}".format(entry["count"]) if entry["count"] is not None else "", offscreen=True)
            li.setProperty("chosen", "1" if entry["key"] == self.category else "")
            li.setProperty("region", ("open" if entry.get("open") else "closed") if entry.get("region") else "")
            li.setProperty("inner", "1" if entry.get("inner") else "")
            items.append(li)
        found.reset()
        found.addItems(items)
        keys = [e["key"] for e in self.entries]
        if select and self.category in keys:
            found.selectItem(keys.index(self.category))
        elif 0 <= at < len(items):
            found.selectItem(at)

    def draw(self, keep=None) -> None:
        """The category's channels in; `keep`: the channel to stay on."""
        win = self.win
        began = time.perf_counter()
        now = time.time()
        found = win.c(L.LIVE_LIST)
        found.reset()
        found.addItems([channel_row(self.session.base, channel, now, None, self.group) for channel in self.shown])
        ids = [c["id"] for c in self.shown]
        if keep in ids:
            found.selectItem(ids.index(keep))
        kind, value = self.category
        if kind == "group":
            heading = livetv.group_name(next((g for g in self.lineup.get("groups") or [] if g["name"] == value), {"label": value}))
        else:
            heading = {"all": T("all_channels"), "favorites": T("favourites"), "recent": T("recently_watched")}.get(kind) or "“%s”" % value
        win.put(L.LIVE_HEADING, heading)
        win.put(L.LIVE_COUNT, words.plural(self.total, "channel") if self.total else "")
        win.put(L.LIVE_EMPTY, "" if self.shown else T("nothing_found") if kind == "search" else T("no_channels"))
        kodi.log("Live TV: %d of %d channels drawn in %d ms" % (len(self.shown), self.total, (time.perf_counter() - began) * 1000), xbmc.LOGDEBUG)

    def focused_channel_id(self):
        channel = self.selected()
        return channel["id"] if channel else None

    def selected(self):
        at = self.win.c(L.LIVE_LIST).getSelectedPosition()
        return self.shown[at] if 0 <= at < len(self.shown) else None

    def refresh(self) -> None:
        """What changed on the server (back from playing, an event): the
        categories' counts, and the channels in the list from where they
        start, focus kept on its channel."""
        keep = self.focused_channel_id()
        self.lineup = livetv.groups(self.session.client)
        before = self.category
        self.settle()
        count = max(len(self.shown), livetv.PAGE)
        found = self.page(0 if before != self.category else self.offset)
        if count > livetv.PAGE and before == self.category:
            more = livetv.browse(self.session.client, *self.browse_args(), offset=self.offset + livetv.PAGE, limit=min(count - livetv.PAGE, 400))
            found["channels"] = found["channels"] + more["channels"]
        self.take(found)
        self.draw_categories()
        self.draw(keep=keep)

    def browse_args(self) -> tuple:
        kind, value = self.category
        if kind == "search":
            return ("all", None, value)
        return (kind, value if kind == "group" else None, None)

    def leave(self) -> None:
        super().leave()
        if self.timer is not None:
            self.timer.cancel()
            self.timer = None

    # --------------------------------------------------------- time passes

    def around(self) -> range:
        """The rows near focus: the ones seen, and those a few presses away."""
        at = max(self.win.c(L.LIVE_LIST).getSelectedPosition(), 0)
        return range(max(0, at - self.AROUND), min(len(self.shown), at + self.AROUND + 1))

    def freshen(self, fetch: bool = True) -> None:
        """What's on now in the rows near focus, as time passed; with
        `fetch`, the channels among them whose programme ended asked for
        again (what comes after it)."""
        rows = self.around()
        now = time.time()
        if fetch:
            ended = [self.shown[i] for i in rows if (self.shown[i].get("now") or {}).get("stop", now + 1) <= now]
            if ended:
                client = self.session.client
                for channel, fresh in zip(ended, api.together(*[lambda c=c: livetv.channel(client, c["id"]) for c in ended])):
                    if isinstance(fresh, dict):
                        channel.update(fresh)
        found = self.win.c(L.LIVE_LIST)
        for at in rows:
            channel_row(self.session.base, self.shown[at], now, found.getListItem(at), self.group)

    def wait(self) -> None:
        """What's on is redrawn in TICK seconds, while this page shows."""
        if self.timer is not None:
            self.timer.cancel()
        self.timer = threading.Timer(self.TICK, lambda: self.win.post(self.tick))
        self.timer.daemon = True
        self.timer.start()

    def tick(self) -> None:
        if self.win.closing or self.win.screen is not self:
            return
        try:
            if xbmc.getCondVisibility("Player.HasVideo"):
                # Under the player: redrawn as the window shows again.
                return
            self.freshen()
        except api.ApiError as e:
            kodi.log("Live TV: %s" % e.message, xbmc.LOGWARNING)
        finally:
            self.wait()

    # -------------------------------------------------------------- moving

    def focused(self, control_id: int) -> None:
        if control_id == L.LIVE_CATEGORIES:
            self.win.rest(self.rested_on_category)

    def moved(self, control_id: int, action: int) -> None:
        if control_id == L.LIVE_CATEGORIES:
            # The channels follow focus once it rests.
            self.win.rest(self.rested_on_category)
        elif control_id == L.LIVE_LIST:
            self.win.rest(lambda: self.win.screen is self and self.rested_on_channel())

    def rested_on_category(self) -> None:
        if self.win.screen is not self or self.win.getFocusId() != L.LIVE_CATEGORIES:
            return
        at = self.win.c(L.LIVE_CATEGORIES).getSelectedPosition()
        if 0 <= at < len(self.entries) and self.entries[at]["key"][0] in ("all", "favorites", "recent", "group"):
            self.choose(self.entries[at]["key"])

    def choose(self, category: tuple, then=None) -> None:
        """Shows `category`'s first page, asked for away from the window's
        thread (a newer choice wins); `then()` once it is in."""
        if category == self.category and self.shown:
            if then:
                then()
            return
        self.asked += 1
        asked = self.asked

        def done(found):
            if asked != self.asked or self.win.screen is not self:
                return
            self.category = category
            key = self.remembered_as()
            if key and category[0] != "search":
                kodi.remember(key, category[0] + (":" + category[1] if category[1] else ""))
            self.take(found)
            self.draw_categories()
            self.draw()
            if then:
                then()

        self.win.later(lambda: self.page(category=category), done, lambda: self.win.toast(T("page_failed"), 5))

    def rested_on_channel(self) -> None:
        """Near the end of the channels in, the next page; near the start
        of a list that began further on, the page before."""
        at = self.win.c(L.LIVE_LIST).getSelectedPosition()
        if self.loading or at < 0:
            return
        if at >= len(self.shown) - self.AHEAD and self.offset + len(self.shown) < self.total:
            self.load_more(self.offset + len(self.shown), before=False)
        elif at < self.AHEAD and self.offset > 0:
            self.load_more(max(0, self.offset - livetv.PAGE), before=True)
        self.freshen()

    def load_more(self, offset: int, before: bool) -> None:
        self.loading = True
        category = self.category
        limit = (self.offset - offset) if before else livetv.PAGE

        def done(found):
            self.loading = False
            if category != self.category or self.win.screen is not self:
                return
            found_list = self.win.c(L.LIVE_LIST)
            now = time.time()
            if before:
                at = found_list.getSelectedPosition()
                self.shown = found["channels"] + self.shown
                self.offset = found["offset"]
                found_list.reset()
                found_list.addItems([channel_row(self.session.base, c, now, None, self.group) for c in self.shown])
                found_list.selectItem(at + len(found["channels"]))
            else:
                at = found_list.getSelectedPosition()
                self.shown += found["channels"]
                found_list.addItems([channel_row(self.session.base, c, now, None, self.group) for c in found["channels"]])
                # Kodi's list goes back to its start as items are added.
                found_list.selectItem(at)

        def failed():
            self.loading = False

        kind, value, query = self.browse_args()
        self.win.later(lambda: livetv.browse(self.session.client, kind, value, query, offset, limit), done, failed)

    # ------------------------------------------------------------ actions

    def click(self, control_id: int) -> None:
        if control_id == L.LIVE_CATEGORIES:
            at = self.win.c(L.LIVE_CATEGORIES).getSelectedPosition()
            if not 0 <= at < len(self.entries):
                return
            kind, value = self.entries[at]["key"]
            if kind == "region":
                # Opened or closed where it is.
                self.open_regions.symmetric_difference_update({value})
                return self.draw_categories()
            if kind == "search":
                return self.search()
            return self.choose((kind, value), then=lambda: self.shown and self.win.setFocusId(L.LIVE_LIST))
        if control_id == L.LIVE_LIST:
            at = self.win.c(L.LIVE_LIST).getSelectedPosition()
            if 0 <= at < len(self.shown):
                self.watch(at)

    def search(self) -> None:
        query = (xbmcgui.Dialog().input(T("search_channels"), defaultt=self.category[1] if self.category[0] == "search" else "") or "").strip()
        if query:
            self.choose(("search", query), then=lambda: self.win.setFocusId(L.LIVE_LIST if self.shown else L.LIVE_CATEGORIES))

    def watch(self, at: int) -> None:
        """Plays the channel at `at`, its category's channels around it in
        Kodi's queue."""
        play_channels(self.session.base, self.shown, at, self.group)

    def menu(self, control_id: int) -> None:
        if control_id != L.LIVE_LIST:
            return
        channel = self.selected()
        if channel is None:
            return
        at = self.win.c(L.LIVE_LIST).getSelectedPosition()
        favourite = bool(channel.get("favorite"))
        self.win.choose(
            [
                (T("watch"), lambda: self.watch(at)),
                (T("remove_favourite" if favourite else "add_favourite"), lambda: self.set_favourite(channel, not favourite)),
                (T("schedule"), lambda: self.schedule(channel)),
            ]
        )

    def info(self, control_id: int) -> None:
        channel = self.selected() if control_id == L.LIVE_LIST else None
        if channel is not None:
            self.schedule(channel)

    def set_favourite(self, channel: dict, favourite: bool) -> None:
        livetv.set_favorite(self.session.client, channel["id"], favourite)
        channel["favorite"] = favourite
        self.lineup = livetv.groups(self.session.client)
        if self.category[0] == "favorites":
            # The channel leaves the list (or the list goes, the last one).
            self.settle()
            self.take(self.page())
            self.draw_categories()
            self.draw()
            return self.win.setFocusId(self.first())
        self.draw_categories()
        at = next((i for i, c in enumerate(self.shown) if c is channel), -1)
        if at >= 0:
            channel_row(self.session.base, channel, time.time(), self.win.c(L.LIVE_LIST).getListItem(at), self.group)

    def schedule(self, channel: dict, start=None) -> None:
        """The channel's programmes, SCHEDULE_HOURS at a time, in Kodi's
        list: what's on watches the channel, the rest say what they are,
        Later shows the hours after."""
        now = time.time()
        start = start or now
        logo = livetv.logo(self.session.base, channel)
        called = livetv.name(channel, self.group)
        programs, at = None, 0
        while True:
            if programs is None:
                try:
                    programs = livetv.schedule(self.session.client, channel["id"], start)
                except api.ApiError as e:
                    return self.win.toast(T("server_error") % e.message, 6)
                if start > now:
                    programs = [p for p in programs if p["start"] >= start]
                if not programs:
                    return self.win.toast(T("nothing_scheduled") % called, 5)
                at = 0
            entries = [schedule_entry(p, now, logo) for p in programs] + [xbmcgui.ListItem(T("later"), offscreen=True)]
            chosen = xbmcgui.Dialog().select(called, entries, preselect=at, useDetails=True)
            if chosen < 0:
                return
            if chosen == len(programs):
                start, programs = max(p["stop"] for p in programs), None
                continue
            program, at = programs[chosen], chosen
            if livetv.airing(program, time.time()):
                ids = [c["id"] for c in self.shown]
                if channel["id"] in ids:
                    return self.watch(ids.index(channel["id"]))
                return play_channels(self.session.base, [channel], 0)
            # Its details; back from them, the schedule as it was.
            xbmcgui.Dialog().textviewer(livetv.headline(program), program_text(channel, program, now, self.group))

    def number(self, digit: int) -> None:
        """Number keys typed together make a channel's number (the web
        player's), shown as they are typed; focus goes to it."""
        self.typed = (self.typed + str(digit))[-5:]
        self.win.toast(self.typed, self.TYPING + 1)
        if self.typed_timer is not None:
            self.typed_timer.cancel()
        self.typed_timer = threading.Timer(self.TYPING, lambda: self.win.post(self.go_to_typed))
        self.typed_timer.daemon = True
        self.typed_timer.start()

    def go_to_typed(self) -> None:
        """The channel of the number typed: in the category shown, else in
        All channels; its page asked for, and focus on it."""
        typed, self.typed = self.typed, ""
        if not typed or self.win.screen is not self:
            return
        try:
            category = self.category if self.category[0] != "search" else ("all", None)
            kind, value = category
            found = livetv.by_number(livetv.browse(self.session.client, kind, value if kind == "group" else None, typed, limit=50)["channels"], typed)
            if found is None and kind != "all":
                category = ("all", None)
                found = livetv.by_number(livetv.browse(self.session.client, "all", query=typed, limit=50)["channels"], typed)
            if found is None:
                return self.win.toast(T("no_channel_numbered") % typed)
            self.category = category
            self.take(self.page(around=found["id"]))
        except api.ApiError as e:
            return self.win.toast(T("server_error") % e.message, 6)
        self.draw_categories(select=True)
        self.draw(keep=found["id"])
        self.win.setFocusId(L.LIVE_LIST)
        self.freshen(fetch=False)


def play_channels(base: str, channels: list, at: int, group=None) -> None:
    """Plays channel `at` of `channels`, those around it in Kodi's queue."""
    queue, start = livetv.queue(base, channels, at, time.time(), group)
    xbmc.Player().play(queue, startpos=start)


def channel_tile(base: str, channel: dict, now: float) -> "xbmcgui.ListItem":
    """A channel among search's rows: its logo, number and name, what's on
    and how far into it."""
    current = livetv.on_now(channel, now)
    li = xbmcgui.ListItem(livetv.name(channel), livetv.headline(current) if current else livetv.nothing_on(channel), offscreen=True)
    found = livetv.logo(base, channel)
    if found:
        li.setArt({"tile": found})
    li.setProperty("id", str(channel["id"]))
    li.setProperty("kind", "channel")
    if current:
        li.getVideoInfoTag().setResumePoint(max(1.0, now - current["start"]), float(current["stop"] - current["start"]))
    return li


def channel_row(base: str, channel: dict, now: float, li=None, group=None) -> "xbmcgui.ListItem":
    """A channel in Live TV's list (skin.live_row), and what the panel beside
    it says of it (skin.live_page), as `li`'s labels, picture and
    properties; a new item without `li`. In `group`, its name leaves out
    the group's region."""
    li = li if li is not None else xbmcgui.ListItem(offscreen=True)
    current = livetv.on_now(channel, now)
    later = livetv.up_next(channel, now)
    li.setLabel(livetv.name(channel, group))
    li.setLabel2(livetv.headline(current) if current else livetv.nothing_on(channel))
    li.setArt({"thumb": livetv.logo(base, channel) or ""})
    called = livetv.short_name(channel, group)
    li.setProperty("short", channel.get("number") or called[:3])
    li.setProperty("id", str(channel["id"]))
    li.setProperty("favorite", "1" if channel.get("favorite") else "")
    tag = li.getVideoInfoTag()
    if current:
        # The progress bar: ListItem.PercentPlayed, as a tile's.
        tag.setResumePoint(max(1.0, now - current["start"]), float(current["stop"] - current["start"]))
        li.setProperty("when", " · ".join(p for p in (livetv.span(current), livetv.left(current, now)) if p))
        airs = ["%s · %s" % (livetv.day(current["start"], now), livetv.span(current)), livetv.episode(current)] + livetv.badges(current)
        li.setProperty("title", livetv.headline(current))
        li.setProperty("airs", " · ".join(p for p in airs if p))
        li.setProperty("description", (current.get("description") or "").strip())
        li.setProperty("picture", image(base, current.get("image"), livetv.PICTURE_WIDTH) or "")
    else:
        tag.setResumePoint(0.0, 0.0)
        li.setProperty("when", "")
        li.setProperty("title", livetv.name(channel, group))
        li.setProperty("airs", livetv.nothing_on(channel))
        li.setProperty("description", "")
        li.setProperty("picture", "")
    li.setProperty("next", "%s  %s" % (livetv.clock(later["start"]), livetv.headline(later)) if later else "")
    return li


def schedule_entry(program: dict, now: float, logo=None) -> "xbmcgui.ListItem":
    """A programme in a channel's schedule: when, its title, and its time
    and episode under it, beside the channel's logo."""
    when = T("on_now") if livetv.airing(program, now) else "%s %s" % (livetv.day(program["start"], now), livetv.clock(program["start"]))
    under = " · ".join(p for p in [livetv.span(program), livetv.episode(program)] + livetv.badges(program) if p)
    li = xbmcgui.ListItem("%s  ·  %s" % (when, livetv.headline(program)), under, offscreen=True)
    if logo:
        li.setArt({"icon": logo, "thumb": logo})
    return li


def program_text(channel: dict, program: dict, now: float, group=None) -> str:
    """A programme still to come, in Kodi's text viewer."""
    line = " · ".join(
        p for p in ["%s · %s" % (livetv.day(program["start"], now), livetv.span(program)), livetv.episode(program)] + livetv.badges(program) if p
    )
    parts = ["[B]%s[/B]" % livetv.name(channel, group), line]
    if program.get("description"):
        parts += ["", program["description"].strip()]
    if program.get("categories"):
        parts += ["", ", ".join(program["categories"])]
    return "\n".join(parts)


class Libraries(Screen):
    """Every library (TvLibrariesScreen): the rail shows the first four or
    five, and Libraries opens this page. A card for each in the profile's order:
    its kind's icon, its name, how much it holds, and "Scanning…" while the
    server scans it. Sort libraries sorts them as the profile's menu does;
    a card's menu opens it, moves it, or goes back to the server's order."""

    name = "libraries"

    @property
    def rail(self) -> str:
        return "libraries" if L.RAIL_ALL_LIBRARIES in rail_entries(len(self.win.libraries), livetv.available(self.win.live)) else "home"

    def load(self) -> None:
        # How much each holds, and which the server is scanning, as of now.
        self.win.take_libraries(self.session.get("/api/libraries"))

    def instead(self):
        if self.win.libraries:
            return None
        return Notice(self.win, T("no_libraries"), T("no_libraries_body"), retry=Home(self.win))

    def show(self, filled: bool) -> None:
        if not filled:
            self.draw()
        self.win.setFocusId(self.last_focus if filled and self.last_focus else self.first())

    def first(self) -> int:
        return L.LIBRARIES_LIST

    def draw(self) -> None:
        win = self.win
        cards = win.c(L.LIBRARIES_LIST)
        cards.reset()
        cards.addItems([library_card(lib) for lib in win.libraries])
        win.put(L.LIBRARIES_SORT, ICON_ROOM + T("sort_libraries"))
        win.visible(L.LIBRARIES_SORT, len(win.libraries) > 1)

    def refresh(self) -> None:
        cards = self.win.c(L.LIBRARIES_LIST)
        at = cards.getSelectedPosition()
        self.draw()
        if self.win.libraries:
            cards.selectItem(min(max(at, 0), len(self.win.libraries) - 1))

    def chosen(self):
        at = self.win.c(L.LIBRARIES_LIST).getSelectedPosition()
        return at if 0 <= at < len(self.win.libraries) else None

    def click(self, control_id: int) -> None:
        win = self.win
        if control_id == L.LIBRARIES_SORT:
            return win.sort_libraries()
        at = self.chosen() if control_id == L.LIBRARIES_LIST else None
        if at is not None:
            win.open(Library(win, win.libraries[at], None))

    def menu(self, control_id: int) -> None:
        win = self.win
        at = self.chosen() if control_id == L.LIBRARIES_LIST else None
        if at is None:
            return
        library = win.libraries[at]
        actions = [(T("open"), lambda: win.open(Library(win, library, None)))]
        if len(win.libraries) > 1:
            if at > 0:
                actions.append((T("move_up"), lambda: self.move(at, at - 1)))
            if at + 1 < len(win.libraries):
                actions.append((T("move_down"), lambda: self.move(at, at + 1)))
            actions.append((T("server_order"), win.use_server_order))
        chosen = xbmcgui.Dialog().contextmenu([name for name, _ in actions])
        if chosen >= 0:
            actions[chosen][1]()

    def move(self, at: int, to: int) -> None:
        ids = [lib["id"] for lib in self.win.libraries]
        ids.insert(to, ids.pop(at))
        if self.win.save_order(ids):
            # Focus goes with the card.
            self.win.c(L.LIBRARIES_LIST).selectItem(to)


def library_card(library: dict) -> "xbmcgui.ListItem":
    """A library's card on the Libraries page: its name, how much it holds
    (and that the server is scanning it), its kind's icon."""
    line = library_count(library)
    if library.get("scanning"):
        line = "%s · %s" % (line, T("scanning_now")) if line else T("scanning_now")
    li = xbmcgui.ListItem(library.get("name") or "", line, offscreen=True)
    li.setProperty("icon", LIBRARY_ICONS.get(library.get("kind"), "icon-folder.png"))
    return li


class Profiles(Screen):
    """"Who's watching?" (TvProfilesScreen): the household's profiles, the
    one in use first in focus. Another one is switched to at once, with its
    PIN (Kodi's number dialog) when it has one; one that needs its password
    is signed in to again with a code. `gate`: Kodi's start, where going on
    as a profile with a PIN asks it too, and Back leaves pley."""

    name = "profiles"

    def __init__(self, win: Window, household: list, gate: bool = False):
        super().__init__(win)
        self.household = household
        self.gate = gate

    def show(self, filled: bool) -> None:
        win = self.win
        faces = win.c(L.PROFILES_LIST)
        if not filled:
            faces.reset()
            faces.addItems([profile_face(self.session.base, p, self.win.me) for p in self.household])
            win.place(L.PROFILES_LIST, L.profiles_left(len(self.household)) - L.PROFILE_MARGIN, L.PROFILES_TOP - L.PROFILE_MARGIN)
            faces.selectItem(next((i for i, p in enumerate(self.household) if p.get("current")), 0))
            win.put(L.PROFILES_NOTE, "")
        win.setFocusId(L.PROFILES_LIST)

    def first(self) -> int:
        return L.PROFILES_LIST

    def back(self) -> bool:
        if self.gate:
            self.win.close()
            return True
        return False

    def click(self, control_id: int) -> None:
        if control_id != L.PROFILES_LIST:
            return
        at = self.win.c(control_id).getSelectedPosition()
        if 0 <= at < len(self.household):
            self.pick(self.household[at])

    def pick(self, profile: dict) -> None:
        self.say("")
        if profile.get("current"):
            if self.gate and profiles.needs_pin(profile, self.win.me):
                return self.with_pin(profile, confirm=True)
            return self.done(switched=False)
        needs = profile.get("needs")
        if needs == "none":
            return self.switch(profile, None)
        if needs == "pin":
            return self.with_pin(profile)
        if needs == "password":
            return self.sign_in_as(profile)
        self.say(T("profile_locked"))

    def with_pin(self, profile: dict, confirm: bool = False, problem: str = "") -> None:
        """Its PIN, typed in Kodi's number dialog (hidden), then sent. After
        a wrong one, or too many, the dialog asks again, its heading saying
        why: it has no line of its own for that."""
        heading = T("pin_of") % (profile.get("name") or "")
        pin = xbmcgui.Dialog().numeric(0, "%s %s" % (problem, heading) if problem else heading, "", True)
        if not pin:
            return
        if not (profiles.PIN_MIN <= len(pin) <= profiles.PIN_MAX and pin.isdigit()):
            return self.say(T("pin_digits"))
        self.switch(profile, pin, confirm)

    def switch(self, profile: dict, pin, confirm: bool = False) -> None:
        win = self.win
        try:
            profiles.switch(self.session.client, profile["id"], pin)
        except api.ApiError as e:
            if confirm and e.code == "invalid_request":
                # The server can't check the PIN of the profile in use: on.
                return self.done(switched=False)
            if e.code == "browser":
                return self.sign_in_as(profile)
            if e.code == "wrong_pin" and not pin:
                # The list was older than the profile's new PIN.
                return self.with_pin(profile)
            if e.code == "signed_out":
                return win.failed(e, Home(win))
            name = kodi.setting("server_name") or kodi.setting("server_url")
            if e.code in ("wrong_pin", "throttled"):
                return self.with_pin(profile, confirm, profiles.refusal(e, T, name))
            return self.say(profiles.refusal(e, T, name))
        self.done(switched=True)

    def sign_in_as(self, profile: dict) -> None:
        """A profile that needs its password: only a sign-in with a code,
        approved where it is typed, can choose it."""
        name = profile.get("name") or ""
        if xbmcgui.Dialog().yesno(name, T("needs_password") % (name, name), nolabel=T("cancel"), yeslabel=T("sign_in")):
            self.win.leave_for("sign_in")

    def done(self, switched: bool) -> None:
        kodi.set_property(profiles.GATE, "1")
        if switched:
            self.win.switched()
        else:
            self.win.go(Home(self.win))

    def say(self, words_: str) -> None:
        self.win.put(L.PROFILES_NOTE, words_)


def profile_face(base: str, profile: dict, me: dict) -> xbmcgui.ListItem:
    """A face on "Who's watching?": the name, the picture or the initial on
    the name's placeholder, and a lock for a PIN (`me`: who the add-on is,
    for the one in use)."""
    name = profile.get("name") or ""
    li = xbmcgui.ListItem(name, offscreen=True)
    li.setProperty("ph", words.placeholder(name))
    li.setProperty("initial", name.strip()[:1].upper() or "?")
    found = image(base, profile.get("avatar_url"), 256)
    if found:
        li.setArt({"tile": found})
    if profiles.needs_pin(profile, me):
        li.setProperty("locked", "1")
    return li


class Notice(Screen):
    """A sentence instead of a page (TvHomeNotice): the server out of
    reach, nothing in the libraries yet."""

    name = "notice"

    def __init__(self, win: Window, title: str, body: str, retry=None, sign_in: bool = False):
        super().__init__(win)
        self.title = title
        self.body = body
        self.retry = retry
        self.sign_in = sign_in

    def show(self, filled: bool) -> None:
        win = self.win
        win.put(L.NOTICE_TITLE, self.title)
        win.put(L.NOTICE_BODY, self.body)
        first = T("sign_in") if self.sign_in else T("try_again")
        win.put(L.NOTICE_FIRST, ICON_ROOM + first)
        win.put(L.NOTICE_SECOND, T("choose_server"))
        win.visible(L.NOTICE_FIRST, self.retry is not None or self.sign_in)
        win.setFocusId(L.NOTICE_FIRST if (self.retry is not None or self.sign_in) else L.NOTICE_SECOND)

    def first(self) -> int:
        return L.NOTICE_FIRST

    def click(self, control_id: int) -> None:
        win = self.win
        if control_id == L.NOTICE_FIRST and self.sign_in:
            return win.leave_for("sign_in")
        if control_id == L.NOTICE_FIRST and self.retry is not None:
            screen = type(self.retry)(win) if isinstance(self.retry, (Home, Search)) else self.retry
            win.stack = []
            win.screen = None
            if not win.libraries:
                # Nothing came at the start: started again, "Who's watching?" too.
                return win.start()
            return win.open(screen)
        if control_id == L.NOTICE_SECOND:
            win.leave_for("choose_server")


def run(plugin, base: str) -> None:
    """Opens pley's window until it is closed. Choosing a server and signing
    in, asked from it, happen with it closed, and it opens again after
    when the add-on is signed in."""
    while True:
        window = Window(L.WINDOW, kodi.addon_dir(), "default", "1080i")
        window.session = Session(base)
        window.plugin = plugin
        window.doModal()
        after, listing = window.after, window.listing
        del window
        kodi.clear_property(UI)
        if after == "choose_server":
            plugin.route_choose_server()
        elif after == "sign_in":
            plugin.route_sign_in()
        elif after == "sign_out":
            plugin.sign_out()
            return
        elif after == "listing":
            # Back from its top goes back to Kodi, as from the add-on's
            # other lists; pley's window opens again from the add-on.
            xbmc.executebuiltin("ActivateWindow(Videos,%s,return)" % listing)
            return
        else:
            return
        if not plugin.signed_in():
            return
