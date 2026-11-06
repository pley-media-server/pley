sub init()
    m.title = m.top.findNode("title")
    m.text = m.top.findNode("text")
    m.buttons = m.top.findNode("buttons")
    m.buttons.buttons = [t("common.actions.tryAgain"), t("signin.unreachable.switchServer")]
    m.buttons.observeField("buttonSelected", "onButton")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    session = m.global.session
    m.title.text = t("signin.unreachable.title", { name: session.serverName })
    m.text.text = t("signin.unreachable.body", { address: urlDisplayAddress(session.server) })
    m.buttons.setFocus(true)
    uiLaunched()
end sub

sub onButton(event as object)
    if event.getData() = 1
        uiNavigate({ to: "servers", reset: true })
        return
    end if
    retry = m.top.params.retry
    if retry = invalid or httpString(retry.to) = "" then retry = { to: "home", params: {} }
    uiNavigate({ to: retry.to, params: retry.params, replace: true })
end sub
