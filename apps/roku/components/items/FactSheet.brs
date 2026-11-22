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
end sub

' The lines laid out once, top to bottom: a version's name in the accent,
' a block's heading in bold, a fact's label in grey and its value in the
' text color (on up to three lines) labelWidth along, a note in grey
' across the width.
sub onLines()
    m.scroll.control = "stop"
    m.page.removeChildrenIndex(m.page.getChildCount(), 0)
    width = m.top.width
    valueX = m.top.labelWidth
    y = 0
    for each line in m.top.lines
        if line.kind = "note"
            note = addLabel(line.text, "body", "0xA1A9B6FF", 0, y, width, 4)
            y += note.boundingRect().height + 8
        else if line.kind = "fact"
            label = addLabel(line.text, "body", "0xA1A9B6FF", 0, y, valueX - 24, 2)
            value = addLabel(line.value, "body", "0xEBEBEBFF", valueX, y, width - valueX, 3)
            height = label.boundingRect().height
            if value.boundingRect().height > height then height = value.boundingRect().height
            y += height + 8
        else
            style = "title"
            color = "0xEBEBEBFF"
            if line.kind = "version"
                style = "section"
                color = "0x7DD3FCFF"
            end if
            if y > 0
                y += 28
                if line.kind = "version" then y += 24
            end if
            heading = addLabel(line.text, style, color, 0, y, width, 2)
            y += heading.boundingRect().height + 12
        end if
    end for
    m.total = y
    m.offset = 0
    m.page.translation = [0, 0]
    m.top.clippingRect = [0, 0, width + 24, m.top.height]
    ' The bar: as tall as the part in view is of the whole.
    longer = m.total > m.top.height
    m.track.visible = longer
    m.thumb.visible = longer
    if longer
        m.track.translation = [width + 16, 0]
        m.track.height = m.top.height
        m.thumb.height = Int(m.top.height * m.top.height / m.total)
        m.thumb.translation = [width + 16, 0]
    end if
end sub

function addLabel(text as string, style as string, color as string, x as integer, y as integer, width as integer, lines as integer) as object
    label = m.page.createChild("Label")
    label.font = uiType(style)
    label.color = color
    label.width = width
    label.wrap = true
    label.maxLines = lines
    label.translation = [x, y]
    label.text = text
    return label
end function

' Up and Down move a fifth of the panel at a time.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key <> "up" and key <> "down" then return false
    limit = m.total - m.top.height
    if limit < 0 then limit = 0
    stepBy = Int(m.top.height / 5)
    offset = m.offset
    if key = "down" then offset += stepBy else offset -= stepBy
    if offset > limit then offset = limit
    if offset < 0 then offset = 0
    if offset = m.offset then return true
    m.offset = offset
    m.scroll.control = "stop"
    m.pageTo.keyValue = [m.page.translation, [0, -offset]]
    thumbY = 0
    if limit > 0 then thumbY = Int((m.top.height - m.thumb.height) * offset / limit)
    m.thumbTo.keyValue = [m.thumb.translation, [m.top.width + 16, thumbY]]
    m.scroll.control = "start"
    return true
end function
