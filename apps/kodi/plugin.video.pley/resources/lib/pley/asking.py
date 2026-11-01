"""Asking for movies and shows, in Kodi's lists (docs/API.md, Requests):
Discover's search and lists of TMDB titles, each doing its main action on
OK and the rest from its menu; a show's seasons with their state, and the
season picker in Kodi's multiselect; Your requests (Requests, for those
who decide on them) with what can be done to each; and global search's
titles from TMDB. Plugin (plugin.py) takes these routes from Asking."""

from __future__ import annotations

import json
import threading
import time

import xbmc
import xbmcgui
import xbmcplugin

from . import api, kodi, listing, requests, words
from .profiles import sentence

# Titles a list shows at once, at least. TMDB's pages hold 20, fewer (even
# none) where titles were left out, so more are read, up to PAGES.
SCREENFUL = 20
PAGES = 5
REQUESTS_PAGE = 50
# Pictures at the sizes TMDB has.
POSTER = 500
BACKDROP = 1280
SEASON_POSTER = 342
# Global search's titles from TMDB, kept for the listing that shows them
# once they came (Searching): {q, titles, at}.
FOUND = "pley.tmdb"
FOUND_KEEP = 300
FOUND_SHOWN = 8
# Seconds they may take: the plugin's Python stays busy until they come,
# and a call meanwhile starts a Python of its own (slow on a small box).
# Slower, they are left to On TMDB, which opens them on Discover.
FOUND_WAIT = 3
ICONS = {
    "trending": "DefaultMusicTop100.png",
    "movies": "DefaultMovies.png",
    "shows": "DefaultTVShows.png",
    "upcoming": "DefaultYear.png",
    "anime": "DefaultGenre.png",
}
# Kodi writes a sorted list's Label2 from its sort's mask: a title's state
# rides in its production code, which nothing else shows.
LABEL = "%L"
LABEL2 = "%P"
# Requests' filters, as their listing's parameters.
FILTERS = ("view", "kind", "user_id", "q")
# What only managers do: refused, it needs a sign-in with `approvals`.
DECIDING = ("approve", "retry", "decline", "decline_reason", "handled", "delete")
# Refusals the plugin says itself (plugin.run): the server or the service
# is away, or the sign-in ended.
AWAY = ("unreachable", "service_off", "signed_out")
# Routes that run as actions (plugin.ACTIONS).
ACTIONS = ("title", "act", "request_filter", "more_seasons")


class Asking:
    """Plugin's routes for Discover and requests."""

    # The summary this call read (requests_summary), and why it couldn't.
    summary_read = False
    summary_error = None
    _summary = None

    # ------------------------------------------------------------ helpers

    def requests_summary(self, fresh: bool = False, ask: bool = True):
        """The summary (requests.summary): as the service last read it, or
        as the server says it now (`fresh`, or when the service has none and
        `ask`). {"enabled": false} where requests are off; None when it
        can't be read (the refusal in summary_error)."""
        if self.summary_read and not fresh:
            return self._summary
        found = None
        if not fresh:
            try:
                found = json.loads(kodi.get_property(requests.SUMMARY) or "null")
            except ValueError:
                found = None
        if not isinstance(found, dict) and (fresh or ask):
            try:
                found = requests.summary(self.client()) or {"enabled": False}
            except api.ApiError as e:
                kodi.log("requests' summary: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
                self.summary_error = e
            else:
                kodi.set_property(requests.SUMMARY, json.dumps(found))
        self._summary = found if isinstance(found, dict) else None
        self.summary_read = self._summary is not None or fresh or ask
        return self._summary

    def picture(self, url, width: int):
        return listing.image(self.proxy_base, url, width)

    def art(self, poster, backdrop) -> dict:
        return {k: v for k, v in (("poster", poster), ("thumb", poster), ("fanart", backdrop)) if v}

    def entry(self, label: str, params: dict, icon: str, folder: bool = True, plot: str = "", label2: str = "", top: bool = False, last: bool = False) -> None:
        """One of the listings' own entries (a list, a filter, what is
        wrong), its words on the right and atop its plot."""
        li = xbmcgui.ListItem(label, label2, offscreen=True)
        li.setArt({"icon": icon, "thumb": icon})
        tag = li.getVideoInfoTag()
        if plot:
            tag.setPlot(plot)
        if label2:
            tag.setProductionCode(label2)
        if top or last:
            li.setProperty("SpecialSort", "top" if top else "bottom")
        xbmcplugin.addDirectoryItem(self.handle, self.url(**params), li, isFolder=folder)

    def sorts(self, years: bool = True) -> None:
        """The server's order first, then by name (or year)."""
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED, LABEL, LABEL2)
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_LABEL, LABEL, LABEL2)
        if years:
            xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_VIDEO_YEAR)

    def opening(self, kind: str, item_id) -> str:
        """The builtin that opens a library's title: a movie plays, a show
        lists its seasons."""
        if kind == "movie":
            return "PlayMedia(%s)" % self.url(route="play", id=item_id)
        return "Container.Update(%s)" % self.url(route="item", id=item_id)

    def changed(self) -> None:
        """What was just done shows: the listing again (pley's window, when
        it was done from there), and global search's TMDB titles asked for
        again."""
        kodi.clear_property(FOUND)
        kodi.refresh_listing()

    def cant(self, error: api.ApiError) -> None:
        """A listing that can't be shown: what is wrong in its place, and
        Try again where it may help."""
        if error.code in AWAY:
            raise error
        T = kodi.text
        title, body, retry = requests.problem(error, T, requests.manages(self.requests_summary()), now=time.time())
        if error.code == "insufficient_scope":
            self.entry(title, {"route": "sign_in"}, "DefaultUser.png", folder=False, plot=body, label2=T("sign_in_again"))
        else:
            self.entry(title, {"route": "retry"}, "DefaultIconInfo.png", folder=False, plot=body)
        if retry:
            self.entry(T("try_again"), {"route": "retry"}, "DefaultAddonsUpdates.png", folder=False)
        self.end()

    def refused(self, error: api.ApiError, kind=None, scope: str = "requests") -> None:
        """An action the server refused: said in a notice, or for a sign-in
        that can't, the offer to sign in again."""
        if error.code in AWAY:
            raise error
        T = kodi.text
        s = self.requests_summary()
        found = requests.problem(error, T, requests.manages(s), s, kind, time.time(), scope)
        if error.code == "insufficient_scope":
            if xbmcgui.Dialog().yesno(found[0], found[1], nolabel=T("cancel"), yeslabel=T("sign_in_again")):
                self.route_sign_in()
            return
        kodi.notify(requests.problem_text(found), error=error.status != 503)

    def requests_entry(self, s, last: bool = False) -> None:
        """Your requests (Requests, for managers), with its number."""
        self.entry(requests.entry_label(s, kodi.text), {"route": "requests"}, "DefaultInProgressShows.png", last=last)

    def asking_entries(self, s) -> None:
        """The root's entries: Discover, and the requests."""
        if requests.discovers(s):
            self.entry(kodi.text("discover"), {"route": "discover"}, "DefaultFavourites.png", plot=kodi.text("search_tmdb"))
        if requests.lists(s):
            self.requests_entry(s)

    def notify_service(self, forget: bool = False) -> None:
        """The service reads the summary again (news.py). Until it has, the
        listing shown again reads the one kept: with none, On TMDB and
        Request more seasons would go. `forget`: that one is another
        sign-in's or profile's, and goes now."""
        if forget:
            kodi.clear_property(requests.SUMMARY)
        xbmc.executebuiltin("NotifyAll(%s,%s)" % (kodi.ADDON_ID, requests.ASKING))

    # ------------------------------------------------------------ Discover

    def route_discover(self) -> None:
        """Discover: TMDB's search and lists, then the requests."""
        T = kodi.text
        s = self.requests_summary(fresh=True)
        if s is None:
            return self.cant(self.summary_error)
        if s.get("enabled") and s.get("sign_in_again") == "requests":
            # Signed in before the add-on could ask: that alone shows.
            self.entry(T("sign_in_to_ask"), {"route": "sign_in"}, "DefaultUser.png", folder=False, plot=T("sign_in_to_ask_body"), label2=T("sign_in_again"))
            return self.end()
        if not requests.discovers(s) and not requests.lists(s):
            # Turned off since its entry showed.
            if s.get("enabled"):
                return self.cant(api.ApiError(403, "no_access", ""))
            return self.cant(api.ApiError(503, "unavailable", "", {"setup": False}))
        if requests.may_ask(s):
            credit = T("tmdb_credit")
            self.entry(T("search"), {"route": "discover_search"}, "DefaultAddonsSearch.png", plot=T("search_tmdb"))
            for key, name in requests.LISTS:
                self.entry(T(name), {"route": "discover_list", "list": key}, ICONS[key], plot=credit)
        else:
            # Its own requests only.
            self.entry(T("no_access"), {"route": "retry"}, "DefaultIconInfo.png", folder=False, plot=T("no_access_body"))
        if requests.lists(s):
            self.requests_entry(s)
        self.end()

    def route_discover_list(self) -> None:
        key = self.params.get("list") or "trending"
        page = int(self.params.get("page") or 1)
        client = self.client()
        try:
            found, last, total, mine = self.titles(lambda at: client.get("/api/discover/%s" % key, page=at), page)
        except api.ApiError as e:
            return self.cant(e)
        self.list_titles(found, mine)
        if last < total:
            self.folder(kodi.text("next_page"), {"route": "discover_list", "list": key, "page": last + 1}, last=True)
        self.end()

    def route_discover_search(self) -> None:
        query = self.params.get("q")
        if not query:
            query = (xbmcgui.Dialog().input(kodi.text("search_tmdb")) or "").strip()
            self.end(False)
            if query:
                # The results' own address, as library search's.
                xbmc.executebuiltin("Container.Update(%s)" % self.url(route="discover_search", q=query))
            return
        page = int(self.params.get("page") or 1)
        client = self.client()
        try:
            found, last, total, mine = self.titles(lambda at: client.get("/api/discover/search", q=query, page=at), page)
        except api.ApiError as e:
            return self.cant(e)
        if not found and page == 1:
            # OK searches again.
            self.entry(kodi.text("nothing_on_tmdb") % query, {"route": "discover_search"}, "DefaultIconInfo.png", plot=kodi.text("search_tmdb"))
        self.list_titles(found, mine)
        if last < total:
            self.folder(kodi.text("next_page"), {"route": "discover_search", "q": query, "page": last + 1}, last=True)
        self.end()

    def titles(self, load, page: int) -> tuple:
        """TMDB titles from `page` on (load(page) → DiscoverPage) until a
        screenful, each kind and id once: (titles, the last page read, the
        pages there are, the requests they may point to, by id). Pages may
        come short, even empty, while more remain."""
        answer, mine = api.together(lambda: load(page), self.active_requests)
        found, seen = [], set()
        last = page
        while True:
            for title in answer.get("results") or []:
                key = (title.get("kind"), title.get("tmdb_id")) if isinstance(title, dict) else None
                if key is not None and key not in seen:
                    seen.add(key)
                    found.append(title)
            total = answer.get("total_pages") or 0
            if len(found) >= SCREENFUL or last >= total or last - page + 1 >= PAGES:
                return found, last, total, mine
            last += 1
            answer = load(last)

    def active_requests(self) -> dict:
        """The requests on their way that titles may point to (request_id:
        this account's and those it follows, everyone's for managers), by
        id; none when they can't be read (the titles show their badges)."""
        try:
            found = self.client().get("/api/requests", status=",".join(requests.ACTIVE), limit=200)
        except api.ApiError as e:
            kodi.log("requests for Discover: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
            return {}
        return {r["id"]: r for r in found.get("requests") or [] if isinstance(r, dict) and r.get("id")}

    def list_titles(self, found: list, mine: dict) -> None:
        entries = [self.title_entry(title, mine.get(title.get("request_id"))) for title in found]
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        xbmcplugin.setContent(self.handle, "tvshows" if found and all(t.get("kind") == "show" for t in found) else "movies")
        self.sorts()

    def title_entry(self, title: dict, request=None) -> tuple:
        """A TMDB title as a list entry: its badge (or its request's stage)
        on the right and atop its plot; OK does its main action, or opens a
        library's title; the rest is on its menu."""
        T = kodi.text
        s = self.requests_summary()
        kind = "show" if title.get("kind") == "show" else "movie"
        tmdb = title.get("tmdb_id")
        if request is not None:
            line = requests.colored(*requests.stage(request, T, requests.manages(s), time.time()))
        else:
            line = requests.colored(*requests.badge(title, T))
        li = xbmcgui.ListItem(title.get("title") or "", line, offscreen=True)
        li.setArt(self.art(self.picture(title.get("poster_url"), POSTER), self.picture(title.get("backdrop_url"), BACKDROP)))
        tag = li.getVideoInfoTag()
        tag.setMediaType("tvshow" if kind == "show" else "movie")
        tag.setTitle(title.get("title") or "")
        if title.get("year"):
            tag.setYear(title["year"])
        if title.get("rating"):
            tag.setRating(float(title["rating"]))
        if tmdb:
            tag.setUniqueIDs({"tmdb": str(tmdb)}, "tmdb")
        tag.setPlot(plot(line, title.get("overview"), T("tmdb_credit")))
        tag.setProductionCode(line)
        item_id = title.get("item_id")
        opens = bool(item_id) and title.get("status") in ("library", "partial")
        menu = []
        if kind == "show":
            menu.append((T("seasons"), "Container.Update(%s)" % self.url(route="seasons", tmdb=tmdb)))
        if item_id and not opens:
            menu.append((T("open"), self.opening(kind, item_id)))
        if opens and title.get("status") == "partial" and requests.may_ask(s):
            menu.append((T("request_more_seasons"), "RunPlugin(%s)" % self.url(route="title", kind=kind, tmdb=tmdb, more=1)))
        if request is not None:
            menu += self.request_menu(request)
        li.addContextMenuItems(menu)
        if not opens:
            return self.url(route="title", kind=kind, tmdb=tmdb), li, False
        if kind == "movie":
            li.setProperty("IsPlayable", "true")
            return self.url(route="play", id=item_id), li, False
        return self.url(route="item", id=item_id), li, True

    def request_menu(self, request: dict) -> list:
        return [
            (label, "RunPlugin(%s)" % self.url(route="act", id=request["id"], do=action))
            for action, label in requests.actions(request, kodi.text)
        ]

    # --------------------------------------------------- a title, asked for

    def detail(self, kind: str, tmdb: int) -> tuple:
        """A TMDB title's details and the summary as it is now, asked at
        once. Raises api.ApiError, the summary's too."""
        client = self.client()
        found, s = api.together(lambda: client.get("/api/discover/%s/%d" % (kind, tmdb)), lambda: self.requests_summary(fresh=True))
        if s is None:
            raise self.summary_error
        found.setdefault("kind", kind)
        found.setdefault("tmdb_id", tmdb)
        return found, s

    def route_title(self) -> None:
        """A TMDB title's main action, on OK; `more`:
        Request more seasons, from its menu; `season`: one of its seasons,
        from its seasons' list."""
        kind = "show" if self.params.get("kind") == "show" else "movie"
        try:
            detail, s = self.detail(kind, int(self.params["tmdb"]))
        except api.ApiError as e:
            return self.refused(e, kind)
        if self.params.get("season") is not None:
            return self.season_action(detail, s, int(self.params["season"]))
        main = requests.main_action(detail, s)
        if self.params.get("more"):
            main = more_action(main, detail, s)
        self.main_action(detail, s, main)

    def main_action(self, detail: dict, s, main: str) -> None:
        T = kodi.text
        dialog = xbmcgui.Dialog()
        now = time.time()
        heading = heading_of(detail)
        kind = detail["kind"]
        if main == "sign_in":
            if dialog.yesno(T("sign_in_to_ask"), T("sign_in_to_ask_body"), nolabel=T("cancel"), yeslabel=T("sign_in_again")):
                self.route_sign_in()
        elif main == "restricted":
            dialog.ok(heading, detail.get("restriction") or "")
        elif main == "open":
            xbmc.executebuiltin(self.opening(kind, detail["item_id"]))
        elif main == "in_library":
            dialog.ok(heading, sentence(T("in_library")))
        elif main == "declined":
            dialog.ok(heading, requests.declined_line(detail, T, now))
        elif main == "requested":
            dialog.ok(heading, request_lines(requests.own(detail), requests.manages(s), T("requested")))
        elif main == "following":
            request = requests.followed(detail)
            lines = request_lines(request, requests.manages(s), T("following"))
            if request is None or "unwatch" not in (request.get("actions") or []):
                dialog.ok(heading, lines)
            elif dialog.yesno(heading, lines, nolabel=T("cancel"), yeslabel=T("stop_following")):
                self.carry_out(request, "unwatch")
        elif main == "tell_me_too":
            if dialog.yesno(heading, T("someone_asked"), nolabel=T("cancel"), yeslabel=T("tell_me_too")):
                # Following asks for what is asked for already.
                seasons = [n["number"] for n in detail.get("seasons") or [] if n.get("state") == "requested"]
                self.ask_for(detail, seasons if kind == "show" and seasons else None)
        elif main == "no_access":
            dialog.ok(heading, sentence(T("no_access")) + " " + T("no_access_body"))
        elif main == "used_up":
            dialog.ok(heading, "[CR]".join(requests.allowance(detail, s, T, now)))
        elif kind == "show":
            self.pick(detail, s)
        else:
            lines = [heading] + requests.allowance(detail, s, T, now)
            if dialog.yesno(T("request"), "[CR]".join(lines), nolabel=T("cancel"), yeslabel=T("request")):
                self.ask_for(detail, None)

    def season_action(self, detail: dict, s, number: int) -> None:
        """OK on a season of a TMDB show: the picker with it ticked when it
        may be asked for; its request when it has one; else what the show
        offers."""
        T = kodi.text
        season = next((n for n in detail.get("seasons") or [] if n.get("number") == number), None)
        if season is None:
            return self.changed()
        main = requests.main_action(detail, s)
        if requests.askable(season, detail) and main not in ("sign_in", "no_access", "used_up"):
            return self.pick(detail, s, only=number)
        if season.get("state") == "library" and detail.get("item_id"):
            return xbmc.executebuiltin(self.opening("show", detail["item_id"]))
        request = next((r for r in detail.get("requests") or [] if r.get("id") == season.get("request_id")), None)
        if season.get("state") == "requested":
            heading = "%s · %s" % (detail.get("title") or "", requests.season_name(season))
            return xbmcgui.Dialog().ok(heading, request_lines(request, requests.manages(s), T("requested")))
        if season.get("declined"):
            return xbmcgui.Dialog().ok(heading_of(detail), requests.declined_line(detail, T, time.time()))
        self.main_action(detail, s, main)

    def pick(self, detail: dict, s, only=None) -> None:
        """Which seasons? (Kodi's multiselect), then asked for: none picked
        says so, more than may be asked for says the allowance, and either
        offers the list again."""
        T = kodi.text
        now = time.time()
        listed, ticked = requests.picker(detail, only)
        dialog = xbmcgui.Dialog()
        if not listed:
            return dialog.ok(heading_of(detail), T("nothing_more") % (detail.get("title") or ""))
        manage = requests.manages(s)
        rows = []
        for season in listed:
            state = requests.colored(*requests.season_state(season, detail, T, manage, now))
            li = xbmcgui.ListItem(requests.season_name(season), state, offscreen=True)
            poster = self.picture(season.get("poster_url") or detail.get("poster_url"), SEASON_POSTER)
            if poster:
                li.setArt({"thumb": poster, "poster": poster})
            rows.append(li)
        left = detail.get("seasons_left")
        while True:
            chosen = dialog.multiselect(T("which_seasons"), rows, preselect=ticked, useDetails=True)
            if chosen is None:
                return
            if not chosen:
                dialog.ok(T("which_seasons"), T("pick_a_season"))
                ticked = []
            elif left is not None and len(chosen) > left:
                dialog.ok(T("which_seasons"), "[CR]".join(requests.allowance(detail, s, T, now)))
                ticked = chosen
            else:
                return self.ask_for(detail, [listed[i]["number"] for i in chosen])

    def ask_for(self, detail: dict, seasons) -> None:
        """POST /api/requests, and what it did, said."""
        kind = detail["kind"]
        try:
            found, following = requests.ask(self.client(), kind, detail["tmdb_id"], seasons)
        except api.ApiError as e:
            return self.refused(e, kind)
        kodi.notify(requests.asked(found, following, kodi.text, detail.get("title") or ""))
        self.notify_service()
        self.changed()

    def route_seasons(self) -> None:
        """A TMDB show's seasons, each with its state, under the show's main
        action: its page, in Kodi's lists."""
        T = kodi.text
        try:
            detail, s = self.detail("show", int(self.params["tmdb"]))
        except api.ApiError as e:
            return self.cant(e)
        now = time.time()
        manage = requests.manages(s)
        main = requests.main_action(detail, s)
        poster = self.picture(detail.get("poster_url"), POSTER)
        backdrop = self.picture(detail.get("backdrop_url"), BACKDROP)
        self.main_entry(detail, s, main, poster, backdrop)
        if main == "open" and more_action(main, detail, s) == "more_seasons" and requests.more_seasons(detail):
            self.title_action(T("request_more_seasons"), " ".join(requests.allowance(detail, s, T, now)[:1]), detail, poster, backdrop, more=1)
        entries = []
        for season in requests.ordered(detail.get("seasons")):
            line = requests.colored(*requests.season_state(season, detail, T, manage, now))
            name = requests.season_name(season)
            li = xbmcgui.ListItem(name, line, offscreen=True)
            li.setArt(self.art(self.picture(season.get("poster_url"), POSTER) or poster, backdrop))
            tag = li.getVideoInfoTag()
            tag.setMediaType("season")
            tag.setTitle(name)
            tag.setTvShowTitle(detail.get("title") or "")
            tag.setSeason(season.get("number") or 0)
            if season.get("air_date"):
                tag.setPremiered(season["air_date"])
            count = words.plural(season["episodes"], "episode") if season.get("episodes") else ""
            tag.setPlot(plot(line, "" if count == line else count, T("tmdb_credit")))
            tag.setProductionCode(line)
            url = self.url(route="title", kind="show", tmdb=detail["tmdb_id"], season=season.get("number") or 0)
            entries.append((url, li, False))
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        xbmcplugin.setContent(self.handle, "seasons")
        self.sorts(years=False)
        self.end()

    def main_entry(self, detail: dict, s, main: str, poster, backdrop) -> None:
        """The show's main action, atop its seasons: its words, and on the
        right its state (the stage, the allowance, why it can't)."""
        T = kodi.text
        now = time.time()
        request = requests.own(detail) if main == "requested" else requests.followed(detail) if main == "following" else None
        label2 = ""
        if request is not None:
            label2 = requests.colored(*requests.stage(request, T, requests.manages(s), now))
        elif main == "tell_me_too":
            label2 = T("someone_asked")
        elif main in ("request", "more_seasons", "used_up"):
            label2 = " ".join(requests.allowance(detail, s, T, now)[:1])
        elif main == "declined":
            label2 = requests.declined_line(detail, T, now)
        elif main == "no_access":
            label2 = T("no_access_body")
        labels = {
            "sign_in": T("sign_in_to_ask"),
            "restricted": detail.get("restriction") or "",
            "open": T("open"),
            "in_library": T("in_library"),
            "declined": T("declined"),
            "requested": T("requested"),
            "following": T("following"),
            "tell_me_too": T("tell_me_too"),
            "more_seasons": T("request_more_seasons"),
            "no_access": T("no_access"),
            "used_up": T("request_more_seasons") if detail.get("status") == "partial" else T("request"),
        }
        menu = self.request_menu(request) if request is not None else []
        self.title_action(labels.get(main, T("request")), label2, detail, poster, backdrop, menu=menu)

    def title_action(self, label: str, label2: str, detail: dict, poster, backdrop, menu=(), **params) -> None:
        li = xbmcgui.ListItem(label, label2, offscreen=True)
        li.setArt(self.art(poster, backdrop))
        li.setProperty("SpecialSort", "top")
        tag = li.getVideoInfoTag()
        tag.setMediaType("tvshow")
        tag.setTitle(detail.get("title") or "")
        if detail.get("year"):
            tag.setYear(detail["year"])
        tag.setPlot(plot(label2, detail.get("overview"), kodi.text("tmdb_credit")))
        tag.setProductionCode(label2)
        li.addContextMenuItems(list(menu))
        url = self.url(route="title", kind="show", tmdb=detail["tmdb_id"], **params)
        xbmcplugin.addDirectoryItem(self.handle, url, li, isFolder=False)

    # ------------------------------------------------------------ requests

    def route_requests(self) -> None:
        """Your requests, newest first; for managers Requests, opening on
        Needs attention (oldest first), with filters atop. Once the first
        page of the account's own shows (a manager's Yours), their news is
        seen."""
        T = kodi.text
        client = self.client()
        s = self.requests_summary()
        manage = requests.manages(s)
        offset = int(self.params.get("offset") or 0)
        chosen = {k: self.params[k] for k in FILTERS if self.params.get(k)}
        view = chosen.get("view") or ("attention" if manage else "all")
        try:
            found = client.get("/api/requests", offset=offset, limit=REQUESTS_PAGE, **query_of(chosen, view))
        except api.ApiError as e:
            return self.cant(e)
        listed = [r for r in found.get("requests") or [] if isinstance(r, dict) and r.get("id")]
        if offset == 0:
            if (s or {}).get("sign_in_again") == "approvals":
                self.entry(T("sign_in_to_approve"), {"route": "sign_in"}, "DefaultUser.png", folder=False, label2=T("sign_in_again"), top=True)
            if manage:
                self.filters(chosen, view, found.get("people") or [])
            if not listed:
                self.nothing(chosen, view, manage, s)
        entries = [self.request_entry(r, manage) for r in listed]
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        if offset + len(listed) < (found.get("total") or 0):
            self.folder(T("next_page"), dict(chosen, route="requests", offset=offset + REQUESTS_PAGE), last=True)
        xbmcplugin.setContent(self.handle, "movies")
        self.sorts(years=False)
        self.end()
        # Downloads move with no event: the service asks for this list
        # again every 10 seconds while it is on screen (news.py).
        kodi.set_property(requests.LIVE, self.path if requests.moving(listed) else "")
        own = not manage or view == "mine"
        if own and offset == 0 and ((s or {}).get("unseen") or any(r.get("unseen") for r in listed)):
            try:
                client.post("/api/requests/seen")
            except api.ApiError as e:
                kodi.log("requests seen: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
            else:
                self.notify_service()

    def nothing(self, chosen: dict, view: str, manage: bool, s) -> None:
        """What an empty list of requests says."""
        T = kodi.text
        if set(chosen) - {"view"} or view not in ("attention", "all", "mine"):
            return self.entry(T("no_matches"), {"route": "retry"}, "DefaultIconInfo.png", folder=False)
        if view == "attention":
            return self.entry(T("nothing_needs_attention"), {"route": "retry"}, "DefaultIconInfo.png", folder=False)
        if requests.may_ask(s):
            # OK goes to Discover.
            return self.entry(T("nothing_asked"), {"route": "discover"}, "DefaultIconInfo.png", plot=T("search_tmdb"))
        self.entry(T("nothing_asked"), {"route": "retry"}, "DefaultIconInfo.png", folder=False)

    def request_entry(self, request: dict, manage: bool) -> tuple:
        """A request: its stage on the right and atop its plot, then its
        seasons, what it says and who asked when; OK opens its title once a
        library has it, else offers what may be done to it."""
        T = kodi.text
        now = time.time()
        line = requests.colored(*requests.stage(request, T, manage, now))
        li = xbmcgui.ListItem(request.get("title") or "", line, offscreen=True)
        li.setArt(self.art(self.picture(request.get("poster_url"), POSTER), self.picture(request.get("backdrop_url"), BACKDROP)))
        kind = "show" if request.get("kind") == "show" else "movie"
        tag = li.getVideoInfoTag()
        tag.setMediaType("tvshow" if kind == "show" else "movie")
        tag.setTitle(request.get("title") or "")
        if request.get("year"):
            tag.setYear(request["year"])
        if request.get("tmdb_id"):
            tag.setUniqueIDs({"tmdb": str(request["tmdb_id"])}, "tmdb")
        tag.setPlot(request_lines(request, manage))
        tag.setProductionCode(line)
        menu = self.request_menu(request)
        if kind == "show" and request.get("tmdb_id"):
            menu.append((T("seasons"), "Container.Update(%s)" % self.url(route="seasons", tmdb=request["tmdb_id"])))
        li.addContextMenuItems(menu)
        item_id = request.get("item_id")
        if not item_id:
            return self.url(route="act", id=request["id"], do="choose"), li, False
        if kind == "movie":
            li.setProperty("IsPlayable", "true")
            return self.url(route="play", id=item_id), li, False
        return self.url(route="item", id=item_id), li, True

    def filters(self, chosen: dict, view: str, people: list) -> None:
        """Requests' filters, atop its list: which requests, movies or
        shows, whose, and a title's words."""
        T = kodi.text
        here = dict(chosen, route="request_filter")
        heading = T("filter")
        views = dict(views_of(T))
        self.entry(views.get(view, views["attention"]), dict(here, pick="view"), "DefaultInProgressShows.png", folder=False, label2=heading, top=True)
        kind = chosen.get("kind")
        kinds = {"movie": (T("movies"), "DefaultMovies.png"), "show": (T("shows"), "DefaultTVShows.png")}
        label, icon = kinds.get(kind, (T("movies_and_shows"), "DefaultVideo.png"))
        self.entry(label, dict(here, pick="kind"), icon, folder=False, label2=heading, top=True)
        if people or chosen.get("user_id"):
            names = {str(p.get("id")): p.get("name") or "" for p in people}
            who = names.get(chosen.get("user_id") or "", T("everyone"))
            self.entry(who, dict(here, pick="user_id"), "DefaultActor.png", folder=False, label2=heading, top=True)
        typed = chosen.get("q")
        self.entry(T("search"), dict(here, pick="q"), "DefaultAddonsSearch.png", folder=False, label2="“%s”" % typed if typed else heading, top=True)

    def route_request_filter(self) -> None:
        """One of Requests' filters changed (its dialog), and the list shown
        so, in place of the one before."""
        T = kodi.text
        chosen = {k: self.params[k] for k in FILTERS if self.params.get(k)}
        pick = self.params.get("pick")
        dialog = xbmcgui.Dialog()
        if pick == "q":
            # Kodi's keyboard, which tells cancelling from an empty entry
            # (that clears the filter); Dialog.input says "" for both.
            keyboard = xbmc.Keyboard(chosen.get("q", ""), T("search"))
            keyboard.doModal()
            if not keyboard.isConfirmed():
                return
            chosen["q"] = keyboard.getText().strip()
        elif pick in ("view", "kind", "user_id"):
            if pick == "view":
                options = views_of(T)
            elif pick == "kind":
                options = [("", T("movies_and_shows")), ("movie", T("movies")), ("show", T("shows"))]
            else:
                # Those with requests the other filters show.
                others = {k: v for k, v in chosen.items() if k != "user_id"}
                found = self.client().get("/api/requests", limit=1, **query_of(others, others.get("view") or "attention"))
                options = [("", T("everyone"))] + [(str(p.get("id")), p.get("name") or "") for p in found.get("people") or []]
            keys = [key for key, _ in options]
            current = chosen.get(pick, "attention" if pick == "view" else "")
            picked = dialog.select(T("filter"), [label for _, label in options], preselect=keys.index(current) if current in keys else 0)
            if picked < 0:
                return
            chosen[pick] = keys[picked]
        params = {k: v for k, v in chosen.items() if v}
        xbmc.executebuiltin("Container.Update(%s,replace)" % self.url(route="requests", **params))

    def route_act(self) -> None:
        """Something done to a request: from its menu (`do`), or OK on it
        ("choose": what may be done, in Kodi's list; what it is, when
        nothing may)."""
        T = kodi.text
        do = self.params.get("do") or "choose"
        try:
            request = self.client().get("/api/requests/%d" % int(self.params["id"]))
        except api.ApiError as e:
            if e.status == 404:
                # Deleted meanwhile.
                return self.changed()
            return self.refused(e)
        manage = requests.manages(self.requests_summary())
        offered = requests.actions(request, T)
        if do == "choose":
            title = request.get("title") or ""
            if not offered:
                return xbmcgui.Dialog().ok(title, request_lines(request, manage))
            pill = requests.stage(request, T, manage, time.time())[0]
            picked = xbmcgui.Dialog().select("%s · %s" % (title, pill) if pill else title, [label for _, label in offered])
            if picked < 0:
                return
            do = offered[picked][0]
        if do not in [action for action, _ in offered]:
            # It changed since its list showed: as it is now.
            return self.changed()
        self.carry_out(request, do)

    def carry_out(self, request: dict, action: str) -> None:
        """One of a request's actions, asked first where it can't be undone."""
        T = kodi.text
        client = self.client()
        dialog = xbmcgui.Dialog()
        title = request.get("title") or ""
        path = "/api/requests/%d" % request["id"]
        try:
            if action == "withdraw":
                name = "remove" if request.get("stage") == "declined" else "withdraw"
                if not dialog.yesno(T(name), T(name + "_ask") % title, nolabel=T("cancel"), yeslabel=T(name)):
                    return
                client.delete(path)
            elif action == "unwatch":
                client.delete(path + "/watch")
            elif action in ("approve", "retry"):
                client.post(path + "/approve")
            elif action == "decline":
                client.post(path + "/decline")
            elif action == "decline_reason":
                reason = (dialog.input(T("decline_reason").rstrip("…")) or "").strip()
                if not reason:
                    return
                client.post(path + "/decline", {"reason": reason})
            elif action == "handled":
                client.post(path + "/handled")
            elif action == "delete":
                if not dialog.yesno(T("delete"), T("delete_request_ask"), nolabel=T("cancel"), yeslabel=T("delete")):
                    return
                client.delete(path)
            elif action == "ask_again":
                kind = "show" if request.get("kind") == "show" else "movie"
                found, following = requests.ask(client, kind, request["tmdb_id"], request.get("seasons"))
                kodi.notify(requests.asked(found, following, T, title))
        except api.ApiError as e:
            # A race answers 409 with the request as it is now: shown so.
            self.refused(e, request.get("kind"), "approvals" if action in DECIDING else "requests")
        self.notify_service()
        self.changed()

    # ------------------------------------------------- from the libraries

    def route_more_seasons(self) -> None:
        """Request more seasons, from a library show's menu: its TMDB
        show's picker."""
        T = kodi.text
        client = self.client()
        item_id = int(self.params["id"])
        try:
            found = client.get("/api/items/%d/request" % item_id)
            if found.get("askable") == []:
                title = client.get("/api/items/%d" % item_id).get("title") or ""
                return xbmcgui.Dialog().ok(T("request_more_seasons"), T("nothing_more") % title)
            detail, s = self.detail("show", int(found["tmdb_id"]))
        except api.ApiError as e:
            if e.status == 404:
                # Not matched on TMDB.
                title = self.params.get("title") or ""
                return kodi.notify(T("nothing_on_tmdb") % title if title else T("nothing_found"))
            return self.refused(e, "show")
        main = more_action(requests.main_action(detail, s), detail, s)
        if main == "more_seasons":
            return self.pick(detail, s)
        self.main_action(detail, s, main)

    def more_seasons_menu(self, item: dict) -> list:
        """Request more seasons, on a library show's menu while this sign-in
        may ask (the summary the service keeps: listings ask nothing more)."""
        if item.get("kind") != "show" or not requests.may_ask(self.requests_summary(ask=False)):
            return []
        url = self.url(route="more_seasons", id=item["id"], title=item.get("title") or "")
        return [(kodi.text("request_more_seasons"), "RunPlugin(%s)" % url)]

    def on_tmdb(self, query: str, found) -> None:
        """Global search's titles from TMDB that the libraries don't have,
        after the libraries' own: its first ones, then See all on Discover.
        `found`: those titles, None while they're on their way (Searching
        shows the listing again once they come)."""
        T = kodi.text
        url = {"route": "discover_search", "q": query}
        if found:
            self.entry(T("on_tmdb"), url, "DefaultAddonsSearch.png", plot=T("tmdb_credit"), last=True)
            entries = []
            for title in found[:FOUND_SHOWN]:
                address, li, folder = self.title_entry(title)
                li.setProperty("SpecialSort", "bottom")
                entries.append((address, li, folder))
            xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        elif found is None:
            self.entry(T("on_tmdb"), url, "DefaultAddonsSearch.png", plot=T("tmdb_credit"), label2=T("searching") + "…", last=True)
        self.entry(T("see_all_on_discover"), url, "DefaultAddonsSearch.png", plot=T("search_tmdb"), last=True)


def more_action(main: str, detail: dict, s) -> str:
    """Request more seasons in place of a show's main action, where it may
    be asked for: else why not (no access, nothing left), or what the
    title is (requested, declined…)."""
    if main not in ("open", "more_seasons", "request"):
        return main
    if not requests.discovers(s):
        return "no_access"
    if requests.used_up(detail):
        return "used_up"
    return "more_seasons"


def query_of(chosen: dict, view: str) -> dict:
    """GET /api/requests' parameters for Requests' filters."""
    query = {k: chosen[k] for k in ("kind", "user_id", "q") if chosen.get(k)}
    if view == "attention":
        query.update(attention=1, order="oldest")
    elif view == "mine":
        query["mine"] = 1
    elif view != "all":
        query["status"] = view
    return query


def views_of(T) -> list:
    """Requests' views for managers: Needs attention, by status, all, and
    Yours (those the account made or follows)."""
    return [
        ("attention", T("needs_attention")),
        ("pending", T("pending")),
        ("approved", T("stage_approved")),
        ("available", T("available")),
        ("declined", T("declined")),
        ("failed", T("failed")),
        ("all", T("all")),
        ("mine", T("yours")),
    ]


def heading_of(detail: dict) -> str:
    """"Heat (1995)"."""
    title = detail.get("title") or ""
    return "%s (%s)" % (title, detail["year"]) if detail.get("year") else title


def request_lines(request, manage: bool, first: str = "") -> str:
    """What a request says, for Kodi's dialog and a plot: its stage, its
    seasons, what it says and who asked when."""
    T = kodi.text
    now = time.time()
    lines = [first] if first else []
    if request is not None:
        lines.append(requests.colored(*requests.stage(request, T, manage, now)))
        if request.get("seasons"):
            lines.append(words.seasons_label(request["seasons"]))
        lines += requests.details(request, T, manage, now)
        lines.append(requests.asked_line(request, T, manage, now))
    return "[CR]".join(lines)


def plot(line: str, body, credit: str) -> str:
    """A plot: the state's line atop, then the body, then the credit."""
    return "[CR][CR]".join(p for p in (line, (body or "").strip(), credit) if p)


class Searching:
    """Global search's look at TMDB (GET /api/discover/search with
    missing=1), asked beside the libraries' search and never waited for:
    what came by the time those answered shows with them; else, once it
    comes, the listing is shown again with it (FOUND). `client` waits
    FOUND_WAIT at most."""

    def __init__(self, client, query: str, path: str):
        self.client = client
        self.query = query
        self.path = path
        self.found = kept_titles(query)
        # Whether the server answered (a refusal or a timeout keeps nothing).
        self.answered = False
        self.done = threading.Event()
        if self.found is not None:
            self.done.set()
        else:
            threading.Thread(target=self.ask, daemon=True).start()

    def ask(self) -> None:
        found = []
        try:
            answer = self.client.get("/api/discover/search", q=self.query, missing=1)
            found = [t for t in answer.get("results") or [] if isinstance(t, dict)]
            self.answered = True
        except api.ApiError as e:
            kodi.log("TMDB in global search: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
        self.found = found
        self.done.set()

    def ready(self):
        """The titles, once they came; None while on their way."""
        return self.found if self.done.is_set() else None

    def later(self) -> None:
        """Once the titles came (after the listing showed without them):
        kept for the listing's next showing, which is asked for while it is
        still on screen. FOUND_WAIT, the request's timeout, bounds the wait."""
        if not self.done.wait(FOUND_WAIT + api.CONNECT_TIMEOUT) or not self.answered:
            return
        kodi.set_property(FOUND, json.dumps({"q": self.query, "titles": self.found, "at": time.time()}))
        if self.found and xbmc.getInfoLabel("Container.FolderPath") == self.path:
            xbmc.executebuiltin("Container.Refresh")


def kept_titles(query: str):
    """Global search's TMDB titles for `query`, kept a while (FOUND)."""
    try:
        kept = json.loads(kodi.get_property(FOUND) or "null")
    except ValueError:
        return None
    if not isinstance(kept, dict) or kept.get("q") != query or not 0 <= time.time() - (kept.get("at") or 0) <= FOUND_KEEP:
        return None
    titles = kept.get("titles")
    return [t for t in titles if isinstance(t, dict)] if isinstance(titles, list) else None
