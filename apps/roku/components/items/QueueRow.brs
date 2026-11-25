sub init()
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.number = m.top.findNode("number")
    m.playing = m.top.findNode("playing")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.duration = m.top.findNode("duration")
    m.isCurrent = false
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
    m.title.font = uiType("bodyLarge")
    m.subtitle.font = uiType("body")
    m.number.font = uiType("body")
    m.duration.font = uiType("body")
end sub

sub onContent()
    track = m.top.itemContent
    if track = invalid then return
    m.isCurrent = track.hasField("current") and track.current = true
    m.number.text = track.number
    m.number.visible = not m.isCurrent
    m.playing.visible = m.isCurrent
    m.title.text = track.title
    m.subtitle.text = track.subtitle
    m.duration.text = track.duration
    onFocus()
end sub

' The track playing on its ground and in the accent; the focused one on a
' lighter ground with the bar.
sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.bar.visible = focused
    m.background.visible = focused or m.isCurrent
    if focused
        m.background.color = "0x2E3440FF"
        m.title.color = "0x7DD3FCFF"
    else
        m.background.color = "0x1E222AFF"
        m.title.color = "0xEBEBEBFF"
        if m.isCurrent then m.title.color = "0x38BDF8FF"
    end if
end sub

' As wide as the list's items, the duration at the right, 28 px in.
sub onWidth()
    w = m.top.width
    if w <= 0 then return
    m.background.width = w
    m.duration.translation = [w - 28 - m.duration.width, 0]
    m.title.width = w - 112 - 28 - m.duration.width - 16
    m.subtitle.width = m.title.width
end sub
