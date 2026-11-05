' Discover (GET /api/discover/{list}): a hero, TMDB's lists as rows of
' posters loaded as they come into view, and Your requests (GET
' /api/requests?mine=1). A request changed elsewhere changes the badge of
' its title where it shows (the scene's requestNote), never the lists.

sub init()
    ' Whether Your requests showed: a poll that fails then leaves them be.
    m.requestsShown = false
    m.backdrop = m.top.findNode("backdrop")
    m.shade = m.top.findNode("shade")
    m.page = m.top.findNode("page")
    m.chips = m.top.findNode("chips")
    m.credit = m.top.findNode("credit")
    m.hero = m.top.findNode("hero")
    m.heroName = m.top.findNode("heroName")
    m.heroLine = m.top.findNode("heroLine")
    m.heroOverview = m.top.findNode("heroOverview")
    m.heroButtons = m.top.findNode("heroButtons")
    m.heroNote = m.top.findNode("heroNote")
    m.rows = m.top.findNode("rows")
    m.problem = m.top.findNode("problem")
    m.problemTitle = m.top.findNode("problemTitle")
    m.problemText = m.top.findNode("problemText")
    m.problemButtons = m.top.findNode("problemButtons")
    m.busy = m.top.findNode("busy")
    m.started = false
    ' The rows in order: { key (a list's, or "requests"), title, state
    ' ("waiting", "asked", "shown"), page (the last one asked for), seen
    ' (the titles shown: { "movie:603": true }), first (its first title,
    ' a DiscoverTitle, or invalid) }.
    m.lists = []
    m.generation = 0
    ' The hero's title (DiscoverTitle), and how it shows (reqHero's);
    ' whether it was found, or none can be.
    m.heroTitle = invalid
    m.heroShown = invalid
    m.heroFound = false
    ' A row answered: a problem after that is only that row's.
    m.answered = false
    ' The problem shown in place of the page (reqProblem's), or invalid;
    ' whether the page shows only Sign in.
    m.problemShown = invalid
    m.signInOnly = false
    ' How far the page slides up to the rows, and where the focus was when
    ' covered.
    m.slide = 560
    m.lastFocus = ""
    m.heroButtons.observeField("buttonSelected", "onHeroButton")
    m.problemButtons.observeField("buttonSelected", "onProblemButton")
    m.chips.observeField("chosen", "onChip")
    m.rows.observeField("rowItemSelected", "onTile")
    m.rows.observeField("rowItemFocused", "onRowFocused")
    m.busy.observeField("fire", "reloadRequests")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestsChanged", "reloadRequests")
    m.top.observeField("requestNote", "onRequestNote")
end sub

sub onActive()
    if not m.top.active
        m.busy.control = "stop"
        m.lastFocus = "hero"
        if m.rows.isInFocusChain() then m.lastFocus = "rows"
        if m.chips.isInFocusChain() then m.lastFocus = "chips"
        return
    end if
    if not m.started
        m.started = true
        showPage()
        return
    end if
    restoreFocus()
    ' Back from a title's page: what was asked for there shows in Your
    ' requests.
    reloadRequests()
end sub

' The page for the summary: only Sign in for a sign-in from before
' apps could ask; else the hero and the rows, each row's tiles empty
' frames until it loads.
sub showPage()
    summary = reqSummaryOf(m.top.requestSummary)
    m.busy.control = "stop"
    m.answered = false
    m.heroTitle = invalid
    m.heroShown = invalid
    m.heroFound = false
    m.hero.visible = false
    m.backdrop.uri = ""
    m.problem.visible = false
    m.problemShown = invalid
    m.page.translation = [0, 0]
    m.shade.width = 1260
    m.signInOnly = summary.signInAgain = "requests"
    if m.signInOnly
        showProblem(reqSignInAgain())
        return
    end if
    showChips(summary)
    m.credit.visible = true
    ' Answers to the page as it was before are left aside.
    m.generation = m.generation + 1
    m.lists = []
    for each list in reqLists()
        m.lists.Push({ key: list.key, title: list.title, state: "waiting", page: 0, seen: {}, first: invalid })
    end for
    if reqShowsRequests(summary) and summary.own > 0 then m.lists.Push({ key: "requests", title: "Your requests", state: "waiting", page: 0, seen: {}, first: invalid })
    content = CreateObject("roSGNode", "ContentNode")
    for each list in m.lists
        row = content.createChild("ContentNode")
        row.title = list.title
        nodesAppend(row, reqSkeletonTiles())
    end for
    m.rows.content = content
    placeRows(true)
    m.rows.visible = true
    m.top.setFocus(true)
    ensureRows(1)
end sub

' The requests, where the summary has them, with the menu's number of
' what waits there. Without them the focus leaves the empty row.
sub showChips(summary as object)
    chips = []
    if reqShowsRequests(summary) then chips.Push(reqRequestsChip(summary))
    if FormatJson(m.chips.chips) <> FormatJson(chips) then m.chips.chips = chips
    m.chips.visible = chips.Count() > 0
    if not m.chips.visible and m.chips.isInFocusChain() then focusRows(false)
end sub

' The rows under the hero, or at the top of the page without one.
sub placeRows(withHero as boolean)
    if withHero
        m.rows.translation = [96, 700]
        m.slide = 560
    else
        m.rows.translation = [96, 196]
        m.slide = 56
    end if
    if m.rows.isInFocusChain() then m.page.translation = [0, -m.slide]
end sub

' --- The rows ---

' The rows up to upto load, if they haven't.
sub ensureRows(upto as integer)
    for i = 0 to upto
        if i < m.lists.Count() and m.lists[i].state = "waiting" then askRow(i)
    end for
end sub

' The row after the one focused comes into view: it loads.
sub onRowFocused()
    position = m.rows.rowItemFocused
    if position <> invalid and position.Count() = 2 then ensureRows(position[0] + 1)
end sub

sub askRow(index as integer)
    list = m.lists[index]
    list.state = "asked"
    if list.key = "requests"
        summary = reqSummaryOf(m.top.requestSummary)
        uiRequest({ method: "GET", path: "/api/requests", query: { mine: 1, limit: 20 }, build: "requestTiles", manage: summary.manage, row: index, generation: m.generation }, "onRequestsRow")
        return
    end if
    uiRequest({ method: "GET", path: "/api/discover/" + list.key, query: { page: list.page + 1 }, build: "discoverTiles", known: list.seen, row: index, list: list.key, generation: m.generation }, "onListRow")
end sub

' Whether an answer is to a row on the page now.
function isCurrent(spec as object, key as string) as boolean
    return spec.generation = m.generation and spec.row < m.lists.Count() and m.lists[spec.row].key = key
end function

' A page of a list: its titles after those shown, then See all. A page
' short of a screenful isn't the end: the next one comes, up to five,
' then See all goes on from there.
sub onListRow(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    answer = event.getData()
    if not isCurrent(spec, spec.list) then return
    list = m.lists[spec.row]
    shown = []
    if list.page > 0
        for each node in m.rows.content.getChild(spec.row).getChildren(-1, 0)
            if node.kind = "discover" then shown.Push(node)
        end for
    end if
    if not answer.ok
        list.state = "shown"
        ' The server away: the page in view gives way to the screen that
        ' says so, a covered one's row offers Try again.
        if uiFailed(answer) and (m.top.active or answer.error <> "network") then return
        problem = reqProblem(answer, reqSummaryOf(m.top.requestSummary).manage)
        if problem <> invalid and not m.answered
            showProblem(problem)
            return
        end if
        ' What came before stays, See all going on from it.
        if shown.Count() > 0
            setRow(spec.row, shown, reqMoreTile("See all", list.key, ""))
        else
            setRow(spec.row, [], reqNoticeTile("Couldn't load these. Select to try again.", "retry"))
        end if
        ' Without trending, the hero comes from the other lists: every one
        ' at once, as when trending has nothing to ask for.
        if list.key = "trending" and list.first = invalid then ensureRows(m.lists.Count() - 1)
        findHero()
        return
    end if
    m.answered = true
    page = answer.data
    if list.first = invalid and httpNumber(page.first.tmdb_id, 0) > 0 then list.first = page.first
    ' A server that answers the same page again has no more.
    more = page.page < page.totalPages and page.page > list.page
    list.page = page.page
    if request.content <> invalid
        for each node in request.content.getChildren(-1, 0)
            if not list.seen.DoesExist(node.key)
                list.seen[node.key] = true
                shown.Push(node)
            end if
        end for
    end if
    askNext = shown.Count() < 7 and more and page.page < 5
    extra = invalid
    if not askNext
        if more then extra = reqMoreTile("See all", list.key, "")
        if shown.Count() = 0 and extra = invalid then extra = reqNoticeTile("Nothing here right now.", "note")
    end if
    if shown.Count() > 0 or extra <> invalid then setRow(spec.row, shown, extra)
    if askNext
        askRow(spec.row)
    else
        list.state = "shown"
        ' Trending has nothing the profile may ask for: every list comes
        ' at once, for a hero.
        if list.key = "trending" and list.first = invalid then ensureRows(m.lists.Count() - 1)
    end if
    findHero()
end sub

' Your requests: their posters and stages, then See all.
sub onRequestsRow(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    answer = event.getData()
    if not isCurrent(spec, "requests") then return
    m.lists[spec.row].state = "shown"
    if not answer.ok
        if uiFailed(answer) then return
        ' A poll that fails leaves the requests shown, and asks again in
        ' its time; with none shown yet, the row says so.
        if m.requestsShown
            restartPoll(false)
        else
            setRow(spec.row, [], reqNoticeTile("Couldn't load these. Select to try again.", "retry"))
        end if
        return
    end if
    m.requestsShown = true
    tiles = []
    if request.content <> invalid then tiles = request.content.getChildren(-1, 0)
    extra = reqMoreTile("See all", "requests", "")
    if tiles.Count() = 0 then extra = reqNoticeTile(reqEmptyText("all", true, false), "note")
    setRow(spec.row, tiles, extra)
    restartPoll(answer.data.busy)
end sub

' Your requests asked again in 10 s while one of them downloads, in 30 s
' while the event stream is down (reqListPoll).
sub restartPoll(busy as boolean)
    m.busy.control = "stop"
    seconds = reqListPoll(busy, m.top.getScene().eventsLive)
    if seconds > 0 and m.top.active
        m.busy.duration = seconds
        m.busy.control = "start"
    end if
end sub

' Your requests asks again: requests changed, one of them downloads, the
' event stream is down, or the page shows again.
sub reloadRequests()
    for i = 0 to m.lists.Count() - 1
        if m.lists[i].key = "requests" and m.lists[i].state = "shown" then askRow(i)
    end for
end sub

' Puts tiles (ContentNodes) in row index, then extra (a tile's fields, or
' invalid), in place of what it showed; the focus stays where it was.
sub setRow(index as integer, tiles as object, extra as dynamic)
    content = m.rows.content
    if content = invalid or index >= content.getChildCount() then return
    row = CreateObject("roSGNode", "ContentNode")
    row.title = m.lists[index].title
    row.appendChildren(tiles)
    if extra <> invalid then nodesAppend(row, [extra])
    focus = m.rows.rowItemFocused
    content.replaceChild(row, index)
    if focus <> invalid and focus.Count() = 2 and focus[0] = index and m.rows.isInFocusChain()
        column = focus[1]
        if column >= row.getChildCount() then column = row.getChildCount() - 1
        if column < 0 then column = 0
        m.rows.jumpToRowItem = [index, column]
    end if
end sub

' A tile opens its title or its list; a row's Try again loads it again.
sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row = invalid then return
    tile = row.getChild(position[1])
    if tile = invalid then return
    if tile.kind = "retry" and position[0] < m.lists.Count()
        list = m.lists[position[0]]
        list.page = 0
        list.seen = {}
        setRow(position[0], nodesContent(reqSkeletonTiles()).getChildren(-1, 0), invalid)
        askRow(position[0])
        return
    end if
    uiOpenTile(tile)
end sub

' A title's request changed: its badge, in the rows and the hero.
sub onRequestNote()
    note = m.top.requestNote
    uiPatchRequestTiles(m.rows.content, note)
    if m.heroTitle <> invalid and reqKey(httpString(m.heroTitle.kind), m.heroTitle.tmdb_id) = note.key
        m.heroTitle.status = note.status
        m.heroTitle.item_id = note.libraryId
        drawHero()
    end if
end sub

' --- The hero ---

' The hero, once the lists say which it is (reqHeroOf).
sub findHero()
    if m.heroFound then return
    title = reqHeroOf(m.lists)
    if title = invalid then return
    m.heroFound = true
    showHero(title)
end sub

' The hero's title, or none: the rows move up.
sub showHero(title as object)
    if httpNumber(title.tmdb_id, 0) = 0
        placeRows(false)
        if m.top.hasFocus() then focusRows(true)
        return
    end if
    m.heroTitle = title
    placeRows(true)
    drawHero()
end sub

sub drawHero()
    if m.heroTitle = invalid then return
    m.heroShown = reqHero(m.heroTitle, reqSummaryOf(m.top.requestSummary), m.global.session.server, CreateObject("roDateTime").AsSeconds())
    m.heroName.text = m.heroShown.title
    m.heroLine.text = m.heroShown.line
    m.heroOverview.text = m.heroShown.overview
    m.heroNote.text = m.heroShown.note
    if FormatJson(m.heroButtons.buttons) <> FormatJson(m.heroShown.buttons) then m.heroButtons.buttons = m.heroShown.buttons
    if m.backdrop.uri <> m.heroShown.backdrop
        m.backdrop.setHttpAgent(m.top.getScene().getHttpAgent())
        m.backdrop.uri = m.heroShown.backdrop
    end if
    m.hero.visible = true
    if m.top.hasFocus() then m.heroButtons.setFocus(true)
end sub

' Open: the library's title; Request and Details: the title's page,
' asking at once for Request.
sub onHeroButton(event as object)
    hero = m.heroShown
    if hero = invalid then return
    action = hero.actions[event.getData()]
    if action = "open"
        uiNavigate({ to: "details", params: { itemId: hero.libraryId } })
    else
        uiNavigate({ to: "requestTitle", params: { kind: hero.tmdbKind, tmdbId: hero.tmdbId, title: hero.title, ask: action = "request" } })
    end if
end sub

sub onChip()
    uiNavigate({ to: "requests" })
end sub

' --- What stops Discover ---

' In place of the page, why it can't be used: with Sign in, Try
' again, or nothing to do here.
sub showProblem(problem as object)
    m.problemShown = problem
    m.busy.control = "stop"
    m.hero.visible = false
    m.rows.visible = false
    m.chips.visible = false
    m.credit.visible = false
    m.backdrop.uri = ""
    m.page.translation = [0, 0]
    m.shade.width = 1260
    m.problemTitle.text = problem.title
    m.problemText.text = problem.text
    buttons = []
    if problem.signIn then buttons.Push("Sign in")
    if problem.retry then buttons.Push("Try again")
    m.problemButtons.buttons = buttons
    m.problemButtons.visible = buttons.Count() > 0
    m.problem.visible = true
    if m.top.isInFocusChain()
        if buttons.Count() > 0
            m.problemButtons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

sub onProblemButton()
    if m.problemShown = invalid then return
    if m.problemShown.signIn
        uiNavigate({ to: "link", params: { reason: "requests" } })
    else
        showPage()
    end if
end sub

' The summary changed: signing in again (or a new sign-in) changes the
' whole page; otherwise the chips and the hero's main action follow.
sub onSummary()
    if not m.started then return
    summary = reqSummaryOf(m.top.requestSummary)
    if (summary.signInAgain = "requests") <> m.signInOnly
        showPage()
        return
    end if
    if m.problemShown <> invalid then return
    showChips(summary)
    drawHero()
    addRequestsRow(summary)
end sub

' A first request, asked for since the page showed: Your requests comes
' after TMDB's lists, loaded as it comes into view like them.
sub addRequestsRow(summary as object)
    if not reqShowsRequests(summary) or summary.own <= 0 or m.rows.content = invalid then return
    for each list in m.lists
        if list.key = "requests" then return
    end for
    m.lists.Push({ key: "requests", title: "Your requests", state: "waiting", page: 0, seen: {} })
    row = m.rows.content.createChild("ContentNode")
    row.title = "Your requests"
    nodesAppend(row, reqSkeletonTiles())
    position = m.rows.rowItemFocused
    if m.rows.isInFocusChain() and position <> invalid and position.Count() = 2 then ensureRows(position[0] + 1)
end sub

' --- The remote ---

' The page slides up to the rows as they take the focus, the backdrop
' shaded under them all, and back down.
sub focusRows(onRows as boolean)
    if onRows
        m.page.translation = [0, -m.slide]
        m.shade.width = 1920
        m.rows.setFocus(true)
    else
        m.page.translation = [0, 0]
        m.shade.width = 1260
        if m.hero.visible
            m.heroButtons.setFocus(true)
        else if m.chips.visible
            m.chips.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

sub restoreFocus()
    if m.problem.visible
        if m.problemButtons.visible
            m.problemButtons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    else if m.lastFocus = "rows" and m.rows.visible
        m.rows.setFocus(true)
    else if m.lastFocus = "chips" and m.chips.visible
        m.chips.setFocus(true)
    else if m.hero.visible
        m.heroButtons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

' Up and Down between the chips, the hero and the rows; Back from the
' rows goes to the hero first.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press or m.problem.visible then return false
    if key = "down"
        if m.chips.isInFocusChain()
            if m.hero.visible
                m.heroButtons.setFocus(true)
            else if m.rows.visible
                focusRows(true)
            end if
            return true
        end if
        if (m.heroButtons.isInFocusChain() or m.top.hasFocus()) and m.rows.visible
            focusRows(true)
            return true
        end if
    else if key = "up"
        if m.rows.isInFocusChain()
            focusRows(false)
            return true
        end if
        if (m.heroButtons.isInFocusChain() or m.top.hasFocus()) and m.chips.visible
            m.chips.setFocus(true)
            return true
        end if
    else if key = "back" and m.rows.isInFocusChain() and m.hero.visible
        focusRows(false)
        return true
    end if
    return false
end function
