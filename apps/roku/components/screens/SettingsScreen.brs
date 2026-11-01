' Settings: the server, the profile and this Roku; the profile's audio and
' subtitle defaults (Tracks.brs), as on Samsung and LG: Audio language,
' Subtitles, and Subtitle language when subtitles come on by language. OK on
' one opens its choices in a panel at the right; a choice is saved at once
' (PUT /api/me/track-defaults, all three) and shows before the server
' answers, put back if it refuses. A server or sign-in that doesn't let apps
' change them says to change them on the web. Quality is this Roku's own
' (AutoQuality.brs), kept in its registry, and so is "Always start as"
' (Profiles.brs), offered when the household has more than one profile.

sub init()
    m.server = m.top.findNode("server")
    m.profile = m.top.findNode("profile")
    m.device = m.top.findNode("device")
    m.version = m.top.findNode("version")
    m.web = m.top.findNode("web")
    m.buttons = m.top.findNode("buttons")
    m.message = m.top.findNode("message")
    m.panel = m.top.findNode("panel")
    m.panelTitle = m.top.findNode("panelTitle")
    m.choices = m.top.findNode("choices")
    m.panelHint = m.top.findNode("panelHint")
    ' Diagnostics and the licences, at the right: Right and Left go between
    ' them and the settings, and the focus comes back to the one it was on.
    m.about = m.top.findNode("about")
    m.about.buttons = ["Diagnostics", "Open-source licences"]
    m.about.observeField("buttonSelected", "onAbout")
    m.lastFocus = m.buttons
    m.dialog = invalid
    ' The profile's defaults as Settings knows them ({ audio, subtitle,
    ' mode }), invalid until GET /api/auth/me answered; refused once the
    ' server said apps can't change them.
    m.defaults = invalid
    m.refused = false
    m.saving = 0
    ' The household's profiles ([{ id, name }]), for "Always start as".
    m.household = []
    ' What each button does, in order, and the choices the panel shows.
    m.actions = []
    m.choice = invalid
    m.choiceKind = ""
    m.buttons.observeField("buttonSelected", "onButton")
    m.choices.observeField("itemSelected", "onChoice")
    m.choices.observeField("itemFocused", "onChoiceFocused")
    m.top.observeField("active", "onActive")
    showRows()
end sub

sub onActive()
    if not m.top.active then return
    session = m.global.session
    m.server.text = session.serverName + " (" + urlDisplayAddress(session.server) + ")"
    m.device.text = session.device
    m.version.text = session.appVersion
    m.web.text = webAddress()
    if m.panel.visible
        m.choices.setFocus(true)
    else
        m.lastFocus.setFocus(true)
    end if
    uiGet("/api/auth/me", "onMe")
    if authAllows(session.scope, "profiles") then uiGet("/api/oauth/profiles", "onHousehold")
end sub

sub onHousehold(event as object)
    answer = event.getData()
    if not answer.ok or GetInterface(answer.data, "ifArray") = invalid then return
    m.household = profHousehold(answer.data)
    uiKeepHousehold(m.household.Count())
    showRows()
end sub

function startAsSection() as object
    return CreateObject("roRegistrySection", "startAs")
end function

' Settings stay usable when the server doesn't answer: switching server
' and signing out need no answer.
sub onMe(event as object)
    answer = event.getData()
    if answer.ok and answer.data <> invalid
        m.profile.text = httpSentence(answer.data.name)
        ' A choice still on its way stays as chosen.
        if m.saving = 0 then useProfile(answer.data)
    else if answer.error = "network"
        m.profile.text = "The server can't be reached right now."
    end if
end sub

' The profile as the server has it (GET /api/auth/me, or the answer to a
' change).
sub useProfile(me as dynamic)
    if httpObject(me) = invalid then return
    m.defaults = tracksDefaults(me)
    showRows()
end sub

' The buttons: the defaults' rows, this Roku's quality and whether the
' next episode plays by itself, then Watch history, Switch profile, "Always
' start as" (a household of several), Sort libraries, Switch server and Sign
' out; the focus stays on the row it was on.
sub showRows()
    labels = []
    actions = []
    for each row in tracksRows(m.defaults, m.refused)
        labels.Push(row.text)
        actions.Push(row.kind)
    end for
    ' This Roku's, kept on it: no server needed.
    labels.Push("Quality: " + aqLabel(aqChoice(playbackSection())))
    actions.Push("quality")
    autoplay = "Off"
    if playerAutoplay(playbackSection()) then autoplay = "On"
    labels.Push("Play the next episode by itself: " + autoplay)
    actions.Push("autoplay")
    for each action in ["Watch history", "Switch profile"]
        labels.Push(action)
        actions.Push(action)
    end for
    if m.household.Count() > 1
        labels.Push("Always start as: " + profStartAsLabel(m.household, profStartAs(startAsSection(), m.global.session.server)))
        actions.Push("startAs")
    end if
    for each action in ["Sort libraries", "Switch server", "Sign out"]
        labels.Push(action)
        actions.Push(action)
    end for
    focused = ""
    if m.actions.Count() > 0
        at = m.buttons.buttonFocused
        if at >= 0 and at < m.actions.Count() then focused = m.actions[at]
    end if
    if FormatJson(labels) = FormatJson(m.buttons.buttons) then return
    hadFocus = m.buttons.isInFocusChain()
    m.actions = actions
    m.buttons.buttons = labels
    for i = 0 to actions.Count() - 1
        if actions[i] = focused then m.buttons.focusButton = i
    end for
    if hadFocus then m.buttons.setFocus(true)
end sub

sub onButton(event as object)
    chosen = m.actions[event.getData()]
    if chosen = "quality"
        openQuality()
        return
    end if
    if chosen = "autoplay"
        playerSetAutoplay(playbackSection(), not playerAutoplay(playbackSection()))
        showRows()
        return
    end if
    if chosen = "Watch history"
        uiNavigate({ to: "history" })
        return
    end if
    if chosen = "startAs"
        m.message.text = ""
        showChoices("startAs", profStartAsChoices(m.household, profStartAs(startAsSection(), m.global.session.server)))
        return
    end if
    if chosen = "audio" or chosen = "mode" or chosen = "subtitle"
        openChoices(chosen)
        return
    end if
    if chosen = "Sort libraries"
        uiNavigate({ to: "sortLibraries" })
        return
    end if
    if chosen = "Switch profile"
        uiNavigate({ to: "profiles" })
        return
    end if
    if chosen = "Switch server"
        uiNavigate({ to: "servers", reset: true })
        return
    end if
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = "Sign out?"
    dialog.message = ["You'll be signed out of " + m.global.session.serverName + "."]
    dialog.buttons = ["Sign out", "Cancel"]
    dialog.observeField("buttonSelected", "onSignOutButton")
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

' Signed out, the scene shows sign-in (MainScene.onSignedIn).
sub onSignOutButton(event as object)
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
    if event.getData() = 0 then uiRequest({ op: "signOut" }, "")
end sub

' --- The defaults ---

' The choices of one row, the current one checked and focused.
sub openChoices(kind as string)
    if m.defaults = invalid
        ' The profile hasn't come: asked again.
        m.message.text = "Loading your profile…"
        uiGet("/api/auth/me", "onMe")
        return
    end if
    if m.refused
        m.message.text = "Change these on the web, at " + webAddress() + "."
        return
    end if
    m.message.text = ""
    showChoices(kind, tracksChoices(kind, m.defaults))
end sub

' The Quality menu (docs/QUALITY.md), kept on this Roku: Automatic unless
' another was chosen.
sub openQuality()
    m.message.text = ""
    current = aqChoice(playbackSection())
    choice = { title: "Quality", labels: [], values: [], hints: [], current: 0 }
    for each quality in aqMenu()
        if quality.id = current then choice.current = choice.values.Count()
        choice.labels.Push(quality.label)
        choice.values.Push(quality.id)
        choice.hints.Push(aqHint(quality.id))
    end for
    showChoices("quality", choice)
end sub

' Where the quality setting is kept on this Roku (the player reads it).
function playbackSection() as object
    return CreateObject("roRegistrySection", "playback")
end function

sub showChoices(kind as string, choice as object)
    m.choiceKind = kind
    m.choice = choice
    items = []
    for each label in m.choice.labels
        items.Push({ title: label })
    end for
    m.panelTitle.text = m.choice.title
    m.choices.content = nodesContent(items)
    m.choices.checkedItem = m.choice.current
    at = m.choice.current
    if at < 0 then at = 0
    m.choices.jumpToItem = at
    choiceHint(at)
    m.panel.visible = true
    m.choices.setFocus(true)
end sub

sub closeChoices()
    m.panel.visible = false
    m.buttons.setFocus(true)
end sub

sub onChoiceFocused(event as object)
    choiceHint(event.getData())
end sub

' A subtitle mode's hint, under its choices.
sub choiceHint(at as integer)
    text = ""
    if m.choice <> invalid and at >= 0 and at < m.choice.hints.Count() then text = m.choice.hints[at]
    m.panelHint.text = text
end sub

sub onChoice(event as object)
    if m.choice = invalid then return
    at = event.getData()
    if at < 0 or at >= m.choice.values.Count() then return
    closeChoices()
    if m.choiceKind = "quality"
        value = m.choice.values[at]
        if value = "auto" and aqChoice(playbackSection()) <> "auto"
            ' Picked again, Automatic starts afresh at the original, here
            ' and on the server.
            aqRemember(playbackSection(), m.global.session.server, 0, CreateObject("roDateTime").AsSeconds())
            uiRequest({ op: "tellNetwork", ceilingKbps: 0 }, "")
        end if
        aqChoose(playbackSection(), value)
        showRows()
        return
    end if
    if m.choiceKind = "startAs"
        profSetStartAs(startAsSection(), m.global.session.server, m.choice.values[at])
        showRows()
        return
    end if
    saveDefaults(tracksWith(m.defaults, m.choiceKind, m.choice.values[at]))
end sub

' Shows defaults at once and saves them; the server's answer is the
' profile, a refusal puts back what was.
sub saveDefaults(defaults as object)
    if tracksSame(defaults, m.defaults) then return
    before = m.defaults
    m.defaults = defaults
    m.message.text = ""
    showRows()
    m.saving = m.saving + 1
    uiRequest({ method: "PUT", path: "/api/me/track-defaults", jsonText: FormatJson(tracksDefaultsBody(defaults)), before: before }, "onSaved")
end sub

sub onSaved(event as object)
    m.saving = m.saving - 1
    answer = event.getData()
    if answer.ok
        m.message.text = "Saved."
        if m.saving = 0 then useProfile(answer.data)
        return
    end if
    m.defaults = event.getRoSGNode().spec.before
    if answer.error = "signed_out"
        showRows()
        return
    end if
    if tracksRefused(answer.status)
        m.refused = true
        m.message.text = "This server doesn't let the app change these: change them on the web, at " + webAddress() + "."
    else
        m.message.text = answer.message
    end if
    showRows()
end sub

' Where the defaults are changed instead: the web app's account page.
function webAddress() as string
    return urlDisplayAddress(m.global.session.server) + "/account"
end function

sub onAbout(event as object)
    m.lastFocus = m.about
    if event.getData() = 0
        uiNavigate({ to: "diagnostics" })
    else
        uiNavigate({ to: "licences" })
    end if
end sub

' Back closes the choices; left from them does too. Right and Left go
' between the settings and Diagnostics.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.panel.visible
        if key = "back" or key = "left"
            closeChoices()
            return true
        end if
        return false
    end if
    if key = "right" and m.buttons.isInFocusChain()
        m.about.setFocus(true)
        return true
    end if
    if key = "left" and m.about.isInFocusChain()
        m.lastFocus = m.buttons
        m.buttons.setFocus(true)
        return true
    end if
    return false
end function
