' "Who's watching?" (GET /api/oauth/profiles), as the other TVs ask it: OK
' on a profile switches to it (the switch-profile grant, through the API
' task), after its PIN for one that has one; the one signed in goes on,
' its PIN asked first at a start. A profile that needs its password is
' signed in to with a code, chosen in the browser. Shown as the app
' starts when the household has more than one profile, and from Settings.

sub init()
    m.profiles = m.top.findNode("profiles")
    m.message = m.top.findNode("message")
    m.buttons = m.top.findNode("buttons")
    m.started = false
    m.list = []
    ' Who the app is (profMe), and the answers load waits for.
    m.me = invalid
    m.answers = {}
    ' The profile a PIN is being typed for, and the dialog.
    m.picked = invalid
    m.dialog = invalid
    m.busy = false
    ' A PIN went with the switch under way.
    m.sentPin = false
    m.profiles.observeField("itemSelected", "onPicked")
    m.buttons.observeField("buttonSelected", "onButton")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if not m.started
        m.started = true
        load()
    end if
    if m.list.Count() > 0
        m.profiles.setFocus(true)
    else if m.buttons.visible
        m.buttons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

' A sign-in from before the app asked to switch profiles can't list them:
' signing in again, choosing the profile in the browser, does instead.
sub load()
    if not authAllows(m.global.session.scope, "profiles")
        showMessage("This sign-in can't switch profiles: sign in again, and choose the profile in the browser.", ["Sign in again"])
        return
    end if
    showMessage("Loading…", [])
    m.answers = {}
    uiGet("/api/oauth/profiles", "onProfiles")
    ' Who the app is, for the PIN of the profile signed in; alongside the
    ' list.
    uiGet("/api/auth/me", "onMe")
end sub

sub onProfiles(event as object)
    m.answers.profiles = event.getData()
    if m.answers.DoesExist("me") then showProfiles()
end sub

sub onMe(event as object)
    m.answers.me = event.getData()
    if m.answers.DoesExist("profiles") then showProfiles()
end sub

sub showProfiles()
    answer = m.answers.profiles
    ' Without who the app is, a start can't tell whether to ask its PIN:
    ' that is tried again too.
    if answer.ok and not m.answers.me.ok then answer = m.answers.me
    if not answer.ok
        if answer.error = "signed_out" then return
        message = answer.message
        if answer.status = 403 then message = "This sign-in can't switch profiles: sign in again, and choose the profile in the browser."
        showMessage(message, ["Try again", "Sign in again"])
        return
    end if
    m.me = profMe(m.answers.me.data)
    m.list = profTiles(answer.data, m.global.session.server, m.me)
    profKeepHousehold(CreateObject("roRegistrySection", "household"), m.global.session.server, answer.data)
    if m.list.Count() = 0
        showMessage("No profiles to choose from.", ["Try again", "Sign in again"])
        return
    end if
    shown = m.list.Count()
    if shown > 6 then shown = 6
    m.profiles.numColumns = shown
    m.profiles.translation = [(1920 - shown * 220 - (shown - 1) * 48) / 2, 400]
    m.profiles.content = nodesContent(m.list)
    for i = 0 to m.list.Count() - 1
        if m.list[i].current then m.profiles.jumpToItem = i
    end for
    showMessage("", [])
    if m.top.isInFocusChain() then m.profiles.setFocus(true)
end sub

sub showMessage(text as string, buttons as object)
    m.message.text = text
    m.buttons.buttons = buttons
    m.buttons.visible = buttons.Count() > 0
    if buttons.Count() > 0 and m.top.isInFocusChain() then m.buttons.setFocus(true)
end sub

sub onButton(event as object)
    if m.busy then return
    if m.buttons.buttons[event.getData()] = "Try again"
        load()
    else
        uiNavigate({ to: "link" })
    end if
end sub

sub onPicked(event as object)
    if m.busy then return
    profile = m.list[event.getData()]
    m.message.text = ""
    choice = profChoice(profile, m.me, not m.top.getScene().profileChosen)
    if choice = "done"
        done()
    else if choice = "switch"
        switchTo(profile, "")
    else if choice = "confirm" or choice = "pin"
        askPin(profile, "")
    else if choice = "approve"
        askSignIn(profile)
    else
        m.message.text = "This profile can't be used from the app."
    end if
end sub

sub askPin(profile as object, problem as string)
    m.picked = profile
    dialog = CreateObject("roSGNode", "StandardPinPadDialog")
    dialog.title = "PIN for " + profile.title
    lines = []
    if problem <> "" then lines.Push(problem)
    dialog.message = lines
    dialog.buttons = ["OK", "Cancel"]
    dialog.observeField("buttonSelected", "onPin")
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onPin(event as object)
    pin = ""
    if m.dialog <> invalid then pin = httpString(m.dialog.pin)
    closeDialog()
    if event.getData() <> 0 or m.picked = invalid then return
    if Len(pin) < 4
        askPin(m.picked, "PINs are 4 to 8 digits.")
        return
    end if
    switchTo(m.picked, pin)
end sub

' The profile needs its password, which apps never see: sign in to it with
' a code instead, choosing it in the browser.
sub askSignIn(profile as object)
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = profile.title
    dialog.message = ["This profile needs its password: sign in to it with a code, and choose it in the browser."]
    dialog.buttons = ["Sign in with a code", "Cancel"]
    dialog.observeField("buttonSelected", "onSignIn")
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onSignIn(event as object)
    closeDialog()
    if event.getData() = 0 then uiNavigate({ to: "link" })
end sub

sub closeDialog()
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
end sub

sub switchTo(profile as object, pin as string)
    m.busy = true
    m.picked = profile
    m.sentPin = pin <> ""
    m.message.text = "Switching to " + profile.title + "…"
    uiRequest({ op: "switchProfile", profile: profile.itemId, pin: pin }, "onSwitched")
end sub

sub onSwitched(event as object)
    m.busy = false
    answer = event.getData()
    ' No PIN to check is fine for the profile signed in only.
    if answer.ok and (not answer.data.through or m.picked.current)
        done()
        return
    end if
    if answer.ok
        m.message.text = "The server couldn't switch to " + m.picked.title + "."
        return
    end if
    m.message.text = ""
    if answer.error = "signed_out" then return
    if answer.error = "wrong_pin" and not m.sentPin
        ' The list was older than the profile's PIN: it is asked for.
        askPin(m.picked, "")
    else if answer.error = "wrong_pin" or answer.error = "throttled"
        ' The pad again, saying why: a wrong PIN, or how long to wait. A
        ' locked one says so on the screen, as the pad can't be dimmed.
        askPin(m.picked, answer.message)
    else if answer.error = "browser"
        askSignIn(m.picked)
    else
        m.message.text = answer.message
    end if
end sub

' While a switch is under way the screen stays: Back would leave the app
' half switched.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press or not m.busy then return false
    return key = "back" or key = "OK" or key = "options"
end function

' The profile chosen: Home, everything as that profile sees it.
sub done()
    m.top.getScene().profileChosen = true
    uiNavigate({ to: "home", reset: true })
end sub
