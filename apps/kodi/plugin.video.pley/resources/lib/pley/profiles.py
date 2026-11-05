"""The household's profiles ("Who's watching?") and switching to them, as
pley's TV apps do (docs/API.md, Profiles): through the service, which alone
holds and renews the sign-in (auth.Tokens.switch_profile, the proxy's
/switch-profile). A PIN is typed in Kodi's own number dialog and never
kept; a profile that needs its password is signed in to again with a code,
picked on the phone that approves it."""

from __future__ import annotations

import xbmc

from . import api, kodi

# PINs are 4 to 8 digits.
PIN_MIN, PIN_MAX = 4, 8
# Kept in Kodi's Home window, which lives as long as Kodi: "Who's watching?"
# was answered (a profile picked, a PIN checked, a sign-in approved), so a
# start asks it again only after Kodi starts over.
GATE = "pley.gate"
# Set while that "Who's watching?" is on screen: the service plays no cast
# from the user's remote past it and its PINs (casting.py).
PICKING = "pley.picking"
# What the plugin, or the service for a cast, tells pley's window when it
# switched over it.
SWITCHED = "pley.switched"


def household(client: api.Client):
    """The household (GET /api/oauth/profiles): [] for someone outside one,
    or a server from before profiles; None for a sign-in that can't switch
    (made before the add-on asked for `profiles`)."""
    try:
        found = client.get("/api/oauth/profiles")
    except api.ApiError as e:
        if e.status == 403:
            return None
        if e.status == 404:
            return []
        raise
    if not isinstance(found, list):
        return []
    return [p for p in found if isinstance(p, dict) and p.get("id")]


def switch(client: api.Client, profile_id: int, pin=None) -> None:
    """Acts as `profile_id` from now on (with the current profile and its
    PIN: checks the PIN). Raises api.ApiError, its code the server's reason
    ("wrong_pin", "throttled", "pin_locked", "browser", "not_allowed"), else
    its OAuth error, "signed_out", "unreachable" or "another_device"."""
    client.json("POST", "/switch-profile", json_body={"profile": profile_id, "pin": pin})


def tell_switched() -> None:
    """The add-on acts as another profile now (the plugin's switch, or the
    service's for a cast): "Who's watching?" is answered, and Pley's window,
    when open, shows the new profile (window.Monitor hears SWITCHED)."""
    kodi.set_property(GATE, "1")
    if kodi.get_property(kodi.WINDOW_OPEN):
        xbmc.executebuiltin("NotifyAll(%s,%s)" % (kodi.ADDON_ID, SWITCHED))


def asks_at_start(household, me: dict, answered: bool) -> bool:
    """Whether Kodi's start asks "Who's watching?": in a household of more
    than one profile, when the profile in use has a PIN (GET /api/auth/me's
    has_pin), or the server didn't say, until it is answered once Kodi
    started. Without a PIN it goes straight in, as the TV apps do."""
    if answered or not household or len(household) < 2:
        return False
    return me.get("has_pin") is not False


def needs_pin(profile: dict, me: dict) -> bool:
    """Whether `profile` has a PIN to ask, which its lock shows: another one
    whose switch takes it, or the one in use when it has one (GET
    /api/auth/me's has_pin: the server says switching to it needs nothing),
    asked at a start, as the TV apps ask it."""
    if profile.get("current"):
        return bool(me.get("has_pin") and me.get("id") == profile.get("id"))
    return profile.get("needs") == "pin"


def refusal(error: api.ApiError, text, server: str) -> str:
    """Why a switch was refused, in a sentence (`text`: kodi.text)."""
    code = error.code
    if code == "wrong_pin":
        return text("wrong_pin")
    if code == "throttled":
        # The server's own sentence says how long to wait.
        return sentence(error.message) if error.message else text("too_many_pins")
    if code == "pin_locked":
        return text("pin_locked")
    if code == "not_allowed":
        return text("not_allowed")
    if code in ("unauthorized_client", "insufficient_scope"):
        return text("cant_switch")
    if code in ("unreachable", "service_off"):
        return text("cant_reach") % server
    if code == "another_device":
        return text("another_device") % server
    return text("server_error") % error.message


def sentence(words: str) -> str:
    words = (words or "").strip()
    if not words:
        return ""
    words = words[0].upper() + words[1:]
    return words if words.endswith((".", "!", "?")) else words + "."
