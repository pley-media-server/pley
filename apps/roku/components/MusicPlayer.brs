' The music player (docs/API.md, "Playback"): each track gets a session
' (POST /api/playback, the Roku's capabilities with it), plays with the
' scene's Audio node (the token in its HTTP agent, renewed as the token
' is), reports progress about every 10 s, on pause and every 30 s while
' paused, and as "stopped" when it ends or another starts, and its session
' is closed then. A track that
' doesn't play as it is plays again converted, once; one that doesn't play
' at all is passed over. The end of the queue stops it. The queue keeps
' the order it was given in (lib/Music.brs), so Stop shuffling puts it
' back. The sleep timer (lib/Sleep.brs) is the music player's, not Now
' Playing's: the music plays on while other screens show.

sub init()
    m.audio = m.top.findNode("audio")
    m.audio.notificationInterval = 1
    m.audio.observeField("state", "onState")
    m.audio.observeField("position", "onPosition")
    m.top.observeField("play", "onPlay")
    m.top.observeField("control", "onControl")
    m.top.observeField("jump", "onJump")
    m.global.session.observeFieldScoped("accessToken", "onToken")
    m.clock = CreateObject("roTimespan")
    m.queue = musicQueue([], 0, false)
    ' Which opening the answers belong to, its session and details, how it
    ' was asked for ("auto", or "transcode" once the file didn't play), and
    ' whether it has played.
    m.attempt = 0
    m.sessionId = ""
    m.info = invalid
    m.detail = invalid
    m.mode = "auto"
    m.played = false
    m.lastReportAt = 0
    ' The sleep timer, off until set, for as long as music plays; when it
    ' paused the music (clock ms), -1 otherwise; whether the track shown
    ' waits for Play, with no session yet (the timer ended with the one
    ' before); and what Now Playing was told of it last.
    m.sleep = sleepNew(0, 0#)
    m.sleepPausedAt = -1#
    m.waiting = false
    m.sleepShown = ""
    m.sleepTimer = m.top.findNode("sleepTimer")
    m.sleepTimer.observeField("fire", "onSleepTick")
    m.pausedTimer = m.top.findNode("pausedTimer")
    m.pausedTimer.duration = playerPausedReportEveryMs() / 1000
    m.pausedTimer.observeField("fire", "onPausedTick")
    showSleep(sleepNow())
end sub

sub onToken()
    m.audio.setHttpAgent(uiHttpAgent())
end sub

sub onPlay()
    request = m.top.play
    if request = invalid or GetInterface(request.queue, "ifArray") = invalid or request.queue.Count() = 0 then return
    ' The track playing ends before the new queue replaces it.
    finishTrack()
    sleepWoken()
    m.queue = musicQueue(request.queue, Int(httpNumber(request.at, 0)), httpTrue(request.shuffled))
    showQueue()
    m.top.message = ""
    playTrack(m.queue.at, "auto")
end sub

' The queue as Now Playing lists it.
sub showQueue()
    m.top.queue = musicOrder(m.queue)
    m.top.queueSize = m.queue.order.Count()
    m.top.shuffled = m.queue.shuffled
    m.top.queueId = m.top.queueId + 1
end sub

' A track of Up next: it plays, and the queue goes on from it.
sub onJump()
    at = m.top.jump
    if at < 0 or at >= m.queue.order.Count() then return
    sleepWoken()
    playTrack(at, "auto")
end sub

' The track at, from its start; its session and its details are asked for
' together (the details say the file's container).
sub playTrack(at as integer, mode as string)
    showTrack(at, "loading")
    m.mode = mode
    track = musicTrackAt(m.queue, at)
    uiRequest({ op: "play", itemId: track.itemId, startMs: 0, mode: mode, attempt: m.attempt }, "onSession")
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(track.itemId), attempt: m.attempt }, "onDetail")
end sub

' The track at, at its start, the one before left: "loading" while its
' session is asked for, or "paused", waiting for Play with none (where the
' sleep timer stopped the music).
sub showTrack(at as integer, state as string)
    finishTrack()
    m.queue.at = at
    m.attempt = m.attempt + 1
    m.info = invalid
    m.detail = invalid
    m.played = false
    m.waiting = state = "paused"
    track = musicTrackAt(m.queue, at)
    m.top.track = track
    m.top.at = at
    m.top.positionMs = 0
    m.top.durationMs = track.durationMs
    m.top.state = state
    m.audio.control = "stop"
end sub

sub onSession(event as object)
    answer = event.getData()
    if event.getRoSGNode().spec.attempt <> m.attempt
        ' For a track left meanwhile: the server would go on preparing it.
        if answer.ok and answer.data <> invalid then closeSession(httpString(answer.data.session_id))
        return
    end if
    if not answer.ok or answer.data = invalid
        if answer.error = "signed_out"
            stopAll()
            return
        end if
        passOver(answer.message)
        return
    end if
    m.info = answer.data
    m.sessionId = httpString(m.info.session_id)
    begin()
end sub

sub onDetail(event as object)
    if event.getRoSGNode().spec.attempt <> m.attempt then return
    answer = event.getData()
    if answer.ok and answer.data <> invalid
        m.detail = answer.data
    else
        m.detail = {}
    end if
    begin()
end sub

sub begin()
    if m.info = invalid or m.detail = invalid then return
    track = musicTrackAt(m.queue, m.queue.at)
    url = httpString(m.info.url)
    if Left(url, 1) = "/" then url = m.global.session.server + url
    content = CreateObject("roSGNode", "ContentNode")
    content.url = url
    content.streamFormat = musicStreamFormat(m.info, m.detail)
    content.title = track.title
    if LCase(Left(url, 8)) = "https://" then nodesFillCertificates(content)
    duration = Int(httpNumber(m.info.duration_ms, 0))
    if duration > 0 then m.top.durationMs = duration
    m.audio.setHttpAgent(uiHttpAgent())
    m.audio.content = content
    m.audio.control = "play"
    m.lastReportAt = m.clock.TotalMilliseconds()
end sub

sub nodesFillCertificates(content as object)
    content.addFields({ HttpCertificatesFile: "common:/certs/ca-bundle.crt" })
end sub

sub onState(event as object)
    state = event.getData()
    if m.info = invalid then return
    if state <> "paused" then m.pausedTimer.control = "stop"
    if state = "playing"
        wasPaused = m.top.state = "paused"
        m.played = true
        m.top.state = "playing"
        sleepWoken()
        if wasPaused then report("playing")
    else if state = "paused"
        m.top.state = "paused"
        report("paused")
        m.pausedTimer.control = "start"
    else if state = "finished"
        m.top.positionMs = m.top.durationMs
        ' The sleep timer ends with this track: the next one waits, paused.
        if sleepEndsWith(m.sleep, trackKey())
            sleepFired()
            if m.queue.at + 1 < m.queue.order.Count()
                showTrack(m.queue.at + 1, "paused")
                return
            end if
        end if
        nextTrack()
    else if state = "error"
        ' The file as it is doesn't play on this Roku: converted, once.
        if not m.played and m.mode <> "transcode" and httpString(m.info.method) <> "transcode"
            playTrack(m.queue.at, "transcode")
        else
            passOver("This track can't be played on this Roku.")
        end if
    end if
end sub

sub onPosition(event as object)
    seconds = event.getData()
    if seconds = invalid or m.info = invalid then return
    m.top.positionMs = Int(seconds * 1000)
    if playerShouldReport(m.audio.state, m.clock.TotalMilliseconds() - m.lastReportAt) then report("playing")
end sub

sub onControl()
    action = m.top.control
    if m.queue.order.Count() = 0 then return
    if action = "toggle"
        if m.top.state = "paused"
            action = "resume"
        else
            action = "pause"
        end if
    end if
    if action = "resume" or action = "next" or action = "previous" then sleepWoken()
    if action = "pause"
        m.audio.control = "pause"
    else if action = "resume" and m.waiting
        playTrack(m.queue.at, "auto")
    else if action = "resume"
        m.audio.control = "resume"
    else if action = "next"
        nextTrack()
    else if action = "previous"
        playTrack(musicPreviousAt(m.queue.at, m.top.positionMs), "auto")
    else if action = "shuffle" or action = "unshuffle"
        ' The track plays on; its place in the new order is told after it.
        musicShuffle(m.queue, action = "shuffle")
        showQueue()
        m.top.at = m.queue.at
    else if action = "sleep"
        ' The timer's button: Keep listening in its last 30 s, else the next
        ' choice.
        if sleepNow().secondsLeft >= 0
            sleepExtend(m.sleep, trackKey())
        else
            setSleep(sleepNextMinutes(m.sleep.minutes))
        end if
        showSleep(sleepNow())
    else if action = "stop"
        stopAll()
    end if
end sub

sub nextTrack()
    if m.queue.at + 1 < m.queue.order.Count()
        playTrack(m.queue.at + 1, "auto")
    else
        stopAll()
    end if
end sub

' A track that can't be played: said, and the next one plays.
sub passOver(message as string)
    m.top.message = musicTrackAt(m.queue, m.queue.at).title + ": " + message
    nextTrack()
end sub

' Nothing plays any more: the queue goes, and the sleep timer with it.
' Paused, the progress goes on being told, so the session stays the
' player's.
sub onPausedTick()
    if m.audio.state = "paused" then report("paused")
end sub

sub stopAll()
    m.pausedTimer.control = "stop"
    finishTrack()
    m.attempt = m.attempt + 1
    m.audio.control = "stop"
    m.queue = musicQueue([], 0, false)
    m.waiting = false
    m.sleepPausedAt = -1#
    setSleep(0)
    showSleep(sleepNow())
    showQueue()
    m.top.state = "stopped"
end sub

' The track left: where it got to, and its session closed.
sub finishTrack()
    if m.sessionId = "" then return
    report("stopped")
    closeSession(m.sessionId)
    m.sessionId = ""
    m.info = invalid
end sub

sub report(state as string)
    if m.sessionId = "" then return
    m.lastReportAt = m.clock.TotalMilliseconds()
    track = musicTrackAt(m.queue, m.queue.at)
    uiRequest({ method: "POST", path: "/api/items/" + urlValue(track.itemId) + "/progress", json: playerProgress(m.top.positionMs, m.top.durationMs, state, m.sessionId) }, "")
end sub

sub closeSession(sessionId as string)
    if sessionId = "" then return
    uiRequest({ method: "DELETE", path: "/api/playback/" + urlEncode(sessionId) }, "")
end sub

' --- The sleep timer ---

function clockMs() as double
    return m.clock.TotalMilliseconds() * 1#
end function

' The track playing, as the timer tells tracks apart.
function trackKey() as string
    if m.queue.order.Count() = 0 then return ""
    return urlValue(musicTrackAt(m.queue, m.queue.at).itemId)
end function

' What the timer says now.
function sleepNow() as object
    left = -1#
    if m.played and m.top.durationMs > 0 then left = (m.top.durationMs - m.top.positionMs) * 1#
    return sleepState(m.sleep, clockMs(), left, trackKey())
end function

' Set from Now Playing (minutes; 0 for Off, -1 for the end of the track).
sub setSleep(minutes as integer)
    m.sleep = sleepNew(minutes, clockMs())
    if minutes = 0 and m.sleepPausedAt < 0
        m.sleepTimer.control = "stop"
    else
        m.sleepTimer.control = "start"
    end if
end sub

' Once a second: the countdown of the last 30 s, the pause at the end,
' and the music stopped 10 minutes into that pause.
sub onSleepTick()
    if m.sleepPausedAt >= 0
        if clockMs() - m.sleepPausedAt >= sleepLeaveAfterMs() then stopAll()
        return
    end if
    state = sleepNow()
    if not state.fire
        showSleep(state)
        return
    end if
    sleepFired()
    ' A track still loading would start once it has: it waits instead.
    if m.top.state = "loading"
        showTrack(m.queue.at, "paused")
    else
        m.audio.control = "pause"
    end if
end sub

' The timer's end: it goes off, and the pause it makes is timed.
sub sleepFired()
    m.sleep = sleepNew(0, 0#)
    m.sleepPausedAt = clockMs()
    m.sleepTimer.control = "start"
    showSleep(sleepNow())
end sub

' Played again after the timer's pause: the music stays.
sub sleepWoken()
    if m.sleepPausedAt < 0 then return
    m.sleepPausedAt = -1#
    if m.sleep.minutes = 0 then m.sleepTimer.control = "stop"
end sub

' What Now Playing shows of the timer, told when it changes.
sub showSleep(state as object)
    shown = { button: sleepButtonLabel(m.sleep, state), secondsLeft: state.secondsLeft }
    text = FormatJson(shown)
    if text = m.sleepShown then return
    m.sleepShown = text
    m.top.sleep = shown
end sub
