' The live player (docs/API.md, "Live TV"). Tuning is POST
' /api/live/channels/{id}/watch with this Roku's client_id (the API task's),
' which ends what it watched before, so changing channel holds one
' connection; then its live HLS playlist, asked once by the API task
' (the server answers it once the channel's first segments are made, and
' says plainly why when it can't), then given to the Video node as live
' content, from its live edge. Live is live: the Video node's own controls
' are off, and nothing pauses, rewinds or skips. Up and down
' change channel within the category the player was opened from, a page
' of it at a time (GET /api/live/browse?around=), round its ends. Leaving
' sends DELETE /api/live/sessions/{session}, so the connection goes at
' once, and Live TV opens back at the channel watched.

sub init()
    m.video = m.top.findNode("video")
    m.waiting = m.top.findNode("waiting")
    m.card = m.top.findNode("card")
    m.cardInitials = m.top.findNode("cardInitials")
    m.cardLogo = m.top.findNode("cardLogo")
    m.tuning = m.top.findNode("tuning")
    m.cardTitle = m.top.findNode("cardTitle")
    m.cardLine = m.top.findNode("cardLine")
    m.banner = m.top.findNode("banner")
    m.bannerInitials = m.top.findNode("bannerInitials")
    m.bannerLogo = m.top.findNode("bannerLogo")
    m.bannerChannel = m.top.findNode("bannerChannel")
    m.bannerTitle = m.top.findNode("bannerTitle")
    m.bannerMeta = m.top.findNode("bannerMeta")
    m.bannerTrack = m.top.findNode("bannerTrack")
    m.bannerFill = m.top.findNode("bannerFill")
    m.bannerNext = m.top.findNode("bannerNext")
    m.bannerHint = m.top.findNode("bannerHint")
    m.star = m.top.findNode("star")
    m.list = m.top.findNode("list")
    m.listTitle = m.top.findNode("listTitle")
    m.listRange = m.top.findNode("listRange")
    m.channelList = m.top.findNode("channelList")
    m.notice = m.top.findNode("notice")
    m.noticeTitle = m.top.findNode("noticeTitle")
    m.noticeText = m.top.findNode("noticeText")
    m.noticeButtons = m.top.findNode("noticeButtons")
    m.toast = m.top.findNode("toast")
    m.toastText = m.top.findNode("toastText")
    m.bannerTimer = m.top.findNode("bannerTimer")
    m.zapTimer = m.top.findNode("zapTimer")
    m.clock = m.top.findNode("clock")
    m.pictureTimer = m.top.findNode("pictureTimer")
    m.toastTimer = m.top.findNode("toastTimer")
    m.stallTimer = m.top.findNode("stallTimer")
    m.stallTimer.observeField("fire", "onStall")
    ' Waiting for data since the timer started (the spinner shows after it).
    m.stalling = false
    for each spinner in [m.waiting, m.tuning]
        spinner.poster.uri = "pkg:/images/spinner.png"
        spinner.poster.width = 64
        spinner.poster.height = 64
    end for
    m.twelve = CreateObject("roDeviceInfo").GetClockFormat() <> "24h"
    m.started = false
    ' The channel shown (Live.brs's liveChannel), and the one up and down
    ' lead to before it is tuned.
    m.channel = invalid
    m.zapTo = invalid
    ' The category the player was opened from, and the page of it held
    ' ({ total, offset, channels }; up and down go through it, the list
    ' shows it); a page asked for up or down, to step into as it comes.
    m.category = "all"
    m.categoryName = "All channels"
    m.page = invalid
    m.pageAsked = 0
    ' Which tuning the answers belong to, the watch asked for last (until
    ' it answers), the session and its playlist.
    m.attempt = 0
    m.watchRequest = invalid
    m.sessionId = ""
    m.playlist = ""
    m.played = false
    ' When the stream was tuned again after breaking (playerMayRestart).
    m.restarts = []
    m.span = CreateObject("roTimespan")
    m.trouble = invalid
    m.video.notificationInterval = 1
    m.video.observeField("state", "onState")
    m.noticeButtons.observeField("buttonSelected", "onNoticeButton")
    m.channelList.observeField("itemSelected", "onListSelected")
    m.bannerTimer.observeField("fire", "onBannerTimer")
    m.zapTimer.observeField("fire", "onZapTimer")
    m.clock.observeField("fire", "onClock")
    m.pictureTimer.observeField("fire", "onPictureTimer")
    m.toastTimer.observeField("fire", "onToastTimer")
    m.cardLogo.observeField("loadStatus", "onLogos")
    m.bannerLogo.observeField("loadStatus", "onLogos")
    m.top.observeField("active", "onActive")
    m.top.observeField("liveTakenOver", "onTakenOver")
end sub

sub onActive()
    if not m.top.active
        finish()
        return
    end if
    if m.started then return
    m.started = true
    ' A channel takes over from the music.
    m.global.music.control = "stop"
    ' The playlist and its segments carry the token: a refreshed one goes
    ' to the Video node's agent at once.
    m.global.session.observeFieldScoped("accessToken", "onToken")
    m.clock.control = "start"
    params = m.top.params
    category = httpString(params.category)
    if category <> "" then m.category = category
    channel = params.channel
    if GetInterface(channel, "ifAssociativeArray") = invalid then return
    m.categoryName = httpString(params.categoryName)
    if m.categoryName = "" then m.categoryName = liveCategoryName(m.category, [])
    m.listTitle.text = m.categoryName
    ' Recently watched changes with every channel watched: its order is
    ' the one it had as the player opened.
    around = channel.id
    if m.category = "recent" then around = 0
    askPage(0, around, "")
    tune(channel)
end sub

sub onToken()
    m.video.setHttpAgent(uiHttpAgent())
end sub

function nowSeconds() as integer
    return CreateObject("roDateTime").AsSeconds()
end function

' --- The category's channels ---

' A page of the category (from offset, or around a channel), as the list
' shows it; then pick "first" or "last" of it is the channel up or down
' leads to.
sub askPage(offset as integer, around as integer, pick as string)
    m.pageAsked = m.pageAsked + 1
    uiRequest({ op: "livePage", kind: "list", category: m.category, offset: offset, limit: livePageSize(), around: around, asked: m.pageAsked, pick: pick }, "onPage")
end sub

sub onPage(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    if spec.asked <> m.pageAsked then return
    answer = event.getData()
    if not answer.ok then return
    m.page = answer.data
    content = request.content
    if content = invalid then content = CreateObject("roSGNode", "ContentNode")
    m.channelList.content = content
    markCurrent()
    m.listRange.text = liveRange(m.page.offset, m.page.channels.Count(), m.page.total)
    ' The channel shown, with what's on now.
    if m.channel <> invalid
        at = liveIndexOf(m.page.channels, m.channel.id)
        if at >= 0
            m.channel = m.page.channels[at]
            m.channelList.jumpToItem = at
            if m.banner.visible then showBanner(false)
        end if
    end if
    if spec.pick = "first" and m.page.channels.Count() > 0 then zapTo(m.page.channels[0])
    if spec.pick = "last" and m.page.channels.Count() > 0 then zapTo(m.page.channels[m.page.channels.Count() - 1])
    if spec.pick = "top" then m.channelList.jumpToItem = 0
    if spec.pick = "bottom" then m.channelList.jumpToItem = content.getChildCount() - 1
end sub

' The channel playing stands out in the list.
sub markCurrent()
    content = m.channelList.content
    if content = invalid then return
    for i = 0 to content.getChildCount() - 1
        item = content.getChild(i)
        shown = m.channel <> invalid and item.channelId = m.channel.id
        if item.current <> shown then item.current = shown
    end for
end sub

sub openList()
    hideBanner()
    m.list.visible = true
    if m.channel <> invalid and m.page <> invalid
        at = liveIndexOf(m.page.channels, m.channel.id)
        if at >= 0 then m.channelList.jumpToItem = at
    end if
    m.channelList.setFocus(true)
end sub

sub closeList()
    m.list.visible = false
    ' The list gives the focus up: its parent taking it may not.
    m.channelList.setFocus(false)
    if m.notice.visible
        m.noticeButtons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

' Past the list's ends, the page after it (or before), round the ends of
' the category.
sub listPage(by as integer)
    if m.page = invalid or m.page.total <= m.page.channels.Count() then return
    size = livePageSize()
    pages = Int((m.page.total + size - 1) / size)
    pageNo = Int(m.page.offset / size)
    pick = "top"
    if by < 0 then pick = "bottom"
    askPage(((pageNo + by + pages) mod pages) * size, 0, pick)
end sub

sub onListSelected(event as object)
    if m.page = invalid then return
    at = event.getData()
    if at < 0 or at >= m.page.channels.Count() then return
    channel = m.page.channels[at]
    closeList()
    if m.channel <> invalid and channel.id = m.channel.id and not m.notice.visible then return
    tune(channel)
end sub

' The Options key in the list: the channel in focus in or out of the
' favourites (PUT or DELETE /api/live/channels/{id}/favorite).
sub toggleFavorite()
    if m.page = invalid then return
    at = m.channelList.itemFocused
    if at < 0 or at >= m.page.channels.Count() then return
    channel = m.page.channels[at]
    favorite = not channel.favorite
    method = "DELETE"
    if favorite then method = "PUT"
    channel.favorite = favorite
    item = m.channelList.content.getChild(at)
    if item <> invalid then item.favorite = favorite
    uiRequest({ method: method, path: "/api/live/channels/" + urlValue(channel.id) + "/favorite", favorite: favorite, name: channel.name }, "onFavorite")
end sub

sub onFavorite(event as object)
    answer = event.getData()
    spec = event.getRoSGNode().spec
    if not answer.ok
        if not uiFailed(answer) then showToast(answer.message)
        return
    end if
    if spec.favorite
        showToast(spec.name + " is in your favourites")
    else
        showToast(spec.name + " is out of your favourites")
    end if
    if m.channel <> invalid and m.channel.name = spec.name then m.channel.favorite = spec.favorite
end sub

' --- Tuning ---

' Up and down: the next channel of the category (round its ends), the
' page after or before it fetched at its ends.
sub zap(by as integer)
    from = m.zapTo
    if from = invalid then from = m.channel
    if from = invalid or m.page = invalid then return
    move = liveZap(m.page, from.id, by)
    if move = invalid then return
    if move.channel <> invalid
        zapTo(move.channel)
        return
    end if
    which = "first"
    if move.last then which = "last"
    askPage(move.offset, 0, which)
end sub

' The channel's logo and name show at once, and it is tuned once the
' presses stop. What's on shows only when asked for.
sub zapTo(channel as object)
    m.zapTo = channel
    hideBanner()
    showCard(channel, "")
    m.bannerTimer.control = "stop"
    m.zapTimer.control = "stop"
    m.zapTimer.control = "start"
end sub

sub onZapTimer()
    target = m.zapTo
    m.zapTo = invalid
    if target = invalid then return
    if m.channel <> invalid and target.id = m.channel.id and m.sessionId <> ""
        ' Back where it started.
        if m.played then m.card.visible = false
        return
    end if
    tune(target)
end sub

' Watches channel: the session before it ends (the server ends it too,
' by the client_id), its logo shows while it starts.
sub tune(channel as object, takeOver = 0 as integer)
    m.attempt = m.attempt + 1
    abandonWatch()
    leave()
    m.zapTo = invalid
    m.zapTimer.control = "stop"
    m.channel = channel
    m.played = false
    m.trouble = invalid
    m.pictureTimer.control = "stop"
    m.video.control = "stop"
    m.notice.visible = false
    showWaiting(false)
    showCard(channel, "")
    hideBanner()
    markCurrent()
    if not m.list.visible then m.top.setFocus(true)
    m.watchRequest = uiRequest({ op: "liveWatch", channelId: channel.id, takeOver: takeOver, attempt: m.attempt }, "onWatch")
end sub

' The watch asked for isn't wanted any more: the API task ends it if it
' comes.
sub abandonWatch()
    if m.watchRequest = invalid then return
    m.watchRequest.abandoned = true
    m.watchRequest = invalid
end sub

' DELETE /api/live/sessions/{session}: the connection goes at once.
sub leave()
    if m.sessionId = "" then return
    uiRequest({ method: "DELETE", path: "/api/live/sessions/" + urlEncode(m.sessionId) }, "")
    m.sessionId = ""
    m.playlist = ""
end sub

function isCurrent(event as object) as boolean
    return event.getRoSGNode().spec.attempt = m.attempt
end function

sub onWatch(event as object)
    if not isCurrent(event) then return
    m.watchRequest = invalid
    answer = event.getData()
    if not answer.ok or httpObject(answer.data) = invalid
        if answer.error = "signed_out" then return
        if answer.error = "network"
            uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
            return
        end if
        trouble(liveTrouble(answer, m.channel.name))
        return
    end if
    m.sessionId = httpString(answer.data.session_id)
    m.playlist = httpString(answer.data.url)
    fresh = liveChannel(answer.data.channel, m.global.session.server, liveLogoWidth(), m.category)
    if fresh <> invalid and fresh.id = m.channel.id then m.channel = fresh
    showCard(m.channel, "")
    if m.banner.visible then showBanner(false)
    ' The playlist once, waited for: the server answers it once the
    ' channel's first segments are made, or says why not.
    uiRequest({ method: "GET", path: m.playlist, timeoutMs: 50000, attempt: m.attempt }, "onPlaylist")
end sub

sub onPlaylist(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if not answer.ok
        if answer.error = "signed_out" then return
        if answer.error = "network"
            uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
            return
        end if
        leave()
        trouble(liveTrouble(answer, m.channel.name))
        return
    end if
    play()
end sub

' The live stream in the Video node, its token in the agent's header.
sub play()
    content = CreateObject("roSGNode", "ContentNode")
    content.url = m.global.session.server + m.playlist
    content.streamFormat = "hls"
    content.title = liveTitle(m.channel)
    content.live = true
    if LCase(Left(content.url, 8)) = "https://" then nodesFill(content, { HttpCertificatesFile: "common:/certs/ca-bundle.crt" })
    m.video.setHttpAgent(uiHttpAgent())
    m.video.content = content
    m.video.control = "play"
end sub

sub onState(event as object)
    state = event.getData()
    ' A stall shows the spinner only once it has lasted a moment: the
    ' briefest flicker of one doesn't.
    if state = "buffering" and m.played
        if not m.stalling
            m.stalling = true
            m.stallTimer.control = "start"
        end if
    else
        m.stalling = false
        m.stallTimer.control = "stop"
        showWaiting(false)
    end if
    if state = "playing"
            if not m.played
            m.played = true
            m.card.visible = false
            m.tuning.control = "stop"
            ' A channel without pictures (radio) keeps its card.
            m.pictureTimer.control = "start"
        end if
        return
    end if
    if state = "error" or state = "finished"
        broken()
    end if
end sub

sub onStall()
    if m.stalling then showWaiting(true)
end sub

' Someone took over the connection of the channel playing (the server's
' live_taken_over, sent before the stream breaks off, one for each viewer
' of the channel; matched on the channel): it stops at once with the
' server's words, said once, and isn't tuned again by itself. Said already
' from the playlist's 410 without words, the event's words show instead.
sub onTakenOver(event as object)
    data = event.getData()
    if m.channel = invalid or not m.started then return
    if Int(httpNumber(data.channel_id, 0)) <> m.channel.id then return
    stopped = liveStopped(data.message)
    if m.trouble <> invalid
        if m.trouble.kind = "stopped" and httpSentence(data.message) <> "" and m.trouble.message <> stopped.message then trouble(stopped)
        return
    end if
    ' What is on its way belongs to watching that ended, and its session is
    ' gone on the server: nothing to leave.
    m.attempt = m.attempt + 1
    abandonWatch()
    m.zapTo = invalid
    m.zapTimer.control = "stop"
    m.sessionId = ""
    m.playlist = ""
    trouble(stopped)
end sub

sub showWaiting(shown as boolean)
    if shown = m.waiting.visible then return
    m.waiting.visible = shown
    if shown
        m.waiting.control = "start"
    else
        m.waiting.control = "stop"
    end if
end sub

sub onPictureTimer()
    if not m.played or m.channel = invalid then return
    video = httpString(m.video.videoFormat)
    audio = httpString(m.video.audioFormat)
    if video = "" and audio <> "" then showCard(m.channel, "radio")
end sub

' The stream broke: what the server says of its playlist says what to do.
' Gone or refused, the player says so; otherwise the channel is tuned
' again, a few times at most.
sub broken()
    if m.sessionId = "" or m.trouble <> invalid then return
    m.video.control = "stop"
    showCard(m.channel, "")
    uiRequest({ method: "GET", path: m.playlist, timeoutMs: 50000, attempt: m.attempt }, "onChecked")
end sub

sub onChecked(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if answer.error = "signed_out" then return
    if answer.error = "network"
        uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
        return
    end if
    if answer.status = 410 or answer.status = 403 or answer.status = 503
        leave()
        trouble(liveTrouble(answer, m.channel.name))
        return
    end if
    nowMs = m.span.TotalMilliseconds()
    if not playerMayRestart(m.restarts, nowMs)
        leave()
        message = "It stopped and couldn't get going again."
        trouble({ kind: "other", title: m.channel.name + " stopped", message: message, lines: message, retry: true, connections: [] })
        return
    end if
    m.restarts.Push(nowMs)
    tune(m.channel)
end sub

' --- What shows ---

' The channel's logo (or its name) in the middle, over black, the spinner
' under it while it tunes; over a channel without pictures ("radio"),
' what's on instead.
sub showCard(channel as object, kind as string)
    m.cardTitle.text = liveTitle(channel)
    on = liveOnNow(channel, nowSeconds())
    line = "Tuning…"
    if kind = "radio"
        line = "Radio"
        if on <> invalid then line = on.title
    else if on <> invalid
        line = on.title
    end if
    m.cardLine.text = line
    m.cardInitials.text = channel.name
    ' The scene's agent carries the token; it changes with each refresh.
    m.cardLogo.setHttpAgent(m.top.getScene().getHttpAgent())
    m.cardLogo.uri = channel.logo
    onLogos()
    m.tuning.visible = kind <> "radio"
    if kind = "radio"
        m.tuning.control = "stop"
    else
        m.tuning.control = "start"
    end if
    m.card.visible = true
end sub

' Names show where no logo does (logos are often see-through).
sub onLogos()
    m.cardInitials.visible = m.cardLogo.uri = "" or m.cardLogo.loadStatus = "failed"
    m.bannerInitials.visible = m.bannerLogo.uri = "" or m.bannerLogo.loadStatus = "failed"
end sub

' The banner: the channel, what's on with when and how long it has left,
' how far along, what's next, and the Live chip. With hides, it goes
' after 5 s.
sub showBanner(hides as boolean)
    channel = m.zapTo
    if channel = invalid then channel = m.channel
    if channel = invalid then return
    now = nowSeconds()
    on = liveOnNow(channel, now)
    coming = liveComingUp(channel, now)
    m.bannerChannel.text = liveTitle(channel)
    m.bannerInitials.text = channel.name
    m.bannerLogo.setHttpAgent(m.top.getScene().getHttpAgent())
    m.bannerLogo.uri = channel.logo
    onLogos()
    if on <> invalid
        m.bannerTitle.text = on.title
        meta = [liveSpan(on, m.twelve), liveTimeLeft(on, now)]
        if on.line <> "" then meta.Push(on.line)
        if on.badges <> "" then meta.Push(on.badges)
        m.bannerMeta.text = meta.Join("   ·   ")
        m.bannerFill.width = Int(1100 * liveProgress(on, now))
        m.bannerTrack.visible = true
        m.bannerFill.visible = true
    else
        m.bannerTitle.text = channel.name
        m.bannerMeta.text = "Nothing in the guide now"
        if not channel.hasGuide then m.bannerMeta.text = "No guide for this channel"
        m.bannerTrack.visible = false
        m.bannerFill.visible = false
    end if
    m.bannerNext.text = ""
    if coming <> invalid then m.bannerNext.text = "Next   " + liveClock(coming.start, m.twelve) + "   " + coming.title
    m.star.text = ""
    if channel.favorite then m.star.text = "★ Favourite"
    m.bannerHint.text = "Up and down: channels   ·   Left: " + m.categoryName + "   ·   OK: close"
    if m.list.visible or m.notice.visible then return
    m.banner.visible = true
    m.bannerTimer.control = "stop"
    if hides then m.bannerTimer.control = "start"
end sub

sub hideBanner()
    m.bannerTimer.control = "stop"
    m.banner.visible = false
end sub

sub onBannerTimer()
    if m.zapTo = invalid then hideBanner()
end sub

' Each 15 s: what's on moves on.
sub onClock()
    if m.banner.visible then showBanner(false)
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

' A watching that didn't start or stopped: what the server said, with Try
' again where it may help (Watch again when it stopped), Other channels,
' and Back.
sub trouble(problem as object)
    m.trouble = problem
    m.video.control = "stop"
    m.card.visible = false
    m.tuning.control = "stop"
    showWaiting(false)
    hideBanner()
    m.noticeTitle.text = problem.title
    m.noticeText.text = problem.lines
    ' What holds each connection takes room: the text on the left, the
    ' buttons beside it, all from the top.
    if problem.connections.Count() > 0
        m.noticeTitle.translation = [96, 120]
        m.noticeText.translation = [96, 210]
        m.noticeText.width = 920
        m.noticeText.maxLines = 16
        m.noticeButtons.translation = [1104, 210]
        m.noticeButtons.minWidth = 720
    else
        m.noticeTitle.translation = [96, 340]
        m.noticeText.translation = [96, 430]
        m.noticeText.width = 1500
        m.noticeText.maxLines = 6
        m.noticeButtons.translation = [96, 700]
        m.noticeButtons.minWidth = 400
    end if
    ' Every connection in use: what holds each, and Watch here instead
    ' where this account may take one over.
    m.noticeActions = liveTroubleButtons(problem)
    labels = []
    for each button in m.noticeActions
        labels.Push(button.label)
    end for
    m.noticeButtons.buttons = labels
    m.notice.visible = true
    if not m.list.visible then m.noticeButtons.setFocus(true)
end sub

sub onNoticeButton(event as object)
    at = event.getData()
    if m.noticeActions = invalid or at < 0 or at >= m.noticeActions.Count() then return
    button = m.noticeActions[at]
    if button.action = "takeOver"
        m.restarts = []
        tune(m.channel, button.channelId)
    else if button.action = "retry"
        m.restarts = []
        tune(m.channel)
    else if button.action = "list"
        openList()
    else
        uiNavigate({ back: true })
    end if
end sub

' Leaving: the session ends, the token's observer goes, and Live TV opens
' back at this channel.
sub finish()
    m.attempt = m.attempt + 1
    abandonWatch()
    leave()
    m.video.control = "stop"
    for each timer in [m.bannerTimer, m.zapTimer, m.clock, m.pictureTimer, m.toastTimer, m.stallTimer]
        timer.control = "stop"
    end for
    if m.channel <> invalid then m.top.getScene().liveChannel = m.channel.id
    if m.started then m.global.session.unobserveFieldScoped("accessToken")
end sub

' Up and down change channel (Up: the next number); Left opens the
' category's channels, Right, OK and the Options key (*) show what's on
' (it never shows by itself as a channel starts); play, rewind, fast
' forward and Replay do nothing. In the list:
' the Options key adds to or takes out of the favourites, up and down past
' its ends go to the page before or after, Back and Right close it. Back
' with the banner showing hides it; otherwise it leaves (the scene).
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.list.visible
        if key = "back" or key = "right"
            closeList()
            return true
        end if
        if key = "options"
            toggleFavorite()
            return true
        end if
        if key = "up" and m.channelList.itemFocused = 0
            listPage(-1)
            return true
        end if
        if key = "down" and m.channelList.content <> invalid and m.channelList.itemFocused = m.channelList.content.getChildCount() - 1
            listPage(1)
            return true
        end if
        return true
    end if
    if m.notice.visible then return false
    if key = "up"
        zap(1)
        return true
    end if
    if key = "down"
        zap(-1)
        return true
    end if
    if key = "left"
        openList()
        return true
    end if
    ' Live is live: no pause, rewind or skip.
    if key = "play" or key = "rewind" or key = "fastforward" or key = "replay" then return true
    if key = "right" or key = "OK" or key = "options"
        if m.banner.visible
            hideBanner()
        else
            showBanner(true)
        end if
        return true
    end if
    if key = "back" and m.banner.visible and m.zapTo = invalid
        hideBanner()
        return true
    end if
    return false
end function
