' The server's JSON as the screens show it: plain fields, which become
' ContentNodes (lib/Nodes.brs): movies, TV shows and music.

function contentShown(kind as string) as boolean
    return kind = "movie" or kind = "show" or kind = "season" or kind = "episode" or contentMusic(kind)
end function

' Artists, albums and tracks: square pictures, played by the music player.
function contentMusic(kind as string) as boolean
    return kind = "artist" or kind = "album" or kind = "track"
end function

' Movies, episodes and extras (trailers and the like) play in the player.
function contentPlayable(kind as string) as boolean
    return kind = "movie" or kind = "episode" or kind = "extra"
end function

' Picture widths asked of the server, those of the cells that show them
' (the server resizes to buckets): small pictures load fast and keep a
' Roku Express's texture memory free.
function contentPosterWidth() as integer
    return 220
end function

function contentWideWidth() as integer
    return 400
end function

function contentEpisodeWidth() as integer
    return 300
end function

' Square tiles: an album's cover, an artist's picture.
function contentSquareWidth() as integer
    return 260
end function

' Backdrops are drawn faint (15 % and 40 %) behind text: a third of the
' screen's width is plenty.
function contentBackdropWidth() as integer
    return 640
end function

' The poster beside a title's details.
function contentDetailPosterWidth() as integer
    return 420
end function

' One tile of a row or a grid. wide: a 16:9 picture (Continue Watching,
' episodes) instead of a poster.
function contentTile(item as object, server as string, wide as boolean) as object
    kind = httpString(item.kind)
    title = httpSentence(item.title)
    subtitle = ""
    if kind = "movie" or kind = "show"
        year = httpNumber(item.year, 0)
        if year > 0 then subtitle = Int(year).ToStr()
    else if kind = "season"
        subtitle = httpSentence(item.parent_title)
    else if kind = "artist"
        albums = httpNumber(item.child_count, 0)
        if albums = 1
            subtitle = "1 album"
        else if albums > 1
            subtitle = urlValue(albums) + " albums"
        end if
    else if kind = "album"
        subtitle = httpSentence(item.parent_title)
        year = httpNumber(item.year, 0)
        if subtitle = "" and year > 0 then subtitle = urlValue(year)
    else if kind = "track"
        subtitle = httpSentence(item.grandparent_title)
    else if kind = "episode"
        showTitle = httpSentence(item.grandparent_title)
        code = fmtEpisode(item.parent_index, item.index)
        parts = []
        if code <> "" then parts.Push(code)
        if title <> "" then parts.Push(title)
        if showTitle <> ""
            subtitle = parts.Join(" · ")
            title = showTitle
        else
            subtitle = code
        end if
    end if
    state = httpObject(item.user_state)
    position = 0
    watched = false
    if state <> invalid
        position = httpNumber(state.position_ms, 0)
        watched = httpTrue(state.watched)
    end if
    duration = httpNumber(item.duration_ms, 0)
    progress = 0.0
    if not watched then progress = fmtProgress(position, duration)
    return {
        title: title,
        subtitle: subtitle,
        image: contentTileImage(item, server, wide, 0),
        wide: wide,
        square: contentMusic(kind) and not wide,
        progress: progress,
        watched: watched,
        itemId: httpNumber(item.id, 0),
        kind: kind,
        parentId: httpNumber(item.parent_id, 0),
        grandparentId: httpNumber(item.grandparent_id, 0),
        positionMs: position,
        durationMs: duration
    }
end function

' The picture of a tile, width pixels wide (0: the tile's own size): for
' wide tiles an episode's still or a backdrop, else the poster. Seasons
' and episodes without a poster of their own ask for theirs anyway: the
' server answers with the show's.
function contentTileImage(item as object, server as string, wide as boolean, width as integer) as string
    kind = httpString(item.kind)
    if wide
        if width = 0 then width = contentWideWidth()
        for each key in ["thumb_url", "backdrop_url", "poster_url"]
            url = urlImage(server, item[key], width)
            if url <> "" then return url
        end for
        return ""
    end if
    if width = 0
        width = contentPosterWidth()
        if contentMusic(kind) then width = contentSquareWidth()
    end if
    url = urlImage(server, item.poster_url, width)
    if url = "" and (kind = "season" or kind = "episode" or kind = "album" or kind = "track")
        url = urlImage(server, "/api/items/" + urlValue(item.id) + "/images/poster", width)
    end if
    return url
end function

' The libraries, in the profile's order: [{ id, name, kind, itemCount }].
' (A key named count would hide the array's Count().)
function contentLibraries(libraries as dynamic) as object
    out = []
    for each library in httpArray(libraries)
        kind = httpString(library.kind)
        if kind = "movies" or kind = "shows" or kind = "music"
            out.Push({ id: httpNumber(library.id, 0), name: httpSentence(library.name), kind: kind, itemCount: httpNumber(library.item_count, 0) })
        end if
    end for
    return out
end function

' Home's rows from GET /api/hubs/home (a library's Recommended rows too):
' [{ key, title, wide, square, tiles }]. Titles of kinds this version
' doesn't show are left out, and rows left empty with them. A library's
' newest, newly released and top rated end with See all.
function contentHomeRows(hubs as dynamic, server as string) as object
    rows = []
    for each hub in httpArray(hubs)
        key = httpString(hub.key)
        wide = key = "continue_watching"
        square = contentRowSquare(hub)
        items = []
        for each item in httpArray(hub.items)
            if GetInterface(item, "ifAssociativeArray") <> invalid and contentShown(httpString(item.kind)) then items.Push(contentTile(item, server, wide))
        end for
        sort = contentSeeAllSort(key)
        libraryId = httpNumber(hub.library_id, 0)
        if items.Count() > 0 and sort <> "" and libraryId > 0 then items.Push(contentSeeAllTile(libraryId, sort, wide, square))
        if items.Count() > 0 then rows.Push({ key: key, title: httpSentence(hub.title), wide: wide, square: square, tiles: items })
    end for
    return rows
end function

' The Library tab's sort a row's See all shows its library in, as the row
' lists it (newest or best first, the web's and Android's): "" for rows
' without one.
function contentSeeAllSort(key as string) as string
    if Left(key, 14) = "recently_added" then return "added"
    if Left(key, 17) = "recently_released" then return "release"
    if Left(key, 9) = "top_rated" then return "rating"
    return ""
end function

' See all at a row's end, in the row's shape: its library's Library tab in
' sort's order.
function contentSeeAllTile(libraryId as dynamic, sort as string, wide as boolean, square as boolean) as object
    return { title: "See all", subtitle: "", image: "", wide: wide, square: square, progress: 0.0, watched: false, itemId: 0, kind: "more", list: "", query: "", libraryId: libraryId, sort: sort }
end function

' Whether a hub's row is of square tiles: a music library's, by its first
' title.
function contentRowSquare(hub as object) as boolean
    if httpString(hub.key) = "continue_watching" then return false
    items = httpArray(hub.items)
    if items.Count() = 0 or GetInterface(items[0], "ifAssociativeArray") = invalid then return false
    return contentMusic(httpString(items[0].kind))
end function

' The size of a row's tiles, and the row's height, for its shape: a 16:9
' picture, a square or a poster, and two lines of text under it.
function contentRowSize(wide as boolean, square as boolean) as object
    if wide then return { size: [400, 300], height: 360 }
    if square then return { size: [contentSquareWidth(), 340], height: 400 }
    return { size: [220, 410], height: 470 }
end function

' A short fingerprint of a hub as the server sent it.
function contentSignature(value as object) as string
    bytes = CreateObject("roByteArray")
    bytes.FromAsciiString(FormatJson(value))
    return bytes.GetCRC32().ToStr() + ":" + bytes.Count().ToStr()
end function

' Whether Home asks for its rows again as it shows: after something played
' here, or when they are a minute old, for what the person's other devices
' played or marked meanwhile. Roku's HTTP can't read the server's event
' stream as it comes; rows that didn't change cost
' the server's answer only.
function contentHomeStale(played as integer, playedSeen as integer, seconds as integer) as boolean
    return played <> playedSeen or seconds >= 60
end function

' Home's rows from GET /api/hubs/home (as contentHomeRows), made again only
' where they changed: known holds the fingerprint of each row Home shows
' ({ key: signature }), taken from the server's JSON, so a hub sent as
' before isn't even read. { summary: [{ key, wide, square, sig, built }]
' for every row in order, changed: the rows to build, in order }.
function contentRowChanges(hubs as dynamic, server as string, known as dynamic) as object
    shown = httpObject(known)
    summary = []
    changed = []
    for each hub in httpArray(hubs)
        if GetInterface(hub, "ifAssociativeArray") <> invalid
            key = httpString(hub.key)
            sig = contentSignature(hub)
            if shown <> invalid and shown.DoesExist(key) and shown[key] = sig
                summary.Push({ key: key, wide: key = "continue_watching", square: contentRowSquare(hub), sig: sig, built: false })
            else
                for each row in contentHomeRows([hub], server)
                    summary.Push({ key: row.key, wide: row.wide, square: row.square, sig: sig, built: true })
                    changed.Push(row)
                end for
            end if
        end if
    end for
    return { summary: summary, changed: changed }
end function

' Places in a grid for titles that haven't come yet: empty frames.
function contentPlaceholders(count as integer) as object
    out = []
    for i = 1 to count
        out.Push({ title: "", subtitle: "", image: "", kind: "skeleton" })
    end for
    return out
end function

' Tiles of the titles in a listing (a library page, search) this version
' shows.
function contentTiles(items as dynamic, server as string) as object
    tiles = []
    for each item in httpArray(items)
        if GetInterface(item, "ifAssociativeArray") <> invalid and contentShown(httpString(item.kind)) then tiles.Push(contentTile(item, server, false))
    end for
    return tiles
end function

' A show's seasons: [{ itemId, title, index, episodes, watchedEpisodes }].
function contentSeasons(children as dynamic) as object
    seasons = []
    for each item in httpArray(children)
        if GetInterface(item, "ifAssociativeArray") <> invalid and httpString(item.kind) = "season"
            title = httpSentence(item.title)
            index = httpNumber(item.index, -1)
            if title = ""
                if index = 0
                    title = "Specials"
                else
                    title = "Season " + urlValue(index)
                end if
            end if
            seasons.Push({ itemId: httpNumber(item.id, 0), title: title, index: index, episodes: httpNumber(item.leaf_count, 0), watchedEpisodes: httpNumber(item.viewed_leaf_count, 0) })
        end if
    end for
    return seasons
end function

' A season's episodes, as rows: { itemId, kind, title ("1. Pilot"),
' subtitle, summary, image, progress, watched, positionMs, durationMs,
' parentId, grandparentId }.
function contentEpisodes(children as dynamic, server as string) as object
    episodes = []
    for each item in httpArray(children)
        if GetInterface(item, "ifAssociativeArray") <> invalid and httpString(item.kind) = "episode"
            tile = contentTile(item, server, true)
            tile.image = contentTileImage(item, server, true, contentEpisodeWidth())
            title = httpSentence(item.title)
            index = httpNumber(item.index, -1)
            if index >= 0 then title = urlValue(index) + ". " + title
            tile.title = title
            parts = []
            duration = fmtDuration(tile.durationMs)
            if tile.progress > 0
                parts.Push(fmtLeft(tile.positionMs, tile.durationMs))
            else if duration <> ""
                parts.Push(duration)
            end if
            if tile.watched then parts.Push("Watched")
            tile.subtitle = parts.Join(" · ")
            tile.summary = httpClean(item.summary, 300, false)
            episodes.Push(tile)
        end if
    end for
    return episodes
end function

' A title's details screen (a movie, an episode, a show or an extra): what
' it shows and which buttons it has. actions match buttons: "resume",
' "start", "playNext" (a show's next episode, onDeck), "episodes",
' "watched", "unwatched", "tracks" (Audio and subtitles), "listAdd",
' "listRemove" and "more". More holds what is used less, as on the other
' TVs ([{ label, action }]): "rate", "show" (an episode's) and "playlist".
' onDeck is a show's episode to play next (ItemSummary), invalid until the
' server said or when there is none.
function contentDetail(detail as object, server as string, onDeck = invalid as dynamic) as object
    kind = httpString(detail.kind)
    title = httpSentence(detail.title)
    heading = title
    line = fmtMeta(detail)
    if kind = "show"
        parts = []
        year = httpNumber(detail.year, 0)
        if year > 0 then parts.Push(urlValue(year))
        seasons = httpNumber(detail.child_count, 0)
        if seasons = 1
            parts.Push("1 season")
        else if seasons > 1
            parts.Push(urlValue(seasons) + " seasons")
        end if
        rating = httpSentence(detail.content_rating)
        if rating <> "" then parts.Push(rating)
        score = httpNumber(detail.rating, 0)
        if score > 0 then parts.Push("★ " + fmtOneDecimal(score))
        line = parts.Join(" · ")
    else if kind = "episode"
        showTitle = httpSentence(detail.grandparent_title)
        code = fmtEpisode(detail.parent_index, detail.index)
        if showTitle <> "" then heading = showTitle
        parts = []
        if code <> "" then parts.Push(code)
        if showTitle <> "" then parts.Push(title)
        meta = fmtMeta(detail)
        if meta <> "" then parts.Push(meta)
        line = parts.Join(" · ")
    end if
    tagline = httpSentence(detail.tagline)
    state = httpObject(detail.user_state)
    position = 0
    if state <> invalid then position = httpNumber(state.position_ms, 0)
    duration = httpNumber(detail.duration_ms, 0)
    watched = false
    if state <> invalid then watched = httpTrue(state.watched)
    inList = httpTrue(detail.in_watchlist)
    files = httpArray(detail.files)
    buttons = []
    actions = []
    note = ""
    if kind = "show"
        upcoming = httpObject(onDeck)
        if upcoming <> invalid
            verb = "Play "
            upcomingState = httpObject(upcoming.user_state)
            if upcomingState <> invalid and fmtResumable(upcomingState.position_ms, upcoming.duration_ms) then verb = "Resume "
            buttons.Push(verb + fmtEpisode(upcoming.parent_index, upcoming.index))
            actions.Push("playNext")
        end if
        buttons.Push("Episodes")
        actions.Push("episodes")
    else if not contentPlayable(kind)
        note = "Pley for Roku doesn't play this kind of title yet."
    else if files.Count() = 0
        note = "This item has no file to play."
    else if fmtResumable(position, duration)
        buttons = [fmtResume(position), "Play from start"]
        actions = ["resume", "start"]
    else
        buttons = ["Play"]
        actions = ["start"]
    end if
    if kind = "movie" or kind = "episode" or kind = "show"
        if watched
            buttons.Push("Mark as unwatched")
            actions.Push("unwatched")
        else
            buttons.Push("Mark as watched")
            actions.Push("watched")
        end if
    end if
    if (kind = "movie" or kind = "episode") and contentHasTracks(detail)
        buttons.Push("Audio and subtitles")
        actions.Push("tracks")
    end if
    if kind = "movie" or kind = "show"
        if inList
            buttons.Push("Remove from My List")
            actions.Push("listRemove")
        else
            buttons.Push("Add to My List")
            actions.Push("listAdd")
        end if
    end if
    stars = 0
    if state <> invalid then stars = contentStars(state.rating)
    more = []
    rateable = kind = "movie" or kind = "episode" or kind = "show"
    if rateable then more.Push({ label: contentRatingLabel(stars), action: "rate" })
    if kind = "episode" and httpNumber(detail.grandparent_id, 0) > 0 then more.Push({ label: "Go to show", action: "show" })
    if rateable then more.Push({ label: "Add to playlist", action: "playlist" })
    if more.Count() > 0
        buttons.Push("More…")
        actions.Push("more")
    end if
    backdrop = urlImage(server, detail.backdrop_url, contentBackdropWidth())
    if backdrop = "" then backdrop = urlImage(server, detail.thumb_url, contentBackdropWidth())
    genres = []
    for each genre in httpArray(detail.genres)
        name = httpSentence(genre)
        if name <> "" and genres.Count() < 3 then genres.Push(name)
    end for
    return {
        itemId: httpNumber(detail.id, 0),
        kind: kind,
        heading: heading,
        line: line,
        tagline: tagline,
        genres: genres.Join(", "),
        summary: httpClean(detail.summary, 1200, true),
        backdrop: backdrop,
        poster: contentTileImage(detail, server, false, contentDetailPosterWidth()),
        positionMs: position,
        durationMs: duration,
        watched: watched,
        inList: inList,
        stars: stars,
        showId: httpNumber(detail.grandparent_id, 0),
        seasonId: httpNumber(detail.parent_id, 0),
        buttons: buttons,
        actions: actions,
        more: more,
        note: note
    }
end function

' Whether a title's page offers Audio and subtitles before playing: its
' file has two audio tracks or more, or subtitles.
function contentHasTracks(detail as object) as boolean
    file = httpObject(contentFile(detail, invalid))
    if file = invalid then return false
    audio = 0
    for each stream in httpArray(file.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid
            kind = httpString(stream.kind)
            if kind = "subtitle" then return true
            if kind = "audio" then audio = audio + 1
        end if
    end for
    return audio > 1
end function

' The stars of the profile's own rating (user_state.rating, 0 to 10 in
' half stars), rounded up as the other apps show it: 0 for none.
function contentStars(rating as dynamic) as integer
    value = httpNumber(rating, 0)
    if value <= 0 then return 0
    stars = -Int(-value / 2)
    if stars > 5 then stars = 5
    return stars
end function

' "1 star", "4 stars"; "No rating" for none.
function contentStarsLabel(stars as integer) as string
    if stars = 0 then return "No rating"
    if stars = 1 then return "1 star"
    return urlValue(stars) + " stars"
end function

' A title page's rating button: Rate, or the stars given.
function contentRatingLabel(stars as integer) as string
    if stars = 0 then return "Rate"
    return "Your rating: " + contentStarsLabel(stars)
end function

' The file a playback plays: the one with fileId, else the first.
function contentFile(detail as object, fileId as dynamic) as dynamic
    files = httpArray(detail.files)
    wanted = httpNumber(fileId, -1)
    for each file in files
        if GetInterface(file, "ifAssociativeArray") <> invalid and httpNumber(file.id, -2) = wanted then return file
    end for
    if files.Count() > 0 then return files[0]
    return invalid
end function

' The Video node's streamFormat for a PlaybackInfo: HLS for remuxes and
' transcodes, else the original file's container.
function contentStreamFormat(info as object, detail as object) as string
    if httpTrue(info.hls) then return "hls"
    file = contentFile(detail, info.file_id)
    container = ""
    if file <> invalid then container = LCase(httpString(file.container))
    if container = "mkv" or container = "webm" then return "mkv"
    if container = "mp3" then return "mp3"
    return "mp4"
end function

' What the Video node opens for a PlaybackInfo: { url, streamFormat,
' title, playStart, subtitleTracks, subtitle, previews }. subtitle is the
' TrackName of the subtitles the person's choices pick ("" for none); the
' server sends text tracks as WebVTT. previews: the seek previews as a BIF
' file beside a keyed session's media, which Roku's trick play shows ("" for
' an address without a key).
function contentStream(info as object, detail as object, server as string, startMs as dynamic) as object
    url = httpString(info.url)
    if Left(url, 1) = "/" then url = server + url
    previews = ""
    keyed = CreateObject("roRegex", "^(.*/api/media/pley_mk_[A-Za-z0-9_-]+/)[^/]+$", "")
    match = keyed.Match(url)
    if match.Count() = 2 then previews = match[1] + "previews.bif"
    file = contentFile(detail, info.file_id)
    selected = -1
    if file <> invalid then selected = httpNumber(file.selected_subtitle, -1)
    tracks = []
    subtitle = ""
    for each track in httpArray(info.subtitles)
        trackUrl = httpString(track.url)
        if trackUrl <> ""
            if Left(trackUrl, 1) = "/" then trackUrl = server + trackUrl
            language = httpString(track.language)
            if language = "" then language = "und"
            tracks.Push({ Language: language, TrackName: trackUrl, Description: contentSubtitleName(track, language) })
            if httpNumber(track.index, -2) = selected then subtitle = trackUrl
        end if
    end for
    return {
        url: url,
        streamFormat: contentStreamFormat(info, detail),
        title: contentPlayerTitle(detail),
        playStart: Int(httpNumber(startMs, 0) / 1000),
        subtitleTracks: tracks,
        subtitle: subtitle,
        previews: previews
    }
end function

' What Roku's subtitle menu calls a track: its title, else its language's
' name (its code for one not among the 34); "Unknown" for neither, "Forced"
' for forced ones of no language. A title that says forced isn't said twice.
function contentSubtitleName(track as object, language as string) as string
    name = httpSentence(track.title)
    if name = "" and language <> "und" then name = fmtLanguageName(language, language)
    forced = httpTrue(track.forced)
    if name = ""
        if forced then return "Forced"
        return "Unknown"
    end if
    if forced and not contentSaysForced(name) then return name + " (Forced)"
    return name
end function

' Whether a track's name says it is forced ("Forced", "English (Forced)").
function contentSaysForced(name as string) as boolean
    re = CreateObject("roRegex", "(^|[^a-z0-9])forced($|[^a-z0-9])", "i")
    return re.IsMatch(name)
end function

' "Show · S1 E2 · Pilot" for an episode, the title for a movie.
function contentPlayerTitle(detail as object) as string
    title = httpSentence(detail.title)
    if httpString(detail.kind) <> "episode" then return title
    parts = []
    showTitle = httpSentence(detail.grandparent_title)
    if showTitle <> "" then parts.Push(showTitle)
    code = fmtEpisode(detail.parent_index, detail.index)
    if code <> "" then parts.Push(code)
    if title <> "" then parts.Push(title)
    return parts.Join(" · ")
end function

' Up Next's card for the next episode: { itemId, title, line, image,
' startMs }.
function contentUpNext(item as object, server as string) as object
    tile = contentTile(item, server, true)
    ' A movie (a playlist's next): its year.
    line = tile.subtitle
    if tile.kind = "episode"
        line = fmtEpisode(item.parent_index, item.index)
        title = httpSentence(item.title)
        if title <> ""
            if line <> "" then line = line + " · "
            line = line + title
        end if
    end if
    return { itemId: tile.itemId, title: tile.title, line: line, image: tile.image, startMs: playerStartOf(tile.positionMs, tile.durationMs) }
end function
