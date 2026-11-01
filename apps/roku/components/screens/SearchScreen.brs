' Search with the on-screen keyboard, once typing rests for half a second.
' What the libraries have (GET /api/search) comes in rows by kind, as on
' the other apps (lib/Search.brs): People, Live TV (cards; OK watches the
' channel), Movies, Shows, Seasons, Episodes, Artists, Albums, Tracks.
' Where the person may ask for titles (the scene's summary), On TMDB (GET
' /api/discover/search?missing=1), asked at the same time and never waited
' for, comes after them, ending with See all on Discover. Discover's
' search (params.discover) has TMDB's titles only, with their badges, a
' page at a time. params: { discover, query (searched at once) }.

sub init()
    m.title = m.top.findNode("title")
    m.keyboard = m.top.findNode("keyboard")
    m.grid = m.top.findNode("grid")
    m.rows = m.top.findNode("rows")
    ' The Live TV channels found, for the player.
    m.channels = []
    m.message = m.top.findNode("message")
    m.debounce = m.top.findNode("debounce")
    m.started = false
    m.discover = false
    ' The search asked last: answers for earlier ones are dropped.
    m.query = ""
    m.path = ""
    m.tmdbPath = ""
    ' Whether TMDB's titles show with the libraries'; the libraries' rows
    ' ([{ key, node }]) and TMDB's tiles as they come (invalid until then),
    ' and what the libraries' failure said.
    m.mayAsk = false
    m.libraryRows = invalid
    m.tmdbTiles = invalid
    m.libraryMessage = ""
    m.rowKeys = []
    ' Discover's search: the pages asked for (since the focus last moved,
    ' asked), and the titles shown.
    m.page = 0
    m.totalPages = -1
    m.seen = {}
    m.loading = false
    m.asked = 0
    m.content = invalid
    m.lastFocus = m.keyboard
    m.keyboard.observeField("text", "onText")
    m.debounce.observeField("fire", "search")
    m.grid.observeField("itemSelected", "onSelected")
    m.grid.observeField("itemFocused", "onFocused")
    m.rows.observeField("rowItemSelected", "onRowTile")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestNote", "onRequestNote")
end sub

sub onActive()
    if not m.top.active
        m.lastFocus = m.keyboard
        if m.grid.isInFocusChain() then m.lastFocus = m.grid
        if m.rows.isInFocusChain() then m.lastFocus = m.rows
        return
    end if
    if not m.started then firstShow()
    m.lastFocus.setFocus(true)
    for each change in uiChangesSince()
        onTitleChanged(change)
    end for
end sub

' Discover's search, or the libraries' (with TMDB's where the person may
' ask); words given are searched at once.
sub firstShow()
    m.started = true
    m.discover = httpTrue(m.top.params.discover)
    m.mayAsk = reqMayAsk(reqSummaryOf(m.top.requestSummary))
    if m.discover then m.title.text = "Discover"
    showMessage(placeholder())
    query = httpString(m.top.params.query)
    if query <> ""
        m.keyboard.text = query
        search()
    end if
end sub

' What the screen says before anything is typed.
function placeholder() as string
    if m.discover then return "Type to search TMDB's movies and shows."
    if m.mayAsk then return "Type to search your libraries and TMDB."
    return "Type to search your libraries."
end function

sub onTitleChanged(change as object)
    if m.grid.visible then uiPatchTiles(m.grid.content, change)
    if m.rows.visible then uiPatchTiles(m.rows.content, change)
end sub

' A title's request changed: its badge.
sub onRequestNote()
    note = m.top.requestNote
    uiPatchRequestTiles(m.content, note)
    if m.rows.visible then uiPatchRequestTiles(m.rows.content, note)
end sub

' The person may now ask, or no longer: the results again, laid out for it.
sub onSummary()
    if not m.started or m.discover then return
    mayAsk = reqMayAsk(reqSummaryOf(m.top.requestSummary))
    if mayAsk = m.mayAsk then return
    m.mayAsk = mayAsk
    m.query = ""
    hideResults()
    showMessage(placeholder())
    search()
end sub

' Searches once typing rests for half a second.
sub onText()
    m.debounce.control = "stop"
    m.debounce.control = "start"
end sub

sub search()
    query = m.keyboard.text.Trim()
    if query = m.query then return
    m.query = query
    m.path = ""
    m.tmdbPath = ""
    m.libraryRows = invalid
    m.tmdbTiles = invalid
    m.libraryMessage = ""
    if query = ""
        hideResults()
        showMessage(placeholder())
        return
    end if
    if m.discover
        startPages()
        return
    end if
    m.path = urlWithQuery("/api/search", { q: query, limit: 40 })
    uiRequest({ method: "GET", path: m.path, build: "searchRows" }, "onResults")
    if m.mayAsk then askTmdb()
end sub

' The libraries' rows by kind, which the task made (lib/Search.brs), and
' the channels found, for the player.
sub onResults(event as object)
    request = event.getRoSGNode()
    if request.spec.path <> m.path then return
    answer = event.getData()
    m.libraryRows = []
    m.channels = []
    if not answer.ok
        if uiFailed(answer) then return
        m.libraryMessage = answer.message
    else
        m.channels = answer.data.channels
        nodes = []
        if request.content <> invalid then nodes = request.content.getChildren(-1, 0)
        keys = answer.data.rows
        for i = 0 to keys.Count() - 1
            if i < nodes.Count() then m.libraryRows.Push({ key: keys[i], node: nodes[i] })
        end for
    end if
    showRows()
end sub

' The focus on the results: Discover's grid, else the rows, else the
' keyboard.
sub focusResults()
    if m.grid.visible
        m.grid.setFocus(true)
    else if m.rows.visible
        m.rows.setFocus(true)
    else
        m.keyboard.setFocus(true)
    end if
end sub

' OK on a channel watches it; up and down in the player go through what
' the search found.
sub watch(channelId as dynamic)
    for each channel in m.channels
        if channel.id = channelId then uiNavigate({ to: "livePlayer", params: { channel: channel, category: "search:" + m.query, categoryName: "Results for “" + m.query + "”" } })
    end for
end sub

' --- On TMDB, with the libraries' titles ---

' TMDB's first titles the libraries lack, for the words searched.
sub askTmdb()
    m.tmdbPath = urlWithQuery("/api/discover/search", { q: m.query, missing: 1 })
    uiRequest({ method: "GET", path: m.tmdbPath, build: "discoverTiles", known: {} }, "onTmdb")
end sub

' TMDB's titles and See all on Discover. TMDB not answering (or failing
' otherwise) is a tile that asks again; the server not answering is the
' libraries' search's to say.
sub onTmdb(event as object)
    request = event.getRoSGNode()
    if request.spec.path <> m.tmdbPath then return
    answer = event.getData()
    tiles = []
    if answer.ok
        if request.content <> invalid then tiles = request.content.getChildren(-1, 0)
        if tiles.Count() > 0 then tiles.Push(nodesContent([reqMoreTile("See all on Discover", "", m.query)]).getChild(0))
    else if answer.error <> "network" and answer.error <> "signed_out"
        problem = reqProblem(answer, reqSummaryOf(m.top.requestSummary).manage)
        text = ""
        if problem = invalid
            text = "Couldn't load these. Select to try again."
        else if problem.retry
            text = problem.title + ". Select to try again."
        end if
        if text <> "" then tiles.Push(nodesContent([reqNoticeTile(text, "retry")]).getChild(0))
    end if
    m.tmdbTiles = tiles
    showRows()
end sub

' The rows as they have come: the libraries' by kind, then On TMDB. With
' nothing from the libraries, a line says so above TMDB's; with nothing
' from either, nothing was found. The focus stays on the tile it was on.
sub showRows()
    noneHere = m.libraryRows <> invalid and m.libraryRows.Count() = 0
    shown = []
    if m.libraryRows <> invalid then shown.Append(m.libraryRows)
    if m.mayAsk and m.tmdbTiles <> invalid and m.tmdbTiles.Count() > 0
        row = CreateObject("roSGNode", "ContentNode")
        row.title = "On TMDB"
        row.appendChildren(m.tmdbTiles)
        shown.Push({ key: "tmdb", node: row })
    end if
    line = "Nothing in your libraries matches."
    if m.libraryMessage <> "" then line = m.libraryMessage
    if shown.Count() = 0
        ' TMDB's answer alone, empty: the libraries' is waited for.
        if not noneHere then return
        hideResults()
        if m.libraryMessage = "" and (not m.mayAsk or m.tmdbTiles <> invalid) then line = "Nothing found for “" + m.query + "”."
        showMessage(line)
        return
    end if
    focusKey = ""
    focusColumn = 0
    position = m.rows.rowItemFocused
    if m.rows.isInFocusChain() and position <> invalid and position.Count() = 2 and position[0] < m.rowKeys.Count()
        focusKey = m.rowKeys[position[0]]
        focusColumn = position[1]
    end if
    content = CreateObject("roSGNode", "ContentNode")
    sizes = []
    heights = []
    m.rowKeys = []
    for each entry in shown
        content.appendChild(entry.node)
        shape = searchRowSize(entry.key)
        sizes.Push(shape.size)
        heights.Push(shape.height)
        m.rowKeys.Push(entry.key)
    end for
    m.rows.rowItemSize = sizes
    m.rows.rowHeights = heights
    m.rows.content = content
    if noneHere
        m.message.translation = [780, 170]
        m.message.text = line
        m.message.visible = true
        m.rows.translation = [780, 240]
    else
        m.message.visible = false
        m.rows.translation = [780, 170]
    end if
    m.grid.visible = false
    m.rows.visible = true
    for i = 0 to m.rowKeys.Count() - 1
        if m.rowKeys[i] = focusKey
            count = shown[i].node.getChildCount()
            if focusColumn >= count then focusColumn = count - 1
            m.rows.jumpToRowItem = [i, focusColumn]
        end if
    end for
end sub

' A tile opens its title, or Discover's search; a channel's card watches
' it; TMDB's Try again asks again.
sub onRowTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row = invalid then return
    tile = row.getChild(position[1])
    if tile = invalid then return
    if tile.kind = "retry"
        m.tmdbTiles = invalid
        askTmdb()
        return
    end if
    if tile.hasField("channelId")
        watch(tile.channelId)
        return
    end if
    uiOpenTile(tile)
end sub

' --- Discover's search ---

' TMDB's titles for the words searched, the first page in frames until it
' comes.
sub startPages()
    m.tmdbPath = urlWithQuery("/api/discover/search", { q: m.query })
    m.page = 0
    m.totalPages = -1
    m.seen = {}
    m.loading = false
    m.asked = 0
    m.content = invalid
    m.message.visible = false
    m.grid.content = nodesContent(reqSkeletonTiles())
    m.grid.visible = true
    loadPage()
end sub

sub loadPage()
    if m.loading or (m.totalPages >= 0 and m.page >= m.totalPages) then return
    m.loading = true
    m.asked = m.asked + 1
    uiRequest({ method: "GET", path: m.tmdbPath, query: { page: m.page + 1 }, build: "discoverTiles", known: m.seen }, "onPage")
end sub

' A page of titles, after those shown. A page short of a screenful isn't
' the end: the next one comes, five at most until the focus moves, then
' Look further asks for more.
sub onPage(event as object)
    request = event.getRoSGNode()
    if request.spec.path <> m.tmdbPath then return
    m.loading = false
    answer = event.getData()
    if not answer.ok
        if m.content <> invalid or uiFailed(answer) then return
        hideResults()
        problem = reqProblem(answer, reqSummaryOf(m.top.requestSummary).manage)
        if problem = invalid
            showMessage(answer.message)
        else
            showMessage((problem.title + ". " + problem.text).Trim())
        end if
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
    if request.content <> invalid
        for each node in request.content.getChildren(-1, 0)
            if not m.seen.DoesExist(node.key)
                m.seen[node.key] = true
                tiles.Push(node)
            end if
        end for
    end if
    if m.content <> invalid then dropFurther()
    if m.content = invalid and (tiles.Count() > 0 or m.page >= m.totalPages or m.asked >= 5)
        ' The first titles (or Look further) take the frames' place.
        m.content = CreateObject("roSGNode", "ContentNode")
        m.content.appendChildren(tiles)
        m.grid.content = m.content
    else if m.content <> invalid and tiles.Count() > 0
        m.content.appendChildren(tiles)
    end if
    if m.content <> invalid and m.content.getChildCount() = 0 and m.page >= m.totalPages
        hideResults()
        showMessage("Nothing found for “" + m.query + "”.")
        return
    end if
    count = 0
    if m.content <> invalid then count = m.content.getChildCount()
    if reqWantsPage(m.page, m.totalPages, m.grid.itemFocused, count, 10, m.asked)
        loadPage()
    else if m.content <> invalid and m.page < m.totalPages and m.asked >= 5
        nodesAppend(m.content, [reqFurtherTile()])
    end if
end sub

' Look further goes as the next page comes.
sub dropFurther()
    last = m.content.getChildCount() - 1
    if last >= 0 and m.content.getChild(last).kind = "further" then m.content.removeChildIndex(last)
end sub

' Two rows before the end, the next page comes.
sub onFocused(event as object)
    if not m.discover or m.content = invalid then return
    m.asked = 0
    if event.getData() >= m.content.getChildCount() - 10 then loadPage()
end sub

' --- Results ---

' The keyboard takes the focus before the results it was on go.
sub hideResults()
    if m.grid.isInFocusChain() or m.rows.isInFocusChain() then m.keyboard.setFocus(true)
    m.grid.visible = false
    m.rows.visible = false
    m.channels = []
end sub

sub showMessage(text as string)
    m.message.translation = [780, 200]
    m.message.text = text
    m.message.visible = true
end sub

sub onSelected(event as object)
    tile = m.grid.content.getChild(event.getData())
    if tile <> invalid and tile.kind = "further"
        m.asked = 0
        loadPage()
        return
    end if
    uiOpenTile(tile)
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "right" and m.keyboard.isInFocusChain() and (m.grid.visible or m.rows.visible)
        focusResults()
        return true
    end if
    if (key = "left" or key = "back") and (m.grid.isInFocusChain() or m.rows.isInFocusChain())
        m.keyboard.setFocus(true)
        return true
    end if
    if key = "options" and m.grid.isInFocusChain()
        uiTileOptions(m.grid.content.getChild(m.grid.itemFocused), false)
        return true
    end if
    if key = "options" and m.rows.isInFocusChain()
        position = m.rows.rowItemFocused
        row = m.rows.content.getChild(position[0])
        if row <> invalid then uiTileOptions(row.getChild(position[1]), false)
        return true
    end if
    return false
end function
