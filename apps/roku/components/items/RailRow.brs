sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
    m.pill = m.top.findNode("pill")
    m.mark = m.top.findNode("mark")
    m.icon = m.top.findNode("icon")
    m.name = m.top.findNode("name")
    m.countBack = m.top.findNode("countBack")
    m.count = m.top.findNode("count")
    m.avatar = m.top.findNode("avatar")
    m.initial = m.top.findNode("initial")
    m.frame = m.top.findNode("frame")
    m.picture = m.top.findNode("picture")
    m.current = false
    m.danger = false
end sub

sub onContent()
    entry = m.top.itemContent
    if entry = invalid then return
    m.name.text = entry.title
    ' The profile: its avatar, where the others have an icon; its picture
    ' over its initial on its color, which shows while the picture loads,
    ' or when it fails, in the profile's colors whatever the theme.
    initial = ""
    if entry.hasField("initial") then initial = entry.initial
    picture = ""
    if initial <> ""
        m.avatar.blendColor = entry.color
        m.initial.color = entry.ink
        picture = entry.image
    end if
    if picture <> m.picture.uri
        ' The scene's agent carries the token, which the picture needs.
        if picture <> "" then m.picture.setHttpAgent(m.top.getScene().getHttpAgent())
        m.picture.uri = picture
    end if
    m.frame.visible = picture <> ""
    ' Set with each entry, so a language that changed the family of type
    ' (the rail's entries come again in it) shows in its fonts; a library's
    ' or a profile's name in Chinese has a font with its characters.
    m.name.font = uiFontFor(entry.title, "medium", 28)
    m.initial.font = uiFontFor(initial, "bold", 32)
    m.count.font = uiType("badge")
    m.avatar.visible = initial <> ""
    m.initial.visible = initial <> ""
    m.icon.visible = initial = ""
    m.initial.text = initial
    if initial = "" then m.icon.uri = "pkg:/images/ui/icon-" + entry.icon + "-40.png"
    m.current = entry.hasField("current") and entry.current = true
    count = 0
    if entry.hasField("count") then count = entry.count
    m.danger = entry.hasField("danger") and entry.danger = true
    m.count.visible = count > 0
    m.countBack.visible = count > 0
    if count > 0
        text = count.ToStr()
        if count > 99 then text = "99+"
        m.count.text = text
        m.count.width = 0
        boxWidth = m.count.boundingRect().width + 20
        if boxWidth < 40 then boxWidth = 40
        m.count.width = boxWidth
        m.countBack.width = boxWidth
        m.countBoxWidth = boxWidth
    end if
    onWidth()
    onFocus()
end sub

' The pill as tall as the entry, the icon, the circle, the mark and the
' number in its middle.
sub onHeight()
    h = m.top.height
    if h <= 0 then return
    m.pill.height = h
    m.name.height = h
    m.mark.translation = [0, (h - 40) / 2]
    m.icon.translation = [52, (h - 40) / 2]
    m.avatar.translation = [44, (h - 56) / 2]
    m.initial.translation = [44, (h - 56) / 2]
    m.frame.translation = [44, (h - 56) / 2]
    onWidth()
end sub

' The number at the open rail's right edge; collapsed, a dot of it on the
' icon's corner.
sub onWidth()
    if not m.count.visible then return
    w = m.top.width
    if w <= 0 then w = 456
    y = 20
    if m.top.height > 0 then y = (m.top.height - 40) / 2
    x = w - 32 - m.countBoxWidth
    m.count.translation = [x, y]
    m.countBack.translation = [x, y]
end sub

sub onFocus()
    open = m.top.listHasFocus
    focused = m.top.itemHasFocus and open
    m.name.opacity = 0
    if open then m.name.opacity = 1
    m.count.opacity = m.name.opacity
    m.countBack.opacity = m.name.opacity
    m.mark.visible = m.current and not open
    w = m.top.width
    if w <= 0 then w = 456
    if focused
        m.pill.visible = true
        m.pill.blendColor = m.c.accent
        m.pill.width = w - 48
        m.pill.translation = [24, 0]
        m.icon.blendColor = m.c.accentFg
        m.name.color = m.c.accentFg
    else if m.current and open
        m.pill.visible = true
        m.pill.blendColor = m.c.surface3
        m.pill.width = w - 48
        m.pill.translation = [24, 0]
        m.icon.blendColor = m.c.fg
        m.name.color = m.c.fg
    else
        m.pill.visible = false
        m.name.color = m.c.fg
        if m.current
            m.icon.blendColor = m.c.fg
        else
            m.icon.blendColor = m.c.muted
        end if
    end if
    ' On the accent pill, an accent number is dark instead.
    if m.danger
        m.countBack.blendColor = m.c.danger
        m.count.color = m.c.dangerFg
    else if focused
        m.countBack.blendColor = m.c.accentFg
        m.count.color = m.c.accentHover
    else
        m.countBack.blendColor = m.c.accent
        m.count.color = m.c.accentFg
    end if
end sub
