' A collection (GET /api/collections/{id}, its library's name from GET
' /api/libraries). Play plays a collection of movies in its order, Up Next
' bringing on each next, and Shuffle in any order from any of them; a
' collection of shows plays the first show's next episode (GET
' /api/items/{id}/on-deck), Shuffle a random show's. Shuffle needs two
' titles or more. params: { id, title (shown until the server answers) }.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.shade = m.top.findNode("shade")
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
    ' The page as the API task made it (lib/Pages.brs pageCollection), and
    ' the libraries once the server said ([{ id, name, kind }]).
    m.collection = invalid
    m.libraries = invalid
    ' What the buttons do, in their order: "play", "shuffle".
    m.actions = []
    ' A show's next episode is being asked for: the buttons wait.
    m.picking = false
    m.buttons.observeField("buttonSelected", "onButton")
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
    m.heading.text = httpSentence(m.top.params.title)
    m.top.setFocus(true)
    load()
    uiGet("/api/libraries", "onLibraries")
end sub

sub load()
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
    if m.grid.isInFocusChain() and not m.grid.visible then focusGrid(false)
    ' Only while this screen has the focus: a dialog over it keeps it.
    if m.top.hasFocus() and m.buttons.visible then m.buttons.setFocus(true)
end sub

sub onLibraries(event as object)
    answer = event.getData()
    if not answer.ok then return
    m.libraries = contentLibraries(answer.data)
    showFacts()
end sub

' "Collection", and beside it TMDB for one that mirrors TMDB's, Smart for
' one a filter fills, each in a box as wide as its word.
sub showBadges()
    x = m.kindLabel.boundingRect().width + 16
    for each name in ["tmdb", "smart"]
        badge = m.top.findNode(name)
        badge.visible = (name = "tmdb" and m.collection.automatic) or (name = "smart" and m.collection.smart)
        if badge.visible
            back = m.top.findNode(name + "Back")
            back.width = m.top.findNode(name + "Text").boundingRect().width + 28
            badge.translation = [x, 0]
            x = x + back.width + 12
        end if
    end for
end sub

' What it holds, with its library's name once the libraries are known.
sub showFacts()
    if m.collection = invalid then return
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

' Play with something to play; Shuffle with two titles or more.
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
    if FormatJson(labels) <> FormatJson(m.buttons.buttons) then m.buttons.buttons = labels
    m.actions = actions
    m.buttons.visible = labels.Count() > 0
end sub

sub onButton(event as object)
    if m.collection = invalid then return
    play(m.actions[event.getData()] = "shuffle")
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

' The page slides up to the posters as they take the focus, the backdrop
' shaded under them all, and back down.
sub focusGrid(onGrid as boolean)
    if onGrid
        m.page.translation = [0, -624]
        m.shade.width = 1920
        m.grid.setFocus(true)
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
    if key = "options" and m.grid.isInFocusChain()
        uiTileOptions(m.grid.content.getChild(m.grid.itemFocused), false)
        return true
    end if
    return false
end function
