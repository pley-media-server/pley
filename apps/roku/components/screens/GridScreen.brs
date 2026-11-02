' The profile's playlists (GET /api/playlists) as a grid under their
' heading, the focus on the first. New playlist asks what it is for (Movies
' & TV or Music), then its name on the Roku's keyboard; POST
' /api/playlists makes it, which needs the sign-in to allow playlists, and
' it opens. params: { kind ("playlists"), id, title (shown until the server
' answers) }.

sub init()
    m.title = m.top.findNode("title")
    m.buttons = m.top.findNode("buttons")
    m.grid = m.top.findNode("grid")
    m.message = m.top.findNode("message")
    m.started = false
    ' How many playlists show, -1 until the server said; what the new
    ' playlist is for ("video", "audio") while it is named.
    m.shown = -1
    m.newKind = ""
    m.lastFocus = m.grid
    m.buttons.buttons = ["New playlist"]
    m.buttons.observeField("buttonSelected", "onNew")
    m.grid.observeField("itemSelected", "onSelected")
    m.top.observeField("active", "onActive")
end sub

' Loaded each time it shows: a playlist renamed, emptied, deleted or made
' meanwhile.
sub onActive()
    if not m.top.active
        m.lastFocus = m.grid
        if m.buttons.isInFocusChain() then m.lastFocus = m.buttons
        return
    end if
    if m.started
        for each change in uiChangesSince()
            onTitleChanged(change)
        end for
        if m.shown >= 0
            m.lastFocus.setFocus(true)
        else
            m.top.setFocus(true)
        end if
        load()
        return
    end if
    m.started = true
    uiChangesSince()
    m.title.text = httpSentence(m.top.params.title)
    m.top.setFocus(true)
    load()
end sub

sub load()
    uiRequest({ method: "GET", path: "/api/playlists", build: "playlists" }, "onPlaylists")
end sub

sub onPlaylists(event as object)
    answer = event.getData()
    if not answer.ok
        if m.shown < 0 and not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.shown = answer.data.shown
    focused = m.grid.itemFocused
    m.grid.content = event.getRoSGNode().content
    if focused > 0 and focused < m.shown then m.grid.jumpToItem = focused
    m.buttons.visible = true
    if m.shown = 0
        m.message.text = "No playlists yet. Make one with New playlist, or with Add to playlist, the Options key on a title."
        m.message.visible = true
        ' With nothing to focus in the grid, New playlist takes it.
        if m.top.hasFocus() or m.grid.hasFocus() then m.buttons.setFocus(true)
    else
        m.message.visible = false
        if m.top.hasFocus() then m.grid.setFocus(true)
    end if
end sub

sub onTitleChanged(change as object)
    uiPatchTiles(m.grid.content, change)
end sub

sub onSelected(event as object)
    uiOpenTile(m.grid.content.getChild(event.getData()))
end sub

' --- New playlist ---

sub onNew()
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = "New playlist"
    dialog.message = ["What is it for?"]
    dialog.buttons = ["Movies & TV", "Music", "Cancel"]
    dialog.observeField("buttonSelected", "onNewKind")
    m.top.getScene().dialog = dialog
end sub

sub onNewKind(event as object)
    chosen = event.getData()
    uiCloseDialog()
    if chosen < 0 or chosen > 1 then return
    m.newKind = "video"
    if chosen = 1 then m.newKind = "audio"
    dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
    dialog.title = "New playlist"
    dialog.message = ["Name it, then add to it with Add to playlist, the Options key on a title."]
    dialog.buttons = ["Create", "Cancel"]
    dialog.observeField("buttonSelected", "onNewName")
    m.top.getScene().dialog = dialog
end sub

sub onNewName(event as object)
    dialog = m.top.getScene().dialog
    name = ""
    if dialog <> invalid then name = dialog.text.Trim()
    uiCloseDialog()
    if event.getData() <> 0 or name = "" then return
    uiRequest({ method: "POST", path: "/api/playlists", json: { title: name, kind: m.newKind } }, "onMade")
end sub

' Made: it opens, empty, for titles to be added to it.
sub onMade(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then uiMessage("The playlist wasn't made", [pagePlaylistRefusal(answer)])
        return
    end if
    playlist = httpObject(answer.data)
    if playlist = invalid or not m.top.active then return
    uiNavigate({ to: "playlist", params: { id: httpNumber(playlist.id, 0), title: httpSentence(playlist.title) } })
end sub

' Up from the covers' first row goes to New playlist, Down back; the
' Options key on a cover offers what can be done with its playlist.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "up" and m.grid.isInFocusChain() and m.buttons.visible
        m.buttons.setFocus(true)
        return true
    end if
    if key = "down" and m.buttons.isInFocusChain() and m.shown > 0
        m.grid.setFocus(true)
        return true
    end if
    if key = "options" and m.grid.isInFocusChain() and m.shown > 0
        uiTileOptions(m.grid.content.getChild(m.grid.itemFocused), false)
        return true
    end if
    return false
end function
