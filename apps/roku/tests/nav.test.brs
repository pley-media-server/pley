sub test_nav_names_screens()
    tkEqual(navScreen("home"), "HomeScreen")
    tkEqual(navScreen("player"), "PlayerScreen")
    tkEqual(navScreen("unreachable"), "UnreachableScreen")
    tkEqual(navScreen("sortLibraries"), "SortLibrariesScreen")
    tkEqual(navScreen("discover"), "DiscoverScreen")
    tkEqual(navScreen("discoverList"), "DiscoverListScreen")
    tkEqual(navScreen("requestTitle"), "RequestTitleScreen")
    tkEqual(navScreen("requestSeasons"), "RequestSeasonsScreen")
    tkEqual(navScreen("requests"), "RequestsScreen")
    tkEqual(navScreen("collection"), "CollectionScreen")
    tkEqual(navScreen("collections"), "CollectionsScreen")
    tkEqual(navScreen("person"), "PersonScreen")
    tkEqual(navScreen("artist"), "ArtistScreen")
    tkEqual(navScreen("nowhere"), "")
end sub

sub test_nav_menu_focus_keeps_its_entry()
    menu = ["home", "library:3", "playlists", "search", "discover", "requests", "settings"]
    tkEqual(navMenuFocus(menu, "settings", 4), 6, "Discover and the requests came above it")
    tkEqual(navMenuFocus(menu, "library:3", 2), 1, "Live TV went")
    tkEqual(navMenuFocus(menu, "nowPlaying", 7), 6, "gone from the end: the last entry")
    tkEqual(navMenuFocus(menu, "live", 1), 1, "gone: where it was")
    tkEqual(navMenuFocus(menu, "", -1), 0, "nothing focused yet")
end sub

' A TMDB title a library this account can open has whole opens the
' library's title, its TMDB page offering only Open; a show partly there,
' one with no library item and every other status open its TMDB page.
' The titles of the other apps' fixture (discover/trending.json).
sub test_nav_tmdb_title_in_a_library_opens_its_item()
    server = "http://nas:7539"
    arrival = { kind: "movie", tmdb_id: 329865, title: "Arrival", year: 2016, status: "library", item_id: 101 }
    severance = { kind: "show", tmdb_id: 95396, title: "Severance", year: 2022, status: "partial", item_id: 201 }
    robot = { kind: "movie", tmdb_id: 1184918, title: "The Wild Robot", year: 2024, status: "none", item_id: invalid }
    tkEqual(navForTile(reqTitleTile(arrival, server)), { to: "details", params: { itemId: 101 } }, "in a library: the library's title")
    tkEqual(navForTile(reqTitleTile(severance, server)), { to: "requestTitle", params: { kind: "show", tmdbId: 95396, title: "Severance" } }, "partly there: its page, to ask for more")
    tkEqual(navForTile(reqTitleTile(robot, server)), { to: "requestTitle", params: { kind: "movie", tmdbId: 1184918, title: "The Wild Robot" } })
    arrival.item_id = invalid
    tkEqual(navForTile(reqTitleTile(arrival, server)), { to: "requestTitle", params: { kind: "movie", tmdbId: 329865, title: "Arrival" } }, "in a library it can't open")
    request = reqRequestTile({ id: 4, kind: "movie", tmdb_id: 329865, title: "Arrival", status: "approved", item_id: 101 }, server, false, 0, 0)
    tkEqual(navForTile(request), { to: "requestTitle", params: { kind: "movie", tmdbId: 329865, title: "Arrival" } }, "a request's card: its page, with the request")
end sub

' TMDB's titles aren't the libraries': they have no itemId.
sub test_nav_tmdb_tiles_open_their_page_or_list()
    tkEqual(navForTile({ kind: "discover", tmdbKind: "movie", tmdbId: 603, title: "The Matrix", itemId: 0 }), { to: "requestTitle", params: { kind: "movie", tmdbId: 603, title: "The Matrix" } })
    tkEqual(navForTile({ kind: "discover", tmdbKind: "show", tmdbId: 1396, title: "Breaking Bad", libraryId: 99 }), { to: "requestTitle", params: { kind: "show", tmdbId: 1396, title: "Breaking Bad" } }, "a library's item, its status not said: its TMDB page")
    tkInvalid(navForTile({ kind: "discover", tmdbKind: "person", tmdbId: 7 }))
    tkInvalid(navForTile({ kind: "discover", tmdbKind: "movie", tmdbId: 0 }))
    tkEqual(navForTile(reqMoreTile("See all", "upcoming", "")), { to: "discoverList", params: { list: "upcoming" } })
    tkEqual(navForTile(reqMoreTile("See all", "", "dune")), { to: "discoverList", params: { query: "dune" } }, "Search's: TMDB's results for its words")
    tkEqual(navForTile(reqMoreTile("See all", "requests", "")), { to: "requests", params: { view: "mine" } }, "Discover's Your requests")
    tkInvalid(navForTile(contentSeeAllTile(1, "added", false, false)), "a library row's See all: its screen opens the library")
    tkInvalid(navForTile({ kind: "discoverSeason", number: 2 }), "a season picks itself on its page")
    tkInvalid(navForTile({ kind: "cast", title: "Ada Lovelace" }))
end sub

sub test_nav_tiles_open_details_or_their_show()
    tkEqual(navForTile({ kind: "movie", itemId: 12 }), { to: "details", params: { itemId: 12 } })
    tkEqual(navForTile({ kind: "episode", itemId: 101, parentId: 100 }), { to: "details", params: { itemId: 101 } })
    tkEqual(navForTile({ kind: "show", itemId: 99 }), { to: "details", params: { itemId: 99 } })
    tkEqual(navForTile({ kind: "season", itemId: 100, parentId: 99 }), { to: "show", params: { showId: 99, seasonId: 100 } })
    tkEqual(navForTile({ kind: "person", itemId: 7, title: "Amy Adams" }), { to: "person", params: { id: 7, title: "Amy Adams" } })
    tkEqual(navForTile({ kind: "collection", itemId: 3, title: "Dune" }), { to: "collection", params: { id: 3, title: "Dune" } })
    tkEqual(navForTile({ kind: "extra", itemId: 40 }), { to: "player", params: { itemId: 40, startMs: 0 } }, "an extra plays at once")
    tkEqual(navForTile({ kind: "album", itemId: 5, title: "Blue" }), { to: "album", params: { id: 5, title: "Blue" } })
    tkEqual(navForTile({ kind: "track", itemId: 50, parentId: 5 }), { to: "album", params: { id: 5, title: "", trackId: 50 } }, "a track: its album")
    tkEqual(navForTile({ kind: "artist", itemId: 4, title: "Joni" }), { to: "artist", params: { id: 4, title: "Joni" } })
    tkInvalid(navForTile({ kind: "track", itemId: 50 }))
    tkInvalid(navForTile({ kind: "season", itemId: 100, parentId: 0 }))
    tkInvalid(navForTile({ kind: "movie" }))
end sub
