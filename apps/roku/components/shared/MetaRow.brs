' Lays a title's facts out in a row (MetaRow.xml): 44 px tall, boxes 40
' high in the middle of it, 12 px either side of a dot.

sub init()
    m.c = uiColors()
    m.top.removeChildrenIndex(m.top.getChildCount(), 0)
end sub

sub onParts()
    m.top.removeChildrenIndex(m.top.getChildCount(), 0)
    x = 0
    previousBoxed = false
    first = true
    for each part in m.top.parts
        boxed = part.boxed = true
        if not first
            if boxed and previousBoxed
                ' Badges side by side, without a dot.
                x = x + 12
            else
                x = x + 8
                dot = addLabel("·", "label", m.c.muted, x)
                x = x + labelWidth(dot) + 12
            end if
        end if
        if boxed
            words = addLabel(part.text, "caption", m.c.fg, x + 12)
            width = labelWidth(words) + 24
            edge = m.top.createChild("Poster")
            edge.uri = "pkg:/images/ui/edge4.9.png"
            edge.blendColor = m.c.subtle
            edge.translation = [x, 2]
            edge.width = width
            edge.height = 40
            words.translation = [x + 12, 2]
            words.height = 40
            x = x + width
        else if part.star = true
            star = m.top.createChild("Poster")
            star.uri = "pkg:/images/ui/icon-star-40.png"
            star.blendColor = m.c.muted
            star.width = 28
            star.height = 28
            star.translation = [x, 8]
            words = addLabel(part.text, "label", m.c.muted, x + 36)
            x = x + 36 + labelWidth(words)
        else
            words = addLabel(part.text, "label", m.c.muted, x)
            x = x + labelWidth(words)
        end if
        x = x + 4
        previousBoxed = boxed
        first = false
    end for
end sub

function addLabel(text as string, style as string, color as string, x as integer) as object
    label = m.top.createChild("Label")
    label.font = uiType(style)
    label.color = color
    label.text = text
    label.height = 44
    label.vertAlign = "center"
    label.translation = [x, 0]
    return label
end function

function labelWidth(label as object) as integer
    return Int(label.boundingRect().width) + 2
end function
