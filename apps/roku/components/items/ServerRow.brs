sub init()
    m.background = m.top.findNode("background")
    m.name = m.top.findNode("name")
    m.address = m.top.findNode("address")
    m.tag = m.top.findNode("tag")
    m.warning = false
    m.found = false
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onContent()
    row = m.top.itemContent
    if row = invalid then return
    m.name.text = row.title
    m.address.text = row.address
    m.tag.text = row.tag
    m.warning = row.warning
    m.found = row.found
    onFocus()
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    if focused
        m.background.color = "0x38BDF8FF"
        m.name.color = "0x16191EFF"
        m.address.color = "0x16191EFF"
        m.tag.color = "0x16191EFF"
        return
    end if
    m.background.color = "0x262B35FF"
    m.name.color = "0xEBEBEBFF"
    m.address.color = "0xA1A9B6FF"
    if m.warning
        m.tag.color = "0xEB6B70FF"
    else if m.found
        m.tag.color = "0x3FB27FFF"
    else
        m.tag.color = "0xA1A9B6FF"
    end if
end sub
