sub init()
    m.pill = m.top.findNode("pill")
    m.mark = m.top.findNode("mark")
    m.icon = m.top.findNode("icon")
    m.name = m.top.findNode("name")
    m.countBack = m.top.findNode("countBack")
    m.count = m.top.findNode("count")
    m.avatar = m.top.findNode("avatar")
    m.initial = m.top.findNode("initial")
    m.current = false
    m.danger = false
end sub

sub onContent()
    entry = m.top.itemContent
    if entry = invalid then return
    m.name.text = entry.title
    ' The profile: its initial in a circle, where the others have an icon.
    initial = ""
    if entry.hasField("initial") then initial = entry.initial
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
        m.pill.blendColor = "0x38BDF8FF"
        m.pill.width = w - 48
        m.pill.translation = [24, 0]
        m.icon.blendColor = "0x16191EFF"
        m.name.color = "0x16191EFF"
    else if m.current and open
        m.pill.visible = true
        m.pill.blendColor = "0x2E3440FF"
        m.pill.width = w - 48
        m.pill.translation = [24, 0]
        m.icon.blendColor = "0xEBEBEBFF"
        m.name.color = "0xEBEBEBFF"
    else
        m.pill.visible = false
        m.name.color = "0xEBEBEBFF"
        if m.current
            m.icon.blendColor = "0xEBEBEBFF"
        else
            m.icon.blendColor = "0xA1A9B6FF"
        end if
    end if
    ' On the accent pill, an accent number is dark instead.
    if m.danger
        m.countBack.blendColor = "0xEB6B70FF"
        m.count.color = "0x16191EFF"
    else if focused
        m.countBack.blendColor = "0x16191EFF"
        m.count.color = "0x7DD3FCFF"
    else
        m.countBack.blendColor = "0x38BDF8FF"
        m.count.color = "0x16191EFF"
    end if
end sub
