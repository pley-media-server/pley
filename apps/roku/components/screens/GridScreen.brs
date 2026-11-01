' A grid of titles under a heading: a person's movies and shows (GET
' /api/people/{id}), an artist's albums (GET /api/items/{id}/children), or
' the profile's playlists (GET /api/playlists). params: { kind ("person",
' "artist", "playlists"), id, title (shown until the server answers) }.

sub init()
    m.photoFrame = m.top.findNode("photoFrame")
    m.photo = m.top.findNode("photo")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.summary = m.top.findNode("summary")
    m.grid = m.top.findNode("grid")
    m.message = m.top.findNode("message")
    m.started = false
    m.grid.observeField("itemSelected", "onSelected")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    m.grid.setFocus(true)
    if m.started
        for each change in uiChangesSince()
            onTitleChanged(change)
        end for
        ' A playlist renamed, emptied or deleted meanwhile.
        if m.top.params.kind = "playlists" then load()
        return
    end if
    m.started = true
    uiChangesSince()
    params = m.top.params
    m.title.text = httpSentence(params.title)
    load()
end sub

sub load()
    params = m.top.params
    if params.kind = "person"
        uiRequest({ method: "GET", path: "/api/people/" + urlValue(params.id), build: "person" }, "onGrid")
    else if params.kind = "playlists"
        uiRequest({ method: "GET", path: "/api/playlists", build: "playlists" }, "onGrid")
    else
        ' An artist's albums are square.
        m.grid.itemSize = [220, 300]
        m.grid.numRows = 3
        uiRequest({ method: "GET", path: "/api/items/" + urlValue(params.id) + "/children", build: "albums" }, "onGrid")
    end if
end sub

sub onGrid(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    page = answer.data
    if page.title <> "" then m.title.text = page.title
    m.line.text = page.line
    m.summary.text = page.summary
    if page.image <> ""
        ' A photo at the left, the text beside it.
        for each label in [m.title, m.line, m.summary]
            label.translation = [248, label.translation[1]]
        end for
        m.photoFrame.visible = true
        m.photo.setHttpAgent(m.top.getScene().getHttpAgent())
        m.photo.uri = page.image
    end if
    focused = m.grid.itemFocused
    m.grid.content = event.getRoSGNode().content
    if focused > 0 and focused < page.shown then m.grid.jumpToItem = focused
    if page.shown = 0
        m.message.text = "Nothing to show here."
        if m.top.params.kind = "playlists" then m.message.text = "No playlists yet. Make one with Add to playlist, the Options key on a title."
        m.message.visible = true
    else
        m.message.visible = false
    end if
end sub

sub onTitleChanged(change as object)
    uiPatchTiles(m.grid.content, change)
end sub

sub onSelected(event as object)
    uiOpenTile(m.grid.content.getChild(event.getData()))
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if press and key = "options" and m.grid.content <> invalid
        uiTileOptions(m.grid.content.getChild(m.grid.itemFocused), false)
        return true
    end if
    return false
end function
