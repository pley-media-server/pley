sub init()
    m.title = m.top.findNode("title")
    m.server = m.top.findNode("server")
    m.intro = m.top.findNode("intro")
    m.twoStep = m.top.findNode("twoStep")
    m.steps = m.top.findNode("steps")
    m.url = m.top.findNode("url")
    m.code = m.top.findNode("code")
    m.qrBox = m.top.findNode("qrBox")
    m.qr = m.top.findNode("qr")
    m.status = m.top.findNode("status")
    m.actions = m.top.findNode("actions")
    m.leave = m.top.findNode("leave")
    m.started = false
    m.request = invalid
    m.done = false
    ' The code shown came in place of one that expired, and how many came
    ' so since a button was pressed.
    m.renewed = false
    m.renewals = 0
    m.qr.observeField("loadStatus", "onQrStatus")
    m.actions.observeField("buttonSelected", "onAction")
    m.leave.observeField("fire", "onLeave")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active
        stopAsking()
        return
    end if
    if not m.started
        m.started = true
        session = m.global.session
        m.server.text = "on " + session.serverName + " (" + urlDisplayAddress(session.server) + ")"
        reason = httpString(m.top.params.reason)
        if reason = "ended"
            m.intro.text = "The server ended this TV's sign-in: sign in again."
            m.intro.visible = true
        else if reason = "wider_scope"
            m.intro.text = "The server allowed more than Pley for Roku asks for, so it was signed out."
            m.intro.visible = true
        else if reason = "requests"
            ' Discover's Sign in: this sign-in goes on until the new one.
            m.intro.text = "Sign in again to let this TV ask for movies and shows."
            m.intro.visible = true
        else if reason = "approvals"
            m.intro.text = "Sign in again to approve and decline requests here."
            m.intro.visible = true
        end if
        ask()
        ' Whether the server requires two-step sign-in, asked after the code,
        ' which never waits for it. Anyone may ask, so it goes without the
        ' token.
        uiRequest({ method: "GET", path: "/api/server", auth: false }, "onServerInfo")
    end if
    m.actions.setFocus(true)
end sub

' A server that requires two-step sign-in says so under its name. Below
' the reason to sign in again when there is one, the rest of the screen a
' line lower, so that nothing crowds. Nothing when the server doesn't say
' or didn't answer: the sign-in goes on all the same.
sub onServerInfo(event as object)
    note = authTwoStepNote(event.getData())
    if note = "" or m.done then return
    m.twoStep.text = note
    if m.intro.visible
        m.twoStep.translation = [96, m.intro.translation[1] + 36]
        for each node in [m.steps, m.status, m.actions]
            node.translation = [node.translation[0], node.translation[1] + 36]
        end for
    end if
    m.twoStep.visible = true
end sub

' Asks for a code, and goes on asking whether it was approved. renewed:
' in place of one that expired.
sub ask(renewed = false as boolean)
    m.renewed = renewed
    if renewed
        m.renewals = m.renewals + 1
    else
        m.renewals = 0
    end if
    stopAsking()
    m.code.text = "…"
    m.code.opacity = 1.0
    m.url.text = ""
    hideQr()
    setStatus("Getting a code…", false)
    setButtons(["Change server"])
    m.request = uiRequest({ op: "signIn" }, "onCode")
    m.request.observeField("update", "onUpdate")
end sub

sub stopAsking()
    if m.request = invalid then return
    m.request.unobserveField("result")
    m.request.unobserveField("update")
    m.request = invalid
    if not m.done then uiRequest({ op: "signInStop" }, "")
end sub

sub onCode(event as object)
    answer = event.getData()
    if not answer.ok
        m.request = invalid
        if uiFailed(answer) then return
        setStatus(answer.message, true)
        setButtons(["Try again", "Change server"])
        return
    end if
    m.code.text = answer.data.userCode
    m.url.text = answer.data.verificationUri
    fitAddress()
    showQr(answer.data.qr)
end sub

' The address in the largest size that fits its column on one line; one
' too long even in the smallest wraps.
sub fitAddress()
    m.url.wrap = false
    m.url.width = 0
    for each font in ["font:ExtraLargeBoldSystemFont", "font:LargeBoldSystemFont", "font:MediumBoldSystemFont", "font:SmallBoldSystemFont"]
        m.url.font = font
        if m.url.boundingRect().width <= 1248 then exit for
    end for
    m.url.width = 1248
    m.url.wrap = true
end sub

' The server's QR code of the address with the code in it, shown once it
' has loaded: a server older than it answers 404, and the code is enough.
' Anyone may fetch it, so it goes without the token.
sub showQr(address as string)
    m.qrBox.visible = false
    m.qr.setHttpAgent(uiHttpAgent(false))
    m.qr.uri = address
end sub

sub hideQr()
    m.qrBox.visible = false
    m.qr.uri = ""
end sub

' Drawn at a whole multiple of its size (the server draws 8 pixels a
' module), so its modules stay sharp, centred in its column.
sub onQrStatus()
    ready = m.qr.loadStatus = "ready" and m.qr.uri <> ""
    if ready and m.qr.bitmapWidth > 0
        side = Int(m.qr.bitmapWidth)
        size = side * Int(qrSpace() / side)
        if size <= 0 then size = qrSpace()
        m.qr.width = size
        m.qr.height = size
        m.qr.translation = [Int((qrSpace() - size) / 2), 52]
    end if
    m.qrBox.visible = ready
end sub

' The width of the QR code's column.
function qrSpace() as integer
    return 440
end function

sub onUpdate(event as object)
    update = event.getData()
    status = update.status
    if status = "waiting"
        if m.renewed
            setStatus("That code expired, so here is a new one. Waiting for approval…", false)
        else
            setStatus("Waiting for approval…", false)
        end if
    else if status = "retrying"
        setStatus("Can't reach the server. Trying again in " + urlValue(update.seconds) + " s.", true)
    else if status = "denied"
        expired("Request declined.", true)
    else if status = "expired"
        ' A new one at once: the TV may be left on this screen a while. After
        ' an hour of them, OK gets the next, so a Roku left on overnight
        ' doesn't ask the server for codes all night.
        if m.renewals < renewalsMax()
            ask(true)
        else
            expired("Code expired: press OK for a new one.", false)
        end if
    else if status = "signed_in"
        signedIn()
    else
        expired(httpString(update.message), true)
    end if
end sub

' Codes given in a row with no button pressed: with the server's 10
' minutes a code, an hour's worth.
function renewalsMax() as integer
    return 5
end function

' The code can't be used any more: a new one is a button away.
sub expired(text as string, problem as boolean)
    m.request = invalid
    m.code.opacity = 0.4
    hideQr()
    setStatus(text, problem)
    setButtons(["Get a new code", "Change server"])
end sub

sub signedIn()
    ' The profile was chosen in the browser: no "Who's watching?" now.
    m.top.getScene().profileChosen = true
    m.done = true
    m.request = invalid
    m.steps.visible = false
    m.intro.visible = false
    m.twoStep.visible = false
    m.title.text = "Signed in"
    setStatus("", false)
    setButtons([])
    m.top.setFocus(true)
    uiGet("/api/auth/me", "onMe")
end sub

sub onMe(event as object)
    answer = event.getData()
    if answer.ok and answer.data <> invalid
        name = httpSentence(answer.data.name)
        if name <> "" then m.title.text = "Signed in as " + name
    end if
    m.leave.control = "start"
end sub

sub onLeave()
    uiNavigate({ to: "home", reset: true })
end sub

sub setStatus(text as string, problem as boolean)
    m.status.text = text
    if problem
        m.status.color = uiColors().danger
    else
        m.status.color = uiColors().muted
    end if
end sub

sub setButtons(labels as object)
    m.actions.buttons = labels
    m.actions.visible = labels.Count() > 0
    if labels.Count() > 0 and m.top.isInFocusChain()
        m.actions.focusButton = 0
        m.actions.setFocus(true)
    end if
end sub

sub onAction(event as object)
    label = m.actions.buttons[event.getData()]
    if label = "Change server"
        uiNavigate({ to: "servers", reset: true })
    else
        ask()
    end if
end sub
