sub init()
    quickInit()
end sub

' The arrows: the tile beside, above or below; Down from a row with
' nothing under it goes to the last tile, as the grid's own handling does.
function quickMove(key as string) as boolean
    content = m.top.content
    if content = invalid then return false
    count = content.getChildCount()
    columns = m.top.numColumns
    at = m.top.itemFocused
    if count = 0 or columns <= 0 or at < 0 then return false
    column = at mod columns
    target = -1
    if key = "left" and column > 0 then target = at - 1
    if key = "right" and column < columns - 1 and at + 1 < count then target = at + 1
    if key = "up" and at - columns >= 0 then target = at - columns
    if key = "down"
        if at + columns < count
            target = at + columns
        else if Int(at / columns) < Int((count - 1) / columns)
            target = count - 1
        end if
    end if
    if target < 0 then return false
    m.top.jumpToItem = target
    return true
end function
