sub init()
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.duration = m.top.findNode("duration")
    ' A disc's heading (kind "disc"; QueueRow has none): its words alone,
    ' never drawn focused.
    m.heading = m.top.findNode("heading")
    m.isHeading = false
end sub

sub onContent()
    track = m.top.itemContent
    if track = invalid then return
    m.isHeading = m.heading <> invalid and track.hasField("kind") and track.kind = "disc"
    if m.heading <> invalid
        m.heading.visible = m.isHeading
        m.heading.text = ""
        if m.isHeading then m.heading.text = track.title
    end if
    m.background.visible = not m.isHeading
    m.title.visible = not m.isHeading
    m.title.text = track.title
    m.subtitle.text = track.subtitle
    m.duration.text = track.duration
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus and not m.isHeading
    m.bar.visible = focused
    if focused
        m.background.color = "0x2E3440FF"
        m.title.color = "0x7DD3FCFF"
    else
        m.background.color = "0x1E222AFF"
        m.title.color = "0xEBEBEBFF"
    end if
end sub
