sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.body = m.top.findNode("body")
    m.tick = m.top.findNode("tick")
    m.name = m.top.findNode("name")
    m.state = m.top.findNode("state")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    season = m.top.itemContent
    if season = invalid then return
    m.name.text = season.title
    m.state.text = season.line
    m.tick.visible = season.ticked = true
    m.body.opacity = 1.0
    if season.askable <> true then m.body.opacity = 0.45
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.bar.visible = focused
    if focused
        m.background.color = m.c.surface3
        m.name.color = m.c.accentHover
    else
        m.background.color = m.c.surface
        m.name.color = m.c.fg
    end if
end sub
