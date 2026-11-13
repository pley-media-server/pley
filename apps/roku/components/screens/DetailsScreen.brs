sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.shade = m.top.findNode("shade")
    m.page = m.top.findNode("page")
    m.poster = m.top.findNode("poster")
    m.column = m.top.findNode("column")
    m.text = m.top.findNode("text")
    m.heading = m.top.findNode("heading")
    m.line = m.top.findNode("line")
    m.genres = m.top.findNode("genres")
    m.credits = m.top.findNode("credits")
    m.tagline = m.top.findNode("tagline")
    m.summary = m.top.findNode("summary")
    m.note = m.top.findNode("note")
    m.buttons = m.top.findNode("buttons")
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.stage = m.top.findNode("stage")
    m.stageLine = m.top.findNode("stageLine")
    m.requestPoll = m.top.findNode("requestPoll")
    m.panel = m.top.findNode("panel")
    m.panelTitle = m.top.findNode("panelTitle")
    m.menu = m.top.findNode("menu")
    m.choices = m.top.findNode("choices")
    m.info = m.top.findNode("info")
    m.message.text = t("common.states.loading")
    m.column.visible = false
    ' What the panel shows: "" (closed), a menu ("more", "tracks"), the
    ' choices of one of its entries ("rating", "version", "audio",
    ' "subtitles") or the media info ("info"); the menu Back goes back to
    ' ("" for none), and the values of the choices.
    m.panelKind = ""
    m.panelBack = ""
    m.panelValues = []
    ' Audio and subtitles' entries, in order; the version chosen (its file's
    ' id, invalid for the first), which isn't saved, as on the other apps;
    ' the audio picked on this page (-1 Automatic, -2 none), which subtitles
    ' picked after are saved with.
    m.trackKinds = []
    m.fileId = invalid
    m.audioPick = -2
    m.started = false
    ' What the server says of requests for the title (GET
    ' /api/items/{id}/request), invalid for nothing to show; the asks (an
    ' answer to an earlier one is left out); Request more seasons' tile
    ' while the person may ask for some.
    m.itemRequest = invalid
    m.requestAsks = 0
    m.askMore = invalid
    ' The item's JSON, which the player starts with, and what it shows; a
    ' show's episode to play next, once the server said (invalid for none).
    m.item = invalid
    m.detail = invalid
    m.onDeck = invalid
    m.onDeckKnown = false
    ' The rows under the details by key, as they come, and their keys as
    ' shown, in order.
    m.rowNodes = {}
    m.rowKeys = []
    m.buttons.observeField("buttonSelected", "onButton")
    m.menu.observeField("itemSelected", "onMenu")
    m.choices.observeField("itemSelected", "onChoice")
    m.rows.observeField("rowItemSelected", "onTile")
    m.requestPoll.observeField("fire", "askItemRequest")
    m.top.observeField("active", "onActive")
    m.top.observeField("requestSummary", "onSummary")
    m.top.observeField("requestsChanged", "onRequestsChanged")
end sub

' Loaded each time it shows: back from the player, the place has changed.
' Asking for it also has the server read the start of the file ahead. The
' focus comes here at once, as the screen before had it: the remote works
' whatever the answer, and a second Back goes back. The rows come with the
' first answer.
sub onActive()
    if not m.top.active
        m.requestPoll.control = "stop"
        return
    end if
    uiChangesSince()
    if m.panel.visible
        closePanel()
    else if m.rows.visible and m.page.translation[1] < 0
        m.rows.setFocus(true)
    else if m.buttons.visible
        m.buttons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
    path = "/api/items/" + urlValue(m.top.params.itemId)
    if m.started
        uiGet(path, "onDetail")
    else
        m.started = true
        uiRequest({ method: "GET", path: path, build: "detail" }, "onDetail")
    end if
end sub

sub onTitleChanged(change as object)
    if m.detail <> invalid and change.itemId = m.detail.itemId then uiGet("/api/items/" + urlValue(m.detail.itemId), "onDetail")
end sub

' A reload that fails leaves the details as they were.
sub onDetail(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        if m.detail = invalid and not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    first = m.item = invalid
    m.item = answer.data
    kind = httpString(m.item.kind)
    if kind = "show"
        m.onDeckKnown = false
        uiGet("/api/items/" + urlValue(m.item.id) + "/on-deck", "onOnDeck")
    end if
    if first then askRows(event.getRoSGNode(), answer, kind)
    showDetail()
    askItemRequest()
end sub

' A show's next episode: its button comes first.
sub onOnDeck(event as object)
    answer = event.getData()
    m.onDeck = invalid
    if answer.ok and answer.data <> invalid then m.onDeck = httpObject(answer.data.item)
    m.onDeckKnown = true
    showDetail()
end sub

sub showDetail()
    if m.item = invalid then return
    m.detail = contentDetail(m.item, m.global.session.server, m.onDeck)
    m.message.visible = false
    m.heading.text = m.detail.heading
    m.line.text = m.detail.line
    m.genres.text = m.detail.genres
    m.credits.text = m.detail.credits
    m.tagline.text = m.detail.tagline
    ' Seven buttons take a line from the summary (three, as on Android TV),
    ' and so does the credits line: under a heading of two lines they would
    ' reach the rows' title. More's Details has the whole of it.
    m.summary.maxLines = 4
    if m.detail.buttons.Count() > 6 then m.summary.maxLines = 3
    if m.detail.credits <> "" then m.summary.maxLines = m.summary.maxLines - 1
    m.summary.text = m.detail.summary
    m.note.text = m.detail.note
    layoutText()
    agent = m.top.getScene().getHttpAgent()
    if m.backdrop.uri <> m.detail.backdrop
        m.backdrop.setHttpAgent(agent)
        m.backdrop.uri = m.detail.backdrop
    end if
    if m.poster.uri <> m.detail.poster
        m.poster.setHttpAgent(agent)
        m.poster.uri = m.detail.poster
    end if
    ' A show's buttons wait for its next episode, which comes first.
    hasButtons = m.detail.buttons.Count() > 0 and (m.detail.kind <> "show" or m.onDeckKnown)
    if hasButtons
        focused = m.buttons.buttonFocused
        hadFocus = m.buttons.isInFocusChain()
        if FormatJson(m.buttons.buttons) <> FormatJson(m.detail.buttons) then m.buttons.buttons = m.detail.buttons
        uiKeepButton(m.buttons, focused, hadFocus)
    end if
    m.buttons.visible = hasButtons
    m.column.visible = true
    ' A menu of the panel says its entries as they are now.
    if m.panelKind = "more"
        openMore()
    else if m.panelKind = "tracks"
        openTracks()
    end if
    ' Only while this screen has the focus: a dialog or the panel over it
    ' keeps it.
    if m.top.isInFocusChain() and not m.rows.isInFocusChain() and not m.panel.visible
        if hasButtons
            m.buttons.setFocus(true)
        else
            m.top.setFocus(true)
        end if
    end if
end sub

' The labels with text, one under the other.
sub layoutText()
    shown = [m.heading]
    spacings = []
    for each part in [{ label: m.line, gap: 20 }, { label: m.genres, gap: 8 }, { label: m.credits, gap: 8 }, { label: m.tagline, gap: 28 }, { label: m.summary, gap: 20 }, { label: m.note, gap: 28 }]
        if part.label.text <> ""
            spacings.Push(part.gap)
            shown.Push(part.label)
        end if
    end for
    m.text.removeChildrenIndex(m.text.getChildCount(), 0)
    m.text.appendChildren(shown)
    m.text.itemSpacings = spacings
end sub

' --- The rows ---

' The rows that came with the details (cast and crew, extras), and those
' asked for now: a show's seasons, More Like This and the collections a
' movie or show is part of.
sub askRows(request as object, answer as object, kind as string)
    built = request.content
    if built <> invalid and answer.rows <> invalid
        children = built.getChildren(-1, 0)
        for i = 0 to answer.rows.Count() - 1
            if i < children.Count() then keepRow(answer.rows[i].key, children[i], answer.rows[i].wide)
        end for
    end if
    path = "/api/items/" + urlValue(m.item.id)
    if kind = "show" then uiRequest({ method: "GET", path: path + "/children", build: "seasonTiles", row: "seasons", title: t("titles.page.rows.seasons") }, "onRow")
    if kind = "movie" or kind = "show"
        uiRequest({ method: "GET", path: path + "/related", query: { limit: 20 }, build: "tiles", row: "related", title: t("titles.page.rows.moreLikeThis") }, "onRow")
        uiRequest({ method: "GET", path: path + "/collections", build: "collections", row: "collections", title: t("titles.page.rows.collections") }, "onRow")
    end if
    showRows()
end sub

sub onRow(event as object)
    request = event.getRoSGNode()
    answer = event.getData()
    if not answer.ok or request.content = invalid or request.content.getChildCount() = 0 then return
    row = CreateObject("roSGNode", "ContentNode")
    row.title = request.spec.title
    row.appendChildren(request.content.getChildren(-1, 0))
    keepRow(request.spec.row, row, false)
    if request.spec.row = "seasons" then placeAskTile()
    showRows()
end sub

sub keepRow(key as string, row as object, wide as boolean)
    m.rowNodes[key] = { node: row, wide: wide }
end sub

' The rows there are, in their order; the focus stays on its row.
sub showRows()
    keys = []
    for each key in ["seasons", "credits", "extras", "related", "collections"]
        if m.rowNodes.DoesExist(key) then keys.Push(key)
    end for
    if keys.Count() = 0 or FormatJson(keys) = FormatJson(m.rowKeys) then return
    focusedKey = ""
    position = m.rows.rowItemFocused
    if position <> invalid and position.Count() = 2 and position[0] >= 0 and position[0] < m.rowKeys.Count() then focusedKey = m.rowKeys[position[0]]
    content = CreateObject("roSGNode", "ContentNode")
    sizes = []
    heights = []
    for each key in keys
        row = m.rowNodes[key]
        content.appendChild(row.node)
        if row.wide
            sizes.Push([400, 300])
        else
            sizes.Push([220, 410])
        end if
        heights.Push(sizes.Peek()[1] + 60)
    end for
    m.rowKeys = keys
    m.rows.rowItemSize = sizes
    m.rows.rowHeights = heights
    m.rows.content = content
    for i = 0 to keys.Count() - 1
        if keys[i] = focusedKey then m.rows.jumpToRowItem = [i, position[1]]
    end for
    m.rows.visible = true
end sub

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

sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row <> invalid then uiOpenTile(row.getChild(position[1]))
end sub

' --- Requests ---

' Once the page has drawn, a movie's or show's requests, where the person
' may ask or has asked for something: how its request stands, and whether
' more seasons may be asked for.
sub askItemRequest()
    m.requestPoll.control = "stop"
    if m.item = invalid then return
    kind = httpString(m.item.kind)
    summary = reqSummaryOf(m.top.requestSummary)
    if (kind <> "movie" and kind <> "show") or not (reqMayAsk(summary) or summary.own > 0) then return
    m.requestAsks = m.requestAsks + 1
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(m.item.id) + "/request", ask: m.requestAsks }, "onItemRequest")
end sub

' No match on TMDB (404), or requests off: nothing shows. The server not
' answering leaves it as it was. While the request downloads or goes into
' a library, it is asked again each 10 s.
sub onItemRequest(event as object)
    if event.getRoSGNode().spec.ask <> m.requestAsks then return
    answer = event.getData()
    if answer.ok
        m.itemRequest = httpObject(answer.data)
    else if answer.error <> "network"
        m.itemRequest = invalid
    end if
    showItemRequest()
    request = invalid
    if m.itemRequest <> invalid then request = httpObject(m.itemRequest.request)
    if request <> invalid and reqBusy(httpString(request.stage)) and m.top.active then m.requestPoll.control = "start"
end sub

sub showItemRequest()
    if m.item = invalid then return
    shown = reqItemRequest(m.itemRequest, reqSummaryOf(m.top.requestSummary), CreateObject("roDateTime").AsSeconds(), fmtZoneSeconds())
    stage = shown.stage
    if stage = invalid
        m.stage.visible = false
        m.stageLine.visible = false
    else
        m.stage.color = reqToneColor(stage.tone)
        m.stage.progress = stage.progress
        m.stage.text = stage.text
        m.stageLine.text = stage.line
        m.stage.visible = true
        m.stageLine.visible = stage.line <> ""
    end if
    m.askMore = invalid
    if shown.askMore and shown.kind = "show" and shown.tmdbId > 0 then m.askMore = reqAskMoreTile(shown.kind, shown.tmdbId, httpSentence(m.item.title))
    placeAskTile()
end sub

' Request more seasons at the end of the show's seasons, once they show,
' while more may be asked for.
sub placeAskTile()
    if not m.rowNodes.DoesExist("seasons") then return
    row = m.rowNodes.seasons.node
    count = row.getChildCount()
    placed = count > 0 and row.getChild(count - 1).kind = "askMore"
    if placed and m.askMore = invalid then row.removeChildIndex(count - 1)
    if not placed and m.askMore <> invalid then nodesAppend(row, [m.askMore])
end sub

' The summary changed: the requests asked for, if they weren't yet;
' else whether more may be asked for.
sub onSummary()
    if m.item = invalid then return
    if m.requestAsks = 0
        askItemRequest()
    else
        showItemRequest()
    end if
end sub

' Requests changed: this title's (or many) are asked for again.
sub onRequestsChanged()
    change = m.top.requestsChanged
    tmdbId = httpNumber(change.tmdbId, 0)
    if tmdbId > 0 and m.itemRequest <> invalid and httpNumber(m.itemRequest.tmdb_id, 0) <> tmdbId then return
    askItemRequest()
end sub

' --- The buttons ---

sub onButton(event as object)
    if m.detail = invalid then return
    action = m.detail.actions[event.getData()]
    if action = "resume" or action = "start"
        startMs = 0
        if action = "resume" then startMs = m.detail.positionMs
        ' The player has the item already: one request less before it plays.
        params = { itemId: m.detail.itemId, startMs: startMs, item: m.item }
        if m.fileId <> invalid then params.fileId = m.fileId
        uiNavigate({ to: "player", params: params })
    else if action = "playNext"
        if m.onDeck = invalid then return
        tile = contentTile(m.onDeck, m.global.session.server, true)
        uiNavigate({ to: "player", params: { itemId: tile.itemId, startMs: playerStartOf(tile.positionMs, tile.durationMs) } })
    else if action = "episodes"
        uiNavigate({ to: "show", params: { showId: m.detail.itemId, seasonId: 0 } })
    else if action = "trailer"
        ' From its start, as on the other TVs.
        uiNavigate({ to: "player", params: { itemId: m.detail.trailerId, startMs: 0 } })
    else if action = "tracks"
        openTracks()
    else if action = "more"
        openMore()
    else
        titleAction(action)
    end if
end sub

' What the title's tile would do with the Options key: mark it, My List,
' Add to playlist, Go to show or season.
sub titleAction(action as string)
    uiTitleAction({ itemId: m.detail.itemId, kind: m.detail.kind, parentId: m.detail.seasonId, grandparentId: m.detail.showId }, action)
end sub

' --- The panel ---

' More: what is used less, in a menu over the right of the page, as on the
' other TVs: the profile's rating, Go to show and Go to season, Media info,
' Add to playlist.
sub openMore()
    labels = []
    for each entry in m.detail.more
        labels.Push(entry.label)
    end for
    showMenu("more", t("common.actions.more"), labels)
end sub

' A menu; the entry in focus stays in focus as it shows again.
sub showMenu(kind as string, title as string, labels as object)
    at = 0
    if m.panelKind = kind or m.panelBack = kind then at = m.menu.itemFocused
    items = []
    for each label in labels
        items.Push({ title: label })
    end for
    m.panelKind = kind
    m.panelBack = ""
    m.panelTitle.text = title
    m.menu.content = nodesContent(items)
    if at > 0 and at < items.Count() then m.menu.jumpToItem = at
    m.choices.visible = false
    m.info.visible = false
    m.menu.visible = true
    m.panel.visible = true
    m.menu.setFocus(true)
end sub

' The choices of a menu's entry, the current one checked and focused; Back
' goes to the menu named back ("" closes the panel).
sub showChoices(kind as string, title as string, labels as object, values as object, current as integer, back as string)
    items = []
    for each label in labels
        items.Push({ title: label })
    end for
    m.panelKind = kind
    m.panelBack = back
    m.panelValues = values
    m.panelTitle.text = title
    ' A version's line is long: smaller letters keep it whole.
    size = "Medium"
    if kind = "version" then size = "Small"
    m.choices.font = "font:" + size + "SystemFont"
    m.choices.focusedFont = "font:" + size + "BoldSystemFont"
    m.choices.content = nodesContent(items)
    m.choices.checkedItem = current
    if current > 0 then m.choices.jumpToItem = current
    m.menu.visible = false
    m.info.visible = false
    m.choices.visible = true
    m.panel.visible = true
    m.choices.setFocus(true)
end sub

' Media info: each file and every stream in full (Media.brs), as on the
' other TVs, in text that Up and Down scroll; Back goes back to More.
sub showInfo()
    m.panelKind = "info"
    m.panelBack = "more"
    m.panelTitle.text = t("titles.page.mediaInfo")
    m.info.text = mediaInfoText(m.item)
    m.menu.visible = false
    m.choices.visible = false
    m.info.visible = true
    m.panel.visible = true
    m.info.setFocus(true)
end sub

' Details: the whole summary, then who made the title, its original
' title, when it came out, its studio and how often it was played, as on
' Android TV, in text that Up and Down scroll; Back goes back to More.
sub showAbout()
    m.panelKind = "info"
    m.panelBack = "more"
    m.panelTitle.text = t("titles.page.about")
    m.info.text = contentAbout(m.item, CreateObject("roDateTime").AsSeconds(), fmtZoneSeconds())
    m.menu.visible = false
    m.choices.visible = false
    m.info.visible = true
    m.panel.visible = true
    m.info.setFocus(true)
end sub

sub closePanel()
    m.panel.visible = false
    m.panelKind = ""
    m.panelBack = ""
    if m.buttons.visible
        m.buttons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

sub onMenu(event as object)
    at = event.getData()
    if m.panelKind = "tracks"
        if at >= 0 and at < m.trackKinds.Count() then openTrackChoices(m.trackKinds[at], "tracks")
        return
    end if
    if m.panelKind <> "more" or at < 0 or at >= m.detail.more.Count() then return
    action = m.detail.more[at].action
    if action = "rate"
        labels = []
        values = []
        for stars = 0 to 5
            labels.Push(contentStarsLabel(stars))
            values.Push(stars)
        end for
        showChoices("rating", t("titles.page.yourRating"), labels, values, m.detail.stars, "more")
        return
    end if
    if action = "info"
        showInfo()
        return
    end if
    if action = "about"
        showAbout()
        return
    end if
    closePanel()
    titleAction(action)
end sub

sub onChoice(event as object)
    at = event.getData()
    if at < 0 or at >= m.panelValues.Count() then return
    kind = m.panelKind
    if kind = "rating"
        rate(m.panelValues[at])
        openMore()
    else if kind = "version" or kind = "audio" or kind = "subtitles"
        if kind = "version"
            chooseVersion(m.panelValues[at])
        else
            pickTrack(kind, m.panelValues[at])
        end if
        if m.panelBack = "tracks"
            openTracks()
        else
            closePanel()
        end if
    end if
end sub

' Back from an entry's choices to its menu, else the panel closes.
sub goBackInPanel()
    if m.panelBack = "more"
        openMore()
    else if m.panelBack = "tracks"
        openTracks()
    else
        closePanel()
    end if
end sub

' --- Audio and subtitles ---

' The file Play plays: the version chosen, else the first.
function playingFile() as dynamic
    return contentFile(m.item, m.fileId)
end function

' The title's versions (several files), and the file's audio (two tracks
' or more) and subtitles (any), as on the other TVs: each an entry saying
' what plays; the tracks chosen are saved at once. With only one of them
' its choices open at once.
sub openTracks()
    versions = mediaVersions(m.item, m.fileId)
    tracks = mediaTracks(playingFile())
    labels = []
    m.trackKinds = []
    if versions.values.Count() > 1
        labels.Push(t("titles.page.versionIs", { value: versions.labels[versions.current] }))
        m.trackKinds.Push("version")
    end if
    if tracks.audio.labels.Count() > 0
        labels.Push(t("titles.page.audioIs", { value: tracks.audio.labels[tracks.audio.current] }))
        m.trackKinds.Push("audio")
    end if
    if tracks.subtitles.labels.Count() > 0
        labels.Push(t("titles.page.subtitlesIs", { value: tracks.subtitles.labels[tracks.subtitles.current] }))
        m.trackKinds.Push("subtitles")
    end if
    if m.trackKinds.Count() > 1
        showMenu("tracks", t("titles.actions.audioAndSubtitles"), labels)
    else if m.trackKinds.Count() = 1
        openTrackChoices(m.trackKinds[0], "")
    end if
end sub

sub openTrackChoices(kind as string, back as string)
    if kind = "version"
        versions = mediaVersions(m.item, m.fileId)
        showChoices("version", t("titles.page.version"), versions.labels, versions.values, versions.current, back)
        return
    end if
    tracks = mediaTracks(playingFile())
    if kind = "audio"
        showChoices("audio", t("titles.page.audio"), tracks.audio.labels, tracks.audio.values, tracks.audio.current, back)
    else
        showChoices("subtitles", t("titles.page.subtitles"), tracks.subtitles.labels, tracks.subtitles.values, tracks.subtitles.current, back)
    end if
end sub

' PUT /api/items/{id}/streams, as on the other TVs: audio picked here
' (Automatic as none) goes with the subtitles shown; subtitles picked go
' with the audio picked here, else leave the audio as it was picked. The
' page, and Play pressed at once, start with them; the page is asked for
' again for the server's word on them.
sub pickTrack(kind as string, index as integer)
    file = playingFile()
    if file = invalid then return
    subtitle = Int(httpNumber(file.selected_subtitle, -1))
    if kind = "audio"
        m.audioPick = index
    else
        subtitle = index
    end if
    mediaPick(m.item, file.id, m.audioPick, subtitle)
    body = tracksStreamsBody(file.id, m.audioPick, subtitle)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.item.id) + "/streams", jsonText: FormatJson(body) }, "onTracksSaved")
end sub

' Play plays the version chosen (file_id in POST /api/playback), its
' tracks as the server picks them; it isn't saved, as on the other apps.
sub chooseVersion(fileId as dynamic)
    m.fileId = fileId
    m.audioPick = -2
end sub

sub onTracksSaved(event as object)
    answer = event.getData()
    if not answer.ok and not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
    if m.item <> invalid then uiGet("/api/items/" + urlValue(m.item.id), "onDetail")
end sub

' --- Your rating ---

' PUT /api/items/{id}/rating: 2 to 10 (half stars), null for none. More
' says it at once, and again as it was if the server refuses.
sub rate(stars as integer)
    if stars < 0 or stars > 5 or m.item = invalid then return
    before = m.detail.stars
    rating = invalid
    if stars > 0 then rating = stars * 2
    showRating(rating)
    uiRequest({ method: "PUT", path: "/api/items/" + urlValue(m.item.id) + "/rating", json: { rating: rating }, before: before }, "onRated")
end sub

sub onRated(event as object)
    answer = event.getData()
    if answer.ok then return
    before = event.getRoSGNode().spec.before
    rating = invalid
    if before > 0 then rating = before * 2
    showRating(rating)
    if not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
end sub

sub showRating(rating as dynamic)
    state = httpObject(m.item.user_state)
    if state = invalid
        state = {}
        m.item.user_state = state
    end if
    state.rating = rating
    showDetail()
end sub

' Down from the buttons goes to the rows, up from the first row back; the
' Options key on a title of the rows offers what can be done with it. Back
' or Left closes the panel (an entry's choices go back to its menu), and
' keys its lists leave go nowhere else.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.panel.visible
        if key = "back" or key = "left" then goBackInPanel()
        return true
    end if
    if key = "down" and not m.rows.isInFocusChain() and m.rows.visible
        focusRows(true)
        return true
    end if
    if (key = "up" or key = "back") and m.rows.isInFocusChain()
        focusRows(false)
        return true
    end if
    if key = "options" and m.rows.isInFocusChain()
        position = m.rows.rowItemFocused
        row = m.rows.content.getChild(position[0])
        if row <> invalid then uiTileOptions(row.getChild(position[1]), false)
        return true
    end if
    ' The title's own: Add to playlist besides what its buttons do.
    if key = "options" and m.detail <> invalid
        uiTileOptions({ itemId: m.detail.itemId, kind: m.detail.kind, title: m.detail.heading, watched: m.detail.watched, parentId: m.detail.seasonId, grandparentId: m.detail.showId }, false)
        return true
    end if
    return false
end function
