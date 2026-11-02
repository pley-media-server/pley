' Music: tracks as the music player knows them, and how Roku plays them.

function musicTestTrack() as object
    return ParseJson("{""id"":5201,""kind"":""track"",""title"":""Song 1"",""index"":1,""parent_id"":5100,""parent_title"":""Blue"",""grandparent_id"":5000,""grandparent_title"":""The Fixtures"",""duration_ms"":200000,""user_state"":{""position_ms"":0,""watched"":false,""play_count"":0}}")
end function

sub test_music_tracks_of_an_album_and_a_playlist()
    track = musicTrack(musicTestTrack(), "http://nas:7539")
    tkEqual(track, { itemId: 5201, title: "Song 1", artist: "The Fixtures", album: "Blue", image: "http://nas:7539/api/items/5201/images/poster?w=600", durationMs: 200000, index: 1, disc: 1 }, "the album's cover, through the track; disc 1 without parent_index")
    tkEqual(musicTracks([musicTestTrack(), { id: 9, kind: "album" }], "http://nas:7539").Count(), 1, "tracks only")
    tkEqual(musicTracks([{ entry_id: 3, item: musicTestTrack() }], "http://nas:7539")[0].itemId, 5201, "a playlist's entries")
    rows = musicTrackRows([track], true)
    tkEqual(rows[0].title, "1. Song 1")
    tkEqual(rows[0].duration, "3:20")
    tkEqual(musicTrackRows([track], false)[0].subtitle, "The Fixtures", "a playlist's: the artist")
    album = { title: "Blue", parent_title: "The Fixtures", year: 2001 }
    tkEqual(musicAlbumLine(album, [track, track, track]), "The Fixtures · 2001 · 3 tracks · 10 min")
end sub

sub test_music_album_tracks_by_disc()
    tracks = []
    for each place in [[1, 1], [1, 2], [2, 1], [2, 2], [2, 3]]
        item = musicTestTrack()
        item.parent_index = place[0]
        item.index = place[1]
        item.title = "Song " + urlValue(place[0]) + "-" + urlValue(place[1])
        tracks.Push(musicTrack(item, "http://nas:7539"))
    end for
    tkEqual(tracks[2].disc, 2, "the disc: the track's parent_index")
    rows = musicDiscRows(tracks)
    titles = []
    places = []
    for each row in rows
        titles.Push(row.title)
        places.Push(row.at)
    end for
    tkEqual(titles, ["Disc 1", "1. Song 1-1", "2. Song 1-2", "Disc 2", "1. Song 2-1", "2. Song 2-2", "3. Song 2-3"], "each disc under its heading, numbered on it")
    tkEqual(places, [-1, 0, 1, -1, 2, 3, 4], "a track's place among the album's; -1 for a heading")
    tkEqual(rows[0].kind, "disc")
    tkEqual(rows[1].kind, "track")

    ' One disc: no heading.
    one = musicDiscRows([tracks[2], tracks[3]])
    tkEqual(one.Count(), 2)
    tkEqual(one[0].title, "1. Song 2-1")
    tkEqual(one[1].at, 1)
    tkEqual(musicDiscRows([]).Count(), 0, "no tracks")
end sub

sub test_music_stream_formats()
    detail = { files: [{ id: 7, container: "flac" }] }
    tkEqual(musicStreamFormat({ hls: true, file_id: 7 }, detail), "hls")
    tkEqual(musicStreamFormat({ hls: false, file_id: 7 }, detail), "flac")
    detail.files[0].container = "m4a"
    tkEqual(musicStreamFormat({ hls: false, file_id: 7 }, detail), "mp4")
    detail.files[0].container = "aac"
    tkEqual(musicStreamFormat({ hls: false, file_id: 7 }, detail), "es.aac-adts")
    tkEqual(musicStreamFormat({ hls: false, file_id: 7 }, {}), "mp3", "details without a file")
end sub

sub test_music_previous_goes_back_or_to_the_start()
    tkEqual(musicPreviousAt(2, 5000), 2, "after 3 s: the track's start")
    tkEqual(musicPreviousAt(2, 1000), 1)
    tkEqual(musicPreviousAt(0, 1000), 0)
end sub

' Ten tracks, told apart by their itemId (0 to 9).
function musicTestTracks() as object
    tracks = []
    for i = 0 to 9
        tracks.Push({ itemId: i, title: "Song " + urlValue(i) })
    end for
    return tracks
end function

function musicTestIds(queue as object) as object
    ids = []
    for each track in musicOrder(queue)
        ids.Push(track.itemId)
    end for
    return ids
end function

sub test_music_queue_plays_in_the_order_given()
    queue = musicQueue(musicTestTracks(), 3, false)
    tkEqual(queue.at, 3)
    tkFalse(queue.shuffled)
    tkEqual(musicTrackAt(queue, queue.at).itemId, 3)
    tkEqual(musicTestIds(queue), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
    tkEqual(musicQueue(musicTestTracks(), 12, false).at, 0, "a place it hasn't: the first")
    empty = musicQueue([], 0, true)
    tkEqual(empty.order.Count(), 0)
    musicShuffle(empty, true)
    tkFalse(empty.shuffled, "nothing to shuffle")
end sub

sub test_music_shuffle_keeps_the_track_playing_first()
    queue = musicQueue(musicTestTracks(), 3, false)
    musicShuffle(queue, true)
    tkTrue(queue.shuffled)
    tkEqual(queue.at, 0, "the track playing comes first")
    tkEqual(musicTrackAt(queue, 0).itemId, 3)
    ids = musicTestIds(queue)
    ids.Sort()
    tkEqual(ids, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "every track once")
    ' Nine tracks shuffled five times: one of them is out of order.
    moved = false
    for attempt = 1 to 5
        again = musicQueue(musicTestTracks(), 0, true)
        if FormatJson(musicTestIds(again)) <> FormatJson([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]) then moved = true
    end for
    tkTrue(moved, "the others in a random order")

    ' Stop shuffling: the order given, at the track playing.
    queue.at = 4
    playing = musicTrackAt(queue, 4).itemId
    musicShuffle(queue, false)
    tkFalse(queue.shuffled)
    tkEqual(musicTestIds(queue), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
    tkEqual(queue.at, playing)
    tkEqual(musicTrackAt(queue, queue.at).itemId, playing)
end sub

sub test_music_queue_shuffled_from_the_start()
    queue = musicQueue(musicTestTracks(), 6, true)
    tkTrue(queue.shuffled)
    tkEqual(queue.at, 0)
    tkEqual(musicTrackAt(queue, 0).itemId, 6, "the track chosen first")
    queue = musicQueue([{ itemId: 7 }, { itemId: 7 }], 1, true)
    tkEqual(queue.order, [1, 0], "the same track twice: places tell them apart")
end sub

sub test_music_up_next_lists_the_queue_the_track_playing_marked()
    tracks = []
    for i = 1 to 4
        track = musicTrack(musicTestTrack(), "http://nas:7539")
        track.title = "Song " + urlValue(i)
        tracks.Push(track)
    end for
    tracks[3].album = ""
    rows = musicUpNextRows(tracks, 0, 4, 1)
    tkEqual(rows.Count(), 4, "what played before too")
    tkEqual(rows[2], { title: "3. Song 3", subtitle: "The Fixtures · Blue", duration: "3:20", current: false }, "numbered by its place in the queue")
    tkTrue(rows[1].current, "the track playing")
    tkEqual(rows[3].subtitle, "The Fixtures", "without an album")
    rows = musicUpNextRows(tracks, 2, 4, 3)
    tkEqual(rows[0].title, "3. Song 3", "a part of the queue keeps the numbers of its places")
    tkTrue(rows[1].current)
end sub

sub test_music_up_next_lists_a_long_queue_a_part_at_a_time()
    tkEqual(musicUpNextPart(0, 12, invalid), { first: 0, last: 12 }, "a short queue whole")
    tkEqual(musicUpNextPart(11, 12, { first: 0, last: 12 }), { first: 0, last: 12 }, "to its end")
    part = musicUpNextPart(0, 5000, invalid)
    tkEqual(part, { first: 0, last: 250 })
    tkEqual(musicUpNextPart(199, 5000, part), part, "moving on moves only the mark")
    tkEqual(musicUpNextPart(200, 5000, part), { first: 150, last: 400 }, "near its end, the next part, 50 before the track playing")
    tkEqual(musicUpNextPart(149, 5000, { first: 150, last: 400 }), { first: 99, last: 349 }, "back before it, the part around the track")
    tkEqual(musicUpNextPart(4990, 5000, invalid), { first: 4750, last: 5000 }, "at the queue's end, more of what played")
    tkEqual(musicUpNextPart(1, 2, { first: 0, last: 12 }), { first: 0, last: 2 }, "a part longer than the queue is listed again")
end sub

sub test_music_tiles_are_square()
    tile = contentTile({ id: 5100, kind: "album", title: "Blue", parent_title: "The Fixtures", year: 2001 }, "http://nas:7539", false)
    tkTrue(tile.square)
    tkEqual(tile.subtitle, "The Fixtures")
    tkEqual(tile.image, "http://nas:7539/api/items/5100/images/poster?w=260")
    artist = contentTile({ id: 5000, kind: "artist", title: "The Fixtures", child_count: 2, poster_url: "/api/items/5000/images/poster?v=1" }, "http://nas:7539", false)
    tkEqual(artist.subtitle, "2 albums")
    tkFalse(contentTile(contentTestMovie(), "http://nas:7539", false).square)
end sub

sub test_music_artist_albums_newest_first()
    items = [
        { id: 1, kind: "album", title: "Blue", year: 2001, parent_title: "Joni" },
        { id: 2, kind: "album", title: "Hejira", year: 1976 },
        { id: 3, kind: "album", title: "Court and Spark", sort_title: "Court and Spark", year: 2001 },
        { id: 4, kind: "track", title: "A song" },
        { id: 5, kind: "album", title: "Demos" },
        { id: 6, kind: "album", title: "blue", year: 2001 }
    ]
    albums = musicArtistAlbums(items, "http://nas:7539")
    titles = []
    for each tile in albums.tiles
        titles.Push(tile.title + " " + tile.subtitle)
    end for
    tkEqual(titles, ["Blue 2001", "blue 2001", "Court and Spark 2001", "Hejira 1976", "Demos "], "a later year first, then by title, the server's order kept for the same")
    tkEqual(albums.order, [1, 2, 3, 5, 6], "Play's order, the server's")
    tkTrue(albums.tiles[0].square)
    tkEqual(albums.tiles[0].kind, "album")
end sub

sub test_music_artist_line()
    tkEqual(musicArtistLine({ child_count: 4, leaf_count: 40, genres: ["Rock", "Pop", "Folk", "Jazz"] }, 0), "4 albums · 40 tracks · Rock · Pop · Folk")
    tkEqual(musicArtistLine({ leaf_count: 1 }, 1), "1 album · 1 track", "albums counted from the list")
    tkEqual(musicArtistLine({ child_count: 0, genres: [] }, 0), "")
end sub
