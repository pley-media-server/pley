' Search with the on-screen keyboard, once typing rests for half a second.
' Before anything is typed, the recent searches: the last 10 whose results
' were opened, kept on the Roku for each server and profile; OK searches
' one again (and moves it to the top), Clear forgets them.
' What the libraries have (GET /api/search) comes in rows by kind, as on
' the other apps (lib/Search.brs): Movies, Shows, Seasons, Episodes,
' Artists, Albums, Tracks, Live TV (cards; OK watches the channel), People.
' Where the person may ask for titles (the scene's summary), Discover (GET
' /api/discover/search?missing=1), asked at the same time and never waited
' for, comes after them, ending with See all: all of TMDB's results for
' the words, with their badges (DiscoverListScreen).

sub init()
    m.keyboard = m.top.findNode("keyboard")
    m.rows = m.top.findNode("rows")
    ' The Live TV channels found, for the player.
    m.channels = []
    m.message = m.top.findNode("message")
    m.debounce = m.top.findNode("debounce")
    m.recentTitle = m.top.findNode("recentTitle")
    m.recentList = m.top.findNode("recent")
    ' The recent searches, newest first, and where they are kept.
    m.recent = []
    m.recentKey = ""
    m.started = false
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
    m.lastFocus = m.keyboard
    m.keyboard.observeField("text", "onText")
    m.debounce.observeField("fire", "search")
    m.rows.observeField("rowItemSelected", "onRowTile")
    m.recentList.observeField("itemSelected", "onRecent")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestNote", "onRequestNote")
end sub

sub onActive()
    if not m.top.active
        m.lastFocus = m.keyboard
        if m.rows.isInFocusChain() then m.lastFocus = m.rows
        if m.recentList.isInFocusChain() then m.lastFocus = m.recentList
        return
    end if
    if not m.started then firstShow()
    m.lastFocus.setFocus(true)
    for each change in uiChangesSince()
        onTitleChanged(change)
    end for
end sub

' The libraries' search, with TMDB's where the person may ask.
sub firstShow()
    m.started = true
    m.mayAsk = reqMayAsk(reqSummaryOf(m.top.requestSummary))
    session = m.global.session
    m.recentKey = searchRecentKey(session.server, session.profileId)
    m.recent = searchRecentLoad(recentSection(), m.recentKey)
    showIdle()
end sub

' What the screen says before anything is typed.
function placeholder() as string
    return "Type to search your libraries."
end function

' --- Recent searches ---

function recentSection() as object
    return CreateObject("roRegistrySection", "searches")
end function

' Before anything is typed: the recent searches, else what to type.
sub showIdle()
    if m.recent.Count() = 0
        hideRecent()
        showMessage(placeholder())
        return
    end if
    items = []
    for each query in m.recent
        items.Push({ title: query })
    end for
    items.Push({ title: "Clear" })
    m.recentList.content = nodesContent(items)
    m.message.visible = false
    m.recentTitle.visible = true
    m.recentList.visible = true
end sub

' The keyboard takes the focus before the list it was on goes.
sub hideRecent()
    if m.recentList.isInFocusChain() then m.keyboard.setFocus(true)
    m.recentTitle.visible = false
    m.recentList.visible = false
end sub

' The words searched, as one of the recent searches: one of their results
' was wanted.
sub remember()
    if m.query = "" then return
    m.recent = searchRemember(m.recent, m.query)
    searchRecentSave(recentSection(), m.recentKey, m.recent)
end sub

' A recent search is searched again, at once, and moves to the top; Clear
' forgets them all. The focus goes back to the keyboard.
sub onRecent(event as object)
    at = event.getData()
    m.keyboard.setFocus(true)
    if at >= 0 and at < m.recent.Count()
        query = m.recent[at]
        m.recent = searchRemember(m.recent, query)
        searchRecentSave(recentSection(), m.recentKey, m.recent)
        m.keyboard.text = query
        ' Typing goes on after the words, not before them.
        if m.keyboard.textEditBox <> invalid then m.keyboard.textEditBox.cursorPosition = Len(query)
        m.debounce.control = "stop"
        search()
        return
    end if
    m.recent = []
    searchRecentSave(recentSection(), m.recentKey, m.recent)
    showIdle()
end sub

sub onTitleChanged(change as object)
    if m.rows.visible then uiPatchTiles(m.rows.content, change)
end sub

' A title's request changed: its badge.
sub onRequestNote()
    if m.rows.visible then uiPatchRequestTiles(m.rows.content, m.top.requestNote)
end sub

' The person may now ask, or no longer: the results again, laid out for it.
sub onSummary()
    if not m.started then return
    mayAsk = reqMayAsk(reqSummaryOf(m.top.requestSummary))
    if mayAsk = m.mayAsk then return
    m.mayAsk = mayAsk
    m.query = ""
    hideResults()
    showIdle()
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
        showIdle()
        return
    end if
    hideRecent()
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

' The focus on the results, else the keyboard.
sub focusResults()
    if m.rows.visible
        m.rows.setFocus(true)
    else
        m.keyboard.setFocus(true)
    end if
end sub

' OK on a channel watches it; up and down in the player go through what
' the search found.
sub watch(channelId as dynamic)
    remember()
    for each channel in m.channels
        if channel.id = channelId then uiNavigate({ to: "livePlayer", params: { channel: channel, category: "search:" + m.query, categoryName: "Results for “" + m.query + "”" } })
    end for
end sub

' --- Discover, with the libraries' titles ---

' TMDB's first titles the libraries lack, for the words searched (as many
' as TMDB's search takes).
sub askTmdb()
    m.tmdbPath = urlWithQuery("/api/discover/search", { q: reqSearchWords(m.query), missing: 1 })
    uiRequest({ method: "GET", path: m.tmdbPath, build: "discoverTiles", known: {} }, "onTmdb")
end sub

' TMDB's titles and See all. What stops TMDB's search
' (TMDB not answering, its key refused, no access) is a tile that says
' so, asking again where that may help; the server not answering is the
' libraries' search's to say.
sub onTmdb(event as object)
    request = event.getRoSGNode()
    if request.spec.path <> m.tmdbPath then return
    answer = event.getData()
    tiles = []
    if answer.ok
        if request.content <> invalid then tiles = request.content.getChildren(-1, 0)
        if tiles.Count() > 0 then tiles.Push(nodesContent([reqMoreTile("See all", "", m.query)]).getChild(0))
    else if answer.error <> "network" and answer.error <> "signed_out"
        tiles.Push(nodesContent([reqSearchProblemTile(answer, reqSummaryOf(m.top.requestSummary).manage)]).getChild(0))
    end if
    m.tmdbTiles = tiles
    showRows()
end sub

' The rows as they have come: the libraries' by kind, then Discover. With
' nothing from the libraries, a line says so above TMDB's; with nothing
' from either, nothing was found. The focus stays on the tile it was on.
sub showRows()
    noneHere = m.libraryRows <> invalid and m.libraryRows.Count() = 0
    shown = []
    if m.libraryRows <> invalid then shown.Append(m.libraryRows)
    if m.mayAsk and m.tmdbTiles <> invalid and m.tmdbTiles.Count() > 0
        row = CreateObject("roSGNode", "ContentNode")
        row.title = "Discover"
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
    m.rows.visible = true
    for i = 0 to m.rowKeys.Count() - 1
        if m.rowKeys[i] = focusKey
            count = shown[i].node.getChildCount()
            if focusColumn >= count then focusColumn = count - 1
            m.rows.jumpToRowItem = [i, focusColumn]
        end if
    end for
end sub

' A tile opens its title, or all of TMDB's results; a channel's card
' watches it; TMDB's Try again asks again. The words searched join the
' recent searches.
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
    remember()
    uiOpenTile(tile)
end sub

' --- Results ---

' The keyboard takes the focus before the results it was on go.
sub hideResults()
    if m.rows.isInFocusChain() then m.keyboard.setFocus(true)
    m.rows.visible = false
    m.channels = []
end sub

sub showMessage(text as string)
    m.message.translation = [780, 200]
    m.message.text = text
    m.message.visible = true
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "right" and m.keyboard.isInFocusChain() and m.recentList.visible
        m.recentList.setFocus(true)
        return true
    end if
    if key = "right" and m.keyboard.isInFocusChain() and m.rows.visible
        focusResults()
        return true
    end if
    if (key = "left" or key = "back") and (m.rows.isInFocusChain() or m.recentList.isInFocusChain())
        m.keyboard.setFocus(true)
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
