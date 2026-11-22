' A collection (GET /api/collections/{id}, its library's name from GET
' /api/libraries). Play plays a collection of movies in its order, Up Next
' bringing on each next, and Shuffle in any order from any of them; a
' collection of shows plays the first show's next episode (GET
' /api/items/{id}/on-deck), Shuffle a random show's. Shuffle needs two
' titles or more. A Home row of several collections of one title (params
' ids) is GET /api/collections/merged: their titles together, played the
' same way, and a button for each library's own collection. params: { id,
' or ids, title (shown until the server answers) }.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.page = m.top.findNode("page")
    m.column = m.top.findNode("column")
    m.text = m.top.findNode("text")
    m.eyebrow = m.top.findNode("eyebrow")
    m.kindLabel = m.top.findNode("kindLabel")
    m.heading = m.top.findNode("heading")
    m.facts = m.top.findNode("facts")
    m.summary = m.top.findNode("summary")
    m.note = m.top.findNode("note")
    m.buttons = m.top.findNode("buttons")
    m.grid = m.top.findNode("grid")
    m.message = m.top.findNode("message")
    m.message.text = t("common.states.loading")
    m.kindLabel.text = t("titles.collection.label")
    m.top.findNode("smartText").text = t("titles.smart")
    m.started = false
    ' The collections of a Home row of several, else invalid.
    m.ids = invalid
    ' The page as the API task made it (lib/Pages.brs pageCollection), and
    ' the libraries once the server said ([{ id, name, kind }]).
    m.collection = invalid
    m.libraries = invalid
    ' What the buttons do, in their order: "play", "shuffle", "collection"
    ' (a library's own, on the merged page: its id in m.opens, at the same
    ' place).
    m.actions = []
    m.opens = []
    ' A show's next episode is being asked for: the buttons wait.
    m.picking = false
    m.buttons.observeField("chosen", "onButton")
    m.labelsShown = ""
    m.grid.observeField("itemSelected", "onTile")
    m.top.observeField("active", "onActive")
end sub

' Loaded each time it shows: back from playing, where each title was left
' has changed, and with it where Play starts.
sub onActive()
    if not m.top.active then return
    uiChangesSince()
    if m.started
        if m.grid.visible and m.page.translation[1] < 0
            m.grid.setFocus(true)
        else if m.buttons.visible
            m.buttons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
        load()
        return
    end if
    m.started = true
    ids = httpArray(m.top.params.ids)
    if ids.Count() > 1 then m.ids = ids
    m.heading.text = httpSentence(m.top.params.title)
    m.top.setFocus(true)
    load()
    uiGet("/api/libraries", "onLibraries")
end sub

sub load()
    if m.ids <> invalid
        parts = []
        for each id in m.ids
            parts.Push(urlValue(id))
        end for
        uiRequest({ method: "GET", path: urlWithQuery("/api/collections/merged", { ids: parts.Join(",") }), build: "merged" }, "onCollection")
        return
    end if
    uiRequest({ method: "GET", path: "/api/collections/" + urlValue(m.top.params.id), build: "collection" }, "onCollection")
end sub

sub onTitleChanged(change as object)
    uiPatchTiles(m.grid.content, change)
end sub

' A reload that fails leaves the page as it was.
sub onCollection(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        if m.collection = invalid and not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.collection = answer.data
    collection = m.collection
    m.message.visible = false
    m.heading.text = collection.title
    m.summary.text = collection.summary
    m.note.text = ""
    if collection.shown = 0 then m.note.text = t("titles.collection.empty")
    showBadges()
    showFacts()
    if m.backdrop.uri <> collection.backdrop
        m.backdrop.setHttpAgent(m.top.getScene().getHttpAgent())
        m.backdrop.uri = collection.backdrop
    end if
    showButtons()
    focused = m.grid.itemFocused
    m.grid.content = event.getRoSGNode().content
    if focused > 0 and focused < collection.shown then m.grid.jumpToItem = focused
    m.grid.visible = collection.shown > 0
    m.column.visible = true
    placeGrid()
    if m.grid.isInFocusChain() and not m.grid.visible then focusGrid(false)
    ' Only while this screen has the focus: a dialog over it keeps it.
    if m.top.hasFocus() and m.buttons.visible then m.buttons.setFocus(true)
end sub

sub onLibraries(event as object)
    answer = event.getData()
    if not answer.ok then return
    m.libraries = contentLibraries(answer.data)
    showFacts()
    ' The merged page's buttons name the libraries.
    if m.collection <> invalid and m.ids <> invalid then showButtons()
end sub

' "Collection", and beside it TMDB for one that mirrors TMDB's, Smart for
' one a filter fills, then its category, each in a box as wide as its
' words (a category's at most 560 pixels).
sub showBadges()
    category = m.top.findNode("categoryText")
    category.width = 0
    category.text = m.collection.category
    if category.boundingRect().width > 532 then category.width = 532
    x = m.kindLabel.boundingRect().width + 16
    for each name in ["tmdb", "smart", "category"]
        badge = m.top.findNode(name)
        badge.visible = (name = "tmdb" and m.collection.automatic) or (name = "smart" and m.collection.smart) or (name = "category" and m.collection.category <> "")
        if badge.visible
            back = m.top.findNode(name + "Back")
            back.width = m.top.findNode(name + "Text").boundingRect().width + 28
            badge.translation = [x, 0]
            x = x + back.width + 12
        end if
    end for
end sub

' What it holds, with its library's name once the libraries are known
' (the merged page's, its libraries').
sub showFacts()
    if m.collection = invalid then return
    if m.ids <> invalid
        m.facts.text = pageMergedFacts(m.collection, m.libraries)
        layoutText()
        return
    end if
    library = invalid
    for each one in httpArray(m.libraries)
        if one.id = m.collection.libraryId then library = one
    end for
    m.facts.text = pageCollectionFacts(m.collection, library)
    layoutText()
end sub

' The labels with text, one under the other.
sub layoutText()
    shown = [m.eyebrow, m.heading]
    spacings = [14]
    for each part in [{ label: m.facts, gap: 16 }, { label: m.summary, gap: 20 }, { label: m.note, gap: 28 }]
        if part.label.text <> ""
            spacings.Push(part.gap)
            shown.Push(part.label)
        end if
    end for
    m.text.removeChildrenIndex(m.text.getChildCount(), 0)
    m.text.appendChildren(shown)
    m.text.itemSpacings = spacings
end sub

' Play with something to play; Shuffle with two titles or more; on the
' merged page, each library's own collection once the libraries are known
' (lib/Pages.brs pageMergedButtons).
sub showButtons()
    playable = m.collection.shows.Count()
    if m.collection.movies then playable = m.collection.queue.Count()
    actions = []
    labels = []
    if playable > 0
        actions.Push("play")
        labels.Push(t("common.actions.play"))
    end if
    if playable > 1
        actions.Push("shuffle")
        labels.Push(t("titles.actions.shuffle"))
    end if
    ' Play and Shuffle open none.
    opens = []
    for each action in actions
        opens.Push(0)
    end for
    if m.ids <> invalid
        for each library in pageMergedButtons(m.collection, m.libraries)
            actions.Push("collection")
            opens.Push(library.id)
            labels.Push(t("titles.collection.onlyLibrary", { library: library.name }))
        end for
    end if
    if FormatJson(labels) <> m.labelsShown
        m.labelsShown = FormatJson(labels)
        hadFocus = m.buttons.isInFocusChain()
        m.buttons.chips = contentActionChips(labels, actions)
        contentKeepChip(m.buttons, m.buttons.focusIndex, hadFocus)
    end if
    m.actions = actions
    m.opens = opens
    m.buttons.visible = labels.Count() > 0
end sub

sub onButton(event as object)
    if m.collection = invalid then return
    at = event.getData()
    if at < 0 or at >= m.actions.Count() then return
    if m.actions[at] = "collection"
        uiNavigate({ to: "collection", params: { id: m.opens[at], title: m.collection.title } })
        return
    end if
    play(m.actions[at] = "shuffle")
end sub

' Movies: the player, with the rest to come one after the other (shuffled:
' in any order, from any of them). Shows: the next episode of the first,
' or of any of them.
sub play(shuffled as boolean)
    collection = m.collection
    if collection.movies
        count = collection.queue.Count()
        if count = 0 then return
        at = 0
        if shuffled then at = Rnd(count) - 1
        queue = pageQueueFrom(collection.queue, at, shuffled)
        uiNavigate({ to: "player", params: { itemId: queue[0].itemId, startMs: queue[0].startMs, queue: queue } })
        return
    end if
    count = collection.shows.Count()
    if count = 0 or m.picking then return
    pick = collection.shows[0]
    if shuffled then pick = collection.shows[Rnd(count) - 1]
    m.picking = true
    uiGet("/api/items/" + urlValue(pick) + "/on-deck", "onOnDeck")
end sub

sub onOnDeck(event as object)
    m.picking = false
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
        return
    end if
    episode = invalid
    if answer.data <> invalid then episode = httpObject(answer.data.item)
    ' Left meanwhile: nothing starts behind the screen now on top.
    if not m.top.active then return
    if episode = invalid
        uiMessage(t("titles.nothingToPlay"), [t("titles.collection.noEpisodes")])
        return
    end if
    tile = contentTile(episode, m.global.session.server, true)
    uiNavigate({ to: "player", params: { itemId: tile.itemId, startMs: playerStartOf(tile.positionMs, tile.durationMs) } })
end sub

sub onTile(event as object)
    uiOpenTile(m.grid.content.getChild(event.getData()))
end sub

' The posters under the words and buttons, as a title page's rows: a short
' summary left their names below the screen. Between 480 and 720 (their
' tiles' art starts 30 px down).
sub placeGrid()
    rect = m.column.boundingRect()
    if rect.height <= 0 then return
    y = Int(rect.y + rect.height + 66)
    if y < 480 then y = 480
    if y > 720 then y = 720
    if m.grid.translation[1] <> y then m.grid.translation = [168, y]
end sub

' The page slides up to the posters as they take the focus, the backdrop
' shaded under them all, and back down.
sub focusGrid(onGrid as boolean)
    if onGrid
        placeGrid()
        m.page.translation = [0, -(m.grid.translation[1] - 96)]
        uiDimBackdrop(m.top, true)
        m.grid.setFocus(true)
    else
        m.page.translation = [0, 0]
        uiDimBackdrop(m.top, false)
        if m.buttons.visible
            m.buttons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

' Down from the buttons goes to the posters, Up from their first row (or
' Back) back; the Options key on a poster offers what can be done with its
' title.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "down" and not m.grid.isInFocusChain() and m.grid.visible
        focusGrid(true)
        return true
    end if
    if (key = "up" or key = "back") and m.grid.isInFocusChain()
        focusGrid(false)
        return true
    end if
    if key = "play" and m.grid.isInFocusChain() and m.grid.content <> invalid
        uiPlayTile(m.grid.content.getChild(m.grid.itemFocused))
        return true
    end if
    if key = "options" and m.grid.isInFocusChain()
        uiTileOptions(m.grid.content.getChild(m.grid.itemFocused), false)
        return true
    end if
    return false
end function
