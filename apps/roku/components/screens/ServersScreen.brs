sub init()
    m.list = m.top.findNode("list")
    m.actions = m.top.findNode("actions")
    m.status = m.top.findNode("status")
    m.hint = m.top.findNode("hint")
    m.actions.buttons = ["Enter an address", "Search this network"]
    m.saved = []
    m.found = []
    m.rows = []
    m.started = false
    m.searching = false
    m.listFocused = false
    m.address = ""
    m.dialog = invalid
    m.forgetting = invalid
    m.list.observeField("itemSelected", "onServerSelected")
    m.actions.observeField("buttonSelected", "onAction")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if not m.started
        m.started = true
        uiRequest({ op: "listServers" }, "onSaved")
        search()
    end if
    if m.rows.Count() > 0
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
    setStatus("Looking for Pley on your network…", false)
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
        setStatus("No servers found. Make sure this TV and the server are on the same network, or enter the server's address.", false)
    else
        setStatus("", false)
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
            tag = "Not your saved server"
            warning = true
        else if row.unencrypted
            tag = "Not encrypted"
            warning = true
        else if row.found
            tag = "On this network"
        else if row.saved
            tag = "Saved"
        end if
        if row.saved then anySaved = true
        if row.url = focusedUrl then focusIndex = i
        items.Push({ title: row.name, address: row.address, tag: tag, warning: warning, found: row.found })
    end for
    m.list.content = nodesContent(items)
    m.list.jumpToItem = focusIndex
    m.hint.visible = anySaved
    ' The first servers to show take the focus, once, unless a dialog has it.
    if m.rows.Count() > 0 and not m.listFocused and m.top.isInFocusChain()
        m.listFocused = true
        m.list.setFocus(true)
    end if
end sub

sub setStatus(text as string, problem as boolean)
    m.status.text = text
    if problem
        m.status.color = uiColors().danger
    else
        m.status.color = uiColors().muted
    end if
end sub

sub onServerSelected(event as object)
    row = m.rows[event.getData()]
    setStatus("Connecting to " + row.address + "…", false)
    uiRequest({ op: "useServer", url: row.url, name: row.name, id: row.id }, "onServerUsed")
end sub

sub onServerUsed(event as object)
    answer = event.getData()
    if not answer.ok
        setStatus(answer.message, true)
        return
    end if
    setStatus("", false)
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
    dialog.title = "Enter your server's address"
    dialog.message = ["Such as 192.168.1.20, nas or pley.example.com. Port 7539 is used by default."]
    dialog.buttons = ["Connect", "Cancel"]
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
    setStatus("Checking " + address + "…", false)
    uiRequest({ op: "checkServer", address: address }, "onChecked")
end sub

sub onChecked(event as object)
    answer = event.getData()
    if not answer.ok
        setStatus(answer.message, true)
        return
    end if
    server = answer.data
    setStatus("Connecting to " + server.name + "…", false)
    uiRequest({ op: "useServer", url: server.url, name: server.name, id: "" }, "onServerUsed")
end sub

sub askForget(row as object)
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = "Forget " + row.name + "?"
    dialog.message = ["Pley signs this TV out of it and deletes what it saved from it."]
    dialog.buttons = ["Cancel", "Forget"]
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
    if m.rows.Count() = 0 and m.top.isInFocusChain() then m.actions.setFocus(true)
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
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
