' Where the screens go: their names, and the screen a tile opens.

' The component of a screen by its name in a navigate request, or "".
function navScreen(name as string) as string
    screens = {
        servers: "ServersScreen",
        link: "LinkScreen",
        profiles: "ProfilesScreen",
        home: "HomeScreen",
        live: "LiveTvScreen",
        livePlayer: "LivePlayerScreen",
        library: "LibraryScreen",
        show: "ShowScreen",
        details: "DetailsScreen",
        grid: "GridScreen",
        collection: "CollectionScreen",
        person: "PersonScreen",
        artist: "ArtistScreen",
        album: "AlbumScreen",
        nowPlaying: "NowPlayingScreen",
        playlist: "PlaylistScreen",
        player: "PlayerScreen",
        search: "SearchScreen",
        settings: "SettingsScreen",
        history: "HistoryScreen",
        diagnostics: "TextScreen",
        licences: "TextScreen",
        sortLibraries: "SortLibrariesScreen",
        unreachable: "UnreachableScreen",
        discover: "DiscoverScreen",
        discoverList: "DiscoverListScreen",
        requestTitle: "RequestTitleScreen",
        requestSeasons: "RequestSeasonsScreen",
        requests: "RequestsScreen"
    }
    if not screens.DoesExist(name) then return ""
    return screens[name]
end function

' Where a menu's focus goes as it is built again (keys: its entries'):
' to the entry it was on (key; "" for none), wherever that moved, else
' as far down as it was (at), within the menu.
function navMenuFocus(keys as object, key as string, at as integer) as integer
    if key <> ""
        for i = 0 to keys.Count() - 1
            if keys[i] = key then return i
        end for
    end if
    if at >= keys.Count() then at = keys.Count() - 1
    if at < 0 then at = 0
    return at
end function

' The screen a tile opens: { to, params }, or invalid for kinds this
' version doesn't open. A season opens its show on that season; a
' collection, a person and an artist, their page; a playlist, its
' entries; a track, its album; an extra plays at once. A
' TMDB title opens its page (Request more seasons, with its season
' picker); See all, its list, or Discover's search for its words.
function navForTile(tile as object) as dynamic
    kind = httpString(tile.kind)
    if kind = "discover" or kind = "askMore"
        tmdbId = httpNumber(tile.tmdbId, 0)
        tmdbKind = httpString(tile.tmdbKind)
        if tmdbId = 0 or (tmdbKind <> "movie" and tmdbKind <> "show") then return invalid
        if kind = "askMore" then return { to: "requestTitle", params: { kind: tmdbKind, tmdbId: tmdbId, title: httpString(tile.showTitle), pick: true } }
        return { to: "requestTitle", params: { kind: tmdbKind, tmdbId: tmdbId, title: httpString(tile.title) } }
    end if
    if kind = "more"
        query = httpString(tile.query)
        list = httpString(tile.list)
        if query <> "" then return { to: "search", params: { discover: true, query: query } }
        if list = "requests" then return { to: "requests", params: { view: "mine" } }
        return { to: "discoverList", params: { list: list } }
    end if
    id = httpNumber(tile.itemId, 0)
    if id = 0 then return invalid
    if kind = "movie" or kind = "episode" or kind = "show" then return { to: "details", params: { itemId: id } }
    if kind = "season"
        showId = httpNumber(tile.parentId, 0)
        if showId = 0 then return invalid
        return { to: "show", params: { showId: showId, seasonId: id } }
    end if
    if kind = "collection" or kind = "person" or kind = "artist" then return { to: kind, params: { id: id, title: httpString(tile.title) } }
    if kind = "album" then return { to: "album", params: { id: id, title: httpString(tile.title) } }
    if kind = "track"
        albumId = httpNumber(tile.parentId, 0)
        if albumId = 0 then return invalid
        return { to: "album", params: { id: albumId, title: "", trackId: id } }
    end if
    if kind = "playlist" then return { to: "playlist", params: { id: id, title: httpString(tile.title) } }
    if kind = "extra" then return { to: "player", params: { itemId: id, startMs: 0 } }
    return invalid
end function
