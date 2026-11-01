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

    ' What the person's other devices change, as it happens (an http://
    ' server's event stream): gathered for 0.3 s, then the screens follow.
    m.events = CreateObject("roSGNode", "EventsTask")
    m.events.observeField("event", "onServerEvent")
    m.events.observeField("tokenRefused", "onStreamRefused")
    ' The token the stream was last refused with, renewed since.
    m.renewing = ""
    m.gather = m.top.findNode("gather")
    m.gather.observeField("fire", "onGathered")
    session.observeField("accessToken", "onStream")
    session.observeField("server", "onStream")

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
    ' Ready-to-watch notices waiting to show, the one shown and its dialog,
    ' and what was told already (reqLoadAnnounced's) for the profile.
    m.notices = []
    m.shownNotice = invalid
    m.noticeDialog = invalid
    m.announced = invalid
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
    show({ to: screen, reset: true })
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

' The event stream follows the server and the token signed in with.
sub onStream()
    session = m.global.session
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

' Watch state, My List, libraries or Live TV changed elsewhere: Home asks again
' (now if it shows, else as it shows); a stream of this Roku's stopped from
' elsewhere ends at once.
sub onServerEvent(event as object)
    data = event.getData()
    if httpString(data.type) = "session_stopped"
        if m.stack.Count() > 0 and m.stack.Peek().hasField("serverStopped") then m.stack.Peek().serverStopped = data
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
    ' The notices were the profile before's: one open closes.
    m.notices = []
    dialog = m.noticeDialog
    m.shownNotice = invalid
    m.noticeDialog = invalid
    if dialog <> invalid then dialog.close = true
    m.announced = invalid
    if session.profileId > 0
        m.announced = reqLoadAnnounced(requestsSection(), session.server, session.profileId)
        askRequestSummary("start")
    end if
    pollRequests()
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
' asked again, an older server's too (it may have been updated).
sub onStreamLive()
    m.top.eventsLive = m.events.live
    if m.events.live then askRequestSummary("live")
    pollRequests()
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
        if told.Count() > 0 and told[0].itemId > 0 then notify(reqNotice(told), before)
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
    if told.Count() > 0 then notify(reqNotice(told), before)
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

' A notice of what is ready to watch, shown when it can be: never over
' playing video, signing in or another dialog. The last three wait.
' before: the notices' mark before its news (reqMarkCopy's).
sub notify(notice as object, before as object)
    notice.before = before
    m.notices.Push(notice)
    while m.notices.Count() > 3
        m.notices.Shift()
    end while
    showNotice()
end sub

sub showNotice()
    if m.notices.Count() = 0 or m.stack.Count() = 0 then return
    ' Shown once that one closes (onDialog).
    if dialogShowing() then return
    route = m.stack.Peek().route
    if route = "player" or route = "livePlayer" or route = "link" or route = "servers" or route = "profiles" or route = "unreachable" then return
    m.shownNotice = m.notices.Shift()
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = "Ready to watch"
    dialog.message = [m.shownNotice.text]
    dialog.buttons = ["Open", "OK"]
    dialog.observeField("buttonSelected", "onNoticeButton")
    m.noticeDialog = dialog
    m.top.dialog = dialog
end sub

' Open: the title, or the requests when several have news.
sub onNoticeButton(event as object)
    dialog = event.getRoSGNode()
    notice = invalid
    if m.noticeDialog <> invalid and dialog.isSameNode(m.noticeDialog)
        notice = m.shownNotice
        noticeSeen()
    end if
    dialog.close = true
    if event.getData() = 0 and notice <> invalid
        if notice.itemId > 0
            show({ to: "details", params: { itemId: notice.itemId } })
        else
            show({ to: "requests", params: { view: "mine" } })
        end if
    end if
    ' The next one waiting, if any.
    showNotice()
end sub

' The notice shown was seen (Open, OK or Back): what it told is kept.
sub noticeSeen()
    m.shownNotice = invalid
    m.noticeDialog = invalid
    keepAnnounced()
end sub

' Another dialog took the notice's place before it was seen: it waits
' first in line, to show again once that one closes.
sub noticeCovered()
    if m.shownNotice <> invalid then m.notices.Unshift(m.shownNotice)
    m.shownNotice = invalid
    m.noticeDialog = invalid
end sub

' A dialog shows, from any screen, or the scene lets one go: a notice
' waiting may show once none does, and one shown that another replaced
' waits again. Each dialog's closing is watched, the Back key's too, which
' sets no close field.
sub onDialog()
    dialog = m.top.dialog
    if dialog <> invalid
        if m.closedDialog <> invalid and dialog.isSameNode(m.closedDialog) then m.closedDialog = invalid
        if dialog.hasField("wasClosed") then dialog.observeField("wasClosed", "onDialogClosed")
        if m.noticeDialog <> invalid and not dialog.isSameNode(m.noticeDialog) then noticeCovered()
    end if
    showNotice()
end sub

' The notice's own dialog, closing while another holds the scene's field,
' was replaced by it; else Back closed it.
sub onDialogClosed(event as object)
    closed = event.getRoSGNode()
    m.closedDialog = closed
    if m.noticeDialog <> invalid and closed.isSameNode(m.noticeDialog)
        shown = m.top.dialog
        if shown <> invalid and not shown.isSameNode(closed)
            noticeCovered()
        else
            noticeSeen()
        end if
    end if
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
    closeScreen(m.stack.Pop())
    top = m.stack.Peek()
    top.visible = true
    top.active = true
    launchDialogDone()
    ' Played video, or signing in, ended: a notice waiting shows.
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
