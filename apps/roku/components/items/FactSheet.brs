sub init()
    m.top.focusable = true
    m.page = m.top.findNode("page")
    m.track = m.top.findNode("track")
    m.thumb = m.top.findNode("thumb")
    m.scroll = m.top.findNode("scroll")
    m.pageTo = m.top.findNode("pageTo")
    m.thumbTo = m.top.findNode("thumbTo")
    m.offset = 0
    m.total = 0
    m.nodes = []
    m.shape = ""
    m.top.observeField("visible", "onVisible")
end sub

' The lines laid out top to bottom: a version's name in the accent, a
' block's heading in bold, a fact's label in gray and its value in the
' text color (on up to three lines) labelWidth along, a note in gray
' across the width. Lines that differ from those shown only in their
' values and notes (a conversion's details, asked for again every 2 s)
' change those words where they are, and the panel stays where it was
' scrolled to; other lines are a new sheet, read from the top.
sub onLines()
    lines = m.top.lines
    shape = ""
    for each line in lines
        shape += line.kind + chr(10)
        if line.kind <> "note" then shape += line.text + chr(10)
    end for
    if m.top.visible and shape = m.shape
        for i = 0 to lines.Count() - 1
            setText(m.nodes[i].label, lines[i].text)
            if m.nodes[i].value <> invalid then setText(m.nodes[i].value, lines[i].value)
        end for
        place(true)
        return
    end if
    m.shape = shape
    m.page.removeChildrenIndex(m.page.getChildCount(), 0)
    width = m.top.width
    valueX = m.top.labelWidth
    m.nodes = []
    for each line in lines
        if line.kind = "note"
            m.nodes.Push({ label: addLabel(line.text, "body", "0xA1A9B6FF", 0, width, 4), value: invalid })
        else if line.kind = "fact"
            m.nodes.Push({ label: addLabel(line.text, "body", "0xA1A9B6FF", 0, valueX - 24, 2), value: addLabel(line.value, "body", "0xEBEBEBFF", valueX, width - valueX, 3) })
        else if line.kind = "version"
            m.nodes.Push({ label: addLabel(line.text, "section", "0x7DD3FCFF", 0, width, 2), value: invalid })
        else
            m.nodes.Push({ label: addLabel(line.text, "title", "0xEBEBEBFF", 0, width, 2), value: invalid })
        end if
    end for
    place(false)
end sub

' A sheet put away is read from the top when it shows again.
sub onVisible()
    if not m.top.visible then m.shape = ""
end sub

sub setText(label as object, text as string)
    if label.text <> text then label.text = text
end sub

' Each line under the one before, by the heights its words take now; kept,
' the panel stays at the same line's top, or the last it can scroll to.
sub place(kept as boolean)
    width = m.top.width
    valueX = m.top.labelWidth
    y = 0
    ' Where each line starts and ends, for the panel to stop at whole lines.
    m.tops = []
    m.bottoms = []
    lines = m.top.lines
    for i = 0 to m.nodes.Count() - 1
        kind = lines[i].kind
        label = m.nodes[i].label
        if kind = "note"
            moveTo(label, 0, y)
            height = label.boundingRect().height
            keepLine(y, height)
            y += height + 8
        else if kind = "fact"
            value = m.nodes[i].value
            moveTo(label, 0, y)
            moveTo(value, valueX, y)
            height = label.boundingRect().height
            if value.boundingRect().height > height then height = value.boundingRect().height
            keepLine(y, height)
            y += height + 8
        else
            if y > 0
                y += 28
                if kind = "version" then y += 24
            end if
            moveTo(label, 0, y)
            height = label.boundingRect().height
            keepLine(y, height)
            y += height + 12
        end if
    end for
    m.total = y
    offset = 0
    if kept
        last = lastTop()
        for each top in m.tops
            if top <= m.offset and top > offset then offset = top
        end for
        if last >= 0 and offset > last then offset = last
    end if
    if not kept or offset <> m.offset
        m.scroll.control = "stop"
        m.page.translation = [0, -offset]
    end if
    m.offset = offset
    clipAt(offset)
    ' The bar: as tall as the part in view is of the whole.
    longer = m.total > m.top.height
    m.track.visible = longer
    m.thumb.visible = longer
    if longer
        m.track.translation = [width + 16, 0]
        m.track.height = m.top.height
        m.thumb.height = Int(m.top.height * m.top.height / m.total)
        if m.scroll.state <> "running" then m.thumb.translation = [width + 16, thumbAt(offset)]
    end if
end sub

sub moveTo(label as object, x as integer, y as integer)
    at = label.translation
    if at[0] <> x or at[1] <> y then label.translation = [x, y]
end sub

' The first line's top from which the rest fits in the panel, -1 for none.
function lastTop() as integer
    limit = m.total - m.top.height
    if limit < 0 then limit = 0
    last = -1
    for each top in m.tops
        if top >= limit and (last < 0 or top < last) then last = top
    end for
    return last
end function

' Where the bar's thumb sits with the panel scrolled to offset. The last
' whole line's top can be past where the rest just fits.
function thumbAt(offset as integer) as integer
    limit = m.total - m.top.height
    if limit <= 0 then return 0
    shownTo = offset
    if shownTo > limit then shownTo = limit
    return Int((m.top.height - m.thumb.height) * shownTo / limit)
end function

sub keepLine(top as integer, height as integer)
    m.tops.Push(top)
    m.bottoms.Push(top + height)
end sub

' Cut at the foot of the last line wholly in view from offset, not through
' the next.
sub clipAt(offset as integer)
    bottom = offset + m.top.height
    shown = 0
    for each b in m.bottoms
        if b <= bottom and b - offset > shown then shown = b - offset
    end for
    if shown <= 0 then shown = m.top.height
    m.top.clippingRect = [0, 0, m.top.width + 24, shown]
end sub

function addLabel(text as string, style as string, color as string, x as integer, width as integer, lines as integer) as object
    label = m.page.createChild("Label")
    label.font = uiType(style)
    label.color = color
    label.width = width
    label.wrap = true
    label.maxLines = lines
    label.translation = [x, 0]
    label.text = text
    return label
end function

' Up and Down move about a fifth of the panel at a time.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key <> "up" and key <> "down" then return false
    stepBy = Int(m.top.height / 5)
    ' To a line's top, so none is cut at the panel's top: the furthest
    ' within the step, else the next one, and no further than the first
    ' from which the rest fits.
    if key = "down"
        last = lastTop()
        offset = -1
        for each top in m.tops
            if top > m.offset and top <= m.offset + stepBy and top > offset then offset = top
        end for
        if offset < 0
            for each top in m.tops
                if top > m.offset and (offset < 0 or top < offset) then offset = top
            end for
        end if
        if last >= 0 and offset > last then offset = last
        if offset < m.offset then offset = m.offset
    else
        offset = -1
        for each top in m.tops
            if top < m.offset and top >= m.offset - stepBy and (offset < 0 or top < offset) then offset = top
        end for
        if offset < 0
            for each top in m.tops
                if top < m.offset and top > offset then offset = top
            end for
        end if
        if offset < 0 then offset = 0
    end if
    if offset = m.offset then return true
    m.offset = offset
    clipAt(offset)
    m.scroll.control = "stop"
    m.pageTo.keyValue = [m.page.translation, [0, -offset]]
    m.thumbTo.keyValue = [m.thumb.translation, [m.top.width + 16, thumbAt(offset)]]
    m.scroll.control = "start"
    return true
end function
