' What a title's page, a person's and a collection's show besides the
' title itself (docs/API.md): cast and crew, extras, More Like This, the
' collections it is part of, playlists, and what the Options key offers
' on a tile.
' Plain fields, which the API task makes into ContentNodes (lib/Nodes.brs).

' A person as a tile: their photo, their name, and the part they played
' or what they did (subtitle).
function pagePersonTile(person as object, server as string, subtitle as string) as object
    return {
        title: httpSentence(person.name),
        subtitle: subtitle,
        image: urlImage(server, person.photo_url, contentPosterWidth()),
        wide: false,
        progress: 0.0,
        watched: false,
        itemId: httpNumber(person.id, 0),
        kind: "person"
    }
end function

' What a crew member did: "Director", "Creator"…; "" for an actor.
function pageRoleLabel(role as string) as string
    kind = LCase(role)
    if kind = "creator" or kind = "director" or kind = "writer" or kind = "producer" then return t("titles.roles." + kind)
    return ""
end function

' Cast and crew of a title, as tiles: the actors in billing order with
' the part they played, then creators, directors, writers and producers,
' someone credited twice once with both ("Director, Writer"). At most 30.
function pageCredits(detail as object, server as string) as object
    tiles = []
    seen = {}
    ' Each person's roles said already ("9:director"): a role credited
    ' twice is said once.
    said = {}
    for each credit in httpArray(detail.cast)
        id = 0
        if GetInterface(credit, "ifAssociativeArray") <> invalid then id = httpNumber(credit.id, 0)
        key = urlValue(id)
        if id > 0 and not seen.DoesExist(key) and tiles.Count() < 30
            seen[key] = tiles.Count()
            tiles.Push(pagePersonTile(credit, server, httpSentence(credit.character)))
        end if
    end for
    for each credit in httpArray(detail.crew)
        id = 0
        if GetInterface(credit, "ifAssociativeArray") <> invalid then id = httpNumber(credit.id, 0)
        key = urlValue(id)
        kind = LCase(httpString(credit.role))
        role = pageRoleLabel(kind)
        if id > 0 and role <> "" and not said.DoesExist(key + ":" + kind)
            said[key + ":" + kind] = true
            if seen.DoesExist(key)
                tile = tiles[seen[key]]
                if tile.subtitle = ""
                    tile.subtitle = role
                else
                    tile.subtitle = tile.subtitle + ", " + role
                end if
            else if tiles.Count() < 30
                seen[key] = tiles.Count()
                tiles.Push(pagePersonTile(credit, server, role))
            end if
        end if
    end for
    return tiles
end function

' What an extra is: "Trailer", "Behind the scenes"…
function pageExtraLabel(extraType as dynamic) as string
    kind = LCase(httpString(extraType))
    ' The server's kinds and their words' keys.
    for each known in [["trailer", "trailer"], ["featurette", "featurette"], ["behind_the_scenes", "behindTheScenes"], ["deleted_scene", "deletedScene"], ["interview", "interview"], ["scene", "scene"], ["short", "short"]]
        if kind = known[0] then return t("titles.extras." + known[1])
    end for
    return t("titles.extras.other")
end function

' A title's extras as 16:9 tiles: their name, and what they are.
function pageExtras(detail as object, server as string) as object
    tiles = []
    for each extra in httpArray(detail.extras)
        if GetInterface(extra, "ifAssociativeArray") <> invalid
            tile = contentTile(extra, server, true)
            tile.kind = "extra"
            tile.subtitle = pageExtraLabel(extra.extra_type)
            duration = fmtDuration(extra.duration_ms)
            if duration <> "" then tile.subtitle = tile.subtitle + " · " + duration
            tiles.Push(tile)
        end if
    end for
    return tiles
end function

' The rows under a title's details that come with it: [{ key, title,
' wide, tiles }], cast and crew then extras, rows without tiles left out.
function pageDetailRows(detail as object, server as string) as object
    rows = []
    credits = pageCredits(detail, server)
    if credits.Count() > 0 then rows.Push({ key: "credits", title: t("titles.page.rows.castAndCrew"), wide: false, tiles: credits })
    extras = pageExtras(detail, server)
    if extras.Count() > 0 then rows.Push({ key: "extras", title: t("titles.page.rows.extras"), wide: true, tiles: extras })
    return rows
end function

' A collection as a tile: its poster (the server's own picture of it when
' the answer names none), its name and how many titles it holds.
function pageCollectionTile(collection as object, server as string) as object
    id = httpNumber(collection.id, 0)
    image = urlImage(server, collection.poster_url, contentPosterWidth())
    if image = "" then image = urlImage(server, "/api/collections/" + urlValue(id) + "/images/poster", contentPosterWidth())
    subtitle = ""
    count = httpNumber(collection.item_count, -1)
    if count >= 0 then subtitle = fmtCount(count, "")
    return { title: httpSentence(collection.title), subtitle: subtitle, image: image, wide: false, progress: 0.0, watched: false, itemId: id, kind: "collection" }
end function

function pageCollectionTiles(collections as dynamic, server as string) as object
    tiles = []
    for each collection in httpArray(collections)
        if GetInterface(collection, "ifAssociativeArray") <> invalid and httpNumber(collection.id, 0) > 0 then tiles.Push(pageCollectionTile(collection, server))
    end for
    return tiles
end function

' A collection's page (GET /api/collections/{id}): { title, summary,
' automatic (it mirrors a TMDB collection: the "TMDB" badge), smart (a
' filter fills it: "Smart"), count, libraryId, notes (what its filter
' keeps, else its order, in words), backdrop, movies, queue, shows,
' shown, tiles }. A collection of movies plays them in its order (queue:
' their Up Next cards); one of shows plays a show's next episode (shows:
' their ids, in its order).
function pageCollection(detail as object, server as string) as object
    id = httpNumber(detail.id, 0)
    tiles = []
    queue = []
    shows = []
    movies = false
    for each item in httpArray(detail.items)
        kind = ""
        if GetInterface(item, "ifAssociativeArray") <> invalid then kind = httpString(item.kind)
        if contentShown(kind)
            tiles.Push(contentTile(item, server, false))
            if kind = "movie" then movies = true
            if contentPlayable(kind) then queue.Push(contentUpNext(item, server))
            if kind = "show" or kind = "season" then shows.Push(httpNumber(item.id, 0))
        end if
    end for
    filter = httpString(detail.smart_filter)
    notes = []
    if filter <> ""
        notes = pageFilterNotes(filter)
    else
        order = pageCollectionOrder(detail.sort)
        if order <> "" then notes.Push(order)
    end if
    ' The server's picture of it when the answer names none: its own, else
    ' a title's.
    backdrop = urlImage(server, detail.backdrop_url, contentBackdropWidth())
    if backdrop = "" and id > 0 then backdrop = urlImage(server, "/api/collections/" + urlValue(id) + "/images/backdrop", contentBackdropWidth())
    return {
        title: httpSentence(detail.title),
        summary: httpClean(detail.summary, 600, false),
        automatic: httpTrue(detail.automatic),
        smart: filter <> "",
        count: httpNumber(detail.item_count, tiles.Count()),
        libraryId: httpNumber(detail.library_id, 0),
        notes: notes,
        backdrop: backdrop,
        movies: movies,
        queue: queue,
        shows: shows,
        shown: tiles.Count(),
        tiles: tiles
    }
end function

' What a collection's page says under its title: "5 movies · Movies · By
' release date". library ({ name, kind }) is invalid while the libraries
' aren't known, and its titles are counted as titles.
function pageCollectionFacts(page as object, library as dynamic) as string
    kind = ""
    if library <> invalid then kind = library.kind
    parts = [fmtCount(page.count, kind)]
    if library <> invalid and library.name <> "" then parts.Push(library.name)
    parts.Append(page.notes)
    return parts.Join(" · ")
end function

' A collection's order in words; "" for one this version doesn't know.
function pageCollectionOrder(sort as dynamic) as string
    key = httpString(sort)
    if key = "release" or key = "title" or key = "custom" then return t("titles.collection.order." + key)
    return ""
end function

' What a smart filter keeps, in words, as the other apps say it: "Genre:
' Comedy", "Unwatched", "From 2010", "4K", "Dolby Vision", "Newest first".
' The filter is a listing's query string in the compatibility API's syntax
' (docs/API.md, "Smart collections").
function pageFilterNotes(filter as string) as object
    notes = []
    for each part in filter.Tokenize("&")
        at = Instr(1, part, "=")
        key = part
        value = ""
        if at > 0
            key = Left(part, at - 1)
            value = Mid(part, at + 1)
        end if
        key = httpSentence(urlDecode(key))
        value = httpSentence(urlDecode(value))
        off = value = "0" or value = "false"
        ' The keys whose words say their value ("Genre: Comedy", "From
        ' 2010"), with their words' keys.
        worded = ""
        for each known in [["genre", "genre"], ["label", "label"], ["studio", "studio"], ["contentRating", "rated"], ["year", "year"], ["year>>", "after"], ["year<<", "before"], ["decade", "decade"], ["limit", "limit"]]
            if key = known[0] then worded = known[1]
        end for
        if worded <> ""
            notes.Push(t("titles.smartFilter." + worded, { value: value }))
        else if key = "unwatched"
            if off
                notes.Push(t("titles.smartFilter.watched"))
            else
                notes.Push(t("titles.smartFilter.unwatched"))
            end if
        else if key = "inProgress"
            if off
                notes.Push(t("titles.smartFilter.notInProgress"))
            else
                notes.Push(t("titles.smartFilter.inProgress"))
            end if
        else if key = "resolution" and fmtTokenLabel(fmtResolutions(), value) <> ""
            notes.Push(fmtTokenLabel(fmtResolutions(), value))
        else if key = "dynamicRange" and fmtTokenLabel(fmtDynamicRanges(), value) <> ""
            notes.Push(fmtTokenLabel(fmtDynamicRanges(), value))
        else if key = "sort"
            notes.Push(pageFilterOrder(value))
        else if key <> "type" and key <> "includeGuids" and key <> ""
            ' type and includeGuids are plumbing, not part of what is kept.
            notes.Push(t("titles.smartFilter.other", { name: key, value: value }))
        end if
    end for
    return notes
end function

' A smart filter's order in words: "year:desc" is "Newest first". Only its
' first field counts; python-plexapi may name the kind first
' ("movie.year").
function pageFilterOrder(sort as string) as string
    comma = Instr(1, sort, ",")
    if comma > 0 then sort = Left(sort, comma - 1)
    field = sort
    direction = ""
    colon = Instr(1, sort, ":")
    if colon > 0
        field = Left(sort, colon - 1)
        direction = Mid(sort, colon + 1)
    end if
    dot = Instr(1, field, ".")
    if dot > 0 then field = Mid(field, dot + 1)
    ' The fields and their words' keys: the audience's rating reads as the
    ' rating.
    fields = [["titleSort", "titleSort"], ["year", "year"], ["originallyAvailableAt", "originallyAvailableAt"], ["addedAt", "addedAt"], ["rating", "rating"], ["audienceRating", "rating"],
        ["userRating", "userRating"], ["lastViewedAt", "lastViewedAt"], ["duration", "duration"], ["viewCount", "viewCount"]]
    for each known in fields
        if known[0] = field
            if direction = "desc" then return t("titles.smartFilter.sort." + known[1] + ".desc")
            return t("titles.smartFilter.sort." + known[1] + ".asc")
        end if
    end for
    return t("titles.smartFilter.sortedBy", { name: field })
end function

' A show's seasons as poster tiles: "Season 1", and its episodes.
function pageSeasonTiles(children as dynamic, server as string) as object
    tiles = []
    for each item in httpArray(children)
        if GetInterface(item, "ifAssociativeArray") <> invalid and httpString(item.kind) = "season"
            tile = contentTile(item, server, false)
            seasons = contentSeasons([item])
            tile.title = seasons[0].title
            episodes = seasons[0].episodes
            tile.subtitle = ""
            if episodes > 0 then tile.subtitle = t("titles.counts.episodes", { count: Int(episodes) })
            tiles.Push(tile)
        end if
    end for
    return tiles
end function

' --- People ---

' A person's page (GET /api/people/{id}): { name, photo, initials, line,
' rows }. line says what of theirs is in the libraries ("3 movies · 2 shows
' in your libraries"; "" for nothing). rows ([{ key, title, wide, tiles }])
' has a row per role, acting first, then directing, creating, writing and
' producing, each of their titles in it newest first, as the server sends
' them, captioned with its year and the part ("2019 · as Neo").
function pagePerson(person as object, server as string) as object
    credits = []
    seen = {}
    movies = 0
    shows = 0
    for each credit in httpArray(person.credits)
        item = invalid
        if GetInterface(credit, "ifAssociativeArray") <> invalid then item = httpObject(credit.item)
        kind = ""
        if item <> invalid then kind = httpString(item.kind)
        key = ""
        if item <> invalid then key = urlValue(httpNumber(item.id, 0))
        if (kind = "movie" or kind = "show") and not seen.DoesExist(key)
            seen[key] = true
            if kind = "movie" then movies = movies + 1
            if kind = "show" then shows = shows + 1
            credits.Push({ item: item, roles: httpArray(credit.roles) })
        end if
    end for
    counts = []
    if movies > 0 then counts.Push(fmtCount(movies, "movies"))
    if shows > 0 then counts.Push(fmtCount(shows, "shows"))
    line = ""
    if counts.Count() > 0 then line = t("titles.person.inLibraries", { counts: counts.Join(" · ") })
    rows = []
    for each row in [["actor", "acting"], ["director", "directing"], ["creator", "creating"], ["writer", "writing"], ["producer", "producing"]]
        tiles = []
        for each credit in credits
            roles = []
            for each role in credit.roles
                if GetInterface(role, "ifAssociativeArray") <> invalid and httpString(role.role) = row[0] then roles.Push(role)
            end for
            if roles.Count() > 0
                tile = contentTile(credit.item, server, false)
                parts = []
                if tile.subtitle <> "" then parts.Push(tile.subtitle)
                parts.Push(pageRoleText(roles))
                tile.subtitle = parts.Join(" · ")
                tiles.Push(tile)
            end if
        end for
        if tiles.Count() > 0 then rows.Push({ key: row[0], title: t("titles.person.rows." + row[1]), wide: false, tiles: tiles })
    end for
    name = httpSentence(person.name)
    return { name: name, photo: urlImage(server, person.photo_url, pagePhotoWidth()), initials: pageInitials(name), line: line, rows: rows }
end function

' The size of a person's photo on their page, round.
function pagePhotoWidth() as integer
    return 160
end function

' What someone did in a title, as the other apps say it: "as Neo",
' "Director", "Guest star (3 episodes)"; several the same way, with
' commas.
function pageRoleText(roles as object) as string
    parts = []
    for each role in roles
        kind = httpString(role.role)
        episodes = Int(httpNumber(role.episodes, 0))
        text = ""
        if kind = "actor"
            character = httpSentence(role.character)
            if character <> ""
                text = t("titles.roles.as", { character: character })
            else if episodes > 0
                text = t("titles.roles.guestStar")
            else
                text = t("titles.roles.actor")
            end if
        else
            text = pageRoleLabel(kind)
            if text = "" then text = t("titles.roles.crew")
        end if
        if episodes > 0 then text = t("titles.roles.withEpisodes", { role: text, count: episodes })
        parts.Push(text)
    end for
    return parts.Join(", ")
end function

' "Idris Vale" is "IV": what a person's photo shows without a picture.
function pageInitials(name as string) as string
    out = ""
    for each word in name.Tokenize(" ")
        if Len(out) < 2 then out = out + UCase(Left(word, 1))
    end for
    return out
end function

' What the Options key offers on a tile: [{ label, action }]. inList: the
' title is in My List (true or false), or invalid when not known; a
' Continue Watching tile can be taken out of it. Kinds without actions
' get none.
function pageTileOptions(tile as object, continueWatching as boolean, inList as dynamic) as object
    kind = httpString(tile.kind)
    options = []
    ' Music has no watched marks or My List.
    if contentMusic(kind) then return [{ label: t("titles.actions.addToPlaylist"), action: "playlist" }]
    if kind <> "movie" and kind <> "show" and kind <> "season" and kind <> "episode" then return options
    if httpTrue(tile.watched)
        options.Push({ label: t("titles.actions.markUnwatched"), action: "unwatched" })
    else
        options.Push({ label: t("titles.actions.markWatched"), action: "watched" })
    end if
    if (kind = "movie" or kind = "show") and GetInterface(inList, "ifBoolean") <> invalid
        if inList
            options.Push({ label: t("titles.actions.removeFromMyList"), action: "listRemove" })
        else
            options.Push({ label: t("titles.actions.addToMyList"), action: "listAdd" })
        end if
    end if
    options.Push({ label: t("titles.actions.addToPlaylist"), action: "playlist" })
    if continueWatching then options.Push({ label: t("titles.actions.removeFromContinueWatching"), action: "hide" })
    if kind = "episode" and httpNumber(tile.grandparentId, 0) > 0
        options.Push({ label: t("titles.actions.goToShow"), action: "show" })
        if httpNumber(tile.parentId, 0) > 0 then options.Push({ label: t("titles.actions.goToSeason"), action: "season" })
    end if
    return options
end function

' --- Playlists ---

' "12 titles · 3 h 20 min", "Smart · 40 titles": what a playlist holds.
function pagePlaylistLine(playlist as object) as string
    parts = []
    if httpString(playlist.smart_filter) <> "" then parts.Push(t("titles.smart"))
    count = Int(httpNumber(playlist.item_count, 0))
    if httpString(playlist.kind) = "audio"
        parts.Push(t("titles.counts.tracks", { count: count }))
    else
        parts.Push(fmtCount(count, ""))
    end if
    duration = fmtDuration(playlist.duration_ms)
    if duration <> "" then parts.Push(duration)
    return parts.Join(" · ")
end function

' A playlist as a tile: its first entry's picture, its name and what it
' holds.
function pagePlaylistTile(playlist as object, server as string) as object
    image = ""
    artwork = httpArray(playlist.artwork)
    if artwork.Count() > 0 then image = urlImage(server, artwork[0], contentPosterWidth())
    return { title: httpSentence(playlist.title), subtitle: pagePlaylistLine(playlist), image: image, wide: false, progress: 0.0, watched: false, itemId: httpNumber(playlist.id, 0), kind: "playlist" }
end function

function pagePlaylistTiles(playlists as dynamic, server as string) as object
    tiles = []
    for each playlist in httpArray(playlists)
        if GetInterface(playlist, "ifAssociativeArray") <> invalid and httpNumber(playlist.id, 0) > 0 then tiles.Push(pagePlaylistTile(playlist, server))
    end for
    return tiles
end function

' The playlists a title can be added to: the profile's own of its kind
' ("video" or "audio"), smart ones left out: [{ id, title }].
function pagePlaylistChoices(playlists as dynamic, kind as string) as object
    out = []
    for each playlist in httpArray(playlists)
        if GetInterface(playlist, "ifAssociativeArray") <> invalid and httpString(playlist.kind) = kind and httpString(playlist.smart_filter) = ""
            out.Push({ id: httpNumber(playlist.id, 0), title: httpSentence(playlist.title) })
        end if
    end for
    return out
end function

' The kind of playlist a title goes in: "audio" for music, else "video".
function pagePlaylistKind(kind as string) as string
    if kind = "artist" or kind = "album" or kind = "track" then return "audio"
    return "video"
end function

' A playlist's entries as rows (EpisodeRow): a movie's title and year, an
' episode's show and number, with entryId, the entry's own id.
function pagePlaylistEntries(entries as dynamic, server as string) as object
    rows = []
    watched = t("titles.watched")
    for each entry in httpArray(entries)
        item = invalid
        if GetInterface(entry, "ifAssociativeArray") <> invalid then item = httpObject(entry.item)
        if item <> invalid
            tile = contentTile(item, server, true)
            parts = []
            if tile.subtitle <> "" then parts.Push(tile.subtitle)
            duration = fmtDuration(tile.durationMs)
            if tile.progress > 0
                parts.Push(fmtLeft(tile.positionMs, tile.durationMs))
            else if duration <> ""
                parts.Push(duration)
            end if
            if tile.watched then parts.Push(watched)
            tile.subtitle = parts.Join(" · ")
            tile.summary = ""
            tile.entryId = httpNumber(entry.entry_id, 0)
            rows.Push(tile)
        end if
    end for
    return rows
end function

' What the player plays one after the other from a playlist: [{ itemId,
' title, line, image, startMs }], the Up Next cards of its entries.
function pagePlaylistQueue(entries as dynamic, server as string) as object
    queue = []
    for each entry in httpArray(entries)
        item = invalid
        if GetInterface(entry, "ifAssociativeArray") <> invalid then item = httpObject(entry.item)
        if item <> invalid and contentPlayable(httpString(item.kind)) then queue.Push(contentUpNext(item, server))
    end for
    return queue
end function

' A playlist's queue (one per entry, in the order of the rows before)
' put in the order of the rows after, matched by their entryId: what Play
' plays from an entry once the entries moved. A queue that doesn't match
' the rows stays as it is, until the playlist comes again.
function pagePlaylistReordered(queue as object, before as object, after as object) as object
    if queue.Count() <> before.Count() or after.Count() <> before.Count() then return queue
    places = {}
    for i = 0 to before.Count() - 1
        places[urlValue(before[i].entryId)] = i
    end for
    out = []
    for each row in after
        was = places[urlValue(row.entryId)]
        if was = invalid then return queue
        out.Push(queue[was])
    end for
    return out
end function

' Why a playlist didn't change, in a sentence: a sign-in from before
' Pley for Roku asked to change playlists can't (403), until signed in
' again.
function pagePlaylistRefusal(answer as object) as string
    if answer.status = 403 then return t("titles.playlist.refused")
    return answer.message
end function

' --- Watch history ---

' "now", "12 min ago", "3 hours ago", "yesterday", "5 days ago", else the
' day ("Mar 4, 2026"), as Format.brs writes them; the day is UTC's.
' seconds and now are unix seconds.
function pageRelativeTime(seconds as dynamic, now as integer) as string
    return fmtAgo(seconds, now, 0)
end function

' The plays of the watch history as rows (EpisodeRow): an episode's show,
' then its number and title, when and where it played; historyId, the
' entry's own id.
function pageHistoryRows(entries as dynamic, server as string, now as integer) as object
    rows = []
    for each entry in httpArray(entries)
        item = invalid
        if GetInterface(entry, "ifAssociativeArray") <> invalid then item = httpObject(entry.item)
        if item <> invalid and contentShown(httpString(item.kind))
            tile = contentTile(item, server, true)
            parts = []
            if tile.subtitle <> "" then parts.Push(tile.subtitle)
            if httpString(item.kind) = "track"
                album = httpSentence(item.parent_title)
                if album <> "" then parts.Push(album)
            end if
            parts.Push(pageRelativeTime(entry.viewed_at, now))
            device = httpSentence(entry.device)
            if device <> "" then parts.Push(device)
            tile.subtitle = parts.Join(" · ")
            tile.summary = ""
            tile.historyId = httpNumber(entry.id, 0)
            rows.Push(tile)
        end if
    end for
    return rows
end function

' queue from its entry at, in order or shuffled (the entry at first).
function pageQueueFrom(queue as object, at as integer, shuffled as boolean) as object
    out = []
    for i = at to queue.Count() - 1
        out.Push(queue[i])
    end for
    if shuffled
        for i = 0 to at - 1
            out.Push(queue[i])
        end for
        for i = out.Count() - 1 to 2 step -1
            j = Rnd(i)
            other = out[i]
            out[i] = out[j]
            out[j] = other
        end for
    end if
    return out
end function

' The request an action of the Options key or a title's page makes:
' { method, path }, or invalid for one that only goes somewhere.
function pageActionRequest(action as string, itemId as dynamic) as dynamic
    id = urlValue(itemId)
    if action = "watched" then return { method: "POST", path: "/api/items/" + id + "/watched" }
    if action = "unwatched" then return { method: "POST", path: "/api/items/" + id + "/unwatched" }
    if action = "listAdd" then return { method: "PUT", path: "/api/watchlist/" + id }
    if action = "listRemove" then return { method: "DELETE", path: "/api/watchlist/" + id }
    if action = "hide" then return { method: "DELETE", path: "/api/continue-watching/" + id }
    return invalid
end function

' My List after an action: ids (numbers) with itemId added or taken out.
function pageListWith(ids as dynamic, itemId as dynamic, inList as boolean) as object
    out = []
    id = httpNumber(itemId, 0)
    if inList then out.Push(id)
    for each other in httpArray(ids)
        if other <> id then out.Push(other)
    end for
    return out
end function

' Whether ids (My List as Home last saw it) holds itemId; invalid when Home
' hasn't said.
function pageInList(ids as dynamic, itemId as dynamic) as dynamic
    if GetInterface(ids, "ifArray") = invalid then return invalid
    id = httpNumber(itemId, 0)
    for each other in ids
        if other = id then return true
    end for
    return false
end function
