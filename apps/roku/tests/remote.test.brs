' The user's remote, on another device: every case of the rule the TV
' apps share (packages/tv/test/fixtures/remote.json, which the runner
' gives as pkg:/fixtures/remote.json), through the decisions the players
' make (Remote.brs; Live.brs's liveZap for channel up and down).

function remoteTestFixture() as object
    data = ParseJson(ReadAsciiFile("pkg:/fixtures/remote.json"))
    if data = invalid
        tkFail("the fixture packages/tv/test/fixtures/remote.json didn't load")
        return invalid
    end if
    for each group in ["controls", "video", "music", "live"]
        if httpArray(data[group]).Count() = 0 then tkFail("the fixture has no " + group + " cases")
    end for
    return data
end function

' An action as the fixture writes it: what the player does and whether
' its session reports at once (a play, pause or seek does, and a play or
' pause that finds it so already); a channel as the one it tunes.
function remoteTestWritten(action as object, channel = invalid as dynamic) as object
    if action.act = "none" then return { act: "none", report: false }
    if action.act = "report" then return { act: "none", report: true }
    if action.act = "seek" then return { act: "seek", position_ms: action.positionMs, report: true }
    if action.act = "channel"
        if channel = invalid then return { act: "none", report: false }
        return { act: "channel", channel: channel, report: false }
    end if
    return { act: action.act, report: action.act = "play" or action.act = "pause" }
end function

' The player as the video and music players describe themselves.
function remoteTestPlayer(kind as string, p as object) as object
    return { kind: kind, session: p.session_id, playing: p.state = "playing", positionMs: p.position_ms, durationMs: p.duration_ms, hasNext: p.has_next, index: p.index }
end function

sub test_remote_controls_as_the_fixture_has_them()
    fixture = remoteTestFixture()
    if fixture = invalid then return
    for each c in fixture.controls
        kind = c.in.kind
        if kind = "video"
            declared = remoteVideoControls(httpTrue(c.in.has_next))
        else if kind = "music"
            ' Now Playing's scrubber seeks the Audio node, so the music
            ' declares seek as the other TVs' does.
            declared = remoteMusicControls()
        else
            declared = remoteLiveControls(httpTrue(c.in.has_order))
        end if
        tkEqual(declared, c.out, FormatJson(c.in))
    end for
end sub

sub test_remote_video_as_the_fixture_has_it()
    fixture = remoteTestFixture()
    if fixture = invalid then return
    for each c in fixture.video
        tkEqual(remoteTestWritten(remoteAction(remoteTestPlayer("video", c.in.player), c.in.command)), c.out, FormatJson(c.in))
    end for
end sub

sub test_remote_music_as_the_fixture_has_it()
    fixture = remoteTestFixture()
    if fixture = invalid then return
    for each c in fixture.music
        tkEqual(remoteTestWritten(remoteAction(remoteTestPlayer("music", c.in.player), c.in.command)), c.out, FormatJson(c.in))
    end for
end sub

' Through the page of the category the live player holds, as its channel
' up and down go (liveZap): a channel it would tune again is left alone.
' Without channels (null) the player has no page, and nothing to step
' through.
sub test_remote_live_as_the_fixture_has_it()
    fixture = remoteTestFixture()
    if fixture = invalid then return
    for each c in fixture.live
        p = c.in.player
        player = { kind: "live", session: p.session_id, hasOrder: p.channels <> invalid }
        action = remoteAction(player, c.in.command)
        channel = invalid
        if action.act = "channel"
            channels = []
            for each id in p.channels
                channels.Push({ id: id })
            end for
            move = liveZap({ total: channels.Count(), offset: 0, channels: channels }, p.channel, action.by)
            if move <> invalid and move.channel <> invalid and move.channel.id <> p.channel then channel = move.channel.id
        end if
        tkEqual(remoteTestWritten(action, channel), c.out, FormatJson(c.in))
    end for
end sub

sub test_remote_commands_without_a_session_or_an_event()
    player = { kind: "video", session: "", playing: true, positionMs: 0, durationMs: 0, hasNext: true, index: 0 }
    tkEqual(remoteAction(player, { session_id: "", command: "pause" }).act, "none", "a player with no session yet")
    player.session = "s1"
    tkEqual(remoteAction(player, invalid).act, "none")
    tkEqual(remoteAction(player, { session_id: "s1", command: "seek" }).act, "none", "position_ms left out")
    tkEqual(remoteAction(player, { session_id: "s1", command: "skip", position_ms: 5000 }).act, "none", "offset_ms left out")
    tkEqual(remoteAction(player, { session_id: "s1", command: 7 }).act, "none")
    tkEqual(remoteAction(player, { session_id: "s1", command: "seek", position_ms: 1500.7 }), { act: "seek", positionMs: 1500 }, "a fraction of a millisecond")
    tkEqual(remoteAction({ kind: "live", session: "l1", hasOrder: true }, { session_id: "l1", command: "stop" }).act, "none", "stop comes as session_stopped")
end sub

sub test_remote_device_name_and_kind()
    tkEqual(remoteDeviceKind(), "tv")
    tkEqual(remoteDeviceName("  Living room Roku "), "Living room Roku")
    tkEqual(Len(remoteDeviceName(String(80, "a"))), 64, "as long as the server keeps")
    tkEqual(remoteDeviceName("Den" + Chr(10) + "TV"), "Den TV", "no control characters")
    tkEqual(remoteDeviceName(invalid), "")
    tkEqual(remoteDeviceName("Roku Ultra - X00000ABCDEF"), "Roku Ultra", "a Roku nobody named: no serial number")
    tkEqual(remoteDeviceName("Roku Express 4K+ - YH00AB123456 "), "Roku Express 4K+")
    tkEqual(remoteDeviceName("Kids - upstairs"), "Kids - upstairs", "a name with a dash of its own")
    tkEqual(remoteDeviceName("Den - TV"), "Den - TV")
    tkEqual(remoteDeviceName("Lounge - BEDROOMTV"), "Lounge - BEDROOMTV", "no digit: a word, not a serial")
end sub

' Commands come on the event stream, which connects only to http://: a
' player of an https:// server says it obeys nothing, and remotes offer
' only Stop.
sub test_remote_controls_only_from_an_http_server()
    tkTrue(remoteMayControl("http://192.168.1.20:8096"))
    tkTrue(remoteMayControl("HTTP://media.lan"))
    tkFalse(remoteMayControl("https://media.example.com"))
    tkFalse(remoteMayControl(""))
    tkFalse(remoteMayControl(invalid))
end sub

' Channel up or down that waits for the next page of the category goes on
' only from the channel it started from with no channel picked since
' (Live.brs), as on LG and Samsung.
sub test_live_a_step_waiting_for_its_page_goes_on_only_from_where_it_started()
    ' Channel 5 shows; the remote's next waits for a page, picks counted 3.
    picks = 3
    tkTrue(liveStepGoesOn(5, 3, picks, 5, false, true), "nothing happened meanwhile")
    ' Watch again tunes 5 once more: the same channel, but a pick.
    tkFalse(liveStepGoesOn(5, 3, liveTunePicks(picks, false, false), 5, false, true), "Watch again")
    ' 7 from the list, then 5 again: back on the step's channel, picked.
    after = liveTunePicks(liveTunePicks(picks, false, false), false, false)
    tkFalse(liveStepGoesOn(5, 3, after, 5, false, true), "7 then 5")
    ' The zap timer tunes the channel up or down led to: not a pick.
    tkEqual(liveTunePicks(picks, false, true), picks)
    tkTrue(liveStepGoesOn(5, 3, liveTunePicks(picks, false, true), 5, false, true), "the zap timer's tune")
    ' A stream's hiccup tunes the channel again: not a pick either.
    tkEqual(liveTunePicks(picks, true, false), picks)
    tkTrue(liveStepGoesOn(5, 3, liveTunePicks(picks, true, false), 5, false, true), "a retry")
    ' Up or down led on to another channel meanwhile, or watching stopped.
    tkFalse(liveStepGoesOn(5, 3, picks, 6, false, true), "led to another channel")
    tkFalse(liveStepGoesOn(5, 3, picks, invalid, false, true), "no channel")
    tkFalse(liveStepGoesOn(5, 3, picks, 5, true, true), "it says what went wrong")
    tkFalse(liveStepGoesOn(5, 3, picks, 5, false, false), "the player closed")
end sub

sub test_live_stopped_from_elsewhere_says_who()
    elsewhere = liveStoppedBy({ session_id: "l1", message: "Dinner time", by_admin: false })
    tkEqual(elsewhere.kind, "stopped")
    tkEqual(elsewhere.title, "Live TV stopped")
    tkEqual(elsewhere.lines, "Live TV was stopped from another device." + Chr(10) + "Dinner time")
    tkTrue(elsewhere.retry, "Watch again")
    tkEqual(liveStoppedBy({ session_id: "l1", message: invalid, by_admin: true }).lines, "Live TV was stopped by the server's administrator.")
    tkEqual(liveStoppedBy({ session_id: "l1", message: "  ", by_admin: false }).lines, "Live TV was stopped from another device.")
    tkEqual(liveStoppedBy({ session_id: "l1" }).lines, "Live TV was stopped by the server's administrator.", "who left out")
    tkEqual(liveTroubleButtons(elsewhere)[0].label, "Watch again")
end sub

' The video says it obeys next where Up Next may come: a playlist's or
' collection's next entry, else an episode's next one.
sub test_remote_video_next_where_something_may_come()
    episode = { kind: "episode" }
    movie = { kind: "movie" }
    tkTrue(playerMayHaveNext(episode, invalid, 0))
    tkFalse(playerMayHaveNext(movie, invalid, 0))
    tkTrue(playerMayHaveNext(movie, [{}, {}], 0), "a playlist's next entry")
    tkFalse(playerMayHaveNext(episode, [{}, {}], 1), "the playlist's last: no next episode either")
    tkFalse(playerMayHaveNext(invalid, invalid, 0))
    tkEqual(remoteVideoControls(playerMayHaveNext(episode, invalid, 0)), ["play", "pause", "seek", "skip", "next"])
end sub

' The remote's previous goes where Now Playing's button does.
sub test_remote_previous_as_the_music_button()
    for each c in [{ at: 2, ms: 5000 }, { at: 2, ms: 3000 }, { at: 2, ms: 0 }, { at: 0, ms: 1000 }, { at: 1, ms: 3001 }]
        action = remoteAction({ kind: "music", session: "t1", playing: true, positionMs: c.ms, durationMs: 240000, hasNext: true, index: c.at }, { session_id: "t1", command: "previous" })
        restarts = musicPreviousAt(c.at, c.ms) = c.at
        tkEqual(action.act = "seek", restarts, FormatJson(c))
    end for
end sub

' A video declares audio and subtitles where its file has tracks of them
' (one audio track too: the remote may pick it); music declares neither.
sub test_remote_video_declares_the_tracks_its_file_has()
    tkEqual(remoteVideoControls(false, 0, 0), ["play", "pause", "seek", "skip"])
    tkEqual(remoteVideoControls(false, 1, 0), ["play", "pause", "seek", "skip", "audio"])
    tkEqual(remoteVideoControls(false, 0, 2), ["play", "pause", "seek", "skip", "subtitles"])
    tkEqual(remoteVideoControls(true, 2, 3), ["play", "pause", "seek", "skip", "next", "audio", "subtitles"])
    for each control in remoteMusicControls()
        tkTrue(control <> "audio" and control <> "subtitles", control)
    end for
    file = { streams: [{ kind: "video", index: 0 }, { kind: "audio", index: 1 }, { kind: "audio", index: 2 }, { kind: "subtitle", index: 3 }, { kind: "subtitle", index: 10001, external: true }, { kind: "attachment", index: 5 }] }
    tkEqual(tracksIndexes(file), { audio: [1, 2], subtitles: [3, 10001] }, "sidecar and added subtitles too")
    tkEqual(tracksIndexes(invalid), { audio: [], subtitles: [] })
end sub

' The remote's audio and subtitles (remoteTrackAction), for a film whose
' file has audio 1 and 2 and subtitles 3 and 4, playing audio 1 without
' subtitles: another track is picked and reported, the one playing is
' said again, and what the file doesn't have does nothing.
sub test_remote_audio_and_subtitles()
    player = { session: "s1", audio: [1, 2], subtitles: [3, 4], audioIndex: 1, subtitleIndex: -1 }
    none = { act: "none", index: -1, report: false }
    same = { act: "none", index: -1, report: true }
    cases = [
        { command: { session_id: "s2", command: "audio", stream_index: 2 }, out: none },
        { command: { session_id: "s2", command: "subtitles", stream_index: -1 }, out: none },
        { command: { session_id: "s1", command: "audio", stream_index: 2 }, out: { act: "audio", index: 2, report: true } },
        { command: { session_id: "s1", command: "audio", stream_index: 1 }, out: same },
        { command: { session_id: "s1", command: "audio", stream_index: 9 }, out: none },
        { command: { session_id: "s1", command: "audio", stream_index: 3 }, out: none },
        { command: { session_id: "s1", command: "audio", stream_index: -1 }, out: none },
        { command: { session_id: "s1", command: "audio", stream_index: invalid }, out: none },
        { command: { session_id: "s1", command: "subtitles", stream_index: 3 }, out: { act: "subtitles", index: 3, report: true } },
        { command: { session_id: "s1", command: "subtitles", stream_index: -1 }, out: same },
        { command: { session_id: "s1", command: "subtitles", stream_index: 9 }, out: none },
        { command: { session_id: "s1", command: "subtitles", stream_index: 2 }, out: none },
        { command: { session_id: "s1", command: "subtitles", stream_index: -2 }, out: none },
        { command: { session_id: "s1", command: "pause", stream_index: invalid }, out: none }
    ]
    for each c in cases
        tkEqual(remoteTrackAction(player, c.command), c.out, FormatJson(c.command))
    end for
    showing = { session: "s1", audio: [1, 2], subtitles: [3, 4], audioIndex: 1, subtitleIndex: 4 }
    tkEqual(remoteTrackAction(showing, { session_id: "s1", command: "subtitles", stream_index: 3 }), { act: "subtitles", index: 3, report: true })
    tkEqual(remoteTrackAction(showing, { session_id: "s1", command: "subtitles", stream_index: -1 }), { act: "subtitles", index: -1, report: true }, "off")
    tkEqual(remoteTrackAction(showing, { session_id: "s1", command: "subtitles", stream_index: 4 }), same, "the subtitles showing")
    unknown = { session: "s1", audio: [1, 2], subtitles: [3, 4], audioIndex: -1, subtitleIndex: -2 }
    tkEqual(remoteTrackAction(unknown, { session_id: "s1", command: "audio", stream_index: 1 }), { act: "audio", index: 1, report: true }, "the audio playing not known")
    tkEqual(remoteTrackAction(unknown, { session_id: "s1", command: "subtitles", stream_index: -1 }), { act: "subtitles", index: -1, report: true }, "the subtitles shown not known")
    bare = { session: "s1", audio: [1], subtitles: [], audioIndex: 1, subtitleIndex: -1 }
    tkEqual(remoteTrackAction(bare, { session_id: "s1", command: "subtitles", stream_index: -1 }), same, "off on a file without subtitles")
    starting = { session: "", audio: [1, 2], subtitles: [3, 4], audioIndex: 1, subtitleIndex: -1 }
    tkEqual(remoteTrackAction(starting, { session_id: "", command: "audio", stream_index: 2 }), none, "no session yet")
    tkEqual(remoteTrackAction(player, invalid), none)
end sub

' What the reports say shows: subtitles burned in while no others show,
' else those shown (pictures drawn here or a text track).
sub test_remote_reports_the_subtitles_showing()
    tkEqual(tracksSubtitlePlaying(3, -1), 3, "a text track")
    tkEqual(tracksSubtitlePlaying(-1, -1), -1, "none")
    tkEqual(tracksSubtitlePlaying(-1, 4), 4, "burned in")
    tkEqual(tracksSubtitlePlaying(-2, 4), 4, "burned in, captions not known")
    tkEqual(tracksSubtitlePlaying(-2, -1), -2, "not known")
end sub
