sub init()
    m.name = m.top.findNode("name")
    m.note = m.top.findNode("note")
    m.check = m.top.findNode("check")
    m.fill = m.top.findNode("fill")
    m.name.font = uiType("label")
    m.note.font = uiType("caption")
    m.content = invalid
    m.rowWidth = 608
    m.rowHeight = 88
end sub

' The row's size, the list's: the words between 32 px either side and the
' check, the check in the middle at the right.
sub onLayout()
    if m.top.width > 0 then m.rowWidth = m.top.width
    if m.top.height > 0 then m.rowHeight = m.top.height
    m.fill.width = m.rowWidth
    m.fill.height = m.rowHeight
    m.name.width = m.rowWidth - 120
    m.note.width = m.rowWidth - 120
    m.check.translation = [m.rowWidth - 72, (m.rowHeight - 40) / 2]
    placeWords()
end sub

sub onContent()
    entry = m.top.itemContent
    if entry = invalid then return
    m.content = entry
    m.name.text = entry.title
    note = ""
    if entry.description <> invalid then note = entry.description
    m.note.text = note
    m.note.visible = note <> ""
    placeWords()
    m.check.visible = entry.hasField("checked") and entry.checked = true
    onFocus()
end sub

' With a note, the name sits higher and the note under it.
sub placeWords()
    if m.note.visible
        m.name.translation = [32, m.rowHeight / 2 - 34]
        m.name.height = 40
        m.note.translation = [32, m.rowHeight / 2 + 4]
    else
        m.name.translation = [32, 0]
        m.name.height = m.rowHeight
    end if
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.fill.visible = focused
    if focused
        m.name.color = "0x16191EFF"
        m.note.color = "0x16191ECC"
        m.check.blendColor = "0x16191EFF"
    else
        m.name.color = "0xEBEBEBFF"
        m.note.color = "0xA1A9B6FF"
        m.check.blendColor = "0x38BDF8FF"
    end if
end sub
