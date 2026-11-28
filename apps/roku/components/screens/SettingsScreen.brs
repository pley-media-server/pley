' Settings in two panes, as the Android TV app's (TvSettingsScreen) and
' Roku's own: the categories at the left, the focused one's rows at the
' right. Right or OK on a category goes into its rows at the first, Left or
' Back comes out again; OK on Sign out asks first. Android's Audio and
' Subtitles & display are the Roku's own settings, so Roku has no such
' categories.
'
' Profile has the profile's audio and subtitle defaults (Tracks.brs), as on
' Samsung and LG: Audio language, Subtitles, and Subtitle language when
' subtitles come on by language. OK on one opens its choices in a panel at
' the right; a choice is saved at once (PUT /api/me/track-defaults, all
' three) and shows before the server answers, put back if it refuses. A
' server or sign-in that doesn't let apps change them says to change them
' on the web. Quality is this Roku's own (AutoQuality.brs), kept in its
' registry, and so is "Always start as" (Profiles.brs), offered when the
' household has more than one profile. Language is the account's on a
' server that keeps one (PUT /api/me/language), else this Roku's, as the
' globe picks it (I18n.brs's i18nRow); a new one draws the screen again in
' it, the focus on its row.

sub init()
    m.c = uiColors()
    ' The page dimmed behind the panel, in the theme's shade.
    m.top.findNode("dim").color = themeShade(m.c, 0.5)
    m.titleLabel = m.top.findNode("title")
    m.message = m.top.findNode("message")
    m.categories = m.top.findNode("categories")
    m.header = m.top.findNode("header")
    m.face = m.top.findNode("face")
    m.initial = m.top.findNode("initial")
    m.picture = m.top.findNode("picture")
    m.nameLabel = m.top.findNode("name")
    m.addressLabel = m.top.findNode("address")
    m.intro = m.top.findNode("intro")
    m.notes = m.top.findNode("notes")
    m.panel = m.top.findNode("panel")
    m.panelTitle = m.top.findNode("panelTitle")
    m.choices = m.top.findNode("choices")
    m.panelHint = m.top.findNode("panelHint")
    m.panelTitle.font = uiType("section")
    m.panelHint.font = uiType("caption")
    m.lastFocus = m.categories
    m.dialog = invalid
    ' The categories, top to bottom, and the one whose rows show.
    m.categoryKeys = ["profile", "playback", "appearance", "server", "diagnostics", "about", "signOut"]
    m.category = "profile"
    ' Made again (a theme picked in Appearance), it opens on the category
    ' it was left on (its params' category), once.
    m.placed = false
    ' The profile signed in as GET /api/auth/me has it, invalid until it
    ' answered.
    m.me = invalid
    ' The profile's defaults as Settings knows them ({ audio, subtitle,
    ' mode }), invalid until GET /api/auth/me answered; refused once the
    ' server said apps can't change them.
    m.defaults = invalid
    m.refused = false
    m.saving = 0
    ' Each language picked counts: an answer to one before the last
    ' changes nothing the last did.
    m.languagePicks = 0
    ' What the message at the top right says ({ key, values, text }), drawn
    ' again in a new language.
    m.words = { key: "", values: invalid, text: "" }
    ' Whether Profile's head says the server can't be reached.
    m.profileUnreachable = false
    ' The household's profiles ([{ id, name }]), for "Always start as".
    m.household = []
    ' Each category's rows in a list of its own, made once and filled again
    ' only when what they show changes: moving between categories shows
    ' another list, which costs a Roku nothing. For each, what its rows do
    ' in order, what they show, where they start and the notes under them.
    m.lists = {}
    m.actions = {}
    m.shown = {}
    m.tops = {}
    m.notesOf = {}
    panes = m.top.findNode("panes")
    for each key in m.categoryKeys
        list = panes.createChild("QuickList")
        list.update({ itemComponentName: "SettingRow", itemSize: [1060, 96], drawFocusFeedback: false, vertFocusAnimationStyle: "floatingFocus", visible: false })
        list.observeField("itemSelected", "onRow")
        m.lists[key] = list
        m.actions[key] = []
        m.shown[key] = ""
        m.tops[key] = 150
        m.notesOf[key] = ""
    end for
    m.rows = m.lists[m.category]
    ' What the choices the panel shows are.
    m.choice = invalid
    m.choiceKind = ""
    m.picture.setHttpAgent(m.top.getScene().getHttpAgent())
    m.categories.observeField("itemFocused", "onCategoryFocused")
    m.categories.observeField("itemSelected", "onCategory")
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
    showWords()
    if m.panel.visible then closeChoices()
    showCategories()
    showHeader()
    showPane()
end sub

sub showCategories()
    icons = { profile: "user-round", playback: "play", appearance: "palette", server: "server", diagnostics: "activity", about: "info", signOut: "log-out" }
    items = []
    for each key in m.categoryKeys
        label = t("settings.rows.signOut")
        if key <> "signOut" then label = t("settings.categories." + key)
        items.Push({ title: label, description: "", icon: icons[key], trailing: "", checked: false, danger: key = "signOut", isCategory: true, held: false })
    end for
    at = m.categories.itemFocused
    m.categories.content = nodesContent(items)
    if at > 0 then m.categories.jumpToItem = at
    holdCategory(m.lastFocus.isSameNode(m.rows))
end sub

' The category whose rows have the focus keeps a muted fill.
sub holdCategory(held as boolean)
    content = m.categories.content
    if content = invalid then return
    for i = 0 to content.getChildCount() - 1
        content.getChild(i).held = held and m.categoryKeys[i] = m.category
    end for
end sub

sub onActive()
    if not m.top.active then return
    if not m.placed then placeCategory()
    showPane()
    if m.panel.visible
        m.choices.setFocus(true)
    else
        m.lastFocus.setFocus(true)
    end if
    uiGet("/api/auth/me", "onMe")
    if authAllows(m.global.session.scope, "profiles") then uiGet("/api/oauth/profiles", "onHousehold")
end sub

sub onHousehold(event as object)
    answer = event.getData()
    if not answer.ok or GetInterface(answer.data, "ifArray") = invalid then return
    m.household = profHousehold(answer.data)
    profKeepHousehold(CreateObject("roRegistrySection", "household"), m.global.session.server, answer.data)
    showPane()
end sub

function startAsSection() as object
    return CreateObject("roRegistrySection", "startAs")
end function

' Settings stay usable when the server doesn't answer: switching server
' and signing out need no answer.
sub onMe(event as object)
    answer = event.getData()
    if answer.ok and answer.data <> invalid
        m.me = httpObject(answer.data)
        m.profileUnreachable = false
        showHeader()
        ' A choice still on its way stays as chosen.
        if m.saving = 0 then useProfile(answer.data)
    else if answer.error = "network"
        m.profileUnreachable = true
        showHeader()
    end if
end sub

' Profile's head: the profile's picture, or its initial on its color
' (Avatar.brs), its name, and the server's address (or that it can't be
' reached).
sub showHeader()
    session = m.global.session
    m.addressLabel.text = urlDisplayAddress(session.server)
    if m.profileUnreachable then m.addressLabel.text = t("settings.unreachable")
    if m.me <> invalid
        m.nameLabel.text = httpSentence(m.me.name)
        avatar = avatarOf(m.me)
        picture = m.me.avatar_url
    else
        ' Until the server answers, or when it can't be reached, the
        ' profile as the app signed in heard it (the rail's), so the circle
        ' is never the theme's.
        avatar = session.profileAvatar
        if avatar = invalid or avatar.initial = invalid
            m.nameLabel.text = "…"
            return
        end if
        m.nameLabel.text = session.profileName
        picture = avatar.picture
    end if
    m.initial.text = avatar.initial
    m.initial.font = uiFontFor(avatar.initial, "bold", 44)
    m.initial.color = avatar.ink
    m.face.blendColor = avatar.color
    m.picture.uri = urlImage(session.server, picture, 96)
end sub

' The profile as the server has it (GET /api/auth/me, or the answer to a
' change).
sub useProfile(me as dynamic)
    if httpObject(me) = invalid then return
    m.defaults = tracksDefaults(me)
    showPane()
end sub

sub onCategoryFocused(event as object)
    at = event.getData()
    if at < 0 or at >= m.categoryKeys.Count() then return
    m.category = m.categoryKeys[at]
    selectPane()
end sub

' OK on a category goes into its rows; on Sign out, it asks.
sub onCategory()
    if m.category = "signOut"
        askSignOut()
        return
    end if
    enterRows()
end sub

sub enterRows()
    if m.actions[m.category].Count() = 0 then return
    m.rows.jumpToItem = 0
    m.rows.setFocus(true)
    m.lastFocus = m.rows
    holdCategory(true)
end sub

sub leaveRows()
    m.categories.setFocus(true)
    m.lastFocus = m.categories
    holdCategory(false)
end sub

' Every category's rows, each list filled again where what it shows
' changed, and the focused category's shown.
sub showPane()
    for each key in m.categoryKeys
        fillPane(key)
    end for
    selectPane()
end sub

' The focused category's list, and what goes with it: Profile's head above
' its rows, Sign out's words, and the notes under Diagnostics' and About's.
sub selectPane()
    for each key in m.categoryKeys
        m.lists[key].visible = key = m.category
    end for
    m.rows = m.lists[m.category]
    m.header.visible = m.category = "profile"
    m.intro.visible = m.category = "signOut"
    notes = m.notesOf[m.category]
    m.notes.text = notes
    m.notes.translation = [792, m.tops[m.category] + m.actions[m.category].Count() * 96 + 28]
end sub

' The rows of a category, as the Android TV app's, each its icon, its value
' under its label; the focus stays on the row it was on.
sub fillPane(category as string)
    session = m.global.session
    rows = []
    notes = ""
    top = 150
    if category = "profile"
        top = 274
        rows.Push(settingRow("switchProfile", t("settings.rows.switchProfile"), "", "users"))
        for each row in tracksRows(m.defaults, m.refused)
            icon = "captions"
            if row.kind = "audio" then icon = "audio-lines"
            rows.Push(settingRow(row.kind, row.label, row.value, icon))
        end for
        rows.Push(settingRow("language", t("settings.language.title"), languageShown(), "globe"))
        rows.Push(settingRow("history", t("settings.rows.history"), "", "history"))
        rows.Push(settingRow("sortLibraries", t("settings.rows.sortLibraries"), "", "arrow-up-down"))
        if m.household.Count() > 1
            rows.Push(settingRow("startAs", t("settings.alwaysStartAs"), profStartAsLabel(m.household, profStartAs(startAsSection(), session.server)), "user-round"))
        end if
        ' Roku has no QR code of its own: the address, and OK says what
        ' is managed there.
        rows.Push(settingRow("web", t("settings.web"), webAddress(), "app-window"))
    else if category = "playback"
        ' This Roku's, kept on it: no server needed.
        rows.Push(settingRow("quality", t("settings.quality"), aqLabel(aqChoice(playbackSection())), "gauge"))
        autoplay = settingRow("autoplay", t("settings.autoplay"), "", "skip-forward")
        autoplay.trailing = "switch"
        autoplay.checked = playerAutoplay(playbackSection())
        rows.Push(autoplay)
    else if category = "appearance"
        ' This Roku's, for the profile: no server needed. Each opens the
        ' Appearance page, the accent's on the accents.
        rows.Push(settingRow("theme", t("settings.appearance.theme"), themeShownName(), "palette"))
        rows.Push(settingRow("accent", t("settings.appearance.accent.title"), accentShownName(), "droplet"))
    else if category = "server"
        server = settingRow("server", session.serverName, urlDisplayAddress(session.server) + " · " + t("settings.serverValue"), "server")
        server.trailing = ""
        rows.Push(server)
        rows.Push(settingRow("switchServer", t("settings.rows.switchServer"), "", "arrow-left"))
    else if category = "diagnostics"
        rows.Push(settingRow("diagnostics", t("settings.openDiagnostics"), t("settings.openDiagnosticsValue"), "activity"))
        notes = t("settings.thisRoku") + " · " + session.device
    else if category = "about"
        version = settingRow("version", t("settings.version"), session.appVersion, "info")
        version.trailing = ""
        rows.Push(version)
        rows.Push(settingRow("licences", t("settings.licences"), "", "library-big"))
        notes = t("settings.tmdb")
    else
        m.intro.text = t("settings.signOut.body", { name: session.serverName })
        top = 166 + m.intro.boundingRect().height + 20
        signOut = settingRow("signOut", t("settings.rows.signOut"), "", "log-out")
        signOut.trailing = ""
        signOut.danger = true
        rows.Push(signOut)
    end if
    list = m.lists[category]
    m.notesOf[category] = notes
    m.tops[category] = top
    list.translation = [764, top]
    list.numRows = Int((1026 - top) / 96)
    shown = FormatJson(rows)
    if shown = m.shown[category] then return
    actions = m.actions[category]
    focused = ""
    at = list.itemFocused
    if at >= 0 and at < actions.Count() then focused = actions[at]
    hadFocus = list.isInFocusChain()
    m.shown[category] = shown
    actions = []
    for each row in rows
        actions.Push(row.action)
    end for
    m.actions[category] = actions
    list.content = nodesContent(rows)
    for i = 0 to actions.Count() - 1
        if actions[i] = focused and i > 0 then list.jumpToItem = i
    end for
    if hadFocus then list.setFocus(true)
end sub

' A row: what it does, its label, its value ("" for none) and its icon; it
' opens something (a chevron) unless said otherwise.
function settingRow(action as string, label as string, value as string, icon as string) as object
    return { action: action, title: label, description: value, icon: icon, trailing: "chevron", checked: false, danger: false, isCategory: false }
end function

sub onRow(event as object)
    at = event.getData()
    actions = m.actions[m.category]
    if at < 0 or at >= actions.Count() then return
    chosen = actions[at]
    if chosen = "quality"
        openQuality()
        return
    end if
    if chosen = "autoplay"
        playerSetAutoplay(playbackSection(), not playerAutoplay(playbackSection()))
        showPane()
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
    if chosen = "theme" or chosen = "accent"
        ' Made again in a theme picked there, this screen comes back here.
        m.top.params = { category: "appearance", row: at }
        uiNavigate({ to: "appearance", params: { focus: chosen } })
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
    if chosen = "diagnostics" or chosen = "licences"
        uiNavigate({ to: chosen })
        return
    end if
    if chosen = "web"
        showWeb()
        return
    end if
    if chosen = "signOut" then askSignOut()
end sub

' Where the account is managed, and what is managed there, as the Android
' TV app's web dialog has it without its QR code.
sub showWeb()
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = t("settings.web")
    dialog.message = [webAddress(), t("settings.webHint")]
    dialog.buttons = [t("common.actions.close")]
    dialog.observeField("buttonSelected", "onDialogButton")
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onDialogButton()
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
end sub

sub askSignOut()
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
    for i = 0 to m.choice.labels.Count() - 1
        items.Push({ title: m.choice.labels[i], checked: i = m.choice.current })
    end for
    m.panelTitle.text = m.choice.title
    m.choices.content = nodesContent(items)
    at = m.choice.current
    if at < 0 then at = 0
    m.choices.jumpToItem = at
    choiceHint(at)
    m.panel.visible = true
    m.choices.setFocus(true)
end sub

sub closeChoices()
    m.panel.visible = false
    m.rows.setFocus(true)
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
        showPane()
        return
    end if
    if m.choiceKind = "startAs"
        profSetStartAs(startAsSection(), m.global.session.server, m.choice.values[at])
        showPane()
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
    showPane()
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
        showPane()
        return
    end if
    if tracksRefused(answer.status)
        m.refused = true
        say("settings.readOnly", { address: webAddress() })
    else
        sayText(answer.message)
    end if
    showPane()
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
    ' 简体中文 named in another language, in a font that has it.
    m.message.font = uiFontFor(text, "regular", 30)
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
        showPane()
        return
    end if
    body = "{""language"":null}"
    if language <> "" then body = FormatJson({ language: language })
    spec = { method: "PUT", path: "/api/me/language", jsonText: body, profile: true, language: language, pick: m.languagePicks }
    uiRequest(spec, "onLanguageSaved")
end sub

' A Pley from before languages has no such route (404, or 405): the
' choice is this Roku's while the server is the one in use, unless one
' was picked since. One from before a language refuses it (400): the
' server wants updating. Any other refusal says why.
sub onLanguageSaved(event as object)
    answer = event.getData()
    if answer.ok
        showPane()
        return
    end if
    if answer.status = 404 or answer.status = 405
        spec = event.getRoSGNode().spec
        scene = m.top.getScene()
        scene.languageRefused = m.global.session.server
        if spec.pick = m.languagePicks then scene.languageChoice = spec.language
        showPane()
        return
    end if
    spec = event.getRoSGNode().spec
    if answer.status = 400 and spec.language <> ""
        sayText(t("settings.language.tooOld", { language: i18nName(spec.language) }))
        return
    end if
    if not uiFailed(answer) then sayText(answer.message)
end sub

' The category and row the screen was made on (its params'), the row in
' focus.
sub placeCategory()
    m.placed = true
    params = m.top.params
    if params = invalid or params.category = invalid then return
    for i = 0 to m.categoryKeys.Count() - 1
        if m.categoryKeys[i] = params.category
            m.categories.jumpToItem = i
            m.category = params.category
            selectPane()
            showPane()
            if params.row <> invalid and params.row > 0 then m.rows.jumpToItem = params.row
            m.lastFocus = m.rows
            holdCategory(true)
        end if
    end for
end sub

' The theme shown, as Appearance names it.
function themeShownName() as string
    return t("settings.appearance.names." + m.global.theme)
end function

' The accent shown, as Appearance names it: Default for the theme's own.
function accentShownName() as string
    if m.global.accent = "" then return t("settings.appearance.accent.own")
    return t("settings.appearance.accent.names." + m.global.accent)
end function

' Where the defaults are changed instead: the web app's account page.
function webAddress() as string
    return urlDisplayAddress(m.global.session.server) + "/account"
end function

' Back closes the choices; left from them does too. From the rows, Left
' and Back go back to the categories; from a category, Right goes into its
' rows (Left opens the menu, Back leaves, as on every page).
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.panel.visible
        if key = "back" or key = "left"
            closeChoices()
            return true
        end if
        return false
    end if
    if m.rows.hasFocus() and (key = "back" or key = "left")
        leaveRows()
        return true
    end if
    if m.categories.hasFocus() and key = "right"
        enterRows()
        return true
    end if
    return false
end function
