sub init()
    m.top.backgroundUri = ""
    m.top.backgroundColor = uiColors().bg
    m.top.palette = uiPalette()
    m.screens = m.top.findNode("screens")
    m.stack = []
    ' The sign-in screen shown at launch, until another replaces it.
    m.launchDialog = invalid

    session = CreateObject("roSGNode", "Session")
    m.global.addFields({ session: session })
    session.observeField("accessToken", "onAgentChanged")
    session.observeField("userAgent", "onAgentChanged")
    session.observeField("signedIn", "onSignedIn")
    onAgentChanged()

    m.api = CreateObject("roSGNode", "ApiTask")
    m.global.addFields({ api: m.api })

    ' Music plays on while the screens change.
    m.music = CreateObject("roSGNode", "MusicPlayer")
    m.top.appendChild(m.music)
    m.global.addFields({ music: m.music })
    m.music.observeField("stoppedBy", "onMusicStoppedBy")

    ' What the person's other devices change, as it happens (an http://
    ' server's event stream): gathered for 0.3 s, then the screens follow.
    m.events = CreateObject("roSGNode", "EventsTask")
    m.events.observeField("event", "onServerEvent")
    m.events.observeField("tokenRefused", "onStreamRefused")
    m.events.observeField("failed", "onStreamFailed")
    ' The token the stream was last refused with, renewed since.
    m.renewing = ""
    m.gather = m.top.findNode("gather")
    m.gather.observeField("fire", "onGathered")
    session.observeField("accessToken", "onStream")
    session.observeField("server", "onStream")
    m.top.observeField("liveTv", "onStream")

    ' Casts from the user's remotes (Cast.brs): which cast plays (an
    ' answer for an earlier one is left), an artist's albums gathered for
    ' it, and the notice saying where it came from. A cast from another
    ' profile of the household waits for the switch to it (castSwitched),
    ' counted so that an answer for an earlier one is left too.
    m.castAsk = 0
    m.castGather = invalid
    m.castSwitch = invalid
    m.castSwitches = 0
    m.castToast = m.top.findNode("castToast")
    m.castToastText = m.top.findNode("castToastText")
    m.castToastTimer = m.top.findNode("castToastTimer")
    m.castToastTimer.observeField("fire", "onCastToastTimer")

    ' Requests: what the profile may do (the summary), asked as a profile
    ' signs in, as the stream connects, a second after request events and,
    ' without the stream, each minute. Answers for a profile signed in
    ' before (requestsAsked) are left out.
    m.top.requestSummary = reqNoSummary(false)
    m.requestsAsked = 0
    m.requestGather = m.top.findNode("requestGather")
    m.requestGather.observeField("fire", "onRequestsGathered")
    m.requestPoll = m.top.findNode("requestPoll")
    m.requestPoll.observeField("fire", "onRequestPoll")
    ' What the gathered events changed, for the screen on top; news they
    ' told (the summary's unseen rising without any is asked about).
    m.requestChange = invalid
    m.newsEvents = 0
    ' Titles whose badge the gathered events changed ({ "movie:603":
    ' { kind, tmdbId } }), and those being asked for (true: changed again
    ' meanwhile, so asked once more).
    m.badgeTitles = {}
    m.badgesAsked = {}
    ' Ready-to-watch notices waiting to show, the one shown in the card and
    ' where the focus was before it, and what was told already
    ' (reqLoadAnnounced's) for the profile.
    m.notices = []
    m.shownNotice = invalid
    m.noticeFocus = invalid
    m.noticeCard = m.top.findNode("notice")
    m.noticeCard.observeField("event", "onNoticeEvent")
    m.announced = invalid
    ' A notice waits for the remote to rest, looked at four times a second
    ' while one waits: when a look last found a key within the second
    ' (invalid for none since they began), and when the app last counted
    ' something as a press (a screen changing, a notice closing).
    m.device = CreateObject("roDeviceInfo")
    m.noticeWait = m.top.findNode("noticeWait")
    m.noticeWait.observeField("fire", "onNoticeWait")
    m.noticeWaiting = false
    ' When the looks first found a second gone since the last key, and
    ' whether they saw that key come.
    m.keyAt = invalid
    m.keySeen = false
    m.pressAt = CreateObject("roTimespan")
    ' The dialog last closed (one may stay in the scene's field).
    m.closedDialog = invalid
    session.observeField("profileId", "onProfile")
    m.events.observeField("live", "onStreamLive")
    m.top.observeField("requestNote", "onRequestNote")
    m.top.observeField("requestTouched", "onRequestTouched")
    m.top.observeField("dialog", "onDialog")

    m.events.control = "run"
    m.api.observeField("ready", "onApiReady")
    m.api.control = "run"
end sub

' The task listens: the first request can go.
sub onApiReady()
    if m.api.ready then uiRequest({ op: "boot" }, "onBoot")
end sub

' Roku's launch beacons: AppLaunchComplete once the first screen can be
' used (Servers and sign-in at once, Home once its rows show), and around a
' sign-in shown at launch AppDialogInitiate and AppDialogComplete, whose
' time Roku leaves out of the launch's.
sub onBoot(event as object)
    answer = event.getData()
    screen = "servers"
    if answer.ok then screen = answer.data.screen
    if screen = "link" then m.top.signalBeacon("AppDialogInitiate")
    ' "Who's watching?" may open on the PIN pad of the profile signed in.
    params = {}
    if answer.ok and httpTrue(answer.data.pin) then params.pin = true
    show({ to: screen, reset: true, params: params })
    if screen = "link" then m.launchDialog = m.stack.Peek()
    if screen <> "home" then uiLaunched()
end sub

' The sign-in shown at launch is over once another screen replaces it.
sub launchDialogDone()
    if m.launchDialog = invalid then return
    if m.stack.Count() > 0 and m.stack.Peek().isSameNode(m.launchDialog) then return
    m.launchDialog = invalid
    m.top.signalBeacon("AppDialogComplete")
end sub

' The event stream follows the server and the token signed in with, and
' names this Roku a receiver of casts: of channels too while the profile
' has Live TV.
sub onStream()
    session = m.global.session
    m.events.query = castReceiverQuery(session.playerId, session.deviceName, m.top.liveTv)
    m.events.userAgent = session.userAgent
    m.events.server = session.server
    m.events.token = session.accessToken
end sub

' The stream's token ran out while nothing asked the server: any request
' has the API task renew it, once for each token.
sub onStreamRefused(event as object)
    token = event.getData()
    if token = "" or token <> m.global.session.accessToken or token = m.renewing then return
    m.renewing = token
    uiGet("/api/auth/me", "")
end sub

' The stream failed: a request the API task sends finds out whether the
' server still answers. When nothing does, the players' token waits for
' the server to prove itself again, as after any request without an answer.
sub onStreamFailed(event as object)
    token = event.getData()
    if token <> "" and token = m.global.session.accessToken then uiGet("/api/auth/me", "")
end sub

' Watch state, My List, libraries or Live TV changed elsewhere: Home asks again
' (now if it shows, else as it shows); a stream of this Roku's stopped from
' elsewhere ends at once, as does a live channel whose connection someone
' took over. The user's remote, on another device, controls the player on
' top and the music (Remote.brs); each acts only on its own session, and
' the music, playing behind the screens, hears of its own stop too. What
' the remote casts here plays (onCast).
sub onServerEvent(event as object)
    data = event.getData()
    if httpString(data.type) = "cast"
        onCast(data)
        return
    end if
    if httpString(data.type) = "session_stopped"
        m.music.serverStopped = data
        if m.stack.Count() > 0 and m.stack.Peek().hasField("serverStopped") then m.stack.Peek().serverStopped = data
        return
    end if
    if httpString(data.type) = "player_command"
        m.music.playerCommand = data
        if m.stack.Count() > 0 and m.stack.Peek().hasField("playerCommand") then m.stack.Peek().playerCommand = data
        return
    end if
    if httpString(data.type) = "live_taken_over"
        if m.stack.Count() > 0 and m.stack.Peek().hasField("liveTakenOver") then m.stack.Peek().liveTakenOver = data
        return
    end if
    if reqIsEvent(data)
        onRequestEvent(data)
        return
    end if
    if not evChangesHome(data) then return
    m.gather.control = "stop"
    m.gather.control = "start"
end sub

sub onGathered()
    m.top.remoteChanges = m.top.remoteChanges + 1
    if m.stack.Count() > 0 and m.stack.Peek().hasField("refresh") then m.stack.Peek().refresh = true
end sub

' --- Requests ---

' A profile signed in, or none: what requests let it do is asked again,
' and the notices start from what it was told before.
sub onProfile()
    session = m.global.session
    m.requestsAsked = m.requestsAsked + 1
    setRequestSummary(reqNoSummary(false))
    m.requestChange = invalid
    m.newsEvents = 0
    m.badgeTitles = {}
    m.badgesAsked = {}
    ' Live TV opens at the channel this profile watched last, not the last profile's.
    m.top.liveChannel = 0
    ' A cast for the profile before, still fetching, plays nothing.
    m.castAsk = m.castAsk + 1
    m.castGather = invalid
    ' The notices were the profile before's: one shown goes.
    m.notices = []
    if m.shownNotice <> invalid
        m.noticeCard.hide = true
        m.shownNotice = invalid
        giveFocusBack()
    end if
    showNotice()
    m.announced = invalid
    if session.profileId > 0
        m.announced = reqLoadAnnounced(requestsSection(), session.server, session.profileId)
        askRequestSummary("start")
    end if
    pollRequests()
    ' A cast that switched to this profile plays now; one that switched to
    ' another is left.
    pending = m.castSwitch
    if pending <> invalid and pending.switched and session.profileId > 0 and session.profileId <> pending.profileId then m.castSwitch = invalid
    castSwitched()
end sub

function requestsSection() as object
    return CreateObject("roRegistrySection", "requests")
end function

' why: "start", "events", "poll", "live" or "asked" (a screen did
' something, and shows what came of it).
sub askRequestSummary(why as string)
    if m.global.session.profileId <= 0 then return
    uiRequest({ method: "GET", path: "/api/requests/summary", asked: m.requestsAsked, why: why }, "onRequestSummary")
end sub

sub onRequestSummary(event as object)
    spec = event.getRoSGNode().spec
    if spec.asked <> m.requestsAsked then return
    summary = reqSummaryAnswer(event.getData())
    ' The server didn't answer: as it was, asked again on the next minute.
    if summary = invalid then return
    before = m.top.requestSummary
    setRequestSummary(summary)
    pollRequests()
    ' Changes no event told of (none came, or the stream was down): the
    ' screen on top asks again, unless it made them.
    if before.known and summary.changedAt <> before.changedAt and spec.why <> "events" and spec.why <> "asked" then pokeRequests({ requestId: 0, kind: "", tmdbId: 0 })
    ' News no event told of: the requests that have it.
    unseenBefore = 0
    if before.known then unseenBefore = before.unseen
    if summary.unseen > unseenBefore and m.newsEvents = 0 and reqShowsRequests(summary) then askUnseen()
    m.newsEvents = 0
end sub

' The summary, for the scene and each screen that shows what it allows.
sub setRequestSummary(summary as object)
    m.top.requestSummary = summary
    for each screen in m.stack
        if screen.hasField("requestSummary") then screen.requestSummary = summary
    end for
end sub

' Without the event stream, the summary is asked each minute; a server
' without requests (an older one), only as the stream connects again or
' the app starts.
sub pollRequests()
    if m.global.session.profileId > 0 and not m.events.live and not m.top.requestSummary.absent
        m.requestPoll.control = "start"
    else
        m.requestPoll.control = "stop"
    end if
end sub

sub onRequestPoll()
    askRequestSummary("poll")
end sub

' The stream is up (again): nothing it missed is told, so the summary is
' asked again, an older server's too (it may have been updated). Up or
' down, the page on top asks for its requests again: without the stream,
' that answer starts it asking each 30 s.
sub onStreamLive()
    m.top.eventsLive = m.events.live
    if m.events.live then askRequestSummary("live")
    pollRequests()
    pokeRequests({ requestId: 0, kind: "", tmdbId: 0 })
end sub

' A screen did something to a request: what the profile may do now (its
' allowance, the managers' count). The stream tells of the request's
' title; without it, its badge is asked for here, unless the screen names
' no request (a title's page, which gives its own).
sub onRequestTouched()
    touched = m.top.requestTouched
    askRequestSummary("asked")
    requestId = httpNumber(touched.requestId, 0)
    if m.events.live or requestId = 0 or not showsTiles() then return
    askBadge(httpString(touched.kind), httpNumber(touched.tmdbId, 0))
end sub

' A request event: the summary is asked a second on, once for those that
' come together, and the screen on top follows; a title ready to watch is
' told of.
sub onRequestEvent(data as object)
    kind = httpString(data.type)
    if kind = "request_available" or kind = "request_decided" then m.newsEvents = m.newsEvents + 1
    if kind = "request_available" and m.announced <> invalid
        before = reqMarkCopy(m.announced)
        told = reqAnnounceEvent(data, m.announced)
        if told.Count() > 0 and told[0].itemId > 0 then notify(reqNotice(told, m.global.session.server), before)
        keepAnnounced()
    end if
    if kind <> "requests_settings_changed" then gatherRequestChange(data)
    m.requestGather.control = "stop"
    m.requestGather.control = "start"
end sub

' What the events changed: one request ({ requestId, kind, tmdbId }), or
' many (requestId 0); and the titles whose badge they changed.
sub gatherRequestChange(data as object)
    change = { requestId: httpNumber(data.request_id, 0), kind: httpString(data.kind), tmdbId: httpNumber(data.tmdb_id, 0) }
    if change.requestId = 0 then change = { requestId: 0, kind: "", tmdbId: 0 }
    if m.requestChange = invalid
        m.requestChange = change
    else if m.requestChange.requestId <> change.requestId
        m.requestChange = { requestId: 0, kind: "", tmdbId: 0 }
    end if
    if change.tmdbId > 0 then m.badgeTitles[reqKey(change.kind, change.tmdbId)] = { kind: change.kind, tmdbId: change.tmdbId }
end sub

' Whether a screen in the stack shows TMDB's titles as tiles.
function showsTiles() as boolean
    for each screen in m.stack
        if screen.hasField("requestNote") then return true
    end for
    return false
end function

' A title's badge, as its page says it for this profile (GET
' /api/discover/{kind}/{tmdbId}): its status depends on who asks and on
' all of its requests, never on one. One question per title at a time.
sub askBadge(kind as string, tmdbId as dynamic)
    if (kind <> "movie" and kind <> "show") or httpNumber(tmdbId, 0) <= 0 then return
    key = reqKey(kind, tmdbId)
    if m.badgesAsked.DoesExist(key)
        m.badgesAsked[key] = true
        return
    end if
    m.badgesAsked[key] = false
    uiRequest({ method: "GET", path: "/api/discover/" + kind + "/" + urlValue(tmdbId), key: key, kind: kind, tmdbId: tmdbId, asked: m.requestsAsked }, "onBadge")
end sub

' A title's badge for the screens showing it; a title changed again
' meanwhile is asked once more, this answer left out.
sub onBadge(event as object)
    spec = event.getRoSGNode().spec
    if spec.asked <> m.requestsAsked then return
    again = httpTrue(m.badgesAsked[spec.key])
    m.badgesAsked.Delete(spec.key)
    if again
        askBadge(spec.kind, spec.tmdbId)
        return
    end if
    note = reqTitleNote(spec.key, event.getData())
    if note <> invalid then m.top.requestNote = note
end sub

sub onRequestsGathered()
    askRequestSummary("events")
    change = m.requestChange
    m.requestChange = invalid
    if change <> invalid then pokeRequests(change)
    titles = m.badgeTitles
    m.badgeTitles = {}
    if not showsTiles() then return
    for each key in titles
        askBadge(titles[key].kind, titles[key].tmdbId)
    end for
end sub

' The screen on top shows requests: it asks again for what changed.
sub pokeRequests(change as object)
    if m.stack.Count() = 0 then return
    top = m.stack.Peek()
    if top.hasField("requestsChanged") then top.requestsChanged = change
end sub

' A title's request changed, here or elsewhere: every screen showing its
' tile, covered or not, gives it its new badge.
sub onRequestNote()
    note = m.top.requestNote
    for each screen in m.stack
        if screen.hasField("requestNote") then screen.requestNote = note
    end for
end sub

' The profile's requests with news it hasn't seen: those newly ready to
' watch are told of.
sub askUnseen()
    uiRequest({ method: "GET", path: "/api/requests", query: { unseen: 1, limit: 20 }, asked: m.requestsAsked }, "onUnseen")
end sub

sub onUnseen(event as object)
    answer = event.getData()
    if event.getRoSGNode().spec.asked <> m.requestsAsked or m.announced = invalid or not answer.ok then return
    data = httpObject(answer.data)
    if data = invalid then return
    before = reqMarkCopy(m.announced)
    told = reqToAnnounce(data.requests, m.announced)
    if told.Count() > 0 then notify(reqNotice(told, m.global.session.server), before)
    keepAnnounced()
end sub

' What was told is kept, but for what notices not seen yet tell: should
' the app close first, it is told again.
sub keepAnnounced()
    session = m.global.session
    if m.announced = invalid or session.profileId <= 0 then return
    waiting = []
    if m.shownNotice <> invalid then waiting.Push(m.shownNotice)
    waiting.Append(m.notices)
    reqSaveAnnounced(requestsSection(), session.server, session.profileId, reqMarkToKeep(m.announced, waiting))
end sub

' A notice of what is ready to watch, shown in the card when it can be
' (showNotice). The last three wait. before: the notices' mark before its
' news (reqMarkCopy's).
sub notify(notice as object, before as object)
    notice.before = before
    m.notices.Push(notice)
    while m.notices.Count() > 3
        m.notices.Shift()
    end while
    showNotice()
end sub

' The first notice waiting shows once the remote has rested and the page
' is free for it (noticeFree), looked at four times a second meanwhile.
sub showNotice()
    if m.shownNotice <> invalid or m.notices.Count() = 0
        m.noticeWaiting = false
        m.noticeWait.control = "stop"
        return
    end if
    if m.noticeWaiting then return
    ' A key before the looks began says nothing of when it came.
    m.noticeWaiting = true
    m.keyAt = invalid
    m.keySeen = false
    m.noticeWait.control = "start"
    onNoticeWait()
end sub

sub onNoticeWait()
    if m.shownNotice <> invalid or m.notices.Count() = 0
        showNotice()
        return
    end if
    ' Roku says whole seconds since the last key: the look that first finds
    ' one second gone comes a second after the key at the most, a quarter
    ' of a second later than that at the least (the looks' pace), and how
    ' long ago the key was is that second and the time since.
    seconds = m.device.TimeSinceLastKeypress()
    if seconds < 1
        m.keyAt = invalid
        m.keySeen = true
    else if m.keyAt = invalid and m.keySeen
        m.keyAt = CreateObject("roTimespan")
    end if
    keyMs = -1
    if m.keyAt <> invalid then keyMs = 1000 + m.keyAt.TotalMilliseconds()
    if not reqRested(seconds, keyMs, m.pressAt.TotalMilliseconds()) or not noticeFree() then return
    m.noticeWaiting = false
    m.noticeWait.control = "stop"
    ' The card takes the focus, and gives it back here when it closes.
    m.noticeFocus = focusedNode()
    m.shownNotice = m.notices.Shift()
    m.noticeCard.notice = { text: m.shownNotice.text, poster: m.shownNotice.poster }
end sub

' Whether a notice may show now: over a screen it may (reqNoticeOver), no
' dialog up, and the focus on that screen, not in a panel or a keyboard.
function noticeFree() as boolean
    if m.stack.Count() = 0 or dialogShowing() then return false
    top = m.stack.Peek()
    if not reqNoticeOver(top.route) then return false
    inside = false
    node = focusedNode()
    while node <> invalid
        if node.isSameNode(top) then inside = true
        if node.id = "panel" then return false
        kind = node.subtype()
        if Instr(1, kind, "Keyboard") > 0 or Instr(1, kind, "PinPad") > 0 then return false
        node = node.getParent()
    end while
    return inside
end function

' The node with the focus, or invalid.
function focusedNode() as dynamic
    node = m.top.focusedChild
    for i = 1 to 40
        if node = invalid or node.hasFocus() then return node
        node = node.focusedChild
    end for
    return node
end function

' What the card did. Open opens the title (the requests, for news of
' several); Close, Back, Up, Down or its time up give the focus back where
' it was. The focus taken from it by a dialog leaves it under that,
' waiting; taken by the page, it was seen.
sub onNoticeEvent()
    what = m.noticeCard.event
    if what = "stray"
        giveFocusBack()
        return
    end if
    notice = m.shownNotice
    if notice = invalid then return
    if what = "lost"
        if dialogShowing()
            m.noticeCard.cover = true
            return
        end if
        m.noticeCard.hide = true
    end if
    noticeSeen()
    if what = "open"
        ' The page under it keeps where its focus was, for Back.
        giveFocusBack()
        if notice.itemId > 0
            show({ to: "details", params: { itemId: notice.itemId } })
        else
            show({ to: "requests", params: { view: "mine" } })
        end if
    else if what = "close"
        giveFocusBack()
    end if
    ' As a press: the next waits for the remote to rest from it too.
    m.pressAt.Mark()
    showNotice()
end sub

' The focus where it was before the card, while that is still on the
' screen on top; else on that screen.
sub giveFocusBack()
    if m.stack.Count() = 0 then return
    top = m.stack.Peek()
    node = m.noticeFocus
    shown = true
    while node <> invalid
        ' Hidden meanwhile (the rows gave way to a message), it can't
        ' take the focus.
        if node.hasField("visible") and not node.visible then shown = false
        if node.isSameNode(top)
            if shown
                m.noticeFocus.setFocus(true)
            else
                top.setFocus(true)
            end if
            return
        end if
        node = node.getParent()
    end while
    top.setFocus(true)
end sub

' The notice shown was seen: what it told is kept.
sub noticeSeen()
    m.shownNotice = invalid
    keepAnnounced()
end sub

' Another screen came over the card before its notice was seen: it waits
' first in line, to show again.
sub coverNotice()
    if m.shownNotice = invalid then return
    if m.noticeCard.isInFocusChain() then giveFocusBack()
    m.noticeCard.hide = true
    m.notices.Unshift(m.shownNotice)
    m.shownNotice = invalid
end sub

' --- Casts ---

' A cast from the user's remote, on another device (Cast.brs has the
' rules): for this Roku, while it takes casts, what it names plays as the
' Roku's own Play would, in place of what plays, once what it needs has
' come, and "Playing from …" says where it came from. One from another
' profile of the household switches to it first (castSwitchTo). Of two
' casts close together the later plays; an answer that comes once casts
' aren't taken (signed out, "Who's watching?") is left.
sub onCast(data as object)
    session = m.global.session
    action = castAction(session.playerId, castTaken(), data)
    if action.act = "none" then return
    need = castSwitch(session.profileId, data)
    if need.needs = "ignore" then return
    banner = castBanner(data.from)
    ' While the Roku changes profiles for a cast, any other cast is
    ' dropped; once switched, casts play as ever.
    pending = m.castSwitch
    if pending <> invalid and not pending.switched then return
    m.castSwitch = invalid
    m.castAsk = m.castAsk + 1
    m.castGather = invalid
    if need.needs = "switch"
        castSwitchTo(need, action, banner)
    else
        castPlay(action, banner)
    end if
end sub

' What a cast names (castAction's) is fetched, then plays.
sub castPlay(action as object, banner as string)
    if action.act = "channel"
        castRequest("/api/live/channels/" + urlValue(action.channelId), "onCastChannel", { banner: banner })
    else if action.act = "playlist"
        castRequest("/api/playlists/" + urlValue(action.playlistId), "onCastPlaylist", { build: "playlist", banner: banner, shuffle: action.shuffle })
    else
        castRequest("/api/items/" + urlValue(action.itemId), "onCastItem", { banner: banner, startMs: action.startMs, shuffle: action.shuffle, audioIndex: action.audioIndex, subtitleIndex: action.subtitleIndex })
    end if
end sub

' A cast from another profile of the household (castSwitch's need): the
' Roku switches to the remote's profile with the cast's ticket, then it
' plays. What plays stops first, and its last progress, stopped report and
' session end go before the switch (drain: the API task holds the switch
' for them, 5 s at most), with the tokens of the profile that watched it,
' never the remote's.
sub castSwitchTo(need as object, action as object, banner as string)
    m.castSwitches = m.castSwitches + 1
    m.castSwitch = { seq: m.castSwitches, profileId: need.profileId, action: action, banner: banner, switched: false }
    m.music.control = "stop"
    if m.stack.Count() > 1
        top = m.stack.Peek().subtype()
        if top = "PlayerScreen" or top = "LivePlayerScreen" then pop()
    end if
    uiRequest({ op: "switchProfile", profile: need.profileId, ticket: need.ticket, castSwitch: m.castSwitches, drain: true }, "onCastSwitched")
end sub

' The switch's answer: switched, the cast plays once the Roku knows it is
' on that profile (castSwitched); refused or failed, a notice says why and
' nothing plays.
sub onCastSwitched(event as object)
    pending = m.castSwitch
    if pending = invalid or event.getRoSGNode().spec.castSwitch <> pending.seq then return
    answer = event.getData()
    if answer.ok and not httpTrue(answer.data.through)
        pending.switched = true
        castSwitched()
        return
    end if
    m.castSwitch = invalid
    notice = castSwitchNotice(answer)
    if notice <> "" and castTaken() then castSay(notice)
end sub

' Switched for a cast, and the profile known (GET /api/auth/me) as the
' remote's: everything shows as that profile sees it, from Home, as after
' "Who's watching?", and the cast plays over it, once. Casts no longer
' taken meanwhile (signed out, "Who's watching?") leave it.
sub castSwitched()
    pending = m.castSwitch
    if pending = invalid or not pending.switched or m.global.session.profileId <> pending.profileId then return
    m.castSwitch = invalid
    if not castTaken() then return
    show({ to: "home", reset: true })
    m.castAsk = m.castAsk + 1
    m.castGather = invalid
    castPlay(pending.action, pending.banner)
end sub

' Whether casts are taken now: signed in, past the sign-in and "Who's
' watching?" (Cast.brs's castTakes).
function castTaken() as boolean
    routes = []
    for each screen in m.stack
        routes.Push(screen.route)
    end for
    return castTakes(m.global.session.signedIn, routes)
end function

' A request for the cast under way, its answer to callback; more: the
' build, and what the cast said that the answer needs.
sub castRequest(path as string, callback as string, more as object)
    spec = { method: "GET", path: path, cast: m.castAsk }
    spec.Append(more)
    uiRequest(spec, callback)
end sub

' The data of a cast's answer while that cast is still the one to play
' and casts are taken; invalid otherwise, or when it failed, which is
' said where the cast would have been.
function castAnswer(event as object) as dynamic
    if event.getRoSGNode().spec.cast <> m.castAsk or not castTaken() then return invalid
    answer = event.getData()
    if answer.ok and answer.data <> invalid then return answer.data
    ' Signed out, the scene shows sign-in; asked for the profile before a
    ' switch, it was left.
    if answer.error <> "signed_out" and answer.error <> "profile_changed" then castSay(answer.message)
    return invalid
end function

' A title: a movie or an episode plays (with the tracks the remote
' picked), a show or a season its next episode, an album or an artist its
' tracks, a track its album from it.
sub onCastItem(event as object)
    item = castAnswer(event)
    if item = invalid then return
    spec = event.getRoSGNode().spec
    id = urlValue(item.id)
    plan = castItemPlay(httpString(item.kind), spec.startMs, spec.shuffle)
    if plan.plays = "video"
        ' The player has the item already: one request less before it plays.
        params = { itemId: item.id, startMs: castVideoStartMs(plan.startMs, item), item: item }
        audio = Int(httpNumber(spec.audioIndex, -2))
        subtitle = Int(httpNumber(spec.subtitleIndex, -2))
        if audio <> -2 or subtitle <> -2
            params.audioIndex = castGivenTracks(item, audio, subtitle)
            params.tracksGiven = true
        end if
        castShow({ to: "player", params: params }, spec.banner)
    else if plan.plays = "nextEpisode"
        castRequest("/api/items/" + id + "/on-deck", "onCastOnDeck", { banner: spec.banner })
    else if plan.plays = "album"
        castRequest("/api/items/" + id + "/children", "onCastTracks", { build: "trackQueue", banner: spec.banner, shuffle: plan.shuffle })
    else if plan.plays = "artist"
        castRequest("/api/items/" + id + "/children", "onCastAlbums", { build: "albums", banner: spec.banner, shuffle: plan.shuffle })
    else if plan.plays = "track"
        startMs = plan.startMs
        if startMs < 0 then startMs = 0
        albumId = httpNumber(item.parent_id, 0)
        if albumId > 0
            castRequest("/api/items/" + urlValue(albumId) + "/children", "onCastTracks", { build: "trackQueue", banner: spec.banner, track: item, startMs: startMs })
        else
            castTracks(musicTracks([item], m.global.session.server), 0, false, startMs, spec.banner)
        end if
    else
        castSay(castNothing())
    end if
end sub

' A show's or a season's next episode, where its Play starts it.
sub onCastOnDeck(event as object)
    data = castAnswer(event)
    if data = invalid then return
    episode = httpObject(data.item)
    if episode = invalid
        castSay(castNothing())
        return
    end if
    castShow({ to: "player", params: { itemId: episode.id, startMs: castVideoStartMs(-1, episode) } }, event.getRoSGNode().spec.banner)
end sub

' An album's tracks, from the first (shuffled: from any); or a track's
' album from that track, as the album's page plays a track, the track
' alone when it isn't there.
sub onCastTracks(event as object)
    data = castAnswer(event)
    if data = invalid then return
    spec = event.getRoSGNode().spec
    tracks = httpArray(data.queue)
    if spec.track = invalid
        castTracks(tracks, 0, httpTrue(spec.shuffle), 0, spec.banner)
        return
    end if
    at = castTrackAt(tracks, spec.track.id)
    if at < 0
        tracks = musicTracks([spec.track], m.global.session.server)
        at = 0
    end if
    castTracks(tracks, at, false, spec.startMs, spec.banner)
end sub

' An artist's albums came, in the server's order: their tracks are asked
' for an album at a time, then play together, as the artist's Play plays
' them.
sub onCastAlbums(event as object)
    data = castAnswer(event)
    if data = invalid then return
    spec = event.getRoSGNode().spec
    m.castGather = { ids: httpArray(data.order), at: 0, tracks: [], shuffle: spec.shuffle, banner: spec.banner }
    castGatherNext()
end sub

sub castGatherNext()
    gather = m.castGather
    if gather.at < gather.ids.Count()
        castRequest("/api/items/" + urlValue(gather.ids[gather.at]) + "/children", "onCastGathered", { build: "trackQueue" })
        return
    end if
    m.castGather = invalid
    castTracks(gather.tracks, 0, gather.shuffle, 0, gather.banner)
end sub

sub onCastGathered(event as object)
    data = castAnswer(event)
    if data = invalid or m.castGather = invalid then return
    m.castGather.tracks.Append(httpArray(data.queue))
    m.castGather.at = m.castGather.at + 1
    castGatherNext()
end sub

' A playlist, as its page's Play (or Shuffle) plays it: music as the
' queue with Now Playing, videos in the player one after the other.
sub onCastPlaylist(event as object)
    data = castAnswer(event)
    if data = invalid then return
    spec = event.getRoSGNode().spec
    shuffled = httpTrue(spec.shuffle)
    if httpString(data.kind) = "audio"
        castTracks(httpArray(data.queue), 0, shuffled, 0, spec.banner)
        return
    end if
    queue = pageQueueFrom(httpArray(data.queue), 0, shuffled)
    if queue.Count() = 0
        castSay(castNothing())
        return
    end if
    castShow({ to: "player", params: { itemId: queue[0].itemId, startMs: queue[0].startMs, queue: queue } }, spec.banner)
end sub

' A channel, tuned in the live player; up and down go through All
' channels.
sub onCastChannel(event as object)
    data = castAnswer(event)
    if data = invalid then return
    channel = liveChannel(data, m.global.session.server, liveLogoWidth(), "all")
    if channel = invalid
        castSay(castNothing())
        return
    end if
    castShow({ to: "livePlayer", params: { channel: channel, category: "all", categoryName: liveCategoryName("all", []) } }, event.getRoSGNode().spec.banner)
end sub

' Tracks cast here play as the music's queue from at (shuffled: from any
' track, the others after it in a random order, as Shuffle on their page
' plays them), fromMs into the first, with Now Playing.
sub castTracks(tracks as object, at as integer, shuffled as boolean, fromMs as integer, banner as string)
    if tracks.Count() = 0
        castSay(castNothing())
        return
    end if
    if shuffled then at = Rnd(tracks.Count()) - 1
    m.music.play = { queue: tracks, at: at, shuffled: shuffled, fromMs: fromMs }
    if m.stack.Count() > 0 and m.stack.Peek().subtype() = "NowPlayingScreen"
        castSay(banner)
        return
    end if
    castShow({ to: "nowPlaying" }, banner)
end sub

' What a cast plays shows in place of the player on top (a video or a
' channel, which then ends), else over the screen on top, a dialog up
' closed; and where it came from over it.
sub castShow(nav as object, banner as string)
    if dialogShowing() and m.top.dialog.hasField("close") then m.top.dialog.close = true
    top = ""
    if m.stack.Count() > 0 then top = m.stack.Peek().subtype()
    nav.replace = top = "PlayerScreen" or top = "LivePlayerScreen"
    show(nav)
    castSay(banner)
end sub

' A cast's notice for 4 s at the top, as the players' notices show; over
' the video player, under the title it shows at the top as an item
' begins.
sub castSay(text as string)
    top = 72
    if m.stack.Count() > 0 and m.stack.Peek().subtype() = "PlayerScreen" then top = 232
    m.castToast.translation = [460, top]
    m.castToastText.text = text
    m.castToast.visible = true
    m.castToastTimer.control = "stop"
    m.castToastTimer.control = "start"
end sub

sub onCastToastTimer()
    m.castToast.visible = false
end sub

' Music stopped from another device or by the server's administrator
' while another screen is on top (it plays behind them): the scene says
' who stopped it and their message. Now Playing, on top, says so itself.
sub onMusicStoppedBy()
    stoppedBy = m.music.stoppedBy
    if stoppedBy = "" then return
    if m.stack.Count() > 0 and m.stack.Peek().subtype() = "NowPlayingScreen" then return
    lines = stoppedBy.Split(Chr(10))
    message = []
    if lines.Count() > 1 then message.Push(lines[1])
    uiMessage(lines[0], message)
end sub

' A dialog shows, from any screen, or the scene lets one go: a notice
' waiting may show once none does, and the card under one waits for it.
' Each dialog's closing is watched, the Back key's too, which sets no
' close field.
sub onDialog()
    dialog = m.top.dialog
    if dialog <> invalid
        if m.closedDialog <> invalid and dialog.isSameNode(m.closedDialog) then m.closedDialog = invalid
        if dialog.hasField("wasClosed") then dialog.observeField("wasClosed", "onDialogClosed")
        if m.shownNotice <> invalid and dialogShowing() then m.noticeCard.cover = true
    end if
    showNotice()
end sub

' A dialog closed: the card under it, if any, has the focus again.
sub onDialogClosed(event as object)
    m.closedDialog = event.getRoSGNode()
    if m.shownNotice <> invalid and not dialogShowing() then m.noticeCard.cover = false
    showNotice()
end sub

' Whether a dialog is up: one the scene holds that hasn't been closed (a
' closed dialog may stay in the scene's field until another replaces it).
function dialogShowing() as boolean
    dialog = m.top.dialog
    if dialog = invalid then return false
    if dialog.hasField("close") and httpTrue(dialog.close) then return false
    if dialog.hasField("wasClosed") and httpTrue(dialog.wasClosed) then return false
    if m.closedDialog <> invalid and dialog.isSameNode(m.closedDialog) then return false
    return true
end function

' Pictures load through the scene's HTTP agent (nodes without their own
' use their nearest parent's): a new token makes a new agent.
sub onAgentChanged()
    m.top.setHttpAgent(uiHttpAgent())
end sub

' Signed out by the server (or by the person, in Settings): sign in again.
sub onSignedIn()
    session = m.global.session
    if session.signedIn or m.stack.Count() = 0 or session.server = "" then return
    top = m.stack.Peek().subtype()
    if top = "LinkScreen" or top = "ServersScreen" then return
    show({ to: "link", reset: true, params: { reason: session.signOutReason } })
end sub

sub onNavigate(event as object)
    show(event.getData())
end sub

' nav: { to, params, reset, replace } or { back: true }, with a message
' ({ title, lines }) to show over the screen it leads to.
sub show(nav as object)
    if httpTrue(nav.back)
        pop()
        if nav.message <> invalid then uiMessage(nav.message.title, nav.message.lines)
        return
    end if
    route = httpString(nav.to)
    name = navScreen(route)
    if name = "" then return
    ' Requests failing together say so once.
    if name = "UnreachableScreen" and m.stack.Count() > 0
        if m.stack.Peek().subtype() = name then return
    end if
    ' A notice shown waits for the next rest; a new screen counts as a
    ' press.
    coverNotice()
    m.pressAt.Mark()
    if httpTrue(nav.reset)
        ' Another profile, sign-in or server: what it played ends.
        m.music.control = "stop"
        while m.stack.Count() > 0
            closeScreen(m.stack.Pop())
        end while
    else if httpTrue(nav.replace) and m.stack.Count() > 0
        closeScreen(m.stack.Pop())
    end if
    if m.stack.Count() > 0
        covered = m.stack.Peek()
        covered.active = false
        covered.visible = false
    end if
    screen = CreateObject("roSGNode", name)
    params = nav.params
    if params = invalid then params = {}
    screen.route = route
    screen.params = params
    if screen.hasField("requestSummary") then screen.requestSummary = m.top.requestSummary
    screen.observeField("navigate", "onNavigate")
    m.stack.Push(screen)
    m.screens.appendChild(screen)
    screen.active = true
    launchDialogDone()
    showNotice()
end sub

sub pop()
    if m.stack.Count() <= 1 then return
    coverNotice()
    m.pressAt.Mark()
    closeScreen(m.stack.Pop())
    top = m.stack.Peek()
    top.visible = true
    top.active = true
    launchDialogDone()
    ' Played video, or signing in, ended: a notice waiting shows, once the
    ' remote rests from coming back.
    showNotice()
end sub

sub closeScreen(screen as object)
    screen.unobserveField("navigate")
    screen.active = false
    m.screens.removeChild(screen)
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if press and key = "back" and m.stack.Count() > 1
        pop()
        return true
    end if
    ' Back on the first screen leaves the app.
    return false
end function
