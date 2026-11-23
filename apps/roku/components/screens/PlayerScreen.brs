' The player (docs/API.md, "Playback"). Progress goes to the server about
' every 10 s while playing, on pause and every 30 s while paused, and as
' "stopped" on the way out, then the session is closed. A stream that
' breaks is checked with the server: stopped by an administrator (410),
' the player leaves and says so; otherwise it starts again where it was,
' with a new session and the current token (a token that ran out answers
' 401 to the Video node). Skip Intro and Skip Credits follow the web
' player: an episode's credits bring up Up Next instead. The controls are
' Pley's own, as the Android TV player's (PlayerControls; the Video node's
' are off): the scrubber, which Left and Right seek with (Seeker.brs), and
' the buttons, whose panels have the file's subtitles (every track,
' pictures too, with subtitle sync for text ones or pictures drawn here,
' SubtitleSync.brs, and Find subtitles…), its audio, Quality, its
' chapters, the sleep timer (Sleep.brs) and the playback's details
' (Details.brs). Subtitles (or audio) the person picks are remembered on the server
' for the show or the movie (Tracks.brs), so the next episode starts with
' matching ones; the user's remote, on another device, picks them as the
' menu does. The quality is the one chosen in Settings or the menu (the
' remote chooses it as the menu does); when it doesn't play well,
' Automatic lowers a conversion's bitrate as it is sent, or opens the
' stream again lower where it was once it stopped anyway
' (AutoQuality.brs). PGS subtitles are drawn over the video from
' the server's pictures (Pictures.brs), so the file plays as it is; the
' server burns them in only where it can't make those.

sub init()
    ' What holds the focus while nothing else does: the player's keys come
    ' to it (onKeyEvent). The screen itself can't take the focus back from
    ' a part of it that has it.
    m.keys = m.top.findNode("keys")
    m.video = m.top.findNode("video")
    m.skip = m.top.findNode("skip")
    m.controls = m.top.findNode("controls")
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
    m.menuSlide = m.top.findNode("menuSlide")
    m.menuTitle = m.top.findNode("menuTitle")
    m.menuList = m.top.findNode("menuList")
    m.menuText = m.top.findNode("menuText")
    m.menuFacts = m.top.findNode("menuFacts")
    m.detailsShown = ""
    m.menuHint = m.top.findNode("menuHint")
    m.menuTitle.font = uiType("section")
    m.menuText.font = uiType("body")
    m.menuHint.font = uiType("caption")
    ' What the panel shows ("subtitles", "audio", "quality", "chapters",
    ' "sleep", "details", "finding", "found"…), the button that opened it
    ' (focused again as it closes), and the values of its rows: a choice's,
    ' or the action of a row that isn't one ("sync", "find").
    m.menuKind = ""
    m.menuButton = ""
    m.menuValues = []
    ' Find subtitles…: the language looked in ("": the person's subtitle
    ' language), and the results listed ([{ id, label, hint }]) after the
    ' Language entry.
    m.findLanguage = ""
    m.found = []
    m.menuList.observeField("itemSelected", "onMenuChoice")
    m.menuList.observeField("itemFocused", "onMenuChoiceFocused")
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
    ' Whether the stream opening was asked for by the user's remote (a
    ' track or a quality picked, a cast naming tracks), which hears of it as
    ' it plays.
    m.reportAtPlay = false
    ' Whether the pause is the user's remote's, which never brings up the
    ' controls: the person's own key, or the video playing again, ends it.
    m.remoteQuiet = false
    ' The audio a cast named for its title (-1: the server's pick by the
    ' user's choices); its subtitles are in the item's file
    ' (Cast.brs's castGivenTracks).
    m.playAudio = -1
    ' Whether this opening's video has played yet (errors before are the file's).
    m.played = false
    ' Whether this opening's stream was given again after failing at once
    ' (broken()), and whether that try is still to play or fail.
    m.givenAgain = false
    m.againPending = false
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
    ' The offset the stream's text tracks were opened with.
    m.streamSyncMs = 0
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
    ' The server converts no video, as its last play answer had it
    ' (original_only): only Original is offered, and plays.
    m.originalOnly = false
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
    keepWatching = t("player.sleep.keepWatching")
    m.sleepButtons.buttons = [keepWatching]
    ' Longer words ("Continuer à regarder") make the button wider than its
    ' least, and the notice behind it with it.
    button = textWidth(keepWatching, uiType("label")) + 64
    if button < m.sleepButtons.minWidth then button = m.sleepButtons.minWidth
    back = m.top.findNode("sleepBack")
    width = m.sleepButtons.translation[0] + button + 8
    if width > back.width then back.width = width
    m.sleepButtons.observeField("buttonSelected", "onKeepWatching")
    m.sleepTimer = m.top.findNode("sleepTimer")
    m.sleepTimer.observeField("fire", "onSleepTick")
    m.pausedTimer = m.top.findNode("pausedTimer")
    m.pausedTimer.duration = playerPausedReportEveryMs() / 1000
    m.pausedTimer.observeField("fire", "onPausedTick")
    m.video.observeField("downloadedSegment", "onSegment")
    ' Picture subtitles drawn here: the track (-1 none), its pictures known
    ' (Pictures.brs), where the video's picture shows (invalid when the
    ' file doesn't say its size), the folder of their files after the
    ' server, the cue showing (-1 none), the frames (the one at front shows,
    ' the other loads the next), which answer is awaited and whether a seek
    ' since wants another at once, the last position seen (and when) to
    ' tell a seek by, and the pictures that failed to load (and when). Once
    ' the server turns out not to make them, they are burned in for as long
    ' as the player shows.
    m.picturesOff = false
    m.pictureIndex = -1
    m.pictures = invalid
    m.picturePicture = invalid
    m.pictureFolder = ""
    m.pictureShown = -1
    m.pictureFrames = [m.top.findNode("pictureA"), m.top.findNode("pictureB")]
    m.pictureFront = 0
    for each frame in m.pictureFrames
        frame.setHttpAgent(uiHttpAgent(false))
        frame.observeField("loadStatus", "onPictureLoad")
    end for
    m.pictureFailed = {}
    ' Captions as last seen: the person turning them off turns off pictures
    ' drawn here.
    m.captionModeSeen = httpString(m.video.globalCaptionMode)
    m.pictureAsk = 0
    m.pictureWaiting = false
    m.pictureAgain = false
    m.pictureLastMs = -1
    m.pictureLastAt = 0
    m.picturesTimer = m.top.findNode("picturesTimer")
    m.picturesTimer.duration = picAgainMs() / 1000
    m.picturesTimer.observeField("fire", "askPictures")
    m.top.findNode("syncTitle").text = t("player.menu.sync")
    m.top.findNode("syncHint").text = t("player.sync.hint")
    ' Done, then Reset.
    m.syncButtons.buttons = [t("common.actions.done"), t("player.sync.reset")]
    m.syncButtons.observeField("buttonSelected", "onSyncButton")
    m.video.notificationInterval = 1
    m.video.observeField("state", "onState")
    m.video.observeField("position", "onPosition")
    for each field in ["currentSubtitleTrack", "globalCaptionMode", "currentAudioTrack", "availableAudioTracks"]
        m.video.observeField(field, "onTracks")
    end for
    m.skip.observeField("pressed", "onSkip")
    m.skip.observeField("playKey", "onSkipKey")
    m.top.findNode("upNextTitle").text = t("player.upNext.title")
    m.nextButtons.buttons = [t("player.upNext.playNow"), t("common.actions.cancel")]
    m.nextButtons.observeField("buttonSelected", "onNextButton")
    ' What the notice's buttons do ("retry", "back"), in their order.
    m.noticeActions = []
    m.noticeButtons.observeField("buttonSelected", "onNoticeButton")
    m.countdownTimer.observeField("fire", "onCountdown")
    ' The title at the top as each item begins, once it plays.
    m.banner = m.top.findNode("banner")
    m.bannerTitle = m.top.findNode("bannerTitle")
    m.bannerLine = m.top.findNode("bannerLine")
    m.bannerDue = false
    m.bannerTimer = m.top.findNode("bannerTimer")
    m.bannerTimer.duration = playerBannerMs() / 1000
    m.bannerTimer.observeField("fire", "hideBanner")
    ' The server out of reach: since when (clock ms), -1 while it answers.
    ' Until something played, that is said at once.
    m.reconnectFrom = -1
    m.everPlayed = false
    m.reconnectTimer = m.top.findNode("reconnectTimer")
    m.reconnectTimer.duration = playerReconnectEveryMs() / 1000
    m.reconnectTimer.observeField("fire", "onReconnectTick")
    ' The playhead standing still: where, and since when (clock ms); and
    ' whether this opening said the server can't keep up.
    m.stillAt = -1
    m.stillSince = 0
    m.slowSaid = false
    m.stallTimer = m.top.findNode("stallTimer")
    m.stallTimer.observeField("fire", "onStallTick")
    ' The last position read, and when, to tell a seek by.
    m.seenMs = -1
    m.seenAt = 0
    ' The media ended: Up Next waits at its end. How often this title's
    ' stream was opened again for falling short of its end.
    m.ended = false
    m.shortTries = 0
    m.detailsTimer = m.top.findNode("detailsTimer")
    m.detailsTimer.observeField("fire", "onDetailsTick")
    m.top.observeField("active", "onActive")
    m.top.observeField("serverStopped", "onServerStopped")
    m.top.observeField("playerCommand", "onPlayerCommand")
    setupControls()
    styleOverlays()
end sub

' The player's notices in Pley's type (Ui.brs's uiType), as the controls.
sub styleOverlays()
    fonts = {
        bannerTitle: "title", bannerLine: "label", upNextTitle: "caption", nextTitle: "section", nextLine: "label",
        countdown: "caption", syncTitle: "caption", syncValue: "section", syncHint: "caption", noticeTitle: "pageTitle",
        noticeText: "bodyLarge", sleepText: "label", toastText: "label"
    }
    for each id in fonts
        m.top.findNode(id).font = uiType(fonts[id])
    end for
end sub

' Paused, the progress goes on being told, so the session stays the
' player's.
sub onPausedTick()
    if m.video.state = "paused" then report("paused")
end sub

' This playback's session stopped from elsewhere: it ends now, not at its
' next report.
sub onServerStopped(event as object)
    data = event.getData()
    if m.sessionId <> "" and httpString(data.session_id) = m.sessionId then stopped(data)
end sub

' A command of the user's remote, on another device, for this playback's
' session (Remote.brs has the rule). Only a video playing or paused obeys:
' not one still starting, at its end, stopped, or saying what went wrong.
' A play or pause is reported as the video's own are, when its state
' changes; a seek at once; the next item with its own session; audio and
' subtitles as Pley's menu picks them (obeyTrack), and the quality too
' (obeyQuality). None brings up the player's controls: what it did shows
' for a moment in the toast (Remote.brs's remoteNotice), and the next
' title as it begins.
sub onPlayerCommand(event as object)
    state = m.video.state
    if m.sessionId = "" or not m.top.active or not m.played or m.ended or m.notice.visible then return
    if state <> "playing" and state <> "paused" and state <> "buffering" then return
    command = event.getData()
    what = httpString(command.command)
    if what = "audio" or what = "subtitles"
        obeyTrack(command)
        return
    end if
    if what = "quality"
        obeyQuality(command)
        return
    end if
    duration = invalid
    if m.durationMs > 0 then duration = m.durationMs
    player = { kind: "video", session: m.sessionId, playing: state <> "paused", positionMs: m.positionMs, durationMs: duration, hasNext: m.next <> invalid }
    action = remoteAction(player, command)
    if action.act = "play"
        m.video.control = "resume"
    else if action.act = "pause"
        ' Its pause doesn't bring up the controls, as the person's does, and
        ' puts away any that were up.
        m.remoteQuiet = true
        hideControls()
        m.video.control = "pause"
    else if action.act = "report"
        ' As it is already: said again, so a remote a little behind settles.
        report(reportedState())
    else if action.act = "seek"
        ' As the rewind key does: left up, Up Next's countdown would play
        ' the next episode over the place the remote asked for.
        if m.upNext.visible then cancelUpNext()
        endSeeking()
        seekTo(action.positionMs)
        ' Told now, not as the next reading finds the playhead moved.
        m.seenMs = m.positionMs
        m.seenAt = m.clock.TotalMilliseconds()
        report(reportedState())
    else if action.act = "next"
        playNext(false)
    end if
    remoteSays(remoteNotice(command, action))
end sub

' What the user's remote did, for a moment in the toast.
sub remoteSays(text as string)
    if text <> "" then showToast(text, remoteNoticeSeconds())
end sub

' The remote's pause is over: the next pause is the person's own.
sub endQuiet()
    m.remoteQuiet = false
end sub

' The remote's audio or subtitles (Remote.brs's remoteTrackAction), on the
' path of Pley's menu: saved for the show or the movie, and the stream
' opened again with it, which tells the server at once as it plays; the
' toast names it for a moment. The track playing already is said again at
' once, and shows nothing.
sub obeyTrack(command as object)
    tracks = tracksIndexes(playingFile())
    player = { session: m.sessionId, audio: tracks.audio, subtitles: tracks.subtitles, audioIndex: m.audioPlaying, subtitleIndex: subtitlePlaying() }
    action = remoteTrackAction(player, command)
    if action.act = "none"
        if action.report then report(reportedState())
        return
    end if
    remoteSays(remoteTrackNotice(action, mediaTracks(playingFile())))
    m.reportAtPlay = true
    pickTrack(action.act, action.index)
end sub

' The remote's quality (Remote.brs's remoteQualityAction), as a pick in
' Pley's Quality menu: kept as the Roku's, and the title opened again at
' it where it was, which tells the server at once as it plays; the toast
' names it for a moment. The quality picked already is said again at
' once, and shows nothing.
sub obeyQuality(command as object)
    player = { session: m.sessionId, qualities: aqMenuIds(playingFile(), m.originalOnly), quality: qualityPicked() }
    action = remoteQualityAction(player, command)
    if action.act = "none"
        if action.report then report(reportedState())
        return
    end if
    remoteSays(remoteQualityNotice(action, aqMenuFor(playingFile(), m.originalOnly)))
    m.reportAtPlay = true
    chooseQuality(action.quality)
end sub

' The Quality menu's pick for the file playing, as it ticks it and as the
' reports say it.
function qualityPicked() as string
    return aqPicked(aqChoice(playbackSection()), playingFile(), m.originalOnly)
end function

' The subtitles showing, as the reports and the remote name them: those
' burned in, else those shown (Tracks.brs).
function subtitlePlaying() as integer
    burned = -1
    if m.info <> invalid and httpTrue(m.info.burn_subtitles) then burned = Int(httpNumber(m.info.subtitle_index, -1))
    return tracksSubtitlePlaying(m.subtitleShown, burned)
end function

' What the video is doing, as progress tells it.
function reportedState() as string
    if m.video.state = "paused" then return "paused"
    return "playing"
end function

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
    m.playAudio = Int(httpNumber(params.audioIndex, -1))
    ' A cast's tracks: the remote hears of them as soon as they play.
    m.reportAtPlay = httpTrue(params.tracksGiven)
    newTitle()
    m.bannerDue = true
    m.shortTries = 0
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
    m.givenAgain = false
    m.againPending = false
    m.next = invalid
    m.upNextShown = false
    m.upNextCancelled = false
    m.ended = false
    m.stallTimer.control = "stop"
    m.slowSaid = false
    m.seenMs = -1
    m.markers = []
    m.skipMarker = invalid
    m.skipKey = ""
    m.skipDismissed = false
    m.textTracks = 0
    m.tracksFrom = -1
    endQuiet()
    stopPictures()
    m.sync.visible = false
    closeMenu()
    hideSkip()
    hideUpNext()
    m.notice.visible = false
    endSeeking()
    hideControls()
    stopPreviews()
    setLoading(true)
    m.keys.setFocus(true)
    if m.item = invalid
        uiRequest({ method: "GET", path: "/api/items/" + urlValue(itemId), attempt: m.attempt }, "onItem")
        return
    end if
    itemKnown()
    askPlayback()
end sub

' POST /api/playback for the version chosen, else the first file, with
' its picture subtitles to burn in when the person's choices pick some the
' app doesn't draw itself, at the
' quality chosen in Settings (or Automatic's). A title's first opening in
' Automatic asks the server what it knows of the network first.
sub askPlayback()
    choice = aqChoice(playbackSection())
    ' Where only the original plays it is asked for, whatever is kept:
    ' there is nothing to ask the network for, and a quality would be
    ' answered with the original copied into a stream.
    if m.originalOnly then choice = "original"
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
    ' PGS the app can't draw (no pictures from this server) is burned in:
    ' the caps say the app doesn't show it, or the server would leave it out.
    drawn = picCodecs()
    if m.picturesOff
        drawn = []
        spec.picturesOff = true
    end if
    ' The version chosen on the title's page; a title played next has none
    ' of its files, so it plays its first (the server's default too).
    file = contentFile(m.item, m.top.params.fileId)
    if file <> invalid
        spec.fileId = httpNumber(file.id, 0)
        burn = playerBurnIndex(file, drawn)
        if burn >= 0 then spec.subtitleIndex = burn
    end if
    again = ""
    if choice = "auto" then again = m.autoQuality
    ' What it obeys of the user's remote: next where something may come
    ' next, the file's audio and subtitles, and the quality.
    tracks = tracksIndexes(file)
    spec.controls = remoteVideoControls(playerMayHaveNext(m.item, m.queue, m.queueAt), tracks.audio.Count(), tracks.subtitles.Count(), true)
    ' The audio a cast named, for this play (Cast.brs's castGivenTracks).
    if m.playAudio >= 0 then spec.audioIndex = m.playAudio
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
    m.originalOnly = httpTrue(m.info.original_only)
    m.sessionId = httpString(m.info.session_id)
    begin()
end sub

' Plays once both the item and the session are there.
sub begin()
    if m.item = invalid or m.info = invalid then return
    m.stillAt = m.startMs
    m.stillSince = m.clock.TotalMilliseconds()
    m.stallTimer.control = "start"
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
    fillControls()
    if stream.subtitle <> ""
        captionsOn()
    else
        captionsBack()
    end if
    m.video.control = "play"
    m.keys.setFocus(true)
    m.lastReportAt = m.clock.TotalMilliseconds()
    watchPlayback()
    startPictures()
end sub

' Automatic's watch over what now plays: the title's own after a restart,
' told the quality it plays at now (the original when the server played
' the file as it is), the bitrate that is, the server's own quality for
' the file, the lines the stream is named by, whether its bitrate changes
' as it is sent (live_bitrate) and whether it comes in several sizes.
sub watchPlayback()
    ' Automatic has no other quality to step to on a server that converts
    ' no video.
    if m.asked = invalid or not m.asked.automatic or m.originalOnly or httpString(m.item.kind) = "track"
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
        if decision.down then showToast(t("player.toasts.qualityLowered", { quality: aqBitrateLabel(decision.kbps, m.watch.lines) }))
        setBitrate(decision.kbps)
        return
    end if
    if decision.down then m.autoNotice = t("player.toasts.qualityLowered", { quality: aqLabel(decision.quality) })
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

' A notice at the top for seconds: 4 for the player's own, which say more
' than the remote's word of what it did.
sub showToast(text as string, seconds = 4 as integer)
    m.toastText.text = text
    ' As wide as its words (840 at most, then on more lines), its right edge
    ' at the safe area's; under the clock while the controls show.
    width = textWidth(text, uiType("label"))
    if width > 760 then width = 760
    m.toastText.width = width
    lines = Int(m.toastText.boundingRect().height / 36)
    if lines < 1 then lines = 1
    height = 40 + lines * 36
    if height < 76 then height = 76
    m.toastText.height = height
    back = m.top.findNode("toastBack")
    back.width = width + 80
    back.height = height
    top = 54
    if m.controls.shown then top = 158
    m.toast.translation = [1824 - width - 80, top]
    m.toast.visible = true
    m.toastTimer.control = "stop"
    m.toastTimer.duration = seconds
    m.toastTimer.control = "start"
end sub

sub onToastTimer()
    m.toast.visible = false
end sub

' How wide text is in font on one line, as a Label draws it (one kept
' unseen for measuring).
function textWidth(text as string, font as object) as integer
    if m.measure = invalid
        m.measure = m.top.createChild("Label")
        m.measure.opacity = 0
    end if
    m.measure.font = font
    m.measure.text = text
    return Int(m.measure.boundingRect().width)
end function

' The title at the top for 4 s as an item begins to play (the first, the
' next episode, a queue's next), not as the same one opens again.
sub showBanner()
    m.bannerDue = false
    banner = playerBanner(m.item)
    if banner.title = "" or m.controls.shown then return
    m.bannerTitle.text = banner.title
    m.bannerLine.text = banner.line
    m.banner.visible = true
    m.bannerTimer.control = "stop"
    m.bannerTimer.control = "start"
end sub

sub hideBanner()
    m.bannerTimer.control = "stop"
    m.banner.visible = false
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
    m.streamSyncMs = m.syncMs
    if LCase(Left(stream.url, 8)) = "https://" then nodesFill(content, { HttpCertificatesFile: "common:/certs/ca-bundle.crt" })
    m.video.setHttpAgent(uiHttpAgent())
    m.video.content = content
    if subtitle <> "" then m.video.subtitleTrack = subtitle
    ' The seek previews are the controls' to show.
    startPreviews(stream.previews)
end sub

' Where subtitle sync is remembered on this Roku.
function syncSection() as object
    return CreateObject("roRegistrySection", "subtitleSync")
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
    ' Pictures drawn here move at once.
    drawPictures()
end sub

sub onSyncButton(event as object)
    if event.getData() = 1
        stepSync(-m.syncMs)
    else
        closeSync()
    end if
end sub

' Done or Back: a new offset is remembered for the file, and the stream
' opens again where it was, its text tracks moved by the server. Pictures
' drawn here moved already: the stream plays on.
sub closeSync()
    if not m.sync.visible then return
    m.sync.visible = false
    m.keys.setFocus(true)
    if m.syncMs = m.syncWas then return
    syncRemember(syncSection(), m.syncKey, m.syncMs)
    if m.pictureIndex < 0 then reopen()
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
    setLoading(true)
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
    mode = httpString(m.video.globalCaptionMode)
    subtitle = tracksSubtitleShown(mode, httpString(m.video.currentSubtitleTrack), m.info.subtitles, m.global.session.server)
    settled = m.tracksFrom >= 0 and m.clock.TotalMilliseconds() >= m.tracksFrom
    ' Roku's options can't name pictures drawn here: captions on without a
    ' text track leave them be; turned off by the person, they go.
    turnedOff = settled and LCase(mode) = "off" and LCase(m.captionModeSeen) <> "off"
    m.captionModeSeen = mode
    if m.pictureIndex >= 0 and subtitle = -1 and not turnedOff then subtitle = -2
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
    if not settled then return
    saveTracks(file, subtitlePicked, audioPicked)
end sub

' PUT /api/items/{id}/streams with the tracks playing; kept in the file too,
' so a stream started again shows them. Text subtitles picked over picture
' ones the server draws need a stream without those; over ones drawn here,
' only that those go.
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
        if body.DoesExist("audio_index")
            file.selected_audio = body.audio_index
            file.audio_picked = body.audio_index >= 0
        end if
        file.selected_subtitle = body.subtitle_index
    end if
    if subtitlePicked and m.pictureIndex >= 0 and subtitle <> m.pictureIndex
        stopPictures()
        ' A sync changed meanwhile moved the pictures alone: the text track
        ' moves with the stream opened again.
        if subtitle >= 0 and m.syncMs <> m.streamSyncMs then reopen()
    end if
    if not subtitlePicked or not tracksNeedNewSession(burned, subtitle) then return
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    open(m.itemId, position, m.item, m.mode)
end sub

' A request before playback failed: the server doesn't answer (the player
' reconnects, or says so), the sign-in ended (the scene shows sign-in), or
' a refusal (Try again where asking again may help).
sub failed(answer as object)
    if answer.error = "signed_out" then return
    if answer.error = "network"
        reconnect()
        return
    end if
    actions = ["back"]
    if playerMayTryAgain(answer.status) then actions = ["retry", "back"]
    notice(t("player.errors.cantPlay"), answer.message, actions)
end sub

sub onState(event as object)
    state = event.getData()
    watchState(state)
    drawPictures()
    ' The playhead is read again from now: a pause, or a seek's wait, isn't
    ' time it should have moved in. The last reading stays, so a seek made
    ' here (Skip intro, Chapters) moves the next one away from it.
    m.seenAt = m.clock.TotalMilliseconds()
    if state <> "paused" then m.pausedTimer.control = "stop"
    if state <> "playing" then hideBanner()
    m.controls.playing = state <> "paused"
    m.controls.paused = state = "paused"
    if state = "buffering" then setLoading(true)
    if state = "playing"
        m.againPending = false
        m.played = true
        m.everPlayed = true
        ' Played again, by anyone: the remote's pause is over.
        endQuiet()
        endReconnect()
        setLoading(false)
        if m.bannerDue then showBanner()
        ' Played again after the sleep timer's pause: the player stays.
        if m.sleepPausedAt >= 0
            m.sleepPausedAt = -1#
            if m.sleep.minutes = 0 then m.sleepTimer.control = "stop"
        end if
        if m.tracksFrom < 0 then m.tracksFrom = m.clock.TotalMilliseconds() + tracksSettleMs()
        restartHide()
        if m.paused or m.reportAtPlay
            m.paused = false
            m.reportAtPlay = false
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
        m.pausedTimer.control = "start"
        setLoading(false)
        ' The person's pause shows where it is, and when it would end; the
        ' remote's and the sleep timer's don't.
        if not m.remoteQuiet and m.sleepPausedAt < 0
            focus = m.focusKey
            if not m.controls.shown or focus = "" then focus = "scrubber"
            showControls(focus)
        end if
    else if state = "finished"
        ' The failed stream's end, as it is given again (broken()).
        if m.againPending then return
        ' Media that falls short of the title (resumed past the end of what
        ' the file or stream holds) isn't its end.
        if playerEndedShort(m.positionMs, m.startMs, m.durationMs)
            endedShort()
        else
            ended()
        end if
    else if state = "error"
        broken()
    end if
end sub

sub onPosition(event as object)
    seconds = event.getData()
    if seconds = invalid then return
    m.positionMs = Int(seconds * 1000)
    now = m.clock.TotalMilliseconds()
    if m.watch <> invalid then aqPosition(m.watch, now, m.positionMs)
    if m.positionMs <> m.stillAt
        m.stillAt = m.positionMs
        m.stillSince = now
    end if
    ' A seek, here or in Roku's own bar, is told at once, as the other apps
    ' tell it a second after: other devices resume from there.
    sought = m.seenMs >= 0 and playerSought(m.seenMs, m.positionMs, now - m.seenAt)
    m.seenMs = m.positionMs
    m.seenAt = now
    if sought
        state = "playing"
        if m.paused then state = "paused"
        report(state)
    else if playerShouldReport(m.video.state, now - m.lastReportAt)
        report("playing")
    end if
    if m.controls.shown then m.controls.positionMs = m.positionMs
    if m.next <> invalid and not m.upNextShown and m.upNextAt > 0 and m.positionMs >= m.upNextAt and not sleepEndsWith(m.sleep, urlValue(m.itemId)) then showUpNext()
    updateSkip()
    followPictures()
    drawPictures()
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
        ' The controls keep the focus while they show.
        if not m.controls.shown and not m.sleepButtons.isInFocusChain() then m.skip.setFocus(true)
    else
        hideSkip()
    end if
end sub

sub hideSkip()
    if m.skip.hasFocus() then focusKeys()
    m.skip.visible = false
end sub

' The focus back on the player's keys; with the controls up and nothing in
' them focused (Skip had it), on the scrubber.
sub focusKeys()
    m.keys.setFocus(true)
    if m.controls.shown and m.focusKey = "" then focusControl("scrubber")
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
    m.controls.positionMs = target
    m.video.seek = target / 1000
end sub

' POST /api/items/{id}/progress for the current session. Stopped, the
' screens showing the title follow (Ui.brs's uiNoteChange).
sub report(state as string)
    if m.sessionId = "" then return
    if state = "stopped" then uiNoteChange({ itemId: m.itemId, action: "played", positionMs: m.positionMs, durationMs: m.durationMs, watched: playerWatched(m.positionMs, m.durationMs) })
    m.lastReportAt = m.clock.TotalMilliseconds()
    path = "/api/items/" + urlValue(m.itemId) + "/progress"
    ' With the tracks playing and the quality picked, which the user's
    ' remotes tick.
    tracks = { audio: m.audioPlaying, subtitle: subtitlePlaying(), quality: qualityPicked() }
    uiRequest({ method: "POST", path: path, json: playerProgress(m.positionMs, m.durationMs, state, m.sessionId, tracks), session: m.sessionId }, "onReported")
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
    m.stallTimer.control = "stop"
    hideBanner()
    if m.durationMs > 0 then m.positionMs = m.durationMs
    report("stopped")
    endSession()
    ' The sleep timer's end: nothing plays after this one.
    if sleepEndsWith(m.sleep, urlValue(m.itemId))
        uiNavigate({ back: true })
        return
    end if
    if m.next = invalid
        uiNavigate({ back: true })
        return
    end if
    if not m.upNextCancelled and playerAutoplay(playbackSection())
        playNext()
        return
    end if
    ' Up Next waits at the end, as on the other apps: counting down again
    ' after a Cancel when the next plays by itself, for Play now when not.
    m.ended = true
    m.upNextCancelled = false
    showUpNext()
end sub

' The media ended moments after it started, a minute or more before the
' title's end, as on Android: a file played as it is is converted, and a
' stream opened again once for the title; falling short after that, the
' player says so. Nothing is reported watched.
sub endedShort()
    m.stallTimer.control = "stop"
    hideBanner()
    if m.info <> invalid and httpString(m.info.method) <> "transcode" and m.mode <> "transcode"
        open(m.itemId, m.startMs, m.item, "transcode")
        return
    end if
    if m.shortTries > 0
        endSession()
        notice(t("player.errors.cantPlay"), t("player.errors.endsEarly"), ["back"])
        return
    end if
    m.shortTries = m.shortTries + 1
    open(m.itemId, m.startMs, m.item, m.mode)
end sub

' Leaving the player: the place is kept and the session closed (or, still
' to come, closed when it does).
sub finish()
    abandonPlayback()
    m.attempt = m.attempt + 1
    m.tracksFrom = -1
    m.reportAtPlay = false
    endQuiet()
    m.countdownTimer.control = "stop"
    m.sleepTimer.control = "stop"
    m.pausedTimer.control = "stop"
    m.sleepNotice.visible = false
    m.stallTimer.control = "stop"
    m.detailsTimer.control = "stop"
    endReconnect()
    hideBanner()
    newTitle()
    m.toastTimer.control = "stop"
    m.toast.visible = false
    stopPictures()
    stopPreviews()
    hideControls()
    m.holdKey = ""
    m.holdTimer.control = "stop"
    m.settleTimer.control = "stop"
    setLoading(false)
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
    stopPictures()
    m.sessionId = ""
    m.video.control = "stop"
    lines = playerStoppedMessage(data).Split(Chr(10))
    message = []
    if lines.Count() > 1 then message.Push(lines[1])
    uiNavigate({ back: true, message: { title: lines[0], lines: message } })
end sub

sub broken()
    ' The first stream after the app starts can fail at once with a reader
    ' error, and play when given again as it is (a Roku Ultra on OS 16):
    ' before anything played, a stream that fails is given once more before
    ' it counts as broken. One that fails again goes on as below.
    m.againPending = false
    if not m.played and not m.givenAgain and m.video.content <> invalid
        m.givenAgain = true
        m.againPending = true
        content = m.video.content
        subtitle = httpString(m.video.subtitleTrack)
        m.video.content = invalid
        m.video.content = content
        if subtitle <> "" then m.video.subtitleTrack = subtitle
        m.video.control = "play"
        return
    end if
    if playerErrorKind(m.video.errorCode, m.video.errorInfo, m.played) = "media"
        ' The original doesn't play on this Roku after all: converted, once.
        if m.info <> invalid and httpString(m.info.method) <> "transcode" and m.mode <> "transcode"
            open(m.itemId, m.positionMs, m.item, "transcode")
            return
        end if
        endSession()
        text = t("player.errors.unplayable")
        if m.info <> invalid
            reason = httpSentence(m.info.reason)
            if reason <> "" then text = text + " " + reason
        end if
        notice(t("player.errors.cantPlay"), text, ["back"])
        return
    end if
    if m.sessionId = ""
        notice(t("player.errors.cantPlay"), t("player.errors.stalled"), ["retry", "back"])
        return
    end if
    ' What became of the session says what to do.
    setLoading(true)
    uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onSessionState")
end sub

sub onSessionState(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId then return
    answer = event.getData()
    meaning = playerSessionState(answer.status)
    if meaning = "stopped"
        stopped(answer.data)
    else if meaning = "unreachable"
        reconnect()
    else if meaning <> "signed_out"
        restart()
    end if
end sub

' The server doesn't answer. Once something played, the player keeps its
' place (and its queue) and asks again every 2 s, saying "Reconnecting…",
' as long as the server keeps a session it doesn't hear from (90 s), as the
' other apps do; before anything played, or after that, it says so, with
' Try again.
sub reconnect()
    now = m.clock.TotalMilliseconds()
    if m.reconnectFrom < 0 then m.reconnectFrom = now
    if not m.everPlayed or now - m.reconnectFrom >= playerReconnectMs()
        session = m.global.session
        notice(t("player.errors.cantReach", { server: session.serverName }), t("player.errors.cantReachText", { address: urlDisplayAddress(session.server) }), ["retry", "back"])
        return
    end if
    hideBanner()
    m.controls.bufferingText = t("player.reconnecting")
    setLoading(true)
    m.reconnectTimer.control = "start"
end sub

' The session asked about again, or, when none came, asked for again.
sub onReconnectTick()
    if m.reconnectFrom < 0 then return
    if m.sessionId = ""
        open(m.itemId, m.positionMs, m.item, m.mode)
    else
        uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onSessionState")
    end if
end sub

' The server answers again (the video plays), or the player gave up.
sub endReconnect()
    m.reconnectFrom = -1
    m.reconnectTimer.control = "stop"
    m.controls.bufferingText = ""
end sub

' --- Stalls ---

' Once a second: whether the playhead stood still 12 s while the video is
' meant to play (not paused, ended, reconnecting or saying why it
' stopped), as the other apps watch for.
sub onStallTick()
    now = m.clock.TotalMilliseconds()
    state = m.video.state
    meant = m.played and not m.ended and m.reconnectFrom < 0 and not m.notice.visible and (state = "playing" or state = "buffering")
    if not meant then m.stillSince = now
    if not meant or now - m.stillSince < playerStallMs() then return
    m.stillSince = now
    starving = false
    if state = "buffering"
        status = m.video.bufferingStatus
        if GetInterface(status, "ifAssociativeArray") <> invalid and GetInterface(status.isUnderrun, "ifBoolean") <> invalid then starving = status.isUnderrun
    end if
    method = httpString(m.info.method)
    ' A file that has its data and doesn't move doesn't play smoothly here:
    ' converted, once.
    if not starving and method = "direct" and m.mode <> "transcode"
        open(m.itemId, m.positionMs, m.item, "transcode")
        return
    end if
    ' A stream waits on the server: how fast it converts says whether a
    ' lower quality would help.
    if method <> "direct" and m.sessionId <> ""
        uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onStallChecked")
        return
    end if
    restart()
end sub

sub onStallChecked(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId then return
    answer = event.getData()
    meaning = playerSessionState(answer.status)
    if meaning = "stopped"
        stopped(answer.data)
        return
    end if
    if meaning = "unreachable"
        reconnect()
        return
    end if
    if meaning = "signed_out" then return
    speed = 0
    if answer.ok and GetInterface(answer.data, "ifAssociativeArray") <> invalid
        transcode = httpObject(answer.data.transcode)
        if transcode <> invalid then speed = httpNumber(transcode.speed, 0)
    end if
    if speed > 0 and speed < 1
        ' Automatic goes lower by itself; otherwise the person is told, once.
        if m.watch = invalid and not m.slowSaid
            m.slowSaid = true
            showToast(t("player.toasts.slowServer"))
        end if
        return
    end if
    restart()
end sub

' The session lives on (the token ran out) or expired: a new one from the
' same place, with the current token. The server ends the old one itself
' (same client_id).
sub restart()
    now = m.clock.TotalMilliseconds()
    if not playerMayRestart(m.restarts, now)
        notice(t("player.errors.cantPlay"), t("player.errors.stalled"), ["retry", "back"])
        return
    end if
    m.restarts.Push(now)
    open(m.itemId, m.positionMs, m.item, m.mode)
end sub

' What went wrong, over the player, with buttons for actions: "retry"
' (Try again) and "back".
sub notice(title as string, text as string, actions as object)
    endReconnect()
    hideBanner()
    m.stallTimer.control = "stop"
    setLoading(false)
    m.menuButton = ""
    closeMenu()
    endSeeking()
    hideControls()
    hideSkip()
    hideUpNext()
    m.noticeTitle.text = title
    m.noticeText.text = text
    m.noticeActions = actions
    buttons = []
    for each action in actions
        if action = "retry"
            buttons.Push(t("common.actions.tryAgain"))
        else
            buttons.Push(t("common.actions.back"))
        end if
    end for
    m.noticeButtons.buttons = buttons
    ' The buttons centered under the words.
    m.noticeButtons.translation = [960 - Int(m.noticeButtons.boundingRect().width / 2), 700]
    m.notice.visible = true
    m.noticeButtons.setFocus(true)
end sub

sub onNoticeButton(event as object)
    if m.noticeActions[event.getData()] = "retry"
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
    m.menuButton = ""
    closeMenu()
    endSeeking()
    hideControls()
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
    m.countdown.text = t("player.upNext.nextIn", { seconds: m.countdownLeft })
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
        m.countdown.text = t("player.upNext.nextIn", { seconds: m.countdownLeft })
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
    ' At the end, there is nothing left to play.
    if m.ended
        uiNavigate({ back: true })
        return
    end if
    m.keys.setFocus(true)
    updateSkip()
end sub

' Up Next shows at the credits: moving on counts the episode as watched.
' The user's remote moves on from anywhere (finished false): the episode
' keeps its place.
sub playNext(finished = true as boolean)
    nextEpisode = m.next
    hideUpNext()
    if nextEpisode = invalid then return
    if m.sessionId <> ""
        if finished and m.durationMs > 0 then m.positionMs = m.durationMs
        report("stopped")
        endSession()
    end if
    m.video.control = "stop"
    m.restarts = []
    newTitle()
    ' A cast's audio was for the title it named.
    m.playAudio = -1
    m.bannerDue = true
    m.shortTries = 0
    if m.queue <> invalid then m.queueAt = m.queueAt + 1
    open(nextEpisode.itemId, nextEpisode.startMs, invalid, "auto")
end sub

' The keys, as the Android TV player's (TvPlayerKeys.kt). While the sleep
' timer's Keep watching, a panel, subtitle sync or Up Next shows, they are
' theirs: Back closes them, and play, fast forward and rewind act on the
' video. Otherwise:
' - Controls hidden: OK, Up and Left or Right bring them up on the
'   scrubber (Left and Right seeking from there), Down on Play/Pause, and
'   Back leaves the player (the scene), which closes the session.
' - Controls shown: Left and Right seek on the scrubber and move along the
'   row; Up and Down go between them (Up from the scrubber to Skip); OK
'   plays or pauses on the scrubber, or confirms a seek coming, and presses
'   a button; Back lets a seek coming go, then hides them. They hide 5 s
'   after the last key while it plays.
' - Whatever shows: play plays or pauses, fast forward and rewind jump
'   30 s (the controls show where to), replay goes back 10 s, and Options
'   (*) opens the subtitles.
' A Roku tells a key's press and release: Left or Right not let go within
' 0.4 s is held, and scrubs until it is.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press
        if key = m.holdKey then endHold()
        return false
    end if
    ' Any key of the person's own: the remote's pause is theirs now.
    if m.remoteQuiet then endQuiet()
    if m.holdKey <> "" and key <> m.holdKey then endHold()
    ' The sleep timer's Keep watching takes OK; Back leaves it to count
    ' down, and the other keys do as they would without it.
    if m.sleepButtons.isInFocusChain()
        if key = "OK" then return false
        focusKeys()
        if key = "back" then return true
    end if
    if m.menu.visible
        if key = "back" or key = "left"
            menuBack()
            return true
        end if
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
        ' At the end, Play plays the next one.
        if m.ended and key = "play"
            playNext()
            return true
        end if
        if key = "play" or key = "fastforward" or key = "rewind" or key = "replay"
            cancelUpNext()
            playerKey(key)
            return true
        end if
        return false
    end if
    if m.notice.visible or m.info = invalid then return false
    if key = "play"
        togglePlay()
        return true
    end if
    if key = "fastforward" or key = "rewind"
        direction = 1
        if key = "rewind" then direction = -1
        showControls("scrubber")
        stepSeek(direction, skrJumpMs(), skrSettleMs())
        return true
    end if
    if key = "replay"
        if not m.controls.shown then m.controls.badgeForward = false
        stepSeek(-1, skrStepMs(), skrSettleMs())
        return true
    end if
    if key = "options"
        openPanel("subtitles", "subtitles")
        return true
    end if
    if not m.controls.shown
        if key = "OK"
            if not confirmSeek() then showControls("scrubber")
            return true
        end if
        if key = "left" or key = "right"
            showControls("scrubber")
            pressSeek(key)
            return true
        end if
        if key = "up"
            showControls("scrubber")
            return true
        end if
        if key = "down"
            showControls("playPause")
            return true
        end if
        return false
    end if
    ' Shown: any key keeps them up a while longer.
    restartHide()
    if key = "back"
        if m.seek.target >= 0
            skrCancel(m.seek)
            m.settleTimer.control = "stop"
            showTarget()
            restartHide()
        else
            hideControls()
        end if
        return true
    end if
    if m.skip.hasFocus()
        ' Up on the scrubber went to Skip: Down comes back.
        if key = "down"
            focusControl("scrubber")
            return true
        end if
        return false
    end if
    if m.focusKey = "scrubber"
        if key = "left" or key = "right"
            pressSeek(key)
        else if key = "OK"
            if not confirmSeek() then togglePlay()
        else if key = "up"
            if m.skip.visible
                m.focusKey = ""
                m.controls.focusKey = ""
                m.skip.setFocus(true)
            end if
        else if key = "down"
            focusControl("playPause")
        end if
        return true
    end if
    if key = "up"
        focusControl("scrubber")
    else if key = "left" or key = "right"
        moveAlongRow(key)
    else if key = "OK"
        pressButton(m.focusKey)
    else if key = "down" and m.focusKey = ""
        focusControl("playPause")
    end if
    return true
end function

' --- The controls ---

sub setupControls()
    m.seek = skrNew()
    ' What has the focus in them: "scrubber" or a button's id.
    m.focusKey = "scrubber"
    m.buttonIds = []
    ' Left or Right held (or waiting to be called held), and which way.
    m.holdKey = ""
    m.holdDirection = 0
    m.hideTimer = m.top.findNode("hideTimer")
    m.hideTimer.observeField("fire", "onHideTimer")
    m.settleTimer = m.top.findNode("settleTimer")
    m.settleTimer.observeField("fire", "onSettle")
    m.badgeTimer = m.top.findNode("badgeTimer")
    m.badgeTimer.observeField("fire", "onBadgeTimer")
    m.holdTimer = m.top.findNode("holdTimer")
    m.holdTimer.observeField("fire", "onHoldTimer")
    m.confirmTimer = m.top.findNode("confirmTimer")
    m.confirmTimer.observeField("fire", "onConfirmTimer")
    m.bufferingTimer = m.top.findNode("bufferingTimer")
    m.bufferingTimer.observeField("fire", "onBufferingTimer")
    ' The seek previews: the task cutting them out of the BIF, the BIF it
    ' fetched, the file's chapters for the preview's name.
    m.previewTask = invalid
    m.previewUrl = ""
    m.chapterList = []
end sub

' What the controls say of what plays, as it begins: its title, how it
' plays, its length, chapters and markers, and the buttons it has.
sub fillControls()
    banner = playerBanner(m.item)
    m.controls.title = banner.title
    m.controls.line = banner.line
    m.controls.method = detMethod(m.info)
    m.controls.methodKind = httpString(m.info.method)
    m.controls.durationMs = m.durationMs
    m.controls.positionMs = m.positionMs
    m.chapterList = detChapters(playingFile())
    starts = []
    if m.chapterList.Count() > 1
        for each chapter in m.chapterList
            starts.Push(chapter.startMs)
        end for
    end if
    m.controls.chapters = starts
    markers = []
    for each marker in m.markers
        label = t("player.controls.intro")
        if marker.kind = "credits" then label = t("player.controls.credits")
        markers.Push({ startMs: marker.startMs, endMs: marker.endMs, label: label })
    end for
    m.controls.markers = markers
    m.controls.buttons = controlButtons()
end sub

' The row, as the Android TV player's: Chapters (for two or more) and the
' sleep timer at the left; Previous, Play/Pause and Next in the middle
' (Previous and Next for an episode, a queue or chapters); Subtitles,
' Audio (for a file with tracks), Quality and Info at the right.
function controlButtons() as object
    buttons = []
    if m.chapterList.Count() > 1 then buttons.Push({ id: "chapters", icon: "list-video", label: t("player.menu.chapters"), group: "left" })
    buttons.Push({ id: "sleep", icon: "moon", label: t("player.sleep.title"), group: "left" })
    previousNext = m.chapterList.Count() > 1 or m.queue <> invalid or httpString(m.item.kind) = "episode"
    if previousNext then buttons.Push({ id: "previous", icon: "skip-back", label: t("player.controls.previous"), group: "center" })
    buttons.Push({ id: "playPause", icon: "pause", label: t("player.controls.pause"), group: "center" })
    if previousNext then buttons.Push({ id: "next", icon: "skip-forward", label: t("player.controls.next"), group: "center" })
    tracks = mediaTracks(playingFile())
    buttons.Push({ id: "subtitles", icon: "captions", label: t("player.menu.subtitles"), group: "right" })
    if tracks.audio.labels.Count() > 0 then buttons.Push({ id: "audio", icon: "audio-lines", label: t("player.menu.audio"), group: "right" })
    buttons.Push({ id: "quality", icon: "gauge", label: t("player.menu.quality"), group: "right" })
    buttons.Push({ id: "info", icon: "info", label: t("player.controls.info"), group: "right" })
    m.buttonIds = []
    for each group in ["left", "center", "right"]
        for each button in buttons
            if button.group = group then m.buttonIds.Push(button.id)
        end for
    end for
    return buttons
end function

' Up on the scrubber or Down on Play/Pause brings them up there; they
' hide 5 s after the last key while it plays.
sub showControls(focus as string)
    if m.info = invalid or m.notice.visible or m.upNext.visible or m.menu.visible then return
    m.controls.positionMs = m.positionMs
    m.controls.playing = m.video.state <> "paused"
    m.controls.paused = m.video.state = "paused"
    focusControl(focus)
    m.controls.shown = true
    if m.skip.hasFocus() or m.sleepButtons.isInFocusChain() then m.keys.setFocus(true)
    hideBanner()
    restartHide()
    placeSkip()
    placeSleepNotice()
end sub

sub hideControls()
    m.hideTimer.control = "stop"
    if not m.controls.shown then return
    m.controls.shown = false
    if m.seek.target >= 0
        skrCancel(m.seek)
        m.settleTimer.control = "stop"
        showTarget()
    end if
    placeSkip()
    placeSleepNotice()
    updateSkip()
    if m.skip.visible and not m.sleepButtons.isInFocusChain() then m.skip.setFocus(true)
end sub

' Whatever seek is on its way ends: a held key, the presses waiting, the
' badge (a new opening, Up Next, a notice).
sub endSeeking()
    m.holdKey = ""
    m.holdTimer.control = "stop"
    m.settleTimer.control = "stop"
    m.confirmTimer.control = "stop"
    m.badgeTimer.control = "stop"
    m.controls.badge = ""
    skrCancel(m.seek)
    showTarget()
end sub

sub focusControl(key as string)
    m.focusKey = key
    m.controls.focusKey = key
    if m.skip.hasFocus() then m.keys.setFocus(true)
end sub

sub restartHide()
    m.hideTimer.control = "stop"
    if m.controls.shown and m.video.state = "playing" and m.seek.target < 0 then m.hideTimer.control = "start"
end sub

sub onHideTimer()
    if m.video.state <> "playing" or m.seek.target >= 0 or m.holdKey <> "" then return
    hideControls()
end sub

' Skip's right edge at the safe area's, its bottom at y 978, or over the
' scrubber while the controls show (Android TV's).
sub placeSkip()
    if m.controls.shown
        m.skip.translation = [1544, 650]
    else
        m.skip.translation = [1544, 918]
    end if
end sub

' The sleep timer's notice at the bottom left, above the controls while
' they show.
sub placeSleepNotice()
    if m.controls.shown
        m.sleepNotice.translation = [96, 672]
    else
        m.sleepNotice.translation = [96, 890]
    end if
end sub

' Left and Right along the row, stopping at its ends.
sub moveAlongRow(key as string)
    at = -1
    for i = 0 to m.buttonIds.Count() - 1
        if m.buttonIds[i] = m.focusKey then at = i
    end for
    if at < 0 then return
    if key = "left" then at = at - 1 else at = at + 1
    if at < 0 or at >= m.buttonIds.Count() then return
    focusControl(m.buttonIds[at])
end sub

sub pressButton(id as string)
    if id = "playPause"
        togglePlay()
    else if id = "previous"
        previousPlace()
    else if id = "next"
        nextPlace()
    else if id <> ""
        openPanel(id, id)
    end if
end sub

' Play or pause; the person's pause brings up the controls, with when it
' ends.
sub togglePlay()
    if m.video.state = "paused"
        m.video.control = "resume"
    else
        m.video.control = "pause"
    end if
end sub

' Previous: the chapter's start, or the one before within its first 3 s;
' the start without chapters.
sub previousPlace()
    at = skrAt(m.seek, m.positionMs)
    target = 0
    if m.chapterList.Count() > 1
        current = detChapterAt(m.chapterList, at)
        target = m.chapterList[current].startMs
        if at - target <= 3000 and current > 0 then target = m.chapterList[current - 1].startMs
    end if
    seekNow(target)
end sub

' Next: the next episode or the queue's next, else the next chapter.
sub nextPlace()
    if m.next <> invalid
        skrCancel(m.seek)
        playNext(false)
        return
    end if
    at = skrAt(m.seek, m.positionMs)
    for each chapter in m.chapterList
        if chapter.startMs > at + 1000
            seekNow(chapter.startMs)
            return
        end if
    end for
end sub

' A seek of its own (Previous, Next, a chapter): one coming gives way.
sub seekNow(targetMs as integer)
    skrCancel(m.seek)
    m.settleTimer.control = "stop"
    if m.upNext.visible then cancelUpNext()
    seekTo(targetMs)
    showTarget()
    m.confirmTimer.control = "stop"
end sub

' --- Seeking (Seeker.brs) ---

' Left or Right: a step of 10 s, waiting for OK (5 s at most), and the
' key watched for being held.
sub pressSeek(key as string)
    direction = 1
    if key = "left" then direction = -1
    stepSeek(direction, skrStepMs(), skrWaitMs())
    m.holdKey = key
    m.holdDirection = direction
    m.holdTimer.control = "stop"
    m.holdTimer.duration = skrHoldAfterMs() / 1000
    m.holdTimer.control = "start"
end sub

sub stepSeek(direction as integer, stepMs as integer, waitMs as integer)
    skrPress(m.seek, direction, stepMs, waitMs, m.positionMs, m.durationMs)
    m.hideTimer.control = "stop"
    m.settleTimer.control = "stop"
    m.settleTimer.duration = m.seek.waitMs / 1000
    m.settleTimer.control = "start"
    if not m.controls.shown
        m.controls.badgeForward = direction > 0
        m.controls.badge = skrBadgeText(m.seek.badgeMs)
        m.badgeTimer.control = "stop"
        m.badgeTimer.control = "start"
    end if
    showTarget()
end sub

' The hold timer: first, the key is held; then each tick moves the target
' on. A release lost on the way (some Rokus lose one) ends it after 30 s.
sub onHoldTimer()
    if m.holdKey = ""
        m.holdTimer.control = "stop"
        return
    end if
    now = m.clock.TotalMilliseconds()
    if not m.seek.holding
        skrHoldStart(m.seek, now, m.positionMs)
        m.settleTimer.control = "stop"
        m.holdTimer.control = "stop"
        m.holdTimer.duration = 0.1
        m.holdTimer.control = "start"
        return
    end if
    if now - m.seek.holdStart > 30000
        m.holdKey = ""
        m.holdTimer.control = "stop"
        skrCancel(m.seek)
        showTarget()
        return
    end if
    skrHoldTick(m.seek, m.holdDirection, now, m.durationMs)
    showTarget()
end sub

' The key let go: a scrub waits where it got to for OK, as presses do.
sub endHold()
    m.holdKey = ""
    m.holdTimer.control = "stop"
    if skrRelease(m.seek)
        m.settleTimer.control = "stop"
        m.settleTimer.duration = m.seek.waitMs / 1000
        m.settleTimer.control = "start"
    end if
end sub

' The presses settled: the seek goes.
sub onSettle()
    target = skrSettle(m.seek, m.seek.presses)
    if target < 0 then return
    if m.upNext.visible then cancelUpNext()
    seekTo(target)
    m.confirmTimer.control = "stop"
    m.confirmTimer.control = "start"
    showTarget()
    restartHide()
end sub

' OK while a seek is coming, or just after one, goes there and plays on.
function confirmSeek() as boolean
    result = skrConfirm(m.seek)
    if result.seekMs >= 0
        m.settleTimer.control = "stop"
        if m.upNext.visible then cancelUpNext()
        seekTo(result.seekMs)
        showTarget()
    end if
    if not result.confirmed then return false
    if m.video.state = "paused" then m.video.control = "resume"
    restartHide()
    return true
end function

sub onConfirmTimer()
    skrSettled(m.seek)
end sub

sub onBadgeTimer()
    m.controls.badge = ""
end sub

' Where the scrubber stands and its preview: the frame for the target
' asked for, and the chapter it is in.
sub showTarget()
    target = m.seek.target
    m.controls.targetMs = target
    ' Gone, its picture goes: the next seek shows none until its own comes.
    if target < 0
        m.controls.previewUri = ""
        return
    end if
    if m.previewTask <> invalid and m.previewTask.ready then m.previewTask.want = target
    m.controls.previewChapter = chapterName(target)
end sub

' "Chapter 3 · The Heist" for the chapter at ms, or its own name where it
' says it is a chapter; "" without chapters.
function chapterName(ms as integer) as string
    if m.chapterList.Count() < 2 then return ""
    at = detChapterAt(m.chapterList, ms)
    name = m.chapterList[at].name
    word = t("player.controls.chapterWord")
    if name = "" then return t("player.controls.chapter", { number: at + 1 })
    if LCase(Left(name, 7)) = "chapter" or LCase(Left(name, Len(word))) = LCase(word) then return name
    return t("player.controls.chapterNamed", { number: at + 1, name: name })
end function

' --- Seek previews (PreviewTask) ---

' The stream's previews, fetched once for its BIF: the same address
' opened again keeps the task it has.
sub startPreviews(url as string)
    if url = m.previewUrl and m.previewTask <> invalid then return
    stopPreviews()
    if url = "" then return
    m.previewUrl = url
    m.previewTask = CreateObject("roSGNode", "PreviewTask")
    m.previewTask.url = url
    m.previewTask.userAgent = m.global.session.userAgent
    m.previewTask.observeField("ready", "onPreviewsReady")
    m.previewTask.observeField("frame", "onPreviewFrame")
    m.previewTask.control = "run"
end sub

sub stopPreviews()
    if m.previewTask <> invalid
        m.previewTask.unobserveField("ready")
        m.previewTask.unobserveField("frame")
        m.previewTask.quit = true
    end if
    m.previewTask = invalid
    m.previewUrl = ""
    m.controls.previewsReady = false
    m.controls.previewUri = ""
end sub

sub onPreviewsReady()
    m.controls.previewsReady = m.previewTask <> invalid and m.previewTask.ready
    if m.seek.target >= 0 then showTarget()
end sub

sub onPreviewFrame(event as object)
    frame = event.getData()
    if frame <> invalid then m.controls.previewUri = frame.uri
end sub

' --- Loading ---

' The ring shows half a second into a wait, as on Android TV.
sub setLoading(on as boolean)
    if on
        if not m.controls.buffering and m.bufferingTimer.control <> "start" then m.bufferingTimer.control = "start"
    else
        m.bufferingTimer.control = "stop"
        m.controls.buffering = false
    end if
end sub

sub onBufferingTimer()
    m.controls.buffering = true
end sub

' --- The side panels ---

' A button's panel, as the Android TV player's: the controls give way to
' it, and come back on the button as it closes.
sub openPanel(kind as string, button as string)
    if m.info = invalid then return
    m.menuButton = button
    ' Shown first, so the skip button stays away as the controls hide.
    m.menu.visible = true
    hideSkip()
    hideControls()
    m.menuSlide.control = "start"
    if kind = "subtitles" or kind = "audio"
        openTracks(kind)
    else if kind = "quality"
        openQualities()
    else if kind = "chapters"
        openChapters()
    else if kind = "sleep"
        openSleep()
    else
        openDetails()
    end if
end sub

' Back in a panel: from Find subtitles' steps to Subtitles, else closed.
sub menuBack()
    kind = m.menuKind
    if kind = "finding" or kind = "found" or kind = "adding"
        openTracks("subtitles")
    else if kind = "findLanguage"
        findSubtitles()
    else
        closeMenu()
    end if
end sub

' The panel goes; the controls come back on the button that opened it.
sub closeMenu()
    if not m.menu.visible then return
    m.menu.visible = false
    m.menuKind = ""
    m.detailsTimer.control = "stop"
    m.keys.setFocus(true)
    button = m.menuButton
    m.menuButton = ""
    if button <> "" and m.info <> invalid and not m.notice.visible and not m.upNext.visible
        showControls(button)
    else
        updateSkip()
    end if
end sub

' The file playing: the session's, else the item's first.
function playingFile() as dynamic
    if m.item = invalid then return invalid
    fileId = invalid
    if m.info <> invalid then fileId = m.info.file_id
    return contentFile(m.item, fileId)
end function

' The quality as the panel says it: "Automatic · 8 Mbps 1080p" while
' Automatic plays, else the one chosen, named for the file.
function qualityShown() as string
    choice = aqChoice(playbackSection())
    if m.originalOnly then choice = "original"
    labels = {}
    for each quality in aqMenuFor(playingFile(), m.originalOnly)
        labels[quality.id] = quality.label
    end for
    if choice = "auto"
        now = labels["original"]
        if m.watch <> invalid and aqStepped(m.watch)
            now = aqBitrateLabel(m.watch.playingKbps, m.watch.lines)
        else if m.watch <> invalid and labels.DoesExist(m.watch.quality)
            now = labels[m.watch.quality]
        end if
        return t("player.quality.automaticAt", { quality: now })
    end if
    if labels.DoesExist(choice) then return labels[choice]
    return aqLabel(choice)
end function

' The file's subtitles (Off first) or audio (Automatic first), as on the
' title's page, the one playing checked. Subtitles end with Subtitle sync
' (for text subtitles, or pictures drawn here) and Find subtitles….
sub openTracks(kind as string)
    tracks = mediaTracks(playingFile())
    m.menuKind = kind
    if kind = "audio"
        m.menuTitle.text = t("player.menu.audio")
        list = tracks.audio
    else
        m.menuTitle.text = t("player.menu.subtitles")
        list = tracks.subtitles
    end if
    items = []
    values = []
    for i = 0 to list.labels.Count() - 1
        items.Push({ title: list.labels[i] })
        values.Push(list.values[i])
    end for
    current = list.current
    if list.labels.Count() = 0 then current = -1
    m.trackCurrent = invalid
    if current >= 0 then m.trackCurrent = list.values[current]
    if kind = "subtitles"
        if m.textTracks > 0 or m.pictureIndex >= 0
            items.Push({ title: t("player.menu.syncNow", { offset: syncLabel(m.syncMs) }) })
            values.Push("sync")
        end if
        items.Push({ title: t("player.menu.find") })
        values.Push("find")
    end if
    m.menuValues = values
    showMenuChoices(items, current)
end sub

' A track picked in the panel, or by the user's remote, is saved for the
' show or movie, as on the title's page (PUT /api/items/{id}/streams):
' audio picked goes with the subtitles shown, subtitles with the audio as
' it was picked. The stream then opens again where it was with them,
' since a conversion has the audio and the burned-in pictures it was made
' with.
sub pickTrack(kind as string, index as integer)
    file = playingFile()
    if file = invalid then return
    subtitle = Int(httpNumber(file.selected_subtitle, -1))
    audio = -2
    if kind = "audio"
        audio = index
    else
        subtitle = index
    end if
    mediaPick(m.item, file.id, audio, subtitle)
    ' The person's pick takes over from a cast's audio.
    m.playAudio = -1
    body = tracksStreamsBody(file.id, audio, subtitle)
    setLoading(true)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.itemId) + "/streams", jsonText: FormatJson(body), attempt: m.attempt }, "onTrackPicked")
end sub

' Saved or not, the stream opens again with the tracks picked: the item is
' asked for again, for the server's word on them.
sub onTrackPicked(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if answer.error = "signed_out" then return
    if not answer.ok then showToast(t("player.toasts.notSaved"))
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    stopPictures()
    open(m.itemId, position, invalid, m.mode)
end sub

' The qualities this file is offered, the one chosen checked, each with
' what it means under it.
sub openQualities()
    m.menuKind = "quality"
    m.menuTitle.text = t("player.menu.quality")
    choice = aqChoice(playbackSection())
    if m.originalOnly then choice = "original"
    items = []
    m.menuValues = []
    current = 0
    for each quality in aqMenuFor(playingFile(), m.originalOnly)
        if quality.id = choice then current = items.Count()
        items.Push({ title: quality.label, description: aqHint(quality.id) })
        m.menuValues.Push(quality.id)
    end for
    showMenuChoices(items, current)
    m.menuHint.text = t("player.menu.qualityNow", { quality: qualityShown() })
end sub

sub openChapters()
    m.menuKind = "chapters"
    m.menuTitle.text = t("player.menu.chapters")
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
    m.menuTitle.text = t("player.sleep.title")
    items = []
    m.menuValues = []
    current = 0
    for each choice in sleepChoices(playingKind())
        if choice.minutes = m.sleep.minutes then current = items.Count()
        items.Push({ title: choice.label })
        m.menuValues.Push(choice.minutes)
    end for
    showMenuChoices(items, current)
    if m.sleep.minutes <> 0 then m.menuHint.text = sleepLabel(m.sleep, sleepNow(), playingKind())
end sub

' The panel's rows, the choice made (current, -1 for none) checked and
' focused.
sub showMenuChoices(items as object, current as integer)
    for i = 0 to items.Count() - 1
        items[i].checked = i = current
    end for
    m.menuList.content = nodesContent(items)
    at = current
    if at < 0 then at = 0
    m.menuList.jumpToItem = at
    m.menuText.visible = false
    m.menuFacts.visible = false
    m.menuList.visible = true
    m.menuHint.text = ""
    m.menuList.setFocus(true)
end sub

sub onMenuChoiceFocused()
end sub

' A row chosen: a track, a quality (kept as the Roku's: Settings shows it,
' and the title opens again at it where it was), a chapter gone to, the
' sleep timer set; Subtitle sync and Find subtitles… go on from there.
sub onMenuChoice(event as object)
    at = event.getData()
    kind = m.menuKind
    if kind = "found"
        if at = 0
            openFindLanguages()
        else
            addSubtitles(at - 1)
        end if
        return
    end if
    if at < 0 or at >= m.menuValues.Count() then return
    value = m.menuValues[at]
    if kind = "subtitles" and GetInterface(value, "ifString") <> invalid
        if value = "sync"
            m.menuButton = ""
            closeMenu()
            openSync()
        else
            findSubtitles()
        end if
        return
    end if
    ' A language to look in: the panel stays, and looks there.
    if kind = "findLanguage"
        m.findLanguage = value
        findSubtitles()
        return
    end if
    closeMenu()
    if kind = "chapters"
        seekNow(value)
        return
    end if
    if kind = "subtitles" or kind = "audio"
        if m.trackCurrent = invalid or value <> m.trackCurrent then pickTrack(kind, value)
        return
    end if
    if kind = "sleep"
        setSleep(value)
        return
    end if
    ' Original alone is offered where it alone plays: the quality kept stays
    ' as it is, for servers that convert.
    if m.originalOnly or value = aqChoice(playbackSection()) then return
    chooseQuality(value)
end sub

' A quality picked, in the menu or by the user's remote: kept as the
' Roku's, and the title opened again at it where it was.
sub chooseQuality(value as string)
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
    m.sleepText.text = t("player.sleep.pausingIn", { seconds: state.secondsLeft })
    if not m.sleepNotice.visible
        m.sleepNotice.visible = true
        ' The button takes the focus while nothing else is up.
        if ((m.keys.hasFocus() or m.top.hasFocus()) and not m.controls.shown) or m.skip.hasFocus() then m.sleepButtons.setFocus(true)
    end if
end sub

sub hideSleepNotice()
    if m.sleepButtons.isInFocusChain() then focusKeys()
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
    m.menuTitle.text = t("player.menu.details")
    m.detailsShown = ""
    showDetails(invalid)
    m.menuList.visible = false
    m.menuText.visible = false
    m.menuFacts.visible = true
    m.menuHint.text = ""
    m.keys.setFocus(true)
    onDetailsTick()
    m.detailsTimer.control = "start"
end sub

' While the details show, a conversion's are asked for again every 2 s, as
' on Android, and the position in them moves on.
sub onDetailsTick()
    if m.menuKind <> "details" or not m.menu.visible
        m.detailsTimer.control = "stop"
        return
    end if
    if m.sessionId <> "" and httpString(m.info.method) <> "direct"
        uiRequest({ method: "GET", path: "/api/playback/" + urlEncode(m.sessionId), session: m.sessionId }, "onDetails")
    else
        showDetails(invalid)
    end if
end sub

sub onDetails(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId or m.menuKind <> "details" then return
    answer = event.getData()
    if answer.ok and answer.data <> invalid then showDetails(httpObject(answer.data.transcode))
end sub

sub showDetails(live as dynamic)
    if m.info = invalid then return
    details = detPlayback(m.info, playingFile(), m.audioPlaying, live, m.positionMs)
    lines = detSheet(details)
    if m.pictureIndex >= 0 then lines.Push({ kind: "note", text: t("player.details.picturesDrawn") })
    if m.watch <> invalid and aqLowered(m.watch) then lines.Push({ kind: "note", text: t("player.details.qualityLowered", { quality: qualityShown() }) })
    ' Asked again every 2 s: laid out again only when something changed.
    shown = FormatJson(lines)
    if shown = m.detailsShown then return
    m.detailsShown = shown
    m.menuFacts.lines = lines
end sub

' --- Picture subtitles drawn here (Pictures.brs) ---

' The person's picture subtitles, when the app draws them: their list asked
' for at once, from where the playback starts. Without a key to fetch them
' by, the server burns them in instead.
sub startPictures()
    stopPictures()
    file = contentFile(m.item, m.info.file_id)
    index = picTrack(m.info, file, m.picturesOff)
    if index < 0 then return
    m.pictureIndex = index
    m.picturePicture = picPicture(file, 1920, 1080)
    m.pictureFolder = picFolder(m.info.url, m.global.session.server)
    if m.pictureFolder = ""
        picturesFailed()
        return
    end if
    ' What shows is these pictures: Roku's options only list text tracks.
    m.subtitleShown = index
    m.video.notificationInterval = picNotifySeconds()
    askPictures()
end sub

sub stopPictures()
    m.pictureIndex = -1
    m.pictures = invalid
    m.pictureShown = -1
    ' An answer on its way is for pictures no longer drawn.
    m.pictureAsk = m.pictureAsk + 1
    m.pictureWaiting = false
    m.pictureAgain = false
    m.pictureLastMs = -1
    m.picturesTimer.control = "stop"
    for each frame in m.pictureFrames
        frame.opacity = 0
        frame.uri = ""
    end for
    m.pictureFailed = {}
    m.video.notificationInterval = 1
end sub

' The list of the track's pictures, from where the video is: one request at
' a time, the next a few seconds after the last answer while the track
' isn't whole.
sub askPictures()
    if m.pictureIndex < 0 then return
    ' A request still out, or the session between two (a stream given
    ' again): a look later, not none, or the track stopped at its first
    ' answer.
    if m.pictureWaiting or m.sessionId = ""
        m.picturesTimer.control = "start"
        return
    end if
    m.picturesTimer.control = "stop"
    m.pictureWaiting = true
    path = picListPath(m.pictureFolder, m.pictureIndex, m.positionMs)
    ' The session's key opens it: no token.
    uiRequest({ method: "GET", path: path, auth: false, build: "pictures", ask: m.pictureAsk }, "onPictures")
end sub

sub onPictures(event as object)
    if event.getRoSGNode().spec.ask <> m.pictureAsk then return
    m.pictureWaiting = false
    answer = event.getData()
    kind = picAnswerKind(answer)
    if kind = "unavailable"
        picturesFailed()
        return
    end if
    ' A session stopped: the player hears of it itself.
    if kind = "stopped" then return
    if kind = "ok"
        m.pictures = picMerge(m.pictures, answer.data)
        m.pictureShown = -2
        drawPictures()
        if m.pictures.complete then return
    end if
    ' A seek while it was asked for: the pictures where the video is now.
    if m.pictureAgain
        m.pictureAgain = false
        askPictures()
        return
    end if
    m.picturesTimer.control = "start"
end sub

' A seek while the track isn't whole: the pictures where the video is now
' are asked for at once, rather than at the next look.
sub followPictures()
    if m.pictureIndex < 0 then return
    now = m.clock.TotalMilliseconds()
    seeked = picSeeked(m.pictureLastMs, m.pictureLastAt, m.positionMs, now)
    m.pictureLastMs = m.positionMs
    m.pictureLastAt = now
    if not seeked or (m.pictures <> invalid and m.pictures.complete) then return
    if m.pictureWaiting
        m.pictureAgain = true
    else
        askPictures()
    end if
end sub

' The server doesn't make pictures (an older server or transcoder): they
' are burned in from now on, the stream opening again where it is, as on
' the other TVs.
sub picturesFailed()
    m.picturesOff = true
    stopPictures()
    if m.info = invalid then return
    position = m.positionMs
    seconds = m.video.position
    if seconds <> invalid and seconds > 0 then position = Int(seconds * 1000)
    open(m.itemId, position, m.item, m.mode)
end sub

' The picture for now, while the video plays, at its place on the screen;
' the next one loads meanwhile in the other frame, which then comes to the
' front. Paused or seeking, Roku's bar has the bottom of the screen.
sub drawPictures()
    if m.pictureIndex < 0 then return
    plan = { show: -1, preload: -1 }
    if m.pictures <> invalid then plan = picPlan(m.pictures, m.positionMs, m.syncMs)
    shown = plan.show
    if m.video.state <> "playing" then shown = -1
    base = m.global.session.server + m.pictureFolder
    if shown <> m.pictureShown
        m.pictureShown = shown
        if shown >= 0
            cue = m.pictures.cues[shown]
            uri = base + cue.image
            if m.pictureFrames[m.pictureFront].uri <> uri and m.pictureFrames[1 - m.pictureFront].uri = uri then m.pictureFront = 1 - m.pictureFront
            front = m.pictureFrames[m.pictureFront]
            loadPicture(front, uri)
            ' Not loading, as it failed a moment ago: looked at again next time.
            if front.uri <> uri then m.pictureShown = -2
            place = picBox(cue, m.pictures, picPlane(m.pictures, m.picturePicture, 1920, 1080))
            front.translation = [place.x, place.y]
            front.width = place.width
            front.height = place.height
            front.opacity = 1
        else
            m.pictureFrames[m.pictureFront].opacity = 0
        end if
        m.pictureFrames[1 - m.pictureFront].opacity = 0
    end if
    if plan.preload >= 0 then loadPicture(m.pictureFrames[1 - m.pictureFront], base + m.pictures.cues[plan.preload].image)
end sub

' Has frame load uri, unless its load failed a moment ago (picLoadAgain).
sub loadPicture(frame as object, uri as string)
    if frame.uri = uri then return
    if not picLoadAgain(m.pictureFailed[uri], m.clock.TotalMilliseconds()) then return
    frame.uri = uri
end sub

' A picture that didn't load (a timeout, the server busy) loads again a few
' seconds later, when it shows or comes next.
sub onPictureLoad(event as object)
    if event.getData() <> "failed" then return
    frame = event.getRoSGNode()
    uri = httpString(frame.uri)
    if uri = "" then return
    m.pictureFailed[uri] = m.clock.TotalMilliseconds()
    frame.uri = ""
    if frame.opacity > 0 then m.pictureShown = -2
end sub

' --- Find subtitles… (SubtitleSearch.brs) ---

' The server's results for the file playing, in the person's subtitle
' language, listed in the menu.
sub findSubtitles()
    m.menuKind = "finding"
    m.menuTitle.text = t("player.find.title")
    m.found = []
    showMenuText(t("player.find.searching"))
    m.keys.setFocus(true)
    fileId = 0
    if m.info <> invalid then fileId = m.info.file_id
    uiRequest({ method: "GET", path: ssSearchPath(m.itemId, fileId, m.findLanguage), timeoutMs: 30000, session: m.sessionId }, "onSubtitlesFound")
end sub

' The languages to look in, the one looked in checked.
sub openFindLanguages()
    m.menuKind = "findLanguage"
    m.menuTitle.text = t("player.find.language")
    choice = ssLanguageChoices(m.findLanguage)
    items = []
    for each label in choice.labels
        items.Push({ title: label })
    end for
    m.menuValues = choice.values
    showMenuChoices(items, choice.current)
end sub

sub showMenuText(text as string)
    m.menuText.text = text
    m.menuList.visible = false
    m.menuFacts.visible = false
    m.menuText.visible = true
    m.menuHint.text = t("player.menu.backGoesBack")
end sub

sub onSubtitlesFound(event as object)
    if event.getRoSGNode().spec.session <> m.sessionId or m.menuKind <> "finding" then return
    answer = event.getData()
    if not answer.ok
        showMenuText(ssProblem(answer, true))
        return
    end if
    m.found = ssResults(answer.data)
    m.menuKind = "found"
    ' The Language row first, then the results, each with what it is under
    ' it; the first result in focus.
    items = [{ title: ssLanguageEntry(m.findLanguage), description: t("player.find.languageHint") }]
    for each result in m.found
        items.Push({ title: result.label, description: result.hint })
    end for
    m.menuList.content = nodesContent(items)
    if m.found.Count() > 0
        m.menuList.jumpToItem = 1
        m.menuHint.text = ""
    else
        m.menuList.jumpToItem = 0
        m.menuHint.text = ssNothingFound(m.findLanguage)
    end if
    m.menuText.visible = false
    m.menuFacts.visible = false
    m.menuList.visible = true
    m.menuList.setFocus(true)
end sub

' The result picked is added on the server, then shown.
sub addSubtitles(at as integer)
    if at < 0 or at >= m.found.Count() or m.info = invalid then return
    m.menuKind = "adding"
    showMenuText(t("player.find.adding", { name: m.found[at].label }))
    m.keys.setFocus(true)
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
        if answer.ok then answer.message = t("player.find.noTrack")
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
    stopPictures()
    body = tracksStreamsBody(m.info.file_id, -2, index)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.itemId) + "/streams", jsonText: FormatJson(body) }, "")
    if file <> invalid then file.selected_subtitle = index
    stream = contentStream(m.info, m.item, m.global.session.server, m.positionMs)
    m.textTracks = stream.subtitleTracks.Count()
    showToast(t("player.toasts.subtitlesAdded", { track: contentSubtitleName(track, httpString(track.language)) }))
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
