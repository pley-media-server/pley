' The live player (docs/API.md, "Live TV"). Tuning is POST
' /api/live/channels/{id}/watch with this Roku's client_id (the API task's),
' which ends what it watched before, so changing channel holds one
' connection; then its live HLS playlist, asked once by the API task
' (the server answers it once the channel's first segments are made, and
' says plainly why when it can't), then given to the Video node as live
' content, from its live edge. Live is live: the Video node's own controls
' are off, and nothing pauses, rewinds or skips. Up and down
' change channel within the category the player was opened from (or the
' one a channel was last chosen in from the list, which can list another
' category or a search), a page of it at a time (GET
' /api/live/browse?around=), round its ends. Leaving
' sends DELETE /api/live/sessions/{session}, so the connection goes at
' once, and Live TV opens back at the channel watched.

sub init()
    m.video = m.top.findNode("video")
    m.waiting = m.top.findNode("waiting")
    m.card = m.top.findNode("card")
    m.cardInitials = m.top.findNode("cardInitials")
    m.cardLogo = m.top.findNode("cardLogo")
    m.tuning = m.top.findNode("tuning")
    m.cardTitle = m.top.findNode("cardTitle")
    m.cardLine = m.top.findNode("cardLine")
    m.banner = m.top.findNode("banner")
    m.bannerInitials = m.top.findNode("bannerInitials")
    m.bannerLogo = m.top.findNode("bannerLogo")
    m.bannerChannel = m.top.findNode("bannerChannel")
    m.bannerTitle = m.top.findNode("bannerTitle")
    m.bannerMeta = m.top.findNode("bannerMeta")
    m.bannerTrack = m.top.findNode("bannerTrack")
    m.bannerFill = m.top.findNode("bannerFill")
    m.bannerNext = m.top.findNode("bannerNext")
    m.bannerHint = m.top.findNode("bannerHint")
    m.star = m.top.findNode("star")
    m.list = m.top.findNode("list")
    m.listTitle = m.top.findNode("listTitle")
    m.listRange = m.top.findNode("listRange")
    m.channelList = m.top.findNode("channelList")
    m.categoryList = m.top.findNode("categoryList")
    m.listHint = m.top.findNode("listHint")
    m.listHint.text = listHintText()
    m.notice = m.top.findNode("notice")
    m.noticeTitle = m.top.findNode("noticeTitle")
    m.noticeText = m.top.findNode("noticeText")
    m.noticeButtons = m.top.findNode("noticeButtons")
    m.toast = m.top.findNode("toast")
    m.toastText = m.top.findNode("toastText")
    m.bannerTimer = m.top.findNode("bannerTimer")
    m.zapTimer = m.top.findNode("zapTimer")
    m.clock = m.top.findNode("clock")
    m.pictureTimer = m.top.findNode("pictureTimer")
    m.toastTimer = m.top.findNode("toastTimer")
    m.retryTimer = m.top.findNode("retryTimer")
    m.stallTimer = m.top.findNode("stallTimer")
    m.stallTimer.observeField("fire", "onStall")
    ' Waiting for data since the timer started (the spinner shows after it).
    m.stalling = false
    for each spinner in [m.waiting, m.tuning]
        spinner.poster.uri = "pkg:/images/spinner.png"
        spinner.poster.width = 64
        spinner.poster.height = 64
    end for
    m.twelve = CreateObject("roDeviceInfo").GetClockFormat() <> "24h"
    m.started = false
    ' The channel shown (Live.brs's liveChannel), and the one up and down
    ' lead to before it is tuned.
    m.channel = invalid
    m.zapTo = invalid
    ' The category up and down go through (the one the player was opened
    ' from, or the one a channel was last chosen in from the list), the
    ' page of it held ({ total, offset, channels }) and its rows, and the
    ' asking an answer must be for.
    m.category = "all"
    m.categoryName = "All channels"
    m.page = invalid
    m.pageContent = invalid
    m.pageAsked = 0
    m.asks = 0
    ' What the list shows: that category, unless another was chosen in it
    ' or a search made there, with its page, the asking it waits for, and
    ' whether it shows a search made there, with the category before it.
    m.listCategory = "all"
    m.listName = "All channels"
    m.listPage = invalid
    m.listAsked = 0
    m.listSearched = false
    m.listBefore = "all"
    ' Choosing another category in the list: the groups (GET
    ' /api/live/groups, once), the entries listed, the one in focus, the
    ' region open.
    m.summary = invalid
    m.groups = []
    m.railEntries = []
    m.railFocused = 0
    m.open = ""
    ' A page that failed ({ offset, around }) and when (m.span's ms): asked
    ' for again once 10 s have passed, as the list moves or Up or Down is
    ' pressed.
    m.failedPage = invalid
    m.failedAt = -1
    m.failedCategory = ""
    m.span = CreateObject("roTimespan")
    ' When the channel playing was last asked for again (m.span's ms).
    m.refreshedAt = -1
    ' Which tuning the answers belong to, the watch asked for last (until
    ' it answers), the session and its playlist.
    m.attempt = 0
    m.watchRequest = invalid
    m.sessionId = ""
    m.playlist = ""
    m.played = false
    ' The times the channel was tuned again after failing, since it was
    ' tuned or Try again was chosen (liveRetryDelay), and the tuning the
    ' retry waiting belongs to.
    m.retries = 0
    m.retryAttempt = 0
    m.trouble = invalid
    m.video.notificationInterval = 1
    m.video.observeField("state", "onState")
    m.noticeButtons.observeField("buttonSelected", "onNoticeButton")
    m.channelList.observeField("itemSelected", "onListSelected")
    m.categoryList.observeField("itemSelected", "onCategorySelected")
    m.categoryList.observeField("itemFocused", "onCategoryFocused")
    m.bannerTimer.observeField("fire", "onBannerTimer")
    m.zapTimer.observeField("fire", "onZapTimer")
    m.clock.observeField("fire", "onClock")
    m.pictureTimer.observeField("fire", "onPictureTimer")
    m.toastTimer.observeField("fire", "onToastTimer")
    m.retryTimer.observeField("fire", "onRetryTimer")
    m.cardLogo.observeField("loadStatus", "onLogos")
    m.bannerLogo.observeField("loadStatus", "onLogos")
    m.top.observeField("active", "onActive")
    m.top.observeField("liveTakenOver", "onTakenOver")
end sub

sub onActive()
    if not m.top.active
        finish()
        return
    end if
    if m.started then return
    m.started = true
    ' A channel takes over from the music.
    m.global.music.control = "stop"
    ' The playlist and its segments carry the token: a refreshed one goes
    ' to the Video node's agent at once.
    m.global.session.observeFieldScoped("accessToken", "onToken")
    m.clock.control = "start"
    params = m.top.params
    category = httpString(params.category)
    if category <> "" then m.category = category
    channel = params.channel
    if GetInterface(channel, "ifAssociativeArray") = invalid then return
    m.categoryName = httpString(params.categoryName)
    if m.categoryName = "" then m.categoryName = liveCategoryName(m.category, [])
    m.listCategory = m.category
    m.listName = m.categoryName
    m.listTitle.text = m.categoryName
    askPage(m.category, 0, aroundIn(m.category, channel), "")
    tune(channel)
end sub

sub onToken()
    m.video.setHttpAgent(uiHttpAgent())
end sub

function nowSeconds() as integer
    return CreateObject("roDateTime").AsSeconds()
end function

' --- The category's channels ---

' Where a category's page is asked around: the channel. Recently watched
' changes with every channel watched: its order is the one it had as it
' was asked for, from its top.
function aroundIn(category as string, channel as dynamic) as integer
    if channel = invalid or category = "recent" then return 0
    return channel.id
end function

' A page of a category (from offset, or around a channel): for up and
' down when it is the category they go through (then pick "first" or
' "last" of it is the channel they lead to), for the list when it is the
' one the list shows (pick "top" or "bottom": where its focus goes).
' Within 10 s of a page of the category that failed, nothing more of it is
' asked: the other categories, the list's or up and down's, go on.
sub askPage(category as string, offset as integer, around as integer, pick as string)
    if category = m.failedCategory and not liveMayAskAgain(m.failedAt, m.span.TotalMilliseconds()) then return
    m.asks = m.asks + 1
    if category = m.category then m.pageAsked = m.asks
    if category = m.listCategory then m.listAsked = m.asks
    uiRequest({ op: "livePage", kind: "list", category: category, offset: offset, limit: livePageSize(), around: around, asked: m.asks, pick: pick }, "onPage")
end sub

sub onPage(event as object)
    request = event.getRoSGNode()
    spec = request.spec
    zapping = spec.category = m.category and spec.asked = m.pageAsked
    listed = spec.category = m.listCategory and spec.asked = m.listAsked
    if not zapping and not listed then return
    answer = event.getData()
    if not answer.ok
        ' Watching goes on; the list says why it is empty.
        m.failedPage = { category: spec.category, offset: spec.offset, around: spec.around }
        m.failedAt = m.span.TotalMilliseconds()
        m.failedCategory = spec.category
        if listed and m.listPage = invalid then m.listRange.text = answer.message
        return
    end if
    if spec.category = m.failedCategory
        m.failedPage = invalid
        m.failedAt = -1
        m.failedCategory = ""
    end if
    page = answer.data
    content = request.content
    if content = invalid then content = CreateObject("roSGNode", "ContentNode")
    if zapping
        m.page = page
        m.pageContent = content
        ' The channel shown, with what's on now.
        at = -1
        if m.channel <> invalid then at = liveIndexOf(page.channels, m.channel.id)
        if at >= 0
            m.channel = page.channels[at]
            if m.banner.visible then showBanner(false)
        end if
        if spec.pick = "first" and page.channels.Count() > 0 then zapTo(page.channels[0])
        if spec.pick = "last" and page.channels.Count() > 0 then zapTo(page.channels[page.channels.Count() - 1])
    end if
    if listed then showListPage(page, content, spec.pick)
end sub

' A page in the list, its focus on the channel playing when it is there,
' else at its top, or at its top or bottom as pick says (paged to).
sub showListPage(page as object, content as object, pick as string)
    m.listPage = page
    m.channelList.content = content
    markCurrent()
    if not m.categoryList.visible then m.listRange.text = liveRange(page.offset, page.channels.Count(), page.total)
    at = -1
    if m.channel <> invalid then at = liveIndexOf(page.channels, m.channel.id)
    if at < 0 or pick = "top" then at = 0
    if pick = "bottom" then at = content.getChildCount() - 1
    if at >= 0 and at < content.getChildCount() then m.channelList.jumpToItem = at
end sub

' The channel playing stands out in the list.
sub markCurrent()
    content = m.channelList.content
    if content = invalid then return
    for i = 0 to content.getChildCount() - 1
        item = content.getChild(i)
        shown = m.channel <> invalid and item.channelId = m.channel.id
        if item.current <> shown then item.current = shown
    end for
end sub

' The list opens on the category up and down go through, at the channel
' playing, as on Android: another chosen in it last time, or a search,
' isn't kept.
sub openList()
    hideBanner()
    m.listSearched = false
    if m.listCategory <> m.category
        m.listCategory = m.category
        m.listAsked = m.pageAsked
        m.listPage = invalid
        m.channelList.content = invalid
        m.listRange.text = ""
        if m.page <> invalid then showListPage(m.page, m.pageContent, "")
    end if
    m.listName = m.categoryName
    m.listTitle.text = m.listName
    if m.page = invalid then askAgain(m.category)
    m.list.visible = true
    if m.channel <> invalid and m.listPage <> invalid
        at = liveIndexOf(m.listPage.channels, m.channel.id)
        if at >= 0 then m.channelList.jumpToItem = at
    end if
    m.channelList.setFocus(true)
end sub

sub closeList()
    closeCategories()
    m.list.visible = false
    ' The list gives the focus up: its parent taking it may not.
    m.channelList.setFocus(false)
    if m.notice.visible
        m.noticeButtons.setFocus(true)
    else
        m.top.setFocus(true)
    end if
end sub

' The page that failed, when it was category's, asked for again.
sub askAgain(category as string)
    if m.failedPage <> invalid and m.failedPage.category = category then askPage(category, m.failedPage.offset, m.failedPage.around, "")
end sub

' Past the list's ends, the page after it (or before), round the ends of
' its category.
sub listPage(by as integer)
    page = m.listPage
    if page = invalid or page.total <= page.channels.Count() then return
    size = livePageSize()
    pages = Int((page.total + size - 1) / size)
    pageNo = Int(page.offset / size)
    pick = "top"
    if by < 0 then pick = "bottom"
    askPage(m.listCategory, ((pageNo + by + pages) mod pages) * size, 0, pick)
end sub

sub onListSelected(event as object)
    page = m.listPage
    if page = invalid then return
    at = event.getData()
    if at < 0 or at >= page.channels.Count() then return
    channel = page.channels[at]
    ' Chosen in another category (or a search): up and down go through
    ' that one from now on.
    if m.listCategory <> m.category
        m.category = m.listCategory
        m.categoryName = m.listName
        m.page = page
        m.pageContent = m.channelList.content
        m.pageAsked = m.listAsked
    end if
    closeList()
    if m.channel <> invalid and channel.id = m.channel.id and not m.notice.visible then return
    tune(channel)
end sub

' --- Another category, or a search, in the list ---

' What a category is called in the list.
function listNameOf(category as string) as string
    if category = m.category then return m.categoryName
    return liveCategoryName(category, m.groups)
end function

' The list shows category's channels, from the channel playing when it is
' among them; up and down keep theirs until a channel is chosen there.
sub showListCategory(category as string)
    m.listSearched = false
    m.listName = listNameOf(category)
    closeCategories()
    m.listTitle.text = m.listName
    if category = m.listCategory and m.listPage <> invalid then return
    m.listCategory = category
    m.listPage = invalid
    m.channelList.content = invalid
    if category = m.category and m.page <> invalid
        m.listAsked = m.pageAsked
        showListPage(m.page, m.pageContent, "")
        return
    end if
    m.listRange.text = "Loading…"
    ' Chosen: asked for at once, whatever failed before.
    m.failedAt = -1
    m.failedCategory = ""
    askPage(category, 0, aroundIn(category, m.channel), "")
end sub

' Left in the list: the categories as Live TV's rail lists them (GET
' /api/live/groups, the first time), the one the list shows in focus.
sub openCategories()
    m.channelList.visible = false
    m.categoryList.visible = true
    m.listTitle.text = "Categories"
    m.listRange.text = ""
    m.listHint.text = "OK: its channels   ·   Back: the channels"
    m.categoryList.setFocus(true)
    if m.summary = invalid
        m.listRange.text = "Loading…"
        uiRequest({ method: "GET", path: "/api/live/groups" }, "onGroups")
        return
    end if
    buildCategories(false)
end sub

sub onGroups(event as object)
    answer = event.getData()
    if not answer.ok
        if m.categoryList.visible then m.listRange.text = answer.message
        return
    end if
    m.summary = liveSummary(answer.data)
    m.groups = liveGroups(answer.data)
    region = liveRegionOf(m.listCategory, m.groups)
    if region <> "" then m.open = region
    if m.categoryList.visible then buildCategories(false)
end sub

' The categories, focus on the one the list shows (or, with keep, on the
' one in focus).
sub buildCategories(keep as boolean)
    m.listRange.text = ""
    wanted = m.listCategory
    at = m.categoryList.itemFocused
    if keep and at >= 0 and at < m.railEntries.Count() then wanted = m.railEntries[at].key
    m.railEntries = liveRail(m.summary, m.groups, m.open, "")
    at = 0
    for i = 0 to m.railEntries.Count() - 1
        if m.railEntries[i].kind <> "heading" and m.railEntries[i].key = wanted then at = i
    end for
    m.railFocused = at
    m.categoryList.content = nodesContent(liveRailItems(m.railEntries, m.open, m.listCategory))
    m.categoryList.jumpToItem = at
end sub

sub onCategoryFocused(event as object)
    at = event.getData()
    target = liveRailPast(m.railEntries, at, m.railFocused)
    m.railFocused = target
    if target <> at then m.categoryList.jumpToItem = target
end sub

' OK on a category shows its channels; on a region, its groups open (and
' another's close) or close.
sub onCategorySelected(event as object)
    at = event.getData()
    if at < 0 or at >= m.railEntries.Count() then return
    entry = m.railEntries[at]
    if entry.kind = "heading" then return
    if entry.kind = "region"
        if m.open = entry.key
            m.open = ""
        else
            m.open = entry.key
        end if
        buildCategories(true)
        return
    end if
    showListCategory(entry.key)
end sub

' Back from the categories: the list's channels again.
sub closeCategories()
    if not m.categoryList.visible then return
    m.categoryList.visible = false
    m.channelList.visible = true
    m.listTitle.text = m.listName
    m.listHint.text = listHintText()
    m.listRange.text = ""
    if m.listPage <> invalid then m.listRange.text = liveRange(m.listPage.offset, m.listPage.channels.Count(), m.listPage.total)
    if m.list.visible then m.channelList.setFocus(true)
end sub

function listHintText() as string
    return "OK: watch   ·   Options (*): favourite   ·   Back: close" + Chr(10) + "Left: categories   ·   Replay: search"
end function

' Replay in the list: Roku's keyboard, every channel whose name, number or
' programme on now has the words in it.
sub askListSearch()
    dialog = CreateObject("roSGNode", "StandardKeyboardDialog")
    dialog.title = "Search channels"
    dialog.message = ["Channels and what's on"]
    if Left(m.listCategory, 7) = "search:" then dialog.text = Mid(m.listCategory, 8)
    dialog.buttons = ["Search", "Cancel"]
    dialog.observeField("buttonSelected", "onListSearch")
    m.top.getScene().dialog = dialog
end sub

sub onListSearch(event as object)
    dialog = m.top.getScene().dialog
    text = ""
    labels = []
    if dialog <> invalid
        text = dialog.text.Trim()
        labels = dialog.buttons
    end if
    uiCloseDialog()
    chosen = event.getData()
    if chosen < 0 or chosen >= labels.Count() or labels[chosen] <> "Search" then return
    query = httpSentence(text)
    if query = "" then return
    ' The category shown as the first of the searches made here began.
    if not m.listSearched then m.listBefore = m.listCategory
    showListCategory("search:" + query)
    m.listSearched = true
end sub

' The Options key in the list: the channel in focus in or out of the
' favourites.
sub toggleListed()
    if m.listPage = invalid then return
    at = m.channelList.itemFocused
    if at < 0 or at >= m.listPage.channels.Count() then return
    toggleFavorite(m.listPage.channels[at])
end sub

' A channel in or out of the favourites (PUT or DELETE
' /api/live/channels/{id}/favorite): its star changes at once wherever it
' shows, and goes back if the server refuses.
sub toggleFavorite(channel as object)
    favorite = not channel.favorite
    markFavorite(channel.id, favorite)
    method = "DELETE"
    if favorite then method = "PUT"
    uiRequest({ method: method, path: "/api/live/channels/" + urlValue(channel.id) + "/favorite", channelId: channel.id, favorite: favorite, name: channel.name }, "onFavorite")
end sub

' The channel's star: on the channel playing (or the one Up or Down leads
' to), in the pages held and their rows, and on the banner.
sub markFavorite(id as integer, favorite as boolean)
    for each channel in [m.channel, m.zapTo]
        if channel <> invalid and channel.id = id then channel.favorite = favorite
    end for
    for each held in [{ page: m.page, rows: m.pageContent }, { page: m.listPage, rows: m.channelList.content }]
        if held.page <> invalid
            at = liveIndexOf(held.page.channels, id)
            if at >= 0
                held.page.channels[at].favorite = favorite
                item = invalid
                if held.rows <> invalid then item = held.rows.getChild(at)
                if item <> invalid then item.favorite = favorite
            end if
        end if
    end for
    if m.banner.visible then showBanner(false)
end sub

sub onFavorite(event as object)
    answer = event.getData()
    spec = event.getRoSGNode().spec
    if not answer.ok
        markFavorite(spec.channelId, not spec.favorite)
        if not uiFailed(answer) then showToast(answer.message)
        return
    end if
    if spec.favorite
        showToast(spec.name + " is in your favourites")
    else
        showToast(spec.name + " is out of your favourites")
    end if
end sub

' --- Tuning ---

' Up and down: the next channel of the category (round its ends), the
' page after or before it fetched at its ends.
sub zap(by as integer)
    from = m.zapTo
    if from = invalid then from = m.channel
    if m.page = invalid then askAgain(m.category)
    if from = invalid or m.page = invalid then return
    move = liveZap(m.page, from.id, by)
    if move = invalid then return
    if move.channel <> invalid
        zapTo(move.channel)
        return
    end if
    which = "first"
    if move.last then which = "last"
    askPage(m.category, move.offset, 0, which)
end sub

' The channel's logo and name show at once, and it is tuned once the
' presses stop. What's on shows only when asked for.
sub zapTo(channel as object)
    m.zapTo = channel
    hideBanner()
    showCard(channel, "")
    m.bannerTimer.control = "stop"
    m.zapTimer.control = "stop"
    m.zapTimer.control = "start"
end sub

sub onZapTimer()
    target = m.zapTo
    m.zapTo = invalid
    if target = invalid then return
    if m.channel <> invalid and target.id = m.channel.id and m.sessionId <> ""
        ' Back where it started.
        if m.played then m.card.visible = false
        return
    end if
    tune(target)
end sub

' Watches channel: the session before it ends (the server ends it too,
' by the client_id), its logo shows while it starts. again: tuned again
' after a failure, which counts against the retries.
sub tune(channel as object, takeOver = 0 as integer, again = false as boolean)
    m.attempt = m.attempt + 1
    abandonWatch()
    leave()
    if not again then m.retries = 0
    m.retryTimer.control = "stop"
    m.zapTo = invalid
    m.zapTimer.control = "stop"
    m.channel = channel
    ' Kept for the server and profile: Live TV opens at it, after a
    ' restart too.
    session = m.global.session
    liveSaveChannel(CreateObject("roRegistrySection", "liveViews"), liveViewKey(session.server, session.profileId), channel.id)
    m.played = false
    m.trouble = invalid
    m.pictureTimer.control = "stop"
    m.video.control = "stop"
    m.notice.visible = false
    showWaiting(false)
    showCard(channel, "")
    hideBanner()
    markCurrent()
    if not m.list.visible then m.top.setFocus(true)
    m.watchRequest = uiRequest({ op: "liveWatch", channelId: channel.id, takeOver: takeOver, attempt: m.attempt }, "onWatch")
end sub

' The watch asked for isn't wanted any more: the API task ends it if it
' comes.
sub abandonWatch()
    if m.watchRequest = invalid then return
    m.watchRequest.abandoned = true
    m.watchRequest = invalid
end sub

' DELETE /api/live/sessions/{session}: the connection goes at once.
sub leave()
    if m.sessionId = "" then return
    uiRequest({ method: "DELETE", path: "/api/live/sessions/" + urlEncode(m.sessionId) }, "")
    m.sessionId = ""
    m.playlist = ""
end sub

function isCurrent(event as object) as boolean
    return event.getRoSGNode().spec.attempt = m.attempt
end function

sub onWatch(event as object)
    if not isCurrent(event) then return
    m.watchRequest = invalid
    answer = event.getData()
    if not answer.ok or httpObject(answer.data) = invalid
        if answer.error = "signed_out" then return
        if answer.error = "network"
            uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
            return
        end if
        trouble(liveTrouble(answer, m.channel.name))
        return
    end if
    m.sessionId = httpString(answer.data.session_id)
    m.playlist = httpString(answer.data.url)
    fresh = liveChannel(answer.data.channel, m.global.session.server, liveLogoWidth(), m.category)
    if fresh <> invalid and fresh.id = m.channel.id then m.channel = fresh
    showCard(m.channel, "")
    if m.banner.visible then showBanner(false)
    ' The playlist once, waited for: the server answers it once the
    ' channel's first segments are made, or says why not.
    uiRequest({ method: "GET", path: m.playlist, timeoutMs: 50000, attempt: m.attempt }, "onPlaylist")
end sub

sub onPlaylist(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if not answer.ok
        if answer.error = "signed_out" then return
        if answer.error = "network"
            uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
            return
        end if
        failed(liveTrouble(answer, m.channel.name))
        return
    end if
    play()
end sub

' The live stream in the Video node, its token in the agent's header.
sub play()
    content = CreateObject("roSGNode", "ContentNode")
    content.url = m.global.session.server + m.playlist
    content.streamFormat = "hls"
    content.title = liveTitle(m.channel)
    content.live = true
    if LCase(Left(content.url, 8)) = "https://" then nodesFill(content, { HttpCertificatesFile: "common:/certs/ca-bundle.crt" })
    m.video.setHttpAgent(uiHttpAgent())
    m.video.content = content
    m.video.control = "play"
end sub

sub onState(event as object)
    state = event.getData()
    ' A stall shows the spinner only once it has lasted a moment: the
    ' briefest flicker of one doesn't.
    if state = "buffering" and m.played
        if not m.stalling
            m.stalling = true
            m.stallTimer.control = "start"
        end if
    else
        m.stalling = false
        m.stallTimer.control = "stop"
        showWaiting(false)
    end if
    if state = "playing"
            if not m.played
            m.played = true
            m.card.visible = false
            m.tuning.control = "stop"
            ' A channel without pictures (radio) keeps its card.
            m.pictureTimer.control = "start"
        end if
        return
    end if
    if state = "error" or state = "finished"
        broken()
    end if
end sub

sub onStall()
    if m.stalling then showWaiting(true)
end sub

' Someone took over the connection of the channel playing (the server's
' live_taken_over, sent before the stream breaks off, one for each viewer
' of the channel; matched on the channel): it stops at once with the
' server's words, said once, and isn't tuned again by itself. Said already
' from the playlist's 410 without words, the event's words show instead.
' Changing channel already (Up or Down pressed), the change goes on to the
' channel chosen, with no notice of the one left.
sub onTakenOver(event as object)
    data = event.getData()
    if m.channel = invalid or not m.started then return
    if Int(httpNumber(data.channel_id, 0)) <> m.channel.id then return
    stopped = liveStopped(data.message)
    if m.trouble <> invalid
        if m.trouble.kind = "stopped" and httpSentence(data.message) <> "" and m.trouble.message <> stopped.message then trouble(stopped)
        return
    end if
    ' What is on its way belongs to watching that ended, and its session is
    ' gone on the server: nothing to leave.
    m.attempt = m.attempt + 1
    abandonWatch()
    m.sessionId = ""
    m.playlist = ""
    if m.zapTo <> invalid
        m.video.control = "stop"
        return
    end if
    trouble(stopped)
end sub

sub showWaiting(shown as boolean)
    if shown = m.waiting.visible then return
    m.waiting.visible = shown
    if shown
        m.waiting.control = "start"
    else
        m.waiting.control = "stop"
    end if
end sub

sub onPictureTimer()
    if not m.played or m.channel = invalid then return
    video = httpString(m.video.videoFormat)
    audio = httpString(m.video.audioFormat)
    if video = "" and audio <> "" then showCard(m.channel, "radio")
end sub

' The stream broke: what the server says of its playlist says what to do.
' Gone or refused, the player says so; otherwise the channel is tuned
' again (failed).
sub broken()
    if m.sessionId = "" or m.trouble <> invalid then return
    m.video.control = "stop"
    showCard(m.channel, "")
    uiRequest({ method: "GET", path: m.playlist, timeoutMs: 50000, attempt: m.attempt }, "onChecked")
end sub

sub onChecked(event as object)
    if not isCurrent(event) then return
    answer = event.getData()
    if answer.error = "signed_out" then return
    if answer.error = "network"
        uiUnreachable({ to: "livePlayer", params: { channel: m.channel, category: m.category } })
        return
    end if
    if not answer.ok
        failed(liveTrouble(answer, m.channel.name))
        return
    end if
    ' Its playlist is there: the stream itself broke off.
    message = "It stopped and couldn't get going again."
    failed({ kind: "other", title: m.channel.name + " stopped", message: message, lines: message, retry: true, connections: [] })
end sub

' Watching failed: the channel is tuned again where that may help (the
' server let it go, it wasn't ready, the stream broke), twice at most,
' after 1 s and then 3 s; after that, or when the server refused it, the
' player says why.
sub failed(problem as object)
    leave()
    delayMs = -1
    if liveRetried(problem) then delayMs = liveRetryDelay(m.retries)
    if delayMs < 0
        trouble(problem)
        return
    end if
    m.retries = m.retries + 1
    m.played = false
    m.video.control = "stop"
    showCard(m.channel, "")
    m.retryAttempt = m.attempt
    m.retryTimer.duration = delayMs / 1000
    m.retryTimer.control = "start"
end sub

sub onRetryTimer()
    if m.retryAttempt <> m.attempt or m.trouble <> invalid or m.channel = invalid then return
    tune(m.channel, 0, true)
end sub

' --- What shows ---

' The channel's logo (or its name) in the middle, over black, the spinner
' under it while it tunes; over a channel without pictures ("radio"),
' what's on instead.
sub showCard(channel as object, kind as string)
    m.cardTitle.text = liveTitle(channel)
    on = liveOnNow(channel, nowSeconds())
    line = "Tuning…"
    if kind = "radio"
        line = "Radio"
        if on <> invalid then line = on.title
    else if on <> invalid
        line = on.title
    end if
    m.cardLine.text = line
    m.cardInitials.text = channel.name
    ' The scene's agent carries the token; it changes with each refresh.
    m.cardLogo.setHttpAgent(m.top.getScene().getHttpAgent())
    m.cardLogo.uri = channel.logo
    onLogos()
    m.tuning.visible = kind <> "radio"
    if kind = "radio"
        m.tuning.control = "stop"
    else
        m.tuning.control = "start"
    end if
    m.card.visible = true
end sub

' Names show where no logo does (logos are often see-through).
sub onLogos()
    m.cardInitials.visible = m.cardLogo.uri = "" or m.cardLogo.loadStatus = "failed"
    m.bannerInitials.visible = m.bannerLogo.uri = "" or m.bannerLogo.loadStatus = "failed"
end sub

' The banner: the channel, what's on with when and how long it has left,
' how far along, what's next, and the Live chip. With hides, it goes
' after 5 s.
sub showBanner(hides as boolean)
    channel = m.zapTo
    if channel = invalid then channel = m.channel
    if channel = invalid then return
    now = nowSeconds()
    on = liveOnNow(channel, now)
    coming = liveComingUp(channel, now)
    m.bannerChannel.text = liveTitle(channel)
    m.bannerInitials.text = channel.name
    m.bannerLogo.setHttpAgent(m.top.getScene().getHttpAgent())
    m.bannerLogo.uri = channel.logo
    onLogos()
    if on <> invalid
        m.bannerTitle.text = on.title
        meta = [liveSpan(on, m.twelve), liveTimeLeft(on, now)]
        if on.line <> "" then meta.Push(on.line)
        if on.badges <> "" then meta.Push(on.badges)
        m.bannerMeta.text = meta.Join("   ·   ")
        m.bannerFill.width = Int(1100 * liveProgress(on, now))
        m.bannerTrack.visible = true
        m.bannerFill.visible = true
    else
        m.bannerTitle.text = channel.name
        m.bannerMeta.text = "Nothing in the guide now"
        if not channel.hasGuide then m.bannerMeta.text = "No guide for this channel"
        m.bannerTrack.visible = false
        m.bannerFill.visible = false
    end if
    m.bannerNext.text = ""
    if coming <> invalid then m.bannerNext.text = "Next   " + liveClock(coming.start, m.twelve) + "   " + coming.title
    m.star.text = ""
    if channel.favorite then m.star.text = "★ Favourite"
    m.bannerHint.text = "Up and down: channels   ·   Left: " + m.categoryName + "   ·   Options (*): favourite   ·   OK: close"
    if m.list.visible or m.notice.visible then return
    m.banner.visible = true
    m.bannerTimer.control = "stop"
    if hides then m.bannerTimer.control = "start"
end sub

sub hideBanner()
    m.bannerTimer.control = "stop"
    m.banner.visible = false
end sub

sub onBannerTimer()
    if m.zapTo = invalid then hideBanner()
end sub

' Each 15 s: what's on moves on, on the banner and in the list; once the
' programme listed as on now has ended, the channel playing is asked for
' again (GET /api/live/channels/{id}), at most once a minute.
sub onClock()
    if m.banner.visible then showBanner(false)
    if m.list.visible then redrawList()
    if m.channel = invalid or not liveNowEnded([m.channel], 0, 1, nowSeconds()) then return
    ms = m.span.TotalMilliseconds()
    if not liveMayRefresh(m.refreshedAt, ms) then return
    m.refreshedAt = ms
    uiRequest({ method: "GET", path: "/api/live/channels/" + urlValue(m.channel.id), channelId: m.channel.id }, "onChannelFresh")
end sub

sub onChannelFresh(event as object)
    answer = event.getData()
    if not answer.ok or m.channel = invalid then return
    fresh = liveChannel(answer.data, m.global.session.server, liveLogoWidth(), m.category)
    if fresh = invalid or fresh.id <> m.channel.id then return
    m.channel = fresh
    for each page in [m.page, m.listPage]
        if page <> invalid
            at = liveIndexOf(page.channels, fresh.id)
            if at >= 0 then page.channels[at] = fresh
        end if
    end for
    if m.banner.visible then showBanner(false)
    if m.list.visible then redrawList()
    ' Radio shows what's on in its card.
    if m.played and m.card.visible and m.zapTo = invalid then showCard(m.channel, "radio")
end sub

' The list's channels, what's on each and how far along.
sub redrawList()
    content = m.channelList.content
    if m.listPage = invalid or content = invalid then return
    now = nowSeconds()
    for i = 0 to m.listPage.channels.Count() - 1
        item = content.getChild(i)
        if item <> invalid
            row = liveListItem(m.listPage.channels[i], now, m.twelve)
            if item.line <> row.line then item.line = row.line
            if Abs(item.progress - row.progress) > 0.001 then item.progress = row.progress
        end if
    end for
end sub

sub showToast(text as string)
    m.toastText.text = text
    m.toast.visible = true
    m.toastTimer.control = "stop"
    m.toastTimer.control = "start"
end sub

sub onToastTimer()
    m.toast.visible = false
end sub

' A watching that didn't start or stopped: what the server said, with Try
' again where it may help (Watch again when it stopped), Other channels,
' and Back.
sub trouble(problem as object)
    m.trouble = problem
    m.retryTimer.control = "stop"
    m.video.control = "stop"
    m.card.visible = false
    m.tuning.control = "stop"
    showWaiting(false)
    hideBanner()
    m.noticeTitle.text = problem.title
    m.noticeText.text = problem.lines
    ' What holds each connection takes room: the text on the left, the
    ' buttons beside it, all from the top.
    if problem.connections.Count() > 0
        m.noticeTitle.translation = [96, 120]
        m.noticeText.translation = [96, 210]
        m.noticeText.width = 920
        m.noticeText.maxLines = 16
        m.noticeButtons.translation = [1104, 210]
        m.noticeButtons.minWidth = 720
    else
        m.noticeTitle.translation = [96, 340]
        m.noticeText.translation = [96, 430]
        m.noticeText.width = 1500
        m.noticeText.maxLines = 6
        m.noticeButtons.translation = [96, 700]
        m.noticeButtons.minWidth = 400
    end if
    ' Every connection in use: what holds each, and Watch here instead
    ' where this account may take one over.
    m.noticeActions = liveTroubleButtons(problem)
    labels = []
    for each button in m.noticeActions
        labels.Push(button.label)
    end for
    m.noticeButtons.buttons = labels
    m.notice.visible = true
    if not m.list.visible then m.noticeButtons.setFocus(true)
end sub

sub onNoticeButton(event as object)
    at = event.getData()
    if m.noticeActions = invalid or at < 0 or at >= m.noticeActions.Count() then return
    button = m.noticeActions[at]
    if button.action = "takeOver"
        tune(m.channel, button.channelId)
    else if button.action = "retry"
        tune(m.channel)
    else if button.action = "list"
        openList()
    else
        uiNavigate({ back: true })
    end if
end sub

' Leaving: the session ends, the token's observer goes, and Live TV opens
' back at this channel.
sub finish()
    m.attempt = m.attempt + 1
    abandonWatch()
    leave()
    m.video.control = "stop"
    for each timer in [m.bannerTimer, m.zapTimer, m.clock, m.pictureTimer, m.toastTimer, m.stallTimer, m.retryTimer]
        timer.control = "stop"
    end for
    if m.channel <> invalid then m.top.getScene().liveChannel = m.channel.id
    if m.started then m.global.session.unobserveFieldScoped("accessToken")
end sub

' Up and down change channel (Up: the next number); Left opens the
' category's channels, Right and OK show what's on (it never shows by
' itself as a channel starts), and the Options key (*) puts the channel in
' the favourites or takes it out, the banner showing its star; play,
' rewind, fast forward and Replay do nothing. In the list:
' the Options key adds to or takes out of the favourites, up and down past
' its ends go to the page before or after, Left opens the categories
' (Back and Right close them), Replay searches, Back leaves a search for
' the category before, and Back and Right close it. Back with the banner
' showing hides it; otherwise it leaves (the scene).
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if m.list.visible and m.categoryList.visible
        if key = "back" or key = "right" then closeCategories()
        return true
    end if
    if m.list.visible
        if key = "back" and m.listSearched
            showListCategory(m.listBefore)
            return true
        end if
        if key = "back" or key = "right"
            closeList()
            return true
        end if
        if key = "left"
            openCategories()
            return true
        end if
        if key = "replay"
            askListSearch()
            return true
        end if
        if key = "options"
            toggleListed()
            return true
        end if
        if key = "up" and m.channelList.itemFocused = 0
            listPage(-1)
            return true
        end if
        if key = "down" and m.channelList.content <> invalid and m.channelList.itemFocused = m.channelList.content.getChildCount() - 1
            listPage(1)
            return true
        end if
        return true
    end if
    if m.notice.visible then return false
    if key = "up"
        zap(1)
        return true
    end if
    if key = "down"
        zap(-1)
        return true
    end if
    if key = "left"
        openList()
        return true
    end if
    ' Live is live: no pause, rewind or skip.
    if key = "play" or key = "rewind" or key = "fastforward" or key = "replay" then return true
    if key = "options"
        channel = m.zapTo
        if channel = invalid then channel = m.channel
        if channel <> invalid
            toggleFavorite(channel)
            showBanner(true)
        end if
        return true
    end if
    if key = "right" or key = "OK"
        if m.banner.visible
            hideBanner()
        else
            showBanner(true)
        end if
        return true
    end if
    if key = "back" and m.banner.visible and m.zapTo = invalid
        hideBanner()
        return true
    end if
    return false
end function
