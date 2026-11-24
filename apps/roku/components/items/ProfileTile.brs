sub init()
    m.ring = m.top.findNode("ring")
    m.face = m.top.findNode("face")
    m.initial = m.top.findNode("initial")
    m.picture = m.top.findNode("picture")
    m.name = m.top.findNode("name")
    m.needs = m.top.findNode("needs")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    profile = m.top.itemContent
    if profile = invalid then return
    m.face.blendColor = profile.color
    m.initial.text = profile.initial
    m.name.text = profile.title
    m.needs.text = profile.subtitle
    ' The scene's agent carries the token, which a household's pictures need.
    m.picture.setHttpAgent(m.top.getScene().getHttpAgent())
    m.picture.uri = profile.image
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.gridHasFocus
    m.ring.visible = focused
    if focused
        m.name.color = "0xEBEBEBFF"
    else
        m.name.color = "0xA1A9B6FF"
    end if
end sub
