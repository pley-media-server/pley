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
    m.keys = []
    m.tops = []
    m.bottoms = []
    m.shape = ""
    m.top.observeField("visible", "onVisible")
end sub

' The lines laid out top to bottom: a version's name in the accent, a
' block's heading in bold, a fact's label in gray and its value in the
' text color (on up to three lines) labelWidth along, a note in gray
' across the width. The same sheet again (its headings the same: a
' conversion's details, asked for again every 2 s, whose speed comes and
' goes) changes only the words that changed, and the panel stays on the
' line at its top; another sheet, or one shown again, is read from the top.
sub onLines()
    lines = m.top.lines
    shape = ""
    keys = []
    for each line in lines
        if line.kind = "heading" or line.kind = "version" then shape += line.kind + ":" + line.text + chr(10)
        keys.Push(line.kind + ":" + line.text)
    end for
    if m.top.visible and shape = m.shape
        ' The line at the panel's top, to find again among the new ones.
        at = 0
        for i = 0 to m.tops.Count() - 1
            if m.tops[i] <= m.offset then at = i
        end for
        anchor = -1
        if at < m.keys.Count()
            for i = 0 to keys.Count() - 1
                if anchor < 0 and keys[i] = m.keys[at] then anchor = i
            end for
        end if
        if anchor < 0 then anchor = at
        for i = 0 to lines.Count() - 1
            line = lines[i]
            if i < m.nodes.Count() and m.nodes[i].kind = line.kind
                setText(m.nodes[i].label, line.text)
                if m.nodes[i].value <> invalid then setText(m.nodes[i].value, line.value)
            else
                if i < m.nodes.Count()
                    dropNode(m.nodes[i])
                    m.nodes[i] = makeNode(line)
                else
                    m.nodes.Push(makeNode(line))
                end if
            end if
        end for
        while m.nodes.Count() > lines.Count()
            dropNode(m.nodes.Pop())
        end while
        m.keys = keys
        place(anchor)
        return
    end if
    m.shape = shape
    m.keys = keys
    m.page.removeChildrenIndex(m.page.getChildCount(), 0)
    m.nodes = []
    for each line in lines
        m.nodes.Push(makeNode(line))
    end for
    place(-1)
end sub

' A sheet put away is read from the top when it shows again.
sub onVisible()
    if not m.top.visible then m.shape = ""
end sub

' A line's labels: { kind, label, value (a fact's, else invalid) }.
function makeNode(line as object) as object
    width = m.top.width
    valueX = m.top.labelWidth
    if line.kind = "note" then return { kind: "note", label: addLabel(line.text, "body", "0xA1A9B6FF", 0, width, 4), value: invalid }
    if line.kind = "fact" then return { kind: "fact", label: addLabel(line.text, "body", "0xA1A9B6FF", 0, valueX - 24, 2), value: addLabel(line.value, "body", "0xEBEBEBFF", valueX, width - valueX, 3) }
    if line.kind = "version" then return { kind: "version", label: addLabel(line.text, "section", "0x7DD3FCFF", 0, width, 2), value: invalid }
    return { kind: line.kind, label: addLabel(line.text, "title", "0xEBEBEBFF", 0, width, 2), value: invalid }
end function

sub dropNode(node as object)
    m.page.removeChild(node.label)
    if node.value <> invalid then m.page.removeChild(node.value)
end sub

sub setText(label as object, text as string)
    if label.text <> text then label.text = text
end sub

' Each line under the one before, by the heights its words take now. The
' panel then shows the anchor line at its top (or the last it can scroll
' to), else, for -1, the sheet's top.
sub place(anchor as integer)
    width = m.top.width
    valueX = m.top.labelWidth
    y = 0
    ' Where each line starts and ends, for the panel to stop at whole lines.
    m.tops = []
    m.bottoms = []
    for each node in m.nodes
        label = node.label
        if node.kind = "note"
            moveTo(label, 0, y)
            height = label.boundingRect().height
            keepLine(y, height)
            y += height + 8
        else if node.kind = "fact"
            value = node.value
            moveTo(label, 0, y)
            moveTo(value, valueX, y)
            height = label.boundingRect().height
            if value.boundingRect().height > height then height = value.boundingRect().height
            keepLine(y, height)
            y += height + 8
        else
            if y > 0
                y += 28
                if node.kind = "version" then y += 24
            end if
            moveTo(label, 0, y)
            height = label.boundingRect().height
            keepLine(y, height)
            y += height + 12
        end if
    end for
    m.total = y
    offset = 0
    if anchor >= 0 and m.tops.Count() > 0
        if anchor >= m.tops.Count() then anchor = m.tops.Count() - 1
        offset = m.tops[anchor]
        last = lastTop()
        if last >= 0 and offset > last then offset = last
    end if
    ' A scroll on its way goes on, to where the line is now: stopped, its
    ' frame under way could still land after the place set here.
    moving = anchor >= 0 and m.scroll.state = "running"
    if not moving
        m.scroll.control = "stop"
        if anchor < 0 or offset <> m.offset then m.page.translation = [0, -offset]
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
        if not moving then m.thumb.translation = [width + 16, thumbAt(offset)]
    end if
    if moving
        m.pageTo.keyValue = [m.page.translation, [0, -offset]]
        m.thumbTo.keyValue = [m.thumb.translation, [width + 16, thumbAt(offset)]]
        m.scroll.control = "start"
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
