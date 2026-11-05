' The user's remote, on another of their devices (docs/API.md, "Remote
' control"): what each player here obeys, and what it does with a
' command (the server's player_command). The Roku obeys and shows no
' remote of its own. The rule is the other apps', pinned for every TV app
' by packages/tv/test/fixtures/remote.json.

' What a film or episode obeys: next only where something may come after
' it (an episode, a playlist's next entry); audio and subtitles where its
' file has tracks of them (audio and subtitles count them), which the
' remote picks from; the quality where the player has its Quality menu.
function remoteVideoControls(hasNext as boolean, audio = 0 as integer, subtitles = 0 as integer, quality = false as boolean) as object
    controls = ["play", "pause", "seek", "skip"]
    if hasNext then controls.Push("next")
    if audio > 0 then controls.Push("audio")
    if subtitles > 0 then controls.Push("subtitles")
    if quality then controls.Push("quality")
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

' Whether the players here may say they obey anything: commands come on
' the event stream, which connects only to an http:// server (ROKU.md,
' Known limits). With https://, they say nothing, so remotes offer only
' Stop, which reaches the Roku when its session is next reported.
function remoteMayControl(server as dynamic) as boolean
    return LCase(Left(httpString(server).Trim(), 7)) = "http://"
end function

' What the players say they play on.
function remoteDeviceKind() as string
    return "tv"
end function

' What remotes call this Roku: the name its owner gave it in Roku's
' settings ("Living room Roku"), at most 64 characters as the server
' keeps; "" leaves the server to name it from the User-Agent. A Roku
' nobody named is called by its model and serial number ("Roku Ultra -
' X00000ABCDEF"): the serial isn't shown to remotes and administrators.
function remoteDeviceName(name as dynamic) as string
    clean = httpClean(name, 64, false).Trim()
    serial = CreateObject("roRegex", "\s+-\s+(?=[A-Z]*[0-9])[A-Z0-9]{8,}$", "")
    return serial.Replace(clean, "").Trim()
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

' What a video does with the remote's audio or subtitles, as { act, index,
' report }:
' - act "audio" or "subtitles": that track (index; -1 turns subtitles
'   off), picked as Pley's menu picks it, then reported;
' - act "none" with report true: the track picked is the one it has, said
'   again at once, so a remote a little behind settles;
' - act "none" with report false: nothing (another session's command, a
'   track the file doesn't have, or one without its stream_index).
' player: { session (its session id, "" while it has none), audio and
' subtitles (the stream indexes of its file's tracks), audioIndex (the
' audio it plays, -1 while not known), subtitleIndex (the subtitles it
' shows: -1 none, -2 not known) }. command: the event's fields
' (session_id, command, stream_index).
function remoteTrackAction(player as object, command as dynamic) as object
    none = { act: "none", index: -1, report: false }
    data = httpObject(command)
    if data = invalid then return none
    session = httpString(player.session)
    if session = "" or httpString(data.session_id) <> session then return none
    index = httpNumber(data.stream_index, invalid)
    if index = invalid then return none
    index = Int(index)
    what = httpString(data.command)
    if what = "audio"
        if not remoteHas(player.audio, index) then return none
        if index = httpNumber(player.audioIndex, -1) then return { act: "none", index: -1, report: true }
    else if what = "subtitles"
        if index <> -1 and not remoteHas(player.subtitles, index) then return none
        if index = httpNumber(player.subtitleIndex, -2) then return { act: "none", index: -1, report: true }
    else
        return none
    end if
    return { act: what, index: index, report: true }
end function

' What a video does with the remote's quality, as { act, quality, report }:
' - act "quality": that quality (one of the menu's ids), picked as Pley's
'   Quality menu picks it, then reported;
' - act "none" with report true: the quality picked is the one it has,
'   said again at once, so a remote a little behind settles;
' - act "none" with report false: nothing (another session's command, or
'   a quality its menu doesn't offer for the file).
' player: { session (its session id, "" while it has none), qualities (the
' ids its menu offers for the file), quality (the one its menu has
' picked) }. command: the event's fields (session_id, command, quality).
function remoteQualityAction(player as object, command as dynamic) as object
    none = { act: "none", quality: "", report: false }
    data = httpObject(command)
    if data = invalid then return none
    session = httpString(player.session)
    if session = "" or httpString(data.session_id) <> session or httpString(data.command) <> "quality" then return none
    wanted = httpString(data.quality)
    offered = false
    for each id in httpArray(player.qualities)
        if httpString(id) = wanted then offered = true
    end for
    if wanted = "" or not offered then return none
    if wanted = httpString(player.quality) then return { act: "none", quality: "", report: true }
    return { act: "quality", quality: wanted, report: true }
end function

' Whether indexes (an array of stream indexes) has index.
function remoteHas(indexes as dynamic, index as integer) as boolean
    for each value in httpArray(indexes)
        if httpNumber(value, -9) = index then return true
    end for
    return false
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
