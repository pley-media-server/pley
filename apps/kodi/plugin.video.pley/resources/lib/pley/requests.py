"""Requests (docs/API.md, Requests): what the summary lets this profile do,
what a TMDB title or a request says of itself, and what can be done about
it, in the words every pley app uses. No Kodi here: Kodi's lists
(asking.py), the service's notices (news.py) and the tests use it, each
with kodi.text (`text`)."""

from __future__ import annotations

import urllib.parse

from . import api, words
from . import layout as L
from .profiles import sentence

# The summary as the service last read it (news.py), for the plugin: JSON,
# {"enabled": false} on a server that takes no requests.
SUMMARY = "pley.requests"
# Set by Your requests while something on it downloads or imports: its
# address, refreshed by the service every 10 seconds while on screen.
LIVE = "pley.requests.live"
# What the plugin tells the service (NotifyAll) when it asked for something,
# did something to a request, saw them or switched profile: the summary is
# read again.
ASKING = "pley.asking"

# The stage pills' colours (pending amber; on its way blue; ready green;
# refused red; gone gray), in pley's palette.
AMBER = "FFF59E0B"
BLUE = L.ACCENT
GREEN = "FF3FB27F"
RED = L.DANGER_TEXT
GRAY = L.MUTED
STAGE_COLORS = {
    "pending": AMBER,
    "approved": BLUE,
    "waiting": BLUE,
    "searching": BLUE,
    "downloading": BLUE,
    "importing": BLUE,
    "partial": GREEN,
    "available": GREEN,
    "declined": RED,
    "failed": RED,
    "gone": GRAY,
}
# Requests still on their way: a title is "requested" while one covers it.
ACTIVE = ("pending", "approved", "failed")
# Stages no event follows (a download's progress): their lists are asked
# for again every 10 seconds.
MOVING = ("downloading", "importing")
# Discover's lists (GET /api/discover/{list}), with their names' strings.
LISTS = (
    ("trending", "trending"),
    ("movies", "popular_movies"),
    ("shows", "popular_shows"),
    ("upcoming", "coming_soon"),
    ("anime", "popular_anime"),
)
# MediaRequest.actions, by their labels' strings.
ACTIONS = {
    "withdraw": "withdraw",
    "unwatch": "stop_following",
    "approve": "approve",
    "retry": "try_again",
    "decline": "decline",
    "handled": "handled",
    "delete": "delete",
    "ask_again": "request_again",
}
# Listings that show the summary's number (refreshed when it changes), and
# those that show requests (refreshed when one changes). Discover's rows
# never are.
COUNTED = ("", "root", "discover")
LISTED = ("requests", "seasons")
# The longest words TMDB is searched for, in characters: the server refuses
# longer ones.
QUERY_MAX = 200


def summary(client) -> dict | None:
    """GET /api/requests/summary. None from a server that has no requests
    (404), or none for apps yet (403 insufficient_scope): requests are off
    there, and asked about again only when the event stream opens again
    or Kodi starts. Raises api.ApiError otherwise."""
    try:
        found = client.get("/api/requests/summary")
    except api.ApiError as e:
        if e.status == 404 or e.code == "insufficient_scope":
            return None
        raise
    return found if isinstance(found, dict) else None


def discovers(s) -> bool:
    """Whether Discover shows: requests are on and this account may ask."""
    return bool(s) and bool(s.get("enabled")) and s.get("access") != "off"


def may_ask(s) -> bool:
    """Discover's rows, search's Discover row and asking: Discover, from a
    sign-in that can ask (one made before apps could only says so)."""
    return discovers(s) and s.get("sign_in_again") != "requests"


def lists(s) -> bool:
    """Whether Your requests shows (Requests, for managers)."""
    if not s or not s.get("enabled") or s.get("sign_in_again") == "requests":
        return False
    return s.get("access") != "off" or (s.get("own") or 0) > 0 or bool(s.get("manage"))


def manages(s) -> bool:
    """Whether this sign-in decides on everyone's requests."""
    return bool(s) and bool(s.get("enabled")) and bool(s.get("manage"))


def count(s) -> tuple:
    """The number the requests entry carries, and whether it is in the
    danger colour: for managers what waits for them and the news, red
    while a request couldn't be sent; for others their news."""
    if not s:
        return 0, False
    if manages(s):
        failed = s.get("failed") or 0
        return (s.get("pending") or 0) + failed + (s.get("unseen") or 0), failed > 0
    return s.get("unseen") or 0, False


def entry_label(s, text) -> str:
    """The name of the entry that opens the requests, with its number: "Your
    requests (2)", "Requests (3)" for managers (its number red while one
    couldn't be sent)."""
    number, danger = count(s)
    label = text("requests_page") if manages(s) else text("your_requests")
    if number:
        label += " " + colored("({:,})".format(number), RED if danger else None)
    return label


def colored(text: str, color) -> str:
    return "[COLOR %s]%s[/COLOR]" % (color, text) if text and color else text or ""


def search_text(query: str) -> str:
    """Words as the server takes them for TMDB: trimmed, and cut to
    QUERY_MAX characters."""
    return query.strip()[:QUERY_MAX].strip()


def showing(path: str, routes) -> bool:
    """Whether the pley listing at `path` (Container.FolderPath) is one of
    `routes` (COUNTED, LISTED)."""
    prefix = "plugin://plugin.video.pley/"
    if not path.startswith(prefix):
        return False
    query = urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)
    return (query.get("route") or [""])[0] in routes


# ------------------------------------------------------------------ titles


def badge(title: dict, text) -> tuple:
    """A TMDB title's badge (DiscoverTitle.status) and its colour; ("",
    None) for none (it may be asked for, or can't be)."""
    status = title.get("status")
    if status == "library":
        return text("in_library"), GREEN
    if status == "partial":
        # Partly in a library this account can open; else partly asked for.
        return (text("partly_in_library"), GREEN) if title.get("item_id") else (text("requested"), BLUE)
    if status == "requested":
        return text("requested"), BLUE
    if status == "declined":
        return text("declined"), RED
    return "", None


def own(detail: dict):
    """This account's active request for the title, if it made one."""
    return next((r for r in detail.get("requests") or [] if r.get("mine") and r.get("status") in ACTIVE), None)


def followed(detail: dict):
    """The active request for the title this account follows, if any."""
    return next((r for r in detail.get("requests") or [] if r.get("watching") and r.get("status") in ACTIVE), None)


def used_up(detail: dict) -> bool:
    """Whether nothing more of the title's kind may be asked for now."""
    left = detail.get("seasons_left" if detail.get("kind") == "show" else "movies_left")
    return left is not None and left <= 0


def main_action(detail: dict, s) -> str:
    """What a title's page offers first: "sign_in",
    "restricted", "open", "in_library", "declined", "requested",
    "following", "tell_me_too", "more_seasons", "request", and in place of
    asking "no_access" or "used_up"."""
    if s and s.get("sign_in_again") == "requests":
        return "sign_in"
    status = detail.get("status")
    if status == "restricted":
        return "restricted"
    if status == "library":
        return "open" if detail.get("item_id") else "in_library"
    if status == "declined":
        return "declined"
    if status == "requested":
        if own(detail) is not None:
            return "requested"
        if detail.get("watching"):
            return "following"
        wanted = "tell_me_too"
    elif status == "partial":
        if detail.get("item_id"):
            return "open"
        wanted = "more_seasons"
    else:
        wanted = "request"
    if not discovers(s):
        return "no_access"
    # Following costs nothing against the limits.
    if wanted != "tell_me_too" and used_up(detail):
        return "used_up"
    return wanted


def declined_line(detail: dict, text, now: float) -> str:
    """"Declined: “reason”. You can ask again in 12 days." (no reason for a
    follower's)."""
    declined = detail.get("declined") or {}
    reason = (declined.get("reason") or "").strip()
    line = text("declined_because") % reason if reason else text("declined") + "."
    if declined.get("again_at"):
        line += " " + text("ask_again") % words.relative(declined["again_at"], now)
    return line


# ----------------------------------------------------------------- seasons


def ordered(seasons) -> list:
    """A show's seasons in number order, specials last."""
    return sorted(seasons or [], key=lambda s: (s.get("number") == 0, s.get("number") or 0))


def askable(season: dict, detail: dict) -> bool:
    """Whether a season may be asked for: missing, partly in a library or
    not aired yet, not declined, of a title this account isn't kept from."""
    if detail.get("status") == "restricted" or season.get("declined"):
        return False
    return season.get("state") in ("missing", "partial", "unaired")


def more_seasons(detail: dict) -> bool:
    """Whether "Request more seasons" shows: a season other than specials
    that has aired is missing or partly there, and may be asked for."""
    return any(
        s.get("number") != 0 and s.get("state") in ("missing", "partial") and askable(s, detail) for s in detail.get("seasons") or []
    )


def picker(detail: dict, only=None) -> tuple:
    """The season picker: (the seasons it lists, the positions ticked).
    Kodi's list can't hold a row that can't be ticked, so it lists the
    seasons that may be asked for, specials last. It ticks the missing and
    partial ones but specials, lowest first, up to what may be asked for
    (`only`: that season alone); unaired seasons and specials are offered
    unticked."""
    listed = [s for s in ordered(detail.get("seasons")) if askable(s, detail)]
    if only is not None:
        return listed, [i for i, s in enumerate(listed) if s.get("number") == only]
    ticked = [i for i, s in enumerate(listed) if s.get("number") != 0 and s.get("state") in ("missing", "partial")]
    left = detail.get("seasons_left")
    return listed, ticked if left is None else ticked[: max(left, 0)]


def season_name(season: dict) -> str:
    return season.get("name") or words.season_title(None, season.get("number"))


def season_state(season: dict, detail: dict, text, manage: bool, now: float) -> tuple:
    """What a season of a TMDB show says of itself, and its colour: in a
    library, its request's stage, declined, partly there, when it airs,
    or how many episodes it has."""
    state = season.get("state")
    if state == "library":
        return text("in_library"), GREEN
    if state == "requested":
        request = next((r for r in detail.get("requests") or [] if r.get("id") == season.get("request_id")), None)
        return stage(request, text, manage, now) if request is not None else (text("requested"), BLUE)
    if season.get("declined"):
        return text("declined"), RED
    if state == "partial":
        return text("episodes_in_library") % (season.get("in_library") or 0, season.get("aired") or 0), None
    if state == "unaired":
        aired = season.get("air_date")
        return (text("airs") % words.day(aired, now) if aired else text("not_out_yet")), None
    return words.plural(season.get("episodes") or 0, "episode"), None


# ---------------------------------------------------------------- requests


def downloading(download, text, now: float) -> str:
    """A download's line: "Downloading 42% · in 20 min", or its state."""
    state = (download or {}).get("state")
    if state == "queued":
        return text("download_queued")
    if state == "paused":
        return text("download_paused")
    if state == "warning":
        return text("download_warning")
    if state == "importing":
        return text("importing")
    if not download:
        return text("downloading")
    line = "%s %d%%" % (text("downloading"), int(100 * min(max(download.get("progress") or 0, 0), 1)))
    eta = download.get("eta")
    return line + " · " + words.relative(eta, now) if eta and eta > now else line


def stage(request: dict, text, manage: bool, now: float) -> tuple:
    """A request's stage pill, and its colour."""
    kind = request.get("stage") or request.get("status")
    color = STAGE_COLORS.get(kind, BLUE)
    if kind == "pending":
        return text("stage_pending"), color
    if kind == "approved":
        return text("stage_approved"), color
    if kind == "waiting":
        at = request.get("stage_at")
        if not at:
            return text("not_out_yet"), color
        # A day, or within a day how soon.
        when = words.relative(at, now) if 0 < at - now < words.DAY else words.day(at, now)
        return (text("airs") if request.get("kind") == "show" else text("out")) % when, color
    if kind == "searching":
        return text("searching"), color
    if kind == "downloading":
        return downloading(request.get("download"), text, now), color
    if kind == "importing":
        return text("importing"), color
    if kind == "partial":
        ready = text("ready_seasons") % words.seasons_label(request.get("available_seasons"))
        going = downloading(request["download"], text, now) if request.get("download") else ""
        return " · ".join(p for p in (ready, going) if p), color
    if kind == "available":
        return text("ready_to_watch"), color
    if kind == "gone":
        return text("gone"), color
    if kind == "declined":
        return text("declined"), color
    if kind == "failed":
        if manage and request.get("reason"):
            return text("couldnt_send") % request["reason"], color
        return text("not_sent_yet"), color
    return "", None


def details(request: dict, text, manage: bool, now: float) -> list:
    """What a request says under its pill: why it waits, the reason it was
    declined, a library its maker can't open, the managers' note."""
    lines = []
    kind = request.get("stage")
    if kind == "failed":
        if request.get("retry_at"):
            lines.append(text("trying_again") % words.relative(request["retry_at"], now))
        elif not manage:
            lines.append(text("failed_waits"))
    if kind == "declined" and (request.get("reason") or "").strip():
        lines.append("“%s”" % request["reason"].strip())
    if kind in ("available", "partial") and not request.get("item_id"):
        lines.append(text("ready_cant_open"))
    if manage and request.get("note"):
        lines.append(request["note"])
    return lines


def asked_line(request: dict, text, manage: bool, now: float) -> str:
    """Who asked, and when: "Asked for 3 days ago", for managers "Ann asked
    for it 3 days ago"; "Asked for by someone else" where it isn't said."""
    who = request.get("requested_by")
    if not who:
        return text("asked_by_someone")
    when = words.relative(request.get("created_at"), now)
    if manage and not request.get("mine"):
        return text("asked_by") % (who.get("name") or "", when)
    return text("asked_for") % when


def actions(request: dict, text) -> list:
    """What this session may do to `request` now (its actions, in the
    server's order), as (action, label): withdrawing a declined one
    removes it; Decline comes with Decline with a reason… (Kodi's keyboard)."""
    found = []
    for action in request.get("actions") or []:
        if action == "withdraw":
            found.append((action, text("remove") if request.get("stage") == "declined" else text("withdraw")))
        elif action == "decline":
            found.append((action, text("decline")))
            found.append(("decline_reason", text("decline_reason")))
        elif action in ACTIONS:
            found.append((action, text(ACTIONS[action])))
    return found


def moving(found: list) -> bool:
    """Whether any of these requests downloads or imports (refreshed often)."""
    return any(r.get("stage") in MOVING or (r.get("stage") == "partial" and r.get("download")) for r in found)


# ------------------------------------------------------------------ asking


def ask(client, kind: str, tmdb_id: int, seasons=None) -> tuple:
    """POST /api/requests: (the request, whether this account follows one
    already there rather than asking anew: 200, not 201). Raises
    api.ApiError."""
    body = {"kind": kind, "tmdb_id": tmdb_id}
    if kind == "show" and seasons is not None:
        body["seasons"] = sorted(seasons)
    answer = client.request("POST", "/api/requests", json_body=body)
    if answer.status >= 400:
        raise api.error_of(answer)
    try:
        found = answer.json() or {}
    except ValueError:
        found = {}
    return found, answer.status == 200


def asked(request: dict, following: bool, text, title: str = "") -> str:
    """What asking did, in a sentence."""
    title = request.get("title") or title
    if following:
        return text("asked_following") % title
    if request.get("status") == "approved":
        return text("asked_approved") % title
    if request.get("status") == "failed":
        return text("asked_failed") % title
    return text("asked_pending") % title


def allowance(left: dict, s, text, now: float) -> list:
    """The allowance lines: what this account may still
    ask for in 7 days, or when it may ask for more, then its limits; none
    without limits. `left`: the title's details or the summary."""
    movies, seasons = left.get("movies_left"), left.get("seasons_left")
    if movies is None and seasons is None:
        return []

    def when(name):
        at = left.get(name)
        return words.relative(at, now) if at else text("later_in_text")

    lines = []
    if movies == 0 and seasons == 0:
        lines.append(text("all_used_up") % (when("movies_next_at"), when("seasons_next_at")))
    else:
        if movies == 0:
            lines.append(text("movies_used_up") % when("movies_next_at"))
        if seasons == 0:
            lines.append(text("seasons_used_up") % when("seasons_next_at"))
        more = []
        if movies:
            more.append(words.plural(movies, "more movie", "more movies"))
        if seasons:
            more.append(words.plural(seasons, "more season", "more seasons"))
        if more:
            lines.append(text("can_ask_for") % " and ".join(more))
    limits = []
    if (s or {}).get("movies_limit") is not None and movies is not None:
        limits.append(words.plural(s["movies_limit"], "movie"))
    if (s or {}).get("seasons_limit") is not None and seasons is not None:
        limits.append(words.plural(s["seasons_limit"], "season"))
    if limits:
        lines.append(text("limits") % " and ".join(limits))
    return lines


# ---------------------------------------------------------------- refusals


def problem(error: api.ApiError, text, manage: bool = False, s=None, kind=None, now: float = 0.0, scope: str = "requests") -> tuple:
    """What to say when requests can't be used, or asking was refused:
    (title, body, whether Try again may help). `scope`:
    what the refused call needed (deciding needs `approvals`)."""
    code, body = error.code, error.body or {}
    if error.status == 503 and code == "unavailable" and not body.get("setup"):
        return text("requests_off"), text("requests_off_body"), False
    if code == "tmdb_key" or (error.status == 503 and body.get("retry") is False):
        return text("tmdb_key"), text("tmdb_key_fix") if manage else text("tmdb_key_others"), False
    if error.status == 503 and (code == "tmdb_down" or body.get("setup")):
        return text("tmdb_down"), "", True
    if code == "no_access":
        return text("no_access"), text("no_access_body"), False
    if code == "insufficient_scope":
        if scope == "approvals":
            return text("sign_in_to_approve"), "", False
        return text("sign_in_to_ask"), text("sign_in_to_ask_body"), False
    if code == "declined" and body.get("retry_at"):
        return text("ask_again") % words.relative(body["retry_at"], now), "", False
    if code == "too_many_requests":
        if body.get("retry_at"):
            return text("more_in") % words.relative(body["retry_at"], now), "", False
        limit = (s or {}).get("seasons_limit" if kind == "show" else "movies_limit")
        if limit is not None:
            noun = words.plural(limit, "season") if kind == "show" else words.plural(limit, "movie")
            return text("at_most") % noun, "", False
    if code in ("restricted", "in_library", "conflict", "declined", "too_many_requests") or error.status in (400, 409):
        return sentence(error.message), "", False
    if error.status == 503:
        return text("tmdb_down"), "", True
    return text("server_error") % error.message, "", False


def problem_text(found: tuple) -> str:
    """problem()'s title and body as one notice."""
    title, body = found[0], found[1]
    return sentence(title) + " " + body if body else title


# ----------------------------------------------------------------- notices


def ready(event: dict, text) -> str:
    """A ready-to-watch notice (request_available): the title, or for a
    show partly there, its seasons newly ready."""
    title = event.get("title") or ""
    seasons = event.get("seasons") or []
    if event.get("complete", True) or event.get("kind") != "show" or not seasons:
        return text("ready") % title
    label = words.seasons_label(seasons)
    one = len([n for n in seasons if n > 0]) == 1 and 0 not in seasons
    return (text("season_ready") if one else text("seasons_ready")) % (label, title)


def news(count_: int, text) -> str:
    """"3 of your requests have news"."""
    return text("news") % "{:,}".format(count_)


def ready_now(request: dict) -> bool:
    """Whether a request is news a TV pops up: ready to watch, or partly,
    in a library this account can open."""
    return request.get("stage") in ("available", "partial") and bool(request.get("item_id"))


def as_event(request: dict) -> dict:
    """A request ready to watch, as its request_available would say it."""
    partial = request.get("stage") == "partial"
    return {
        "request_id": request.get("id"),
        "title": request.get("title"),
        "kind": request.get("kind"),
        "item_id": request.get("item_id"),
        "poster_url": request.get("poster_url"),
        "seasons": request.get("available_seasons") if partial else None,
        "complete": not partial,
    }
