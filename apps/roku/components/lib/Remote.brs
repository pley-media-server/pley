' The user's remote, on another of their devices (docs/API.md, "Remote
' control"): what each player here obeys, and what it does with a
' command (the server's player_command). The Roku obeys and shows no
' remote of its own. The rule is the other apps', pinned for every TV app
' by packages/tv/test/fixtures/remote.json.

' What a film or episode obeys: next only where something may come after
' it (an episode, a playlist's next entry).
function remoteVideoControls(hasNext as boolean) as object
    controls = ["play", "pause", "seek", "skip"]
    if hasNext then controls.Push("next")
    return controls
end function

' What the music obeys: everything Now Playing's buttons and scrubber do.
function remoteMusicControls() as object
    return ["play", "pause", "seek", "skip", "next", "previous"]
end function

' What a live channel obeys: channel up and down when the player has an
' order to go through; live is live, so no pause or seek.
function remoteLiveControls(hasOrder as boolean) as object
    if hasOrder then return ["next", "previous"]
    return []
end function

' What the players say they play on.
function remoteDeviceKind() as string
    return "tv"
end function

' What remotes call this Roku: the name its owner gave it in Roku's
' settings ("Living room Roku"), at most 64 characters as the server
' keeps; "" leaves the server to name it from the User-Agent.
function remoteDeviceName(name as dynamic) as string
    return httpClean(name, 64, false).Trim()
end function

' "Previous" this far into a track starts it again, as Now Playing's
' button does (Music.brs's musicPreviousAt).
function remoteRestartAfterMs() as integer
    return 3000
end function

' What a player does with a command, as { act } with act:
' - "none": nothing (another session's command, one it doesn't obey, or
'   one missing its value);
' - "report": nothing changes, but it tells the server where it is again
'   at once (a play while playing, a pause while paused), so a remote a
'   little behind settles;
' - "play", "pause": as its own keys do, and reported as any play or
'   pause is;
' - "seek" with positionMs: there, reported at once;
' - "next", "previous": the next item or track (or the one before), which
'   tells the server with its own session;
' - "channel" with by (1 or -1): channel up or down.
' player: { kind: "video", "music" or "live", session (its session id,
' "" while it has none), playing, positionMs, durationMs (0 or invalid
' while the length isn't known), hasNext, index (the track's place in the
' play order), hasOrder (live) }. command: the event's fields
' (session_id, command, position_ms, offset_ms).
function remoteAction(player as object, command as dynamic) as object
    none = { act: "none" }
    data = httpObject(command)
    if data = invalid then return none
    session = httpString(player.session)
    if session = "" or httpString(data.session_id) <> session then return none
    what = httpString(data.command)
    if player.kind = "live"
        if not httpTrue(player.hasOrder) then return none
        if what = "next" then return { act: "channel", by: 1 }
        if what = "previous" then return { act: "channel", by: -1 }
        return none
    end if
    if what = "play"
        if httpTrue(player.playing) then return { act: "report" }
        return { act: "play" }
    else if what = "pause"
        if httpTrue(player.playing) then return { act: "pause" }
        return { act: "report" }
    else if what = "seek"
        ' A seek or skip without its value (null, or left out) does nothing.
        if httpNumber(data.position_ms, invalid) = invalid then return none
        return { act: "seek", positionMs: remoteWithin(data.position_ms, player.durationMs) }
    else if what = "skip"
        if httpNumber(data.offset_ms, invalid) = invalid then return none
        return { act: "seek", positionMs: remoteWithin(httpNumber(player.positionMs, 0) + data.offset_ms, player.durationMs) }
    else if what = "next"
        if httpTrue(player.hasNext) then return { act: "next" }
        return none
    else if what = "previous"
        if player.kind <> "music" then return none
        if httpNumber(player.positionMs, 0) > remoteRestartAfterMs() or httpNumber(player.index, 0) = 0 then return { act: "seek", positionMs: 0 }
        return { act: "previous" }
    end if
    ' A command a newer server knows.
    return none
end function

' From the start to the end; past the start only while the length isn't
' known.
function remoteWithin(ms as dynamic, durationMs as dynamic) as integer
    at = Int(ms)
    if at < 0 then at = 0
    duration = Int(httpNumber(durationMs, 0))
    if duration > 0 and at > duration then at = duration
    return at
end function
