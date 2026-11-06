sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.title = m.top.findNode("title")
    m.meta = m.top.findNode("meta")
    m.play = m.top.findNode("play")
    m.seasons = m.top.findNode("seasons")
    m.episodes = m.top.findNode("episodes")
    m.message = m.top.findNode("message")
    m.seasonTimer = m.top.findNode("seasonTimer")
    m.message.text = t("common.states.loading")
    m.started = false
    m.showId = 0
    m.wantedSeason = 0
    ' The season whose episodes are shown, and the one asked for last.
    m.shownSeason = 0
    m.askedSeason = 0
    m.onDeck = invalid
    m.lastFocus = m.seasons
    m.play.observeField("buttonSelected", "onPlay")
    m.seasons.observeField("itemFocused", "onSeasonFocused")
    m.seasons.observeField("itemSelected", "onSeasonSelected")
    m.episodes.observeField("itemSelected", "onEpisode")
    m.seasonTimer.observeField("fire", "loadSeason")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active
        ' Covered by an episode's details: focus comes back to the same place.
        m.lastFocus = m.seasons
        if m.episodes.isInFocusChain() then m.lastFocus = m.episodes
        if m.play.isInFocusChain() then m.lastFocus = m.play
        return
    end if
    if not m.started
        m.started = true
        m.showId = m.top.params.showId
        m.wantedSeason = m.top.params.seasonId
        path = "/api/items/" + urlValue(m.showId)
        uiGet(path, "onShow")
        uiRequest({ method: "GET", path: path + "/children", build: "seasons" }, "onSeasons")
        uiGet(path + "/on-deck", "onOnDeck")
        uiChangesSince()
        m.seasons.setFocus(true)
        return
    end if
    ' Back from an episode: its place and watched marks changed.
    uiChangesSince()
    reloadEpisodes()
    m.lastFocus.setFocus(true)
end sub

sub reloadEpisodes()
    uiGet("/api/items/" + urlValue(m.showId) + "/on-deck", "onOnDeck")
    if m.shownSeason <> 0 then requestEpisodes(m.shownSeason)
end sub

' An episode or a season marked watched or unwatched with the Options key:
' shown at once, then as the server has it.
sub onTitleChanged(change as object)
    uiPatchTiles(m.episodes.content, change)
    reloadEpisodes()
end sub

sub onShow(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid then return
    show = answer.data
    m.title.text = httpSentence(show.title)
    parts = []
    year = httpNumber(show.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    seasons = httpNumber(show.child_count, 0)
    if seasons > 0 then parts.Push(t("titles.counts.seasons", { count: Int(seasons) }))
    m.meta.text = parts.Join(" · ")
    m.backdrop.setHttpAgent(m.top.getScene().getHttpAgent())
    m.backdrop.uri = urlImage(m.global.session.server, show.backdrop_url, contentBackdropWidth())
end sub

sub onSeasons(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    content = event.getRoSGNode().content
    if content = invalid or content.getChildCount() = 0
        m.message.text = t("titles.seasons.noEpisodes")
        return
    end if
    m.seasons.content = content
    index = 0
    for i = 0 to content.getChildCount() - 1
        if content.getChild(i).itemId = m.wantedSeason then index = i
    end for
    m.seasons.jumpToItem = index
    requestEpisodes(content.getChild(index).itemId)
end sub

' Seasons change as focus moves; the episodes follow once it rests.
sub onSeasonFocused()
    m.seasonTimer.control = "stop"
    m.seasonTimer.control = "start"
end sub

sub loadSeason()
    season = invalid
    if m.seasons.content <> invalid then season = m.seasons.content.getChild(m.seasons.itemFocused)
    if season = invalid then return
    if season.itemId <> m.askedSeason then requestEpisodes(season.itemId)
end sub

sub onSeasonSelected()
    loadSeason()
    if m.episodes.content <> invalid and m.episodes.content.getChildCount() > 0 then m.episodes.setFocus(true)
end sub

sub requestEpisodes(seasonId as dynamic)
    m.askedSeason = seasonId
    ' The season rides along in the spec, which the API task ignores.
    uiRequest({ method: "GET", path: "/api/items/" + urlValue(seasonId) + "/children", build: "episodes", seasonId: seasonId }, "onEpisodes")
end sub

sub onEpisodes(event as object)
    request = event.getRoSGNode()
    seasonId = request.spec.seasonId
    ' An answer for a season left meanwhile.
    if seasonId <> m.askedSeason then return
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer)
            m.message.text = answer.message
            m.message.visible = true
        end if
        return
    end if
    keep = 0
    if seasonId = m.shownSeason then keep = m.episodes.itemFocused
    m.shownSeason = seasonId
    m.episodes.content = request.content
    if keep > 0 and keep < answer.data.size then m.episodes.jumpToItem = keep
    if answer.data.size = 0
        m.message.text = t("titles.seasons.noEpisodes")
        m.message.visible = true
    else
        m.message.visible = false
    end if
end sub

sub onEpisode(event as object)
    uiOpenTile(m.episodes.content.getChild(event.getData()))
end sub

sub onOnDeck(event as object)
    answer = event.getData()
    m.onDeck = invalid
    if answer.ok and answer.data <> invalid and httpObject(answer.data.item) <> invalid
        item = answer.data.item
        m.onDeck = contentTile(item, m.global.session.server, true)
        key = "titles.actions.playEpisode"
        if fmtResumable(m.onDeck.positionMs, m.onDeck.durationMs) then key = "titles.actions.resumeEpisode"
        m.play.buttons = [t(key, { episode: fmtEpisode(item.parent_index, item.index) })]
    end if
    hadFocus = m.play.isInFocusChain()
    m.play.visible = m.onDeck <> invalid
    if hadFocus and not m.play.visible then m.seasons.setFocus(true)
end sub

sub onPlay()
    if m.onDeck = invalid then return
    uiNavigate({ to: "player", params: { itemId: m.onDeck.itemId, startMs: playerStartOf(m.onDeck.positionMs, m.onDeck.durationMs) } })
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    hasEpisodes = m.episodes.content <> invalid and m.episodes.content.getChildCount() > 0
    if key = "right" and (m.seasons.isInFocusChain() or m.play.isInFocusChain()) and hasEpisodes
        m.episodes.setFocus(true)
        return true
    end if
    if key = "left" and m.episodes.isInFocusChain()
        m.seasons.setFocus(true)
        return true
    end if
    if key = "up" and m.seasons.isInFocusChain() and m.play.visible
        m.play.setFocus(true)
        return true
    end if
    if key = "down" and m.play.isInFocusChain()
        m.seasons.setFocus(true)
        return true
    end if
    if key = "options" and m.episodes.isInFocusChain() and hasEpisodes
        ' Its show is this one: no Go to show.
        episode = m.episodes.content.getChild(m.episodes.itemFocused)
        if episode <> invalid then uiTileOptions({ itemId: episode.itemId, kind: "episode", title: episode.title, watched: episode.watched, parentId: episode.parentId, grandparentId: 0 }, false)
        return true
    end if
    if key = "options" and m.seasons.isInFocusChain() and m.seasons.content <> invalid
        season = m.seasons.content.getChild(m.seasons.itemFocused)
        if season <> invalid then uiTileOptions({ itemId: season.itemId, kind: "season", title: season.title, watched: season.episodes > 0 and season.watchedEpisodes >= season.episodes, parentId: m.showId, grandparentId: 0 }, false)
        return true
    end if
    return false
end function
