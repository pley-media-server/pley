sub init()
    m.frame = m.top.findNode("frame")
    m.poster = m.top.findNode("poster")
    m.tick = m.top.findNode("tick")
    m.text = m.top.findNode("text")
    m.buttons = m.top.findNode("buttons")
    m.untouched = m.top.findNode("untouched")
    ' On screen, and under a dialog; how long it has been up.
    m.shown = false
    m.covered = false
    m.since = CreateObject("roTimespan")
    m.buttons.observeField("buttonSelected", "onButton")
    m.buttons.observeField("buttonFocused", "touched")
    m.untouched.observeField("fire", "onUntouched")
    m.top.observeField("focusedChild", "onFocusChanged")
end sub

sub onNotice()
    notice = m.top.notice
    if notice = invalid or notice.text = invalid then return
    poster = ""
    if GetInterface(notice.poster, "ifString") <> invalid then poster = notice.poster
    m.text.text = notice.text
    ' The scene keeps the card for the whole session: its buttons are in
    ' the language shown when it shows.
    labels = [t("common.actions.open"), t("common.actions.close")]
    if FormatJson(m.buttons.buttons) <> FormatJson(labels) then m.buttons.buttons = labels
    withPoster = poster <> ""
    m.frame.visible = withPoster
    m.poster.visible = withPoster
    m.tick.visible = not withPoster
    if withPoster
        m.poster.setHttpAgent(m.top.getScene().getHttpAgent())
        m.poster.uri = poster
        m.text.translation = [140, 32]
        m.text.width = 548
    else
        m.poster.uri = ""
        m.text.translation = [92, 32]
        m.text.width = 596
    end if
    m.shown = true
    m.covered = false
    m.since.Mark()
    m.top.visible = true
    m.buttons.focusButton = 0
    m.buttons.setFocus(true)
    touched()
end sub

' Every key, and the focus moving between Open and Close, gives it 10 s
' more.
sub touched()
    if not m.shown or m.covered then return
    m.untouched.control = "stop"
    m.untouched.control = "start"
end sub

sub onUntouched()
    finish("close")
end sub

sub onButton()
    if not m.shown then return
    ' An OK this soon was meant for the page under it.
    if m.since.TotalMilliseconds() < 600
        touched()
        return
    end if
    if m.buttons.buttonSelected = 0
        finish("open")
    else
        finish("close")
    end if
end sub

' The focus left it: the scene says whether a dialog covers it. The focus
' coming to it once it has gone is the scene's to give back.
sub onFocusChanged()
    inside = m.top.isInFocusChain()
    if not m.shown
        if inside then m.top.event = "stray"
        return
    end if
    if not inside and not m.covered then m.top.event = "lost"
end sub

' Under a dialog its time stops; once none covers it, it has the focus
' again and its 10 s start over.
sub onCover()
    if not m.shown then return
    if m.top.cover
        m.covered = true
        m.untouched.control = "stop"
        return
    end if
    if not m.covered then return
    m.covered = false
    if not m.top.isInFocusChain() then m.buttons.setFocus(true)
    touched()
end sub

sub onHide()
    if not m.shown then return
    m.shown = false
    m.covered = false
    m.untouched.control = "stop"
    m.top.visible = false
end sub

sub finish(how as string)
    if not m.shown then return
    onHide()
    m.top.event = how
end sub

' Up, Down and Back close it. Keys stay here: the page under it gets
' none while it shows.
function onKeyEvent(key as string, press as boolean) as boolean
    if not m.shown then return false
    if press
        touched()
        if key = "up" or key = "down" or key = "back" then finish("close")
    end if
    return true
end function
