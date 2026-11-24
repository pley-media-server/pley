' A list that moves its focus itself: Roku's own lists glide to every row,
' tile or item, and an app can't make the glide shorter, so a press felt
' slow. The list's own script (quickMove) jumps the focus at once with
' jumpToItem and the like; a key that moves nothing (an edge) goes to the
' page, as the list's own handling would. Held, a key moves again after
' 0.4 s, then every 0.09 s, until it is let go, as Roku's lists do.

sub quickInit()
    m.repeatKey = ""
    m.repeat = invalid
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press
        if key = m.repeatKey then quickStop()
        return false
    end if
    if not quickMove(key) then return false
    m.repeatKey = key
    ' Made once a key first moves the list: most lists never repeat one.
    if m.repeat = invalid
        m.repeat = CreateObject("roSGNode", "Timer")
        m.repeat.observeField("fire", "onQuickRepeat")
    end if
    m.repeat.duration = 0.4
    m.repeat.control = "start"
    return true
end function

sub onQuickRepeat()
    if m.repeatKey = "" then return
    if not quickMove(m.repeatKey)
        quickStop()
        return
    end if
    m.repeat.duration = 0.09
    m.repeat.control = "start"
end sub

sub quickStop()
    m.repeatKey = ""
    if m.repeat <> invalid then m.repeat.control = "stop"
end sub
