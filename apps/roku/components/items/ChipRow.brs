sub init()
    m.top.focusable = true
    m.backs = []
    m.labels = []
    m.numbers = []
    m.top.observeField("focusedChild", "onLook")
end sub

' Chips side by side, 16 pixels apart; a chip of width 0 is as wide as its
' words (and its number), with room either side. A chip's number (count,
' "" for none) is a small box after its words.
sub onChips()
    m.top.removeChildrenIndex(m.top.getChildCount(), 0)
    m.backs = []
    m.labels = []
    m.numbers = []
    x = 0
    for each chip in m.top.chips
        back = m.top.createChild("Rectangle")
        label = m.top.createChild("Label")
        label.font = "font:SmallSystemFont"
        label.text = chip.label
        number = invalid
        if chip.DoesExist("count") and chip.count <> "" then number = makeNumber(chip)
        words = Int(label.boundingRect().width)
        inner = words
        if number <> invalid
            ' Pixels to spare: a label as wide as its words, rounded down,
            ' cuts them short.
            words = words + 2
            inner = words + number.gap + number.width
        end if
        width = chip.width
        if width <= 0 then width = inner + m.top.pad
        back.translation = [x, 0]
        back.width = width
        back.height = 56
        label.height = 56
        label.vertAlign = "center"
        if number = invalid
            label.translation = [x, 0]
            label.width = width
            label.horizAlign = "center"
        else
            ' The words and the number together in the middle, the words
            ' cut short in a chip too narrow for both.
            room = width - m.top.pad - number.gap - number.width
            if words > room then words = room
            if words < 0 then words = 0
            start = x + Int((width - (words + number.gap + number.width)) / 2)
            label.translation = [start, 0]
            label.width = words
            number.back.translation = [start + words + number.gap, 10]
            number.text.translation = [start + words + number.gap, 10]
        end if
        m.backs.Push(back)
        m.labels.Push(label)
        m.numbers.Push(number)
        x = x + width + 16
    end for
    m.top.rowWidth = x - 16
    if x = 0 then m.top.rowWidth = 0
    if m.top.focusIndex >= m.backs.Count() then m.top.focusIndex = 0
    onLook()
end sub

' A chip's number: { back, text, width, gap (from the words), tone }. Where
' it asks to be seen, in a box in the accent (accent) or the danger colour
' (danger), as the menu's; else plain, in bold after the words, as a
' row of eight tabs has room for.
function makeNumber(chip as object) as object
    back = m.top.createChild("Rectangle")
    text = m.top.createChild("Label")
    text.font = "font:SmallestBoldSystemFont"
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

sub onLook()
    focused = m.top.hasFocus()
    for i = 0 to m.backs.Count() - 1
        lit = focused and i = m.top.focusIndex
        if lit
            m.backs[i].color = "0x38BDF8FF"
            words = "0x16191EFF"
        else if i = m.top.current
            m.backs[i].color = "0x2E3440FF"
            words = "0xEBEBEBFF"
        else
            m.backs[i].color = "0x1E222AFF"
            words = "0xA1A9B6FF"
        end if
        m.labels[i].color = words
        number = m.numbers[i]
        if number <> invalid then lookNumber(number, lit, words)
    end for
end sub

' On the accent chip, an accent number is dark instead; a plain one is in
' its words' colour (words).
sub lookNumber(number as object, lit as boolean, words as string)
    if number.tone = "danger"
        number.back.color = "0xEB6B70FF"
        number.text.color = "0x16191EFF"
    else if number.tone = "accent" and lit
        number.back.color = "0x16191EFF"
        number.text.color = "0x7DD3FCFF"
    else if number.tone = "accent"
        number.back.color = "0x38BDF8FF"
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
    if key = "right" and m.top.focusIndex < m.backs.Count() - 1
        m.top.focusIndex = m.top.focusIndex + 1
        return true
    end if
    if key = "OK"
        m.top.chosen = m.top.focusIndex
        return true
    end if
    return false
end function
