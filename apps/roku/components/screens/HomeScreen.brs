sub init()
    m.menu = m.top.findNode("menu")
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.messageTitle = m.top.findNode("messageTitle")
    m.messageText = m.top.findNode("messageText")
    m.backdrop = m.top.findNode("backdrop")
    m.hero = m.top.findNode("hero")
    m.heroTitle = m.top.findNode("heroTitle")
    m.heroContext = m.top.findNode("heroContext")
    m.heroMeta = m.top.findNode("heroMeta")
    m.heroLeft = m.top.findNode("heroLeft")
    m.heroSummary = m.top.findNode("heroSummary")
    m.heroButtons = m.top.findNode("heroButtons")
    m.info = m.top.findNode("info")
    m.infoTitle = m.top.findNode("infoTitle")
    m.infoMeta = m.top.findNode("infoMeta")
    m.infoSummary = m.top.findNode("infoSummary")
    m.rest = m.top.findNode("rest")
    m.started = false
    m.libraries = []
    ' GET /api/libraries' answer, for why Home is empty; invalid until it
    ' comes.
    m.librariesAnswer = invalid
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
    ' The hero (homeInfo), its buttons' actions, and what is known of the
    ' titles focus rested on ({ itemId: homeInfo }, their details once
    ' asked for), the one the info block shows (0 for none) and the one
    ' the rows' focus is on.
    m.heroInfo = invalid
    m.heroActions = []
    m.heroLabels = []
    m.infos = {}
    m.infoShown = 0
    m.restingOn = 0
    ' Loads so far: details asked for before the last one are asked again.
    m.loads = 0
    ' Whether the message says why Home is empty.
    m.emptyShown = false
    ' Whether the focus was put in the rows (the page laid out for them).
    m.inRows = false
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
    m.rows.observeField("rowItemFocused", "onTileFocused")
    m.heroButtons.observeField("chosen", "onHeroButton")
    m.rest.observeField("fire", "onRested")
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
        if m.heroButtons.isInFocusChain() then m.lastFocus = m.heroButtons
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
    if m.lastFocus.isSameNode(m.heroButtons) and not m.hero.visible then m.lastFocus = m.menu
    if m.rows.visible
        m.lastFocus.setFocus(true)
        m.inRows = m.lastFocus.isSameNode(m.rows)
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
    ' What is known of the titles may have moved on (watched, played):
    ' it shows until their details come again.
    m.loads = m.loads + 1
    if m.rowKeys.Count() = 0 then showMessage("", "Loading…")
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
    m.librariesAnswer = answer.data
    ' Home empty meanwhile says why, now that it is known.
    if m.emptyShown then showEmpty()
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
        if m.rowKeys.Count() = 0 and not uiFailed(answer) then showMessage("", answer.message)
        return
    end if
    m.top.getScene().myList = answer.data.myList
    rows = answer.data.rows
    if rows.Count() = 0
        m.rowKeys = []
        m.rowSigs = {}
        m.heroInfo = invalid
        showEmpty()
        return
    end if
    firstShown = not m.rows.visible
    if not place(rows, request.content)
        ' Rows it didn't build that aren't shown either: all of them again.
        m.rowKeys = []
        m.rowSigs = {}
        load()
        return
    end if
    showHero(answer.data.hero)
    ' The page takes the focus as it first shows: the hero's first button,
    ' as on the other TVs, else the rows.
    if firstShown and m.top.isInFocusChain() then focusPage()
end sub

' No rows: why, as on Android TV (homeEmpty).
sub showEmpty()
    empty = homeEmpty(m.librariesAnswer)
    showMessage(empty.title, empty.text)
    m.emptyShown = true
end sub

' The menu, in the other TVs' order (homeMenu); Discover and the requests
' (Your requests, a manager's Requests, with their number) where the
' summary has them.
sub buildMenu()
    m.requestEntries = requestEntries()
    m.musicShown = m.global.music.state <> "stopped"
    entries = homeMenu(m.libraries, m.live, m.requestEntries, m.musicShown)
    items = []
    m.menuActions = []
    ' The library of each entry (invalid for the others).
    m.menuLibraries = []
    for each entry in entries
        items.Push(entry)
        m.menuActions.Push(entry.action)
        m.menuLibraries.Push(entry.library)
    end for
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
    ' Home's entry, before anything was focused: the page shown.
    if focusedKey = "" and focused <= 0 then focusedKey = "home"
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

sub showMessage(title as string, text as string)
    m.emptyShown = false
    m.inRows = false
    if m.rows.isInFocusChain() or m.heroButtons.isInFocusChain() then m.menu.setFocus(true)
    m.rows.visible = false
    m.hero.visible = false
    m.info.visible = false
    m.backdrop.uri = ""
    m.messageTitle.text = title
    m.messageText.text = text
    m.message.removeChildrenIndex(m.message.getChildCount(), 0)
    if title <> "" then m.message.appendChild(m.messageTitle)
    m.message.appendChild(m.messageText)
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
    m.message.visible = false
    m.rows.visible = true
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

' --- The hero and the info block ---

' The hero for the title the rows have it show (homeHeroItem). What its
' details said before stays until they come again, so a new answer for the
' rows never draws it without them for a moment.
sub showHero(hero as dynamic)
    if hero = invalid
        m.heroInfo = invalid
        if m.heroButtons.isInFocusChain()
            m.rows.setFocus(true)
            m.inRows = true
        end if
        placeRows()
        return
    end if
    known = m.infos[urlValue(hero.itemId)]
    if known <> invalid and known.complete
        hero.meta = known.meta
        hero.summary = known.summary
    end if
    m.heroInfo = hero
    drawHero()
    if known = invalid or known.loads <> m.loads then askInfo(hero.itemId)
    placeRows()
end sub

sub drawHero()
    info = m.heroInfo
    if info = invalid then return
    m.heroTitle.text = info.title
    m.heroContext.text = info.context
    m.heroMeta.text = info.meta
    m.heroLeft.text = info.timeLeft
    m.heroSummary.text = info.summary
    buttons = homeHeroButtons(info)
    m.heroActions = buttons.actions
    if FormatJson(buttons.labels) <> FormatJson(m.heroLabels)
        m.heroLabels = buttons.labels
        chips = []
        for each label in buttons.labels
            chips.Push({ label: label, width: 0 })
        end for
        m.heroButtons.chips = chips
    end if
    layOut(m.hero, [{ label: m.heroTitle, gap: 0 }, { label: m.heroContext, gap: 8 }, { label: m.heroMeta, gap: 12 }, { label: m.heroLeft, gap: 8 }, { label: m.heroSummary, gap: 16 }], m.heroButtons, 32)
    if not m.inRows then setBackdrop(info.backdrop)
end sub

' Only the labels with text stay in a column, each after its gap; then
' its last child (the hero's buttons).
sub layOut(column as object, parts as object, last as dynamic, lastGap as integer)
    shown = []
    spacings = []
    for each part in parts
        if part.label.text <> ""
            if shown.Count() > 0 then spacings.Push(part.gap)
            shown.Push(part.label)
        end if
    end for
    ' Taken out and put back, the buttons would lose the focus.
    focused = false
    if last <> invalid
        spacings.Push(lastGap)
        shown.Push(last)
        focused = last.isInFocusChain()
    end if
    column.removeChildrenIndex(column.getChildCount(), 0)
    column.appendChildren(shown)
    column.itemSpacings = spacings
    if focused then last.setFocus(true)
end sub

' The scene's agent as each backdrop is asked for, as the other screens
' do: a new token makes the scene a new one.
sub setBackdrop(uri as string)
    if m.backdrop.uri = uri then return
    m.backdrop.setHttpAgent(m.top.getScene().getHttpAgent())
    m.backdrop.uri = uri
end sub

' At rest the hero shows, the rows under it; in the rows, the info block
' takes its place and the rows come up under it.
sub placeRows()
    ' Where the focus was put, not where it is: the ready-to-watch card may
    ' hold it for a while, and gives it back.
    inRows = m.inRows or m.heroInfo = invalid
    m.hero.visible = m.heroInfo <> invalid and not inRows
    if inRows
        m.rows.translation = [440, 330]
    else
        m.rows.translation = [440, 620]
        m.info.visible = false
        m.infoShown = 0
        if m.heroInfo <> invalid then setBackdrop(m.heroInfo.backdrop)
    end if
end sub

' The details of a title, as Home says them (the task reduces them).
sub askInfo(itemId as dynamic)
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(itemId), build: "homeInfo", itemId: itemId, loads: m.loads }, "onInfo")
end sub

sub onInfo(event as object)
    answer = event.getData()
    if not answer.ok or GetInterface(answer.data, "ifAssociativeArray") = invalid then return
    info = answer.data
    info.loads = event.getRoSGNode().spec.loads
    ' What is kept stays small: the titles of a few rows.
    if m.infos.Count() >= 100 then m.infos = {}
    m.infos[urlValue(info.itemId)] = info
    if m.heroInfo <> invalid and m.heroInfo.itemId = info.itemId
        m.heroInfo = info
        drawHero()
    end if
    if m.infoShown = info.itemId and m.restingOn = info.itemId then drawInfo(info)
end sub

' Focus moved in the rows: once it rests 0.4 s on a title, the info block
' and the backdrop follow it, as on the other TVs.
sub onTileFocused()
    tile = focusedNode()
    m.restingOn = 0
    if tile <> invalid then m.restingOn = tile.itemId
    m.rest.control = "stop"
    if m.restingOn > 0 and m.rows.isInFocusChain() then m.rest.control = "start"
end sub

sub onRested()
    if m.restingOn = 0 or not m.rows.isInFocusChain() then return
    tile = focusedNode()
    if tile = invalid or tile.itemId <> m.restingOn then return
    known = m.infos[urlValue(tile.itemId)]
    if known = invalid or known.loads <> m.loads then askInfo(tile.itemId)
    if known = invalid
        ' What the tile knows, until its details come.
        known = { itemId: tile.itemId, title: tile.title, context: "", meta: tile.subtitle, summary: "", backdrop: "", complete: false }
    end if
    m.infoShown = tile.itemId
    drawInfo(known)
end sub

sub drawInfo(info as object)
    m.infoTitle.text = info.title
    meta = info.meta
    if info.context <> "" and meta <> ""
        meta = info.context + " · " + meta
    else if info.context <> ""
        meta = info.context
    end if
    m.infoMeta.text = meta
    m.infoSummary.text = info.summary
    layOut(m.info, [{ label: m.infoTitle, gap: 0 }, { label: m.infoMeta, gap: 8 }, { label: m.infoSummary, gap: 12 }], invalid, 0)
    m.info.visible = true
    if info.backdrop <> "" then setBackdrop(info.backdrop)
end sub

' The tile focus is on in the rows, or invalid.
function focusedNode() as dynamic
    if m.rows.content = invalid then return invalid
    position = m.rows.rowItemFocused
    if position = invalid or position.Count() < 2 then return invalid
    row = m.rows.content.getChild(position[0])
    if row = invalid then return invalid
    tile = row.getChild(position[1])
    if tile = invalid or not tile.hasField("itemId") or tile.itemId = 0 then return invalid
    return tile
end function

' Resume or Play plays a movie or an episode (From start, from its
' start); Info, and Open for anything else, opens its page; the watched
' mark toggles, and Home asks for its rows again.
sub onHeroButton(event as object)
    info = m.heroInfo
    if info = invalid then return
    at = event.getData()
    if at < 0 or at >= m.heroActions.Count() then return
    action = m.heroActions[at]
    if action = "resume"
        uiNavigate({ to: "player", params: { itemId: info.itemId, startMs: Int(info.positionMs) } })
    else if action = "start"
        uiNavigate({ to: "player", params: { itemId: info.itemId, startMs: 0 } })
    else if action = "info"
        uiOpenTile({ itemId: info.itemId, kind: info.kind, title: info.title })
    else
        uiTitleAction({ itemId: info.itemId, kind: info.kind, parentId: 0, grandparentId: 0 }, action)
    end if
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
        focusPage()
    else if action = "live"
        uiNavigate({ to: "live" })
    else if action = "library"
        uiNavigate({ to: "library", params: { library: m.menuLibraries[index] } })
    else if action = "playlists"
        uiNavigate({ to: "grid", params: { kind: "playlists", id: 0, title: "Playlists" } })
    else if action = "collections"
        uiNavigate({ to: "collections", params: { libraries: m.libraries } })
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

' From the menu into the page: the hero's buttons, else the rows.
sub focusPage()
    if m.hero.visible
        m.heroButtons.setFocus(true)
        m.inRows = false
    else if m.rows.visible
        m.rows.setFocus(true)
        m.inRows = true
    end if
end sub

' Down from the hero goes to the rows, Up from their first row back to
' it; Back from the rows puts the page back at its hero, and Left or Back
' goes to the menu, as on the other TVs.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    ' The focus given back to the screen itself (what had it went): the
    ' page takes it again, as it was laid out.
    if m.top.hasFocus()
        if m.inRows and m.rows.visible
            m.rows.setFocus(true)
        else if m.hero.visible
            m.heroButtons.setFocus(true)
        else
            m.menu.setFocus(true)
        end if
        return true
    end if
    if m.rows.isInFocusChain()
        if key = "up" and m.heroInfo <> invalid
            m.heroButtons.setFocus(true)
            m.inRows = false
            placeRows()
            return true
        end if
        if key = "left" or key = "back"
            m.rest.control = "stop"
            m.menu.setFocus(true)
            m.inRows = false
            placeRows()
            return true
        end if
        if key = "options"
            position = m.rows.rowItemFocused
            row = m.rows.content.getChild(position[0])
            if row <> invalid then uiTileOptions(row.getChild(position[1]), contentContinueWatching(m.rowKeys[position[0]]))
            return true
        end if
        return false
    end if
    if m.heroButtons.isInFocusChain()
        if key = "down" and m.rows.visible
            m.rows.setFocus(true)
            m.inRows = true
            placeRows()
            onTileFocused()
            return true
        end if
        if key = "left" or key = "back"
            m.menu.setFocus(true)
            return true
        end if
        return false
    end if
    if key = "right" and m.menu.isInFocusChain()
        focusPage()
        return true
    end if
    return false
end function
