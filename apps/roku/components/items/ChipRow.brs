sub init()
    m.top.focusable = true
    m.chipNodes = []
    m.top.observeField("focusedChild", "onLook")
end sub

' Chips side by side, as the Android TV app draws them (the look says
' which): a page's buttons are pills (fg at 12 % inside the control's
' edge; an icon alone makes a round, square one), a library's sort and
' filters chips (surface3), its tabs words alone, the tab shown on
' surface3. A chip of width 0 is as wide as its icon, words and number,
' with room either side. A chip's number (count, "" for none) is a small
' box after its words.
sub onChips()
    m.top.removeChildrenIndex(m.top.getChildCount(), 0)
    m.chipNodes = []
    look = m.top.look
    height = 72
    gap = 16
    if look = "buttons" then gap = 20
    x = 0
    for each chip in m.top.chips
        group = m.top.createChild("Group")
        back = group.createChild("Poster")
        back.uri = "pkg:/images/ui/r12.9.png"
        edge = group.createChild("Poster")
        edge.uri = "pkg:/images/ui/edge.9.png"
        edge.blendColor = "0x6C7584FF"
        edge.visible = look = "buttons"
        icon = invalid
        if chip.DoesExist("icon") and chip.icon <> ""
            icon = group.createChild("Poster")
            icon.uri = "pkg:/images/ui/icon-" + chip.icon + "-40.png"
            icon.width = 40
            icon.height = 40
        end if
        label = group.createChild("Label")
        label.font = uiType("label")
        label.text = chip.label
        label.height = height
        label.vertAlign = "center"
        number = invalid
        if chip.DoesExist("count") and chip.count <> "" then number = makeNumber(group, chip)
        words = 0
        if chip.label <> "" then words = Int(label.boundingRect().width) + 2
        inner = words
        if icon <> invalid
            inner = inner + 40
            if words > 0 then inner = inner + 16
        end if
        if number <> invalid then inner = inner + number.gap + number.width
        width = chip.width
        round = icon <> invalid and chip.label = ""
        if width <= 0 then width = inner + m.top.pad
        ' An icon alone: a circle as tall as the row, as Android TV's round
        ' buttons.
        if round
            width = height
            back.uri = "pkg:/images/ui/circle.png"
            edge.uri = "pkg:/images/ui/edge-round.png"
        end if
        group.translation = [x, 0]
        group.scaleRotateCenter = [width / 2, height / 2]
        back.width = width
        back.height = height
        edge.width = width
        edge.height = height
        ' The icon, the words and the number together in the middle, the
        ' words cut short in a chip too narrow for them.
        room = width - m.top.pad
        if icon <> invalid then room = room - 40 - 16
        if number <> invalid then room = room - number.gap - number.width
        if round then room = 0
        if words > room then words = room
        if words < 0 then words = 0
        content = words
        if icon <> invalid
            content = content + 40
            if words > 0 then content = content + 16
        end if
        if number <> invalid then content = content + number.gap + number.width
        start = Int((width - content) / 2)
        if icon <> invalid
            icon.translation = [start, (height - 40) / 2]
            start = start + 40
            if words > 0 then start = start + 16
        end if
        label.translation = [start, 0]
        label.width = words
        label.visible = words > 0
        if number <> invalid
            number.back.translation = [start + words + number.gap, 18]
            number.text.translation = [start + words + number.gap, 18]
        end if
        ' A round button's name, under it while it has the focus.
        caption = invalid
        if round and chip.DoesExist("name") and chip.name <> ""
            caption = m.top.createChild("Label")
            caption.font = uiType("caption")
            caption.color = "0xEBEBEBFF"
            caption.text = chip.name
            captionWidth = Int(caption.boundingRect().width) + 2
            captionX = x + Int((width - captionWidth) / 2)
            if captionX < 0 then captionX = 0
            caption.translation = [captionX, height + 16]
            caption.visible = false
        end if
        m.chipNodes.Push({ group: group, back: back, edge: edge, icon: icon, label: label, number: number, caption: caption })
        x = x + width + gap
    end for
    m.top.rowWidth = x - gap
    if x = 0 then m.top.rowWidth = 0
    if m.top.focusIndex >= m.chipNodes.Count() then m.top.focusIndex = 0
    onLook()
end sub

' A chip's number: { back, text, width, gap (from the words), tone }. Where
' it asks to be seen, in a box in the accent (accent) or the danger color
' (danger), as the rail's; else plain, in bold after the words, as a
' row of eight tabs has room for.
function makeNumber(group as object, chip as object) as object
    back = group.createChild("Poster")
    back.uri = "pkg:/images/ui/r12.9.png"
    text = group.createChild("Label")
    text.font = uiType("badge")
    text.text = chip.count
    tone = "plain"
    if chip.DoesExist("accent") and chip.accent = true then tone = "accent"
    if chip.DoesExist("danger") and chip.danger = true then tone = "danger"
    width = Int(text.boundingRect().width) + 2
    gap = 10
    if tone <> "plain"
        width = width + 18
        if width < 36 then width = 36
        gap = 12
    end if
    back.width = width
    back.height = 36
    back.visible = tone <> "plain"
    text.width = width
    text.height = 36
    text.horizAlign = "center"
    text.vertAlign = "center"
    return { back: back, text: text, width: width, gap: gap, tone: tone }
end function

' Focused, the accent fills the chip, its edge goes and it grows to 1.05;
' its icon and words go dark on it.
sub onLook()
    focused = m.top.hasFocus()
    look = m.top.look
    for i = 0 to m.chipNodes.Count() - 1
        chip = m.chipNodes[i]
        lit = focused and i = m.top.focusIndex
        chip.back.visible = true
        if lit
            chip.back.blendColor = "0x38BDF8FF"
            words = "0x16191EFF"
        else if look = "buttons"
            chip.back.blendColor = "0xEBEBEB1F"
            words = "0xEBEBEBFF"
        else if i = m.top.current
            chip.back.blendColor = "0x2E3440FF"
            words = "0xEBEBEBFF"
        else if look = "tabs"
            chip.back.visible = false
            words = "0xA1A9B6FF"
        else
            chip.back.blendColor = "0x2E3440FF"
            words = "0xEBEBEBFF"
        end if
        chip.edge.visible = look = "buttons" and not lit
        chip.label.color = words
        if chip.icon <> invalid then chip.icon.blendColor = words
        if lit
            chip.group.scale = [1.05, 1.05]
        else
            chip.group.scale = [1, 1]
        end if
        if chip.number <> invalid then lookNumber(chip.number, lit, words)
        if chip.caption <> invalid then chip.caption.visible = lit
    end for
end sub

' On the accent chip, an accent number is dark instead; a plain one is in
' its words' color (words).
sub lookNumber(number as object, lit as boolean, words as string)
    if number.tone = "danger"
        number.back.blendColor = "0xEB6B70FF"
        number.text.color = "0x16191EFF"
    else if number.tone = "accent" and lit
        number.back.blendColor = "0x16191EFF"
        number.text.color = "0x7DD3FCFF"
    else if number.tone = "accent"
        number.back.blendColor = "0x38BDF8FF"
        number.text.color = "0x16191EFF"
    else
        number.text.color = words
    end if
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "left" and m.top.focusIndex > 0
        m.top.focusIndex = m.top.focusIndex - 1
        return true
    end if
    if key = "right" and m.top.focusIndex < m.chipNodes.Count() - 1
        m.top.focusIndex = m.top.focusIndex + 1
        return true
    end if
    if key = "OK"
        m.top.chosen = m.top.focusIndex
        return true
    end if
    return false
end function
