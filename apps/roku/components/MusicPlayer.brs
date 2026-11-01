' The music player (docs/API.md, "Playback"): each track gets a session
' (POST /api/playback, the Roku's capabilities with it), plays with the
' scene's Audio node (the token in its HTTP agent, renewed as the token
' is), reports progress about every 10 s, on pause and as "stopped" when
' it ends or another starts, and its session is closed then. A track that
' doesn't play as it is plays again converted, once; one that doesn't play
' at all is passed over. The end of the queue stops it. The queue keeps
' the order it was given in (lib/Music.brs), so Stop shuffling puts it
' back.

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
end sub

sub onToken()
    m.audio.setHttpAgent(uiHttpAgent())
end sub

sub onPlay()
    request = m.top.play
    if request = invalid or GetInterface(request.queue, "ifArray") = invalid or request.queue.Count() = 0 then return
    ' The track playing ends before the new queue replaces it.
    finishTrack()
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
    if at >= 0 and at < m.queue.order.Count() then playTrack(at, "auto")
end sub

' The track at, from its start; its session and its details are asked for
' together (the details say the file's container).
sub playTrack(at as integer, mode as string)
    finishTrack()
    m.queue.at = at
    m.mode = mode
    m.attempt = m.attempt + 1
    m.info = invalid
    m.detail = invalid
    m.played = false
    track = musicTrackAt(m.queue, at)
    m.top.track = track
    m.top.at = at
    m.top.positionMs = 0
    m.top.durationMs = track.durationMs
    m.top.state = "loading"
    m.audio.control = "stop"
    uiRequest({ op: "play", itemId: track.itemId, startMs: 0, mode: mode, attempt: m.attempt }, "onSession")
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(track.itemId), attempt: m.attempt }, "onDetail")
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
    if state = "playing"
        wasPaused = m.top.state = "paused"
        m.played = true
        m.top.state = "playing"
        if wasPaused then report("playing")
    else if state = "paused"
        m.top.state = "paused"
        report("paused")
    else if state = "finished"
        m.top.positionMs = m.top.durationMs
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
    if action = "pause"
        m.audio.control = "pause"
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

sub stopAll()
    finishTrack()
    m.attempt = m.attempt + 1
    m.audio.control = "stop"
    m.queue = musicQueue([], 0, false)
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
