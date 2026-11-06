' What can be done with a title from the screens that show it: the
' Options key (*) on a tile, and the buttons of a title's page. On the
' render thread, with Ui.brs; changes go to the server through the API
' task, then to the scene's list of changes (uiNoteChange). A screen that
' includes this file has onTitleChanged(change), called for a change made
' while it shows; one covered meanwhile reads them as it shows again
' (uiChangesSince).

' The Options key (*) on a tile: what can be done with its title, in a
' dialog (Mark as watched or unwatched, Add to or Remove from My List,
' Remove from Continue Watching, Go to show, Go to season). My List is as
' Home last saw it.
sub uiTileOptions(tile as dynamic, continueWatching as boolean)
    if tile = invalid then return
    options = pageTileOptions(tile, continueWatching, pageInList(m.top.getScene().myList, tile.itemId))
    if options.Count() = 0 then return
    labels = []
    for each option in options
        labels.Push(option.label)
    end for
    labels.Push(t("common.actions.cancel"))
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = tile.title
    dialog.buttons = labels
    dialog.observeField("buttonSelected", "uiTileOptionChosen")
    m.tileOptions = { tile: { itemId: tile.itemId, kind: tile.kind, parentId: tile.parentId, grandparentId: tile.grandparentId }, options: options }
    m.top.getScene().dialog = dialog
end sub

sub uiTileOptionChosen(event as object)
    chosen = event.getData()
    state = m.tileOptions
    m.tileOptions = invalid
    uiCloseDialog()
    if state = invalid or chosen < 0 or chosen >= state.options.Count() then return
    uiTitleAction(state.tile, state.options[chosen].action)
end sub

' Does what an action of the Options key or a title's page says, for tile
' ({ itemId, kind, parentId, grandparentId }): goes to an episode's show
' (its page) or season (its episodes, the season's page on Roku), as on
' the other apps, or asks the server.
sub uiTitleAction(tile as object, action as string)
    if action = "playlist"
        uiAddToPlaylist(tile)
        return
    end if
    showId = httpNumber(tile.grandparentId, 0)
    if action = "show"
        if showId > 0 then uiNavigate({ to: "details", params: { itemId: showId } })
        return
    end if
    if action = "season"
        if showId > 0 then uiNavigate({ to: "show", params: { showId: showId, seasonId: httpNumber(tile.parentId, 0) } })
        return
    end if
    request = pageActionRequest(action, tile.itemId)
    if request = invalid then return
    request.action = action
    request.itemId = tile.itemId
    uiRequest(request, "uiTitleActionDone")
end sub

sub uiTitleActionDone(event as object)
    answer = event.getData()
    spec = event.getRoSGNode().spec
    if not answer.ok
        if not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
        return
    end if
    scene = m.top.getScene()
    if spec.action = "listAdd" or spec.action = "listRemove" then scene.myList = pageListWith(scene.myList, spec.itemId, spec.action = "listAdd")
    change = uiNoteChange({ itemId: spec.itemId, action: spec.action })
    m.changesSeen = change.count
    onTitleChanged(change)
end sub

' --- Add to playlist ---

' The profile's playlists of the title's kind, to add it to one, or to a
' new one named on the keyboard.
sub uiAddToPlaylist(tile as object)
    m.playlistTile = tile
    uiRequest({ method: "GET", path: "/api/playlists" }, "uiPlaylistsListed")
end sub

sub uiPlaylistsListed(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        if not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
        return
    end if
    m.playlistChoices = pagePlaylistChoices(answer.data.playlists, pagePlaylistKind(httpString(m.playlistTile.kind)))
    labels = []
    for each playlist in m.playlistChoices
        labels.Push(playlist.title)
    end for
    labels.Push(t("titles.playlist.newMenu"))
    labels.Push(t("common.actions.cancel"))
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = t("titles.actions.addToPlaylist")
    dialog.buttons = labels
    dialog.observeField("buttonSelected", "uiPlaylistChosen")
    m.top.getScene().dialog = dialog
end sub

sub uiPlaylistChosen(event as object)
    chosen = event.getData()
    uiCloseDialog()
    count = m.playlistChoices.Count()
    if chosen < count
        playlist = m.playlistChoices[chosen]
        uiRequest({ method: "POST", path: "/api/playlists/" + urlValue(playlist.id) + "/items", json: { item_ids: [m.playlistTile.itemId] }, playlist: playlist.title }, "uiPlaylistAdded")
    else if chosen = count
        dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
        dialog.title = t("titles.playlist.new")
        dialog.buttons = [t("titles.playlist.create"), t("common.actions.cancel")]
        dialog.observeField("buttonSelected", "uiPlaylistNamed")
        m.top.getScene().dialog = dialog
    end if
end sub

sub uiPlaylistNamed(event as object)
    dialog = m.top.getScene().dialog
    name = ""
    if dialog <> invalid then name = dialog.text.Trim()
    uiCloseDialog()
    if event.getData() <> 0 or name = "" then return
    uiRequest({ method: "POST", path: "/api/playlists", json: { title: name, item_ids: [m.playlistTile.itemId] }, playlist: name }, "uiPlaylistAdded")
end sub

sub uiPlaylistAdded(event as object)
    answer = event.getData()
    if answer.ok
        uiMessage(t("titles.playlist.added"), [t("titles.playlist.addedTo", { title: event.getRoSGNode().spec.playlist })])
    else if not uiFailed(answer)
        uiMessage(t("common.states.failed"), [pagePlaylistRefusal(answer)])
    end if
end sub

' A title marked watched or unwatched, or played to some place, on this
' screen or another: its tiles in content (a list's, or rows of lists)
' show it. Each is replaced by a copy, which the list draws again.
sub uiPatchTiles(content as dynamic, change as object)
    if content = invalid then return
    action = httpString(change.action)
    if action <> "watched" and action <> "unwatched" and action <> "played" then return
    for i = 0 to content.getChildCount() - 1
        node = content.getChild(i)
        if node.getChildCount() > 0
            uiPatchTiles(node, change)
        else if node.hasField("itemId") and node.itemId = change.itemId and node.hasField("watched")
            fields = {}
            for each key in ["title", "subtitle", "image", "wide", "square", "progress", "watched", "itemId", "kind", "parentId", "grandparentId", "positionMs", "durationMs", "summary", "entryId", "historyId", "qualityBadges"]
                if node.hasField(key) then fields[key] = node.getField(key)
            end for
            fields.positionMs = 0
            fields.progress = 0.0
            if action = "played"
                fields.watched = change.watched
                if not change.watched
                    fields.positionMs = change.positionMs
                    fields.progress = fmtProgress(change.positionMs, change.durationMs)
                end if
            else
                fields.watched = action = "watched"
            end if
            fresh = CreateObject("roSGNode", "ContentNode")
            nodesFill(fresh, fields)
            content.replaceChild(fresh, i)
        end if
    end for
end sub
