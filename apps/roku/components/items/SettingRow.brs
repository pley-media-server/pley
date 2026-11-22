sub init()
    m.pill = m.top.findNode("pill")
    m.icon = m.top.findNode("icon")
    m.label = m.top.findNode("label")
    m.value = m.top.findNode("value")
    m.chevron = m.top.findNode("chevron")
    m.switch = m.top.findNode("switch")
    m.track = m.top.findNode("track")
    m.thumb = m.top.findNode("thumb")
    m.label.font = uiType("body")
    m.value.font = uiType("caption")
    m.danger = false
    m.checked = false
end sub

sub onContent()
    row = m.top.itemContent
    if row = invalid then return
    m.label.text = row.title
    m.value.text = row.description
    m.icon.uri = "pkg:/images/ui/icon-" + row.icon + "-40.png"
    m.danger = row.hasField("danger") and row.danger = true
    m.checked = row.hasField("checked") and row.checked = true
    trailing = ""
    if row.hasField("trailing") then trailing = row.trailing
    m.chevron.visible = trailing = "chevron"
    m.switch.visible = trailing = "switch"
    onLayout()
    onFocus()
end sub

' The label alone in the middle of the row; with a value, the two lines
' together in the middle.
sub onLayout()
    w = m.top.width
    h = m.top.height
    if w <= 0 or h <= 0 then return
    m.pill.width = w
    m.pill.height = h - 8
    m.icon.translation = [28, (h - 40) / 2]
    textWidth = w - 92 - 28
    if m.chevron.visible then textWidth = textWidth - 52
    if m.switch.visible then textWidth = textWidth - 92
    m.label.width = textWidth
    m.value.width = textWidth
    if m.value.text = ""
        m.value.visible = false
        m.label.height = h
        m.label.vertAlign = "center"
        m.label.translation = [92, 0]
    else
        m.value.visible = true
        m.label.height = 38
        m.label.vertAlign = "bottom"
        m.label.translation = [92, h / 2 - 38]
        m.value.height = 34
        m.value.translation = [92, h / 2 + 2]
    end if
    m.chevron.translation = [w - 64, (h - 40) / 2]
    m.switch.translation = [w - 108, (h - 44) / 2]
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.pill.visible = focused
    if focused
        m.label.color = "0x16191EFF"
        m.value.color = "0x16191ECC"
        m.icon.blendColor = "0x16191EFF"
        m.chevron.blendColor = "0x16191EFF"
    else
        m.label.color = "0xEBEBEBFF"
        if m.danger then m.label.color = "0xEB6B70FF"
        m.value.color = "0xA1A9B6FF"
        m.icon.blendColor = "0xA1A9B6FF"
        if m.danger then m.icon.blendColor = "0xEB6B70FF"
        m.chevron.blendColor = "0x8B95A5FF"
    end if
    ' The switch: the accent track with its thumb at the right when on; on
    ' the accent row, dark instead.
    if m.checked
        m.thumb.translation = [42, 6]
        if focused
            m.track.blendColor = "0x16191EFF"
            m.thumb.blendColor = "0x38BDF8FF"
        else
            m.track.blendColor = "0x38BDF8FF"
            m.thumb.blendColor = "0x16191EFF"
        end if
    else
        m.thumb.translation = [6, 6]
        if focused
            m.track.blendColor = "0x16191E66"
            m.thumb.blendColor = "0x16191EFF"
        else
            m.track.blendColor = "0x2E3440FF"
            m.thumb.blendColor = "0xA1A9B6FF"
        end if
    end if
end sub
