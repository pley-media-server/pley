"""pley's look on Kodi's screen: where things are, and the ids of the
controls that draw them.

The window's skin (resources/skins/default/1080i/pley-main.xml) is written
from this by apps/kodi/skin.py, and the window (window.py) moves and fills
the same controls, so both read their numbers here. Sizes are the Android
TV app's (apps/android, design spec §1.3, §2) doubled: it lays out a 960 by
540 canvas, Kodi's skins a 1920 by 1080 one.
"""

from __future__ import annotations

from collections import namedtuple

WIDTH, HEIGHT = 1920, 1080

# pley's colours (PleyColors.kt), as Kodi writes them: AARRGGBB.
BG = "FF0B0D10"
SURFACE = "FF12151A"
SURFACE2 = "FF191D24"
SURFACE3 = "FF222731"
FG = "FFE9EBEF"
FG_85 = "D9E9EBEF"
MUTED = "FFA1A9B6"
SUBTLE = "FF8B95A5"
ACCENT = "FF38BDF8"
ACCENT_BRIGHT = "FF7DD3FC"
ON_ACCENT = "FF03131F"
DANGER_TEXT = "FFFF6B70"
DIM = "990B0D10"

# Estuary's fonts (Kodi's own skin, LibreELEC's and CoreELEC's): the
# nearest to pley's TV type (PleyType.Tv). Other skins without these names
# draw their default font instead.
FONT_HERO = "font_MainMenu"  # HeroTitle, 76 px bold: Estuary's largest bold is 60
FONT_PAGE = "font52_title"  # PageTitle
FONT_SECTION = "font36_title"  # Section (row headers)
FONT_TITLE = "font32_title"  # Title
FONT_BODY_LARGE = "font13"  # BodyLarge (summaries)
FONT_BODY = "font27"  # Body
FONT_LABEL = "font27"  # Label (buttons, the metadata line)
FONT_CAPTION = "font12"  # Caption (tile captions)
FONT_CAPTION_BOLD = "font25_title"  # a focused tile's caption
FONT_BADGE = "font20_title"  # Badge

# The page's lines (PleyTv, TvSafeArea).
GUTTER = 192  # every page's left edge, and the focused tile's
TEXT_WIDTH = 928  # the text column over backdrops, x 192 to 1120
SAFE_X, SAFE_Y = 96, 54
FOCUSED_ROW_Y = 344  # a focused row's header, under the info block
ROW_HEADER = 48
HEADER_TO_TILES = 28
TILES_TO_CAPTION = 28
CAPTION = 68  # two lines of caption
ROW_GAP = 56
BUTTON = 80
BUTTON_GAP = 20
BUTTON_LABEL_ROOM = 48

# Room a list keeps above its tiles for the focused one's growth and ring.
LIST_MARGIN = 30

Shape = namedtuple("Shape", "name width height gap art round")

# Tiles (TileKind.kt): their wells, the gap between them in a row, the
# artwork they show, and whether they are circles.
SHAPES = {
    "poster": Shape("poster", 216, 324, 28, "poster", False),
    "landscape": Shape("landscape", 408, 230, 36, "thumb", False),
    "episode": Shape("episode", 368, 208, 36, "thumb", False),
    "square": Shape("square", 264, 264, 28, "poster", False),
    "person": Shape("person", 192, 192, 28, "photo", True),
    # A Live TV channel: its logo kept whole on a dark well.
    "channel": Shape("channel", 368, 208, 36, "thumb", False),
}
# The shapes a row of each screen can take, in the order of their lists.
ROW_SHAPES = ("poster", "landscape", "square", "episode", "person", "channel")
GRID_SHAPES = ("grid", "wide", "square")
GRID = {
    # The library's posters (TileKind.LibraryGrid): 7 to a row.
    "grid": Shape("grid", 192, 288, 28, "poster", False),
    # Playlists, a playlist's or person's episodes.
    "wide": Shape("wide", 368, 208, 36, "thumb", False),
    "square": Shape("square", 264, 264, 28, "poster", False),
}


# A library poster's quality badges (the Android TV app's TileOverlays):
# pills 36 tall, 12 in from the art's left and foot and 12 apart, each its
# word in FONT_BADGE with 12 either side. By the key the tile gives the
# skin (tiles.badges): its word, and its pill's width. "4K" and the
# widest after it fill a poster of the grid to its inset.
BADGE_HEIGHT, BADGE_INSET, BADGE_GAP = 36, 12, 12
BADGES = {
    "4k": ("4K", 50),
    "dolby_vision": ("DV", 54),
    "hdr10plus": ("HDR+", 82),
    "hdr10": ("HDR", 70),
    "hlg": ("HLG", 68),
}


def stride(shape: Shape) -> int:
    return shape.width + shape.gap


def row_height(shape: Shape) -> int:
    """A row from its header's top to its captions' foot."""
    return ROW_HEADER + HEADER_TO_TILES + shape.height + TILES_TO_CAPTION + CAPTION


def item_height(shape: Shape) -> int:
    """A tile in its list: the room above, the tile, its caption."""
    return LIST_MARGIN + shape.height + TILES_TO_CAPTION + CAPTION


# ------------------------------------------------------------------- ids

WINDOW = "pley-main.xml"

# The backdrop and its scrims.
BACKDROP = 1001
SCRIM_HERO, SCRIM_ROWS, SCRIM_ITEM = 1003, 1004, 1005

# The rail (TvRail, RailItems.kt): a group, its entries' groups and buttons.
# Search, Home, Live TV after it where the server has channels (as in the
# web's sidebar), the libraries, then Playlists and Collections; with more
# libraries than the rail lists, Libraries (a page of them all) takes
# Collections' place. The Android TV app's rail scrolls; Kodi's entries have
# fixed places, so it lists as many libraries as fit over Settings at full
# height (window.rail_shown).
RAIL = 100
RAIL_AVATAR = 110
RAIL_SEARCH = 111
RAIL_HOME = 112
RAIL_LIVE = 117
# Room for five libraries: with Search, Home, Libraries and Playlists, nine
# entries fit (RAIL_FIT); a sixth, as the other TV apps list, would not.
RAIL_LIBRARIES = (113, 114, 115, 122, 123)
RAIL_LEAST_LIBRARIES = 4  # listed whatever else the rail holds
RAIL_ALL_LIBRARIES = 116
RAIL_PLAYLISTS = 119
RAIL_COLLECTIONS = 120
RAIL_SETTINGS = 121
RAIL_ENTRIES = (RAIL_AVATAR, RAIL_SEARCH, RAIL_HOME, RAIL_LIVE) + RAIL_LIBRARIES + (RAIL_ALL_LIBRARIES, RAIL_PLAYLISTS, RAIL_COLLECTIONS, RAIL_SETTINGS)
RAIL_AVATAR_Y = 86  # the avatar's middle; it is 100 tall, down to 136
RAIL_TOP = 184  # the first entry's middle, a gap under the avatar
RAIL_ROW = 80  # an entry's pill (40 on the Android TV app's canvas), as tall as the space between entries allows
RAIL_GAP = 8  # at least, between two pills
RAIL_PITCH = RAIL_ROW + RAIL_GAP
RAIL_SETTINGS_Y = 986  # Settings' middle: its foot on the safe area's, 1026
# How many entries fit from Search down at full height, Settings apart.
RAIL_FIT = (RAIL_SETTINGS_Y - RAIL_TOP) // RAIL_PITCH
RAIL_NAME, RAIL_SUBTITLE, RAIL_INITIAL, RAIL_PICTURE, RAIL_COLOR = 130, 131, 132, 133, 134


def rail_group(entry: int) -> int:
    """The group of a rail entry's button, icons and labels."""
    return entry + 100


def rail_icons(entry: int) -> tuple:
    """A rail entry's icon as it is drawn at rest, current and focused."""
    return entry * 10 + 1, entry * 10 + 2, entry * 10 + 3


def rail_labels(entry: int) -> tuple:
    return entry * 10 + 4, entry * 10 + 5, entry * 10 + 6


def rail_pill(entry: int) -> int:
    """The pill behind the current entry while the rail is open."""
    return entry * 10 + 7


def rail_pitch(entries: int) -> int:
    """How far apart `entries` entries from Search down are: closer than
    RAIL_PITCH when there are too many for the last to end a step above
    Settings (more than RAIL_FIT). Settings' pill is as tall as the others',
    so a full rail goes down to it evenly."""
    # From Search's middle, a step per entry down to Settings' middle.
    return min(RAIL_PITCH, (RAIL_SETTINGS_Y - RAIL_TOP) // entries)


def rail_row(pitch: int) -> int:
    """How tall an entry's pill is at `pitch`."""
    return min(RAIL_ROW, pitch - RAIL_GAP)


# Screens: each a group, shown alone.
SCREENS = {
    "home": 2000,
    "item": 3000,
    "library": 4000,
    "search": 5000,
    "grid": 6000,
    "notice": 7000,
    "libraries": 8000,
    "profiles": 9000,
    "live": 10000,
}

# A metadata line (TvMetaLine): a group (which the window moves) around a row
# of pieces, each a separator, a star, a text and a badge, of which the
# window shows what it says. Kodi's Python can't reach rows (grouplists)
# themselves, only what is in them and groups.
META_PIECES = 8


def meta_piece(line: int, i: int) -> tuple:
    """(separator, star, text, badge) of piece `i` of the line `line`."""
    base = line + 10 + i * 4
    return base, base + 1, base + 2, base + 3


# Home (TvHome): the hero at rest, the info block over the rows, the rows.
HOME_HERO = 2010
HOME_TITLE, HOME_CONTEXT, HOME_META, HOME_LEFT, HOME_SUMMARY = 2011, 2012, 2100, 2014, 2015
HOME_BUTTONS = 2016
HOME_BUTTONS_BOX = 2030  # the group around them, which the window moves
HOME_PLAY, HOME_FROM_START, HOME_INFO, HOME_WATCHED, HOME_UNWATCHED = 2017, 2018, 2019, 2020, 2021
HOME_INFO_BLOCK = 2050
HOME_INFO_TITLE, HOME_INFO_META, HOME_INFO_SUMMARY = 2051, 2150, 2053
HOME_ROWS = 2200
HOME_SLOTS = 10  # Continue Watching, My List and the libraries' rows

# An item's page (TvItemScreen).
ITEM_HERO = 3010
ITEM_TITLE, ITEM_CONTEXT, ITEM_META, ITEM_LEFT, ITEM_GENRES, ITEM_SUMMARY, ITEM_CREDITS = 3011, 3012, 3100, 3014, 3015, 3016, 3017
ITEM_BUTTONS = 3018
ITEM_BUTTONS_BOX = 3030
ITEM_PLAY, ITEM_FROM_START, ITEM_WATCHED, ITEM_UNWATCHED = 3019, 3020, 3021, 3022
ITEM_ADD_TO_LIST, ITEM_IN_LIST, ITEM_TRAILER, ITEM_MORE = 3023, 3024, 3025, 3026
ITEM_ADD_TO_PLAYLIST = 3027
ITEM_SHUFFLE = 3028  # an artist's or album's tracks, shuffled
ITEM_INFO_BLOCK = 3050
ITEM_INFO_TITLE, ITEM_INFO_EPISODE, ITEM_INFO_META, ITEM_INFO_SUMMARY = 3051, 3052, 3150, 3054
ITEM_INFO_LINE = 3055  # an episode's "Season 1 · Episode 3 · 45 min"
ITEM_SEASONS = 3060  # the season tabs
ITEM_ROWS = 3200
ITEM_SLOTS = 4  # episodes (an artist's albums, an album's tracks), extras, cast, more like this

# A library (TvLibraryScreen): its tabs, its controls, its grid; its
# Recommended tab's rows.
LIBRARY_TITLE = 4011
LIBRARY_TABS = 4012
LIBRARY_RECOMMENDED, LIBRARY_ALL, LIBRARY_COLLECTIONS = 4013, 4014, 4015
LIBRARY_CONTROLS = 4020
# The sort chip twice: ascending and descending (its icon says which).
LIBRARY_SORT, LIBRARY_SORT_DOWN, LIBRARY_UNWATCHED, LIBRARY_GENRE, LIBRARY_YEAR = 4021, 4121, 4023, 4024, 4025
LIBRARY_FILTER = 4022
# The filter chips twice too: off, and on (leading with a check). The
# resolution and the dynamic range have no chips of their own, which the
# row has no room for: Filter's twin says them while one is chosen.
LIBRARY_UNWATCHED_ON, LIBRARY_GENRE_ON, LIBRARY_YEAR_ON = 4123, 4124, 4125
LIBRARY_FILTER_ON = 4122
LIBRARY_COUNT = 4026
LIBRARY_GRID = 4030  # one list per grid shape
LIBRARY_EMPTY = 4040
LIBRARY_ROWS = 4200
LIBRARY_SLOTS = 5

# Search (TvSearchScreen).
SEARCH_FIELD = 5011
SEARCH_EMPTY_TITLE, SEARCH_EMPTY_BODY = 5012, 5013
SEARCH_ROWS = 5200
SEARCH_SLOTS = 7  # movies, shows, episodes, people, music, Live TV's channels and Discover

# A grid page: playlists, a playlist, collections, a collection, a person.
GRID_TITLE, GRID_SUBTITLE = 6011, 6012
GRID_LISTS = 6020
GRID_EMPTY = 6040

# A page with a sentence instead (TvHomeNotice).
NOTICE_TITLE, NOTICE_BODY, NOTICE_BUTTONS, NOTICE_FIRST, NOTICE_SECOND = 7011, 7012, 7013, 7014, 7015

# Every library (TvLibrariesScreen): its title, Sort libraries, a card for
# each, three to a row (80 by 40 dp apart, doubled).
LIBRARIES_TITLE, LIBRARIES_BUTTONS, LIBRARIES_SORT = 8011, 8012, 8013
LIBRARIES_LIST = 8020
LIBRARIES_TOP = 196
LIBRARIES_CARD = (522, 160)
LIBRARIES_GAP = 32

# "Who's watching?" (TvProfilesScreen), without the rail: the title, the
# household's faces in a row, centred while six fit, and what a refused
# switch said.
PROFILES_TITLE, PROFILES_NOTE = 9011, 9012
PROFILES_LIST = 9020
PROFILES_TOP = 408
PROFILE = 200  # a face, round
PROFILE_GAP = 80
PROFILES_FIT = 6
PROFILE_MARGIN = 30  # room in the list for a focused face's growth and ring


def profiles_left(count: int) -> int:
    """Where the row of `count` faces starts: centred while they fit, else
    at the safe area's edge, and it scrolls."""
    if count > PROFILES_FIT:
        return SAFE_X
    span = count * PROFILE + max(count - 1, 0) * PROFILE_GAP
    return (WIDTH - span) // 2


# Live TV (livetv.py), as the web's LiveTvPage lays it out:
# the categories (Search, Favourites, Recently watched, All channels and
# the groups, a region's under it), the channels of the one shown by
# number with what's on now under its name and count, and beside them the
# focused channel's programme now and next, which the skin reads from the
# list's own item.
LIVE_TITLE, LIVE_HEADING, LIVE_COUNT = 10011, 10012, 10013
LIVE_CATEGORIES = 10030
LIVE_LIST = 10020
LIVE_EMPTY = 10040
LIVE_PANEL = 10050
LIVE_TOP = 176
LIVE_CATEGORY_X = GUTTER - 16
LIVE_CATEGORY_WIDTH = 384
LIVE_CATEGORY_ROW = 72
LIVE_CATEGORY_ROWS = 12
LIVE_LIST_X = LIVE_CATEGORY_X + LIVE_CATEGORY_WIDTH + 24
LIVE_ROW = 124
LIVE_ROWS = 7
LIVE_LIST_WIDTH = 800
LIVE_LOGO = (160, 90)
LIVE_PANEL_X = LIVE_LIST_X + LIVE_LIST_WIDTH + 32
LIVE_PANEL_WIDTH = WIDTH - SAFE_X - LIVE_PANEL_X
LIVE_PICTURE_HEIGHT = LIVE_PANEL_WIDTH * 9 // 16


# Signing in (signin.py, TvLinkScreen): the code in two halves, the steps
# (the first only with a QR code, which the server draws), the QR code, and
# a third note when the server requires two-step sign-in.
SIGNIN_WINDOW = "pley-signin.xml"
SIGNIN_CHANGE = 3101  # Change server
SIGNIN_ADDRESS, SIGNIN_STATUS, SIGNIN_SCAN, SIGNIN_QR = 3104, 3106, 3107, 3108
SIGNIN_CODE_A, SIGNIN_CODE_B = 3111, 3112
SIGNIN_STEP_SCAN, SIGNIN_STEP_GO, SIGNIN_STEP_ENTER = 3113, 3118, 3119
SIGNIN_NUMBER_GO, SIGNIN_NUMBER_ENTER = 3114, 3115
SIGNIN_QR_CARD = 3120
SIGNIN_TWO_STEP = 3121

# A message at the foot of the page.
TOAST, TOAST_LABEL = 900, 901


def button_box(button: int) -> int:
    """The group of a round button and the label it shows under itself."""
    return button + 60


def button_label(button: int) -> int:
    return button + 70


def slot(rows: int, i: int) -> int:
    """The group of row slot `i` of the rows starting at `rows`."""
    return rows + i * 10


def slot_header(rows: int, i: int) -> int:
    return slot(rows, i) + 1


def slot_list(rows: int, i: int, shape: str) -> int:
    return slot(rows, i) + 2 + ROW_SHAPES.index(shape)


def grid_list(base: int, shape: str) -> int:
    return base + GRID_SHAPES.index(shape)


def find_slot(rows: int, slots: int, control: int):
    """(slot, shape) of a row list's id, or None."""
    offset = control - rows
    if offset < 0 or offset >= slots * 10:
        return None
    i, part = divmod(offset, 10)
    if part < 2 or part - 2 >= len(ROW_SHAPES):
        return None
    return i, ROW_SHAPES[part - 2]
