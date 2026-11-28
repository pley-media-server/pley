sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
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
    m.category = false
    m.heard = invalid
end sub

sub onContent()
    row = m.top.itemContent
    if row = invalid then return
    m.label.text = row.title
    m.value.text = row.description
    m.icon.uri = "pkg:/images/ui/icon-" + row.icon + "-40.png"
    m.danger = row.hasField("danger") and row.danger = true
    m.checked = row.hasField("checked") and row.checked = true
    m.category = row.hasField("isCategory") and row.isCategory = true
    ' A list's item hears no change to its content's fields: held, which
    ' the screen sets as the focus comes and goes, is listened to.
    if m.heard <> invalid then m.heard.unobserveFieldScoped("held")
    m.heard = invalid
    if m.category
        row.observeFieldScoped("held", "onFocus")
        m.heard = row
    end if
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
    if m.chevron.visible or m.category then textWidth = textWidth - 52
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
    row = m.top.itemContent
    held = m.category and not focused and row <> invalid and row.held = true
    m.pill.visible = focused or held
    if m.category then m.chevron.visible = focused or held
    if held
        m.pill.blendColor = m.c.surface2
    else
        m.pill.blendColor = m.c.accent
    end if
    if focused
        m.label.color = m.c.accentFg
        m.value.color = themeAlpha(m.c.accentFg, "CC")
        m.icon.blendColor = m.c.accentFg
        m.chevron.blendColor = m.c.accentFg
    else
        m.label.color = m.c.fg
        if m.danger then m.label.color = m.c.danger
        m.value.color = m.c.muted
        m.icon.blendColor = m.c.muted
        if m.danger then m.icon.blendColor = m.c.danger
        m.chevron.blendColor = m.c.subtle
    end if
    ' The switch: the accent track with its thumb at the right when on; on
    ' the accent row, dark instead.
    if m.checked
        m.thumb.translation = [42, 6]
        if focused
            m.track.blendColor = m.c.accentFg
            m.thumb.blendColor = m.c.accent
        else
            m.track.blendColor = m.c.accent
            m.thumb.blendColor = m.c.accentFg
        end if
    else
        m.thumb.translation = [6, 6]
        if focused
            m.track.blendColor = themeAlpha(m.c.accentFg, "66")
            m.thumb.blendColor = m.c.accentFg
        else
            m.track.blendColor = m.c.surface3
            m.thumb.blendColor = m.c.muted
        end if
    end if
end sub
