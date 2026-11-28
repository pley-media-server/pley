sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
    m.background = m.top.findNode("background")
    m.name = m.top.findNode("name")
    m.address = m.top.findNode("address")
    m.tag = m.top.findNode("tag")
    m.warning = false
    m.found = false
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    row = m.top.itemContent
    if row = invalid then return
    m.name.text = row.title
    m.address.text = row.address
    m.tag.text = row.tag
    m.warning = row.warning
    m.found = row.found
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    if focused
        m.background.color = m.c.accent
        m.name.color = m.c.accentFg
        m.address.color = m.c.accentFg
        m.tag.color = m.c.accentFg
        return
    end if
    m.background.color = m.c.surface2
    m.name.color = m.c.fg
    m.address.color = m.c.muted
    if m.warning
        m.tag.color = m.c.danger
    else if m.found
        m.tag.color = m.c.success
    else
        m.tag.color = m.c.muted
    end if
end sub
