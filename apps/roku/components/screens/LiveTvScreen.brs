' Live TV (docs/API.md, "Live TV"), as the web app shows it: the
' categories (GET /api/live/groups) down the left, and the category's
' channels a page of 50 at a time (GET /api/live/browse) at the right, as
' the guide, its programmes fetched for those channels only (GET
' /api/live/guide?ids=) six hours at a time as it is scrolled to, two days
' at most, or as cards. A provider's lineup runs to tens of thousands of
' channels in hundreds of groups: nothing here asks for all of them. The
' tab, the category and the channel last watched are kept on the Roku for
' each server and profile; the list opens at that channel, back from
' watching it and after a restart.

sub init()
    m.tabs = m.top.findNode("tabs")
    m.rail = m.top.findNode("rail")
    m.searchRow = m.top.findNode("searchRow")
    m.categoryLabel = m.top.findNode("category")
    m.count = m.top.findNode("count")
    m.info = m.top.findNode("info")
    m.infoLogo = m.top.findNode("infoLogo")
    m.infoChannel = m.top.findNode("infoChannel")
    m.infoTitle = m.top.findNode("infoTitle")
    m.infoMeta = m.top.findNode("infoMeta")
    m.infoText = m.top.findNode("infoText")
    m.guide = m.top.findNode("guide")
    m.cards = m.top.findNode("cards")
    m.message = m.top.findNode("message")
    m.retry = m.top.findNode("retry")
    m.clock = m.top.findNode("clock")
    m.hint = m.top.findNode("hint")
    m.top.findNode("title").text = t("livetv.title")
    m.message.text = t("common.states.loading")
    m.guide.loadingDataText = t("livetv.page.loadingGuide")
    m.guide.channelNoDataText = t("livetv.guide.noInformation")
    m.started = false
    m.twelve = fmtTwelveHour()
    m.view = { tab: "guide", category: "all" }
    m.viewKey = ""
    ' The groups ({ total, favorites, recent } and [{ name, region, label,
    ' count }]), the rail's entries and the one in focus, the region open in
    ' it, and the words searched for.
    m.summary = invalid
    m.groups = []
    m.railEntries = []
    m.railFocused = 0
    m.open = ""
    m.query = ""
    ' The category shown before a search, which leaving it shows again.
    m.beforeSearch = "all"
    ' The guide: where it starts, how far it is fetched, the page shown
    ' ({ total, offset, channels }), the focus as it was last seen (to tell
    ' a page's end gone round), and which asking an answer must be for.
    m.guideStart = 0
    m.loadedTo = 0
    m.page = invalid
    m.lastRow = -1
    m.jumped = false
    m.asked = 0
    m.loading = false
    ' Where the last Back brought the guide back to now (backToNow).
    m.backedTo = ""
    ' The cards: the pages in the grid, from offset first, their channels,
    ' and the category's size.
    m.cardsFrom = 0
    m.cardChannels = []
    m.cardsTotal = -1
    m.cardsLoading = false
    ' The card page asked for last, whose answer alone counts.
    m.cardsRequest = invalid
    ' The channels take the focus as they come (toChannels).
    m.toContent = false
    ' What Try again asks for again: "groups", or the category's channels
    ' around a channel; and when a page or a part of the guide failed with
    ' the list there (m.span's ms), asked for again as it moves.
    m.retryWhat = ""
    m.retryAround = 0
    m.failedAt = -1
    m.span = CreateObject("roTimespan")
    ' When what's on in the cards was last asked for again (m.span's ms).
    m.refreshedAt = -1
    m.retry.buttons = [t("common.actions.tryAgain")]
    m.retry.observeField("buttonSelected", "onRetry")
    m.tabs.chips = [{ label: t("livetv.tabs.guide"), width: 208 }, { label: t("livetv.tabs.channels"), width: 208 }]
    m.tabs.observeField("chosen", "onTabChosen")
    m.rail.observeField("itemSelected", "onRailSelected")
    m.rail.observeField("itemFocused", "onRailFocused")
    m.searchRow.observeField("itemSelected", "askSearch")
    m.guide.observeField("programFocusedDetails", "onProgramFocused")
    m.guide.observeField("programSelected", "onProgramSelected")
    m.guide.observeField("leftEdgeTargetTime", "onLeftEdge")
    m.cards.observeField("itemSelected", "onCardSelected")
    m.cards.observeField("itemFocused", "onCardFocused")
    m.clock.observeField("fire", "onClock")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    m.clock.control = "stop"
    if m.top.active then m.clock.control = "start"
    if not m.top.active
        m.lastFocus = m.rail
        if m.tabs.isInFocusChain() then m.lastFocus = m.tabs
        if m.searchRow.isInFocusChain() then m.lastFocus = m.searchRow
        if m.guide.isInFocusChain() then m.lastFocus = m.guide
        if m.cards.isInFocusChain() then m.lastFocus = m.cards
        if m.retry.isInFocusChain() then m.lastFocus = m.retry
        return
    end if
    scene = m.top.getScene()
    if not m.started
        m.started = true
        session = m.global.session
        m.viewKey = liveViewKey(session.server, session.profileId)
        m.view = liveLoadView(viewSection(), m.viewKey)
        ' The channel last watched, kept from before the app started: the
        ' list opens at it, as it does back from the player.
        if scene.liveChannel = 0 then scene.liveChannel = m.view.channel
        m.tabs.current = 0
        if m.view.tab = "channels" then m.tabs.current = 1
        m.tabs.focusIndex = m.tabs.current
        m.rail.setFocus(true)
        showMessage(t("common.states.loading"))
        loadGroups(true)
        return
    end if
    if m.lastFocus <> invalid and m.lastFocus.visible
        m.lastFocus.setFocus(true)
    else
        m.rail.setFocus(true)
    end if
    ' Back from watching: the counts moved on (Recently watched), and the
    ' list opens at the channel watched.
    loadGroups(false)
    watched = scene.liveChannel
    if watched > 0 then returnTo(watched)
end sub

function viewSection() as object
    return CreateObject("roRegistrySection", "liveViews")
end function

function nowSeconds() as integer
    return CreateObject("roDateTime").AsSeconds()
end function

' --- Categories ---

' GET /api/live/groups: the rail; the first time, the category's channels
' after.
sub loadGroups(first as boolean)
    uiRequest({ method: "GET", path: "/api/live/groups", first: first }, "onGroups")
end sub

sub onGroups(event as object)
    answer = event.getData()
    first = event.getRoSGNode().spec.first = true
    if not answer.ok
        if not first or uiFailed(answer) then return
        if answer.status = 403
            showMessage(t("livetv.page.notOpen") + Chr(10) + t("livetv.page.notOpenHint"))
        else
            showFailed(answer.message, "groups", 0)
        end if
        return
    end if
    m.summary = liveSummary(answer.data)
    m.groups = liveGroups(answer.data)
    if m.summary.total = 0
        showMessage(t("livetv.page.noChannels") + Chr(10) + t("livetv.page.noChannelsHint"))
        return
    end if
    category = liveCategoryValid(m.view.category, m.summary, m.groups)
    region = liveRegionOf(category, m.groups)
    if region <> "" then m.open = region
    changed = category <> m.view.category
    m.view.category = category
    buildRail()
    if first or changed then showCategory(m.top.getScene().liveChannel)
end sub

' The rail's entries, the focus kept on the same one.
sub buildRail()
    focusedKey = invalid
    if m.rail.content <> invalid and m.rail.itemFocused >= 0 and m.rail.itemFocused < m.railEntries.Count() then focusedKey = m.railEntries[m.rail.itemFocused].key
    m.railEntries = liveRail(m.summary, m.groups, m.open, m.query)
    at = 0
    for i = 0 to m.railEntries.Count() - 1
        entry = m.railEntries[i]
        if entry.kind <> "heading"
            if focusedKey = invalid and entry.key = m.view.category then at = i
            if focusedKey <> invalid and entry.key = focusedKey then at = i
        end if
    end for
    m.railFocused = at
    m.rail.content = nodesContent(liveRailItems(m.railEntries, m.open, m.view.category))
    m.rail.jumpToItem = at
    m.searchRow.content = nodesContent([{ title: liveSearchLabel(m.query), countText: "", mark: "", inner: false, heading: false, current: searching() }])
end sub

' Rewind and fast forward in the rail: a screen of categories back or on.
sub pageRail(by as integer)
    kinds = []
    for each entry in m.railEntries
        kinds.Push(entry.kind)
    end for
    target = liveRailPage(kinds, m.rail.itemFocused, 13, by)
    m.railFocused = target
    m.rail.jumpToItem = target
end sub

' Rewind and fast forward in the guide: two hours earlier or later, within
' what it spans; Play: back to now.
sub pageGuide(key as string)
    if m.guide.content = invalid then return
    span = Int(m.guide.duration)
    left = guideLeft()
    if key = "play"
        left = nowSeconds()
    else if key = "rewind"
        left = left - span
    else
        left = left + span
    end if
    last = m.guideStart + liveGuideSpan() - span
    if left > last then left = last
    if left < m.guideStart then left = m.guideStart
    m.guide.leftEdgeTargetTime = left
end sub

' The time at the guide's left edge.
function guideLeft() as integer
    left = m.guide.leftEdgeTargetTime
    if left <= 0 then left = m.guideStart
    return left
end function

' Back in the guide away from now (another time shown, or a programme not
' on now in focus) brings it back to now, before it goes on to the
' categories; not twice from where it stands, when the guide found nothing
' nearer now.
function backToNow() as boolean
    focused = focusedGuide()
    if focused = invalid then return false
    from = 0
    upto = 0
    if focused.program <> invalid
        p = programTimes(focused.program)
        from = p.start
        upto = p.ends
    end if
    if not liveGuideAway(guideLeft(), Int(m.guide.duration), from, upto, nowSeconds()) then return false
    if guidePlace() = m.backedTo then return false
    pageGuide("play")
    m.backedTo = guidePlace()
    return true
end function

' Where the guide stands: the row and programme in focus, and the time at
' its left edge.
function guidePlace() as string
    details = m.guide.programFocusedDetails
    row = 0
    at = 0
    if details <> invalid
        row = Int(httpNumber(details.focusChannelIndex, 0))
        at = Int(httpNumber(details.focusIndex, 0))
    end if
    return row.ToStr() + "|" + at.ToStr() + "|" + guideLeft().ToStr()
end function

' A heading ("Regions", "More groups") is passed over, the way the focus
' was going.
sub onRailFocused(event as object)
    at = event.getData()
    target = liveRailPast(m.railEntries, at, m.railFocused)
    m.railFocused = target
    if target <> at then m.rail.jumpToItem = target
end sub

' OK on Search asks what to find; on a category, it shows and the focus
' goes to its channels; on a region, its groups open (and another's
' close) or close.
sub onRailSelected(event as object)
    at = event.getData()
    if at < 0 or at >= m.railEntries.Count() then return
    entry = m.railEntries[at]
    if entry.kind = "heading" then return
    if entry.kind = "search"
        askSearch()
        return
    end if
    if entry.kind = "region"
        if m.open = entry.key
            m.open = ""
        else
            m.open = entry.key
        end if
        buildRail()
        return
    end if
    choose(entry.key)
    toChannels()
end sub

' Shows a category, kept on the Roku (a search isn't).
sub choose(category as string)
    if category = m.view.category then return
    m.view.category = category
    if Left(category, 7) <> "search:" then liveSaveView(viewSection(), m.viewKey, m.view)
    buildRail()
    showCategory(0)
end sub

' --- Search ---

' Roku's keyboard: every channel whose name, number or programme on now
' has the words in it, and the groups whose names do.
sub askSearch()
    dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
    dialog.title = t("livetv.search.title")
    dialog.message = [t("livetv.search.prompt")]
    dialog.text = m.query
    ' What each button does, in their order: Clear only with words.
    buttons = [t("livetv.search.search")]
    m.searchActions = ["search"]
    if m.query <> ""
        buttons.Push(t("livetv.search.clear"))
        m.searchActions.Push("clear")
    end if
    buttons.Push(t("common.actions.cancel"))
    m.searchActions.Push("cancel")
    dialog.buttons = buttons
    dialog.observeField("buttonSelected", "onSearchAnswered")
    m.top.getScene().dialog = dialog
end sub

sub onSearchAnswered(event as object)
    dialog = m.top.getScene().dialog
    text = ""
    if dialog <> invalid then text = dialog.text.Trim()
    uiCloseDialog()
    chosen = event.getData()
    actions = m.searchActions
    if actions = invalid or chosen < 0 or chosen >= actions.Count() then return
    action = actions[chosen]
    if action = "cancel" then return
    if action = "clear" or text = ""
        if searching()
            leaveSearch()
        else
            m.query = ""
            buildRail()
        end if
        return
    end if
    m.beforeSearch = liveBeforeSearch(m.view.category, m.beforeSearch)
    m.query = httpSentence(text)
    m.view.category = ""
    choose("search:" + m.query)
    toChannels()
end sub

function searching() as boolean
    return Left(m.view.category, 7) = "search:"
end function

' Out of a search, as Back or Clear leaves it: the category before it
' again, the focus where it is.
sub leaveSearch()
    inContent = m.guide.isInFocusChain() or m.cards.isInFocusChain()
    m.query = ""
    m.view.category = ""
    choose(m.beforeSearch)
    if inContent then toChannels()
end sub

' The category's channels from the start, or from the page around channel
' around (one watched).
sub showCategory(around as integer)
    m.categoryLabel.text = liveCategoryName(m.view.category, m.groups)
    m.failedAt = -1
    m.page = invalid
    m.cardChannels = []
    m.cardsTotal = -1
    m.guide.content = invalid
    m.cards.content = invalid
    m.count.text = ""
    if m.view.tab = "guide"
        loadGuidePage(0, around, "first")
    else
        ' In place of a page still loading for the category before.
        loadCards(0, around, false, true)
    end if
end sub

' The list again at channel id, as it shows meanwhile (and the focus with
' it): back from watching it.
sub returnTo(id as integer)
    if m.view.tab = "guide"
        loadGuidePage(0, id, "first")
    else
        loadCards(0, id, false, true)
    end if
end sub

' --- Tabs ---

sub onTabChosen(event as object)
    wanted = "guide"
    if event.getData() = 1 then wanted = "channels"
    if wanted = m.view.tab then return
    m.view.tab = wanted
    liveSaveView(viewSection(), m.viewKey, m.view)
    m.tabs.current = event.getData()
    focused = channelInFocus()
    around = 0
    if focused <> invalid then around = focused.id
    showCategory(around)
end sub

sub showMessage(text as string)
    focused = m.guide.isInFocusChain() or m.cards.isInFocusChain() or m.retry.isInFocusChain()
    m.guide.visible = false
    m.info.visible = false
    m.cards.visible = false
    m.retry.visible = false
    m.hint.text = ""
    m.message.text = text
    m.message.visible = true
    if focused then m.rail.setFocus(true)
end sub

' What failed with nothing to show, with Try again under it (what: the
' groups, or the category's channels around a channel).
sub showFailed(text as string, what as string, around as integer)
    showMessage(text)
    m.retryWhat = what
    m.retryAround = around
    m.retry.visible = true
    if m.toContent then focusContent()
end sub

' Try again: what failed, asked for again, the focus going to the
' channels as they come when Try again had it.
sub onRetry()
    m.toContent = m.retry.isInFocusChain()
    if m.retryWhat = "groups"
        showMessage(t("common.states.loading"))
        loadGroups(true)
    else
        showCategory(m.retryAround)
    end if
end sub

' The channels show, or what failed: Try again gives the focus to them.
sub hideRetry()
    if m.retry.isInFocusChain() then m.toContent = true
    m.retry.visible = false
end sub

sub focusContent()
    if not m.top.isInFocusChain() then return
    if m.view.tab = "guide" and m.guide.visible
        m.toContent = false
        m.guide.setFocus(true)
    else if m.view.tab = "channels" and m.cards.visible
        m.toContent = false
        m.cards.setFocus(true)
    else if m.retry.visible
        m.toContent = false
        m.retry.setFocus(true)
    end if
end sub

' Whether the right of the screen has something to take the focus.
function contentShown() as boolean
    return m.guide.visible or m.cards.visible or m.retry.visible
end function

' The focus goes to the channels: at once when they show, else as they
' come (a category chosen shows "Loading…" first).
sub toChannels()
    m.toContent = true
    focusContent()
end sub

' The channel in focus in the guide or the cards, or invalid.
function channelInFocus() as dynamic
    if m.view.tab = "guide"
        focused = focusedGuide()
        if focused = invalid then return invalid
        return focused.channel
    end if
    at = m.cards.itemFocused
    if at < 0 or at >= m.cardChannels.Count() then return invalid
    return m.cardChannels[at]
end function

' --- The guide ---

' A page of the category's channels with their guide: from offset, or
' around a channel; focus "first" or "last" of it (or the channel around).
sub loadGuidePage(offset as integer, around as integer, focus as string)
    start = liveGuideStart(nowSeconds())
    if start <> m.guideStart
        m.guideStart = start
        m.loadedTo = start + liveGuideChunk()
    end if
    m.asked = m.asked + 1
    m.loading = true
    if m.guide.content = invalid then showMessage(t("livetv.page.loadingGuide"))
    uiRequest({ op: "livePage", kind: "guide", category: m.view.category, offset: offset, limit: livePageSize(), around: around, start: m.guideStart, upto: m.loadedTo, asked: m.asked, focus: focus, focusId: around }, "onGuide")
end sub

' The page in the guide: the focus on the row asked for, the time shown
' kept.
sub onGuide(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    if spec.asked <> m.asked then return
    m.loading = false
    answer = event.getData()
    if not answer.ok
        if m.guide.content <> invalid
            ' The guide stays as it was; what failed is asked for again as
            ' it moves.
            m.failedAt = m.span.TotalMilliseconds()
        else if not uiFailed(answer)
            showFailed(answer.message, "channels", spec.focusId)
        end if
        return
    end if
    content = request.content
    if content = invalid then return
    m.failedAt = -1
    data = answer.data
    if data.upto > m.loadedTo then m.loadedTo = data.upto
    ' From now at the left edge: the focus is on what's on now (at the
    ' half hour, the programme then could have ended).
    left = nowSeconds()
    keepRow = -1
    if m.guide.content <> invalid
        if m.guide.leftEdgeTargetTime > 0 then left = m.guide.leftEdgeTargetTime
        details = m.guide.programFocusedDetails
        if details <> invalid then keepRow = Int(httpNumber(details.focusChannelIndex, 0))
    end if
    if data.channels.Count() = 0
        m.page = invalid
        m.guide.content = invalid
        m.toContent = false
        showMessage(emptyText())
        return
    end if
    m.page = data
    m.backedTo = ""
    row = 0
    if spec.focus = "last" then row = data.channels.Count() - 1
    if spec.focus = "keep" and keepRow >= 0 and keepRow < data.channels.Count() then row = keepRow
    if spec.focusId > 0
        at = liveIndexOf(data.channels, spec.focusId)
        if at >= 0 then row = at
    end if
    m.jumped = true
    m.lastRow = row
    m.guide.contentStartTime = m.guideStart
    ' Its logos too.
    m.guide.setHttpAgent(m.top.getScene().getHttpAgent())
    m.guide.content = content
    m.guide.jumpToChannel = row
    m.guide.leftEdgeTargetTime = left
    m.message.visible = false
    hideRetry()
    m.guide.visible = true
    m.info.visible = true
    showCount()
    m.hint.text = t("livetv.page.guideHint")
    showProgram()
    if m.toContent then focusContent()
end sub

function emptyText() as string
    if searching() then return t("livetv.page.noMatch", { search: m.query })
    if m.view.category = "favorites" then return t("livetv.page.noFavourites")
    if m.view.category = "recent" then return t("livetv.page.nothingWatched")
    return t("livetv.page.empty")
end function

' "2,345 channels   ·   Channels 51–100 of 2,345".
sub showCount()
    total = -1
    shown = 0
    offset = 0
    if m.view.tab = "guide" and m.page <> invalid
        total = m.page.total
        shown = m.page.channels.Count()
        offset = m.page.offset
    else if m.view.tab = "channels"
        total = m.cardsTotal
    end if
    if total < 0
        m.count.text = ""
        return
    end if
    text = t("livetv.page.channelCount", { count: total })
    range = liveRange(offset, shown, total)
    if range <> "" then text = text + "   ·   " + range
    m.count.text = text
end sub

' Scrolled near what is fetched: the next six hours, for the channels
' shown, counted as fetched once they come.
sub onLeftEdge()
    if m.loading or m.page = invalid or m.guide.content = invalid then return
    if not liveMayAskAgain(m.failedAt, m.span.TotalMilliseconds()) then return
    missing = liveGuideMissing(m.guideStart, m.loadedTo, m.guide.leftEdgeTargetTime, Int(m.guide.duration))
    if missing = invalid then return
    m.asked = m.asked + 1
    m.loading = true
    uiRequest({ op: "liveGuide", channels: m.page.channels, start: m.guideStart, upto: missing.to, asked: m.asked, focus: "keep", focusId: 0 }, "onGuide")
end sub

' The focus moved: the programme above the guide, and past the page's last
' row (or before its first) the next page (or the one before), round the
' ends of the category, as the guide itself goes round.
sub onProgramFocused()
    details = m.guide.programFocusedDetails
    if details <> invalid and m.page <> invalid
        row = Int(httpNumber(details.focusChannelIndex, 0))
        count = m.page.channels.Count()
        wasRow = m.lastRow
        m.lastRow = row
        if m.jumped
            m.jumped = false
        else if m.page.total > count and count > 2 and not m.loading and liveMayAskAgain(m.failedAt, m.span.TotalMilliseconds())
            size = livePageSize()
            pages = Int((m.page.total + size - 1) / size)
            pageNo = Int(m.page.offset / size)
            if wasRow = count - 1 and row = 0
                loadGuidePage(((pageNo + 1) mod pages) * size, 0, "first")
                return
            end if
            if wasRow = 0 and row = count - 1
                loadGuidePage(((pageNo - 1 + pages) mod pages) * size, 0, "last")
                return
            end if
        end if
    end if
    showProgram()
end sub

' The channel and programme in focus: { channel, program } (program
' invalid for a channel without one there).
function focusedGuide() as dynamic
    content = m.guide.content
    if content = invalid or m.page = invalid then return invalid
    details = m.guide.programFocusedDetails
    channelAt = 0
    programAt = 0
    if details <> invalid
        channelAt = Int(httpNumber(details.focusChannelIndex, 0))
        programAt = Int(httpNumber(details.focusIndex, 0))
    end if
    if channelAt < 0 or channelAt >= m.page.channels.Count() then return invalid
    row = content.getChild(channelAt)
    program = invalid
    if row <> invalid then program = row.getChild(programAt)
    return { channel: m.page.channels[channelAt], program: program }
end function

' Above the guide: the channel, the programme's title, when it is on
' (and how long it has left, on now), its episode and what it is.
sub showProgram()
    focused = focusedGuide()
    if focused = invalid then return
    channel = focused.channel
    program = focused.program
    m.infoChannel.text = liveTitle(channel)
    ' The scene's agent carries the token; it changes with each refresh.
    m.infoLogo.setHttpAgent(m.top.getScene().getHttpAgent())
    m.infoLogo.uri = channel.logo
    if program = invalid or program.id = "gap"
        m.infoTitle.text = channel.name
        if channel.hasGuide
            m.infoMeta.text = t("livetv.guide.noInformation")
        else
            m.infoMeta.text = t("livetv.guide.noGuide")
        end if
        m.infoText.text = ""
        return
    end if
    now = nowSeconds()
    p = programTimes(program)
    meta = []
    if p.start <= now and p.ends > now
        meta.Push(t("livetv.guide.onNow"))
        meta.Push(liveSpan(p, m.twelve))
        meta.Push(liveTimeLeft(p, now))
    else
        meta.Push(daySpan(p, now))
    end if
    if program.shortDescriptionLine1 <> "" then meta.Push(program.shortDescriptionLine1)
    if program.shortDescriptionLine2 <> "" then meta.Push(program.shortDescriptionLine2)
    m.infoTitle.text = program.title
    m.infoMeta.text = meta.Join("   ·   ")
    m.infoText.text = program.description
end sub

function programTimes(program as object) as object
    start = Int(program.playStart)
    return { start: start, ends: start + Int(program.playDuration) }
end function

' "Tomorrow 8:00 PM – 9:00 PM", the day left out for today.
function daySpan(p as object, now as integer) as string
    span = liveSpan(p, m.twelve)
    day = dayLabel(p.start, now)
    if day = "" then return span
    return t("livetv.time.dayAndTime", { day: day, time: span })
end function

' "Tomorrow", "Yesterday", a weekday, or "" for today, in the Roku's
' time zone.
function dayLabel(seconds as integer, now as integer) as string
    at = localDay(seconds)
    today = localDay(now)
    if at = today then return ""
    if at = today + 1 then return t("livetv.time.tomorrow")
    if at = today - 1 then return t("livetv.time.yesterday")
    when = CreateObject("roDateTime")
    when.FromSeconds(seconds)
    when.ToLocalTime()
    ' GetWeekday() names it in English: its number (0 for Sunday) instead.
    days = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"]
    return t("livetv.time.days." + days[when.GetDayOfWeek()])
end function

function localDay(seconds as integer) as integer
    when = CreateObject("roDateTime")
    when.FromSeconds(seconds)
    when.ToLocalTime()
    return Int(when.AsSeconds() / 86400)
end function

' OK on a programme: one on now (or time without a guide) watches the
' channel; another says what it is, with Watch.
sub onProgramSelected()
    focused = focusedGuide()
    if focused = invalid then return
    program = focused.program
    now = nowSeconds()
    if program <> invalid and program.id <> "gap"
        p = programTimes(program)
        if p.start > now or p.ends <= now
            channelOptions(focused.channel, programOf(program))
            return
        end if
    end if
    watch(focused.channel)
end sub

' --- Cards ---

' A page of cards: from offset (at the end, or before with before), or
' the page around a channel, the grid then starting there.
sub loadCards(offset as integer, around as integer, before as boolean, replace as boolean)
    if m.cardsLoading and not replace then return
    m.cardsLoading = true
    if m.cardChannels.Count() = 0 then showMessage(t("common.states.loading"))
    m.cardsRequest = uiRequest({ op: "livePage", kind: "cards", category: m.view.category, offset: offset, limit: livePageSize(), around: around, before: before, replace: replace, focusId: around }, "onCards")
end sub

sub onCards(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    ' Only the page asked for last: one asked for before it (another tab or
    ' category, or one a new category replaced) changes nothing.
    if m.cardsRequest = invalid or not request.isSameNode(m.cardsRequest) then return
    m.cardsRequest = invalid
    m.cardsLoading = false
    answer = event.getData()
    if not answer.ok
        if m.cardChannels.Count() > 0
            ' The cards stay; the page is asked for again as they move.
            m.failedAt = m.span.TotalMilliseconds()
        else if not uiFailed(answer)
            showFailed(answer.message, "channels", spec.focusId)
        end if
        return
    end if
    if m.view.tab <> "channels" or spec.category <> m.view.category then return
    m.failedAt = -1
    data = answer.data
    m.cardsTotal = data.total
    fresh = request.content
    nodes = []
    if fresh <> invalid then nodes = fresh.getChildren(-1, 0)
    if m.cards.content = invalid or m.cardChannels.Count() = 0 or spec.replace = true
        m.cardsFrom = data.offset
        m.cardChannels = data.channels
        content = CreateObject("roSGNode", "ContentNode")
        content.appendChildren(nodes)
        m.cards.content = content
        at = 0
        if spec.focusId > 0 then at = liveIndexOf(m.cardChannels, spec.focusId)
        if at < 0 then at = 0
        m.cards.jumpToItem = at
    else if spec.before = true
        ' A page before the first in the grid: the focus stays on its card.
        focusAt = m.cards.itemFocused + nodes.Count()
        m.cardsFrom = data.offset
        all = []
        all.Append(data.channels)
        all.Append(m.cardChannels)
        m.cardChannels = all
        m.cards.content.insertChildren(nodes, 0)
        m.cards.jumpToItem = focusAt
    else
        m.cardChannels.Append(data.channels)
        m.cards.content.appendChildren(nodes)
    end if
    if m.cardChannels.Count() = 0
        m.toContent = false
        showMessage(emptyText())
        return
    end if
    m.message.visible = false
    hideRetry()
    m.cards.visible = true
    m.guide.visible = false
    m.info.visible = false
    m.hint.text = t("livetv.page.cardsHint")
    showCount()
    if m.toContent then focusContent()
end sub

' Two rows from the ends of what the grid holds, the next page (or the
' one before) comes.
sub onCardFocused(event as object)
    if not liveMayAskAgain(m.failedAt, m.span.TotalMilliseconds()) then return
    at = event.getData()
    loadedTo = m.cardsFrom + m.cardChannels.Count()
    if at >= m.cardChannels.Count() - 8 and loadedTo < m.cardsTotal
        loadCards(loadedTo, 0, false, false)
    else if at < 8 and m.cardsFrom > 0
        offset = m.cardsFrom - livePageSize()
        if offset < 0 then offset = 0
        loadCards(offset, 0, true, false)
    end if
end sub

' The cards around the one in focus (those in view, and a few rows either
' side): [from, upto) of m.cardChannels.
function cardsInView() as object
    from = m.cards.itemFocused - 24
    if from < 0 then from = 0
    upto = from + livePageSize()
    if upto > m.cardChannels.Count() then upto = m.cardChannels.Count()
    return { from: from, upto: upto }
end function

' Each 15 s, what's on moves on: above the guide, the programme in focus
' (its time left); on the cards in view, how far along, and once a
' programme listed as on now has ended, those cards asked for again (the
' guide's programmes are fetched hours ahead).
sub onClock()
    if m.guide.visible then showProgram()
    if not m.cards.visible or m.cards.content = invalid then return
    now = nowSeconds()
    shown = cardsInView()
    for i = shown.from to shown.upto - 1
        item = m.cards.content.getChild(i)
        if item <> invalid
            card = liveCard(m.cardChannels[i], now, m.twelve)
            if item.line <> card.line then item.line = card.line
            if item.nextLine <> card.nextLine then item.nextLine = card.nextLine
            if Abs(item.progress - card.progress) > 0.001 then item.progress = card.progress
        end if
    end for
    ms = m.span.TotalMilliseconds()
    if not liveNowEnded(m.cardChannels, shown.from, shown.upto, now) or not liveMayRefresh(m.refreshedAt, ms) then return
    m.refreshedAt = ms
    uiRequest({ op: "livePage", kind: "cards", category: m.view.category, offset: m.cardsFrom + shown.from, limit: shown.upto - shown.from, around: 0, cardsFrom: m.cardsFrom }, "onCardsRefreshed")
end sub

' The cards asked for again: each, where it still is, with what's on now.
sub onCardsRefreshed(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    answer = event.getData()
    if not answer.ok or request.content = invalid or m.cards.content = invalid then return
    if m.view.tab <> "channels" or spec.category <> m.view.category or spec.cardsFrom <> m.cardsFrom then return
    data = answer.data
    for i = 0 to data.channels.Count() - 1
        at = data.offset - m.cardsFrom + i
        if at >= 0 and at < m.cardChannels.Count() and m.cardChannels[at].id = data.channels[i].id
            m.cardChannels[at] = data.channels[i]
            fresh = request.content.getChild(i)
            item = m.cards.content.getChild(at)
            if fresh <> invalid and item <> invalid then item.setFields({ line: fresh.line, progress: fresh.progress, nextLine: fresh.nextLine, favorite: fresh.favorite })
        end if
    end for
end sub

sub onCardSelected(event as object)
    at = event.getData()
    if at >= 0 and at < m.cardChannels.Count() then watch(m.cardChannels[at])
end sub

' --- Watching and favourites ---

' The player, with the category: up and down stay in it.
sub watch(channel as object)
    uiNavigate({ to: "livePlayer", params: { channel: channel, category: m.view.category, categoryName: liveCategoryName(m.view.category, m.groups) } })
end sub

' A programme of the guide (its ContentNode) as Live.brs keeps one ({
' start, ends, title, description }), or invalid for time without a guide.
function programOf(program as dynamic) as dynamic
    if program = invalid or program.id = "gap" then return invalid
    p = programTimes(program)
    p.title = program.title
    p.description = program.description
    return p
end function

' The programme the Options key shows the details of: the one in focus in
' the guide, what's on now on a card.
function programInFocus(channel as object) as dynamic
    if m.view.tab = "channels" then return liveOnNow(channel, nowSeconds())
    focused = focusedGuide()
    if focused = invalid then return invalid
    return programOf(focused.program)
end function

' The Options key on a programme or a card (or OK on a programme not on
' now): the channel's name, the programme's details (p, as Live.brs keeps
' one, or invalid), Watch, and Add to or Remove from favourites.
sub channelOptions(channel as object, p as dynamic)
    m.optionsChannel = channel
    ' onOptionChosen goes by their order.
    buttons = [t("livetv.options.watch", { name: channel.name })]
    if channel.favorite
        buttons.Push(t("livetv.favourites.remove"))
    else
        buttons.Push(t("livetv.favourites.add"))
    end if
    buttons.Push(t("common.actions.cancel"))
    dialog = CreateObject("roSGNode", "StandardMessageDialog")
    dialog.title = liveTitle(channel)
    if p <> invalid
        lines = [p.title, daySpan(p, nowSeconds())]
        if p.description <> "" then lines.Push(p.description)
        dialog.message = lines
    end if
    dialog.buttons = buttons
    dialog.observeField("buttonSelected", "onOptionChosen")
    m.top.getScene().dialog = dialog
end sub

sub onOptionChosen(event as object)
    chosen = event.getData()
    channel = m.optionsChannel
    m.optionsChannel = invalid
    uiCloseDialog()
    if channel = invalid then return
    if chosen = 0
        watch(channel)
    else if chosen = 1
        method = "DELETE"
        if not channel.favorite then method = "PUT"
        uiRequest({ method: method, path: "/api/live/channels/" + urlValue(channel.id) + "/favorite", channelId: channel.id }, "onFavorite")
    end if
end sub

' PUT or DELETE …/favorite done: the counts, and the list again where it
' was (a favourite's star, or the Favourites list itself).
sub onFavorite(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then uiMessage(t("livetv.favourites.failed"), [answer.message])
        return
    end if
    loadGroups(false)
    id = event.getRoSGNode().spec.channelId
    if m.view.tab = "guide" and m.page <> invalid
        loadGuidePage(m.page.offset, id, "keep")
    else
        loadCards(0, id, false, true)
    end if
end sub

' Back goes up a step at a time: the guide back to now, a search back to
' the category before, the channels to the categories, and on the rail it
' leaves. Left from the channels goes to the categories too, Right from
' them to the channels; up from the rail's top, the tabs. The Options key
' on a programme or a card: the channel's options.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "back" and m.guide.isInFocusChain() and backToNow() then return true
    ' Then a search back to the category before.
    if key = "back" and searching()
        leaveSearch()
        return true
    end if
    content = m.guide.isInFocusChain() or m.cards.isInFocusChain() or m.retry.isInFocusChain()
    if content and (key = "back" or key = "left")
        m.rail.setFocus(true)
        return true
    end if
    if key = "right" and (m.rail.isInFocusChain() or m.tabs.isInFocusChain())
        if contentShown()
            focusContent()
            return true
        end if
    end if
    ' Replay opens Search from anywhere in Live TV.
    if key = "replay"
        askSearch()
        return true
    end if
    if (key = "rewind" or key = "fastforward") and m.rail.isInFocusChain()
        by = 1
        if key = "rewind" then by = -1
        pageRail(by)
        return true
    end if
    if (key = "rewind" or key = "fastforward" or key = "play") and m.guide.isInFocusChain()
        pageGuide(key)
        return true
    end if
    if key = "right" and m.searchRow.isInFocusChain() and contentShown()
        focusContent()
        return true
    end if
    ' The rail's top, Search pinned above it, the tabs above that.
    if key = "up" and m.rail.isInFocusChain()
        m.searchRow.setFocus(true)
        return true
    end if
    if key = "up" and m.searchRow.isInFocusChain()
        m.tabs.focusIndex = m.tabs.current
        m.tabs.setFocus(true)
        return true
    end if
    if key = "down" and m.tabs.isInFocusChain()
        m.searchRow.setFocus(true)
        return true
    end if
    if key = "down" and m.searchRow.isInFocusChain()
        m.rail.setFocus(true)
        return true
    end if
    if key = "up" and m.cards.isInFocusChain()
        m.tabs.setFocus(true)
        return true
    end if
    if key = "options" and content
        focused = channelInFocus()
        if focused <> invalid then channelOptions(focused, programInFocus(focused))
        return true
    end if
    return false
end function
