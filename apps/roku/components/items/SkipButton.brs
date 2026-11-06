sub init()
    m.label = m.top.findNode("label")
    m.fill = m.top.findNode("fill")
    m.lineTop = m.top.findNode("lineTop")
    m.lineBottom = m.top.findNode("lineBottom")
    m.lineLeft = m.top.findNode("lineLeft")
end sub

' The words, and the button wide enough for them: 280, or more for longer
' words ("Passer le générique"), grown to the left so its right edge stays
' where the player put it.
sub onText()
    m.label.width = 0
    m.label.text = m.top.text
    width = Int(m.label.boundingRect().width) + 64
    if width < 280 then width = 280
    x = 280 - width
    m.fill.translation = [x, 0]
    m.fill.width = width
    m.lineTop.translation = [x, 0]
    m.lineTop.width = width
    m.lineBottom.translation = [x, 58]
    m.lineBottom.width = width
    m.lineLeft.translation = [x, 2]
    m.label.translation = [x, 0]
    m.label.width = width
end sub

' Keys this button doesn't take go up to its parent, the Video node, and on
' to the player: Back leaves the player as it always does.
function onKeyEvent(key as string, press as boolean) as boolean
    if not m.top.visible then return false
    if key = "OK"
        if press then m.top.pressed = true
        return true
    end if
    if key = "play" or key = "fastforward" or key = "rewind" or key = "replay"
        if press then m.top.playKey = key
        return true
    end if
    return false
end function
