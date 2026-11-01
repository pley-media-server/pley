' Title pages' rows, people's and collections' pages, and the Options key.

sub test_pages_credits_cast_then_crew_once_each()
    detail = {
        cast: [
            { id: 7, name: "Amy Adams", role: "actor", character: "Louise Banks", photo_url: "/api/people/7/photo?v=1" },
            { id: 8, name: "Jeremy Renner", role: "actor", character: "Ian Donnelly" },
            { id: 7, name: "Amy Adams", role: "actor", character: "Louise (young)" }
        ],
        crew: [
            { id: 9, name: "Denis Villeneuve", role: "director" },
            { id: 9, name: "Denis Villeneuve", role: "writer" },
            { id: 10, name: "Eric Heisserer", role: "writer" },
            { id: 11, name: "Someone", role: "editor" },
            { id: 8, name: "Jeremy Renner", role: "producer" }
        ]
    }
    tiles = pageCredits(detail, "http://nas:7539")
    names = []
    for each tile in tiles
        names.Push(tile.title + ": " + tile.subtitle)
    end for
    tkEqual(names, ["Amy Adams: Louise Banks", "Jeremy Renner: Ian Donnelly, Producer", "Denis Villeneuve: Director, Writer", "Eric Heisserer: Writer"])
    tkEqual(tiles[0].image, "http://nas:7539/api/people/7/photo?v=1&w=220")
    tkEqual(tiles[0].kind, "person")
    tkEqual(tiles[0].itemId, 7)
    tkEqual(tiles[1].image, "", "no photo: the name shows")
end sub

sub test_pages_extras_say_what_they_are()
    detail = { extras: [{ id: 40, kind: "extra", title: "Official Trailer", extra_type: "trailer", duration_ms: 150000, thumb_url: "/api/items/40/images/thumb?v=1", user_state: { position_ms: 0, watched: false } }, { id: 41, kind: "extra", title: "Making of", extra_type: "odd" }] }
    tiles = pageExtras(detail, "http://nas:7539")
    tkEqual(tiles[0].title, "Official Trailer")
    tkEqual(tiles[0].subtitle, "Trailer · 3 min")
    tkEqual(tiles[0].kind, "extra")
    tkTrue(tiles[0].wide)
    tkEqual(tiles[1].subtitle, "Extra")
    rows = pageDetailRows(detail, "http://nas:7539")
    tkEqual(rows.Count(), 1, "no cast: no row for it")
    tkEqual(rows[0].key, "extras")
end sub

sub test_pages_person_credits_say_the_part()
    person = { id: 7, name: "Amy Adams", credits: [
        { item: { id: 12, kind: "movie", title: "Arrival", year: 2016, user_state: { position_ms: 0, watched: true } }, roles: [{ role: "actor", character: "Louise Banks" }] },
        { item: { id: 99, kind: "show", title: "Sharp Objects", year: 2018 }, roles: [{ role: "actor", character: "Camille", episodes: 8 }, { role: "producer" }] },
        { item: { id: 5, kind: "collection", title: "Odd" }, roles: [] }
    ] }
    tiles = pagePersonCredits(person, "http://nas:7539")
    tkEqual(tiles.Count(), 2, "titles this version shows")
    tkEqual(tiles[0].subtitle, "Louise Banks")
    tkTrue(tiles[0].watched)
    tkEqual(tiles[1].subtitle, "Camille, Producer")
end sub

sub test_pages_collections_and_seasons_as_tiles()
    tile = pageCollectionTile({ id: 3, title: "Dune Collection", item_count: 2 }, "http://nas:7539")
    tkEqual(tile.image, "http://nas:7539/api/collections/3/images/poster?w=220", "the server's picture of it")
    tkEqual(tile.subtitle, "2 titles")
    tkEqual(tile.kind, "collection")
    tkEqual(pageCollectionTile({ id: 4, title: "One", item_count: 1, poster_url: "/api/collections/4/images/poster?v=2" }, "http://nas:7539").image, "http://nas:7539/api/collections/4/images/poster?v=2&w=220")
    seasons = pageSeasonTiles([{ id: 100, kind: "season", index: 1, parent_id: 99, leaf_count: 9 }, { id: 101, kind: "season", index: 0, title: "", leaf_count: 1 }, { id: 5, kind: "episode" }], "http://nas:7539")
    tkEqual(seasons.Count(), 2)
    tkEqual(seasons[0].title, "Season 1")
    tkEqual(seasons[0].subtitle, "9 episodes")
    tkEqual(seasons[0].parentId, 99)
    tkEqual(seasons[1].title, "Specials")
    tkEqual(seasons[1].subtitle, "1 episode")
end sub

sub test_pages_collection_of_movies_plays_them_in_order()
    detail = {
        id: 3, library_id: 1, title: "Dune Collection", summary: "Both parts.", automatic: true, sort: "release", smart_filter: invalid, item_count: 2,
        poster_url: "/api/collections/3/images/poster?v=1", backdrop_url: "",
        items: [
            { id: 12, kind: "movie", title: "Dune", year: 2021, duration_ms: 9000000, user_state: { position_ms: 600000, watched: false } },
            { id: 13, kind: "movie", title: "Dune: Part Two", year: 2024, duration_ms: 9900000, backdrop_url: "/api/items/13/images/backdrop?v=1" },
            { id: 5, kind: "collection", title: "Odd" }
        ]
    }
    page = pageCollection(detail, "http://nas:7539")
    tkEqual(page.title, "Dune Collection")
    tkEqual(page.summary, "Both parts.")
    tkTrue(page.automatic, "from TMDB")
    tkFalse(page.smart)
    tkEqual(page.count, 2)
    tkEqual(page.libraryId, 1)
    tkEqual(page.notes, ["By release date"])
    tkEqual(page.backdrop, "http://nas:7539/api/collections/3/images/backdrop?w=640", "the server's picture of it")
    tkTrue(page.movies)
    tkEqual(page.shown, 2, "titles this version shows")
    tkEqual(page.tiles[1].title, "Dune: Part Two")
    tkEqual(page.queue.Count(), 2)
    tkEqual(page.queue[0].itemId, 12)
    tkEqual(page.queue[0].startMs, 600000, "from where it was left")
    tkEqual(page.queue[1].itemId, 13)
    tkEqual(page.shows, [])
    tkEqual(pageCollectionFacts(page, { name: "Movies", kind: "movies" }), "2 movies · Movies · By release date")
    tkEqual(pageCollectionFacts(page, invalid), "2 titles · By release date", "the libraries not known yet")
end sub

sub test_pages_collection_of_shows_plays_their_next_episodes()
    detail = {
        id: 4, library_id: 2, title: "Prestige drama", summary: invalid, automatic: false, sort: "custom", smart_filter: "type=2&genre=Drama&unwatched=1&sort=year%3Adesc", item_count: 2,
        backdrop_url: "/api/collections/4/images/backdrop?v=7",
        items: [{ id: 99, kind: "show", title: "Severance" }, { id: 98, kind: "show", title: "Succession" }]
    }
    page = pageCollection(detail, "http://nas:7539")
    tkFalse(page.movies)
    tkTrue(page.smart)
    tkEqual(page.summary, "")
    tkEqual(page.queue, [], "shows play their next episode")
    tkEqual(page.shows, [99, 98])
    tkEqual(page.notes, ["Genre: Drama", "Unwatched", "Newest first"], "what its filter keeps, not its order")
    tkEqual(page.backdrop, "http://nas:7539/api/collections/4/images/backdrop?v=7&w=640")
    tkEqual(pageCollectionFacts(page, { name: "TV Shows", kind: "shows" }), "2 shows · TV Shows · Genre: Drama · Unwatched · Newest first")
    empty = pageCollection({ id: 6, title: "Empty", sort: "title", item_count: 0, items: [] }, "http://nas:7539")
    tkEqual(empty.shown, 0)
    tkEqual(pageCollectionFacts(empty, { name: "Movies", kind: "movies" }), "0 movies · Movies · By title")
end sub

sub test_pages_smart_filters_in_words()
    tkEqual(pageFilterNotes("type=1&genre=Comedy&unwatched=1&sort=year%3Adesc"), ["Genre: Comedy", "Unwatched", "Newest first"])
    tkEqual(pageFilterNotes("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20"), ["After 2019", "Recently added first", "At most 20"])
    tkEqual(pageFilterNotes("resolution=4k"), ["resolution: 4k"])
    tkEqual(pageFilterNotes("genre=Science+Fiction&year%3C%3C=2000&decade=1980&unwatched=0&inProgress=1&contentRating=PG-13&studio=A24&label=Kids"), ["Genre: Science Fiction", "Before 2000", "The 1980s", "Watched", "In progress", "Rated PG-13", "Studio: A24", "Label: Kids"])
    tkEqual(pageFilterNotes("&&sort=titleSort"), ["By title"], "empty parts left out")
    tkEqual(pageFilterOrder("titleSort:desc,year"), "By title, Z to A", "the first field counts")
    tkEqual(pageFilterOrder("lastViewedAt"), "Least recently played first")
    tkEqual(pageFilterOrder("mediaHeight:desc"), "Sorted by mediaHeight")
    tkEqual(pageCollectionOrder("title"), "By title")
    tkEqual(pageCollectionOrder("custom"), "Custom order")
    tkEqual(pageCollectionOrder(invalid), "")
end sub

sub test_pages_tile_options_by_kind()
    labels = function(options as object) as object
        out = []
        for each option in options
            out.Push(option.label)
        end for
        return out
    end function
    tkEqual(labels(pageTileOptions({ kind: "movie", itemId: 12, watched: false }, false, false)), ["Mark as watched", "Add to My List", "Add to playlist"])
    tkEqual(labels(pageTileOptions({ kind: "movie", itemId: 12, watched: true }, true, true)), ["Mark as unwatched", "Remove from My List", "Add to playlist", "Remove from Continue Watching"])
    tkEqual(labels(pageTileOptions({ kind: "show", itemId: 99 }, false, invalid)), ["Mark as watched", "Add to playlist"], "My List not known")
    tkEqual(labels(pageTileOptions({ kind: "episode", itemId: 101, grandparentId: 99 }, true, false)), ["Mark as watched", "Add to playlist", "Remove from Continue Watching", "Go to show"])
    tkEqual(pageTileOptions({ kind: "person", itemId: 7 }, false, false).Count(), 0)
    tkEqual(labels(pageTileOptions({ kind: "album", itemId: 5 }, false, false)), ["Add to playlist"], "music: playlists only")
end sub

sub test_pages_playlists()
    road = { id: 1, title: "Road trip", kind: "video", item_count: 12, duration_ms: 12000000, artwork: ["/api/items/12/images/poster?v=1"], smart_filter: invalid }
    songs = { id: 2, title: "Songs", kind: "audio", item_count: 1, duration_ms: 200000, artwork: [], smart_filter: invalid }
    smart = { id: 3, title: "Unwatched dramas", kind: "video", item_count: 40, duration_ms: 0, artwork: [], smart_filter: "genre=Drama&unwatched=1" }
    tile = pagePlaylistTile(road, "http://nas:7539")
    tkEqual(tile.subtitle, "12 titles · 3 h 20 min")
    tkEqual(tile.image, "http://nas:7539/api/items/12/images/poster?v=1&w=220")
    tkEqual(tile.kind, "playlist")
    tkEqual(pagePlaylistLine(songs), "1 track · 3 min")
    tkEqual(pagePlaylistLine(smart), "Smart · 40 titles")
    tkEqual(pagePlaylistChoices([road, songs, smart], "video"), [{ id: 1, title: "Road trip" }], "the kind's own, not smart ones")
    tkEqual(pagePlaylistKind("album"), "audio")
    tkEqual(pagePlaylistKind("show"), "video")
    entries = [{ entry_id: 5, item: contentTestMovie() }, { entry_id: 6, item: contentTestEpisode() }]
    rows = pagePlaylistEntries(entries, "http://nas:7539")
    tkEqual(rows[0].entryId, 5)
    tkEqual(rows[0].subtitle, "2016 · 1 h 46 min left")
    tkEqual(rows[1].subtitle, "S1 E1 · Good News About Hell · 55 min · Watched")
    queue = pagePlaylistQueue(entries, "http://nas:7539")
    tkEqual(queue[0].title, "Arrival")
    tkEqual(queue[0].line, "2016", "a movie's year")
    tkEqual(queue[0].startMs, 600000)
    tkEqual(queue[1].line, "S1 E1 · Good News About Hell")
    tkEqual(pageQueueFrom(queue, 1, false).Count(), 1, "from the entry chosen")
    shuffled = pageQueueFrom(queue, 1, true)
    tkEqual(shuffled.Count(), 2, "shuffled: every entry")
    tkEqual(pagePlaylistRefusal({ status: 403, message: "not allowed" }), "This sign-in can't change playlists: sign out in Settings and sign in again to allow it.")
end sub

sub test_pages_watch_history()
    now = 1800000000
    tkEqual(pageRelativeTime(now - 30, now), "now")
    tkEqual(pageRelativeTime(now - 720, now), "12 min ago")
    tkEqual(pageRelativeTime(now - 3600, now), "1 hour ago")
    tkEqual(pageRelativeTime(now - 7300, now), "2 hours ago")
    tkEqual(pageRelativeTime(now - 90000, now), "yesterday")
    tkEqual(pageRelativeTime(now - 86400 * 5, now), "5 days ago")
    tkEqual(pageRelativeTime(1772582400, now), "Mar 4, 2026")
    rows = pageHistoryRows([{ id: 9, viewed_at: now - 7300, device: "Roku Ultra", item: contentTestEpisode() }, { id: 10, viewed_at: now, item: { id: 7, kind: "person" } }], "http://nas:7539", now)
    tkEqual(rows.Count(), 1, "kinds this version shows")
    tkEqual(rows[0].title, "Severance")
    tkEqual(rows[0].subtitle, "S1 E1 · Good News About Hell · 2 hours ago · Roku Ultra")
    tkEqual(rows[0].historyId, 9)
end sub

sub test_pages_actions_ask_the_server()
    tkEqual(pageActionRequest("watched", 12), { method: "POST", path: "/api/items/12/watched" })
    tkEqual(pageActionRequest("unwatched", 12), { method: "POST", path: "/api/items/12/unwatched" })
    tkEqual(pageActionRequest("listAdd", 12), { method: "PUT", path: "/api/watchlist/12" })
    tkEqual(pageActionRequest("listRemove", 12), { method: "DELETE", path: "/api/watchlist/12" })
    tkEqual(pageActionRequest("hide", 101), { method: "DELETE", path: "/api/continue-watching/101" })
    tkInvalid(pageActionRequest("show", 101))
end sub

sub test_pages_my_list_as_home_saw_it()
    tkInvalid(pageInList(invalid, 12), "not known yet")
    tkTrue(pageInList([5, 12], 12))
    tkFalse(pageInList([5], 12))
    tkEqual(pageListWith([5, 12], 12, false), [5])
    tkEqual(pageListWith([5], 12, true), [12, 5], "added first, as the server lists it")
end sub
