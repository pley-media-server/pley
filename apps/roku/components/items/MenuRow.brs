sub init()
    m.focus = m.top.findNode("focus")
    m.name = m.top.findNode("name")
    m.countBack = m.top.findNode("countBack")
    m.count = m.top.findNode("count")
    m.danger = false
end sub

sub onContent()
    entry = m.top.itemContent
    if entry = invalid then return
    m.name.text = entry.title
    count = 0
    if entry.hasField("count") then count = entry.count
    m.danger = entry.hasField("danger") and entry.danger = true
    shown = count > 0
    m.count.visible = shown
    m.countBack.visible = shown
    m.name.width = 252
    if shown
        text = count.ToStr()
        if count > 99 then text = "99+"
        m.count.text = text
        m.count.width = 0
        boxWidth = m.count.boundingRect().width + 20
        if boxWidth < 40 then boxWidth = 40
        m.count.width = boxWidth
        m.count.translation = [276 - boxWidth, 16]
        m.countBack.width = boxWidth
        m.countBack.translation = [276 - boxWidth, 16]
        m.name.width = 252 - boxWidth - 16
    end if
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.focus.visible = focused
    if focused
        m.name.color = "0x16191EFF"
        m.name.font = "font:MediumBoldSystemFont"
    else
        m.name.color = "0xEBEBEBFF"
        m.name.font = "font:MediumSystemFont"
    end if
    ' On the accent box, an accent number is dark instead.
    if m.danger
        m.countBack.color = "0xEB6B70FF"
        m.count.color = "0x16191EFF"
    else if focused
        m.countBack.color = "0x16191EFF"
        m.count.color = "0x7DD3FCFF"
    else
        m.countBack.color = "0x38BDF8FF"
        m.count.color = "0x16191EFF"
    end if
end sub
