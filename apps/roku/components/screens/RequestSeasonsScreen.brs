' Which seasons? (the season picker): every season of a show, specials
' last, each with how it stands (lib/Requests.brs's reqPickerRows). The
' askable missing ones are ticked to start with, lowest first, as many as
' may be asked for; OK ticks or unticks one. Request asks for those ticked
' (POST /api/requests), then the title's page shows again, saying what
' came of it. Roku's buttons can't be disabled: with none ticked, or more
' than may be asked for, Request goes and its line shows. params: { kind,
' tmdbId, title, year, detail (DiscoverDetail), only (a season's number,
' ticked alone; -1 for the usual ones) }.

sub init()
    m.top.findNode("heading").text = t("requests.picker.title")
    m.subtitle = m.top.findNode("subtitle")
    m.seasons = m.top.findNode("seasons")
    m.words = m.top.findNode("words")
    m.allowance = m.top.findNode("allowance")
    m.buttons = m.top.findNode("buttons")
    m.line = m.top.findNode("line")
    m.started = false
    ' The seasons as listed ({ number, title, line, askable, ticked }), and
    ' what Request knows: seasons left (-1: no limit), the allowance's words.
    m.rows = []
    m.remaining = -1
    m.allowanceText = ""
    m.allowanceShown = invalid
    m.buttonActions = []
    m.sending = false
    m.seasons.observeField("itemSelected", "onSeason")
    m.buttons.observeField("buttonSelected", "onButton")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if not m.started
        m.started = true
        showSeasons()
    end if
    m.seasons.setFocus(true)
end sub

sub showSeasons()
    params = m.top.params
    detail = httpObject(params.detail)
    if detail = invalid then detail = {}
    summary = reqSummaryOf(m.top.requestSummary)
    now = CreateObject("roDateTime").AsSeconds()
    m.allowanceShown = reqTitleAllowance(detail, summary)
    m.remaining = m.allowanceShown.seasonsLeft
    m.rows = reqPickerRows(detail, m.remaining, now, fmtZoneSeconds())
    only = httpNumber(params.only, -1)
    if only >= 0
        for each row in m.rows
            row.ticked = row.askable and row.number = only
        end for
    end if
    parts = [httpString(params.title)]
    year = httpNumber(params.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    m.subtitle.text = parts.Join(" · ")
    m.words.text = reqAskWords(summary.access)
    words = reqAllowance(m.allowanceShown, now)
    m.allowanceText = words.text
    m.allowance.text = (words.text + " " + words.limits).Trim()
    m.seasons.content = nodesContent(m.rows)
    ' The first ticked season, else the first that can be asked for.
    first = -1
    for i = 0 to m.rows.Count() - 1
        if first < 0 and m.rows[i].ticked then first = i
    end for
    for i = 0 to m.rows.Count() - 1
        if first < 0 and m.rows[i].askable then first = i
    end for
    if first > 0 then m.seasons.jumpToItem = first
    showButton()
end sub

' OK ticks or unticks a season; one that can't be asked for says why.
sub onSeason(event as object)
    index = event.getData()
    if index < 0 or index >= m.rows.Count() then return
    row = m.rows[index]
    if not row.askable
        m.line.color = "0xA1A9B6FF"
        m.line.text = t("requests.season.why", { season: row.title, state: row.line })
        return
    end if
    row.ticked = not row.ticked
    ' A new node, which the list draws again.
    m.seasons.content.replaceChild(nodesContent([row]).getChild(0), index)
    showButton()
end sub

' Request with how many are ticked, or the line that stands in for it.
sub showButton()
    ticked = 0
    for each row in m.rows
        if row.ticked then ticked = ticked + 1
    end for
    button = reqPickerButton(ticked, m.remaining, m.allowanceText)
    cancel = t("common.actions.cancel")
    labels = [cancel]
    m.buttonActions = ["cancel"]
    if button.label <> ""
        labels = [button.label, cancel]
        m.buttonActions = ["send", "cancel"]
    end if
    if FormatJson(m.buttons.buttons) <> FormatJson(labels) then m.buttons.buttons = labels
    m.line.text = button.line
    m.line.color = "0xA1A9B6FF"
    if ticked > 0 and button.label = "" then m.line.color = "0xEB6B70FF"
end sub

sub onButton(event as object)
    index = event.getData()
    if index < 0 or index >= m.buttonActions.Count() then return
    if m.buttonActions[index] = "send"
        send()
    else
        uiNavigate({ back: true })
    end if
end sub

sub send()
    if m.sending then return
    numbers = []
    for each row in m.rows
        if row.ticked then numbers.Push(row.number)
    end for
    if numbers.Count() = 0 then return
    m.sending = true
    uiRequest({ method: "POST", path: "/api/requests", jsonText: reqAskBody("show", m.top.params.tmdbId, numbers) }, "onSent")
end sub

' Asked: the title's page, with what came of it. Refused: why, here.
sub onSent(event as object)
    m.sending = false
    answer = event.getData()
    params = m.top.params
    title = httpString(params.title)
    ' What may still be asked for moved either way.
    m.top.getScene().requestTouched = { requestId: 0, kind: "show", tmdbId: httpNumber(params.tmdbId, 0) }
    if not answer.ok
        if uiFailed(answer) then return
        m.line.text = reqAskProblem(answer, m.allowanceShown, "show", CreateObject("roDateTime").AsSeconds())
        m.line.color = "0xEB6B70FF"
        return
    end if
    uiNavigate({ back: true, message: { title: title, lines: [reqAsked(answer.status, answer.data, title)] } })
end sub

' Right from the seasons goes to Request, left back.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "right" and m.seasons.isInFocusChain()
        m.buttons.setFocus(true)
        return true
    end if
    if key = "left" and m.buttons.isInFocusChain()
        m.seasons.setFocus(true)
        return true
    end if
    return false
end function
