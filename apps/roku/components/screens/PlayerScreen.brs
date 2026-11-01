' The player (docs/API.md, "Playback"). Progress goes to the server about
' every 10 s while playing, on pause, and as "stopped" on the way out, then
' the session is closed. A stream that breaks is checked with the server:
' stopped by an administrator (410), the player leaves and says so;
' otherwise it starts again where it was, with a new session and the
' current token (a token that ran out answers 401 to the Video node).
' Skip Intro and Skip Credits follow the web player: an episode's credits
' bring up Up Next instead. Down brings up pley's menu: Quality, subtitle
' sync for a file with text subtitles (SubtitleSync.brs), the file's
' chapters, the sleep timer (Sleep.brs) and the playback's details
' (Details.brs). Subtitles (or audio) the person picks
' in Roku's options are remembered on the server for the show or the movie
' (Tracks.brs), so the next episode starts with matching ones. The quality
' is the one chosen in Settings or the menu; when it doesn't play well,
' Automatic lowers a conversion's bitrate as it is sent, or opens the
' stream again lower where it was once it stopped anyway (AutoQuality.brs).

sub init()
    m.video = m.top.findNode("video")
    m.skip = m.top.findNode("skip")
    m.loading = m.top.findNode("loading")
    m.upNext = m.top.findNode("upNext")
    m.nextImage = m.top.findNode("nextImage")
    m.nextTitle = m.top.findNode("nextTitle")
    m.nextLine = m.top.findNode("nextLine")
    m.countdown = m.top.findNode("countdown")
    m.nextButtons = m.top.findNode("nextButtons")
    m.notice = m.top.findNode("notice")
    m.noticeTitle = m.top.findNode("noticeTitle")
    m.noticeText = m.top.findNode("noticeText")
    m.noticeButtons = m.top.findNode("noticeButtons")
    m.countdownTimer = m.top.findNode("countdownTimer")
    m.sync = m.top.findNode("sync")
    m.syncValue = m.top.findNode("syncValue")
    m.syncButtons = m.top.findNode("syncButtons")
    m.menu = m.top.findNode("menu")
    m.menuTitle = m.top.findNode("menuTitle")
    m.menuList = m.top.findNode("menuList")
    m.menuChoices = m.top.findNode("menuChoices")
    m.menuText = m.top.findNode("menuText")
    m.menuHint = m.top.findNode("menuHint")
    ' What the menu shows ("menu", "quality", "chapters", "details"), the
    ' actions of its entries, and the values of a list of choices.
    m.menuKind = ""
    m.menuActions = []
    m.menuValues = []
    ' Find subtitles…: the results listed ([{ id, label, hint }]).
    m.found = []
    m.menuList.observeField("itemSelected", "onMenuEntry")
    m.menuList.observeField("itemFocused", "onMenuEntryFocused")
    m.menuChoices.observeField("itemSelected", "onMenuChoice")
    m.menuChoices.observeField("itemFocused", "onMenuChoiceFocused")
    m.clock = CreateObject("roTimespan")
    m.started = false
    ' Which opening the answers belong to: answers for an earlier one are dropped.
    m.attempt = 0
    ' The playback session asked for last, until it answers.
    m.playRequest = invalid
    m.itemId = 0
    m.item = invalid
    m.info = invalid
    m.sessionId = ""
    m.mode = "auto"
    m.startMs = 0
    m.positionMs = 0
    m.durationMs = 0
    m.lastReportAt = 0
    m.paused = false
    ' Whether this opening's video has played yet (errors before are the file's).
    m.played = false
    m.next = invalid
    m.upNextAt = -1
    m.upNextShown = false
    m.upNextCancelled = false
    m.countdownLeft = 0
    m.restarts = []
    ' The caption mode before pley turned captions on for its subtitles.
    m.captionMode = invalid
    ' The file's intro and credits, and the one the playhead is in; a part
    ' just skipped keeps its button away until the playhead is past it.
    m.markers = []
    m.skipMarker = invalid
    m.skipKey = ""
    m.skipDismissed = false
    ' Subtitle sync: the file's offset, under what it is remembered, the
    ' offset when the panel opened, and how many text tracks there are.
    m.syncMs = 0
    m.syncKey = ""
    m.syncWas = 0
    m.textTracks = 0
    ' The tracks shown (stream indexes; -1 none, -2 not known yet), the
    ' audio played, and from when (clock ms) a change is the person's: -1
    ' until the stream played.
    m.subtitleShown = -1
    m.audioShown = -2
    m.audioPlaying = -1
    m.tracksFrom = -1
    ' Automatic (docs/QUALITY.md): how this opening was asked for
    ' (aqAsk), the title's watch, the quality Automatic opens the title
    ' at again ("" for the setting's choice), and what to say once it plays.
    m.asked = invalid
    m.watch = invalid
    m.watchItem = invalid
    m.autoQuality = ""
    m.autoNotice = ""
    m.toast = m.top.findNode("toast")
    m.toastText = m.top.findNode("toastText")
    m.toastTimer = m.top.findNode("toastTimer")
    m.toastTimer.observeField("fire", "onToastTimer")
    m.autoTimer = m.top.findNode("autoTimer")
    m.autoTimer.observeField("fire", "onAutoTick")
    ' The sleep timer, off until set in the menu, for the whole time the
    ' player shows; when it paused the video (clock ms), -1 otherwise.
    m.sleep = sleepNew(0, 0#)
    m.sleepPausedAt = -1#
    m.sleepNotice = m.top.findNode("sleepNotice")
    m.sleepText = m.top.findNode("sleepText")
    m.sleepButtons = m.top.findNode("sleepButtons")
    m.sleepButtons.buttons = ["Keep watching"]
    m.sleepButtons.observeField("buttonSelected", "onKeepWatching")
    m.sleepTimer = m.top.findNode("sleepTimer")
    m.sleepTimer.observeField("fire", "onSleepTick")
    m.video.observeField("downloadedSegment", "onSegment")
    m.syncButtons.buttons = ["Done", "Reset"]
    m.syncButtons.observeField("buttonSelected", "onSyncButton")
    m.video.notificationInterval = 1
    m.video.observeField("state", "onState")
    m.video.observeField("position", "onPosition")
    for each field in ["currentSubtitleTrack", "globalCaptionMode", "currentAudioTrack", "availableAudioTracks"]
        m.video.observeField(field, "onTracks")
    end for
    m.skip.observeField("pressed", "onSkip")
    m.skip.observeField("playKey", "onSkipKey")
    m.nextButtons.buttons = ["Play now", "Cancel"]
    m.nextButtons.observeField("buttonSelected", "onNextButton")
    m.noticeButtons.observeField("buttonSelected", "onNoticeButton")
    m.countdownTimer.observeField("fire", "onCountdown")
    m.top.observeField("active", "onActive")
    m.top.observeField("serverStopped", "onServerStopped")
end sub

' This playback's session stopped from elsewhere: it ends now, not at its
' next report.
sub onServerStopped(event as object)
    data = event.getData()
    if m.sessionId <> "" and httpString(data.session_id) = m.sessionId then stopped(data)
end sub

sub onActive()
    if not m.top.active
        ' Leaving: Back, or the sign-in ended.
        finish()
        return
    end if
    if m.started then return
    m.started = true
    ' A video takes over from the music.
    m.global.music.control = "stop"
    ' Playlists and segments carry the token: a refreshed one goes to the
    ' Video node's agent at once.
    m.global.session.observeFieldScoped("accessToken", "onToken")
    params = m.top.params
    ' A playlist's entries, this one first: Up Next brings on each next.
    m.queue = invalid
    m.queueAt = 0
    if GetInterface(params.queue, "ifArray") <> invalid then m.queue = params.queue
    newTitle()
    open(params.itemId, Int(httpNumber(params.startMs, 0)), params.item, "auto")
end sub

' Another title: Automatic starts from the setting again.
sub newTitle()
    m.watch = invalid
    m.watchItem = invalid
    m.autoQuality = ""
    m.autoNotice = ""
    m.autoTimer.control = "stop"
end sub

sub onToken()
    m.video.setHttpAgent(uiHttpAgent())
end sub

' Plays itemId from startMs. With its details already there (item), they
' and a playback session are asked for together; else the details first,
' since its file says which subtitles to ask the server to draw.
sub open(itemId as dynamic, startMs as integer, item as dynamic, mode as string)
    abandonPlayback()
    m.attempt = m.attempt + 1
    m.itemId = itemId
    m.startMs = startMs
    m.positionMs = startMs
    m.mode = mode
    m.item = item
    m.info = invalid
    m.sessionId = ""
    m.paused = false
    m.played = false
    m.next = invalid
    m.upNextShown = false
    m.upNextCancelled = false
    m.markers = []
    m.skipMarker = invalid
    m.skipKey = ""
    m.skipDismissed = false
    m.textTracks = 0
    m.tracksFrom = -1
    m.sync.visible = false
    closeMenu()
    hideSkip()
    hideUpNext()
    m.notice.visible = false
    m.loading.visible = true
    m.top.setFocus(true)
    if m.item = invalid
        uiRequest({ method: "GET", path: "/api/items/" + urlValue(itemId), attempt: m.attempt }, "onItem")
        return
    end if
    itemKnown()
    askPlayback()
end sub

' POST /api/playback for the first file (the server's default too), with
' its picture subtitles to draw when the person's choices pick some, at the
' quality chosen in Settings (or Automatic's). A title's first opening in
' Automatic asks the server what it knows of the network first.
sub askPlayback()
    choice = aqChoice(playbackSection())
    if choice = "auto" and m.autoQuality = ""
        startAutomatic()
        return
    end if
    requestPlayback(choice, 0)
end sub

' Where Automatic starts (AutoQuality.brs): the server's word on this
' network, asked 1.5 s at most (the API task keeps it 10 minutes, so a play
' at home doesn't wait), after a speed test where the server asks for one;
' without an answer, what this Roku remembers. The loading circle shows
' meanwhile.
sub startAutomatic()
    m.networkAt = m.clock.TotalMilliseconds()
    m.probe = invalid
    uiRequest({ op: "network", attempt: m.attempt }, "onNetwork")
end sub

sub onNetwork(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if not answer.ok
        requestPlayback("auto", aqRemembered(playbackSection(), m.global.session.server, nowSeconds()))
        return
    end if
    if aqWantsProbe(answer.data)
        timeout = aqProbeTimeoutMs(m.clock.TotalMilliseconds() - m.networkAt)
        uiRequest({ op: "speedTest", timeoutMs: timeout, attempt: m.attempt }, "onSpeedTest")
        return
    end if
    requestPlayback("auto", aqNetworkCeiling(answer.data))
end sub

' The speed test's bytes over its time go to the server, whose answer is
' the ceiling; too little measured starts as if nothing were known.
sub onSpeedTest(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    result = invalid
    if answer.ok and GetInterface(answer.data, "ifAssociativeArray") <> invalid then result = aqProbeResult(httpNumber(answer.data.bytes, 0), httpNumber(answer.data.ms, 0))
    if result = invalid
        requestPlayback("auto", 0)
        return
    end if
    m.probe = result
    uiRequest({ op: "tellNetwork", probeKbps: result.probe_kbps, timeoutMs: 1000, attempt: m.attempt }, "onNetworkTold")
end sub

sub onNetworkTold(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    ceiling = aqProbeCeiling(m.probe)
    if answer.ok and GetInterface(answer.data, "ifAssociativeArray") <> invalid then ceiling = aqNetworkCeiling(answer.data)
    requestPlayback("auto", ceiling)
end sub

' The session at the choice's quality; ceilingKbps is where Automatic
' starts under (0 for none).
sub requestPlayback(choice as string, ceilingKbps as float)
    ' Keyed addresses: Roku fetches the seek previews itself, without the
    ' Authorization header.
    spec = { op: "play", itemId: m.itemId, startMs: m.startMs, mode: m.mode, attempt: m.attempt, keyed: true }
    file = contentFile(m.item, invalid)
    if file <> invalid
        spec.fileId = httpNumber(file.id, 0)
        burn = playerBurnIndex(file)
        if burn >= 0 then spec.subtitleIndex = burn
    end if
    again = ""
    if choice = "auto" then again = m.autoQuality
    m.asked = aqAsk(choice, again, ceilingKbps, file)
    ' A file that failed to play as it is stays converted (broken()).
    if m.mode <> "transcode" then spec.mode = m.asked.mode
    spec.maxBitrate = m.asked.maxBitrate
    spec.maxHeight = m.asked.maxHeight
    m.playRequest = uiRequest(spec, "onPlayback")
end sub

' Where the quality setting and Automatic's ceilings are kept on this Roku.
function playbackSection() as object
    return CreateObject("roRegistrySection", "playback")
end function

function nowSeconds() as integer
    return CreateObject("roDateTime").AsSeconds()
end function

' Automatic's ceiling for the next titles (kbps, 0 forgets it): kept on
' this Roku for 30 minutes and told to the server, which keeps it a day
' for this Roku on this network. Nothing waits for the server's answer.
sub keepCeiling(kbps as float)
    aqRemember(playbackSection(), m.global.session.server, kbps, nowSeconds())
    uiRequest({ op: "tellNetwork", ceilingKbps: kbps }, "")
end sub

' The session asked for isn't wanted any more: the task closes it if it
' comes (and onPlayback does, if it came already).
sub abandonPlayback()
    if m.playRequest = invalid then return
    m.playRequest.abandoned = true
    m.playRequest = invalid
end sub

function isCurrent(event as object) as boolean
    return event.getRoSGNode().spec.attempt = m.attempt
end function

sub onItem(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        failed(answer)
        return
    end if
    m.item = answer.data
    itemKnown()
    askPlayback()
end sub

' An episode has a next one to offer before its end; a playlist's entry,
' the entry after it.
sub itemKnown()
    if m.queue <> invalid
        if m.queueAt + 1 < m.queue.Count() then m.next = m.queue[m.queueAt + 1]
        return
    end if
    if httpString(m.item.kind) <> "episode" then return
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(m.itemId) + "/next", attempt: m.attempt }, "onNext")
end sub

sub onPlayback(event as object)
    answer = event.getData()
    if not isCurrent(event)
        ' For an opening left meanwhile (Back while it started): the server
        ' would go on preparing it.
        if answer.ok and answer.data <> invalid then closeSession(httpString(answer.data.session_id))
        return
    end if
    m.playRequest = invalid
    if not answer.ok or answer.data = invalid
        failed(answer)
        return
    end if
    m.info = answer.data
    m.sessionId = httpString(m.info.session_id)
    begin()
end sub

' Plays once both the item and the session are there.
sub begin()
    if m.item = invalid or m.info = invalid then return
    m.durationMs = httpNumber(m.info.duration_ms, 0)
    m.markers = playerFileMarkers(m.item, m.info.file_id)
    m.upNextAt = playerUpNextAt(m.durationMs, m.markers)
    ' The sync remembered for this file moves every text track.
    m.syncKey = syncKey(m.global.session.server, m.info.file_id)
    m.syncMs = syncOffset(syncSection(), m.syncKey)
    stream = contentStream(m.info, m.item, m.global.session.server, m.startMs)
    m.textTracks = stream.subtitleTracks.Count()
    ' What pley shows: what the Roku shows until the stream played.
    m.subtitleShown = tracksSubtitleShown("On", stream.subtitle, m.info.subtitles, m.global.session.server)
    m.audioShown = -2
    m.audioPlaying = tracksAudioPlaying(m.info, contentFile(m.item, m.info.file_id))
    m.tracksFrom = -1
    showStream(stream, syncTrackUrl(stream.subtitle, m.syncMs))
    if stream.subtitle <> ""
        captionsOn()
    else
        captionsBack()
    end if
    m.video.control = "play"
    m.video.setFocus(true)
    m.lastReportAt = m.clock.TotalMilliseconds()
    watchPlayback()
end sub

' Automatic's watch over what now plays: the title's own after a restart,
' told the quality it plays at now (the original when the server played
' the file as it is), the bitrate that is, the server's own quality for
' the file, the lines the stream is named by, whether its bitrate changes
' as it is sent (live_bitrate) and whether it comes in several sizes.
sub watchPlayback()
    if m.asked = invalid or not m.asked.automatic or httpString(m.item.kind) = "track"
        newTitle()
        return
    end if
    file = contentFile(m.item, m.info.file_id)
    fileKbps = aqFileKbps(file)
    fileLines = aqFileLines(file)
    converted = httpString(m.info.method) = "transcode"
    cap = httpString(m.info.auto_quality)
    quality = m.asked.quality
    if not converted then quality = "original"
    if quality = "original"
        playing = fileKbps
        if converted and aqKbps(cap) > 0 then playing = aqKbps(cap)
    else
        playing = aqKbps(quality)
        if fileKbps > 0 and fileKbps < playing then playing = fileKbps
    end if
    live = converted and httpTrue(m.info.live_bitrate)
    lines = aqStreamLines(quality, converted, cap, aqNamedLines(aqFileWidth(file), fileLines))
    now = m.clock.TotalMilliseconds()
    if m.watch <> invalid and m.watchItem = m.itemId
        aqRestarted(m.watch, now, quality, playing, live, lines, converted)
    else
        m.watch = aqWatch(now, quality, playing, fileKbps, fileLines, cap, m.asked.lowered and quality <> "original", live, lines, converted)
        m.watchItem = m.itemId
    end if
    m.watch.adaptive = adaptiveStream()
    m.autoTimer.control = "start"
end sub

' The Video node's state, for Automatic: a wait while it plays (one the
' Roku says is for want of data, where it says), and the notice of a step
' down once the stream plays.
sub watchState(state as string)
    if m.watch = invalid then return
    if state = "playing" and m.autoNotice <> ""
        showToast(m.autoNotice)
        m.autoNotice = ""
    end if
    underrun = invalid
    if state = "buffering"
        status = m.video.bufferingStatus
        if GetInterface(status, "ifAssociativeArray") <> invalid and GetInterface(status.isUnderrun, "ifBoolean") <> invalid then underrun = status.isUnderrun
    end if
    decision = aqState(m.watch, m.clock.TotalMilliseconds(), state, underrun)
    if decision <> invalid then autoStep(decision)
end sub

sub onAutoTick()
    if m.watch = invalid or m.info = invalid then return
    decision = aqTick(m.watch, m.clock.TotalMilliseconds())
    if decision <> invalid then autoStep(decision)
end sub

' The stream comes in several sizes the Video node moves between by itself,
' as the connection allows: Automatic leaves its waits to it, and only
' raises where the next title starts.
function adaptiveStream() as boolean
    if m.info = invalid then return false
    flag = m.info.adaptive
    return GetInterface(flag, "ifBoolean") <> invalid and flag
end function

' Each HLS segment the Video node downloaded: its size over the time it
' took is what the network carried.
sub onSegment(event as object)
    if m.watch = invalid then return
    segment = event.getData()
    if GetInterface(segment, "ifAssociativeArray") = invalid then return
    ' 1 is audio alone and 3 captions: the video's are muxed (0) or video (2).
    kind = Int(httpNumber(segment.SegType, 0))
    if kind = 1 or kind = 3 then return
    aqSegment(m.watch, m.clock.TotalMilliseconds(), httpNumber(segment.SegSize, 0), httpNumber(segment.DownloadDuration, 0))
end sub

' Carries out Automatic's step (aqStep). The network's memory follows each
' (a step up to a quality keeps its bitrate too;
' back at the original, it is forgotten). A step down says so: at once
' when the stream plays on, else once the new one plays.
sub autoStep(decision as object)
    if decision.memory >= 0 then keepCeiling(decision.memory)
    if decision.kind = "next" then return
    if decision.kind = "bitrate"
        if decision.down then showToast("Quality lowered to " + aqBitrateLabel(decision.kbps, m.watch.lines))
        setBitrate(decision.kbps)
        return
    end if
    if decision.down then m.autoNotice = "Quality lowered to " + aqLabel(decision.quality)
    mode = m.mode
    quality = decision.quality
    if quality = "converted"
        ' The original converted, at the server's own quality.
        quality = "original"
        mode = "transcode"
    end if
    m.autoQuality = quality
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    m.autoTimer.control = "stop"
    open(m.itemId, position, m.item, mode)
end sub

' PUT /api/playback/{session}/bitrate: the stream goes on at kbps from the
' segments the Video node hasn't fetched yet, the same size and playlist,
' with no new stream (docs/QUALITY.md, "Players that go black between
' streams").
sub setBitrate(kbps as integer)
    if m.sessionId = "" then return
    path = "/api/playback/" + urlEncode(m.sessionId) + "/bitrate"
    uiRequest({ method: "PUT", path: path, json: { max_bitrate: kbps }, session: m.sessionId, kbps: kbps }, "onBitrate")
end sub

' What the stream goes on at, or that the server refused (one from before,
' a session gone): the watch then moves it as a stream whose bitrate can't
' change.
sub onBitrate(event as object)
    spec = event.getRoSGNode().spec
    if m.watch = invalid or spec.session <> m.sessionId then return
    answer = event.getData()
    kbps = 0
    if answer.ok
        kbps = httpNumber(spec.kbps, 0)
        if GetInterface(answer.data, "ifAssociativeArray") <> invalid then kbps = httpNumber(answer.data.bitrate, kbps)
    end if
    aqAnswered(m.watch, httpNumber(spec.kbps, 0), kbps)
end sub

sub showToast(text as string)
    m.toastText.text = text
    m.toast.visible = true
    m.toastTimer.control = "stop"
    m.toastTimer.control = "start"
end sub

sub onToastTimer()
    m.toast.visible = false
end sub

' Gives the Video node stream, its text tracks moved as the sync says,
' showing the track subtitle ("" leaves it to the Roku).
sub showStream(stream as object, subtitle as string)
    content = CreateObject("roSGNode", "ContentNode")
    content.url = stream.url
    content.streamFormat = stream.streamFormat
    content.title = stream.title
    content.playStart = stream.playStart
    content.subtitleTracks = syncTracks(stream.subtitleTracks, m.syncMs)
    if stream.previews <> ""
        content.SDBifUrl = stream.previews
        content.HDBifUrl = stream.previews
        content.FHDBifUrl = stream.previews
    end if
    if LCase(Left(stream.url, 8)) = "https://" then nodesFill(content, { HttpCertificatesFile: "common:/certs/ca-bundle.crt" })
    m.video.setHttpAgent(uiHttpAgent())
    m.video.content = content
    if subtitle <> "" then m.video.subtitleTrack = subtitle
end sub

' Where subtitle sync is remembered on this Roku.
function syncSection() as object
    return CreateObject("roRegistrySection", "subtitleSync")
end function

' pley's menu is offered while a stream plays; subtitle sync in it for a
' file with text subtitles.
function menuOffered() as boolean
    return m.info <> invalid and not m.notice.visible and not m.upNext.visible and not m.sync.visible
end function

sub openSync()
    hideSkip()
    m.syncWas = m.syncMs
    m.syncValue.text = syncLabel(m.syncMs)
    m.sync.visible = true
    m.syncButtons.focusButton = 0
    m.syncButtons.setFocus(true)
end sub

sub stepSync(byMs as integer)
    m.syncMs = syncClamp(m.syncMs + byMs)
    m.syncValue.text = syncLabel(m.syncMs)
end sub

sub onSyncButton(event as object)
    if m.syncButtons.buttons[event.getData()] = "Reset"
        stepSync(-m.syncMs)
    else
        closeSync()
    end if
end sub

' Done or Back: a new offset is remembered for the file, and the stream
' opens again where it was, its text tracks moved by the server.
sub closeSync()
    if not m.sync.visible then return
    m.sync.visible = false
    m.video.setFocus(true)
    if m.syncMs = m.syncWas then return
    syncRemember(syncSection(), m.syncKey, m.syncMs)
    reopen()
end sub

' The stream again from where it is: the same session and address, the
' subtitles that showed shown again, moved (or the track at subtitle).
sub reopen(subtitle = "" as string)
    if m.item = invalid or m.info = invalid then return
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    showing = subtitle
    if showing = "" then showing = syncBaseUrl(httpString(m.video.currentSubtitleTrack))
    if showing = "" then showing = syncBaseUrl(httpString(m.video.subtitleTrack))
    m.positionMs = position
    m.loading.visible = true
    ' The tracks as the stream opens again are pley's doing.
    m.tracksFrom = -1
    showStream(contentStream(m.info, m.item, m.global.session.server, position), syncTrackUrl(showing, m.syncMs))
    m.video.control = "play"
end sub

' Roku shows a subtitle track only while captions are on, a setting of
' the whole Roku: the subtitles pley picks turn them on for its playback,
' and they go back as they were after, unless the person changed them
' meanwhile.
sub captionsOn()
    if m.captionMode = invalid then m.captionMode = m.video.globalCaptionMode
    m.video.globalCaptionMode = "On"
end sub

sub captionsBack()
    if m.captionMode = invalid then return
    if m.video.globalCaptionMode = "On" then m.video.globalCaptionMode = m.captionMode
    m.captionMode = invalid
end sub

' Captions turned on or off, or another subtitle or audio track, in Roku's
' options: once the stream played, the person's pick, remembered on the
' server for the show or the movie. Before, what shows is pley's choice or
' the Roku's own, and only noted.
sub onTracks()
    if m.info = invalid or m.item = invalid then return
    file = contentFile(m.item, m.info.file_id)
    subtitle = tracksSubtitleShown(httpString(m.video.globalCaptionMode), httpString(m.video.currentSubtitleTrack), m.info.subtitles, m.global.session.server)
    ' A converted stream has one audio track; a file played as it is, all.
    audio = -2
    if not httpTrue(m.info.hls) then audio = tracksAudioShown(m.video.availableAudioTracks, tracksTrackId(m.video.currentAudioTrack), file)
    subtitlePicked = subtitle <> -2 and subtitle <> m.subtitleShown
    if subtitlePicked then m.subtitleShown = subtitle
    ' The first audio the Roku tells of is the one it started with.
    audioPicked = audio <> -2 and m.audioShown <> -2 and audio <> m.audioShown
    if audio <> -2
        m.audioShown = audio
        m.audioPlaying = audio
    end if
    if not subtitlePicked and not audioPicked then return
    if m.tracksFrom < 0 or m.clock.TotalMilliseconds() < m.tracksFrom then return
    saveTracks(file, subtitlePicked, audioPicked)
end sub

' PUT /api/items/{id}/streams with the tracks playing; kept in the file too,
' so a stream started again shows them. Text subtitles picked over picture
' ones the server draws need a stream without those.
sub saveTracks(file as dynamic, subtitlePicked as boolean, audioPicked as boolean)
    burned = -1
    if httpTrue(m.info.burn_subtitles) then burned = Int(httpNumber(m.info.subtitle_index, -1))
    subtitle = m.subtitleShown
    if not subtitlePicked and subtitle < 0 then subtitle = burned
    ' The audio only when it was picked: subtitles alone keep the audio picked before.
    audio = -2
    if audioPicked then audio = m.audioPlaying
    body = tracksStreamsBody(m.info.file_id, audio, subtitle)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.itemId) + "/streams", jsonText: FormatJson(body) }, "")
    if file <> invalid
        if body.DoesExist("audio_index") then file.selected_audio = body.audio_index
        file.selected_subtitle = body.subtitle_index
    end if
    if not subtitlePicked or not tracksNeedNewSession(burned, subtitle) then return
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    open(m.itemId, position, m.item, m.mode)
end sub

' A request before playback failed: the server doesn't answer (its own
' screen), the sign-in ended (the scene shows sign-in), or a refusal.
sub failed(answer as object)
    if answer.error = "signed_out" then return
    if answer.error = "network"
        uiUnreachable({ to: "player", params: { itemId: m.itemId, startMs: m.positionMs } })
        return
    end if
    notice("Can't play this item", answer.message, ["Back"])
end sub

sub onState(event as object)
    state = event.getData()
    watchState(state)
    if state = "playing"
        m.played = true
        m.loading.visible = false
        ' Played again after the sleep timer's pause: the player stays.
        if m.sleepPausedAt >= 0
            m.sleepPausedAt = -1#
            if m.sleep.minutes = 0 then m.sleepTimer.control = "stop"
        end if
        if m.tracksFrom < 0 then m.tracksFrom = m.clock.TotalMilliseconds() + tracksSettleMs()
        if m.paused
            m.paused = false
            report("playing")
        end if
        updateSkip()
        return
    end if
    ' Paused, in trick play or waiting: the Video node's own controls.
    hideSkip()
    if state = "paused"
        m.paused = true
        report("paused")
    else if state = "finished"
        ended()
    else if state = "error"
        broken()
    end if
end sub

sub onPosition(event as object)
    seconds = event.getData()
    if seconds = invalid then return
    m.positionMs = Int(seconds * 1000)
    if m.watch <> invalid then aqPosition(m.watch, m.clock.TotalMilliseconds(), m.positionMs)
    if playerShouldReport(m.video.state, m.clock.TotalMilliseconds() - m.lastReportAt) then report("playing")
    if m.next <> invalid and not m.upNextShown and m.upNextAt > 0 and m.positionMs >= m.upNextAt and not sleepEndsWith(m.sleep, urlValue(m.itemId)) then showUpNext()
    updateSkip()
end sub

' Skip Intro or Skip Credits while the playhead is in one and the video
' plays.
sub updateSkip()
    marker = playerSkipFor(m.markers, m.positionMs, m.next <> invalid and not m.upNextCancelled)
    key = ""
    if marker <> invalid then key = marker.key
    if key <> m.skipKey
        m.skipKey = key
        m.skipDismissed = false
        if marker <> invalid then m.skip.text = playerSkipLabel(marker)
    end if
    m.skipMarker = marker
    shown = marker <> invalid and not m.skipDismissed and m.video.state = "playing" and not m.upNext.visible and not m.notice.visible and not m.sync.visible and not m.menu.visible
    if shown = m.skip.visible then return
    if shown
        m.skip.visible = true
        m.skip.setFocus(true)
    else
        hideSkip()
    end if
end sub

sub hideSkip()
    if m.skip.hasFocus() then m.video.setFocus(true)
    m.skip.visible = false
end sub

sub onSkip()
    marker = m.skipMarker
    if marker = invalid then return
    ' Until the playhead is past it, the button stays away.
    m.skipDismissed = true
    hideSkip()
    target = playerSkipTarget(marker, m.durationMs)
    if target < 0
        ended()
        return
    end if
    seekTo(target)
end sub

' Play, fast forward or rewind while the skip button has the focus.
sub onSkipKey(event as object)
    hideSkip()
    playerKey(event.getData())
end sub

' What play, fast forward and rewind do when the skip button or Up Next
' has the focus instead of the video: pause or go on, or 10 s further or
' back.
sub playerKey(key as string)
    if key = "play"
        if m.video.state = "paused"
            m.video.control = "resume"
        else
            m.video.control = "pause"
        end if
    else if key = "fastforward"
        seekTo(m.positionMs + 10000)
    else
        seekTo(m.positionMs - 10000)
    end if
end sub

sub seekTo(targetMs as integer)
    if m.watch <> invalid then aqSeeked(m.watch)
    target = targetMs
    last = Int(m.durationMs) - 1000
    if last > 0 and target > last then target = last
    if target < 0 then target = 0
    m.positionMs = target
    m.video.seek = target / 1000
end sub

' POST /api/items/{id}/progress for the current session. Stopped, the
' screens showing the title follow (Ui.brs's uiNoteChange).
sub report(state as string)
    if m.sessionId = "" then return
    if state = "stopped" then uiNoteChange({ itemId: m.itemId, action: "played", positionMs: m.positionMs, durationMs: m.durationMs, watched: playerWatched(m.positionMs, m.durationMs) })
    m.lastReportAt = m.clock.TotalMilliseconds()
    path = "/api/items/" + urlValue(m.itemId) + "/progress"
    uiRequest({ method: "POST", path: path, json: playerProgress(m.positionMs, m.durationMs, state, m.sessionId), session: m.sessionId }, "onReported")
end sub

sub onReported(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId then return
    answer = event.getData()
    if answer.status = 410 then stopped(answer.data)
end sub

' DELETE /api/playback/{session}: the server stops any conversion.
sub endSession()
    closeSession(m.sessionId)
    m.sessionId = ""
end sub

sub closeSession(sessionId as string)
    if sessionId = "" then return
    uiRequest({ method: "DELETE", path: "/api/playback/" + urlEncode(sessionId) }, "")
end sub

sub ended()
    if m.durationMs > 0 then m.positionMs = m.durationMs
    report("stopped")
    endSession()
    ' The sleep timer's end: nothing plays after this one.
    if sleepEndsWith(m.sleep, urlValue(m.itemId))
        uiNavigate({ back: true })
        return
    end if
    if m.next <> invalid and not m.upNextCancelled and playerAutoplay(playbackSection())
        playNext()
    else
        uiNavigate({ back: true })
    end if
end sub

' Leaving the player: the place is kept and the session closed (or, still
' to come, closed when it does).
sub finish()
    abandonPlayback()
    m.attempt = m.attempt + 1
    m.tracksFrom = -1
    m.countdownTimer.control = "stop"
    m.sleepTimer.control = "stop"
    m.sleepNotice.visible = false
    newTitle()
    m.toastTimer.control = "stop"
    m.toast.visible = false
    if m.sessionId <> ""
        report("stopped")
        endSession()
    end if
    m.video.control = "stop"
    captionsBack()
    if m.started then m.global.session.unobserveFieldScoped("accessToken")
    ' Where titles were left changed: screens that show it ask again.
    scene = m.top.getScene()
    scene.played = scene.played + 1
end sub

' The server stopped the stream: the player leaves, and the scene says why
' over the screen before.
sub stopped(data as dynamic)
    m.tracksFrom = -1
    m.sessionId = ""
    m.video.control = "stop"
    lines = playerStoppedMessage(data).Split(Chr(10))
    message = []
    if lines.Count() > 1 then message.Push(lines[1])
    uiNavigate({ back: true, message: { title: lines[0], lines: message } })
end sub

sub broken()
    if playerErrorKind(m.video.errorCode, m.video.errorInfo, m.played) = "media"
        ' The original doesn't play on this Roku after all: converted, once.
        if m.info <> invalid and httpString(m.info.method) <> "transcode" and m.mode <> "transcode"
            open(m.itemId, m.positionMs, m.item, "transcode")
            return
        end if
        endSession()
        text = "This video can't be played on this Roku."
        if m.info <> invalid
            reason = httpSentence(m.info.reason)
            if reason <> "" then text = text + " " + reason
        end if
        notice("Can't play this item", text, ["Back"])
        return
    end if
    if m.sessionId = ""
        notice("Can't play this item", "Playback stopped and couldn't get going again.", ["Try again", "Back"])
        return
    end if
    ' What became of the session says what to do.
    m.loading.visible = true
    uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onSessionState")
end sub

sub onSessionState(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId then return
    answer = event.getData()
    meaning = playerSessionState(answer.status)
    if meaning = "stopped"
        stopped(answer.data)
    else if meaning = "unreachable"
        uiUnreachable({ to: "player", params: { itemId: m.itemId, startMs: m.positionMs } })
    else if meaning <> "signed_out"
        restart()
    end if
end sub

' The session lives on (the token ran out) or expired: a new one from the
' same place, with the current token. The server ends the old one itself
' (same client_id).
sub restart()
    now = m.clock.TotalMilliseconds()
    if not playerMayRestart(m.restarts, now)
        notice("Can't play this item", "Playback stopped and couldn't get going again.", ["Try again", "Back"])
        return
    end if
    m.restarts.Push(now)
    open(m.itemId, m.positionMs, m.item, m.mode)
end sub

sub notice(title as string, text as string, buttons as object)
    m.loading.visible = false
    closeMenu()
    hideSkip()
    hideUpNext()
    m.noticeTitle.text = title
    m.noticeText.text = text
    m.noticeButtons.buttons = buttons
    m.notice.visible = true
    m.noticeButtons.setFocus(true)
end sub

sub onNoticeButton(event as object)
    if m.noticeButtons.buttons[event.getData()] = "Try again"
        m.restarts = []
        open(m.itemId, m.positionMs, m.item, m.mode)
    else
        uiNavigate({ back: true })
    end if
end sub

sub onNext(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if not answer.ok or answer.data = invalid then return
    item = httpObject(answer.data.item)
    if item <> invalid then m.next = contentUpNext(item, m.global.session.server)
end sub

sub showUpNext()
    closeSync()
    closeMenu()
    hideSkip()
    m.upNextShown = true
    m.nextTitle.text = m.next.title
    m.nextLine.text = m.next.line
    m.nextImage.setHttpAgent(m.top.getScene().getHttpAgent())
    m.nextImage.uri = m.next.image
    m.upNext.visible = true
    m.nextButtons.focusButton = 0
    m.nextButtons.setFocus(true)
    ' Turned off in Settings, the next episode waits for Play now.
    m.countdown.text = ""
    if not playerAutoplay(playbackSection()) then return
    m.countdownLeft = playerUpNextSeconds()
    m.countdown.text = "Next in " + m.countdownLeft.ToStr() + " s"
    m.countdownTimer.control = "start"
end sub

sub hideUpNext()
    m.countdownTimer.control = "stop"
    m.upNext.visible = false
end sub

sub onCountdown()
    m.countdownLeft = m.countdownLeft - 1
    if m.countdownLeft <= 0
        playNext()
    else
        m.countdown.text = "Next in " + m.countdownLeft.ToStr() + " s"
    end if
end sub

sub onNextButton(event as object)
    if event.getData() = 0
        playNext()
    else
        cancelUpNext()
    end if
end sub

' The rest plays out, credits with their Skip Credits button.
sub cancelUpNext()
    m.upNextCancelled = true
    hideUpNext()
    m.video.setFocus(true)
    updateSkip()
end sub

' Up Next shows at the credits: moving on counts the episode as watched.
sub playNext()
    nextEpisode = m.next
    hideUpNext()
    if nextEpisode = invalid then return
    if m.sessionId <> ""
        if m.durationMs > 0 then m.positionMs = m.durationMs
        report("stopped")
        endSession()
    end if
    m.video.control = "stop"
    m.restarts = []
    newTitle()
    if m.queue <> invalid then m.queueAt = m.queueAt + 1
    open(nextEpisode.itemId, nextEpisode.startMs, invalid, "auto")
end sub

' While subtitle sync shows: left and right (rewind and fast forward too)
' move the subtitles, Back closes it. While pley's menu shows: Back goes
' back in it, then closes it. While Up Next counts down: Back cancels it,
' and play, fast forward and rewind cancel it and act on the video.
' Otherwise Down brings up the menu, and Back leaves the player (the
' scene), which closes the session.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    ' The sleep timer's Keep watching: Back leaves it to count down, the
    ' video's own keys go on as ever.
    if m.sleepButtons.isInFocusChain()
        if key = "back"
            m.video.setFocus(true)
            return true
        end if
        if key = "play" or key = "fastforward" or key = "rewind"
            playerKey(key)
            return true
        end if
        return false
    end if
    if m.menu.visible
        if key = "back" or key = "left"
            if m.menuKind = "menu"
                closeMenu()
            else
                openMenu()
            end if
            return true
        end if
        ' The video's own keys go on as ever.
        if key = "play" or key = "fastforward" or key = "rewind"
            playerKey(key)
            return true
        end if
        return false
    end if
    if m.sync.visible
        if key = "left" or key = "rewind"
            stepSync(-syncStepMs())
            return true
        end if
        if key = "right" or key = "fastforward"
            stepSync(syncStepMs())
            return true
        end if
        if key = "back"
            closeSync()
            return true
        end if
        return false
    end if
    if m.upNext.visible
        if key = "back"
            cancelUpNext()
            return true
        end if
        if key = "play" or key = "fastforward" or key = "rewind" or key = "replay"
            cancelUpNext()
            playerKey(key)
            return true
        end if
        return false
    end if
    if key = "down" and menuOffered()
        openMenu()
        return true
    end if
    return false
end function

' --- pley's menu ---

' Its entries: Quality, Subtitle sync for a file with text subtitles,
' Chapters for a file that has them, Sleep timer, Find subtitles…,
' Playback details.
sub openMenu()
    hideSkip()
    m.menuKind = "menu"
    m.menuTitle.text = "Playback"
    items = [{ title: "Quality: " + qualityShown() }]
    m.menuActions = ["quality"]
    if m.textTracks > 0
        items.Push({ title: "Subtitle sync: " + syncLabel(m.syncMs) })
        m.menuActions.Push("sync")
    end if
    if detChapters(playingFile()).Count() > 0
        items.Push({ title: "Chapters" })
        m.menuActions.Push("chapters")
    end if
    items.Push({ title: "Sleep timer: " + sleepLabel(m.sleep, sleepNow(), playingKind()) })
    m.menuActions.Push("sleep")
    items.Push({ title: "Find subtitles…" })
    m.menuActions.Push("find")
    items.Push({ title: "Playback details" })
    m.menuActions.Push("details")
    at = m.menuList.itemFocused
    m.menuList.content = nodesContent(items)
    if at > 0 and at < items.Count() then m.menuList.jumpToItem = at
    m.menuChoices.visible = false
    m.menuText.visible = false
    m.menuList.visible = true
    m.menuHint.text = "Back closes this. Subtitles and audio: the Options key."
    m.menu.visible = true
    m.menuList.setFocus(true)
end sub

sub closeMenu()
    if not m.menu.visible then return
    m.menu.visible = false
    m.menuKind = ""
    m.video.setFocus(true)
    updateSkip()
end sub

' The file playing: the session's, else the item's first.
function playingFile() as dynamic
    if m.item = invalid then return invalid
    fileId = invalid
    if m.info <> invalid then fileId = m.info.file_id
    return contentFile(m.item, fileId)
end function

' The quality as the menu says it: "Automatic · 8 Mbps 1080p" while
' Automatic plays, else the one chosen, named for the file.
function qualityShown() as string
    choice = aqChoice(playbackSection())
    labels = {}
    for each quality in aqMenuFor(playingFile())
        labels[quality.id] = quality.label
    end for
    if choice = "auto"
        now = "Original"
        if m.watch <> invalid and aqStepped(m.watch)
            now = aqBitrateLabel(m.watch.playingKbps, m.watch.lines)
        else if m.watch <> invalid and labels.DoesExist(m.watch.quality)
            now = labels[m.watch.quality]
        end if
        return "Automatic · " + now
    end if
    if labels.DoesExist(choice) then return labels[choice]
    return aqLabel(choice)
end function

sub onMenuEntry(event as object)
    if m.menuKind = "found"
        addSubtitles(event.getData())
        return
    end if
    action = m.menuActions[event.getData()]
    if action = "find"
        findSubtitles()
    else if action = "quality"
        openQualities()
    else if action = "sync"
        closeMenu()
        openSync()
    else if action = "chapters"
        openChapters()
    else if action = "sleep"
        openSleep()
    else
        openDetails()
    end if
end sub

' The qualities this file is offered, the one chosen checked.
sub openQualities()
    m.menuKind = "quality"
    m.menuTitle.text = "Quality"
    choice = aqChoice(playbackSection())
    items = []
    m.menuValues = []
    current = 0
    for each quality in aqMenuFor(playingFile())
        if quality.id = choice then current = items.Count()
        items.Push({ title: quality.label })
        m.menuValues.Push(quality.id)
    end for
    showMenuChoices(items, current)
end sub

sub openChapters()
    m.menuKind = "chapters"
    m.menuTitle.text = "Chapters"
    chapters = detChapters(playingFile())
    items = []
    m.menuValues = []
    for each chapter in chapters
        items.Push({ title: chapter.label })
        m.menuValues.Push(chapter.startMs)
    end for
    showMenuChoices(items, detChapterAt(chapters, m.positionMs))
end sub

' Off, minutes, or the end of what plays; the one set checked.
sub openSleep()
    m.menuKind = "sleep"
    m.menuTitle.text = "Sleep timer"
    items = []
    m.menuValues = []
    current = 0
    for each choice in sleepChoices(playingKind())
        if choice.minutes = m.sleep.minutes then current = items.Count()
        items.Push({ title: choice.label })
        m.menuValues.Push(choice.minutes)
    end for
    showMenuChoices(items, current)
end sub

sub showMenuChoices(items as object, current as integer)
    m.menuChoices.content = nodesContent(items)
    m.menuChoices.checkedItem = current
    m.menuChoices.jumpToItem = current
    m.menuList.visible = false
    m.menuText.visible = false
    m.menuChoices.visible = true
    m.menuHint.text = ""
    onMenuChoiceFocused()
    m.menuChoices.setFocus(true)
end sub

sub onMenuChoiceFocused()
    if m.menuKind <> "quality" then return
    at = m.menuChoices.itemFocused
    if at >= 0 and at < m.menuValues.Count() then m.menuHint.text = aqHint(m.menuValues[at])
end sub

' A quality is kept as the Roku's (Settings shows it) and the title opens
' again at it where it was; a chapter is gone to.
sub onMenuChoice(event as object)
    at = event.getData()
    if at < 0 or at >= m.menuValues.Count() then return
    value = m.menuValues[at]
    kind = m.menuKind
    closeMenu()
    if kind = "chapters"
        seekTo(value)
        return
    end if
    if kind = "sleep"
        setSleep(value)
        return
    end if
    if value = aqChoice(playbackSection()) then return
    aqChoose(playbackSection(), value)
    ' Picked again, Automatic starts afresh at the original.
    if value = "auto" then keepCeiling(0)
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    newTitle()
    open(m.itemId, position, m.item, m.mode)
end sub

' --- The sleep timer ---

' What plays, as the sleep timer names its end.
function playingKind() as string
    if m.item = invalid then return ""
    return httpString(m.item.kind)
end function

function clockMs() as double
    return m.clock.TotalMilliseconds() * 1#
end function

' What the timer says now.
function sleepNow() as object
    left = -1#
    if m.played and m.durationMs > 0 then left = (m.durationMs - m.positionMs) * 1#
    return sleepState(m.sleep, clockMs(), left, urlValue(m.itemId))
end function

' Set from the menu (minutes; 0 for Off, -1 for the end of what plays).
sub setSleep(minutes as integer)
    m.sleep = sleepNew(minutes, clockMs())
    hideSleepNotice()
    if minutes = 0 and m.sleepPausedAt < 0
        m.sleepTimer.control = "stop"
    else
        m.sleepTimer.control = "start"
    end if
end sub

' Once a second: the notice in the last 30 s, the pause at the end, and
' the player left 10 minutes into that pause.
sub onSleepTick()
    if m.sleepPausedAt >= 0
        if clockMs() - m.sleepPausedAt >= sleepLeaveAfterMs() then uiNavigate({ back: true })
        return
    end if
    state = sleepNow()
    if state.fire
        m.sleep = sleepNew(0, 0#)
        hideSleepNotice()
        m.sleepPausedAt = clockMs()
        m.video.control = "pause"
        return
    end if
    if state.secondsLeft < 0
        hideSleepNotice()
        return
    end if
    m.sleepText.text = "Pausing in " + urlValue(state.secondsLeft) + " s"
    if not m.sleepNotice.visible
        m.sleepNotice.visible = true
        ' The button takes the focus while nothing else is up.
        if m.video.hasFocus() or m.skip.hasFocus() then m.sleepButtons.setFocus(true)
    end if
end sub

sub hideSleepNotice()
    if m.sleepButtons.isInFocusChain() then m.video.setFocus(true)
    m.sleepNotice.visible = false
end sub

' Keep watching: the same time again, or this title plays to its end and
' the next one's end stops.
sub onKeepWatching()
    sleepExtend(m.sleep, urlValue(m.itemId))
    hideSleepNotice()
end sub

' How the stream plays, and for a conversion what the server makes of it
' (GET /api/playback/{session}).
sub openDetails()
    m.menuKind = "details"
    m.menuTitle.text = "Playback details"
    showDetails(invalid)
    m.menuList.visible = false
    m.menuChoices.visible = false
    m.menuText.visible = true
    m.menuHint.text = ""
    m.top.setFocus(true)
    if m.sessionId <> "" and httpString(m.info.method) <> "direct" then uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onDetails")
end sub

sub onDetails(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId or m.menuKind <> "details" then return
    answer = event.getData()
    if answer.ok and answer.data <> invalid then showDetails(httpObject(answer.data.transcode))
end sub

sub showDetails(live as dynamic)
    if m.info = invalid then return
    details = detPlayback(m.info, playingFile(), m.audioPlaying, live, m.positionMs)
    lines = [details.method]
    if details.why <> "" then lines.Push(details.why)
    lines.Push("")
    for each row in details.rows
        lines.Push(row.label + ": " + row.value)
    end for
    if m.watch <> invalid and aqLowered(m.watch) then lines.Push("Quality: " + qualityShown() + " · Lowered for your connection")
    m.menuText.text = lines.Join(Chr(10))
end sub

' --- Find subtitles… (SubtitleSearch.brs) ---

' The server's results for the file playing, in the person's subtitle
' language, listed in the menu.
sub findSubtitles()
    m.menuKind = "finding"
    m.menuTitle.text = "Find subtitles"
    m.found = []
    showMenuText("Looking for subtitles…")
    m.top.setFocus(true)
    fileId = 0
    if m.info <> invalid then fileId = m.info.file_id
    uiRequest({ method: "GET", path: ssSearchPath(m.itemId, fileId), timeoutMs: 30000, session: m.sessionId }, "onSubtitlesFound")
end sub

sub showMenuText(text as string)
    m.menuText.text = text
    m.menuList.visible = false
    m.menuChoices.visible = false
    m.menuText.visible = true
    m.menuHint.text = "Back goes back."
end sub

sub onSubtitlesFound(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId or m.menuKind <> "finding" then return
    answer = event.getData()
    if not answer.ok
        showMenuText(ssProblem(answer, true))
        return
    end if
    m.found = ssResults(answer.data)
    if m.found.Count() = 0
        showMenuText("No subtitles were found for this file in your subtitle language.")
        return
    end if
    m.menuKind = "found"
    items = []
    for each result in m.found
        items.Push({ title: result.label })
    end for
    m.menuList.content = nodesContent(items)
    m.menuList.jumpToItem = 0
    m.menuText.visible = false
    m.menuChoices.visible = false
    m.menuList.visible = true
    onMenuEntryFocused()
    m.menuList.setFocus(true)
end sub

sub onMenuEntryFocused()
    if m.menuKind <> "found" then return
    at = m.menuList.itemFocused
    if at >= 0 and at < m.found.Count() then m.menuHint.text = m.found[at].hint
end sub

' The result picked is added on the server, then shown.
sub addSubtitles(at as integer)
    if at < 0 or at >= m.found.Count() or m.info = invalid then return
    m.menuKind = "adding"
    showMenuText("Adding " + m.found[at].label + "…")
    m.top.setFocus(true)
    body = { result: m.found[at].id }
    fileId = httpNumber(m.info.file_id, 0)
    if fileId > 0 then body.file_id = fileId
    uiRequest({ method: "POST", path: "/api/items/" + urlValue(m.itemId) + "/subtitles", json: body, timeoutMs: 60000, session: m.sessionId }, "onSubtitlesAdded")
end sub

' The new track joins the stream's text tracks and shows: the stream opens
' again where it was with it (a new session where the server drew picture
' subtitles onto the video), and it is saved as the person's pick for the
' show or the movie, as a track picked in Roku's options is.
sub onSubtitlesAdded(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId or m.menuKind <> "adding" then return
    answer = event.getData()
    track = invalid
    if answer.ok then track = ssAddedTrack(answer.data)
    if track = invalid
        if answer.ok then answer.message = "The server didn't send the subtitles back."
        showMenuText(ssProblem(answer, false))
        return
    end if
    closeMenu()
    m.info.subtitles = ssWithTrack(m.info.subtitles, track)
    index = Int(httpNumber(track.index, -1))
    file = contentFile(m.item, m.info.file_id)
    burned = -1
    if httpTrue(m.info.burn_subtitles) then burned = Int(httpNumber(m.info.subtitle_index, -1))
    m.subtitleShown = index
    body = tracksStreamsBody(m.info.file_id, -2, index)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.itemId) + "/streams", jsonText: FormatJson(body) }, "")
    if file <> invalid then file.selected_subtitle = index
    stream = contentStream(m.info, m.item, m.global.session.server, m.positionMs)
    m.textTracks = stream.subtitleTracks.Count()
    showToast("Subtitles added: " + contentSubtitleName(track, httpString(track.language)))
    if tracksNeedNewSession(burned, index)
        position = m.positionMs
        seconds = m.video.position
        if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
        open(m.itemId, position, m.item, m.mode)
        return
    end if
    url = httpString(track.url)
    if Left(url, 1) = "/" then url = m.global.session.server + url
    captionsOn()
    reopen(syncTrackUrl(url, m.syncMs))
end sub
