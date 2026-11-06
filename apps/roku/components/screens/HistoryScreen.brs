' Watch history (GET /api/history), 50 plays at a time, the next as the
' focus nears the end; a play taken out (DELETE /api/history/{id}) goes
' from the list at once.

sub init()
    m.count = m.top.findNode("count")
    m.entries = m.top.findNode("entries")
    m.message = m.top.findNode("message")
    m.message.text = t("common.states.loading")
    m.top.findNode("title").text = t("browse.history.title")
    m.started = false
    m.content = CreateObject("roSGNode", "ContentNode")
    m.entries.content = m.content
    ' Plays listed so far (the next page's offset) and in all.
    m.offset = 0
    m.total = -1
    m.loading = false
    m.dialog = invalid
    ' The row the Options key was pressed on.
    m.removing = -1
    m.entries.observeField("itemSelected", "onEntry")
    m.entries.observeField("itemFocused", "onFocused")
    m.top.observeField("active", "onActive")
end sub

function pageSize() as integer
    return 50
end function

sub onActive()
    if not m.top.active then return
    if not m.started
        m.started = true
        loadMore()
    end if
    if m.content.getChildCount() > 0
        m.entries.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

sub loadMore()
    if m.loading or (m.total >= 0 and m.offset >= m.total) then return
    m.loading = true
    uiRequest({ method: "GET", path: "/api/history", query: { offset: m.offset, limit: pageSize() }, build: "history" }, "onPage")
end sub

sub onPage(event as object)
    m.loading = false
    answer = event.getData()
    if not answer.ok
        if m.content.getChildCount() = 0 and not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.total = answer.data.total
    m.offset = m.offset + answer.data.listed
    if answer.data.listed = 0 then m.total = m.offset
    rows = event.getRoSGNode().content
    if rows <> invalid and rows.getChildCount() > 0 then m.content.appendChildren(rows.getChildren(-1, 0))
    showCount()
    if m.content.getChildCount() = 0
        m.message.text = t("browse.history.empty")
        return
    end if
    m.message.visible = false
    if m.top.hasFocus() then m.entries.setFocus(true)
end sub

sub showCount()
    if m.total >= 0 then m.count.text = t("browse.counts.plays", { count: m.total })
end sub

sub onFocused(event as object)
    if event.getData() >= m.content.getChildCount() - 5 then loadMore()
end sub

sub onEntry(event as object)
    uiOpenTile(m.content.getChild(event.getData()))
end sub

' The Options key: Remove from history.
sub entryOptions()
    entry = m.content.getChild(m.entries.itemFocused)
    if entry = invalid then return
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = entry.title
    dialog.buttons = [t("browse.history.remove"), t("common.actions.cancel")]
    dialog.observeField("buttonSelected", "onEntryOption")
    m.removing = m.entries.itemFocused
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onEntryOption(event as object)
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
    if event.getData() <> 0 then return
    entry = m.content.getChild(m.removing)
    if entry = invalid then return
    uiRequest({ method: "DELETE", path: "/api/history/" + urlValue(entry.historyId), historyId: entry.historyId }, "onRemoved")
end sub

sub onRemoved(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then uiMessage(t("common.states.failed"), [answer.message])
        return
    end if
    ' By its id: rows taken out meanwhile moved the others.
    gone = event.getRoSGNode().spec.historyId
    for i = m.content.getChildCount() - 1 to 0 step -1
        if m.content.getChild(i).historyId = gone then m.content.removeChildIndex(i)
    end for
    m.total = m.total - 1
    m.offset = m.offset - 1
    showCount()
    if m.content.getChildCount() = 0
        m.message.text = t("browse.history.empty")
        m.message.visible = true
        m.top.setFocus(true)
    end if
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if press and key = "options" and m.content.getChildCount() > 0
        entryOptions()
        return true
    end if
    return false
end function
