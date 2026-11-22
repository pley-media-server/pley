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
    ' A channel's by its liveId: every ContentNode has a channelId on Roku
    ' OS 16, so every result was drawn as a channel.
    if item.hasField("liveId") then kind = "LiveCard"
    if kind <> m.shownKind
        if m.shown <> invalid then m.top.removeChild(m.shown)
        m.shown = m.top.createChild(kind)
        m.shownKind = kind
        onSize()
        onFocus()
    end if
    m.shown.itemContent = item
end sub

' A channel's card takes a Tile's room too, 24 px at its left and 30 over
' it, so the cards and the tiles of other rows line up under their titles.
sub onSize()
    if m.shown = invalid then return
    if m.shownKind = "LiveCard"
        m.shown.translation = [24, 30]
        m.shown.width = m.top.width - 24
        m.shown.height = m.top.height - 30
    else
        m.shown.translation = [0, 0]
        m.shown.width = m.top.width
        m.shown.height = m.top.height
    end if
end sub

sub onFocus()
    if m.shown = invalid then return
    m.shown.itemHasFocus = m.top.itemHasFocus
    m.shown.rowHasFocus = m.top.rowHasFocus
    m.shown.rowListHasFocus = m.top.rowListHasFocus
end sub
