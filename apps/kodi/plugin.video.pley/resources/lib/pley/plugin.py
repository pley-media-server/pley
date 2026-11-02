"""The plugin: pley's screens as Kodi listings, and what their items do.

Every call to the server goes through the service's proxy, which holds the
sign-in. Only signing in talks to the server directly, before there are
tokens.
"""

from __future__ import annotations

import os
import re
import sys
import threading
import time
import urllib.parse

import xbmc
import xbmcgui
import xbmcplugin

from . import api, asking, kodi, listing, livetv, playback, requests

PAGE = 300


class Plugin(asking.Asking):
    def __init__(self, argv):
        kodi.begin()
        self.base_url = argv[0]
        self.handle = int(argv[1]) if len(argv) > 1 and argv[1].lstrip("-").isdigit() else -1
        query = argv[2][1:] if len(argv) > 2 and argv[2].startswith("?") else ""
        # This listing's address, as Container.FolderPath says it.
        self.path = self.base_url + ("?" + query if query else "")
        self.params = dict(urllib.parse.parse_qsl(query))
        self.resume = len(argv) > 3 and argv[3] == "resume:true"
        self.proxy_base = None

    # ---------------------------------------------------------------- helpers

    def url(self, **params) -> str:
        return self.base_url + "?" + urllib.parse.urlencode(params)

    def proxy(self) -> str:
        """The proxy's address, waiting a moment for a service that is just
        starting."""
        for _ in range(30):
            base = kodi.get_property(kodi.PROXY)
            if base:
                return base
            xbmc.sleep(100)
        raise api.ApiError(0, "service_off", kodi.text("service_off"))

    def client(self, timeout: float = 60) -> api.Client:
        if not self.proxy_base:
            self.proxy_base = self.proxy()
        return api.Client(self.proxy_base, kodi.user_agent(), timeout=timeout, local=True)

    def status(self) -> dict:
        client = self.client()
        try:
            return client.get("/status")
        except api.ApiError:
            return {"signed_in": False, "server": None}

    def folder(self, title: str, params: dict, icon: str = "DefaultFolder.png", last: bool = False) -> None:
        li = xbmcgui.ListItem(title, offscreen=True)
        li.setArt({"icon": icon, "thumb": icon})
        if last:
            # After the items, however they're sorted ("Next page").
            li.setProperty("SpecialSort", "bottom")
        xbmcplugin.addDirectoryItem(self.handle, self.url(**params), li, isFolder=True)

    def action(self, title: str, params: dict, icon: str = "DefaultAddonService.png") -> None:
        li = xbmcgui.ListItem(title, offscreen=True)
        li.setArt({"icon": icon, "thumb": icon})
        xbmcplugin.addDirectoryItem(self.handle, self.url(**params), li, isFolder=False)

    def items(self, items: list, content: str | None = None, in_season: bool = False, extra=None) -> None:
        """`in_season`: a season's episodes, which need no show in their
        names; elsewhere (Home's rows, search, playlists) they say it.
        `extra(at)`: more of the menu of the item at `at` (a playlist's)."""
        kinds = {item["kind"] for item in items}
        # Among films and shows, episodes show their show's poster, as the
        # others do theirs; on their own, their stills.
        posters = kinds != {"episode"}
        base = self.proxy_base
        entries = []
        for at, item in enumerate(items):
            li = listing.list_item(base, item, with_show=not in_season, posters=posters)
            li.addContextMenuItems(self.menu(item) + (extra(at) if extra else []))
            if item["kind"] in listing.PLAYABLE:
                # The resume point goes along, so playing needs no details first.
                position = (item.get("user_state") or {}).get("position_ms") or 0
                entries.append((self.url(route="play", id=item["id"], pos=position), li, False))
            else:
                entries.append((self.url(route="item", id=item["id"]), li, True))
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        if content is None and len(kinds) == 1:
            content = listing.CONTENT.get(next(iter(kinds)))
        elif content is None and kinds and kinds <= {"movie", "show", "episode", "extra"}:
            # A row of films and episodes: poster views still apply.
            content = "movies"
        if content:
            xbmcplugin.setContent(self.handle, content)

    def menu(self, item: dict) -> list:
        entries = []
        if item["kind"] not in listing.MUSIC:
            watched = (item.get("user_state") or {}).get("watched")
            name, value = ("mark_unwatched", "0") if watched else ("mark_watched", "1")
            entries.append((kodi.text(name), "RunPlugin(%s)" % self.url(route="watched", id=item["id"], value=value)))
        if item["kind"] in ("movie", "show"):
            if self.params.get("key") == "my_list":
                entries.append((kodi.text("remove_from_list"), "RunPlugin(%s)" % self.url(route="my_list", id=item["id"], value="0")))
            else:
                entries.append((kodi.text("add_to_list"), "RunPlugin(%s)" % self.url(route="my_list", id=item["id"], value="1")))
        return entries + self.more_seasons_menu(item)

    def end(self, ok: bool = True, cache: bool = False) -> None:
        xbmcplugin.endOfDirectory(self.handle, succeeded=ok, cacheToDisc=cache)

    # ----------------------------------------------------------------- routes

    def run(self) -> None:
        from .subtitles import ACTIONS as SUBTITLE_ACTIONS

        # Kodi's subtitle dialog says `action`, not `route` (subtitles.py).
        route = self.params.get("route") or ("subtitles" if self.params.get("action") in SUBTITLE_ACTIONS else "root")
        handler = getattr(self, "route_" + route, None)
        if handler is None:
            kodi.log("unknown route " + route, xbmc.LOGWARNING)
            return
        try:
            handler()
        except api.ApiError as e:
            kodi.log("%s: %s %s" % (route, e.code, e.message), xbmc.LOGWARNING)
            listing_route = self.handle >= 0 and route not in ACTIONS
            if e.code == "unreachable" and listing_route:
                # The server is off or out of reach (the internet needn't be
                # there: pley works on the home network alone).
                return self.unreachable()
            if e.code == "unreachable":
                message = kodi.text("cant_reach") % self.server_name()
            elif e.code == "signed_out":
                message = kodi.text("signed_out")
            elif e.code == "service_off":
                message = kodi.text("service_off")
            else:
                message = kodi.text("server_error") % e.message
            kodi.notify(message, error=True)
            if route in RESOLVING:
                xbmcplugin.setResolvedUrl(self.handle, False, xbmcgui.ListItem())
            elif listing_route:
                self.end(False)
        finally:
            if route in ACTIONS and route not in RESOLVING and self.handle >= 0:
                # An action opened as a folder (a skin's widget, JSON-RPC)
                # lists nothing: Kodi keeps its Python between calls and
                # waits for this, where it once waited for the script's end.
                self.end(False)

    def server_name(self) -> str:
        return kodi.setting("server_name") or server_setting()

    def unreachable(self) -> None:
        """What a listing shows when the server doesn't answer."""
        li = xbmcgui.ListItem(kodi.text("cant_reach") % self.server_name(), offscreen=True)
        li.setArt({"icon": "DefaultIconError.png", "thumb": "DefaultIconError.png"})
        li.getVideoInfoTag().setPlot(kodi.text("cant_reach_hint"))
        xbmcplugin.addDirectoryItem(self.handle, self.url(route="retry"), li, isFolder=False)
        self.action(kodi.text("try_again"), {"route": "retry"}, "DefaultAddonsUpdates.png")
        self.action(kodi.text("choose_server"), {"route": "choose_server"}, "DefaultNetwork.png")
        self.end()

    def route_retry(self) -> None:
        xbmc.executebuiltin("Container.Refresh")

    def open_window(self) -> None:
        """pley's window (window.py), once a server is chosen and signed in to."""
        try:
            if not server_setting():
                self.route_choose_server()
            elif not self.signed_in():
                self.route_sign_in()
            if not self.signed_in():
                return
            from . import window

            window.run(self, self.proxy_base or self.proxy())
        except api.ApiError as e:
            # Its list ended already: what went wrong is said, not listed.
            kodi.log("Pley's window: %s %s" % (e.code, e.message), xbmc.LOGWARNING)
            kodi.notify(kodi.text("cant_reach") % self.server_name() if e.code == "unreachable" else kodi.text("server_error") % e.message, error=True)

    def signed_in(self) -> bool:
        """Whether the add-on is signed in on the chosen server."""
        server = server_setting()
        status = self.status() if server else {}
        return bool(status.get("signed_in")) and status.get("server") == server

    def opened(self) -> bool:
        """Whether someone opened the add-on (from Kodi's add-ons, a
        favourite), rather than a skin's widget filling itself on Home, or
        something asking for its folders while pley's window is open."""
        if kodi.get_property(kodi.WINDOW_OPEN):
            return False
        return self.handle < 0 or xbmcgui.getCurrentWindowId() != kodi.HOME

    def route_root(self) -> None:
        if kodi.setting_int("look") == 0 and self.opened():
            # pley's look: its own window, over Kodi's list of add-ons,
            # which stays as it was.
            if self.handle >= 0:
                self.end(False)
            return self.open_window()
        server = server_setting()
        name = kodi.setting("server_name") or server
        if not server:
            self.action(kodi.text("choose_server"), {"route": "choose_server"}, "DefaultNetwork.png")
            return self.end()
        status = self.status()
        if not status.get("signed_in") or status.get("server") != server:
            self.action(kodi.text("sign_in_to") % name, {"route": "sign_in"}, "DefaultUser.png")
            self.action(kodi.text("choose_server"), {"route": "choose_server"}, "DefaultNetwork.png")
            return self.end()
        client = self.client()
        # The summary as the service keeps it (read again on its events):
        # asked here only when it has none yet.
        home, libraries, live, summary = api.together(
            lambda: client.get("/api/hubs/home"),
            lambda: client.get("/api/libraries"),
            lambda: livetv.summary(client),
            self.requests_summary,
        )
        for hub in home.get("hubs", []):
            self.folder(hub["title"], {"route": "hub", "key": hub["key"]}, "DefaultRecentlyAddedMovies.png")
        if livetv.available(live):
            # Where it has channels, after Home's rows as in the web's sidebar.
            self.folder(kodi.text("live_tv"), {"route": "live"}, "DefaultPVRChannels.png")
        icons = {"movies": "DefaultMovies.png", "shows": "DefaultTVShows.png", "music": "DefaultMusicAlbums.png"}
        for library in libraries:
            params = {"route": "library", "id": library["id"], "kind": library["kind"]}
            self.folder(library["name"], params, icons.get(library["kind"], "DefaultFolder.png"))
        self.folder(kodi.text("playlists"), {"route": "playlists"}, "DefaultPlaylist.png")
        self.folder(kodi.text("search"), {"route": "search"}, "DefaultAddonsSearch.png")
        # Discover and the requests, where the server takes them.
        self.asking_entries(summary)
        self.action(kodi.text("settings"), {"route": "settings"})
        self.end()

    def route_hub(self) -> None:
        key = self.params["key"]
        hubs = self.client().get("/api/hubs/home", summaries=1).get("hubs", [])
        hub = next((h for h in hubs if h["key"] == key), None)
        self.items(hub["items"] if hub else [])
        self.end()

    def route_library(self) -> None:
        library, kind = self.params["id"], self.params.get("kind")
        offset = int(self.params.get("offset", 0))
        client = self.client()
        items = lambda: client.get(  # noqa: E731
            "/api/libraries/%s/items" % library, sort="title", offset=offset, limit=PAGE, summaries=1
        )
        if offset == 0 and kind in ("movies", "shows"):
            sets, page = api.together(lambda: client.get("/api/libraries/%s/collections" % library), items)
            if sets.get("collections"):
                self.folder(kodi.text("collections"), {"route": "collections", "id": library}, "DefaultSets.png")
        else:
            page = items()
        self.items(page.get("items", []))
        if offset + PAGE < page.get("total", 0):
            params = {"route": "library", "id": library, "kind": kind, "offset": offset + PAGE}
            self.folder(kodi.text("next_page"), params, "DefaultFolder.png", last=True)
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_VIDEO_YEAR)
        self.end()

    def route_item(self) -> None:
        children = self.client().get("/api/items/%s/children" % self.params["id"], summaries=1).get("items", [])
        self.items(children, in_season=True)
        if children and children[0]["kind"] in ("episode", "track"):
            xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
            xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_LABEL)
        self.end()

    def route_collections(self) -> None:
        client = self.client()
        for collection in client.get("/api/libraries/%s/collections" % self.params["id"]).get("collections", []):
            li = xbmcgui.ListItem(collection["title"], offscreen=True)
            li.setArt(
                {
                    "poster": listing.image(self.proxy_base, collection.get("poster_url"), 500),
                    "fanart": listing.image(self.proxy_base, collection.get("backdrop_url"), 1920),
                }
            )
            tag = li.getVideoInfoTag()
            tag.setMediaType("set")
            tag.setTitle(collection["title"])
            tag.setPlot(collection.get("summary") or "")
            xbmcplugin.addDirectoryItem(self.handle, self.url(route="collection", id=collection["id"]), li, isFolder=True)
        xbmcplugin.setContent(self.handle, "sets")
        self.end()

    def route_collection(self) -> None:
        self.items(self.client().get("/api/collections/%s" % self.params["id"], summaries=1).get("items", []))
        self.end()

    def route_playlists(self) -> None:
        for playlist in self.client().get("/api/playlists").get("playlists", []):
            li = xbmcgui.ListItem("%s (%d)" % (playlist["title"], playlist.get("item_count", 0)), offscreen=True)
            artwork = playlist.get("artwork") or []
            if artwork:
                li.setArt({"thumb": listing.image(self.proxy_base, artwork[0], 500)})
            li.addContextMenuItems(
                [
                    (kodi.text("rename_playlist"), "RunPlugin(%s)" % self.url(route="rename_playlist", id=playlist["id"])),
                    (kodi.text("delete_playlist"), "RunPlugin(%s)" % self.url(route="delete_playlist", id=playlist["id"])),
                ]
            )
            xbmcplugin.addDirectoryItem(self.handle, self.url(route="playlist", id=playlist["id"]), li, isFolder=True)
        self.end()

    def route_playlist(self) -> None:
        playlist = self.client().get("/api/playlists/%s" % self.params["id"], summaries=1)
        entries = playlist.get("entries", [])

        def extra(at):
            # A smart playlist fills itself.
            if playlist.get("smart_filter"):
                return []
            url = self.url(route="playlist_remove", id=playlist["id"], entry=entries[at]["entry_id"])
            return [(kodi.text("remove_from_playlist"), "RunPlugin(%s)" % url)]

        self.items([entry["item"] for entry in entries], extra=extra)
        self.end()

    def route_rename_playlist(self) -> None:
        client = self.client()
        path = "/api/playlists/%s" % self.params["id"]
        old = client.get(path).get("title") or ""
        title = (xbmcgui.Dialog().input(kodi.text("rename_playlist"), defaultt=old) or "").strip()
        if title and title != old:
            client.patch(path, {"title": title})
            xbmc.executebuiltin("Container.Refresh")

    def route_delete_playlist(self) -> None:
        client = self.client()
        path = "/api/playlists/%s" % self.params["id"]
        title = client.get(path).get("title") or ""
        asked = kodi.text("delete_playlist_body") % title
        if xbmcgui.Dialog().yesno(kodi.text("delete_playlist_ask"), asked, nolabel=kodi.text("cancel"), yeslabel=kodi.text("delete")):
            client.delete(path)
            kodi.notify(kodi.text("playlist_deleted"))
            xbmc.executebuiltin("Container.Refresh")

    def route_playlist_remove(self) -> None:
        self.client().delete("/api/playlists/%s/items/%s" % (self.params["id"], self.params["entry"]))
        xbmc.executebuiltin("Container.Refresh")

    def route_search(self) -> None:
        query = self.params.get("q")
        if not query:
            query = xbmcgui.Dialog().input(kodi.text("search"))
            self.end(False)
            if query:
                # The results' own address: a refresh (after playback, a
                # watched mark) shows them again rather than the keyboard.
                xbmc.executebuiltin("Container.Update(%s)" % self.url(route="search", q=query))
            return
        client = self.client()
        # TMDB's titles the libraries lack, asked at the same time, never
        # waited for (asking.Searching): where this sign-in may ask, as the
        # service last read the summary (search asks nothing more first).
        may_ask = requests.may_ask(self.requests_summary(ask=False))
        tmdb = asking.Searching(self.client(asking.FOUND_WAIT), query, self.path) if may_ask else None
        found = client.get("/api/search", q=query, limit=100, summaries=1)
        items = found.get("items", [])
        channels = found.get("channels") or []
        if not items and not channels and tmdb is None:
            kodi.notify(kodi.text("nothing_found"))
        self.items(items, content="videos")
        # Live TV's channels found by name, number or what's on: each plays.
        now = time.time()
        entries = []
        for channel in channels:
            li = livetv.list_item(self.proxy_base, channel, now)
            li.addContextMenuItems(self.channel_menu(channel))
            entries.append((livetv.play_url(channel["id"]), li, False))
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        shown = tmdb.ready() if tmdb is not None else []
        if tmdb is not None:
            if not items and not channels:
                # OK searches again.
                self.entry(kodi.text("nothing_in_libraries"), {"route": "search"}, "DefaultIconInfo.png")
            self.on_tmdb(query, shown)
        self.end()
        if shown is None:
            # Shown again if they come in time.
            tmdb.later()

    def route_play(self) -> None:
        client = self.client()
        item_id = int(self.params["id"])
        listed = self.params.get("pos")
        detail = None
        if self.resume and listed is None:
            # Not from a listing: the resume point is in the details.
            detail = client.get("/api/items/%d" % item_id)
            start_ms = (detail.get("user_state") or {}).get("position_ms", 0)
        else:
            start_ms = int(listed or 0) if self.resume else 0
        li = playback.prepare(client, self.proxy_base, item_id, start_ms, detail)
        xbmcplugin.setResolvedUrl(self.handle, True, li)

    # --------------------------------------------------------------- Live TV

    def route_live(self) -> None:
        """Live TV (livetv.py), by category: Favourites, Recently
        watched and All channels, then the groups, those of a region
        gathered under it. Nothing asks for more than a page of channels."""
        found = livetv.groups(self.client())
        if not found.get("total"):
            li = xbmcgui.ListItem(kodi.text("no_channels"), offscreen=True)
            li.setArt({"icon": "DefaultIconInfo.png", "thumb": "DefaultIconInfo.png"})
            li.getVideoInfoTag().setPlot(kodi.text("no_channels_body"))
            xbmcplugin.addDirectoryItem(self.handle, self.url(route="retry"), li, isFolder=False)
            return self.end()
        if found.get("favorites"):
            self.category(kodi.text("favourites"), found["favorites"], {"category": "favorites"}, "DefaultFavourites.png")
        if found.get("recent"):
            self.category(kodi.text("recently_watched"), found["recent"], {"category": "recent"}, "DefaultRecentlyAddedEpisodes.png")
        self.category(kodi.text("all_channels"), found["total"], {"category": "all"}, "DefaultPVRChannels.png")
        self.category(kodi.text("search_channels"), None, {"route": "live_search"}, "DefaultAddonsSearch.png")
        self.groups(livetv.group_tree(found.get("groups") or []))
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
        self.end()

    def route_live_region(self) -> None:
        """A region's groups: "Sport", "News"… of "UK"."""
        region = self.params.get("region")
        found = livetv.groups(self.client()).get("groups") or []
        self.groups([{"kind": "group", "group": g, "label": g.get("label")} for g in found if g.get("region") == region])
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
        self.end()

    def groups(self, entries: list) -> None:
        for entry in entries:
            if entry["kind"] == "region":
                self.category(entry["region"], entry["count"], {"route": "live_region", "region": entry["region"]}, "DefaultCountry.png")
            else:
                group = entry["group"]
                title = entry.get("label") or livetv.group_name(group)
                self.category(title, group.get("count"), {"category": "group", "group": group["name"]}, "DefaultGenre.png")

    def category(self, title: str, count, params: dict, icon: str) -> None:
        """A folder of Live TV's: a category of channels, how many it has."""
        params = dict({"route": "live_channels"}, **params)
        label = "%s  [COLOR %s](%s)[/COLOR]" % (title, "FF8B95A5", "{:,}".format(count)) if count is not None else title
        self.folder(label, params, icon)

    def route_live_search(self) -> None:
        """Finds channels by name, number or what's on: the results' own
        address, so going back to them doesn't ask again."""
        query = (xbmcgui.Dialog().input(kodi.text("search_channels")) or "").strip()
        self.end(False)
        if query:
            xbmc.executebuiltin("Container.Update(%s)" % self.url(route="live_channels", category="all", q=query))

    def route_live_channels(self) -> None:
        """A page of a category's channels (PAGE, as the server sends them),
        each saying what's on now (its label2) and next (its info), Next
        page after them. In a group, names leave out the group's region."""
        query = self.params.get("q")
        # A search looks across every channel, and lists the groups whose
        # names match first, to open.
        category = "all" if query else self.params.get("category") or "all"
        group = self.params.get("group") if category == "group" else None
        offset = int(self.params.get("offset", 0))
        client = self.client()
        if query and not offset:
            page, found = api.together(lambda: livetv.browse(client, "all", None, query, 0, PAGE), lambda: livetv.groups(client))
            for match in livetv.matching_groups(found.get("groups") or [], query):
                self.category(livetv.group_name(match), match.get("count"), {"category": "group", "group": match["name"]}, "DefaultGenre.png")
        else:
            page = livetv.browse(client, category, group, query, offset, PAGE)
        now = time.time()
        entries = []
        for channel in page["channels"]:
            li = livetv.list_item(self.proxy_base, channel, now, group)
            li.addContextMenuItems(self.channel_menu(channel))
            entries.append((livetv.play_url(channel["id"], group), li, False))
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        if offset + len(page["channels"]) < page["total"]:
            more = {k: v for k, v in dict(self.params, offset=offset + PAGE).items() if v}
            self.folder(kodi.text("next_page"), more, "DefaultFolder.png", last=True)
        # By number, as the server lists them; by name if asked.
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_LABEL)
        xbmcplugin.setContent(self.handle, "videos")
        self.end()

    def channel_menu(self, channel: dict) -> list:
        """A channel's menu in Kodi's lists: its favourite, its schedule."""
        favourite = bool(channel.get("favorite"))
        toggle = self.url(route="live_favourite", id=channel["id"], value="0" if favourite else "1")
        schedule = self.url(route="live_schedule", id=channel["id"])
        return [
            (kodi.text("remove_favourite" if favourite else "add_favourite"), "RunPlugin(%s)" % toggle),
            (kodi.text("schedule"), "Container.Update(%s)" % schedule),
        ]

    def route_live_schedule(self) -> None:
        """A channel's programmes, SCHEDULE_HOURS at a time from what's on
        now: what's on plays the channel; the rest say what they are."""
        client = self.client()
        channel_id = int(self.params["id"])
        now = time.time()
        start = float(self.params.get("from") or now)
        channel, programs = api.together(lambda: livetv.channel(client, channel_id), lambda: livetv.schedule(client, channel_id, start))
        if start > now:
            # A later page: what began on the page before isn't again.
            programs = [p for p in programs if p["start"] >= start]
        base = self.proxy_base
        logo = livetv.logo(base, channel)
        entries = []
        for program in programs:
            on = livetv.airing(program, now)
            when = kodi.text("on_now") if on else "%s %s" % (livetv.day(program["start"], now), livetv.clock(program["start"]))
            li = xbmcgui.ListItem("%s  %s" % (when, livetv.headline(program)), livetv.span(program), offscreen=True)
            art = {"thumb": logo, "icon": logo} if logo else {}
            picture = listing.image(base, program.get("image"), livetv.PICTURE_WIDTH)
            if picture:
                art.update(thumb=picture, fanart=picture)
            if art:
                li.setArt(art)
            tag = li.getVideoInfoTag()
            tag.setMediaType("video")
            tag.setTitle(livetv.headline(program))
            line = " · ".join(p for p in [livetv.span(program), livetv.episode(program)] + livetv.badges(program) if p)
            tag.setPlot("[B]%s[/B]\n%s\n\n%s" % (livetv.name(channel), line, (program.get("description") or "").strip()))
            if program.get("categories"):
                tag.setGenres(list(program["categories"]))
            if on:
                li.setProperty("IsPlayable", "true")
                entries.append((self.url(route=livetv.PLAY_ROUTE, id=channel_id), li, False))
            else:
                # Kodi shows its info: there is nothing to play yet.
                entries.append((self.url(route="live_info"), li, False))
        if not programs:
            li = xbmcgui.ListItem(kodi.text("nothing_scheduled") % livetv.name(channel), offscreen=True)
            li.setArt({"icon": "DefaultIconInfo.png", "thumb": "DefaultIconInfo.png"})
            entries.append((self.url(route="live_info"), li, False))
        xbmcplugin.addDirectoryItems(self.handle, entries, len(entries))
        if programs:
            later = max(p["stop"] for p in programs)
            self.folder(kodi.text("later"), {"route": "live_schedule", "id": channel_id, "from": int(later)}, "DefaultFolder.png", last=True)
        # In the order they air.
        xbmcplugin.addSortMethod(self.handle, xbmcplugin.SORT_METHOD_UNSORTED)
        xbmcplugin.setContent(self.handle, "videos")
        self.end()

    def route_live_info(self) -> None:
        """A programme still to come, chosen: its info."""
        xbmc.executebuiltin("Action(Info)")

    def route_live_play(self) -> None:
        """Plays a channel: watched for this Kodi, then its stream. Every
        connection in use (409) says so, the server's words naming the
        channels watched; the queue of channels after it is let go."""
        client = self.client()
        install = playback.install_id()
        take_over = self.params.get("take_over")
        while True:
            try:
                channel = livetv.watch(client, int(self.params["id"]), install, take_over)
                break
            except api.ApiError as e:
                if e.status != 409:
                    raise
                kodi.log("live channel %s: %s" % (self.params["id"], e.message), xbmc.LOGWARNING)
                # What holds each connection; Watch here instead, where this
                # account may take one over.
                take_over = livetv.busy(e.message, e.body.get("connections"))
                if take_over is None:
                    livetv.forget_queue()
                    return xbmcplugin.setResolvedUrl(self.handle, False, xbmcgui.ListItem())
        li = livetv.play_item(self.proxy_base, channel, install, time.time(), self.params.get("group"), take_over)
        kodi.log("watching live channel %d" % channel["id"])
        xbmcplugin.setResolvedUrl(self.handle, True, li)

    def route_live_favourite(self) -> None:
        livetv.set_favorite(self.client(), int(self.params["id"]), self.params.get("value") == "1")
        xbmc.executebuiltin("Container.Refresh")

    # ---------------------------------------------------------------- actions

    def route_watched(self) -> None:
        verb = "watched" if self.params.get("value") == "1" else "unwatched"
        self.client().post("/api/items/%s/%s" % (self.params["id"], verb))
        xbmc.executebuiltin("Container.Refresh")

    def route_my_list(self) -> None:
        path = "/api/watchlist/%s" % self.params["id"]
        if self.params.get("value") == "1":
            self.client().put(path)
        else:
            self.client().delete(path)
        xbmc.executebuiltin("Container.Refresh")

    def route_settings(self) -> None:
        kodi.addon().openSettings()

    def route_tracks(self) -> None:
        """Audio and subtitles (from the add-on's settings): the profile's
        defaults, in Kodi's dialogs."""
        from . import tracks

        client = self.client()
        me = client.get("/api/auth/me")
        dialog = xbmcgui.Dialog()
        tracks.edit_defaults(
            client,
            me,
            lambda heading, labels, preselect: dialog.select(heading, labels, preselect=preselect),
            lambda words, error: kodi.notify(words, error=error),
            kodi.text,
            api.ApiError,
        )

    def route_subtitles(self) -> None:
        """Kodi's subtitle dialog (Subtitles → Download in the player), with
        pley as one of its services: the results for the title pley plays,
        or the one picked, added on the server and handed to Kodi as a
        file. Anything else playing lists nothing."""
        from . import subtitles

        found = subtitles.playing(kodi.get_property(kodi.PLAYING))
        try:
            if self.params.get("action") == "download":
                self.subtitle_download(found)
            elif found is not None:
                self.subtitle_search(found)
        except (api.ApiError, ValueError, OSError) as e:
            kodi.log("subtitles: %r" % e, xbmc.LOGWARNING)
            message = subtitles.problem(e, kodi.text) if isinstance(e, api.ApiError) else kodi.text("server_error") % e
            kodi.notify(message, error=not (isinstance(e, api.ApiError) and e.status == 503))
        self.end()

    def subtitle_search(self, found: dict) -> None:
        from . import subtitles

        results = subtitles.search(self.client(), found["item_id"], found.get("file_id"))
        convert = getattr(xbmc, "convertLanguage", None)
        english = getattr(xbmc, "ENGLISH_NAME", None)
        for url, label, label2, art, props in subtitles.entries(
            results, self.base_url, found["item_id"], found.get("file_id"), (lambda code: convert(code, english)) if convert else None
        ):
            li = xbmcgui.ListItem(label=label, label2=label2, offscreen=True)
            li.setArt(art)
            for name, value in props.items():
                li.setProperty(name, value)
            xbmcplugin.addDirectoryItem(self.handle, url, li, isFolder=False)

    def subtitle_download(self, found) -> None:
        """Adds the result picked, saves it as the profile's choice, and
        gives Kodi its file, which Kodi loads into the player and shows.
        The service stops following track changes in this playback: Kodi
        lists the new track where the playback's layout doesn't know it."""
        from . import subtitles

        client = self.client()
        item_id = int(self.params["item"])
        file_id = int(self.params["file"]) if self.params.get("file") else None
        track = subtitles.add(client, item_id, file_id, self.params["result"])
        path = subtitles.save(client, track, os.path.join(kodi.temp_dir(), "pley-subtitles"))
        try:
            client.put("/api/items/%d/streams" % item_id, subtitles.chosen(file_id, track["index"]))
        except api.ApiError as e:
            kodi.log("added subtitles not saved as the choice: %s" % e.message, xbmc.LOGWARNING)
        if found is not None and found.get("item_id") == item_id:
            kodi.set_property(subtitles.ADDED, found.get("session_id") or "")
        xbmcplugin.addDirectoryItem(self.handle, path, xbmcgui.ListItem(label=path, offscreen=True), isFolder=False)

    def route_profiles(self) -> None:
        """Switch profile (the add-on's settings, Kodi's lists): the
        household in Kodi's list, a PIN in its number dialog, never kept."""
        from . import profiles

        client = self.client()
        dialog = xbmcgui.Dialog()
        household = profiles.household(client)
        if household is None:
            # Signed in before the add-on asked for `profiles`.
            if dialog.yesno(kodi.text("switch_profile"), kodi.text("cant_switch"), nolabel=kodi.text("cancel"), yeslabel=kodi.text("sign_in")):
                self.route_sign_in()
            return
        if len(household) < 2:
            return kodi.notify(kodi.text("no_other_profile"))
        try:
            # Who the add-on is: the one in use says "PIN" when it has one.
            me = client.get("/api/auth/me") or {}
        except api.ApiError:
            me = {}
        entries = []
        for profile in household:
            li = xbmcgui.ListItem(profile.get("name") or "", kodi.text("pin") if profiles.needs_pin(profile, me) else "", offscreen=True)
            picture = listing.image(self.proxy_base, profile.get("avatar_url"), 256)
            li.setArt({"icon": picture or "DefaultUser.png", "thumb": picture or "DefaultUser.png"})
            entries.append(li)
        current = next((i for i, p in enumerate(household) if p.get("current")), 0)
        chosen = dialog.select(kodi.text("whos_watching"), entries, preselect=current, useDetails=True)
        if chosen < 0 or household[chosen].get("current"):
            return
        profile = household[chosen]
        name = profile.get("name") or ""
        needs = profile.get("needs")
        if needs not in ("none", "pin"):
            return self.password_profile(profile) if needs == "password" else kodi.notify(kodi.text("profile_locked"), error=True)
        pin, problem = None, ""
        while True:
            if needs == "pin":
                # After a wrong PIN, or too many, the heading says why.
                heading = kodi.text("pin_of") % name
                pin = dialog.numeric(0, "%s %s" % (problem, heading) if problem else heading, "", True)
                if not pin:
                    return
            try:
                profiles.switch(client, profile["id"], pin)
                break
            except api.ApiError as e:
                if e.code == "browser":
                    return self.password_profile(profile)
                if e.code == "wrong_pin" and not pin:
                    # The list was older than the profile's new PIN.
                    needs = "pin"
                    continue
                if e.code in ("wrong_pin", "throttled"):
                    needs, problem = "pin", profiles.refusal(e, kodi.text, self.server_name())
                    continue
                return kodi.notify(profiles.refusal(e, kodi.text, self.server_name()), error=True)
        kodi.set_property(profiles.GATE, "1")
        kodi.notify(kodi.text("watching_as") % name)
        if kodi.get_property(kodi.WINDOW_OPEN):
            xbmc.executebuiltin("NotifyAll(%s,%s)" % (kodi.ADDON_ID, profiles.SWITCHED))
        # Requests are the profile's own.
        self.notify_service(forget=True)
        xbmc.executebuiltin("Container.Refresh")

    def password_profile(self, profile: dict) -> None:
        """A profile that needs its password: signed in to again with a
        code, chosen where the code is approved."""
        name = profile.get("name") or ""
        if xbmcgui.Dialog().yesno(name, kodi.text("needs_password") % (name, name), nolabel=kodi.text("cancel"), yeslabel=kodi.text("sign_in")):
            self.route_sign_in()

    def route_choose_server(self) -> None:
        dialog = xbmcgui.Dialog()
        progress = xbmcgui.DialogProgressBG()
        progress.create("Pley", kodi.text("looking"))
        try:
            from . import discovery

            servers = discovery.find()
        finally:
            progress.close()
        names = ["%s (%s)" % (s["name"], s["url"].split("://", 1)[1]) for s in servers]
        chosen = dialog.select(kodi.text("choose_server"), names + [kodi.text("enter_address")])
        if chosen < 0:
            return
        if chosen < len(servers):
            server, name, server_id = servers[chosen]["url"], servers[chosen]["name"], servers[chosen]["id"]
        else:
            typed = dialog.input(kodi.text("enter_address"), defaultt=server_setting())
            if not typed:
                return
            try:
                server = api.normalize(typed)
            except ValueError:
                return dialog.ok("Pley", kodi.text("not_pley") % typed)
            name, server_id = server.split("://", 1)[1], ""
        from . import auth

        try:
            auth.metadata(server, kodi.user_agent())
        except api.ApiError as e:
            kodi.log("%s isn't a Pley server: %s" % (server, e.message), xbmc.LOGWARNING)
            if e.code == "unencrypted":
                message = kodi.text("unencrypted")
            elif e.status == 0 and e.code == "unreachable":
                message = kodi.text("not_reachable")
            else:
                message = kodi.text("not_pley")
            return dialog.ok("Pley", message % server)
        current = auth.Store(os.path.join(kodi.profile_dir(), "tokens.json")).load()
        if current and current.get("server") == server:
            # Signed in there already.
            choose(server, name, server_id or current.get("server_id", ""))
            return xbmc.executebuiltin("Container.Refresh")
        # The sign-in there stays until this one is made (cancelled, it's
        # kept). A server found at a new address signs in again: anyone on
        # the network can answer a broadcast with its id, and its tokens
        # would go to them.
        self.sign_in(server, name, server_id)

    def route_sign_in(self) -> None:
        server = server_setting()
        if not server:
            return self.route_choose_server()
        self.sign_in(server, kodi.setting("server_name") or server, kodi.setting("server_id"))

    def route_sign_out(self) -> None:
        if kodi.ask_sign_out():
            self.sign_out()

    def sign_out(self) -> None:
        """Signs out, asked already, and says so."""
        if self.sign_out_quietly():
            kodi.notify(kodi.text("signed_out"))
        else:
            kodi.notify(kodi.text("service_off"), error=True)
        xbmc.executebuiltin("Container.Refresh")

    def sign_out_quietly(self) -> bool:
        """Signs out through the service, which holds the sign-in: whether it
        could."""
        try:
            return self.client().request("POST", "/sign-out").status == 200
        except api.ApiError:
            return False

    def sign_in(self, server: str, name: str, server_id: str) -> None:
        from . import auth, profiles, signin

        flow = auth.DeviceSignIn(server, kodi.user_agent())
        started = flow.start()
        expires = time.time() + int(started.get("expires_in") or 600)
        qr = fetch_qr(server, started["user_code"])
        window = signin.SignIn("pley-signin.xml", kodi.addon_dir(), "default", "1080i")
        window.fill(started["verification_uri"], started["user_code"], waiting_text(expires - time.time()), qr)
        window.show()
        # Whether the server requires two-step sign-in, asked beside the
        # sign-in: the screen says so once the server answers, and the code
        # never waits for it.
        threading.Thread(target=lambda: two_step_required(server) and window.show_two_step(), daemon=True).start()
        monitor = xbmc.Monitor()
        outcome = None
        state = None
        try:
            while outcome is None:
                waited = 0.0
                while waited < flow.interval:
                    if window.cancelled or monitor.abortRequested():
                        break
                    xbmc.sleep(200)
                    waited += 0.2
                if window.cancelled or monitor.abortRequested():
                    break
                left = expires - time.time()
                if left <= 0:
                    outcome = kodi.text("code_expired")
                    break
                try:
                    state, tokens = flow.poll(started["device_code"])
                except api.ApiError as e:
                    if e.status != 0:
                        raise
                    # The network dropped: the person may have allowed it on
                    # their phone already. Asked again until the code expires.
                    window.set_status(kodi.text("retrying"))
                    continue
                window.set_status(waiting_text(left))
                if state == "allowed":
                    store = auth.Store(os.path.join(kodi.profile_dir(), "tokens.json"))
                    previous = store.load()
                    record = flow.record(tokens, name, server_id)
                    store.save(record)
                    choose(server, name, server_id)
                    # Whoever approved it chose the profile: "Who's watching?" is answered.
                    kodi.set_property(profiles.GATE, "1")
                    # What it may do with requests is the new sign-in's.
                    self.notify_service(forget=True)
                    if previous and previous.get("refresh_token") != record["refresh_token"]:
                        # The sign-in this one replaces ends on its server too.
                        auth.revoke(previous, kodi.user_agent())
                    outcome = signed_in_text(server, tokens["access_token"])
                elif state == "denied":
                    outcome = kodi.text("code_refused")
                elif state == "expired":
                    outcome = kodi.text("code_expired")
        finally:
            window.close()
            if qr:
                try:
                    os.remove(qr)
                except OSError:
                    pass
        if window.change:
            # Change server, from the sign-in screen as on the Android app's.
            return self.route_choose_server()
        if outcome is None:
            return
        if state == "allowed":
            kodi.notify(outcome)
            xbmc.executebuiltin("Container.Refresh")
        else:
            xbmcgui.Dialog().ok("Pley", outcome)


# Routes that run as actions, with no listing to end; of them, those that
# hand Kodi what to play.
ACTIONS = (
    "play",
    "live_play",
    "live_favourite",
    "live_info",
    "watched",
    "my_list",
    "settings",
    "tracks",
    "profiles",
    "rename_playlist",
    "delete_playlist",
    "playlist_remove",
    "choose_server",
    "sign_in",
    "sign_out",
    "retry",
) + asking.ACTIONS
RESOLVING = ("play", "live_play")


def choose(server: str, name: str, server_id: str) -> None:
    """Makes `server` the one the add-on shows."""
    kodi.set_setting("server_url", server)
    kodi.set_setting("server_name", name)
    kodi.set_setting("server_id", server_id)


def signed_in_text(server: str, access_token: str) -> str:
    """"Signed in as Alice", or "Signed in" when the server doesn't say
    who (the sign-in worked all the same)."""
    try:
        me = api.Client(server, kodi.user_agent(), access_token, timeout=5).get("/api/auth/me") or {}
        name = me.get("name") or me.get("username")
    except (api.ApiError, AttributeError):
        name = None
    return kodi.text("signed_in_as") % name if name else kodi.text("signed_in")


def fetch_qr(server: str, user_code: str) -> str:
    """The server's QR code of where to type `user_code`, as a file for Kodi
    to show; "" when the server draws none (it predates them). The code
    names the file, so a code that isn't letters and digits (pley's are 8
    letters) has none: a server's answer never leads outside Kodi's
    temporary folder."""
    code = user_code.replace("-", "")
    if not re.fullmatch(r"[A-Z0-9]{4,16}", code):
        return ""
    try:
        answer = api.Client(server, kodi.user_agent()).request("GET", "/api/oauth/device/qr", query={"code": code})
    except api.ApiError:
        return ""
    if answer.status != 200 or not answer.body.startswith(b"\x89PNG"):
        return ""
    path = os.path.join(kodi.temp_dir(), "pley-qr-%s.png" % code)
    with open(path, "wb") as f:
        f.write(answer.body)
    return path


def two_step_required(server: str) -> bool:
    """Whether `server` requires two-step sign-in of every account with a
    password (GET /api/server). The phone that approves the code sets it up
    for whoever hasn't, so the sign-in screen only says so: False when the
    server doesn't say (it predates it) or doesn't answer."""
    try:
        found = api.Client(server, kodi.user_agent(), timeout=5).get("/api/server")
    except api.ApiError:
        return False
    return isinstance(found, dict) and found.get("two_step_required") is True


def waiting_text(seconds_left: float) -> str:
    minutes = int(seconds_left // 60)
    if minutes < 1:
        return kodi.text("waiting_last_minute")
    if minutes == 1:
        return kodi.text("waiting_one_minute")
    return kodi.text("waiting") % minutes


def server_setting() -> str:
    """The chosen server's address, as pley writes it ("" when there is
    none, or it was typed into the settings as something else)."""
    try:
        return api.normalize(kodi.setting("server_url")) if kodi.setting("server_url").strip() else ""
    except ValueError:
        return ""


def run(argv=None) -> None:
    Plugin(argv or sys.argv).run()
