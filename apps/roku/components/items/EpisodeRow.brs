sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.thumb = m.top.findNode("thumb")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.summary = m.top.findNode("summary")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    episode = m.top.itemContent
    if episode = invalid then return
    m.title.text = episode.title
    m.subtitle.text = episode.subtitle
    m.summary.text = episode.summary
    ' The scene's agent carries the token; it changes with each refresh.
    m.thumb.setHttpAgent(m.top.getScene().getHttpAgent())
    m.thumb.uri = episode.image
    progress = 0.0
    if episode.hasField("progress") then progress = episode.progress
    m.fill.width = Int(m.track.width * progress)
    m.track.visible = progress > 0
    m.fill.visible = progress > 0
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.bar.visible = focused
    if focused
        m.background.color = m.c.surface3
        m.title.color = m.c.accentHover
    else
        m.background.color = m.c.surface
        m.title.color = m.c.fg
    end if
end sub

' As wide as the list's items: the words take what the still leaves.
sub onWidth()
    w = m.top.width
    if w <= 0 then return
    m.top.findNode("background").width = w
    for each id in ["title", "subtitle", "summary"]
        m.top.findNode(id).width = w - 364
    end for
end sub
