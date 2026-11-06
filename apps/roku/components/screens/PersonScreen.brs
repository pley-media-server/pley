' A person (GET /api/people/{id}): their photo and name, what of theirs is
' in the libraries, and a row of their titles per role, newest first. The
' focus starts on the first title. params: { id, title (their name, shown
' until the server answers) }.

sub init()
    m.initials = m.top.findNode("initials")
    m.photo = m.top.findNode("photo")
    m.name = m.top.findNode("name")
    m.line = m.top.findNode("line")
    m.rows = m.top.findNode("rows")
    m.message = m.top.findNode("message")
    m.message.text = t("common.states.loading")
    m.started = false
    m.rows.observeField("rowItemSelected", "onTile")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if m.started
        for each change in uiChangesSince()
            onTitleChanged(change)
        end for
        if m.rows.visible
            m.rows.setFocus(true)
        else
            m.top.setFocus(true)
        end if
        return
    end if
    m.started = true
    uiChangesSince()
    m.name.text = httpSentence(m.top.params.title)
    m.initials.text = pageInitials(m.name.text)
    m.top.setFocus(true)
    uiRequest({ method: "GET", path: "/api/people/" + urlValue(m.top.params.id), build: "person" }, "onPerson")
end sub

sub onTitleChanged(change as object)
    uiPatchTiles(m.rows.content, change)
end sub

' The server answers 404 for someone with nothing this profile may see.
sub onPerson(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        if uiFailed(answer) then return
        m.message.text = answer.message
        if answer.status = 404 then m.message.text = t("titles.person.notFound")
        return
    end if
    person = answer.data
    m.name.text = person.name
    m.initials.text = person.initials
    m.line.text = person.line
    if person.photo <> ""
        m.photo.setHttpAgent(m.top.getScene().getHttpAgent())
        m.photo.uri = person.photo
    end if
    if person.shown = 0
        m.message.text = t("titles.person.nothing")
        return
    end if
    m.message.visible = false
    m.rows.content = event.getRoSGNode().content
    m.rows.visible = true
    ' Only while this screen has the focus: a dialog over it keeps it.
    if m.top.hasFocus() then m.rows.setFocus(true)
end sub

sub onTile(event as object)
    position = event.getData()
    row = m.rows.content.getChild(position[0])
    if row <> invalid then uiOpenTile(row.getChild(position[1]))
end sub

' The Options key on a title offers what can be done with it.
function onKeyEvent(key as string, press as boolean) as boolean
    if press and key = "options" and m.rows.isInFocusChain()
        position = m.rows.rowItemFocused
        row = m.rows.content.getChild(position[0])
        if row <> invalid then uiTileOptions(row.getChild(position[1]), false)
        return true
    end if
    return false
end function
