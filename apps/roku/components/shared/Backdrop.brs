sub init()
    ' shown: the picture on screen; next: drawn over it, where the coming
    ' one loads at no opacity until it is ready.
    m.shown = m.top.findNode("a")
    m.next = m.top.findNode("b")
    m.fade = m.top.findNode("fade")
    m.fader = m.top.findNode("fader")
    ' What the fade under way does: "in" (next over shown) or "out" (shown
    ' away), "" with none.
    m.fading = ""
    m.next.observeField("loadStatus", "onLoaded")
    m.shown.observeField("loadStatus", "onLoaded")
    m.fade.observeField("state", "onFaded")
end sub

sub onUri()
    uri = m.top.uri
    ' A fade under way ends where it was going, so the next starts from a
    ' still picture.
    if m.fading <> ""
        m.fade.control = "stop"
        settle()
    end if
    if uri = m.shown.uri
        ' Back to the picture shown before the one still loading.
        m.next.uri = ""
        return
    end if
    if uri = ""
        m.next.uri = ""
        fadeOut()
        return
    end if
    ' The scene's agent, as the screens ask for their pictures.
    m.next.setHttpAgent(m.top.getScene().getHttpAgent())
    m.next.opacity = 0
    m.next.uri = uri
    onLoaded()
end sub

' The coming picture ready: it fades in over the shown one. One that
' failed fades out the shown one, which is another title's.
sub onLoaded()
    if m.fading <> "" or m.next.uri = "" or m.next.uri <> m.top.uri then return
    status = m.next.loadStatus
    if status = "ready"
        m.fading = "in"
        m.fader.keyValue = [0, 1]
        m.fader.fieldToInterp = m.next.id + ".opacity"
        m.fade.control = "start"
    else if status = "failed"
        m.next.uri = ""
        fadeOut()
    end if
end sub

sub fadeOut()
    if m.shown.uri = "" then return
    m.fading = "out"
    m.fader.keyValue = [m.shown.opacity, 0]
    m.fader.fieldToInterp = m.shown.id + ".opacity"
    m.fade.control = "start"
end sub

sub onFaded()
    if m.fade.state = "stopped" and m.fading <> "" then settle()
end sub

' Where the fade was going: the picture faded in shown in full, over the
' one let go, which then loads the next; or the shown one gone.
sub settle()
    fading = m.fading
    m.fading = ""
    if fading = "in"
        old = m.shown
        m.shown = m.next
        m.next = old
        m.shown.opacity = 1
        old.opacity = 0
        old.uri = ""
        ' The next picture is drawn over the one shown.
        m.top.removeChild(old)
        m.top.insertChild(old, 1)
    else if fading = "out"
        m.shown.opacity = 0
        m.shown.uri = ""
    end if
end sub
