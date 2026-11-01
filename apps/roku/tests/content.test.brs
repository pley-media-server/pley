' Items as docs/API.md's types describe them.

function contentTestServer() as string
    return "http://nas:7539"
end function

function contentTestMovie() as object
    return ParseJson("{""id"":12,""library_id"":1,""kind"":""movie"",""title"":""Arrival"",""sort_title"":""Arrival"",""year"":2016,""duration_ms"":6960000,""added_at"":1700000000,""poster_url"":""/api/items/12/images/poster?v=1729600000"",""backdrop_url"":""/api/items/12/images/backdrop?v=1729600000"",""user_state"":{""position_ms"":600000,""watched"":false,""play_count"":0}}")
end function

function contentTestEpisode() as object
    return ParseJson("{""id"":101,""library_id"":2,""kind"":""episode"",""title"":""Good News About Hell"",""sort_title"":""Good News About Hell"",""index"":1,""parent_id"":100,""parent_title"":""Season 1"",""parent_index"":1,""grandparent_id"":99,""grandparent_title"":""Severance"",""duration_ms"":3300000,""added_at"":1700000000,""thumb_url"":""/api/items/101/images/thumb?v=2"",""summary"":""Mark leads a team."",""user_state"":{""position_ms"":0,""watched"":true,""play_count"":1}}")
end function

function contentTestDetail() as object
    detail = contentTestMovie()
    detail.original_title = invalid
    detail.tagline = "Why are they here?"
    detail.genres = ["Drama", "Science Fiction", "Mystery", "Thriller"]
    detail.content_rating = "PG-13"
    detail.summary = "A linguist works with the military."
    detail.files = ParseJson("[{""id"":7,""path"":""Arrival (2016).mkv"",""size"":1,""container"":""mkv"",""duration_ms"":6960000,""bitrate"":null,""width"":3840,""height"":2160,""video_codec"":""hevc"",""audio_codec"":""eac3"",""streams"":[],""edition"":null,""selected_audio"":1,""selected_subtitle"":3,""chapters"":[]}]")
    detail.markers = []
    return detail
end function

sub test_content_movie_tiles_show_poster_year_and_progress()
    tile = contentTile(contentTestMovie(), contentTestServer(), false)
    tkEqual(tile.title, "Arrival")
    tkEqual(tile.subtitle, "2016")
    tkEqual(tile.image, "http://nas:7539/api/items/12/images/poster?v=1729600000&w=220")
    tkEqual(tile.itemId, 12)
    tkEqual(tile.kind, "movie")
    tkFalse(tile.watched)
    tkTrue(tile.progress > 0.086 and tile.progress < 0.087, "600000 of 6960000")
    tkEqual(tile.positionMs, 600000)
end sub

sub test_content_episode_tiles_name_the_show()
    tile = contentTile(contentTestEpisode(), contentTestServer(), true)
    tkEqual(tile.title, "Severance")
    tkEqual(tile.subtitle, "S1 E1 · Good News About Hell")
    tkEqual(tile.image, "http://nas:7539/api/items/101/images/thumb?v=2&w=400")
    tkTrue(tile.watched)
    tkEqual(tile.progress, 0, "watched shows no progress")
    tkEqual(tile.grandparentId, 99)
    poster = contentTile(contentTestEpisode(), contentTestServer(), false)
    tkEqual(poster.image, "http://nas:7539/api/items/101/images/poster?w=220", "the server falls back to the show's")
end sub

sub test_content_home_rows_of_every_kind()
    libraries = ParseJson("[{""id"":1,""name"":""Movies"",""kind"":""movies"",""paths"":[],""item_count"":245},{""id"":2,""name"":""TV Shows"",""kind"":""shows"",""paths"":[],""item_count"":31},{""id"":3,""name"":""Music"",""kind"":""music"",""paths"":[],""item_count"":120}]")
    hubs = [
        { key: "continue_watching", title: "Continue Watching", library_id: invalid, items: [contentTestEpisode(), contentTestMovie()] },
        { key: "my_list", title: "My List", library_id: invalid, items: [] },
        { key: "recently_added:1", title: "Recently Added in Movies", library_id: 1, items: [contentTestMovie()] },
        { key: "recently_added:3", title: "Recently Added in Music", library_id: 3, items: [{ id: 5, kind: "album", title: "Blue" }] },
        { key: "odd", title: "Odd", library_id: invalid, items: [{ id: 6, kind: "person", title: "Someone" }] }
    ]
    rows = contentHomeRows(hubs, contentTestServer())
    tkEqual(rows.Count(), 3, "an empty hub, and one of kinds this version doesn't show, are left out")
    tkEqual(rows[0].title, "Continue Watching")
    tkTrue(rows[0].wide)
    tkEqual(rows[0].tiles.Count(), 2)
    tkEqual(rows[0].tiles[1].image, "http://nas:7539/api/items/12/images/backdrop?v=1729600000&w=400", "a movie's backdrop in a wide tile")
    tkEqual(rows[1].title, "Recently Added in Movies")
    tkFalse(rows[1].wide)
    tkFalse(rows[1].square)
    tkEqual(rows[0].tiles.Count(), 2, "Continue Watching has no See all")
    tkEqual(rows[1].tiles.Count(), 2, "the movie, then See all")
    tkEqual(rows[1].tiles[1], contentSeeAllTile(1, "added", false, false))
    tkTrue(rows[2].square, "a music library's albums")
    tkTrue(rows[2].tiles[0].square)
    tkTrue(rows[2].tiles[1].square, "See all in the row's shape")
    tkEqual(rows[2].tiles[0].image, "http://nas:7539/api/items/5/images/poster?w=260", "an album's cover, asked for square")
    tkEqual(contentRowSize(false, true), { size: [260, 340], height: 400 })
    tkEqual(contentLibraries(libraries), [{ id: 1, name: "Movies", kind: "movies", itemCount: 245 }, { id: 2, name: "TV Shows", kind: "shows", itemCount: 31 }, { id: 3, name: "Music", kind: "music", itemCount: 120 }])
end sub

sub test_content_see_all_by_row()
    tkEqual(contentSeeAllSort("recently_added:3"), "added")
    tkEqual(contentSeeAllSort("recently_added"), "added", "a library's own rows")
    tkEqual(contentSeeAllSort("recently_released:1"), "release")
    tkEqual(contentSeeAllSort("top_rated:1"), "rating")
    tkEqual(contentSeeAllSort("continue_watching:1"), "")
    tkEqual(contentSeeAllSort("my_list"), "")
    hubs = [
        { key: "top_rated:1", title: "Top Rated", library_id: 1, items: [contentTestMovie()] },
        { key: "recently_added", title: "Recently Added", library_id: invalid, items: [contentTestMovie()] }
    ]
    rows = contentHomeRows(hubs, contentTestServer())
    tkEqual(rows[0].tiles[1].sort, "rating")
    tkEqual(rows[0].tiles[1].libraryId, 1)
    tkEqual(rows[1].tiles.Count(), 1, "without its library: no See all")
end sub

sub test_content_seasons_and_episodes()
    seasons = contentSeasons([
        { id: 98, kind: "season", title: "", index: 0, leaf_count: 2 },
        { id: 100, kind: "season", title: "Season 1", index: 1, leaf_count: 9, viewed_leaf_count: 3 },
        { id: 1, kind: "episode", title: "stray" }
    ])
    tkEqual(seasons, [{ itemId: 98, title: "Specials", index: 0, episodes: 2, watchedEpisodes: 0 }, { itemId: 100, title: "Season 1", index: 1, episodes: 9, watchedEpisodes: 3 }])

    started = contentTestEpisode()
    started.user_state = { position_ms: 900000, watched: false, play_count: 0 }
    episodes = contentEpisodes([contentTestEpisode(), started], contentTestServer())
    tkEqual(episodes[0].title, "1. Good News About Hell")
    tkEqual(episodes[0].image, "http://nas:7539/api/items/101/images/thumb?v=2&w=300", "sized to the list's stills")
    tkEqual(episodes[0].subtitle, "55 min · Watched")
    tkEqual(episodes[0].summary, "Mark leads a team.")
    tkEqual(episodes[1].subtitle, "40 min left")
end sub

sub test_content_details_offer_resume_and_start()
    detail = contentDetail(contentTestDetail(), contentTestServer())
    tkEqual(detail.heading, "Arrival")
    tkEqual(detail.line, "2016 · 1 h 56 min · PG-13")
    tkEqual(detail.genres, "Drama, Science Fiction, Mystery")
    tkEqual(detail.buttons, ["Resume from 10:00", "Play from start", "Mark as watched", "Add to My List", "Rate"])
    tkEqual(detail.actions, ["resume", "start", "watched", "listAdd", "rate"])
    tkEqual(detail.backdrop, "http://nas:7539/api/items/12/images/backdrop?v=1729600000&w=640", "faint behind text: small")
    tkEqual(detail.poster, "http://nas:7539/api/items/12/images/poster?v=1729600000&w=420")

    fresh = contentTestDetail()
    fresh.user_state.position_ms = 0
    fresh.user_state.watched = true
    fresh.in_watchlist = true
    fresh.user_state.rating = 7
    tkEqual(contentDetail(fresh, contentTestServer()).buttons, ["Play", "Mark as unwatched", "Remove from My List", "Your rating: 4 stars"], "3.5 stars shown as 4, as the other apps do")

    long = contentTestDetail()
    long.summary = String(60, "A long summary. ")
    tkEqual(Len(contentDetail(long, contentTestServer()).summary), 959, "the whole summary, trimmed")

    empty = contentTestDetail()
    empty.files = []
    tkEqual(contentDetail(empty, contentTestServer()).note, "This item has no file to play.")
    tkEqual(contentDetail(empty, contentTestServer()).buttons, ["Mark as watched", "Add to My List", "Rate"], "what doesn't need a file")
end sub

sub test_content_show_details_offer_its_next_episode_and_episodes()
    show = ParseJson("{""id"":99,""kind"":""show"",""title"":""Severance"",""year"":2022,""child_count"":2,""content_rating"":""TV-MA"",""user_state"":{""position_ms"":0,""watched"":false,""play_count"":0},""files"":[],""in_watchlist"":true}")
    detail = contentDetail(show, contentTestServer())
    tkEqual(detail.line, "2022 · 2 seasons · TV-MA")
    tkEqual(detail.buttons, ["Episodes", "Mark as watched", "Remove from My List", "Rate"], "before the next episode is known")
    tkEqual(detail.note, "")
    onDeck = contentTestEpisode()
    onDeck.user_state.position_ms = 600000
    detail = contentDetail(show, contentTestServer(), onDeck)
    tkEqual(detail.buttons, ["Resume S1 E1", "Episodes", "Mark as watched", "Remove from My List", "Rate"])
    tkEqual(detail.actions, ["playNext", "episodes", "watched", "listRemove", "rate"])
end sub

sub test_content_episode_details_name_show_and_episode()
    episode = contentTestEpisode()
    episode.files = [{ id: 8, container: "mp4" }]
    detail = contentDetail(episode, contentTestServer())
    tkEqual(detail.buttons, ["Play", "Mark as unwatched", "Rate", "Go to show"], "watched; no My List for an episode")
    tkEqual(detail.showId, 99)
    tkEqual(detail.seasonId, 100)
    tkEqual(detail.heading, "Severance")
    tkEqual(detail.line, "S1 E1 · Good News About Hell · 55 min")
    tkEqual(contentPlayerTitle(episode), "Severance · S1 E1 · Good News About Hell")
    tkEqual(contentPlayerTitle(contentTestMovie()), "Arrival")
end sub

sub test_content_stars_round_up_as_the_other_apps()
    tkEqual(contentStars(invalid), 0)
    tkEqual(contentStars(0), 0)
    tkEqual(contentStars(1), 1, "half a star")
    tkEqual(contentStars(8), 4)
    tkEqual(contentStars(7.3), 4)
    tkEqual(contentStars(10), 5)
    tkEqual(contentStars(12), 5, "never more than 5")
    tkEqual(contentStarsLabel(0), "No rating")
    tkEqual(contentStarsLabel(1), "1 star")
    tkEqual(contentRatingLabel(0), "Rate")
    tkEqual(contentRatingLabel(5), "Your rating: 5 stars")
end sub

sub test_content_stream_formats()
    detail = contentTestDetail()
    tkEqual(contentStreamFormat({ hls: true, file_id: 7 }, detail), "hls")
    tkEqual(contentStreamFormat({ hls: false, file_id: 7 }, detail), "mkv")
    detail.files[0].container = "mov"
    tkEqual(contentStreamFormat({ hls: false, file_id: 7 }, detail), "mp4")
    tkEqual(contentStreamFormat({ hls: false, file_id: 99 }, detail), "mp4", "the first file when the id is unknown")
end sub

sub test_content_stream_carries_text_subtitles()
    info = ParseJson("{""session_id"":""s1"",""method"":""remux"",""url"":""/api/playback/s1/master.m3u8"",""hls"":true,""file_id"":7,""duration_ms"":6960000,""audio_index"":1,""subtitle_index"":null,""burn_subtitles"":false,""audio_converted"":false,""subtitles"":[{""index"":3,""codec"":""subrip"",""language"":""eng"",""title"":""English"",""default"":false,""forced"":false,""external"":false,""text_based"":true,""url"":""/api/files/7/subtitles/3.vtt""},{""index"":4,""codec"":""subrip"",""language"":null,""title"":null,""default"":false,""forced"":true,""external"":false,""text_based"":true,""url"":""/api/files/7/subtitles/4.vtt""},{""index"":5,""codec"":""hdmv_pgs_subtitle"",""language"":""fre"",""title"":null,""default"":false,""forced"":false,""external"":false,""text_based"":false,""url"":null}],""reasons"":[],""reason"":null,""auto_quality"":null}")
    stream = contentStream(info, contentTestDetail(), contentTestServer(), 600000)
    tkEqual(stream.url, "http://nas:7539/api/playback/s1/master.m3u8")
    tkEqual(stream.streamFormat, "hls")
    tkEqual(stream.title, "Arrival")
    tkEqual(stream.playStart, 600)
    tkEqual(stream.subtitleTracks, [
        { Language: "eng", TrackName: "http://nas:7539/api/files/7/subtitles/3.vtt", Description: "English" },
        { Language: "und", TrackName: "http://nas:7539/api/files/7/subtitles/4.vtt", Description: "Forced" }
    ])
    tkEqual(stream.subtitle, "http://nas:7539/api/files/7/subtitles/3.vtt", "the file's selected_subtitle")
    tkEqual(stream.previews, "", "no key, no previews: the Video node sends no token for them")
end sub

sub test_content_stream_names_a_keyed_sessions_previews()
    key = "pley_mk_" + String(43, "a")
    stream = contentStream({ url: "/api/media/" + key + "/master.m3u8", hls: true, file_id: 7 }, contentTestDetail(), contentTestServer(), 0)
    tkEqual(stream.url, "http://nas:7539/api/media/" + key + "/master.m3u8")
    tkEqual(stream.previews, "http://nas:7539/api/media/" + key + "/previews.bif")
    direct = contentStream({ url: "/api/media/" + key + "/file.mkv", hls: false, file_id: 7 }, contentTestDetail(), contentTestServer(), 0)
    tkEqual(direct.previews, "http://nas:7539/api/media/" + key + "/previews.bif", "a direct play's too")
end sub

sub test_content_names_subtitles()
    tkEqual(contentSubtitleName({ title: "SDH" }, "eng"), "SDH")
    tkEqual(contentSubtitleName({ title: invalid }, "fre"), "French", "a file without titles: the language's name, not its code")
    tkEqual(contentSubtitleName({ title: invalid }, "fra"), "French", "the ISO 639-2/T code")
    tkEqual(contentSubtitleName({ title: invalid }, "kur"), "kur", "a language not among the 34: its code")
    tkEqual(contentSubtitleName({ title: invalid, forced: true }, "fre"), "French (Forced)")
    tkEqual(contentSubtitleName({ title: "English (Forced)", forced: true }, "eng"), "English (Forced)", "said once")
    tkEqual(contentSubtitleName({ title: "FORCED", forced: true }, "eng"), "FORCED")
    tkEqual(contentSubtitleName({ title: "Forcedly", forced: true }, "eng"), "Forcedly (Forced)")
    tkEqual(contentSubtitleName({ title: "" }, "und"), "Unknown")
    tkEqual(contentSubtitleName({ forced: true }, "und"), "Forced")
end sub

sub test_content_rebuilds_only_the_home_rows_that_changed()
    hubs = [
        { key: "continue_watching", title: "Continue Watching", library_id: invalid, items: [contentTestEpisode(), contentTestMovie()] },
        { key: "recently_added:1", title: "Recently Added in Movies", library_id: 1, items: [contentTestMovie()] }
    ]
    server = contentTestServer()
    first = contentRowChanges(hubs, server, invalid)
    tkEqual(first.changed.Count(), 2, "nothing shown yet: every row")
    tkEqual(first.changed, contentHomeRows(hubs, server), "the same rows as built in one go")
    tkEqual(first.summary[0].key, "continue_watching")
    tkTrue(first.summary[0].wide)
    tkTrue(first.summary[1].built)
    known = {}
    for each row in first.summary
        known[row.key] = row.sig
    end for
    tkEqual(contentRowChanges(hubs, server, known).changed.Count(), 0, "the same answer: nothing to build")

    hubs[0].items[1].user_state.position_ms = 1200000
    hubs.Push({ key: "my_list", title: "My List", library_id: invalid, items: [contentTestMovie()] })
    hubs.Push({ key: "people", title: "People", library_id: invalid, items: [{ id: 5, kind: "person", title: "Someone" }] })
    again = contentRowChanges(hubs, server, known)
    tkEqual(again.summary.Count(), 3, "a hub with nothing this version shows is left out")
    tkEqual(again.changed.Count(), 2, "a title further along, and a new row")
    tkTrue(again.summary[0].built)
    tkFalse(again.summary[1].built)
    tkTrue(again.summary[1].sig = known["recently_added:1"])
    tkEqual(again.changed[1].key, "my_list")
end sub

sub test_content_home_asks_again_after_playing_or_a_minute()
    tkFalse(contentHomeStale(3, 3, 0), "back from a title at once")
    tkFalse(contentHomeStale(3, 3, 59))
    tkTrue(contentHomeStale(4, 3, 5), "something played")
    tkTrue(contentHomeStale(3, 3, 60), "other devices may have changed it")
end sub

sub test_content_up_next_card()
    nextEpisode = contentTestEpisode()
    nextEpisode.index = 2
    nextEpisode.title = "Half Loop"
    nextEpisode.user_state = { position_ms: 0, watched: false, play_count: 0 }
    card = contentUpNext(nextEpisode, contentTestServer())
    tkEqual(card.title, "Severance")
    tkEqual(card.line, "S1 E2 · Half Loop")
    tkEqual(card.startMs, 0)
    nextEpisode.user_state.position_ms = 120000
    tkEqual(contentUpNext(nextEpisode, contentTestServer()).startMs, 120000, "its own place")
end sub
