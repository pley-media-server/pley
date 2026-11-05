' The requests (GET /api/requests), 50 at a time, the next as the focus
' nears the end: a tab for each status with its number (lib/Requests.brs's
' reqTabs, reqTabCount), a manager's filters (movies or shows, who asked,
' words of the title), and what can be done with each request
' (shared/RequestActions.brs). It asks again as it shows, as requests
' change (the scene's requestsChanged), each 10 s while one of them
' downloads, each 30 s while the event stream is down, and after an
' action, the focus staying on the request it was on. Shown to the
' account, its news is seen (POST /api/requests/seen). params: { view (a
' tab's key; "mine" opens a manager's Yours, everyone else's All) }.

sub init()
    m.heading = m.top.findNode("heading")
    m.count = m.top.findNode("count")
    m.tabs = m.top.findNode("tabs")
    m.side = m.top.findNode("side")
    m.list = m.top.findNode("list")
    m.message = m.top.findNode("message")
    m.hint = m.top.findNode("hint")
    m.problem = m.top.findNode("problem")
    m.problemTitle = m.top.findNode("problemTitle")
    m.problemText = m.top.findNode("problemText")
    m.problemButtons = m.top.findNode("problemButtons")
    m.panel = m.top.findNode("panel")
    m.panelTitle = m.top.findNode("panelTitle")
    m.choices = m.top.findNode("choices")
    m.menu = m.top.findNode("menu")
    m.busy = m.top.findNode("busy")
    m.list.content = CreateObject("roSGNode", "ContentNode")
    m.started = false
    ' A manager's page or Your requests: its tabs' keys, the tab shown, and
    ' a manager's filters (kind "" for both, person 0 for everyone, words
    ' "" for any title), with the people they can name (the last answer's).
    m.manage = false
    m.tabKeys = []
    m.view = ""
    m.kind = ""
    m.person = 0
    m.personName = ""
    m.words = ""
    m.people = []
    ' Each status's count with the filters (GET /api/requests' counts),
    ' invalid until known; the tabs' chips as last set (their JSON).
    m.counts = invalid
    m.tabChips = ""
    ' Listings (a tab and filters): answers to an earlier one are left out.
    ' Requests listed so far (the next page's offset) and in all; a page or
    ' a reload on its way, and a reload wanted meanwhile.
    m.listing = 0
    m.offset = 0
    m.total = -1
    m.loading = false
    m.reloading = false
    m.reloadWanted = false
    ' One of the requests listed downloads or goes into a library.
    m.listBusy = false
    m.seeing = false
    m.problemShown = invalid
    m.sideAction = ""
    m.lastFocus = ""
    ' The panel: the filters, or one filter's choices (their values and
    ' words).
    m.panelKind = ""
    m.values = []
    m.choiceLabels = []
    m.list.observeField("itemSelected", "onRow")
    m.list.observeField("itemFocused", "onRowFocused")
    m.tabs.observeField("chosen", "onTab")
    m.side.observeField("chosen", "onSide")
    m.problemButtons.observeField("buttonSelected", "onProblemButton")
    m.menu.observeField("itemSelected", "onMenu")
    m.choices.observeField("itemSelected", "onChoice")
    m.busy.observeField("fire", "reload")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestsChanged", "reload")
end sub

function pageSize() as integer
    return 50
end function

sub onActive()
    if not m.top.active
        m.busy.control = "stop"
        m.lastFocus = "tabs"
        if m.list.isInFocusChain() then m.lastFocus = "list"
        if m.side.isInFocusChain() then m.lastFocus = "side"
        return
    end if
    if not m.started
        m.started = true
        m.top.setFocus(true)
        showPage(httpString(m.top.params.view))
        return
    end if
    restoreFocus()
    ' Back from a title's page: what was done there shows.
    reload()
end sub

' The page for the summary: a manager's tabs and filters, or Your
' requests', on the tab asked for.
sub showPage(view as string)
    summary = reqSummaryOf(m.top.requestSummary)
    m.manage = summary.manage = true
    m.heading.text = reqPageName(summary)
    m.tabKeys = []
    for each entry in reqTabs(m.manage)
        m.tabKeys.Push(entry.key)
    end for
    m.counts = invalid
    m.total = -1
    m.view = reqListView(view, m.manage)
    for i = 0 to m.tabKeys.Count() - 1
        if m.tabKeys[i] = m.view
            m.tabs.current = i
            m.tabs.focusIndex = i
        end if
    end for
    if not m.manage
        m.kind = ""
        m.person = 0
        m.personName = ""
        m.words = ""
    end if
    m.problem.visible = false
    m.problemShown = invalid
    m.panel.visible = false
    m.tabs.visible = true
    showTabs()
    startListing()
end sub

function hasFilters() as boolean
    return m.kind <> "" or m.person > 0 or m.words <> ""
end function

' The tabs, each with its number, closer together where the numbers would
' leave the filters too little room; then the filters.
sub showTabs()
    summary = reqSummaryOf(m.top.requestSummary)
    chips = reqTabChips(m.manage, summary, m.counts, hasFilters(), m.view, m.total)
    json = FormatJson(chips)
    if json <> m.tabChips
        m.tabChips = json
        ' The filters (or Sign in) keep about 180 pixels.
        room = 1728
        if m.manage or summary.signInAgain = "approvals" then room = 1548
        for each pad in [44, 32, 24, 16]
            m.tabs.pad = pad
            m.tabs.chips = chips
            if m.tabs.rowWidth <= room then exit for
        end for
    end if
    showSide(summary)
end sub

' At the right of the tabs: a manager's filters, saying what they keep;
' for an administrator whose sign-in can't approve, Sign in.
sub showSide(summary as object)
    label = ""
    m.sideAction = ""
    if m.manage
        label = reqFilterLabel(m.kind, m.personName, m.words)
        m.sideAction = "filter"
    else if summary.signInAgain = "approvals"
        label = "Sign in"
        m.sideAction = "signIn"
    end if
    if label = ""
        if m.side.isInFocusChain() then m.tabs.setFocus(true)
        m.side.visible = false
        return
    end if
    ' As wide as its words, or as there is room for after the tabs.
    room = 1728 - chipsWidth(m.tabs) - 32
    chips = [{ label: label, width: 0 }]
    if FormatJson(m.side.chips) <> FormatJson(chips) then m.side.chips = chips
    width = chipsWidth(m.side)
    if width > room
        width = room
        m.side.chips = [{ label: label, width: width }]
    end if
    m.side.translation = [1824 - width, 132]
    m.side.current = -1
    if hasFilters() then m.side.current = 0
    m.side.visible = true
end sub

' How wide a row of chips is, or about, should it not have said yet.
function chipsWidth(row as object) as integer
    if row.rowWidth > 0 then return row.rowWidth
    width = 0
    for each chip in row.chips
        width = width + Len(chip.label) * 15 + 80
    end for
    return width
end function

' How many there are, or how to approve here.
sub showCount()
    summary = reqSummaryOf(m.top.requestSummary)
    if not m.manage and summary.signInAgain = "approvals"
        m.count.text = "Sign in again to approve and decline here."
    else if m.total > 0
        m.count.text = reqCount(m.total, "request", "requests")
    else
        m.count.text = ""
    end if
end sub

' --- The list ---

' A tab or filters: the list starts again.
sub startListing()
    m.busy.control = "stop"
    m.listing = m.listing + 1
    m.offset = 0
    m.total = -1
    m.listBusy = false
    m.loading = false
    m.reloading = false
    m.reloadWanted = false
    if m.list.isInFocusChain() then m.tabs.setFocus(true)
    m.list.content = CreateObject("roSGNode", "ContentNode")
    m.list.visible = true
    m.hint.visible = false
    m.message.text = "Loading…"
    m.message.visible = true
    showCount()
    loadMore()
    askCounts()
end sub

sub loadMore()
    if m.loading or m.reloading or (m.total >= 0 and m.offset >= m.total) then return
    m.loading = true
    query = reqListQuery(m.view, m.kind, m.person, m.words, m.offset)
    uiRequest({ method: "GET", path: "/api/requests", query: query, build: "requestRows", manage: m.manage, listing: m.listing }, "onPage")
end sub

sub onPage(event as object)
    request = event.getRoSGNode()
    if request.spec.listing <> m.listing then return
    m.loading = false
    answer = event.getData()
    content = m.list.content
    if not answer.ok
        if content.getChildCount() > 0 or uiFailed(answer) then return
        problem = reqProblem(answer, m.manage)
        if problem = invalid then problem = { title: "Couldn't load the requests", text: answer.message, retry: true, signIn: false }
        showProblem(problem)
        return
    end if
    data = answer.data
    m.total = data.total
    m.people = data.people
    m.offset = m.offset + data.listed
    if data.listed = 0 then m.total = m.offset
    rows = request.content
    if rows <> invalid and rows.getChildCount() > 0 then content.appendChildren(rows.getChildren(-1, 0))
    keepCounts(data.counts)
    showList()
    m.listBusy = m.listBusy or data.busy
    keepAsking()
    markSeen()
    if m.reloadWanted then reload()
end sub

' The list, or what stands for it when empty; the focus goes to it as it
' first shows.
sub showList()
    showCount()
    if m.list.content.getChildCount() = 0
        m.message.text = reqEmptyText(m.view, not m.manage or m.view = "mine", hasFilters())
        m.message.visible = true
        m.hint.visible = false
        if m.list.isInFocusChain() or m.top.hasFocus() then m.tabs.setFocus(true)
        return
    end if
    m.message.visible = false
    m.hint.visible = true
    if m.top.hasFocus() then m.list.setFocus(true)
end sub

' Two rows before the end, the next page comes.
sub onRowFocused(event as object)
    if event.getData() >= m.list.content.getChildCount() - 5 then loadMore()
end sub

' The requests listed so far, asked again in one go.
sub reload()
    if not m.started or m.problemShown <> invalid then return
    if m.loading or m.reloading
        m.reloadWanted = true
        return
    end if
    m.reloadWanted = false
    m.busy.control = "stop"
    m.reloading = true
    query = reqListQuery(m.view, m.kind, m.person, m.words, 0)
    limit = m.list.content.getChildCount()
    if limit < pageSize() then limit = pageSize()
    if limit > 200 then limit = 200
    query.limit = limit
    uiRequest({ method: "GET", path: "/api/requests", query: query, build: "requestRows", manage: m.manage, listing: m.listing }, "onReloaded")
    askCounts()
end sub

' A reload that fails leaves the list as it was.
sub onReloaded(event as object)
    request = event.getRoSGNode()
    if request.spec.listing <> m.listing then return
    m.reloading = false
    answer = event.getData()
    if answer.ok
        data = answer.data
        m.total = data.total
        m.people = data.people
        m.offset = data.listed
        rows = request.content
        if rows = invalid then rows = CreateObject("roSGNode", "ContentNode")
        putRows(rows)
        keepCounts(data.counts)
        showList()
        m.listBusy = data.busy
        markSeen()
    end if
    keepAsking()
    if m.reloadWanted then reload()
end sub

' The rows asked again, in place of those shown. The same requests in the
' same order: only those that changed are replaced. Otherwise the focus
' stays on the request it was on, or where it was.
sub putRows(fresh as object)
    content = m.list.content
    rows = fresh.getChildren(-1, 0)
    if sameRequests(content, rows)
        for i = 0 to rows.Count() - 1
            if rowChanged(content.getChild(i), rows[i]) then content.replaceChild(rows[i], i)
        end for
        return
    end if
    at = m.list.itemFocused
    focusedId = 0
    shown = content.getChild(at)
    if shown <> invalid then focusedId = shown.requestId
    m.list.content = fresh
    for i = 0 to rows.Count() - 1
        if focusedId > 0 and rows[i].requestId = focusedId then at = i
    end for
    if at >= rows.Count() then at = rows.Count() - 1
    if at > 0 then m.list.jumpToItem = at
end sub

function sameRequests(content as object, rows as object) as boolean
    if content.getChildCount() <> rows.Count() then return false
    for i = 0 to rows.Count() - 1
        if content.getChild(i).requestId <> rows[i].requestId then return false
    end for
    return true
end function

' The statuses' counts a list came with, where they are of the filters
' alone; the tabs say them.
sub keepCounts(counts as dynamic)
    if reqCountsQuery(m.view, m.kind, m.person, m.words) = invalid then m.counts = counts
    showTabs()
end sub

' Needs attention's and Yours' lists narrow the counts: those come on their
' own.
sub askCounts()
    query = reqCountsQuery(m.view, m.kind, m.person, m.words)
    if query <> invalid then uiRequest({ method: "GET", path: "/api/requests", query: query, listing: m.listing }, "onCounts")
end sub

sub onCounts(event as object)
    if event.getRoSGNode().spec.listing <> m.listing then return
    answer = event.getData()
    data = invalid
    if answer.ok then data = httpObject(answer.data)
    if data = invalid or httpObject(data.counts) = invalid then return
    m.counts = data.counts
    showTabs()
end sub

function rowChanged(shown as object, fresh as object) as boolean
    for each name in ["title", "subtitle", "image", "stageText", "stageColor", "progress", "who", "line", "unseen", "status", "stage", "libraryId"]
        if shown.getField(name) <> fresh.getField(name) then return true
    end for
    return FormatJson(shown.actions) <> FormatJson(fresh.actions) or FormatJson(shown.seasons) <> FormatJson(fresh.seasons)
end function

' Asked again in 10 s while one of the requests downloads or goes into a
' library, in 30 s while the event stream is down (reqListPoll).
sub keepAsking()
    m.busy.control = "stop"
    seconds = reqListPoll(m.listBusy, m.top.getScene().eventsLive)
    if seconds = 0 or not m.top.active then return
    m.busy.duration = seconds
    m.busy.control = "start"
end sub

' The account's own requests shown (Your requests, a manager's Yours):
' its news is seen, and the number on the menu goes.
sub markSeen()
    if m.seeing or (m.manage and m.view <> "mine") then return
    if reqSummaryOf(m.top.requestSummary).unseen <= 0 then return
    m.seeing = true
    uiRequest({ method: "POST", path: "/api/requests/seen", jsonText: "{}" }, "onSeen")
end sub

sub onSeen(event as object)
    m.seeing = false
    if event.getData().ok then m.top.getScene().requestTouched = { requestId: 0, kind: "", tmdbId: 0 }
end sub

' --- The requests ---

' OK opens the title's page, showing this request.
sub onRow(event as object)
    row = m.list.content.getChild(event.getData())
    if row = invalid then return
    uiNavigate({ to: "requestTitle", params: { kind: row.tmdbKind, tmdbId: row.tmdbId, title: row.title, requestId: row.requestId } })
end sub

' The Options key: what can be done with the request focused.
sub rowOptions()
    row = m.list.content.getChild(m.list.itemFocused)
    if row = invalid then return
    target = { requestId: row.requestId, title: row.title, status: row.status, stage: row.stage, kind: row.tmdbKind, tmdbId: row.tmdbId, seasons: row.seasons, libraryId: row.libraryId, actions: row.actions }
    if not uiRequestOptions(target) then uiMessage(row.title, ["Nothing can be done with this request now."])
end sub

' An action was done, or refused as the request had changed: the list as
' it is now; the summary (a manager's count) and the title's badge
' elsewhere follow.
sub onRequestActed()
    target = m.requestTarget
    if target <> invalid then m.top.getScene().requestTouched = { requestId: target.requestId, kind: target.kind, tmdbId: target.tmdbId }
    reload()
end sub

' --- Tabs, filters and the summary ---

' OK on a tab shows it; on the one shown, goes down to its requests.
sub onTab(event as object)
    index = event.getData()
    if index < 0 or index >= m.tabKeys.Count() then return
    if m.tabKeys[index] = m.view
        if m.list.content.getChildCount() > 0 then m.list.setFocus(true)
        return
    end if
    m.tabs.current = index
    m.view = m.tabKeys[index]
    startListing()
end sub

sub onSide()
    if m.sideAction = "signIn"
        uiNavigate({ to: "link", params: { reason: "approvals" } })
    else if m.sideAction = "filter"
        openFilters()
    end if
end sub

' The filters, each with what it keeps; OK opens its choices.
sub openFilters()
    m.panelKind = "filters"
    m.panelTitle.text = "Filter"
    kind = "Movies and shows"
    if m.kind = "movie" then kind = "Movies"
    if m.kind = "show" then kind = "Shows"
    person = "Everyone"
    if m.person > 0 then person = m.personName
    words = "Any"
    if m.words <> "" then words = reqQuoted(m.words)
    items = [{ title: "Show: " + kind }, { title: "Asked for by: " + person }, { title: "Title: " + words }]
    if hasFilters() then items.Push({ title: "Clear filters" })
    at = m.menu.itemFocused
    m.menu.content = nodesContent(items)
    if at > 0 and at < items.Count() then m.menu.jumpToItem = at
    m.choices.visible = false
    m.menu.visible = true
    m.panel.visible = true
    m.menu.setFocus(true)
end sub

sub onMenu(event as object)
    at = event.getData()
    if at = 0
        current = 0
        if m.kind = "movie" then current = 1
        if m.kind = "show" then current = 2
        showChoices("kind", "Show", ["Movies and shows", "Movies", "Shows"], ["", "movie", "show"], current)
    else if at = 1
        labels = ["Everyone"]
        values = [0]
        current = 0
        for each person in m.people
            if person.id = m.person then current = values.Count()
            labels.Push(person.name)
            values.Push(person.id)
        end for
        ' The one chosen stays offered when the list no longer names them.
        if m.person > 0 and current = 0
            current = values.Count()
            labels.Push(m.personName)
            values.Push(m.person)
        end if
        showChoices("person", "Asked for by", labels, values, current)
    else if at = 2
        askWords()
    else
        m.kind = ""
        m.person = 0
        m.personName = ""
        m.words = ""
        applyFilters()
        openFilters()
    end if
end sub

sub showChoices(kind as string, title as string, labels as object, values as object, current as integer)
    m.panelKind = kind
    m.values = values
    m.choiceLabels = labels
    m.panelTitle.text = title
    items = []
    for each label in labels
        items.Push({ title: label })
    end for
    m.choices.content = nodesContent(items)
    m.choices.checkedItem = current
    m.choices.jumpToItem = current
    m.menu.visible = false
    m.choices.visible = true
    m.panel.visible = true
    m.choices.setFocus(true)
end sub

sub onChoice(event as object)
    at = event.getData()
    if at < 0 or at >= m.values.Count() then return
    if m.panelKind = "kind" then m.kind = m.values[at]
    if m.panelKind = "person"
        m.person = m.values[at]
        m.personName = ""
        if m.person > 0 then m.personName = m.choiceLabels[at]
    end if
    applyFilters()
    openFilters()
end sub

' Words the titles must have, on the keyboard.
sub askWords()
    dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
    dialog.title = "Title"
    dialog.message = ["Requests whose title has these words."]
    dialog.text = m.words
    dialog.buttons = ["OK", "Cancel"]
    dialog.observeField("buttonSelected", "onWords")
    m.top.getScene().dialog = dialog
end sub

sub onWords(event as object)
    dialog = m.top.getScene().dialog
    words = m.words
    if dialog <> invalid then words = dialog.text
    uiCloseDialog()
    if event.getData() = 0 and words.Trim() <> m.words
        m.words = words.Trim()
        applyFilters()
    end if
    openFilters()
end sub

' Other filters: the counts are known again with their list.
sub applyFilters()
    m.counts = invalid
    m.total = -1
    showTabs()
    startListing()
end sub

sub closePanel()
    m.panel.visible = false
    m.panelKind = ""
    m.side.setFocus(true)
end sub

' The summary changed: a manager's page in place of Your requests, or
' back; Sign in where it would help, and Needs attention's and
' Yours' numbers.
sub onSummary()
    if not m.started then return
    summary = reqSummaryOf(m.top.requestSummary)
    if (summary.manage = true) <> m.manage
        showPage(m.view)
        return
    end if
    if m.problemShown <> invalid then return
    m.heading.text = reqPageName(summary)
    showTabs()
    showCount()
end sub

' --- What stops the requests ---

' In place of the list, why it can't be shown: with Sign in, Try
' again, or nothing to do here.
sub showProblem(problem as object)
    m.problemShown = problem
    m.busy.control = "stop"
    m.tabs.visible = false
    m.side.visible = false
    m.list.visible = false
    m.message.visible = false
    m.hint.visible = false
    m.panel.visible = false
    m.count.text = ""
    m.problemTitle.text = problem.title
    m.problemText.text = problem.text
    buttons = []
    if problem.signIn then buttons.Push("Sign in")
    if problem.retry then buttons.Push("Try again")
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
    m.top.setFocus(true)
    showPage(m.view)
end sub

' --- The remote ---

sub restoreFocus()
    if m.problem.visible
        if m.problemButtons.visible
            m.problemButtons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    else if m.lastFocus = "list" and m.list.content.getChildCount() > 0
        m.list.setFocus(true)
    else if m.lastFocus = "side" and m.side.visible
        m.side.setFocus(true)
    else
        m.tabs.setFocus(true)
    end if
end sub

' Down from the tabs goes to the requests, up from the first back; right
' from the last tab reaches the filters (or Sign in). Back closes
' the panel (a filter's choices go back to the filters). The Options key
' on a request offers what can be done with it.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.panel.visible
        if key = "back" or key = "left"
            if m.panelKind = "filters"
                closePanel()
            else
                openFilters()
            end if
            return true
        end if
        return false
    end if
    if m.problem.visible then return false
    ' While the first page comes, the arrows reach the tabs.
    if m.top.hasFocus() and (key = "up" or key = "down" or key = "left" or key = "right")
        m.tabs.setFocus(true)
        return true
    end if
    rows = m.list.content.getChildCount() > 0
    if key = "down" and (m.tabs.isInFocusChain() or m.side.isInFocusChain()) and rows
        m.list.setFocus(true)
        return true
    end if
    if key = "up" and m.list.isInFocusChain()
        m.tabs.setFocus(true)
        return true
    end if
    if key = "right" and m.tabs.isInFocusChain() and m.side.visible
        m.side.setFocus(true)
        return true
    end if
    if key = "left" and m.side.isInFocusChain()
        m.tabs.setFocus(true)
        return true
    end if
    if key = "options" and m.list.isInFocusChain() and rows
        rowOptions()
        return true
    end if
    return false
end function
