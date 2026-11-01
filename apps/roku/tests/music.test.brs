' Music: tracks as the music player knows them, and how Roku plays them.

function musicTestTrack() as object
    return ParseJson("{""id"":5201,""kind"":""track"",""title"":""Song 1"",""index"":1,""parent_id"":5100,""parent_title"":""Blue"",""grandparent_id"":5000,""grandparent_title"":""The Fixtures"",""duration_ms"":200000,""user_state"":{""position_ms"":0,""watched"":false,""play_count"":0}}")
end function

sub test_music_tracks_of_an_album_and_a_playlist()
    track = musicTrack(musicTestTrack(), "http://nas:7539")
    tkEqual(track, { itemId: 5201, title: "Song 1", artist: "The Fixtures", album: "Blue", image: "http://nas:7539/api/items/5201/images/poster?w=600", durationMs: 200000, index: 1 }, "the album's cover, through the track")
    tkEqual(musicTracks([musicTestTrack(), { id: 9, kind: "album" }], "http://nas:7539").Count(), 1, "tracks only")
    tkEqual(musicTracks([{ entry_id: 3, item: musicTestTrack() }], "http://nas:7539")[0].itemId, 5201, "a playlist's entries")
    rows = musicTrackRows([track], true)
    tkEqual(rows[0].title, "1. Song 1")
    tkEqual(rows[0].duration, "3:20")
    tkEqual(musicTrackRows([track], false)[0].subtitle, "The Fixtures", "a playlist's: the artist")
    album = { title: "Blue", parent_title: "The Fixtures", year: 2001 }
    tkEqual(musicAlbumLine(album, [track, track, track]), "The Fixtures · 2001 · 3 tracks · 10 min")
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
