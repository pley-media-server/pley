sub init()
    m.title = m.top.findNode("title")
    m.list = m.top.findNode("list")
    m.actions = m.top.findNode("actions")
    m.status = m.top.findNode("status")
    m.hint = m.top.findNode("hint")
    m.globe = m.top.findNode("globe")
    m.languages = m.top.findNode("languages")
    m.saved = []
    m.found = []
    m.rows = []
    m.started = false
    m.searching = false
    m.listFocused = false
    m.address = ""
    m.dialog = invalid
    m.forgetting = invalid
    ' What the status line says, drawn again in a new language: its key
    ' and values, or a sentence that came written (an answer's message).
    m.statusWords = { key: "", values: invalid, text: "", problem: false }
    ' Where the focus was before Down took it to the globe.
    m.beforeGlobe = invalid
    m.list.observeField("itemSelected", "onServerSelected")
    m.actions.observeField("buttonSelected", "onAction")
    m.languages.observeField("picked", "onLanguagePicked")
    m.languages.observeField("closed", "onLanguagesClosed")
    m.top.observeField("active", "onActive")
    m.top.observeField("language", "relabel")
    relabel()
end sub

' The screen's words in the language shown, the focus where it was.
sub relabel()
    m.title.text = t("signin.servers.title")
    m.hint.text = t("signin.servers.forgetHint")
    focused = m.actions.buttonFocused
    hadFocus = m.actions.isInFocusChain()
    m.actions.buttons = [t("signin.servers.enterAddress"), t("signin.servers.searchNetwork")]
    uiKeepButton(m.actions, focused, hadFocus)
    m.globe.language = m.global.language
    showStatus()
    if m.rows.Count() > 0 then render()
end sub

sub onActive()
    if not m.top.active then return
    if not m.started
        m.started = true
        uiRequest({ op: "listServers" }, "onSaved")
        search()
    end if
    if m.languages.visible
        m.languages.open = true
    else if m.rows.Count() > 0
        m.list.setFocus(true)
    else
        m.actions.setFocus(true)
    end if
end sub

sub onSaved(event as object)
    answer = event.getData()
    if answer.ok then m.saved = answer.data.servers
    render()
end sub

sub search()
    if m.searching then return
    m.searching = true
    setStatus("signin.servers.looking", invalid, false)
    m.discovery = CreateObject("roSGNode", "DiscoveryTask")
    m.discovery.observeField("found", "onFound")
    m.discovery.observeField("done", "onSearched")
    m.discovery.control = "run"
end sub

sub onFound(event as object)
    m.found = event.getData()
    render()
end sub

sub onSearched()
    m.searching = false
    if m.rows.Count() = 0
        setStatus("signin.servers.noneFound", invalid, false)
    else
        setStatus("", invalid, false)
    end if
end sub

sub render()
    focusedUrl = ""
    if m.rows.Count() > 0 and m.list.itemFocused >= 0 and m.list.itemFocused < m.rows.Count() then focusedUrl = m.rows[m.list.itemFocused].url
    m.rows = serversMerge(m.saved, m.found)
    items = []
    anySaved = false
    focusIndex = 0
    for i = 0 to m.rows.Count() - 1
        row = m.rows[i]
        tag = ""
        warning = false
        if row.lookalike
            tag = t("signin.servers.lookalike")
            warning = true
        else if row.unencrypted
            tag = t("signin.servers.notEncrypted")
            warning = true
        else if row.found
            tag = t("signin.servers.onNetwork")
        else if row.saved
            tag = t("signin.servers.saved")
        end if
        if row.saved then anySaved = true
        if row.url = focusedUrl then focusIndex = i
        items.Push({ title: row.name, address: row.address, tag: tag, warning: warning, found: row.found })
    end for
    m.list.content = nodesContent(items)
    m.list.jumpToItem = focusIndex
    m.hint.visible = anySaved
    ' The first servers to show take the focus, once, unless a dialog or
    ' the globe has it.
    if m.rows.Count() > 0 and not m.listFocused and m.top.isInFocusChain() and not m.globe.isInFocusChain() and not m.languages.visible
        m.listFocused = true
        m.list.setFocus(true)
    end if
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

sub onServerSelected(event as object)
    row = m.rows[event.getData()]
    setStatus("signin.servers.connecting", { address: row.address }, false)
    uiRequest({ op: "useServer", url: row.url, name: row.name, id: row.id }, "onServerUsed")
end sub

sub onServerUsed(event as object)
    answer = event.getData()
    if not answer.ok
        setStatusText(answer.message, true)
        return
    end if
    setStatus("", invalid, false)
    if answer.data.signedIn
        uiNavigate({ to: "home", reset: true })
    else
        uiNavigate({ to: "link" })
    end if
end sub

sub onAction(event as object)
    if event.getData() = 0
        askAddress()
    else
        search()
    end if
end sub

sub askAddress()
    dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
    dialog.title = t("signin.address.title")
    dialog.message = [t("signin.address.hint")]
    dialog.buttons = [t("signin.address.connect"), t("common.actions.cancel")]
    dialog.text = m.address
    dialog.observeField("buttonSelected", "onAddressButton")
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onAddressButton(event as object)
    if m.dialog = invalid then return
    address = m.dialog.text.Trim()
    m.dialog.close = true
    m.dialog = invalid
    if event.getData() <> 0 or address = "" then return
    m.address = address
    setStatus("signin.servers.checking", { address: address }, false)
    uiRequest({ op: "checkServer", address: address }, "onChecked")
end sub

sub onChecked(event as object)
    answer = event.getData()
    if not answer.ok
        setStatusText(answer.message, true)
        return
    end if
    server = answer.data
    setStatus("signin.servers.connecting", { address: server.name }, false)
    uiRequest({ op: "useServer", url: server.url, name: server.name, id: "" }, "onServerUsed")
end sub

sub askForget(row as object)
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = t("signin.servers.forgetTitle", { name: row.name })
    dialog.message = [t("signin.servers.forgetBody")]
    dialog.buttons = [t("common.actions.cancel"), t("signin.servers.forget")]
    dialog.observeField("buttonSelected", "onForgetButton")
    m.forgetting = row
    m.dialog = dialog
    m.top.getScene().dialog = dialog
end sub

sub onForgetButton(event as object)
    if m.dialog <> invalid then m.dialog.close = true
    m.dialog = invalid
    if event.getData() <> 1 then return
    uiRequest({ op: "forgetServer", url: m.forgetting.url }, "onForgotten")
end sub

sub onForgotten(event as object)
    answer = event.getData()
    if answer.ok then m.saved = answer.data.servers
    render()
    if m.rows.Count() = 0 and m.top.isInFocusChain() and not m.globe.isInFocusChain() then m.actions.setFocus(true)
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

' The focus goes back from the globe where it came from.
sub leaveGlobe()
    target = m.beforeGlobe
    if target = invalid
        if m.rows.Count() > 0
            target = m.list
        else
            target = m.actions
        end if
    end if
    target.setFocus(true)
end sub

' Down from the servers or the buttons, past their last, reaches the
' globe; Up leaves it. Left and Right never do.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.globe.isInFocusChain()
        if key = "OK"
            openLanguages()
        else if key = "up"
            leaveGlobe()
        end if
        return key = "OK" or key = "up" or key = "down" or key = "left" or key = "right" or key = "options"
    end if
    if key = "down" and (m.list.isInFocusChain() or m.actions.isInFocusChain())
        m.beforeGlobe = m.actions
        if m.list.isInFocusChain() then m.beforeGlobe = m.list
        m.globe.setFocus(true)
        return true
    end if
    if key = "options" and m.list.isInFocusChain() and m.rows.Count() > 0
        row = m.rows[m.list.itemFocused]
        if row.saved
            askForget(row)
            return true
        end if
    end if
    if key = "right" and m.list.isInFocusChain()
        m.actions.setFocus(true)
        return true
    end if
    if key = "left" and m.actions.isInFocusChain() and m.rows.Count() > 0
        m.list.setFocus(true)
        return true
    end if
    return false
end function
