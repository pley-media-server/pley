sub init()
    m.c = uiPlayerColors()
    uiRecolor(m.top, m.c)
    m.back = m.top.findNode("back")
    m.mark = m.top.findNode("mark")
    m.number = m.top.findNode("number")
    m.plate = m.top.findNode("plate")
    m.initials = m.top.findNode("initials")
    m.logo = m.top.findNode("logo")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.star = m.top.findNode("star")
    m.progress = 0.0
    m.current = false
    ' Logos are often see-through: the initials only where none shows.
    m.logo.observeField("loadStatus", "onLogo")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onLogo()
    m.initials.visible = m.logo.uri = "" or m.logo.loadStatus = "failed"
end sub

sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    m.progress = 0.0
    if item.hasField("progress") then m.progress = item.progress
    m.current = item.hasField("current") and item.current = true
    m.number.text = item.number
    m.title.text = item.title
    m.line.text = item.line
    m.initials.text = Left(item.title, 3)
    m.star.visible = item.favorite = true
    m.logo.setHttpAgent(m.top.getScene().getHttpAgent())
    onLayout()
    m.logo.uri = item.image
    onLogo()
    onFocus()
end sub

sub onLayout()
    w = m.top.width
    h = m.top.height
    if w <= 0 or h <= 0 then return
    m.back.width = w
    m.back.height = h
    m.mark.height = h
    m.number.translation = [0, 0]
    m.number.height = h
    plateY = Int((h - 64) / 2)
    m.plate.translation = [88, plateY]
    m.initials.translation = [88, plateY]
    m.logo.translation = [96, plateY + 6]
    textX = 220
    textWidth = w - textX - 52
    m.title.translation = [textX, Int(h / 2) - 34]
    m.title.width = textWidth
    m.line.translation = [textX, Int(h / 2) + 2]
    m.line.width = textWidth
    m.track.translation = [textX, Int(h / 2) + 32]
    m.track.width = textWidth
    m.fill.translation = [textX, Int(h / 2) + 32]
    m.fill.width = Int(textWidth * m.progress)
    m.track.visible = m.progress > 0
    m.fill.visible = m.progress > 0
    m.star.translation = [w - 48, 0]
    m.star.height = h
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.mark.visible = focused
    if focused
        m.back.color = m.c.surface3
    else if m.current
        m.back.color = themeAlpha(m.c.accent, "26")
    else
        m.back.color = themeAlpha(m.c.bg, "00")
    end if
    if m.current
        m.title.color = m.c.accentHover
    else
        m.title.color = m.c.fg
    end if
end sub
