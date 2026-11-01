' Music: an album's or a playlist's tracks as the music player and its
' lists know them, and how Roku's Audio node plays one (docs/API.md,
' "Playback"): a session for each track, the file as it is when the Roku
' decodes it, else a conversion.

' A track: { itemId, title, artist, album, image, durationMs, index }.
function musicTrack(item as object, server as string) as object
    image = urlImage(server, item.poster_url, 600)
    if image = "" then image = urlImage(server, "/api/items/" + urlValue(item.id) + "/images/poster", 600)
    return {
        itemId: httpNumber(item.id, 0),
        title: httpSentence(item.title),
        artist: httpSentence(item.grandparent_title),
        album: httpSentence(item.parent_title),
        image: image,
        durationMs: Int(httpNumber(item.duration_ms, 0)),
        index: Int(httpNumber(item.index, -1))
    }
end function

' The tracks among items: ItemSummary[] (an album's children) or a
' playlist's entries ({ entry_id, item }).
function musicTracks(items as dynamic, server as string) as object
    tracks = []
    for each value in httpArray(items)
        item = httpObject(value)
        if item <> invalid and item.DoesExist("entry_id") then item = httpObject(item.item)
        if item <> invalid and httpString(item.kind) = "track" then tracks.Push(musicTrack(item, server))
    end for
    return tracks
end function

' The tracks as rows of a list (TrackRow): "3. Song" (numbered by the
' album, or the artist for a playlist), and how long it is.
function musicTrackRows(tracks as object, byAlbum as boolean) as object
    rows = []
    for each track in tracks
        title = track.title
        if byAlbum and track.index > 0 then title = urlValue(track.index) + ". " + title
        subtitle = ""
        if not byAlbum then subtitle = track.artist
        rows.Push({ title: title, subtitle: subtitle, duration: fmtClock(track.durationMs), itemId: track.itemId, kind: "track" })
    end for
    return rows
end function

' "Artist · 2019 · 12 tracks · 48 min": an album's line.
function musicAlbumLine(album as object, tracks as object) as string
    parts = []
    artist = httpSentence(album.parent_title)
    if artist <> "" then parts.Push(artist)
    year = httpNumber(album.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    if tracks.Count() = 1
        parts.Push("1 track")
    else if tracks.Count() > 1
        parts.Push(urlValue(tracks.Count()) + " tracks")
    end if
    total = 0
    for each track in tracks
        total = total + track.durationMs
    end for
    duration = fmtDuration(total)
    if duration <> "" then parts.Push(duration)
    return parts.Join(" · ")
end function

' An artist's albums (GET /api/items/{id}/children): { tiles, order }.
' tiles are the albums newest first (then by title), each captioned with
' its year; order has their ids as the server sent them, the order Play
' plays them in.
function musicArtistAlbums(items as dynamic, server as string) as object
    sorted = []
    order = []
    for each item in httpArray(items)
        if GetInterface(item, "ifAssociativeArray") <> invalid and httpString(item.kind) = "album"
            order.Push(httpNumber(item.id, 0))
            ' Placed after those it doesn't come before: albums are few,
            ' and the same year and title keep the server's order.
            at = sorted.Count()
            while at > 0 and musicNewer(item, sorted[at - 1])
                at = at - 1
            end while
            placed = []
            for i = 0 to sorted.Count() - 1
                if i = at then placed.Push(item)
                placed.Push(sorted[i])
            end for
            if at = sorted.Count() then placed.Push(item)
            sorted = placed
        end if
    end for
    tiles = []
    for each album in sorted
        tile = contentTile(album, server, false)
        tile.subtitle = ""
        year = httpNumber(album.year, 0)
        if year > 0 then tile.subtitle = urlValue(year)
        tiles.Push(tile)
    end for
    return { tiles: tiles, order: order }
end function

' Whether album a comes before album b on their artist's page: a later
' year first, then by title.
function musicNewer(a as object, b as object) as boolean
    yearA = httpNumber(a.year, 0)
    yearB = httpNumber(b.year, 0)
    if yearA <> yearB then return yearA > yearB
    return LCase(musicSortTitle(a)) < LCase(musicSortTitle(b))
end function

function musicSortTitle(item as object) as string
    title = httpString(item.sort_title)
    if title = "" then title = httpString(item.title)
    return title
end function

' "2 albums · 24 tracks · Rock · Pop": an artist's line, their genres three
' at most. albums: how many came, for a server that doesn't count them.
function musicArtistLine(artist as object, albums as integer) as string
    parts = []
    count = Int(httpNumber(artist.child_count, albums))
    if count = 1
        parts.Push("1 album")
    else if count > 1
        parts.Push(urlValue(count) + " albums")
    end if
    tracks = Int(httpNumber(artist.leaf_count, 0))
    if tracks = 1
        parts.Push("1 track")
    else if tracks > 1
        parts.Push(urlValue(tracks) + " tracks")
    end if
    genres = 0
    for each genre in httpArray(artist.genres)
        name = httpSentence(genre)
        if name <> "" and genres < 3
            parts.Push(name)
            genres = genres + 1
        end if
    end for
    return parts.Join(" · ")
end function

' The Audio node's streamFormat for a track's PlaybackInfo: HLS for a
' conversion, else the file's container (the item's details say it).
function musicStreamFormat(info as object, detail as dynamic) as string
    if httpTrue(info.hls) then return "hls"
    container = ""
    if detail <> invalid
        file = contentFile(detail, info.file_id)
        if file <> invalid then container = LCase(httpString(file.container))
    end if
    formats = { mp3: "mp3", flac: "flac", m4a: "mp4", mp4: "mp4", mov: "mp4", aac: "es.aac-adts", wav: "wav", mka: "mka", wma: "wma" }
    if formats.DoesExist(container) then return formats[container]
    return "mp3"
end function

' Where "previous" goes: the track's start once it has played 3 s, else
' the track before (the first's start for the first).
function musicPreviousAt(at as integer, positionMs as integer) as integer
    if positionMs > 3000 or at = 0 then return at
    return at - 1
end function
