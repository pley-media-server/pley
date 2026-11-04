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
