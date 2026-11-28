sub init()
    ' The pages' colors, a player's once its content says it is in one.
    m.c = uiColors()
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
    m.c = uiItemColors(entry)
    m.content = entry
    m.name.text = entry.title
    ' 简体中文 and 繁體中文 among the languages, in a font that has them.
    m.name.font = uiFontFor(entry.title, "medium", 28)
    note = ""
    if entry.description <> invalid then note = entry.description
    m.note.text = note
    m.note.visible = note <> ""
    placeWords()
    ' A sort's order shows as its arrow where a check would be: Lucide's
    ' rising and falling bars, as the other apps mark it.
    arrow = ""
    if entry.hasField("arrow") then arrow = entry.arrow
    if arrow = "up"
        m.check.uri = "pkg:/images/ui/icon-arrow-up-narrow-wide-40.png"
    else if arrow = "down"
        m.check.uri = "pkg:/images/ui/icon-arrow-down-wide-narrow-40.png"
    else
        m.check.uri = "pkg:/images/ui/icon-check-40.png"
    end if
    m.check.visible = arrow <> "" or (entry.hasField("checked") and entry.checked = true)
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
    m.fill.blendColor = m.c.accent
    if focused
        m.name.color = m.c.accentFg
        m.note.color = themeAlpha(m.c.accentFg, "CC")
        m.check.blendColor = m.c.accentFg
    else
        m.name.color = m.c.fg
        m.note.color = m.c.muted
        m.check.blendColor = m.c.accent
    end if
end sub
