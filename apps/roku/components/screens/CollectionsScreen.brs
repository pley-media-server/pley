' Collections (GET /api/libraries/{id}/collections for each library of
' movies or shows): a row each, named after its library, in the libraries'
' order, once every library answered; a library without collections has no
' row. params: { libraries ([{ id, name, kind }], Home's menu's) }.

sub init()
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.message.text = t("common.states.loading")
    m.top.findNode("title").text = t("browse.menu.collections")
    m.started = false
    m.rows.observeField("rowItemSelected", "onTile")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if m.started
        if m.rows.visible
            m.rows.setFocus(true)
        else
            m.top.setFocus(true)
        end if
        return
    end if
    m.started = true
    m.top.setFocus(true)
    m.libraries = []
    for each library in httpArray(m.top.params.libraries)
        if GetInterface(library, "ifAssociativeArray") <> invalid and (library.kind = "movies" or library.kind = "shows") then m.libraries.Push(library)
    end for
    ' Each library's collections as they come ({ id: ContentNode }), and
    ' how many answers are still to come.
    m.found = {}
    m.waiting = m.libraries.Count()
    if m.waiting = 0
        m.message.text = t("browse.collections.none")
        return
    end if
    for each library in m.libraries
        uiRequest({ method: "GET", path: "/api/libraries/" + urlValue(library.id) + "/collections", build: "collections", libraryId: library.id }, "onCollections")
    end for
end sub

sub onCollections(event as object)
    request = event.getRoSGNode()
    answer = event.getData()
    m.waiting = m.waiting - 1
    if not answer.ok
        ' Handled (the page goes for Unreachable or sign-in) unless the page
        ' was covered meanwhile: then it says why once every answer is in.
        if uiFailed(answer) and m.top.active then return
        if m.failed = invalid then m.failed = answer.message
    end if
    if answer.ok and request.content <> invalid and request.content.getChildCount() > 0 then m.found[urlValue(request.spec.libraryId)] = request.content
    if m.waiting > 0 then return
    content = CreateObject("roSGNode", "ContentNode")
    for each library in m.libraries
        tiles = m.found[urlValue(library.id)]
        if tiles <> invalid
            row = content.createChild("ContentNode")
            row.title = library.name
            row.appendChildren(tiles.getChildren(-1, 0))
        end if
    end for
    if content.getChildCount() = 0
        m.message.text = t("browse.collections.none")
        if m.failed <> invalid then m.message.text = m.failed
        return
    end if
    m.message.visible = false
    m.rows.content = content
    m.rows.visible = true
    ' Only while this screen has the focus: a dialog over it keeps it.
    if m.top.hasFocus() then m.rows.setFocus(true)
end sub

sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row <> invalid then uiOpenTile(row.getChild(position[1]))
end sub
