sub init()
    m.word = m.top.findNode("word")
    m.word.font = uiFont("bold", 60)
    if m.top.colors = invalid or m.top.colors.Count() = 0 then m.top.colors = uiColors()
    onPlace()
end sub

sub onColors()
    colors = m.top.colors
    if colors = invalid or colors.bg = invalid then return
    m.top.findNode("tile").blendColor = colors.brandTile
    m.top.findNode("far").blendColor = colors.brandFar
    m.top.findNode("deep").blendColor = colors.brandDeep
    m.top.findNode("front").blendColor = colors.brand
    m.top.findNode("light").blendColor = colors.brandLight
    m.word.color = colors.fg
end sub

' At the left from its place, or ending there.
sub onPlace()
    logo = m.top.findNode("logo")
    if not m.top.alignRight
        logo.translation = [0, 0]
        return
    end if
    m.word.width = 0
    logo.translation = [-(92 + Int(m.word.boundingRect().width) + 2), 0]
end sub
