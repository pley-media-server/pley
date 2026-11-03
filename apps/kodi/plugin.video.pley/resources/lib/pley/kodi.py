"""The Kodi calls pley makes, in one place: settings, strings, the shared
properties the plugin and the service talk through, and what this Kodi is."""

from __future__ import annotations

import functools
import json
import os

import xbmc
import xbmcaddon
import xbmcgui
import xbmcvfs

ADDON_ID = "plugin.video.pley"
# Window 10000 (Home) lives as long as Kodi: properties set there are how the
# service tells the plugin where its proxy is, and the plugin tells the
# service what it started playing.
HOME = 10000
PROXY = "pley.proxy"
PLAYING = "pley.playing"
# Set while pley's own window is open: the service tells it of changes.
WINDOW_OPEN = "pley.ui"

# strings.po ids, by name.
TEXT = {
    "choose_server": 30000,
    "sign_in_to": 30001,
    "sign_out": 30002,
    "search": 30003,
    "playlists": 30004,
    "collections": 30005,
    "next_page": 30006,
    "enter_address": 30007,
    "looking": 30009,
    "sign_in": 30010,
    "signed_in_as": 30012,
    "signed_out": 30013,
    "code_expired": 30014,
    "code_refused": 30015,
    "not_reachable": 30016,
    "not_pley": 30017,
    "service_off": 30018,
    "mark_watched": 30019,
    "mark_unwatched": 30020,
    "add_to_list": 30021,
    "remove_from_list": 30022,
    "stream_stopped": 30024,
    "server_error": 30026,
    "settings": 30027,
    "nothing_found": 30030,
    "skip_intro": 30060,
    "skip_credits": 30061,
    "up_next": 30062,
    "play_now": 30063,
    "cancel": 30064,
    "cant_reach": 30065,
    "try_again": 30066,
    "cant_reach_hint": 30067,
    "waiting": 30073,
    "waiting_last_minute": 30074,
    "waiting_one_minute": 30075,
    "signed_in": 30076,
    "retrying": 30077,
    "stopped_by_admin": 30078,
    "stopped_elsewhere": 30079,
    "home": 30100,
    "from_start": 30101,
    "info": 30102,
    "in_my_list": 30103,
    "trailer": 30104,
    "more": 30105,
    "recommended": 30106,
    "library": 30107,
    "filter": 30108,
    "unwatched": 30109,
    "genre": 30110,
    "year": 30111,
    "resume": 30112,
    "play": 30113,
    "resume_episode": 30114,
    "play_episode": 30115,
    "see_all": 30116,
    "unavailable": 30117,
    "search_hint": 30118,
    "search_empty": 30119,
    "search_empty_body": 30120,
    "nothing_found_for": 30121,
    "movies": 30122,
    "shows": 30123,
    "episodes": 30124,
    "people": 30125,
    "music": 30126,
    "cast": 30127,
    "cast_and_crew": 30128,
    "crew": 30129,
    "guest_stars": 30130,
    "more_like_this": 30131,
    "extras": 30132,
    "home_failed": 30133,
    "page_failed": 30134,
    "no_libraries": 30135,
    "no_libraries_body": 30136,
    "nothing_yet": 30137,
    "nothing_yet_body": 30138,
    "scanning": 30139,
    "directed_by": 30140,
    "created_by": 30141,
    "starring": 30142,
    "sort_by": 30143,
    "sort_title": 30144,
    "sort_year": 30145,
    "sort_added": 30146,
    "sort_rating": 30147,
    "sort_release": 30148,
    "sort_duration": 30149,
    "sort_last_played": 30150,
    "any_genre": 30151,
    "any_year": 30152,
    "clear_filters": 30153,
    "remove_from_continue": 30154,
    "go_to_show": 30155,
    "go_to_season": 30156,
    "full_summary": 30157,
    "no_playlists": 30158,
    "no_collections": 30159,
    "no_matches": 30160,
    "look": 30161,
    "look_pley": 30162,
    "look_kodi": 30163,
    "library_empty": 30166,
    "mark_watched_short": 30167,
    "known_for": 30168,
    "playlist_items": 30170,
    "or_go_to": 30173,
    "go_to": 30179,
    "add_to_playlist": 30180,
    "new_video_playlist": 30181,
    "new_music_playlist": 30182,
    "added_to": 30183,
    "all": 30184,
    "save_as": 30185,
    "save_filters": 30186,
    "count_of": 30187,
    "sort_libraries": 30188,
    "server_order": 30189,
    "move_up": 30190,
    "move_down": 30191,
    "first": 30192,
    "after": 30193,
    "sort_needs_newer_server": 30194,
    "tracks": 30195,
    "audio_language": 30196,
    "subtitles": 30197,
    "subtitle_language": 30198,
    "files_default": 30199,
    "same_as_audio": 30200,
    "subtitles_off": 30201,
    "subtitles_forced": 30202,
    "subtitles_foreign": 30203,
    "subtitles_always": 30204,
    "tracks_on_web": 30205,
    "saved": 30206,
    "libraries": 30207,
    "open": 30208,
    "scanning_now": 30209,
    "sign_out_ask": 30210,
    "sign_out_of": 30211,
    "smoother": 30213,
    "shuffle": 30214,
    "albums": 30215,
    "tracks_row": 30216,
    "go_to_artist": 30217,
    "no_tracks": 30218,
    "whos_watching": 30219,
    "switch_profile": 30220,
    "pin_of": 30221,
    "wrong_pin": 30222,
    "too_many_pins": 30223,
    "pin_locked": 30224,
    "not_allowed": 30225,
    "profile_locked": 30226,
    "cant_switch": 30227,
    "needs_password": 30228,
    "pin_digits": 30229,
    "watching_as": 30230,
    "no_other_profile": 30231,
    "pin": 30232,
    "remove_from_playlist": 30233,
    "rename_playlist": 30234,
    "delete_playlist": 30235,
    "delete_playlist_ask": 30236,
    "delete_playlist_body": 30237,
    "delete": 30238,
    "playlist_deleted": 30239,
    "playlist_empty": 30240,
    "smart_playlist_empty": 30241,
    "your_rating": 30242,
    "not_rated": 30243,
    "subtitles_not_set_up": 30244,
    "subtitles_need_newer": 30245,
    "unencrypted": 30246,
    "live_tv": 30247,
    "favourites": 30249,
    "recently_watched": 30250,
    "add_favourite": 30253,
    "remove_favourite": 30254,
    "schedule": 30255,
    "watch": 30256,
    "next": 30257,
    "all_in_use": 30258,
    "no_channels": 30259,
    "no_channels_body": 30260,
    "no_channels_admin": 30261,
    "today": 30262,
    "tomorrow": 30263,
    "yesterday": 30264,
    "nothing_on_now": 30265,
    "no_guide": 30266,
    "later": 30267,
    "live": 30268,
    "new": 30269,
    "no_channel_numbered": 30270,
    "on_now": 30271,
    "nothing_scheduled": 30272,
    "all_channels": 30273,
    "search_channels": 30274,
    "you_elsewhere": 30275,
    "someone_else": 30276,
    "people_watching": 30277,
    "watch_here": 30278,
    "two_step_required": 30279,
    # Requests (requests.py, asking.py, news.py).
    "discover": 30447,
    "your_requests": 30448,
    "requests_page": 30449,
    "trending": 30450,
    "popular_movies": 30451,
    "popular_shows": 30452,
    "coming_soon": 30453,
    "popular_anime": 30454,
    "find_to_ask": 30455,
    "nothing_on_tmdb": 30456,
    "tmdb_credit": 30457,
    "in_library": 30458,
    "partly_in_library": 30459,
    "requested": 30460,
    "declined": 30461,
    "request": 30462,
    "request_more_seasons": 30463,
    "following": 30464,
    "stop_following": 30465,
    "tell_me_too": 30466,
    "someone_asked": 30467,
    "declined_because": 30468,
    "ask_again": 30469,
    "stage_pending": 30470,
    "stage_approved": 30471,
    "out": 30472,
    "airs": 30473,
    "not_out_yet": 30474,
    "searching": 30475,
    "downloading": 30476,
    "importing": 30477,
    "ready_seasons": 30478,
    "ready_to_watch": 30479,
    "gone": 30480,
    "couldnt_send": 30481,
    "not_sent_yet": 30482,
    "failed_waits": 30483,
    "trying_again": 30484,
    "download_queued": 30485,
    "download_paused": 30486,
    "download_warning": 30487,
    "asked_by_someone": 30488,
    "ready_cant_open": 30489,
    "can_ask_for": 30490,
    "limits": 30491,
    "movies_used_up": 30492,
    "seasons_used_up": 30493,
    "all_used_up": 30494,
    "later_in_text": 30495,
    "asked_pending": 30496,
    "asked_approved": 30497,
    "asked_failed": 30498,
    "asked_following": 30499,
    "which_seasons": 30500,
    "pick_a_season": 30501,
    "withdraw": 30502,
    "withdraw_ask": 30503,
    "remove": 30504,
    "remove_ask": 30505,
    "approve": 30506,
    "decline": 30507,
    "decline_reason": 30508,
    "handled": 30509,
    "delete_request_ask": 30510,
    "request_again": 30511,
    "ready": 30512,
    "season_ready": 30513,
    "seasons_ready": 30514,
    "news": 30515,
    "requests_off": 30516,
    "requests_off_body": 30517,
    "no_access": 30518,
    "no_access_body": 30519,
    "tmdb_down": 30520,
    "tmdb_key": 30521,
    "tmdb_key_fix": 30522,
    "tmdb_key_others": 30523,
    "more_in": 30524,
    "at_most": 30525,
    "sign_in_to_ask": 30526,
    "sign_in_to_ask_body": 30527,
    "sign_in_again": 30528,
    "sign_in_to_approve": 30529,
    "nothing_in_libraries": 30532,
    "needs_attention": 30533,
    "pending": 30534,
    "available": 30535,
    "failed": 30536,
    "movies_and_shows": 30537,
    "everyone": 30538,
    "nothing_asked": 30539,
    "nothing_needs_attention": 30540,
    "asked_for": 30541,
    "asked_by": 30542,
    "seasons": 30543,
    "episodes_in_library": 30544,
    "nothing_more": 30545,
    "movie": 30546,
    "show": 30547,
    "yours": 30548,
    "results_for": 30549,
    "another_device": 30550,
    "sort_user_rating": 30551,
    "sort_bitrate": 30552,
    "resolution": 30553,
    "dynamic_range": 30554,
    "any_resolution": 30555,
    "any_dynamic_range": 30556,
}


# One Addon and its strings for each call of the plugin (begin()): a listing
# names each item's menu, and an Addon made each time reads the settings
# again.
_call = {}


def begin() -> None:
    _call.clear()


def addon() -> xbmcaddon.Addon:
    if "addon" not in _call:
        _call["addon"] = xbmcaddon.Addon(ADDON_ID)
    return _call["addon"]


def text(name: str) -> str:
    strings = _call.setdefault("strings", {})
    if name not in strings:
        strings[name] = addon().getLocalizedString(TEXT[name])
    return strings[name]


def setting(name: str) -> str:
    return addon().getSettingString(name)


def setting_int(name: str) -> int:
    return addon().getSettingInt(name)


def set_setting(name: str, value: str) -> None:
    addon().setSettingString(name, value)


def set_setting_int(name: str, value: int) -> None:
    addon().setSettingInt(name, value)


def profile_dir() -> str:
    path = xbmcvfs.translatePath(addon().getAddonInfo("profile"))
    os.makedirs(path, exist_ok=True)
    return path


# What this Kodi remembers between its runs, as the person left it (the tab
# each library was last on): a small file in the add-on's data folder, this
# device's own.
REMEMBERED = "remembered.json"


def remembered(key: str, default: str = "") -> str:
    try:
        with open(os.path.join(profile_dir(), REMEMBERED), encoding="utf-8") as f:
            found = json.load(f)
    except (OSError, ValueError):
        return default
    value = found.get(key) if isinstance(found, dict) else None
    return value if isinstance(value, str) else default


def remember(key: str, value: str) -> None:
    path = os.path.join(profile_dir(), REMEMBERED)
    try:
        with open(path, encoding="utf-8") as f:
            found = json.load(f)
    except (OSError, ValueError):
        found = {}
    if not isinstance(found, dict):
        found = {}
    if found.get(key) == value:
        return
    found[key] = value
    # Written whole, then put in place: a power cut leaves the old file.
    temporary = path + ".new"
    try:
        with open(temporary, "w", encoding="utf-8") as f:
            json.dump(found, f)
        os.replace(temporary, path)
    except OSError as e:
        log("couldn't remember %s: %s" % (key, e), xbmc.LOGWARNING)


def temp_dir() -> str:
    return xbmcvfs.translatePath("special://temp/")


def addon_dir() -> str:
    return xbmcvfs.translatePath(addon().getAddonInfo("path"))


def log(message: str, level: int = xbmc.LOGINFO) -> None:
    xbmc.log("[pley] " + message, level)


def refresh_listing() -> None:
    """Refreshes the listing on screen if it is pley's, and pley's window if
    it is open (window.py hears it); others aren't touched."""
    if xbmc.getInfoLabel("Container.FolderPath").startswith("plugin://%s/" % ADDON_ID):
        xbmc.executebuiltin("Container.Refresh")
    if get_property(WINDOW_OPEN):
        xbmc.executebuiltin("NotifyAll(%s,changed)" % ADDON_ID)


def notify(message: str, error: bool = False, icon=None, ms: int = 5000) -> None:
    """A notice; `icon`: a picture's address (a poster) for Kodi's own icon."""
    if not icon:
        icon = xbmcgui.NOTIFICATION_ERROR if error else xbmcgui.NOTIFICATION_INFO
    xbmcgui.Dialog().notification("Pley", message, icon, ms)


def ask_sign_out() -> bool:
    """Sign out?, with the server it signs out of: whether the person chose
    Sign out."""
    server = setting("server_name") or setting("server_url")
    line = text("sign_out_of") % server if server else ""
    return bool(xbmcgui.Dialog().yesno(text("sign_out_ask"), line, nolabel=text("cancel"), yeslabel=text("sign_out")))


def get_property(name: str) -> str:
    return xbmcgui.Window(HOME).getProperty(name)


def set_property(name: str, value: str) -> None:
    xbmcgui.Window(HOME).setProperty(name, value)


def clear_property(name: str) -> None:
    xbmcgui.Window(HOME).clearProperty(name)


# What this Kodi is doesn't change while it runs, and the plugin's Python
# lives as long (reuselanguageinvoker): asked once. Each answer waits for
# Kodi's GUI, which is busy for whole frames while playback starts.
@functools.lru_cache(maxsize=None)
def kodi_version() -> str:
    """"21.3" of "21.3 (21.3.0) Git:…"."""
    return xbmc.getInfoLabel("System.BuildVersion").split(" ")[0] or "unknown"


@functools.lru_cache(maxsize=None)
def platform() -> str:
    for condition, name in (
        ("System.Platform.Android", "Android"),
        ("System.Platform.Windows", "Windows"),
        ("System.Platform.OSX", "macOS"),
        ("System.Platform.IOS", "iOS"),
        ("System.Platform.TVOS", "tvOS"),
        ("System.Platform.Linux", "Linux"),
    ):
        if xbmc.getCondVisibility(condition):
            return name
    return "unknown"


@functools.lru_cache(maxsize=None)
def is_coreelec() -> bool:
    try:
        with open("/etc/os-release", encoding="utf-8") as release:
            return "coreelec" in release.read().lower()
    except OSError:
        return False


@functools.lru_cache(maxsize=None)
def user_agent() -> str:
    version = addon().getAddonInfo("version")
    return "pleyKodi/{} (Kodi {}; {})".format(version, kodi_version(), platform())


def setting_value(name: str):
    """One of Kodi's own settings, through JSON-RPC (None when it has no
    such setting)."""
    return (rpc("Settings.GetSettingValue", {"setting": name}) or {}).get("value")


def rpc(method: str, params: dict):
    """A JSON-RPC call to Kodi: its result, None when refused."""
    request = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    try:
        answer = json.loads(xbmc.executeJSONRPC(json.dumps(request)))
    except ValueError:
        return None
    if not isinstance(answer, dict) or "error" in answer:
        log("%s: %s" % (method, (answer or {}).get("error") if isinstance(answer, dict) else answer), xbmc.LOGDEBUG)
        return None
    return answer.get("result")
