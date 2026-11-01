sub init()
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.body = m.top.findNode("body")
    m.tick = m.top.findNode("tick")
    m.name = m.top.findNode("name")
    m.state = m.top.findNode("state")
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
        m.background.color = "0x2E3440FF"
        m.name.color = "0x7DD3FCFF"
    else
        m.background.color = "0x1E222AFF"
        m.name.color = "0xEBEBEBFF"
    end if
end sub
