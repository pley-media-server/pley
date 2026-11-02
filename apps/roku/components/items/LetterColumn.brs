sub init()
    m.top.focusable = true
    m.mark = m.top.findNode("mark")
    m.labels = []
    m.bold = -1
    m.top.observeField("focusedChild", "onLook")
end sub

' A label a letter, 30 pixels apart.
sub onLetters()
    for each label in m.labels
        m.top.removeChild(label)
    end for
    m.labels = []
    m.bold = -1
    y = 0
    for each letter in m.top.letters
        label = m.top.createChild("Label")
        label.font = "font:TinySystemFont"
        label.text = letter
        label.translation = [0, y]
        label.width = 40
        label.height = 30
        label.horizAlign = "center"
        label.vertAlign = "center"
        m.labels.Push(label)
        y = y + rowHeight()
    end for
    onLook()
end sub

function rowHeight() as integer
    return 30
end function

' The letter in focus on the accent; without the focus, the grid's letter
' in the text colour on a lighter ground; the others muted, or dimmed where
' no title starts with them.
sub onLook()
    focused = m.top.hasFocus()
    has = m.top.has
    if has = invalid then has = []
    marked = m.top.current
    if focused then marked = m.top.focusIndex
    ' Only the marked letter is bold: one font changes as the mark moves.
    if m.bold <> marked
        if m.bold >= 0 and m.bold < m.labels.Count() then m.labels[m.bold].font = "font:TinySystemFont"
        if marked >= 0 and marked < m.labels.Count() then m.labels[marked].font = "font:TinyBoldSystemFont"
        m.bold = marked
    end if
    for i = 0 to m.labels.Count() - 1
        label = m.labels[i]
        if i = marked and focused
            label.color = "0x16191EFF"
        else if i = marked
            label.color = "0xEBEBEBFF"
        else if i < has.Count() and has[i] = true
            label.color = "0xA1A9B6FF"
        else
            label.color = "0x4B5261FF"
        end if
    end for
    m.mark.translation = [0, marked * rowHeight()]
    m.mark.visible = marked >= 0 and marked < m.labels.Count()
    if focused
        m.mark.color = "0x38BDF8FF"
    else
        m.mark.color = "0x2E3440FF"
    end if
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "up"
        if m.top.focusIndex = 0 then return false
        m.top.focusIndex = m.top.focusIndex - 1
        return true
    end if
    if key = "down"
        if m.top.focusIndex < m.labels.Count() - 1 then m.top.focusIndex = m.top.focusIndex + 1
        return true
    end if
    if key = "OK"
        m.top.chosen = m.top.focusIndex
        return true
    end if
    return key = "right"
end function
