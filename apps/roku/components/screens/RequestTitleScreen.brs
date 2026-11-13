' A title on TMDB (GET /api/discover/{movie|show}/{tmdbId}): its one main
' action, its request's stage and actions (lib/Requests.brs's
' reqTitlePage), and rows of its seasons and cast. A movie is asked for
' after a question; a show's seasons are picked (requestSeasons). It asks
' again each time it shows, as requests change (the scene's
' requestsChanged), each 10 s while something on it downloads, and each
' 30 s while it shows a request and the event stream is down. params:
' { kind, tmdbId, title, ask (Discover's Request: the main action at
' once), pick (a library's Request more seasons: the picker at once),
' requestId (the request a list opened it for: its stage and actions show,
' where the title has it) }.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.shade = m.top.findNode("shade")
    m.page = m.top.findNode("page")
    m.poster = m.top.findNode("poster")
    m.credit = m.top.findNode("credit")
    m.column = m.top.findNode("column")
    m.text = m.top.findNode("text")
    m.heading = m.top.findNode("heading")
    m.line = m.top.findNode("line")
    m.genres = m.top.findNode("genres")
    m.tagline = m.top.findNode("tagline")
    m.overview = m.top.findNode("overview")
    m.stage = m.top.findNode("stage")
    m.stageLine = m.top.findNode("stageLine")
    m.note = m.top.findNode("note")
    m.buttons = m.top.findNode("buttons")
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.problem = m.top.findNode("problem")
    m.problemTitle = m.top.findNode("problemTitle")
    m.problemText = m.top.findNode("problemText")
    m.problemButtons = m.top.findNode("problemButtons")
    m.poll = m.top.findNode("poll")
    m.credit.text = t("requests.credit")
    m.message.text = t("common.states.loading")
    ' The title as the server sent it (DiscoverDetail), and as the page
    ' shows it (reqTitlePage's).
    m.detail = invalid
    m.shown = invalid
    ' Loads asked for: answers to earlier ones are left out.
    m.loads = 0
    ' params' ask or pick was done, or given up.
    m.acted = false
    ' The title's status and library title as last loaded: when they
    ' change (asked for here or in the picker), its tiles elsewhere get
    ' the new badge.
    m.badge = ""
    m.problemShown = invalid
    m.buttons.observeField("buttonSelected", "onButton")
    m.rows.observeField("rowItemSelected", "onTile")
    m.problemButtons.observeField("buttonSelected", "onProblemButton")
    m.poll.observeField("fire", "load")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestsChanged", "load")
end sub

' Loaded each time it shows: back from the picker, the library or the
' player, the title may have changed. The focus comes here at once.
sub onActive()
    if not m.top.active
        m.poll.control = "stop"
        return
    end if
    if m.problem.visible and m.problemButtons.visible
        m.problemButtons.setFocus(true)
    else if m.rows.visible and m.page.translation[1] < 0
        m.rows.setFocus(true)
    else if m.buttons.visible
        m.buttons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
    load()
end sub

sub load()
    m.poll.control = "stop"
    m.loads = m.loads + 1
    params = m.top.params
    path = "/api/discover/" + httpString(params.kind) + "/" + urlValue(httpNumber(params.tmdbId, 0))
    uiRequest({ method: "GET", path: path, build: "discoverDetail", manage: reqSummaryOf(m.top.requestSummary).manage, load: m.loads }, "onDetail")
end sub

' A reload that fails leaves the page as it was.
sub onDetail(event as object)
    request = event.getRoSGNode()
    if request.spec.load <> m.loads then return
    answer = event.getData()
    if not answer.ok or httpObject(answer.data) = invalid
        if m.detail <> invalid
            schedule()
            return
        end if
        if uiFailed(answer) then return
        problem = reqProblem(answer, reqSummaryOf(m.top.requestSummary).manage)
        if problem = invalid then problem = { title: t("requests.title.loadFailed"), text: answer.message, retry: answer.status <> 404, signIn: false }
        showProblem(problem)
        return
    end if
    m.detail = answer.data
    wanted = httpNumber(m.top.params.requestId, 0)
    if wanted > 0 and reqRequestsById(m.detail.requests).DoesExist(urlValue(wanted)) then m.detail.request_id = wanted
    m.problem.visible = false
    m.problemShown = invalid
    showRows(request.content)
    showDetail()
    status = httpString(m.detail.status)
    badge = status + ":" + urlValue(m.shown.itemId)
    if m.badge <> "" and badge <> m.badge then uiNoteRequest({ key: reqKey(m.shown.kind, m.shown.tmdbId), status: status, libraryId: m.shown.itemId })
    m.badge = badge
    schedule()
    if not m.acted
        m.acted = true
        ' A page left (Back) or covered before its title came asks nothing.
        if m.top.active and (httpTrue(m.top.params.ask) or httpTrue(m.top.params.pick)) then actFirst()
    end if
end sub

' Discover's Request, and a library's Request more seasons, go on at once.
sub actFirst()
    for each action in m.shown.actions
        if action = "request" or action = "pick"
            act(action)
            return
        end if
    end for
end sub

' Asked again each 10 s while something on it downloads or goes into a
' library, and each 30 s while it shows a request and the event stream is
' down, when nothing tells it what changed.
sub schedule()
    m.poll.control = "stop"
    if not m.top.active or m.shown = invalid then return
    if m.shown.busy
        m.poll.duration = 10
    else if m.shown.requestId > 0 and not m.top.getScene().eventsLive
        m.poll.duration = 30
    else
        return
    end if
    m.poll.control = "start"
end sub

sub showDetail()
    if m.detail = invalid then return
    m.shown = reqTitlePage(m.detail, reqSummaryOf(m.top.requestSummary), m.global.session.server, CreateObject("roDateTime").AsSeconds(), fmtZoneSeconds())
    m.message.visible = false
    m.heading.text = m.shown.title
    m.line.text = m.shown.line
    m.genres.text = m.shown.genres
    m.tagline.text = m.shown.tagline
    m.overview.text = m.shown.overview
    m.note.text = m.shown.note
    stage = m.shown.stage
    if stage <> invalid
        m.stage.color = reqToneColor(stage.tone)
        m.stage.progress = stage.progress
        m.stage.text = stage.text
        m.stageLine.text = stage.line
    else
        m.stage.text = ""
        m.stageLine.text = ""
    end if
    layoutText()
    agent = m.top.getScene().getHttpAgent()
    if m.backdrop.uri <> m.shown.backdrop
        m.backdrop.setHttpAgent(agent)
        m.backdrop.uri = m.shown.backdrop
    end if
    if m.poster.uri <> m.shown.poster
        m.poster.setHttpAgent(agent)
        m.poster.uri = m.shown.poster
    end if
    hasButtons = m.shown.buttons.Count() > 0
    if hasButtons
        focused = m.buttons.buttonFocused
        hadFocus = m.buttons.isInFocusChain()
        if FormatJson(m.buttons.buttons) <> FormatJson(m.shown.buttons) then m.buttons.buttons = m.shown.buttons
        uiKeepButton(m.buttons, focused, hadFocus)
    else if m.buttons.isInFocusChain()
        m.top.setFocus(true)
    end if
    m.buttons.visible = hasButtons
    m.column.visible = true
    m.credit.visible = true
    ' Only while this screen has the focus: a dialog over it keeps it.
    if m.top.isInFocusChain() and not m.rows.isInFocusChain()
        if hasButtons
            m.buttons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

' The parts with text, one under the other.
sub layoutText()
    shown = [m.heading]
    spacings = []
    parts = [
        { node: m.line, gap: 20, filled: m.line.text <> "" },
        { node: m.genres, gap: 8, filled: m.genres.text <> "" },
        { node: m.tagline, gap: 28, filled: m.tagline.text <> "" },
        { node: m.overview, gap: 20, filled: m.overview.text <> "" },
        { node: m.stage, gap: 28, filled: m.stage.text <> "" },
        { node: m.stageLine, gap: 12, filled: m.stageLine.text <> "" },
        { node: m.note, gap: 24, filled: m.note.text <> "" }
    ]
    for each part in parts
        if part.filled
            spacings.Push(part.gap)
            shown.Push(part.node)
        end if
    end for
    m.text.removeChildrenIndex(m.text.getChildCount(), 0)
    m.text.appendChildren(shown)
    m.text.itemSpacings = spacings
end sub

' --- The rows ---

' The title's seasons and cast. Where they have the tiles they had (a
' reload), only the tiles that changed are replaced, so the focus and the
' rows stay where they are.
sub showRows(content as dynamic)
    if content = invalid or content.getChildCount() = 0
        if m.rows.isInFocusChain() then focusRows(false)
        m.rows.visible = false
        return
    end if
    shownRows = m.rows.content
    if shownRows <> invalid and sameRows(shownRows, content)
        for i = 0 to content.getChildCount() - 1
            row = shownRows.getChild(i)
            fresh = content.getChild(i).getChildren(-1, 0)
            for j = 0 to fresh.Count() - 1
                if tileChanged(row.getChild(j), fresh[j]) then row.replaceChild(fresh[j], j)
            end for
        end for
        return
    end if
    sizes = []
    heights = []
    for i = 0 to content.getChildCount() - 1
        sizes.Push([220, 410])
        heights.Push(470)
    end for
    m.rows.rowItemSize = sizes
    m.rows.rowHeights = heights
    m.rows.content = content
    m.rows.visible = true
end sub

' Whether rows have as many tiles, row by row, with the same keys.
function sameRows(shownRows as object, content as object) as boolean
    if shownRows.getChildCount() <> content.getChildCount() then return false
    for i = 0 to content.getChildCount() - 1
        row = shownRows.getChild(i)
        fresh = content.getChild(i)
        if row.getChildCount() <> fresh.getChildCount() then return false
        for j = 0 to fresh.getChildCount() - 1
            if row.getChild(j).key <> fresh.getChild(j).key then return false
        end for
    end for
    return true
end function

function tileChanged(tile as object, fresh as object) as boolean
    for each name in ["title", "subtitle", "image", "badge", "badgeColor", "progress"]
        if tile.getField(name) <> fresh.getField(name) then return true
    end for
    return false
end function

' The page slides up to the rows as they take the focus, the backdrop
' shaded under them all, and back down.
sub focusRows(onRows as boolean)
    if onRows
        m.page.translation = [0, -820]
        m.shade.width = 1920
        m.rows.setFocus(true)
    else
        m.page.translation = [0, 0]
        m.shade.width = 1260
        if m.buttons.visible
            m.buttons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

' A season that can be asked for opens the picker, with it alone ticked;
' the cast opens nothing.
sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row = invalid or m.shown = invalid then return
    tile = row.getChild(position[1])
    if tile = invalid or tile.kind <> "discoverSeason" or not hasAction("pick") then return
    for each season in httpArray(m.detail.seasons)
        if GetInterface(season, "ifAssociativeArray") <> invalid and httpNumber(season.number, -1) = tile.number and reqSeasonAskable(season, false) then openPicker(tile.number)
    end for
end sub

function hasAction(name as string) as boolean
    for each action in m.shown.actions
        if action = name then return true
    end for
    return false
end function

' --- The buttons ---

sub onButton(event as object)
    if m.shown = invalid then return
    act(m.shown.actions[event.getData()])
end sub

' What a button does (reqTitlePage's actions).
sub act(action as string)
    if action = "signIn"
        uiNavigate({ to: "link", params: { reason: "requests" } })
    else if action = "open"
        uiNavigate({ to: "details", params: { itemId: m.shown.itemId } })
    else if action = "request"
        confirmAsk()
    else if action = "pick"
        openPicker(-1)
    else if action = "follow"
        ask(reqFollowSeasons(m.detail.seasons))
    else
        uiRequestAct(requestTarget(), action)
    end if
end sub

' The season picker, the askable seasons ticked, or only one of them.
sub openPicker(only as integer)
    uiNavigate({ to: "requestSeasons", params: { kind: m.shown.kind, tmdbId: m.shown.tmdbId, title: m.shown.title, year: httpNumber(m.detail.year, 0), detail: m.detail, only: only } })
end sub

' A movie is asked for once the question is answered.
sub confirmAsk()
    confirm = reqAskConfirm(m.shown.title, m.shown.access, m.shown.allowance, CreateObject("roDateTime").AsSeconds())
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = confirm.title
    dialog.message = confirm.lines
    dialog.buttons = [t("requests.buttons.request"), t("common.actions.cancel")]
    dialog.observeField("buttonSelected", "onAskConfirmed")
    m.top.getScene().dialog = dialog
end sub

sub onAskConfirmed(event as object)
    uiCloseDialog()
    if event.getData() = 0 then ask(invalid)
end sub

' Asks for the title (POST /api/requests): a movie, or Tell me too (a
' show's seasons someone asked for already), which costs nothing.
sub ask(seasons as dynamic)
    uiRequest({ method: "POST", path: "/api/requests", jsonText: reqAskBody(m.shown.kind, m.shown.tmdbId, seasons) }, "onAsked")
end sub

' What came of asking, in a message over the page as it is now.
sub onAsked(event as object)
    answer = event.getData()
    title = m.shown.title
    if not answer.ok
        if uiFailed(answer) then return
        uiMessage(t("common.states.failed"), [reqAskProblem(answer, m.shown.allowance, m.shown.kind, CreateObject("roDateTime").AsSeconds())])
        ' Someone asked meanwhile, or it arrived, or the limits moved.
        if answer.status = 409 or answer.status = 429 then changedHere()
        return
    end if
    uiMessage(title, [reqAsked(answer.status, answer.data, title)])
    changedHere()
end sub

' The request the page shows, as RequestActions.brs takes it.
function requestTarget() as object
    target = { requestId: m.shown.requestId, title: m.shown.title, status: m.shown.requestStatus, stage: "", kind: m.shown.kind, tmdbId: m.shown.tmdbId, seasons: [], libraryId: m.shown.itemId, actions: [] }
    request = reqShownRequest(m.detail)
    if request <> invalid
        target.stage = httpString(request.stage)
        target.seasons = reqSeasonNumbers(request.seasons)
        target.actions = reqActionNames(request.actions)
    end if
    return target
end function

' An action on the title's request was done, or refused as the request
' had changed meanwhile: the page as it is now.
sub onRequestActed()
    changedHere()
end sub

' The title changed from here: the summary is asked (what may still be
' asked for, the managers' count), and the page loads again, its badge
' following elsewhere.
sub changedHere()
    m.top.getScene().requestTouched = { requestId: 0, kind: m.shown.kind, tmdbId: m.shown.tmdbId }
    load()
end sub

' The summary changed: the main action follows.
sub onSummary()
    if m.detail <> invalid and m.problemShown = invalid then showDetail()
end sub

' --- What stops the page ---

' In place of the title, why it can't be shown: with Sign in, Try
' again, or nothing to do here.
sub showProblem(problem as object)
    m.problemShown = problem
    m.message.visible = false
    m.column.visible = false
    m.credit.visible = false
    m.rows.visible = false
    m.problemTitle.text = problem.title
    m.problemText.text = problem.text
    buttons = []
    if problem.signIn then buttons.Push(t("requests.buttons.signIn"))
    if problem.retry then buttons.Push(t("common.actions.tryAgain"))
    m.problemButtons.buttons = buttons
    m.problemButtons.visible = buttons.Count() > 0
    m.problem.visible = true
    if not m.top.isInFocusChain() then return
    if buttons.Count() > 0
        m.problemButtons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

sub onProblemButton()
    if m.problemShown = invalid then return
    if m.problemShown.signIn
        uiNavigate({ to: "link", params: { reason: "requests" } })
        return
    end if
    m.problem.visible = false
    m.problemShown = invalid
    m.message.visible = true
    m.top.setFocus(true)
    load()
end sub

' Down from the buttons goes to the rows, up from the first row back.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press or m.problem.visible then return false
    if key = "down" and not m.rows.isInFocusChain() and m.rows.visible
        focusRows(true)
        return true
    end if
    if (key = "up" or key = "back") and m.rows.isInFocusChain()
        focusRows(false)
        return true
    end if
    return false
end function
