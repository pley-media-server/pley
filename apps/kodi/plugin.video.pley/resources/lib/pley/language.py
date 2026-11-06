"""What the add-on learns of the language to show (kodi.language works it
out): the account's own and the server's, heard from the server, and the
profile's own, chosen in Kodi's list dialog as the other apps' Settings →
Profile → Language has it (docs/API.md, Language).

A Pley from before languages keeps no account's language: its
GET /api/server names none, and PUT /api/me/language answers 404 (or
405). There
Profile language is this device's choice, the add-on's Language setting,
as Android's Language row is on such a server: a choice shows at once and
stays on this device."""

from __future__ import annotations

from . import api, i18n, kodi

# What a Pley from before languages answers a choice of the account's.
NOT_KEPT = (404, 405)


def answer(response):
    """A response's JSON object, None when there is none."""
    if response is None or response.status != 200:
        return None
    try:
        found = response.json()
    except ValueError:
        return None
    return found if isinstance(found, dict) else None


def hear(call) -> None:
    """The account's language and the server's, asked as the service's event
    stream opens (as Kodi starts, on another sign-in or profile, when the
    server is back): `call(method, path)` is the service's request. The
    add-on changes language when they make it another."""
    me = answer(call("GET", "/api/auth/me"))
    if me is not None:
        kodi.heard_account(me)
    found = answer(call("GET", "/api/server"))
    if found is not None:
        kodi.heard_server(found)
    kodi.follow_language()


def choices(on_device: bool = False) -> list:
    """The choices, (value, label): Automatic with what that is here, then
    each language named in itself. `on_device`: this device's choice,
    whose Automatic is Kodi's language (else the server's), rather than the
    account's, whose Automatic is this device's."""
    automatic = kodi.device_automatic() if on_device else kodi.automatic_language()
    first = kodi.text("automatic_language") % kodi.text_of(kodi.LANGUAGE_NAMES[automatic])
    return [(None, first)] + [(code, kodi.text_of(kodi.LANGUAGE_NAMES[code])) for code in i18n.LANGUAGES]


def choose(client, me: dict, select, say) -> dict | None:
    """The profile's own language, on all its devices (PUT /api/me/language),
    or this device's on a server that keeps none: `select(heading, labels,
    preselect)` is Kodi's list dialog, `say(words, error)` tells the
    person. The profile as it is now when the language changed, else None."""
    on_device = not kodi.keeps_account_language()
    options = choices(on_device)
    now = i18n.supported(kodi.device_choice() if on_device else (me or {}).get("language"))
    at = next((i for i, (value, _) in enumerate(options) if value == now), 0)
    chosen = select(kodi.text("profile_language"), [label for _, label in options], at)
    if chosen < 0 or chosen == at:
        return None
    value = options[chosen][0]
    if on_device:
        return on_this_device(me, value)
    try:
        found = client.put("/api/me/language", {"language": value})
    except api.ApiError as e:
        if e.status in NOT_KEPT:
            # A Pley from before languages, though it names its own.
            kodi.keep_no_account_language()
            return on_this_device(me, value)
        say(kodi.text("server_error") % e.message, True)
        return None
    profile = found if isinstance(found, dict) and "language" in found else dict(me or {}, language=value)
    kodi.heard_account(profile)
    kodi.follow_language()
    return profile


def on_this_device(me: dict, value) -> dict:
    """`value` as this device's language, shown at once."""
    kodi.choose_device_language(value)
    kodi.follow_language()
    return dict(me or {})
