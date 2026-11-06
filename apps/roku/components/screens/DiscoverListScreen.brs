' One of Discover's lists (GET /api/discover/{list}?page=), or TMDB's
' results for the words searched (GET /api/discover/search?q=&page=, See
' all on Discover in Search), a page at a time. TMDB's pages can come
' short, even empty, while titles were left out, and repeat titles: a
' short page isn't the end, only the last page is, and a title shows once.
' Five pages at most come each time the focus moves, then Look further
' asks for more; a page that fails ends the grid in a tile that says so,
' Select asking for it again. params: { list (a key of reqLists) } or
' { query (the words searched) }.

sub init()
    m.title = m.top.findNode("title")
    m.grid = m.top.findNode("grid")
    m.message = m.top.findNode("message")
    m.problem = m.top.findNode("problem")
    m.problemTitle = m.top.findNode("problemTitle")
    m.problemText = m.top.findNode("problemText")
    m.problemButtons = m.top.findNode("problemButtons")
    m.top.findNode("credit").text = t("requests.credit")
    m.started = false
    ' What the grid shows: { title, path, empty } (reqSeeAll).
    m.shown = invalid
    m.problemShown = invalid
    m.grid.observeField("itemSelected", "onSelected")
    m.grid.observeField("itemFocused", "onFocused")
    m.problemButtons.observeField("buttonSelected", "onProblemButton")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestNote", "onRequestNote")
end sub

sub onActive()
    if not m.top.active then return
    if m.started
        if m.problem.visible and m.problemButtons.visible
            m.problemButtons.setFocus(true)
        else
            m.grid.setFocus(true)
        end if
        return
    end if
    m.started = true
    m.shown = reqSeeAll(httpString(m.top.params.list), httpString(m.top.params.query))
    m.title.text = m.shown.title
    load()
end sub

' The first page, empty frames until it comes.
sub load()
    m.problem.visible = false
    m.problemShown = invalid
    m.message.visible = false
    m.page = 0
    m.totalPages = -1
    m.seen = {}
    m.loading = false
    ' A page failed: none is asked for until its tile's Select.
    m.failed = false
    ' Pages asked for since the focus last moved.
    m.asked = 0
    m.content = invalid
    m.grid.content = nodesContent(reqSkeletonTiles())
    m.grid.visible = true
    m.grid.setFocus(true)
    loadMore()
end sub

sub loadMore()
    if m.loading or m.failed or (m.totalPages >= 0 and m.page >= m.totalPages) then return
    m.loading = true
    m.asked = m.asked + 1
    uiRequest({ method: "GET", path: m.shown.path, query: { page: m.page + 1 }, build: "discoverTiles", known: m.seen }, "onPage")
end sub

sub onPage(event as object)
    m.loading = false
    answer = event.getData()
    if not answer.ok
        if m.content <> invalid
            ' What came stays, ending in what stopped the next page.
            dropEnd()
            m.failed = true
            nodesAppend(m.content, [reqMoreProblemTile(answer, reqSummaryOf(m.top.requestSummary).manage)])
            return
        end if
        if uiFailed(answer) then return
        problem = reqProblem(answer, reqSummaryOf(m.top.requestSummary).manage)
        if problem = invalid then problem = { title: t("requests.discover.loadFailed"), text: answer.message, retry: true, signIn: false }
        showProblem(problem)
        return
    end if
    page = answer.data
    ' A server that answers the same page again has no more.
    if page.page <= m.page
        m.totalPages = m.page
    else
        m.page = page.page
        m.totalPages = page.totalPages
    end if
    tiles = []
    content = event.getRoSGNode().content
    if content <> invalid
        for each node in content.getChildren(-1, 0)
            if not m.seen.DoesExist(node.key)
                m.seen[node.key] = true
                tiles.Push(node)
            end if
        end for
    end if
    if m.content <> invalid then dropEnd()
    if m.content = invalid and (tiles.Count() > 0 or m.page >= m.totalPages or m.asked >= 5)
        ' The first titles (or Look further) take the frames' place.
        m.content = CreateObject("roSGNode", "ContentNode")
        m.content.appendChildren(tiles)
        m.grid.content = m.content
    else if m.content <> invalid and tiles.Count() > 0
        m.content.appendChildren(tiles)
    end if
    if m.content <> invalid and m.content.getChildCount() = 0 and m.page >= m.totalPages
        m.grid.visible = false
        m.message.text = m.shown.empty
        m.message.visible = true
        return
    end if
    ' Until a screenful past the focus: the next page; once five came,
    ' Look further.
    count = 0
    if m.content <> invalid then count = m.content.getChildCount()
    if reqWantsPage(m.page, m.totalPages, m.grid.itemFocused, count, 14, m.asked)
        loadMore()
    else if m.content <> invalid and m.page < m.totalPages and m.asked >= 5
        nodesAppend(m.content, [reqFurtherTile()])
    end if
end sub

' Look further, or what stopped a page, goes as the next page comes.
sub dropEnd()
    last = m.content.getChildCount() - 1
    if last < 0 then return
    kind = m.content.getChild(last).kind
    if kind = "further" or kind = "retry" or kind = "note" then m.content.removeChildIndex(last)
end sub

' Two rows before the end, the next page comes.
sub onFocused(event as object)
    if m.content = invalid then return
    m.asked = 0
    if event.getData() >= m.content.getChildCount() - 14 then loadMore()
end sub

sub onSelected(event as object)
    tile = m.grid.content.getChild(event.getData())
    if tile <> invalid and tile.kind = "further"
        m.asked = 0
        loadMore()
        return
    end if
    if tile <> invalid and tile.kind = "retry"
        m.failed = false
        m.asked = 0
        loadMore()
        return
    end if
    uiOpenTile(tile)
end sub

' A title's request changed: its badge.
sub onRequestNote()
    uiPatchRequestTiles(m.content, m.top.requestNote)
end sub

' In place of the grid, why it can't be shown: with Sign in, Try
' again, or nothing to do here.
sub showProblem(problem as object)
    m.problemShown = problem
    m.grid.visible = false
    m.problemTitle.text = problem.title
    m.problemText.text = problem.text
    buttons = []
    if problem.signIn then buttons.Push(t("requests.buttons.signIn"))
    if problem.retry then buttons.Push(t("common.actions.tryAgain"))
    m.problemButtons.buttons = buttons
    m.problemButtons.visible = buttons.Count() > 0
    m.problem.visible = true
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
    else
        load()
    end if
end sub
