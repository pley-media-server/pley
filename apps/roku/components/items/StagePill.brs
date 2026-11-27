sub init()
    m.c = uiColors()
    m.back = m.top.findNode("back")
    m.label = m.top.findNode("label")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
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
    ' The stage's color (lib/Requests.brs's, in Midnight's) in the theme's,
    ' its words in that fill's words color.
    fill = uiThemed(m.top.color, m.c)
    m.back.color = fill
    m.fill.color = fill
    m.label.color = themeWordsOn(m.c, fill)
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
