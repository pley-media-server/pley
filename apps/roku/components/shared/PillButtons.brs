sub init()
    m.c = uiColors()
    m.top.focusable = true
    m.pills = []
    m.index = 0
    m.top.observeField("focusedChild", "onLook")
end sub

' Each button a pill: as wide as its words and 32 px either side, at least
' minWidth and at most maxWidth; in a column all as wide as the widest.
sub onButtons()
    ' In a player's colors there (dark).
    m.c = uiColorsOf(m.top.dark)
    m.top.removeChildrenIndex(m.top.getChildCount(), 0)
    m.pills = []
    labels = m.top.buttons
    if labels = invalid then labels = []
    height = m.top.buttonHeight
    if height <= 0 then height = 72
    row = m.top.layoutDirection = "horiz"
    gap = 16
    if row then gap = 20
    spacings = m.top.itemSpacings
    if spacings <> invalid and spacings.Count() > 0 then gap = spacings[0]
    widths = []
    widest = 0
    for each words in labels
        label = CreateObject("roSGNode", "Label")
        label.font = uiFontFor(words, "medium", 28)
        label.text = words
        width = Int(label.boundingRect().width) + 2 + 64
        if width < m.top.minWidth then width = m.top.minWidth
        if m.top.maxWidth > 0 and width > m.top.maxWidth then width = m.top.maxWidth
        widths.Push(width)
        if width > widest then widest = width
    end for
    at = 0
    for i = 0 to labels.Count() - 1
        width = widths[i]
        if not row then width = widest
        group = m.top.createChild("Group")
        back = group.createChild("Poster")
        back.uri = "pkg:/images/ui/r12.9.png"
        back.width = width
        back.height = height
        edge = group.createChild("Poster")
        edge.uri = "pkg:/images/ui/edge.9.png"
        edge.blendColor = m.c.borderStrong
        edge.width = width
        edge.height = height
        label = group.createChild("Label")
        label.font = uiFontFor(labels[i], "medium", 28)
        label.text = labels[i]
        label.translation = [32, 0]
        label.width = width - 64
        label.height = height
        label.horizAlign = "center"
        label.vertAlign = "center"
        label.maxLines = 1
        if row
            group.translation = [at, 0]
            at = at + width + gap
        else
            group.translation = [0, at]
            at = at + height + gap
        end if
        m.pills.Push({ back: back, edge: edge, label: label })
    end for
    ' New buttons start from the first, as a ButtonGroup's do.
    m.index = 0
    m.top.buttonFocused = 0
    onLook()
end sub

sub onFocusButton()
    index = m.top.focusButton
    if index < 0 or index >= m.pills.Count() then return
    m.index = index
    m.top.buttonFocused = index
    onLook()
end sub

' Focused, the accent fills the pill and its edge goes, its words dark on
' it; the others are the control's edge around the text color at 12 %.
sub onLook()
    focused = m.top.hasFocus()
    for i = 0 to m.pills.Count() - 1
        pill = m.pills[i]
        lit = focused and i = m.index
        if lit
            pill.back.blendColor = m.c.accent
            pill.label.color = m.c.accentFg
        else
            pill.back.blendColor = themeAlpha(m.c.fg, "1F")
            pill.label.color = m.c.fg
        end if
        pill.edge.visible = not lit
    end for
end sub

' Up and Down in a column, Left and Right in a row, from one to the next;
' past the ends the screen has the key. OK chooses.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press or m.pills.Count() = 0 then return false
    if key = "OK"
        m.top.buttonSelected = m.index
        return true
    end if
    back = "up"
    forward = "down"
    if m.top.layoutDirection = "horiz"
        back = "left"
        forward = "right"
    end if
    index = m.index
    if key = back then index = index - 1
    if key = forward then index = index + 1
    if index = m.index or index < 0 or index >= m.pills.Count() then return false
    m.index = index
    m.top.buttonFocused = index
    onLook()
    return true
end function
