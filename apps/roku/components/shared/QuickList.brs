sub init()
    quickInit()
end sub

' Up and Down: the row before or after.
function quickMove(key as string) as boolean
    content = m.top.content
    if content = invalid then return false
    at = m.top.itemFocused
    if key = "down" and at < content.getChildCount() - 1
        m.top.jumpToItem = at + 1
        return true
    end if
    if key = "up" and at > 0
        m.top.jumpToItem = at - 1
        return true
    end if
    return false
end function
