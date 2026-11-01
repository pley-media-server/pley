' A playlist (GET /api/playlists/{id}): Play and Shuffle play its entries
' one after the other, Up Next bringing on the next; Rename and Delete
' change it (PATCH, DELETE), which needs the sign-in to allow playlists.
' Move lifts an entry, which follows the focus up and down, its title
' marked, until OK puts it there (PUT …/order) or Back puts it back; the
' rows change in place, so focus moves only with keys.
' params: { id, title (shown until the server answers) }.

sub init()
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.buttons = m.top.findNode("buttons")
    m.entries = m.top.findNode("entries")
    m.message = m.top.findNode("message")
    m.hint = m.top.findNode("hint")
    m.started = false
    ' The entry lifted (its place now), -1 for none; the rows' fields as
    ' they are now, and as they were when it was lifted.
    m.moving = -1
    m.rows = []
    m.before = []
    m.playlist = invalid
    m.dialog = invalid
    m.entryOptions = invalid
    m.lastFocus = m.buttons
    m.buttons.buttons = ["Play", "Shuffle", "Rename", "Delete"]
    m.buttons.observeField("buttonSelected", "onButton")
    m.entries.observeField("itemSelected", "onEntry")
    m.entries.observeField("itemFocused", "onEntryFocused")
    m.top.observeField("active", "onActive")
end sub

' Loaded each time it shows: back from playing, the places have changed.
sub onActive()
    if not m.top.active
        m.lastFocus = m.buttons
        if m.entries.isInFocusChain() then m.lastFocus = m.entries
        return
    end if
    if not m.started
        m.started = true
        m.title.text = httpSentence(m.top.params.title)
        uiChangesSince()
        m.top.setFocus(true)
    else
        uiChangesSince()
        if m.buttons.visible then m.lastFocus.setFocus(true)
    end if
    load()
end sub

sub load()
    uiRequest({ method: "GET", path: "/api/playlists/" + urlValue(m.top.params.id), build: "playlist" }, "onPlaylist")
end sub

sub onTitleChanged(change as object)
    uiPatchTiles(m.entries.content, change)
end sub

sub onPlaylist(event as object)
    answer = event.getData()
    if not answer.ok
        if m.playlist = invalid and not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.playlist = answer.data
    m.title.text = m.playlist.title
    m.line.text = m.playlist.line
    keep = m.entries.itemFocused
    m.entries.content = event.getRoSGNode().content
    if keep > 0 and keep < m.playlist.shown then m.entries.jumpToItem = keep
    if m.playlist.shown = 0
        m.message.text = "This playlist is empty. Add titles to it with the Options key on them."
        m.message.visible = true
    else
        m.message.visible = false
    end if
    first = not m.buttons.visible
    m.buttons.visible = true
    if first and m.top.hasFocus() then m.buttons.setFocus(true)
end sub

sub onButton(event as object)
    if m.playlist = invalid then return
    chosen = m.buttons.buttons[event.getData()]
    if chosen = "Play"
        play(0, false)
    else if chosen = "Shuffle"
        play(0, true)
    else if chosen = "Rename"
        dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
        dialog.title = "Rename playlist"
        dialog.text = m.playlist.title
        dialog.buttons = ["Rename", "Cancel"]
        dialog.observeField("buttonSelected", "onRename")
        m.dialog = dialog
        m.top.getScene().dialog = dialog
    else
        dialog = CreateObject("roSGNode", "StandardMessageDialog")
        dialog.title = "Delete " + m.playlist.title + "?"
        dialog.message = ["The playlist goes; its titles stay in your libraries."]
        dialog.buttons = ["Delete", "Cancel"]
        dialog.observeField("buttonSelected", "onDelete")
        m.dialog = dialog
        m.top.getScene().dialog = dialog
    end if
end sub

' Plays the playlist from its entry at (shuffled: in any order, that one
' first), Up Next bringing on each next one.
sub play(at as integer, shuffled as boolean)
    if m.playlist.kind = "audio"
        ' Tracks: the music player, which keeps the playlist's order for
        ' Previous and Stop shuffling (shuffled, it starts anywhere), and
        ' Now Playing.
        tracks = m.playlist.queue
        if tracks.Count() = 0 then return
        if shuffled then at = Rnd(tracks.Count()) - 1
        m.global.music.play = { queue: tracks, at: at, shuffled: shuffled }
        uiNavigate({ to: "nowPlaying" })
        return
    end if
    queue = pageQueueFrom(m.playlist.queue, at, shuffled)
    if queue.Count() = 0 then return
    uiNavigate({ to: "player", params: { itemId: queue[0].itemId, startMs: queue[0].startMs, queue: queue } })
end sub

sub onEntry(event as object)
    if m.moving >= 0
        drop()
    else if m.playlist <> invalid
        play(event.getData(), false)
    end if
end sub

sub closeDialog()
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
end sub

sub onRename(event as object)
    name = ""
    if m.dialog <> invalid then name = m.dialog.text.Trim()
    closeDialog()
    if event.getData() <> 0 or name = "" or name = m.playlist.title then return
    uiRequest({ method: "PATCH", path: "/api/playlists/" + urlValue(m.top.params.id), json: { title: name } }, "onChanged")
end sub

sub onDelete(event as object)
    closeDialog()
    if event.getData() <> 0 then return
    uiRequest({ method: "DELETE", path: "/api/playlists/" + urlValue(m.top.params.id) }, "onDeleted")
end sub

sub onDeleted(event as object)
    answer = event.getData()
    if answer.ok
        uiNavigate({ back: true })
    else if not uiFailed(answer)
        uiMessage("The playlist wasn't deleted", [pagePlaylistRefusal(answer)])
    end if
end sub

' A change made: the playlist again, as the server has it now.
sub onChanged(event as object)
    answer = event.getData()
    if answer.ok
        load()
    else if not uiFailed(answer)
        uiMessage("That didn't work", [pagePlaylistRefusal(answer)])
    end if
end sub

' The Options key on an entry: Move and Remove from playlist (not in a
' smart one, which its filter fills and orders), then what can be done with
' its title.
sub entryOptions()
    entry = m.entries.content.getChild(m.entries.itemFocused)
    if entry = invalid then return
    tile = { itemId: entry.itemId, kind: entry.kind, title: entry.title, watched: entry.watched, parentId: entry.parentId, grandparentId: entry.grandparentId }
    options = []
    if not m.playlist.smart
        if m.entries.content.getChildCount() > 1 then options.Push({ label: "Move", action: "move" })
        options.Push({ label: "Remove from playlist", action: "remove" })
    end if
    for each option in pageTileOptions(tile, false, invalid)
        if option.action <> "playlist" then options.Push(option)
    end for
    labels = []
    for each option in options
        labels.Push(option.label)
    end for
    labels.Push("Cancel")
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = entry.title
    dialog.buttons = labels
    dialog.observeField("buttonSelected", "onEntryOption")
    m.entryOptions = { tile: tile, entryId: entry.entryId, options: options }
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onEntryOption(event as object)
    closeDialog()
    state = m.entryOptions
    m.entryOptions = invalid
    chosen = event.getData()
    if state = invalid or chosen >= state.options.Count() then return
    action = state.options[chosen].action
    if action = "move"
        lift(m.entries.itemFocused)
    else if action = "remove"
        uiRequest({ method: "DELETE", path: "/api/playlists/" + urlValue(m.top.params.id) + "/items/" + urlValue(state.entryId) }, "onChanged")
    else
        uiTitleAction(state.tile, action)
    end if
end sub

' --- Move ---

' The fields of the rows, in order.
function rowFields(content as object) as object
    rows = []
    for i = 0 to content.getChildCount() - 1
        node = content.getChild(i)
        rows.Push({ entryId: node.entryId, title: node.title, subtitle: node.subtitle, image: node.image, wide: node.wide, progress: node.progress, watched: node.watched, itemId: node.itemId, kind: node.kind, parentId: node.parentId, grandparentId: node.grandparentId, positionMs: node.positionMs, durationMs: node.durationMs, summary: node.summary })
    end for
    return rows
end function

' Lifts the entry at, which then follows the focus.
sub lift(at as integer)
    content = m.entries.content
    if content = invalid or at < 0 or at >= content.getChildCount() then return
    m.rows = rowFields(content)
    m.before = m.rows
    m.moving = at
    m.entries.setFocus(true)
    m.hint.visible = true
    retitle(at, at)
end sub

' The rows from first to last as m.rows has them, the one lifted marked:
' each a new node, which the list draws again.
sub retitle(first as integer, last as integer)
    content = m.entries.content
    for i = first to last
        if i < content.getChildCount()
            fields = {}
            fields.Append(m.rows[i])
            if i = m.moving then fields.title = fields.title + "  (moving)"
            fresh = CreateObject("roSGNode", "ContentNode")
            nodesFill(fresh, fields)
            content.replaceChild(fresh, i)
        end if
    end for
end sub

' Focus moved while an entry is lifted: it goes along.
sub onEntryFocused(event as object)
    if m.moving < 0 then return
    at = event.getData()
    if at = m.moving or at < 0 or at >= m.rows.Count() then return
    from = m.moving
    m.rows = orderMoved(m.rows, from, at)
    m.moving = at
    if from < at
        retitle(from, at)
    else
        retitle(at, from)
    end if
end sub

' OK: the entry lifted stays where it is, and the server keeps the order.
sub drop()
    at = m.moving
    m.moving = -1
    m.hint.visible = false
    retitle(at, at)
    ids = []
    changed = false
    for i = 0 to m.rows.Count() - 1
        ids.Push(m.rows[i].entryId)
        if m.rows[i].entryId <> m.before[i].entryId then changed = true
    end for
    if changed then uiRequest({ method: "PUT", path: "/api/playlists/" + urlValue(m.top.params.id) + "/order", json: { entry_ids: ids } }, "onChanged")
end sub

' Back: the entry lifted goes back where it was.
sub putBack()
    lifted = m.rows[m.moving].entryId
    m.rows = m.before
    m.moving = -1
    m.hint.visible = false
    retitle(0, m.rows.Count() - 1)
    for i = 0 to m.rows.Count() - 1
        if m.rows[i].entryId = lifted then m.entries.jumpToItem = i
    end for
end sub

' Right from the buttons goes to the entries, left back; while an entry is
' lifted, Back puts it back and nothing else leaves the list.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.moving >= 0
        if key = "back" then putBack()
        return key = "back" or key = "left" or key = "right" or key = "options"
    end if
    hasEntries = m.entries.content <> invalid and m.entries.content.getChildCount() > 0
    if key = "right" and m.buttons.isInFocusChain() and hasEntries
        m.entries.setFocus(true)
        return true
    end if
    if key = "left" and m.entries.isInFocusChain()
        m.buttons.setFocus(true)
        return true
    end if
    if key = "options" and m.entries.isInFocusChain() and hasEntries
        entryOptions()
        return true
    end if
    return false
end function
