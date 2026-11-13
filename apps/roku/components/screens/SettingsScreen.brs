' Settings: the server, the profile and this Roku; the profile's audio and
' subtitle defaults (Tracks.brs), as on Samsung and LG: Audio language,
' Subtitles, and Subtitle language when subtitles come on by language. OK on
' one opens its choices in a panel at the right; a choice is saved at once
' (PUT /api/me/track-defaults, all three) and shows before the server
' answers, put back if it refuses. A server or sign-in that doesn't let apps
' change them says to change them on the web. Quality is this Roku's own
' (AutoQuality.brs), kept in its registry, and so is "Always start as"
' (Profiles.brs), offered when the household has more than one profile.
' Language, first, is the account's on a server that keeps one (PUT
' /api/me/language), else this Roku's, as the globe picks it (I18n.brs's
' i18nRow); a new one draws the screen again in it, the focus on its row.

sub init()
    m.titleLabel = m.top.findNode("title")
    m.serverLabel = m.top.findNode("serverLabel")
    m.profileLabel = m.top.findNode("profileLabel")
    m.deviceLabel = m.top.findNode("deviceLabel")
    m.appLabel = m.top.findNode("appLabel")
    m.tmdb = m.top.findNode("tmdb")
    m.webLabel = m.top.findNode("webLabel")
    m.webHint = m.top.findNode("webHint")
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
    m.about.observeField("buttonSelected", "onAbout")
    m.lastFocus = m.buttons
    m.dialog = invalid
    ' The profile's defaults as Settings knows them ({ audio, subtitle,
    ' mode }), invalid until GET /api/auth/me answered; refused once the
    ' server said apps can't change them.
    m.defaults = invalid
    m.refused = false
    m.saving = 0
    ' Each language picked counts: an answer to one before the last
    ' changes nothing the last did.
    m.languagePicks = 0
    ' What the message under the address says ({ key, values, text }),
    ' drawn again in a new language.
    m.words = { key: "", values: invalid, text: "" }
    ' Whether "Signed in as" says the server can't be reached.
    m.profileUnreachable = false
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
    m.top.observeField("language", "relabel")
    relabel()
end sub

' The screen's words in the language shown, the focus on the row it was
' on (Language's, for a language just chosen).
sub relabel()
    m.titleLabel.text = t("settings.title")
    m.serverLabel.text = t("settings.server")
    m.profileLabel.text = t("settings.signedInAs")
    m.deviceLabel.text = t("settings.thisRoku")
    m.appLabel.text = t("settings.app")
    m.tmdb.text = t("settings.tmdb")
    m.webLabel.text = t("settings.web")
    m.webHint.text = t("settings.webHint")
    if m.profileUnreachable then m.profile.text = t("settings.unreachable")
    focused = m.about.buttonFocused
    hadFocus = m.about.isInFocusChain()
    m.about.buttons = [t("settings.diagnostics"), t("settings.licences")]
    uiKeepButton(m.about, focused, hadFocus)
    showWords()
    if m.panel.visible then closeChoices()
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
    profKeepHousehold(CreateObject("roRegistrySection", "household"), m.global.session.server, answer.data)
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
        m.profileUnreachable = false
        ' A choice still on its way stays as chosen.
        if m.saving = 0 then useProfile(answer.data)
    else if answer.error = "network"
        m.profileUnreachable = true
        m.profile.text = t("settings.unreachable")
    end if
end sub

' The profile as the server has it (GET /api/auth/me, or the answer to a
' change).
sub useProfile(me as dynamic)
    if httpObject(me) = invalid then return
    m.defaults = tracksDefaults(me)
    showRows()
end sub

' The buttons: Language, the defaults' rows, this Roku's quality and whether the
' next episode plays by itself, then Watch history, Switch profile, "Always
' start as" (a household of several), Sort libraries, Switch server and Sign
' out; the focus stays on the row it was on.
sub showRows()
    labels = [t("settings.rows.language", { value: languageShown() })]
    actions = ["language"]
    for each row in tracksRows(m.defaults, m.refused)
        labels.Push(row.text)
        actions.Push(row.kind)
    end for
    ' This Roku's, kept on it: no server needed.
    labels.Push(t("settings.rows.quality", { value: aqLabel(aqChoice(playbackSection())) }))
    actions.Push("quality")
    autoplay = t("common.states.off")
    if playerAutoplay(playbackSection()) then autoplay = t("common.states.on")
    labels.Push(t("settings.rows.autoplay", { value: autoplay }))
    actions.Push("autoplay")
    for each action in ["history", "switchProfile"]
        labels.Push(t("settings.rows." + action))
        actions.Push(action)
    end for
    if m.household.Count() > 1
        labels.Push(t("settings.rows.alwaysStartAs", { value: profStartAsLabel(m.household, profStartAs(startAsSection(), m.global.session.server)) }))
        actions.Push("startAs")
    end if
    for each action in ["sortLibraries", "switchServer", "signOut"]
        labels.Push(t("settings.rows." + action))
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
        if actions[i] = focused then uiKeepButton(m.buttons, i, hadFocus)
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
    if chosen = "language"
        openLanguages()
        return
    end if
    if chosen = "history"
        uiNavigate({ to: "history" })
        return
    end if
    if chosen = "startAs"
        say("")
        showChoices("startAs", profStartAsChoices(m.household, profStartAs(startAsSection(), m.global.session.server)))
        return
    end if
    if chosen = "audio" or chosen = "mode" or chosen = "subtitle"
        openChoices(chosen)
        return
    end if
    if chosen = "sortLibraries"
        uiNavigate({ to: "sortLibraries" })
        return
    end if
    if chosen = "switchProfile"
        uiNavigate({ to: "profiles" })
        return
    end if
    if chosen = "switchServer"
        uiNavigate({ to: "servers", reset: true })
        return
    end if
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = t("settings.signOut.title")
    dialog.message = [t("settings.signOut.body", { name: m.global.session.serverName })]
    dialog.buttons = [t("settings.signOut.confirm"), t("common.actions.cancel")]
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
        say("settings.loadingProfile")
        uiGet("/api/auth/me", "onMe")
        return
    end if
    if m.refused
        say("settings.changeOnWeb", { address: webAddress() })
        return
    end if
    say("")
    showChoices(kind, tracksChoices(kind, m.defaults))
end sub

' The Quality menu (docs/QUALITY.md), kept on this Roku: Automatic unless
' another was chosen.
sub openQuality()
    say("")
    current = aqChoice(playbackSection())
    choice = { title: t("settings.quality"), labels: [], values: [], hints: [], current: 0 }
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
    if m.choiceKind = "language"
        chooseLanguage(m.choice.values[at])
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
    say("")
    showRows()
    m.saving = m.saving + 1
    uiRequest({ method: "PUT", path: "/api/me/track-defaults", jsonText: FormatJson(tracksDefaultsBody(defaults)), before: before }, "onSaved")
end sub

sub onSaved(event as object)
    m.saving = m.saving - 1
    answer = event.getData()
    if answer.ok
        say("common.states.saved")
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
        say("settings.readOnly", { address: webAddress() })
    else
        sayText(answer.message)
    end if
    showRows()
end sub

' The message under the address: key's words ("" for none), or one as it
' came written (an answer's).
sub say(key as string, values = invalid as dynamic)
    m.words = { key: key, values: values, text: "" }
    showWords()
end sub

sub sayText(text as string)
    m.words = { key: "", values: invalid, text: text }
    showWords()
end sub

sub showWords()
    text = m.words.text
    if m.words.key <> "" then text = t(m.words.key, m.words.values)
    m.message.text = text
end sub

' --- The language ---

' What the row says: Automatic, naming what that is, or the language
' chosen, in itself.
function languageShown() as string
    row = i18nRow(uiLanguageFacts())
    if row.current <> "" then return i18nName(row.current)
    return t("settings.language.automatic", { language: i18nName(row.automatic) })
end function

sub openLanguages()
    say("")
    row = i18nRow(uiLanguageFacts())
    choice = i18nChoices(row.automatic, row.current)
    choice.title = t("settings.language.title")
    choice.hints = []
    showChoices("language", choice)
end sub

' A language chosen ("" for Automatic): the account's, on a server that
' keeps one, shown once the server has it (the API task hears it in the
' answer); else this Roku's, shown at once.
sub chooseLanguage(language as string)
    m.languagePicks = m.languagePicks + 1
    if i18nRow(uiLanguageFacts()).onDevice
        m.top.getScene().languageChoice = language
        showRows()
        return
    end if
    body = "{""language"":null}"
    if language <> "" then body = FormatJson({ language: language })
    spec = { method: "PUT", path: "/api/me/language", jsonText: body, profile: true, language: language, pick: m.languagePicks }
    uiRequest(spec, "onLanguageSaved")
end sub

' A Pley from before languages has no such route (404, or 405): the
' choice is this Roku's while the server is the one in use, unless one
' was picked since. Any other refusal says why.
sub onLanguageSaved(event as object)
    answer = event.getData()
    if answer.ok
        showRows()
        return
    end if
    if answer.status = 404 or answer.status = 405
        spec = event.getRoSGNode().spec
        scene = m.top.getScene()
        scene.languageRefused = m.global.session.server
        if spec.pick = m.languagePicks then scene.languageChoice = spec.language
        showRows()
        return
    end if
    if not uiFailed(answer) then sayText(answer.message)
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
