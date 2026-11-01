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
    labels = { creator: "Creator", director: "Director", writer: "Writer", producer: "Producer" }
    if labels.DoesExist(role) then return labels[role]
    return ""
end function

' Cast and crew of a title, as tiles: the actors in billing order with
' the part they played, then creators, directors, writers and producers,
' someone credited twice once with both ("Director, Writer"). At most 30.
function pageCredits(detail as object, server as string) as object
    tiles = []
    seen = {}
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
        role = pageRoleLabel(httpString(credit.role))
        if id > 0 and role <> ""
            if seen.DoesExist(key)
                tile = tiles[seen[key]]
                if tile.subtitle = ""
                    tile.subtitle = role
                else if Instr(1, tile.subtitle, role) = 0
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
    labels = { trailer: "Trailer", featurette: "Featurette", behind_the_scenes: "Behind the scenes", deleted_scene: "Deleted scene", interview: "Interview", scene: "Scene", short: "Short" }
    kind = httpString(extraType)
    if labels.DoesExist(kind) then return labels[kind]
    return "Extra"
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
    if credits.Count() > 0 then rows.Push({ key: "credits", title: "Cast & crew", wide: false, tiles: credits })
    extras = pageExtras(detail, server)
    if extras.Count() > 0 then rows.Push({ key: "extras", title: "Extras", wide: true, tiles: extras })
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
    if count = 1
        subtitle = "1 title"
    else if count >= 0
        subtitle = urlValue(count) + " titles"
    end if
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
    if key = "release" then return "By release date"
    if key = "title" then return "By title"
    if key = "custom" then return "Custom order"
    return ""
end function

' What a smart filter keeps, in words, as the other apps say it: "Genre:
' Comedy", "Unwatched", "From 2010", "Newest first". The filter is a
' listing's query string in the compatibility API's syntax
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
        if key = "genre"
            notes.Push("Genre: " + value)
        else if key = "label"
            notes.Push("Label: " + value)
        else if key = "studio"
            notes.Push("Studio: " + value)
        else if key = "contentRating"
            notes.Push("Rated " + value)
        else if key = "unwatched"
            if value = "0" or value = "false"
                notes.Push("Watched")
            else
                notes.Push("Unwatched")
            end if
        else if key = "inProgress"
            if value = "0" or value = "false"
                notes.Push("Not in progress")
            else
                notes.Push("In progress")
            end if
        else if key = "year"
            notes.Push("From " + value)
        else if key = "year>>"
            notes.Push("After " + value)
        else if key = "year<<"
            notes.Push("Before " + value)
        else if key = "decade"
            notes.Push("The " + value + "s")
        else if key = "sort"
            notes.Push(pageFilterOrder(value))
        else if key = "limit"
            notes.Push("At most " + value)
        else if key <> "type" and key <> "includeGuids" and key <> ""
            ' type and includeGuids are plumbing, not part of what is kept.
            notes.Push(key + ": " + value)
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
    notes = [
        ["titleSort", "By title", "By title, Z to A"],
        ["year", "Oldest first", "Newest first"],
        ["originallyAvailableAt", "Earliest release first", "Latest release first"],
        ["addedAt", "First added first", "Recently added first"],
        ["rating", "Lowest rated first", "Best rated first"],
        ["audienceRating", "Lowest rated first", "Best rated first"],
        ["userRating", "Your lowest rated first", "Your best rated first"],
        ["lastViewedAt", "Least recently played first", "Recently played first"],
        ["duration", "Shortest first", "Longest first"],
        ["viewCount", "Least played first", "Most played first"]
    ]
    for each note in notes
        if note[0] = field
            if direction = "desc" then return note[2]
            return note[1]
        end if
    end for
    return "Sorted by " + field
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
            if episodes = 1
                tile.subtitle = "1 episode"
            else if episodes > 1
                tile.subtitle = urlValue(episodes) + " episodes"
            end if
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
    if counts.Count() > 0 then line = counts.Join(" · ") + " in your libraries"
    rows = []
    for each row in [["actor", "Acting"], ["director", "Directing"], ["creator", "Creating"], ["writer", "Writing"], ["producer", "Producing"]]
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
        if tiles.Count() > 0 then rows.Push({ key: row[0], title: row[1], wide: false, tiles: tiles })
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
                text = "as " + character
            else if episodes > 0
                text = "Guest star"
            else
                text = "Actor"
            end if
        else
            text = pageRoleLabel(kind)
            if text = "" then text = "Crew"
        end if
        if episodes = 1
            text = text + " (1 episode)"
        else if episodes > 1
            text = text + " (" + urlValue(episodes) + " episodes)"
        end if
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
    if contentMusic(kind) then return [{ label: "Add to playlist", action: "playlist" }]
    if kind <> "movie" and kind <> "show" and kind <> "season" and kind <> "episode" then return options
    if httpTrue(tile.watched)
        options.Push({ label: "Mark as unwatched", action: "unwatched" })
    else
        options.Push({ label: "Mark as watched", action: "watched" })
    end if
    if (kind = "movie" or kind = "show") and GetInterface(inList, "ifBoolean") <> invalid
        if inList
            options.Push({ label: "Remove from My List", action: "listRemove" })
        else
            options.Push({ label: "Add to My List", action: "listAdd" })
        end if
    end if
    options.Push({ label: "Add to playlist", action: "playlist" })
    if continueWatching then options.Push({ label: "Remove from Continue Watching", action: "hide" })
    if kind = "episode" and httpNumber(tile.grandparentId, 0) > 0
        options.Push({ label: "Go to show", action: "show" })
        if httpNumber(tile.parentId, 0) > 0 then options.Push({ label: "Go to season", action: "season" })
    end if
    return options
end function

' --- Playlists ---

' "12 titles · 3 h 20 min", "Smart · 40 titles": what a playlist holds.
function pagePlaylistLine(playlist as object) as string
    parts = []
    if httpString(playlist.smart_filter) <> "" then parts.Push("Smart")
    count = httpNumber(playlist.item_count, 0)
    noun = " titles"
    if httpString(playlist.kind) = "audio" then noun = " tracks"
    if count = 1 then noun = Left(noun, Len(noun) - 1)
    parts.Push(urlValue(count) + noun)
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
            if tile.watched then parts.Push("Watched")
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

' Why a playlist didn't change, in a sentence: a sign-in from before
' pley for Roku asked to change playlists can't (403), until signed in
' again.
function pagePlaylistRefusal(answer as object) as string
    if answer.status = 403 then return "This sign-in can't change playlists: sign out in Settings and sign in again to allow it."
    return answer.message
end function

' --- Watch history ---

' "now", "12 min ago", "3 hours ago", "yesterday", "5 days ago", else the
' day ("Mar 4, 2026"). seconds and now are unix seconds.
function pageRelativeTime(seconds as dynamic, now as integer) as string
    at = httpNumber(seconds, 0)
    diff = now - at
    if diff < 0 then diff = 0
    if diff < 60 then return "now"
    if diff < 3600 then return urlValue(Int(diff / 60)) + " min ago"
    if diff < 86400
        hours = Int(diff / 3600)
        if hours = 1 then return "1 hour ago"
        return urlValue(hours) + " hours ago"
    end if
    days = Int(diff / 86400)
    if days = 1 then return "yesterday"
    if days < 30 then return urlValue(days) + " days ago"
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(at))
    months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    return months[date.GetMonth() - 1] + " " + urlValue(date.GetDayOfMonth()) + ", " + urlValue(date.GetYear())
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
