sub init()
    m.name = m.top.findNode("name")
    m.note = m.top.findNode("note")
    m.check = m.top.findNode("check")
    m.name.font = uiType("label")
    m.note.font = uiType("caption")
    m.content = invalid
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
    ' With a note, the name sits higher and the note under it.
    if note <> ""
        m.name.translation = [32, 10]
        m.name.height = 40
    else
        m.name.translation = [32, 0]
        m.name.height = 88
    end if
    m.check.visible = entry.hasField("checked") and entry.checked = true
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
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
