sub init()
    m.title = m.top.findNode("title")
    m.server = m.top.findNode("server")
    m.intro = m.top.findNode("intro")
    m.twoStep = m.top.findNode("twoStep")
    m.steps = m.top.findNode("steps")
    m.goTo = m.top.findNode("goTo")
    m.url = m.top.findNode("url")
    m.enterCode = m.top.findNode("enterCode")
    m.code = m.top.findNode("code")
    m.sameNetwork = m.top.findNode("sameNetwork")
    m.scan = m.top.findNode("scan")
    m.qrBox = m.top.findNode("qrBox")
    m.qr = m.top.findNode("qr")
    m.status = m.top.findNode("status")
    m.actions = m.top.findNode("actions")
    m.leave = m.top.findNode("leave")
    m.globe = m.top.findNode("globe")
    m.languages = m.top.findNode("languages")
    m.started = false
    m.request = invalid
    m.done = false
    ' The code shown came in place of one that expired, and how many came
    ' so since a button was pressed.
    m.renewed = false
    m.renewals = 0
    ' What the screen says, drawn again in a new language: why to sign in
    ' again, whether the server requires two-step sign-in, the status line
    ' ({ key, values, text, problem }), the buttons (their actions) and,
    ' signed in, the profile's name.
    m.reason = ""
    m.twoStepShown = false
    m.statusWords = { key: "", values: invalid, text: "", problem: false }
    m.buttonActions = []
    m.signedInAs = invalid
    m.qr.observeField("loadStatus", "onQrStatus")
    m.actions.observeField("buttonSelected", "onAction")
    m.leave.observeField("fire", "onLeave")
    m.languages.observeField("picked", "onLanguagePicked")
    m.languages.observeField("closed", "onLanguagesClosed")
    m.top.observeField("active", "onActive")
    m.top.observeField("language", "relabel")
    relabel()
end sub

' The screen's words in the language shown; the code, the address and the
' focus stay.
sub relabel()
    session = m.global.session
    if m.done
        m.title.text = t("signin.link.signedIn")
        if m.signedInAs <> invalid then m.title.text = t("signin.link.signedInAs", { name: m.signedInAs })
    else
        m.title.text = t("signin.link.title")
    end if
    m.server.text = ""
    if session.server <> "" then m.server.text = t("signin.link.onServer", { name: session.serverName, address: urlDisplayAddress(session.server) })
    reasons = { ended: "signin.link.ended", wider_scope: "signin.link.widerScope", requests: "signin.link.requests", approvals: "signin.link.approvals" }
    if reasons.DoesExist(m.reason) then m.intro.text = t(reasons[m.reason])
    if m.twoStepShown then m.twoStep.text = t("signin.link.twoStep")
    m.goTo.text = t("signin.link.goTo")
    m.enterCode.text = t("signin.link.enterCode")
    m.sameNetwork.text = t("signin.link.sameNetwork")
    m.scan.text = t("signin.link.scan")
    m.globe.language = m.global.language
    showStatus()
    focused = m.actions.buttonFocused
    hadFocus = m.actions.isInFocusChain()
    showButtons()
    uiKeepButton(m.actions, focused, hadFocus)
end sub

sub onActive()
    if not m.top.active
        stopAsking()
        return
    end if
    if not m.started
        m.started = true
        m.reason = httpString(m.top.params.reason)
        m.intro.visible = m.reason = "ended" or m.reason = "wider_scope" or m.reason = "requests" or m.reason = "approvals"
        relabel()
        ask()
        ' Whether the server requires two-step sign-in, asked after the code,
        ' which never waits for it. Anyone may ask, so it goes without the
        ' token.
        uiRequest({ method: "GET", path: "/api/server", auth: false }, "onServerInfo")
    end if
    if m.languages.visible
        m.languages.open = true
    else
        m.actions.setFocus(true)
    end if
end sub

' A server that requires two-step sign-in says so under its name. Below
' the reason to sign in again when there is one, the rest of the screen a
' line lower, so that nothing crowds. Nothing when the server doesn't say
' or didn't answer: the sign-in goes on all the same.
sub onServerInfo(event as object)
    note = authTwoStepNote(event.getData())
    if note = "" or m.done then return
    m.twoStepShown = true
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
    setStatus("signin.link.gettingCode", invalid, false)
    setButtons(["changeServer"])
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
        setStatusText(answer.message, true)
        setButtons(["tryAgain", "changeServer"])
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
            setStatus("signin.link.renewed", invalid, false)
        else
            setStatus("signin.link.waiting", invalid, false)
        end if
    else if status = "retrying"
        setStatus("signin.link.retrying", { seconds: Int(httpNumber(update.seconds, 0)) }, true)
    else if status = "denied"
        expired("signin.link.declined", "", true)
    else if status = "expired"
        ' A new one at once: the TV may be left on this screen a while. After
        ' an hour of them, OK gets the next, so a Roku left on overnight
        ' doesn't ask the server for codes all night.
        if m.renewals < renewalsMax()
            ask(true)
        else
            expired("signin.link.expired", "", false)
        end if
    else if status = "signed_in"
        signedIn()
    else
        expired("", httpString(update.message), true)
    end if
end sub

' Codes given in a row with no button pressed: with the server's 10
' minutes a code, an hour's worth.
function renewalsMax() as integer
    return 5
end function

' The code can't be used any more: a new one is a button away. Why: key's
' words, else text, as it came.
sub expired(key as string, text as string, problem as boolean)
    m.request = invalid
    m.code.opacity = 0.4
    hideQr()
    if key <> ""
        setStatus(key, invalid, problem)
    else
        setStatusText(text, problem)
    end if
    setButtons(["newCode", "changeServer"])
end sub

sub signedIn()
    ' The profile was chosen in the browser: no "Who's watching?" now.
    m.top.getScene().profileChosen = true
    m.done = true
    m.request = invalid
    m.steps.visible = false
    m.intro.visible = false
    m.twoStep.visible = false
    m.title.text = t("signin.link.signedIn")
    setStatus("", invalid, false)
    setButtons([])
    m.top.setFocus(true)
    uiGet("/api/auth/me", "onMe")
end sub

sub onMe(event as object)
    answer = event.getData()
    if answer.ok and answer.data <> invalid
        name = httpSentence(answer.data.name)
        if name <> ""
            m.signedInAs = name
            m.title.text = t("signin.link.signedInAs", { name: name })
        end if
    end if
    m.leave.control = "start"
end sub

sub onLeave()
    uiNavigate({ to: "home", reset: true })
end sub

' The status line: key's words with values ("" for none).
sub setStatus(key as string, values as dynamic, problem as boolean)
    m.statusWords = { key: key, values: values, text: "", problem: problem }
    showStatus()
end sub

' The status line: a sentence that came written, an answer's message.
sub setStatusText(text as string, problem as boolean)
    m.statusWords = { key: "", values: invalid, text: text, problem: problem }
    showStatus()
end sub

sub showStatus()
    words = m.statusWords
    text = words.text
    if words.key <> "" then text = t(words.key, words.values)
    m.status.text = text
    if words.problem
        m.status.color = uiColors().danger
    else
        m.status.color = uiColors().muted
    end if
end sub

' The buttons, by what they do: "tryAgain", "newCode", "changeServer".
sub setButtons(actions as object)
    m.buttonActions = actions
    showButtons()
    if actions.Count() > 0 and m.top.isInFocusChain() and not m.globe.isInFocusChain() and not m.languages.visible
        m.actions.focusButton = 0
        m.actions.setFocus(true)
    end if
end sub

sub showButtons()
    words = { tryAgain: "common.actions.tryAgain", newCode: "signin.link.newCode", changeServer: "signin.link.changeServer" }
    labels = []
    for each action in m.buttonActions
        labels.Push(t(words[action]))
    end for
    m.actions.buttons = labels
    m.actions.visible = labels.Count() > 0
end sub

sub onAction(event as object)
    at = event.getData()
    if at < 0 or at >= m.buttonActions.Count() then return
    if m.buttonActions[at] = "changeServer"
        uiNavigate({ to: "servers", reset: true })
    else
        ask()
    end if
end sub

' --- The globe ---

' The languages, Automatic naming what that is on this Roku.
sub openLanguages()
    facts = uiLanguageFacts()
    m.languages.automatic = i18nSystem(facts.serverLanguage, facts.device)
    m.languages.current = facts.choice
    m.languages.open = true
end sub

' A choice shows at once and is kept on this Roku (the scene), Automatic
' forgets it; the focus stays on the globe as the screen is drawn again.
sub onLanguagePicked(event as object)
    m.globe.setFocus(true)
    choice = event.getData()
    if choice = "auto" then choice = ""
    m.top.getScene().languageChoice = choice
end sub

sub onLanguagesClosed()
    m.globe.setFocus(true)
end sub

' Up from the buttons reaches the globe at the top; Down goes back to
' them. Left and Right never do.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press or m.done then return false
    if m.globe.isInFocusChain()
        if key = "OK"
            openLanguages()
        else if key = "down" and m.actions.visible
            m.actions.setFocus(true)
        end if
        return key = "OK" or key = "up" or key = "down" or key = "left" or key = "right" or key = "options"
    end if
    if key = "up" and m.actions.isInFocusChain()
        m.globe.setFocus(true)
        return true
    end if
    return false
end function
