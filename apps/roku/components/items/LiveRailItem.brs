sub init()
    ' The pages' colors, a player's once its content says it is in one.
    m.c = uiColors()
    m.back = m.top.findNode("back")
    m.mark = m.top.findNode("mark")
    m.title = m.top.findNode("title")
    m.count = m.top.findNode("count")
    m.inner = false
    m.current = false
    m.heading = false
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    m.c = uiItemColors(item)
    m.inner = item.inner = true
    m.current = item.current = true
    m.heading = item.heading = true
    if m.heading
        ' "REGIONS", "MORE GROUPS": a section's title, set at the bottom.
        m.title.text = UCase(item.title)
        m.title.font = uiFont("bold", 26)
        m.title.vertAlign = "bottom"
    else
        m.title.text = item.title
        m.title.font = uiSystemFont(27)
        m.title.vertAlign = "center"
    end if
    m.count.text = item.countText
    m.mark.text = item.mark
    onLayout()
    onFocus()
end sub

sub onLayout()
    w = m.top.width
    h = m.top.height
    if w <= 0 or h <= 0 then return
    m.back.width = w
    m.back.height = h
    ' The words on the pages' gutter (x 192, the list at 168).
    x = 24
    if m.inner then x = 56
    m.mark.translation = [x - 8, 0]
    m.mark.height = h
    textX = x + 28
    if m.mark.text = "" then textX = x
    m.title.translation = [textX, 0]
    m.title.width = w - textX - 120
    m.title.height = h - 8
    if not m.heading then m.title.height = h
    m.count.translation = [w - 112, 0]
    m.count.width = 96
    m.count.height = h
end sub

sub onFocus()
    if m.heading
        m.back.color = themeAlpha(m.c.bg, "00")
        m.title.color = m.c.subtle
        return
    end if
    focused = m.top.itemHasFocus and m.top.listHasFocus
    if focused
        m.back.color = m.c.accent
        m.title.color = m.c.accentFg
        m.count.color = m.c.accentFg
        m.mark.color = m.c.accentFg
        return
    end if
    if m.current
        m.back.color = themeAlpha(m.c.accent, "26")
        m.title.color = m.c.accentHover
    else
        m.back.color = themeAlpha(m.c.bg, "00")
        m.title.color = m.c.muted
    end if
    m.count.color = m.c.subtle
    m.mark.color = m.c.subtle
end sub
