sub init()
    quickInit()
end sub

' Left and Right: the tile beside, in the focused row.
function quickMove(key as string) as boolean
    if key <> "left" and key <> "right" then return false
    content = m.top.content
    position = m.top.rowItemFocused
    if content = invalid or position = invalid or position.Count() < 2 then return false
    row = content.getChild(position[0])
    if row = invalid then return false
    column = position[1]
    if key = "right" and column < row.getChildCount() - 1
        m.top.jumpToRowItem = [position[0], column + 1]
        return true
    end if
    if key = "left" and column > 0
        m.top.jumpToRowItem = [position[0], column - 1]
        return true
    end if
    return false
end function
