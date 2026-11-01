sub init()
    m.back = m.top.findNode("back")
    m.label = m.top.findNode("label")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
end sub

sub onLook()
    ' Fields set in markup may come before init.
    if m.label = invalid then return
    m.label.text = m.top.text
    ' As wide as its words, cut short only when they don't fit.
    m.label.width = 0
    textWidth = m.label.boundingRect().width
    if textWidth > m.top.maxWidth - 36
        textWidth = m.top.maxWidth - 36
        m.label.width = textWidth
    end if
    m.back.width = textWidth + 36
    m.back.color = m.top.color
    m.fill.color = m.top.color
    progress = m.top.progress
    shown = progress >= 0
    m.track.visible = shown
    m.fill.visible = shown
    if shown
        if progress > 1 then progress = 1
        at = [textWidth + 60, 17]
        if m.top.stacked then at = [0, 56]
        m.track.translation = at
        m.fill.translation = at
        m.fill.width = Int(m.track.width * progress)
    end if
end sub
