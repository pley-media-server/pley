' Sort libraries: the profile's own order (LibraryOrder.brs), saved at
' each drop. The library lifted follows the list's focus, up and down, its
' title marked; the titles change in place, so focus moves only with keys.

sub init()
    m.list = m.top.findNode("list")
    m.buttons = m.top.findNode("buttons")
    m.hint = m.top.findNode("hint")
    m.message = m.top.findNode("message")
    m.top.findNode("title").text = t("browse.sortLibraries.title")
    m.libraries = []
    ' The library lifted (its place now), -1 for none, and the order before.
    m.moving = -1
    m.before = []
    m.saving = false
    ' A server from before the profile's own order: nothing moves.
    m.older = false
    m.buttons.buttons = [t("browse.sortLibraries.serverOrder"), t("common.actions.done")]
    m.buttons.observeField("buttonSelected", "onButton")
    m.list.observeField("itemSelected", "onSelected")
    m.list.observeField("itemFocused", "onFocused")
    m.top.observeField("active", "onActive")
    hint()
end sub

sub onActive()
    if not m.top.active then return
    m.list.setFocus(true)
    uiGet("/api/libraries", "onLibraries")
end sub

sub onLibraries(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    place(orderLibraries(answer.data))
end sub

' The list as the server has it.
sub place(libraries as object)
    m.libraries = libraries
    m.moving = -1
    items = []
    for each library in libraries
        items.Push({ title: library.name })
    end for
    focused = m.list.itemFocused
    m.list.content = nodesContent(items)
    if focused > 0 and focused < items.Count() then m.list.jumpToItem = focused
    hint()
end sub

' Each title as its library is now, the one lifted marked.
sub retitle()
    content = m.list.content
    if content = invalid then return
    for i = 0 to m.libraries.Count() - 1
        node = content.getChild(i)
        if node <> invalid
            title = m.libraries[i].name
            if i = m.moving then title = t("browse.sortLibraries.moving", { name: title })
            node.title = title
        end if
    end for
end sub

sub hint()
    if m.moving >= 0
        m.hint.text = t("browse.sortLibraries.movingHint")
    else
        m.hint.text = t("browse.sortLibraries.hint")
    end if
end sub

' OK on a library: lifts it, or puts the one lifted where it is.
sub onSelected(event as object)
    if m.older or m.saving then return
    if m.moving < 0
        m.moving = event.getData()
        m.before = m.libraries
        m.message.text = ""
    else
        m.moving = -1
        if FormatJson(orderIds(m.libraries)) <> FormatJson(orderIds(m.before)) then save()
    end if
    retitle()
    hint()
end sub

' Focus moved while a library is lifted: it goes along.
sub onFocused(event as object)
    if m.moving < 0 then return
    at = event.getData()
    if at = m.moving or at < 0 or at >= m.libraries.Count() then return
    m.libraries = orderMoved(m.libraries, m.moving, at)
    m.moving = at
    retitle()
end sub

sub save()
    m.saving = true
    uiRequest({ method: "PUT", path: "/api/me/libraries/order", json: { ids: orderIds(m.libraries) } }, "onSaved")
end sub

sub onButton(event as object)
    if event.getData() = 1
        uiNavigate({ back: true })
        return
    end if
    if m.older or m.saving then return
    m.saving = true
    m.before = m.libraries
    uiRequest({ method: "DELETE", path: "/api/me/libraries/order" }, "onSaved")
end sub

' The server's answer is the list; Home asks again when it shows.
sub onSaved(event as object)
    m.saving = false
    answer = event.getData()
    if answer.ok
        place(orderLibraries(answer.data))
        scene = m.top.getScene()
        scene.librariesMoved = scene.librariesMoved + 1
        return
    end if
    if uiFailed(answer)
        place(m.before)
        return
    end if
    refusal = orderRefusal(answer.status)
    if refusal = "older"
        m.older = true
        m.message.text = t("browse.sortLibraries.needsNewer")
        place(m.before)
    else if refusal = "changed"
        ' A library came or went since they were listed: as they are now.
        uiGet("/api/libraries", "onLibraries")
    else
        m.message.text = answer.message
        place(m.before)
    end if
end sub

' Back puts a lifted library back; right and left go between the list and
' the buttons.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.moving >= 0
        if key = "back"
            lifted = m.libraries[m.moving]
            m.libraries = m.before
            m.moving = -1
            retitle()
            hint()
            for i = 0 to m.libraries.Count() - 1
                if m.libraries[i].id = lifted.id then m.list.jumpToItem = i
            end for
            return true
        end if
        ' Nowhere else while one is lifted.
        return key = "left" or key = "right"
    end if
    if key = "right" and m.list.hasFocus()
        m.buttons.setFocus(true)
        return true
    end if
    if key = "left" and m.buttons.isInFocusChain()
        m.list.setFocus(true)
        return true
    end if
    return false
end function
