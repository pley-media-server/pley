' The shell's rail (Rail.xml): its entries, the current one marked, open
' while it has the focus.

sub init()
    m.list = m.top.findNode("list")
    m.panel = m.top.findNode("panel")
    m.opening = m.top.findNode("opening")
    m.closing = m.top.findNode("closing")
    m.keys = []
    m.entries = []
    m.open = false
    m.list.observeField("itemSelected", "onSelected")
    m.top.observeField("focusedChild", "onFocus")
end sub

' An entry's key: its action, a library's with its id.
function railKey(entry as object) as string
    if entry.action = "library" and entry.library <> invalid then return "library:" + urlValue(entry.library.id)
    return entry.action
end function

' The entries as nodes, the current one marked; while it is open, the
' focus stays on its entry as others come or go above it (Discover once
' the requests' summary comes, Live TV, libraries).
sub onEntries()
    focusedKey = ""
    focused = m.list.itemFocused
    if focused >= 0 and focused < m.keys.Count() then focusedKey = m.keys[focused]
    m.entries = httpArray(m.top.entries)
    m.keys = []
    items = []
    for each entry in m.entries
        key = railKey(entry)
        m.keys.Push(key)
        item = {}
        item.Append(entry)
        item.Delete("library")
        item.current = key = m.top.current
        items.Push(item)
    end for
    m.list.content = nodesContent(items)
    ' Closed, it shows the current entry as it would once closed.
    if not m.open
        showClosed()
        return
    end if
    if focusedKey = "" then focusedKey = m.top.current
    at = navMenuFocus(m.keys, focusedKey, focused)
    if at > 0 then m.list.jumpToItem = at
end sub

sub onCurrent()
    content = m.list.content
    if content = invalid then return
    for i = 0 to m.keys.Count() - 1
        node = content.getChild(i)
        if node <> invalid then node.current = m.keys[i] = m.top.current
    end for
    if not m.open then showClosed()
end sub

' Closed, the rail shows from its top, or as far down as the current
' entry needs (Settings, under a long list of libraries), whatever was
' looked at while it was open. One jump: of two in a row the Roku keeps the
' last, scrolled as little as it needs.
sub showClosed()
    at = -1
    for i = 0 to m.keys.Count() - 1
        if m.keys[i] = m.top.current then at = i
    end for
    if at < 0 then return
    if at < m.list.numRows then at = 0
    if m.list.itemFocused <> at then m.list.jumpToItem = at
end sub

sub onSelected(event as object)
    at = event.getData()
    if at >= 0 and at < m.entries.Count() then m.top.chosen = m.entries[at]
end sub

' Open while it has the focus: on the current entry as it comes.
sub onFocus()
    ' The scene focuses the rail itself: its list takes the focus.
    if m.top.hasFocus() then m.list.setFocus(true)
    open = m.top.isInFocusChain()
    if open = m.open then return
    m.open = open
    if open
        for i = 0 to m.keys.Count() - 1
            if m.keys[i] = m.top.current then m.list.jumpToItem = i
        end for
        m.closing.control = "stop"
        m.opening.control = "start"
    else
        m.opening.control = "stop"
        m.closing.control = "start"
        showClosed()
    end if
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    ' Back on Home's rail leaves the app (the scene's Back on its first
    ' screen); elsewhere it goes back to the page, as Right does.
    if key = "back" and m.top.home then return false
    if key = "right" or key = "back"
        m.top.leave = true
        return true
    end if
    ' Left at the screen's edge stays.
    return key = "left"
end function
