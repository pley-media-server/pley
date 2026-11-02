sub init()
    m.shown = invalid
    m.shownKind = ""
end sub

' A channel's card or a title's tile, made the first time and again when
' the list hands this item the other kind.
sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    kind = "Tile"
    if item.hasField("channelId") then kind = "LiveCard"
    if kind <> m.shownKind
        if m.shown <> invalid then m.top.removeChild(m.shown)
        m.shown = m.top.createChild(kind)
        m.shownKind = kind
        onSize()
        onFocus()
    end if
    m.shown.itemContent = item
end sub

sub onSize()
    if m.shown = invalid then return
    m.shown.width = m.top.width
    m.shown.height = m.top.height
end sub

sub onFocus()
    if m.shown = invalid then return
    m.shown.itemHasFocus = m.top.itemHasFocus
    m.shown.rowHasFocus = m.top.rowHasFocus
    m.shown.rowListHasFocus = m.top.rowListHasFocus
end sub
