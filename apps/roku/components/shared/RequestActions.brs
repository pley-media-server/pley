' What can be done with a request from the screens that show it (a TMDB
' title's page, the requests). On the render thread, with Ui.brs and
' lib/Requests.brs. A screen that includes this file has
' onRequestActed(), called once the server did what an action asked, or
' refused it as the request had changed or gone (m.requestTarget is the
' request it was for).

' What can be done with a request, in a dialog titled with it: target is
' { requestId, title, status, stage, kind, tmdbId, seasons, libraryId,
' actions } (a row's fields, reqRow's, its tmdbKind as kind). Nothing to
' do: false.
function uiRequestOptions(target as object) as boolean
    options = reqActions(target, target.libraryId)
    if options.Count() = 0 then return false
    labels = []
    for each option in options
        labels.Push(option.label)
    end for
    labels.Push(t("common.actions.cancel"))
    m.requestTarget = target
    m.requestOptions = options
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = target.title
    dialog.buttons = labels
    dialog.observeField("buttonSelected", "uiRequestOptionChosen")
    m.top.getScene().dialog = dialog
    return true
end function

sub uiRequestOptionChosen(event as object)
    chosen = event.getData()
    options = m.requestOptions
    m.requestOptions = invalid
    uiCloseDialog()
    if options = invalid or chosen < 0 or chosen >= options.Count() then return
    uiRequestAct(m.requestTarget, options[chosen].action)
end sub

' Does an action (reqActions'): Watch opens the library's title; Request
' again asks as before; Withdraw, Remove and Delete ask first; Decline
' with a reason… asks for the reason on the keyboard; the others go at
' once.
sub uiRequestAct(target as object, action as string)
    m.requestTarget = target
    if action = "watch"
        if httpNumber(target.libraryId, 0) > 0 then uiNavigate({ to: "details", params: { itemId: target.libraryId } })
        return
    end if
    if action = "ask_again"
        uiRequest({ method: "POST", path: "/api/requests", jsonText: reqAskBody(target.kind, target.tmdbId, target.seasons), action: action, requestTitle: target.title }, "uiRequestActed")
        return
    end if
    if action = "declineReason"
        dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
        dialog.title = t("requests.decline.title", { title: target.title })
        dialog.message = [t("requests.decline.text")]
        dialog.buttons = [t("requests.actions.decline"), t("common.actions.cancel")]
        dialog.observeField("buttonSelected", "uiRequestReasonGiven")
        m.top.getScene().dialog = dialog
        return
    end if
    confirm = reqConfirm(action, target.title, httpString(target.status))
    if confirm <> invalid
        m.requestConfirmed = action
        dialog = CreateObject("roSGNode", "StandardMessageDialog")
        dialog.title = confirm.message
        dialog.buttons = [confirm.button, t("common.actions.cancel")]
        dialog.observeField("buttonSelected", "uiRequestConfirmed")
        m.top.getScene().dialog = dialog
        return
    end if
    uiRequestCall(action, "")
end sub

sub uiRequestConfirmed(event as object)
    uiCloseDialog()
    if event.getData() = 0 then uiRequestCall(m.requestConfirmed, "")
end sub

sub uiRequestReasonGiven(event as object)
    dialog = m.top.getScene().dialog
    reason = ""
    if dialog <> invalid then reason = dialog.text
    uiCloseDialog()
    if event.getData() = 0 then uiRequestCall("declineReason", reason)
end sub

sub uiRequestCall(action as string, reason as string)
    target = m.requestTarget
    if target = invalid then return
    spec = reqActionCall(action, target.requestId, reason)
    if spec = invalid then return
    spec.action = action
    spec.requestTitle = target.title
    uiRequest(spec, "uiRequestActed")
end sub

' The server's answer to an action. One it refused says why; the screen
' shows the request as it is now when it changed meanwhile (409, deleted:
' 404) or the limits moved (429).
sub uiRequestActed(event as object)
    answer = event.getData()
    spec = event.getRoSGNode().spec
    if not answer.ok
        if uiFailed(answer) then return
        text = reqActProblem(answer, spec.action, reqSummaryOf(m.top.getScene().requestSummary), httpString(m.requestTarget.kind), CreateObject("roDateTime").AsSeconds())
        uiMessage(t("requests.didntWork"), [text])
        if answer.status = 404 or answer.status = 409 or answer.status = 429 then onRequestActed()
        return
    end if
    if spec.action = "ask_again" then uiMessage(spec.requestTitle, [reqAsked(answer.status, answer.data, spec.requestTitle)])
    onRequestActed()
end sub
