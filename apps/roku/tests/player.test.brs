sub test_player_reports_while_paused_too()
    tkEqual(playerPausedReportEveryMs(), 30000)
    tkTrue(playerPausedReportEveryMs() * 3 <= 90000, "three reports before the server takes the player for gone")
end sub

sub test_player_autoplay_kept_on_the_roku()
    section = CreateObject("roRegistrySection", "playerAutoplayTest")
    section.Delete("autoplay")
    tkTrue(playerAutoplay(section), "on unless turned off")
    playerSetAutoplay(section, false)
    tkFalse(playerAutoplay(section))
    playerSetAutoplay(section, true)
    tkTrue(playerAutoplay(section))
end sub

sub test_player_reports_every_ten_seconds_while_playing()
    tkTrue(playerShouldReport("playing", 10000))
    tkFalse(playerShouldReport("playing", 9999))
    tkFalse(playerShouldReport("paused", 60000))
    tkFalse(playerShouldReport("buffering", 60000))
end sub

sub test_player_progress_bodies()
    tkEqual(playerProgress(61500.7, 6960000, "playing", "s1"), { position_ms: 61500, state: "playing", duration_ms: 6960000, session_id: "s1" })
    tkEqual(playerProgress(-3, 0, "stopped", ""), { position_ms: 0, state: "stopped" })
end sub

sub test_player_tells_media_errors_from_stream_errors()
    tkEqual(playerErrorKind(-1, { category: "http", errcode: 401, dbgmsg: "" }, true), "stream")
    tkEqual(playerErrorKind(-5, { category: "mediaerror" }, true), "media")
    tkEqual(playerErrorKind(-5, invalid, true), "media")
    tkEqual(playerErrorKind(-6, invalid, true), "media")
    tkEqual(playerErrorKind(-1, invalid, true), "stream")
    tkEqual(playerErrorKind(-3, {}, true), "stream")
end sub

sub test_player_converts_what_fails_before_it_plays()
    tkEqual(playerErrorKind(-1, invalid, false), "media", "the first picture never came")
    tkEqual(playerErrorKind(-3, { category: "network" }, false), "media")
    tkEqual(playerErrorKind(-1, { category: "http", errcode: 404 }, false), "stream", "HTTP stays the session's")
end sub

sub test_player_asks_for_picture_subtitles_to_be_drawn()
    file = {
        id: 7,
        selected_subtitle: 4,
        streams: [
            { index: 0, kind: "video", codec: "h264" },
            { index: 3, kind: "subtitle", codec: "subrip", text_based: true },
            { index: 4, kind: "subtitle", codec: "hdmv_pgs_subtitle", text_based: false },
            { index: 5, kind: "subtitle", codec: "dvd_subtitle" },
            { index: 6, kind: "subtitle", codec: "dvd_subtitle", text_based: false }
        ]
    }
    tkEqual(playerBurnIndex(file, []), 4, "PGS the app doesn't draw")
    tkEqual(playerBurnIndex(file, picCodecs()), -1, "PGS the app draws itself")
    file.selected_subtitle = 6
    tkEqual(playerBurnIndex(file, picCodecs()), 6, "VobSub, which the app doesn't draw")
    file.selected_subtitle = 3
    tkEqual(playerBurnIndex(file, []), -1, "text: the Video node shows it")
    file.selected_subtitle = 5
    tkEqual(playerBurnIndex(file, []), -1, "not said to be a picture")
    file.selected_subtitle = invalid
    tkEqual(playerBurnIndex(file, []), -1, "none picked")
    file.selected_subtitle = 9
    tkEqual(playerBurnIndex(file, []), -1, "no such stream")
    tkEqual(playerBurnIndex(invalid, []), -1)
end sub

sub test_player_reads_the_sessions_status()
    tkEqual(playerSessionState(410), "stopped")
    tkEqual(playerSessionState(404), "expired")
    tkEqual(playerSessionState(200), "alive")
    tkEqual(playerSessionState(401), "signed_out")
    tkEqual(playerSessionState(-7), "unreachable")
end sub

sub test_player_says_who_stopped_the_stream()
    tkEqual(playerStoppedMessage("{""code"":""session_stopped"",""error"":""session_stopped"",""message"":""Server maintenance at 10""}"), "Playback was stopped by the server's administrator." + Chr(10) + "Server maintenance at 10")
    tkEqual(playerStoppedMessage("{""code"":""session_stopped"",""error"":""session_stopped"",""message"":null}"), "Playback was stopped by the server's administrator.")
    tkEqual(playerStoppedMessage({ message: "Disk full" }), "Playback was stopped by the server's administrator." + Chr(10) + "Disk full")
    tkEqual(playerStoppedMessage("<html>"), "Playback was stopped by the server's administrator.")
    tkEqual(playerStoppedMessage({ session_id: "s1", message: invalid, by_admin: false }), "Playback was stopped from another device.", "the event stream says who")
end sub

sub test_player_up_next_at_the_credits()
    item = {
        files: [{ id: 7 }, { id: 8 }],
        markers: [
            { file_id: 7, kind: "intro", start_ms: 30000, end_ms: 90000, source: "fingerprint" },
            { file_id: 7, kind: "credits", start_ms: 3100000, end_ms: 3300000, source: "fingerprint" },
            { file_id: 8, kind: "credits", start_ms: 3200000, end_ms: 3300000, source: "chapter" }
        ]
    }
    tkEqual(playerUpNextAt(3300000, playerFileMarkers(item, 7)), 3100000)
    tkEqual(playerUpNextAt(3300000, playerFileMarkers(item, 8)), 3200000)
    tkEqual(playerUpNextAt(3300000, playerFileMarkers(item, 9)), 3270000, "no credits for this file: 30 s before the end")
    tkEqual(playerUpNextAt(3300000, []), 3270000)
    tkEqual(playerUpNextAt(45000, []), -1, "too short")
end sub

sub test_player_takes_the_markers_of_the_file_played()
    item = {
        files: [{ id: 1 }, { id: 2 }],
        markers: [
            { file_id: 1, kind: "intro", start_ms: 10000, end_ms: 70000, source: "fingerprint" },
            { file_id: 2, kind: "intro", start_ms: 0, end_ms: 5000, source: "chapter" },
            { file_id: 1, kind: "recap", start_ms: 0, end_ms: 5000, source: "chapter" },
            { file_id: 1, kind: "credits", start_ms: 9000, end_ms: 9000, source: "frames" },
            "credits"
        ]
    }
    tkEqual(playerFileMarkers(item, 2), [{ kind: "intro", startMs: 0, endMs: 5000, key: "intro:0" }])
    tkEqual(playerFileMarkers(item, invalid), [{ kind: "intro", startMs: 10000, endMs: 70000, key: "intro:10000" }], "the first file's, without other kinds or empty parts")
    tkEqual(playerFileMarkers({ files: [{ id: 1 }], markers: invalid }, 1), [])
    tkEqual(playerFileMarkers({ files: [], markers: item.markers }, invalid), [], "no file: no markers")
    tkEqual(playerFileMarkers(invalid, 1), [])
end sub

function skipMarkers() as object
    return playerFileMarkers({
        files: [{ id: 1 }],
        markers: [
            { file_id: 1, kind: "intro", start_ms: 10000, end_ms: 70000, source: "fingerprint" },
            { file_id: 1, kind: "credits", start_ms: 1200000, end_ms: 1300000, source: "fingerprint" }
        ]
    }, 1)
end function

sub test_player_shows_a_skip_button_until_a_second_before_the_end()
    markers = skipMarkers()
    tkInvalid(playerActiveMarker(markers, 9999))
    tkEqual(playerActiveMarker(markers, 10000).key, "intro:10000")
    tkEqual(playerActiveMarker(markers, 68999).key, "intro:10000")
    tkInvalid(playerActiveMarker(markers, 69000), "a second before the intro ends")
    tkEqual(playerActiveMarker(markers, 1250000).key, "credits:1200000")
    tkEqual(playerSkipLabel(markers[0]), "Skip intro")
    tkEqual(playerSkipLabel(markers[1]), "Skip credits")
end sub

sub test_player_leaves_an_episodes_credits_to_up_next()
    markers = skipMarkers()
    tkEqual(playerSkipFor(markers, 1250000, false).kind, "credits", "a movie, or an episode without a next one")
    tkInvalid(playerSkipFor(markers, 1250000, true), "Up Next comes up instead")
    tkEqual(playerSkipFor(markers, 20000, true).kind, "intro")
    tkInvalid(playerSkipFor(markers, 80000, false))
end sub

sub test_player_skips_to_the_end_of_an_intro_or_credits()
    markers = skipMarkers()
    tkEqual(playerSkipTarget(markers[0], 1500000), 69250, "just before the intro ends, so its last moment plays")
    tkEqual(playerSkipTarget({ kind: "intro", startMs: 0, endMs: 1000 }, 1500000), 500, "inside a very short intro")
    tkEqual(playerSkipTarget(markers[1], 1500000), 1300000)
    tkEqual(playerSkipTarget(markers[1], 1300000), -1, "credits to the end: the title ends")
    tkEqual(playerSkipTarget(markers[1], 1300800), -1, "less than a second left")
    tkEqual(playerSkipTarget(markers[1], invalid), 1300000, "no duration known")
end sub

sub test_player_limits_restarts()
    tkTrue(playerMayRestart([], 100000))
    tkTrue(playerMayRestart([50000, 90000], 100000))
    tkFalse(playerMayRestart([50000, 60000, 90000], 100000))
    tkTrue(playerMayRestart([1000, 2000, 90000], 100000), "older than a minute")
end sub

sub test_player_starts_where_the_title_was_left()
    tkEqual(playerStartOf(600000, 6000000), 600000)
    tkEqual(playerStartOf(5900000, 6000000), 0, "near the end: from the start")
    tkEqual(playerStartOf(0, 6000000), 0)
end sub

sub test_player_reconnects_while_the_server_keeps_the_session()
    tkEqual(playerReconnectMs(), 90000)
    tkTrue(playerReconnectEveryMs() < playerReconnectMs(), "several tries before giving up")
end sub

sub test_player_offers_try_again_where_it_may_help()
    tkTrue(playerMayTryAgain(503), "the transcoder busy or away")
    tkTrue(playerMayTryAgain(500))
    tkTrue(playerMayTryAgain(409))
    tkFalse(playerMayTryAgain(404), "no such title")
    tkFalse(playerMayTryAgain(403), "no access")
    tkFalse(playerMayTryAgain(401))
    tkFalse(playerMayTryAgain(400))
    tkFalse(playerMayTryAgain(410))
end sub

sub test_player_tells_media_that_ends_short_from_the_end()
    ' Resumed at 40:00 of a 45:00 episode whose file holds 41:00.
    tkTrue(playerEndedShort(2401000, 2400000, 2700000), "a few moments after the start, long before the end")
    tkFalse(playerEndedShort(2699000, 2400000, 2700000), "at the end")
    tkFalse(playerEndedShort(2650000, 2640000, 2700000), "within a minute of the end")
    tkFalse(playerEndedShort(2410000, 2400000, 2700000), "played on 10 s first")
    tkFalse(playerEndedShort(1000, 0, 0), "no duration known")
end sub

sub test_player_notices_a_seek()
    tkFalse(playerSought(60000, 61000, 1000), "played on")
    tkFalse(playerSought(60000, 60200, 1000), "a slow second")
    tkTrue(playerSought(60000, 120000, 1000), "forward")
    tkTrue(playerSought(60000, 30000, 1000), "back")
end sub

sub test_player_banner_names_the_show_over_the_episode()
    tkEqual(playerBanner({ kind: "episode", title: "Pilot", grandparent_title: "The Office", parent_index: 1, index: 2 }), { title: "The Office", line: "S1 E2 · Pilot" })
    tkEqual(playerBanner({ kind: "episode", title: "Pilot", parent_index: 1, index: 2 }), { title: "Pilot", line: "" })
    tkEqual(playerBanner({ kind: "movie", title: "Dune" }), { title: "Dune", line: "" })
    tkEqual(playerBanner(invalid), { title: "", line: "" })
end sub
