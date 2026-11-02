sub init()
    m.menu = m.top.findNode("menu")
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.started = false
    m.libraries = []
    ' Whether the server has Live TV for this profile (GET /api/live).
    m.live = false
    ' What the menu was built from, to build it again only when it changes.
    m.librariesShown = ""
    ' The rows shown: their hub keys in order, and { key: signature } (the
    ' task builds again only rows whose signature changed).
    m.rowKeys = []
    m.rowSigs = {}
    ' Which asking for the rows the answer must be for.
    m.rowsAsked = 0
    ' Playbacks at the last load (one more, and watch state moved on), and
    ' how long ago it was.
    m.playedSeen = 0
    m.loadedAgo = CreateObject("roTimespan")
    ' Orders saved at the last load: the menu and the rows follow a new one.
    m.movedSeen = 0
    m.lastFocus = m.menu
    ' How many profiles the household has is asked once, for "Who's
    ' watching?" at the next start.
    m.householdAsked = false
    m.menu.observeField("itemSelected", "onMenu")
    m.rows.observeField("rowItemSelected", "onTile")
    ' Changes told by the server's event stream at the last load.
    m.remoteSeen = 0
    m.top.observeField("active", "onActive")
    m.top.observeField("refresh", "onRefresh")
    buildMenu()
end sub

sub onActive()
    if not m.top.active
        ' Covered by another screen: focus comes back to the same place.
        m.lastFocus = m.menu
        if m.rows.isInFocusChain() then m.lastFocus = m.rows
        return
    end if
    if not m.started
        m.started = true
        uiChangesSince()
        m.menu.setFocus(true)
        load()
        return
    end if
    ' Music started or ended meanwhile: Now Playing in the menu or not.
    if m.musicShown <> (m.global.music.state <> "stopped") then buildMenu()
    if m.rows.visible
        m.lastFocus.setFocus(true)
    else
        m.menu.setFocus(true)
    end if
    ' Back from playing, Continue Watching and the watched marks moved on;
    ' a minute on, other devices may have moved them; sorted, the libraries
    ' and their rows come in their new order.
    ' Titles marked watched or put in My List elsewhere too.
    scene = m.top.getScene()
    changed = uiChangesSince().Count() > 0
    if changed or contentHomeStale(scene.played, m.playedSeen, m.loadedAgo.TotalSeconds()) or scene.librariesMoved <> m.movedSeen or scene.remoteChanges <> m.remoteSeen then load()
end sub

' A title marked watched, put in My List or taken out of Continue Watching
' here: the rows follow.
sub onTitleChanged(change as object)
    if change.count > 0 then load()
end sub

' The libraries (the menu) and the rows, asked for together; the task
' makes the ContentNodes of the rows that changed.
' Another device changed what Home shows, while it shows.
sub onRefresh()
    if m.top.active and m.started then load()
end sub

sub load()
    m.playedSeen = m.top.getScene().played
    m.movedSeen = m.top.getScene().librariesMoved
    m.remoteSeen = m.top.getScene().remoteChanges
    m.loadedAgo.Mark()
    if m.rowKeys.Count() = 0 then showMessage("Loading…")
    uiGet("/api/libraries", "onLibraries")
    uiGet("/api/live", "onLive")
    if not m.householdAsked and authAllows(m.global.session.scope, "profiles")
        m.householdAsked = true
        uiGet("/api/oauth/profiles", "onHousehold")
    end if
    m.rowsAsked = m.rowsAsked + 1
    uiRequest({ method: "GET", path: "/api/hubs/home", build: "rows", known: m.rowSigs, asked: m.rowsAsked }, "onRows")
end sub

sub onHousehold(event as object)
    answer = event.getData()
    if answer.ok and GetInterface(answer.data, "ifArray") <> invalid then profKeepHousehold(CreateObject("roRegistrySection", "household"), m.global.session.server, answer.data)
end sub

' Live TV in the menu while the server has channels this profile may
' watch; an older server (404) has none.
sub onLive(event as object)
    answer = event.getData()
    live = false
    if answer.ok and httpObject(answer.data) <> invalid then live = httpTrue(answer.data.available)
    if not answer.ok and answer.status <> 404 and answer.status <> 403 then return
    if live = m.live then return
    m.live = live
    buildMenu()
end sub

sub onLibraries(event as object)
    answer = event.getData()
    if not answer.ok then return
    libraries = contentLibraries(answer.data)
    shown = FormatJson(libraries)
    if shown = m.librariesShown then return
    m.librariesShown = shown
    m.libraries = libraries
    buildMenu()
end sub

sub onRows(event as object)
    request = event.getRoSGNode()
    if request.spec.asked <> m.rowsAsked then return
    answer = event.getData()
    if not answer.ok
        ' A refresh that fails leaves the rows as they were.
        if m.rowKeys.Count() = 0 and not uiFailed(answer) then showMessage(answer.message)
        return
    end if
    m.top.getScene().myList = answer.data.myList
    rows = answer.data.rows
    if rows.Count() = 0
        m.rowKeys = []
        m.rowSigs = {}
        showMessage("Nothing to watch yet. Add a library on the server, then come back.")
        return
    end if
    if not place(rows, request.content)
        ' Rows it didn't build that aren't shown either: all of them again.
        m.rowKeys = []
        m.rowSigs = {}
        load()
    end if
end sub

' The menu: Home, Live TV where the server has it, the libraries,
' Playlists, Search, Discover and the requests (Your requests, a
' manager's Requests, with their number) where the summary has them,
' Settings, and Now Playing while music plays.
sub buildMenu()
    items = [{ title: "Home" }]
    m.menuActions = ["home"]
    ' The library of each entry (invalid for the others).
    m.menuLibraries = [invalid]
    if m.live
        items.Push({ title: "Live TV" })
        m.menuActions.Push("live")
        m.menuLibraries.Push(invalid)
    end if
    for each library in m.libraries
        items.Push({ title: library.name })
        m.menuActions.Push("library")
        m.menuLibraries.Push(library)
    end for
    for each entry in [["Playlists", "playlists"], ["Search", "search"]]
        items.Push({ title: entry[0] })
        m.menuActions.Push(entry[1])
    end for
    m.requestEntries = requestEntries()
    for each entry in m.requestEntries
        items.Push(entry)
        m.menuActions.Push(entry.action)
    end for
    items.Push({ title: "Settings" })
    m.menuActions.Push("settings")
    m.musicShown = m.global.music.state <> "stopped"
    if m.musicShown
        items.Push({ title: "Now playing" })
        m.menuActions.Push("nowPlaying")
    end if
    ' The focus stays on its entry as entries come or go above it (the
    ' requests' once the summary comes, Live TV, libraries).
    focusedKey = ""
    focused = m.menu.itemFocused
    if m.menuKeys <> invalid and focused >= 0 and focused < m.menuKeys.Count() then focusedKey = m.menuKeys[focused]
    m.menuKeys = []
    for i = 0 to m.menuActions.Count() - 1
        key = m.menuActions[i]
        if key = "library" then key = key + ":" + urlValue(m.menuLibraries[i].id)
        m.menuKeys.Push(key)
    end for
    m.menu.content = nodesContent(items)
    at = navMenuFocus(m.menuKeys, focusedKey, focused)
    if at > 0 then m.menu.jumpToItem = at
end sub

' The menu's entries for requests, from the summary: Discover, and the
' requests with their number.
function requestEntries() as object
    summary = reqSummaryOf(m.top.requestSummary)
    entries = []
    if reqShowsDiscover(summary) then entries.Push({ title: "Discover", action: "discover" })
    if reqShowsRequests(summary)
        badge = reqBadge(summary)
        entries.Push({ title: reqPageName(summary), action: "requests", count: badge.count, danger: badge.danger })
    end if
    return entries
end function

' The summary changed: the menu follows when its entries do.
sub onRequestSummary()
    if m.requestEntries = invalid then return
    if FormatJson(requestEntries()) <> FormatJson(m.requestEntries) then buildMenu()
end sub

sub showMessage(text as string)
    if m.rows.isInFocusChain() then m.menu.setFocus(true)
    m.rows.visible = false
    m.message.text = text
    m.message.visible = true
    if text <> "Loading…" then uiLaunched()
end sub

' Puts rows ([{ key, wide, sig, built }]) on the screen: the
' ones the task built (built, in order), the others as they are. When the
' rows are the same ones in the same order, changed rows are swapped one by
' one and the others keep where they were scrolled to. The focus stays on
' the same row, found by its key, and the same title in it. false when a
' row is neither built nor shown.
function place(rows as object, built as dynamic) as boolean
    focus = focusedTile()
    fresh = []
    if built <> invalid then fresh = built.getChildren(-1, 0)
    keys = []
    for each row in rows
        keys.Push(row.key)
    end for
    content = m.rows.content
    if content <> invalid and FormatJson(keys) = FormatJson(m.rowKeys)
        taken = 0
        for i = 0 to rows.Count() - 1
            if rows[i].built
                if taken >= fresh.Count() then return false
                content.replaceChild(fresh[taken], i)
                taken = taken + 1
            end if
        end for
    else
        shown = {}
        if content <> invalid
            for i = 0 to m.rowKeys.Count() - 1
                shown[m.rowKeys[i]] = content.getChild(i)
            end for
        end if
        nodes = []
        taken = 0
        for each row in rows
            if row.built
                if taken >= fresh.Count() then return false
                nodes.Push(fresh[taken])
                taken = taken + 1
            else if shown.DoesExist(row.key)
                nodes.Push(shown[row.key])
            else
                return false
            end if
        end for
        sizes = []
        heights = []
        for each row in rows
            shape = contentRowSize(row.wide, row.square)
            sizes.Push(shape.size)
            heights.Push(shape.height)
        end for
        content = CreateObject("roSGNode", "ContentNode")
        content.appendChildren(nodes)
        m.rows.rowItemSize = sizes
        m.rows.rowHeights = heights
        m.rows.content = content
    end if
    m.rowKeys = keys
    m.rowSigs = {}
    for each row in rows
        m.rowSigs[row.key] = row.sig
    end for
    refocus(focus, rows)
    wasShown = m.rows.visible
    m.message.visible = false
    m.rows.visible = true
    if not wasShown and m.top.isInFocusChain() then m.rows.setFocus(true)
    uiLaunched()
    return true
end function

' The tile the rows' focus is on: { key, itemId, column }, or invalid.
function focusedTile() as dynamic
    if m.rows.content = invalid or m.rowKeys.Count() = 0 then return invalid
    position = m.rows.rowItemFocused
    if position = invalid or position.Count() < 2 then return invalid
    if position[0] < 0 or position[0] >= m.rowKeys.Count() then return invalid
    itemId = 0
    row = m.rows.content.getChild(position[0])
    if row <> invalid
        tile = row.getChild(position[1])
        if tile <> invalid then itemId = tile.itemId
    end if
    return { key: m.rowKeys[position[0]], itemId: itemId, column: position[1] }
end function

sub refocus(focus as dynamic, rows as object)
    if focus = invalid then return
    for i = 0 to rows.Count() - 1
        if rows[i].key = focus.key
            column = focus.column
            row = m.rows.content.getChild(i)
            for j = 0 to row.getChildCount() - 1
                if row.getChild(j).itemId = focus.itemId then column = j
            end for
            if column >= row.getChildCount() then column = row.getChildCount() - 1
            if column < 0 then column = 0
            m.rows.jumpToRowItem = [i, column]
            return
        end if
    end for
end sub

' A title opens; See all, its library on the Library tab in the row's
' order.
sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row = invalid then return
    tile = row.getChild(position[1])
    if tile <> invalid and tile.kind = "more" and tile.hasField("sort")
        for each library in m.libraries
            if library.id = tile.libraryId then uiNavigate({ to: "library", params: { library: library, sort: tile.sort } })
        end for
        return
    end if
    uiOpenTile(tile)
end sub

sub onMenu(event as object)
    index = event.getData()
    action = m.menuActions[index]
    if action = "home"
        load()
        if m.rows.visible then m.rows.setFocus(true)
    else if action = "live"
        uiNavigate({ to: "live" })
    else if action = "library"
        uiNavigate({ to: "library", params: { library: m.menuLibraries[index] } })
    else if action = "playlists"
        uiNavigate({ to: "grid", params: { kind: "playlists", id: 0, title: "Playlists" } })
    else if action = "search"
        uiNavigate({ to: "search" })
    else if action = "discover"
        uiNavigate({ to: "discover" })
    else if action = "requests"
        uiNavigate({ to: "requests" })
    else if action = "settings"
        uiNavigate({ to: "settings" })
    else if m.global.music.state = "stopped"
        ' The music ended meanwhile.
        buildMenu()
    else
        uiNavigate({ to: "nowPlaying" })
    end if
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if (key = "left" or key = "back") and m.rows.isInFocusChain()
        m.menu.setFocus(true)
        return true
    end if
    if key = "right" and m.menu.isInFocusChain() and m.rows.visible
        m.rows.setFocus(true)
        return true
    end if
    if key = "options" and m.rows.isInFocusChain()
        position = m.rows.rowItemFocused
        row = m.rows.content.getChild(position[0])
        if row <> invalid then uiTileOptions(row.getChild(position[1]), contentContinueWatching(m.rowKeys[position[0]]))
        return true
    end if
    return false
end function
