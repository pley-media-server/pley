' The player's decisions that need no Video node: when to report progress,
' what an error means, when Up Next and the skip buttons show (docs/API.md,
' "Playback").

' Progress is reported about every 10 s while playing.
function playerReportEveryMs() as integer
    return 10000
end function

' And every 30 s while paused: a session not heard from for 90 s has lost
' its player, and its keyed addresses end 15 minutes on.
function playerPausedReportEveryMs() as integer
    return 30000
end function

function playerShouldReport(state as string, sinceLastMs as integer) as boolean
    return state = "playing" and sinceLastMs >= playerReportEveryMs()
end function

' The body of POST /api/items/{id}/progress.
function playerProgress(positionMs as dynamic, durationMs as dynamic, state as string, sessionId as string) as object
    position = Int(httpNumber(positionMs, 0))
    if position < 0 then position = 0
    body = { position_ms: position, state: state }
    duration = Int(httpNumber(durationMs, 0))
    if duration > 0 then body.duration_ms = duration
    if sessionId <> "" then body.session_id = sessionId
    return body
end function

' What a Video node error is: "media" (this file doesn't play on this Roku:
' it is converted instead) or "stream" (HTTP or network: the session's
' status says what happened). errorInfo is the node's errorInfo
' ({ category, errcode, ... }). Before the video first played, anything
' but HTTP is the file's; after, only what says so.
function playerErrorKind(errorCode as integer, errorInfo as dynamic, started as boolean) as string
    category = ""
    info = httpObject(errorInfo)
    if info <> invalid then category = LCase(httpString(info.category))
    if category = "http" then return "stream"
    if not started then return "media"
    if category = "mediaerror" or category = "drm" then return "media"
    if errorCode = -5 or errorCode = -6 then return "media"
    return "stream"
end function

' The subtitles to ask the server to draw onto the video (subtitle_index):
' those the person's choices pick for file (its selected_subtitle) when
' they are pictures (PGS, VobSub, DVB) the app doesn't draw itself, as the
' TV apps ask; -1 for none, for text ones, which the Video node shows,
' and for pictures in a codec of drawn (picCodecs(), or [] once the server
' turned out not to make them).
function playerBurnIndex(file as dynamic, drawn as object) as integer
    data = httpObject(file)
    if data = invalid then return -1
    selected = Int(httpNumber(data.selected_subtitle, -1))
    if selected < 0 then return -1
    for each stream in httpArray(data.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid and httpString(stream.kind) = "subtitle" and httpNumber(stream.index, -2) = selected
            if GetInterface(stream.text_based, "ifBoolean") = invalid or stream.text_based then return -1
            codec = LCase(httpString(stream.codec))
            for each name in drawn
                if name = codec then return -1
            end for
            return selected
        end if
    end for
    return -1
end function

' What the answer to GET /api/playback/{session} says of a broken stream:
' "stopped" (410: stopped from elsewhere), "expired" (404: start a new
' session), "alive" (the stream should play: a token that ran out, or the
' network), "signed_out" or "unreachable".
function playerSessionState(status as integer) as string
    if status = 410 then return "stopped"
    if status = 404 then return "expired"
    if status >= 200 and status < 300 then return "alive"
    if status = 401 then return "signed_out"
    if status <= 0 then return "unreachable"
    return "alive"
end function

' What the player says when the server stopped the stream: the answer's
' message, when the one who stopped it wrote one. The event stream says
' whether that was an administrator (by_admin); a 410 doesn't, and is
' taken for one.
function playerStoppedMessage(body as dynamic) as string
    text = "Playback was stopped by the server's administrator."
    data = invalid
    data = httpObject(httpJson(body))
    if GetInterface(body, "ifAssociativeArray") <> invalid then data = body
    if data <> invalid
        if GetInterface(data.by_admin, "ifBoolean") <> invalid and not data.by_admin then text = "Playback was stopped from another device."
        message = httpSentence(data.message)
        if message <> "" then text = text + Chr(10) + message
    end if
    return text
end function

' The intro and credits of the file being played (the item's first file
' when the session doesn't say), as { kind, startMs, endMs, key }: markers
' belong to a file, since versions can be different cuts.
function playerFileMarkers(item as dynamic, fileId as dynamic) as object
    found = []
    data = httpObject(item)
    if data = invalid then return found
    wanted = httpNumber(fileId, -1)
    if wanted = -1
        files = httpArray(data.files)
        if files.Count() > 0 and GetInterface(files[0], "ifAssociativeArray") <> invalid then wanted = httpNumber(files[0].id, -1)
    end if
    for each marker in httpArray(data.markers)
        if GetInterface(marker, "ifAssociativeArray") <> invalid and httpNumber(marker.file_id, -2) = wanted
            kind = httpString(marker.kind)
            startMs = Int(httpNumber(marker.start_ms, -1))
            endMs = Int(httpNumber(marker.end_ms, -1))
            if (kind = "intro" or kind = "credits") and startMs >= 0 and endMs > startMs
                found.Push({ kind: kind, startMs: startMs, endMs: endMs, key: kind + ":" + startMs.ToStr() })
            end if
        end if
    end for
    return found
end function

' When Up Next shows, in ms into the file: where its credits start, else
' 30 s before the end; -1 for a file too short for it. markers are the
' file's (playerFileMarkers).
function playerUpNextAt(durationMs as dynamic, markers as object) as integer
    duration = Int(httpNumber(durationMs, 0))
    credits = -1
    for each marker in markers
        if marker.kind = "credits" and marker.startMs > 0 and (credits = -1 or marker.startMs < credits) then credits = marker.startMs
    end for
    if credits > 0 and (duration <= 0 or credits < duration) then return credits
    if duration > 60000 then return duration - 30000
    return -1
end function

' Skip Intro and Skip Credits, as in the web player
' (apps/web/src/player/SkipButton.tsx): a button from a marker's start
' until a second before its end.
function playerSkipTailMs() as integer
    return 1000
end function

' The marker the playhead is in, or invalid.
function playerActiveMarker(markers as object, positionMs as integer) as dynamic
    for each marker in markers
        if positionMs >= marker.startMs and positionMs < marker.endMs - playerSkipTailMs() then return marker
    end for
    return invalid
end function

' The marker whose button shows at positionMs, or invalid: credits leave
' the screen to Up Next when an episode has a next one to offer.
function playerSkipFor(markers as object, positionMs as integer, upNextComing as boolean) as dynamic
    marker = playerActiveMarker(markers, positionMs)
    if marker = invalid then return invalid
    if marker.kind = "credits" and upNextComing then return invalid
    return marker
end function

function playerSkipLabel(marker as object) as string
    if marker.kind = "credits" then return "Skip credits"
    return "Skip intro"
end function

' Where skipping lands, in ms: just before an intro ends, so its last
' moment plays, and the end of credits. -1 when that is the end of the
' file: the title then ends as if played to its end.
function playerSkipTarget(marker as object, durationMs as dynamic) as integer
    lead = 0
    if marker.kind = "intro"
        lead = (marker.endMs - marker.startMs) \ 2
        if lead > 750 then lead = 750
    end if
    target = marker.endMs - lead
    duration = Int(httpNumber(durationMs, 0))
    if duration > 0 and target >= duration - playerSkipTailMs() then return -1
    return target
end function

' Whether something may come after item: a playlist's or collection's
' next entry (queue, the entries, playing the one at queueAt), else the
' next episode, which only an episode may have. The user's remote is told
' the video obeys next when it may (Remote.brs).
function playerMayHaveNext(item as dynamic, queue as dynamic, queueAt as integer) as boolean
    if GetInterface(queue, "ifArray") <> invalid then return queueAt + 1 < queue.Count()
    data = httpObject(item)
    return data <> invalid and httpString(data.kind) = "episode"
end function

' Up Next counts down this many seconds before the next episode plays.
function playerUpNextSeconds() as integer
    return 10
end function

' Whether Up Next plays the next episode by itself at the end of its
' countdown (Settings, kept on the Roku): yes, unless turned off.
function playerAutoplay(section as object) as boolean
    return section.Read("autoplay") <> "off"
end function

sub playerSetAutoplay(section as object, playsOn as boolean)
    if playsOn
        section.Delete("autoplay")
    else
        section.Write("autoplay", "off")
    end if
    section.Flush()
end sub

' Whether a broken stream may be started again: at most 3 times a minute.
' restarts are the clock readings (ms) of the earlier restarts.
function playerMayRestart(restarts as object, nowMs as integer) as boolean
    recent = 0
    for each at in restarts
        if nowMs - at < 60000 then recent = recent + 1
    end for
    return recent < 3
end function

' Whether the server counts a title played to positionMs as watched: 90 %
' of it (docs/API.md, POST /api/items/{id}/progress).
function playerWatched(positionMs as dynamic, durationMs as dynamic) as boolean
    duration = httpNumber(durationMs, 0)
    return duration > 0 and httpNumber(positionMs, 0) >= duration * 0.9
end function

' Where a title starts: its place when there is one to resume from.
function playerStartOf(positionMs as dynamic, durationMs as dynamic) as integer
    if fmtResumable(positionMs, durationMs) then return Int(httpNumber(positionMs, 0))
    return 0
end function

' While the server is out of reach, the player keeps its place and asks
' again every 2 s, for as long as the server keeps a session it doesn't hear
' from (90 s), as the other apps do.
function playerReconnectMs() as integer
    return 90000
end function

function playerReconnectEveryMs() as integer
    return 2000
end function

' Whether a refusal before playback is worth trying again: not one the
' same request would get again (a request the server can't take, the
' sign-in, no access, no such title, one stopped), as the Samsung app.
function playerMayTryAgain(status as integer) as boolean
    return status <> 400 and status <> 401 and status <> 403 and status <> 404 and status <> 410
end function

' How long the playhead may stand still while the video is meant to play
' before it counts as stalled (Android's and the web's STALL_MS).
function playerStallMs() as integer
    return 12000
end function

' Whether media that finished at reachedMs, having started at startMs,
' ended short rather than at its end: a minute or more before the duration
' the server gives, moments after where it started. The stream or file
' doesn't hold that part (resuming past its real end), so it isn't watched.
function playerEndedShort(reachedMs as dynamic, startMs as dynamic, durationMs as dynamic) as boolean
    duration = httpNumber(durationMs, 0)
    reached = httpNumber(reachedMs, 0)
    if duration <= 0 then return false
    return reached + 60000 < duration and reached < httpNumber(startMs, 0) + 5000
end function

' Whether the playhead jumped (a seek, Roku's own trick play included)
' between two readings: it moved more than 3 s from where playing on would
' have taken it. The server hears of a seek at once, as the other apps tell
' it a second after one.
function playerSought(fromMs as dynamic, toMs as dynamic, elapsedMs as dynamic) as boolean
    expected = httpNumber(fromMs, 0) + httpNumber(elapsedMs, 0)
    return Abs(httpNumber(toMs, 0) - expected) > 3000
end function

' The title shown at the top for 4 s as each item begins, as on Android TV
' and the native Samsung app: { title, line }, a show's name over an
' episode's "S1 E2 · Pilot".
function playerBanner(item as dynamic) as object
    data = httpObject(item)
    banner = { title: "", line: "" }
    if data = invalid then return banner
    title = httpSentence(data.title)
    if httpString(data.kind) = "episode"
        showTitle = httpSentence(data.grandparent_title)
        parts = []
        code = fmtEpisode(data.parent_index, data.index)
        if code <> "" then parts.Push(code)
        if title <> "" then parts.Push(title)
        if showTitle <> ""
            banner.title = showTitle
            banner.line = parts.Join(" · ")
            return banner
        end if
    end if
    banner.title = title
    return banner
end function

function playerBannerMs() as integer
    return 4000
end function
