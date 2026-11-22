' Home's hero and the info block over its rows, as on Android TV and the
' native Samsung app (their HomeState and Home.cs): the title they show,
' and what they say about it.

' The title the hero shows, from GET /api/hubs/home's hubs: the first of
' Continue Watching, else the newest video in the rows, else the newest
' title; invalid with nothing to show.
function homeHeroItem(hubs as dynamic) as dynamic
    for each hub in httpArray(hubs)
        if GetInterface(hub, "ifAssociativeArray") <> invalid and contentContinueWatching(httpString(hub.key))
            for each item in httpArray(hub.items)
                if GetInterface(item, "ifAssociativeArray") <> invalid and contentShown(httpString(item.kind)) then return item
            end for
        end if
    end for
    newest = invalid
    newestVideo = invalid
    for each hub in httpArray(hubs)
        if GetInterface(hub, "ifAssociativeArray") <> invalid
            for each item in httpArray(hub.items)
                if GetInterface(item, "ifAssociativeArray") <> invalid and contentShown(httpString(item.kind))
                    added = httpNumber(item.added_at, 0)
                    if newest = invalid or added > httpNumber(newest.added_at, 0) then newest = item
                    if homeVideo(httpString(item.kind)) and (newestVideo = invalid or added > httpNumber(newestVideo.added_at, 0)) then newestVideo = item
                end if
            end for
        end if
    end for
    if newestVideo <> invalid then return newestVideo
    return newest
end function

' Movies, shows, seasons and episodes: the hero prefers them to music.
function homeVideo(kind as string) as boolean
    return kind = "movie" or kind = "show" or kind = "season" or kind = "episode"
end function

' What the hero or the info block says about item: an ItemSummary from the
' rows, or the ItemDetail once asked for (complete), which alone brings a
' movie's summary, the content rating and the quality badges. { itemId,
' kind, title (a show's name for an episode), context ("S2 E5 · The Long
' Night"), meta ("2019 · 2h 2m · PG-13 · ★ 7.4 · 4K · HDR") and its parts
' as MetaRow draws them ([{ text, boxed, star }]: the content rating and the
' quality boxed, the score after a star, as Android TV's), summary,
' progress (0 to 1), timeLeft ("26 min left" while in progress), video
' (it plays itself), watched, positionMs, durationMs, backdrop, complete }.
function homeInfo(item as object, server as string, complete as boolean) as object
    kind = httpString(item.kind)
    title = httpSentence(item.title)
    context = ""
    if kind = "episode"
        showTitle = httpSentence(item.grandparent_title)
        parts = []
        code = fmtEpisode(item.parent_index, item.index)
        if code <> "" then parts.Push(code)
        if showTitle <> ""
            if title <> "" then parts.Push(title)
            title = showTitle
        end if
        context = parts.Join(" · ")
    end if
    meta = []
    parts = []
    year = httpNumber(item.year, 0)
    if year > 0 then parts.Push({ text: urlValue(year) })
    if kind = "show"
        seasons = Int(httpNumber(item.child_count, 0))
        if seasons > 0 then parts.Push({ text: t("browse.counts.seasons", { count: seasons }) })
    else if kind = "season"
        episodes = Int(httpNumber(item.leaf_count, 0))
        if episodes > 0 then parts.Push({ text: t("browse.counts.episodes", { count: episodes }) })
    else
        duration = fmtDuration(item.duration_ms)
        if duration <> "" then parts.Push({ text: duration })
    end if
    rating = httpSentence(item.content_rating)
    if rating <> "" then parts.Push({ text: rating, boxed: true })
    score = httpNumber(item.rating, 0)
    if score > 0 then parts.Push({ text: fmtOneDecimal(score), star: true })
    if complete
        for each badge in homeBadges(item)
            parts.Push({ text: badge, boxed: true })
        end for
    end if
    for each part in parts
        if part.star = true
            meta.Push("★ " + part.text)
        else
            meta.Push(part.text)
        end if
    end for
    state = httpObject(item.user_state)
    position = 0
    watched = false
    if state <> invalid
        position = httpNumber(state.position_ms, 0)
        watched = httpTrue(state.watched)
    end if
    duration = httpNumber(item.duration_ms, 0)
    inProgress = not watched and fmtResumable(position, duration)
    progress = 0.0
    timeLeft = ""
    if inProgress
        progress = fmtProgress(position, duration)
        timeLeft = fmtLeft(position, duration)
    end if
    return {
        itemId: httpNumber(item.id, 0),
        kind: kind,
        title: title,
        context: context,
        meta: meta.Join(" · "),
        metaParts: parts,
        summary: httpClean(item.summary, 600, true),
        progress: progress,
        timeLeft: timeLeft,
        video: contentPlayable(kind),
        watched: watched,
        positionMs: position,
        durationMs: duration,
        backdrop: homeBackdrop(item, server),
        complete: complete
    }
end function

' The quality badges of a title's details: its media summary's where the
' server sends one, else its first file's ("4K", "DV").
function homeBadges(item as object) as object
    return contentItemBadges(item)
end function

' The backdrop behind a title: its own, else its show's or its season's.
function homeBackdrop(item as object, server as string) as string
    width = homeBackdropWidth()
    url = urlImage(server, item.backdrop_url, width)
    if url <> "" then return url
    for each key in ["grandparent_id", "parent_id"]
        id = httpNumber(item[key], 0)
        if id > 0 then return urlImage(server, "/api/items/" + urlValue(id) + "/images/backdrop", width)
    end for
    return ""
end function

' The hero's backdrop fills most of the screen beside the menu: sharper
' than the faint ones behind a title's details, yet small enough for a
' Roku Express's texture memory.
function homeBackdropWidth() as integer
    return 960
end function

' The hero's buttons: { labels, actions }. A movie or an episode plays
' (Resume and From start while in progress); anything else opens. Info
' opens the title's page, and the watched mark toggles.
function homeHeroButtons(info as object) as object
    labels = []
    actions = []
    if info.video
        if info.timeLeft <> ""
            labels = [t("common.actions.resume"), t("browse.home.fromStart")]
            actions = ["resume", "start"]
        else
            labels = [t("common.actions.play")]
            actions = ["start"]
        end if
        labels.Push(t("browse.home.info"))
        actions.Push("info")
    else
        labels = [t("common.actions.open")]
        actions = ["info"]
    end if
    if info.kind = "movie" or info.kind = "episode" or info.kind = "show" or info.kind = "season"
        if info.watched
            labels.Push(t("browse.title.markUnwatched"))
            actions.Push("unwatched")
        else
            labels.Push(t("browse.title.markWatched"))
            actions.Push("watched")
        end if
    end if
    return { labels: labels, actions: actions }
end function

' What Home says with no rows: "No libraries yet" when the profile sees
' none, "Scanning your libraries…" while one scans, else "Nothing here
' yet", as on Android TV. libraries is GET /api/libraries' answer, invalid
' until it comes. { title, text }.
function homeEmpty(libraries as dynamic) as object
    if libraries <> invalid and httpArray(libraries).Count() = 0 then return { title: t("browse.home.noLibraries"), text: t("browse.home.noLibrariesHint") }
    for each library in httpArray(libraries)
        if GetInterface(library, "ifAssociativeArray") <> invalid and httpTrue(library.scanning) then return { title: t("browse.home.scanning"), text: t("browse.home.scanningHint") }
    end for
    return { title: t("browse.home.empty"), text: t("browse.home.emptyHint") }
end function

' Home's menu, in the order of the other TVs' rail: Search, Home, Discover
' and the requests (requests: their entries, from the summary), Live TV
' where the server has it, the libraries, Playlists, Collections where
' there is a library of movies or shows (the page says when none has any),
' Now Playing while music plays, and Settings last. [{ title, action, library, count, danger }].
function homeMenu(libraries as object, live as boolean, requests as object, music as boolean) as object
    entries = [{ title: t("browse.menu.search"), action: "search" }, { title: t("browse.menu.home"), action: "home" }]
    for each entry in requests
        entries.Push(entry)
    end for
    if live then entries.Push({ title: t("browse.menu.liveTv"), action: "live" })
    videos = false
    ' A library's name is the server's, never translated.
    for each library in libraries
        entries.Push({ title: library.name, action: "library", library: library })
        if library.kind = "movies" or library.kind = "shows" then videos = true
    end for
    entries.Push({ title: t("browse.menu.playlists"), action: "playlists" })
    if videos then entries.Push({ title: t("browse.menu.collections"), action: "collections" })
    if music then entries.Push({ title: t("browse.menu.nowPlaying"), action: "nowPlaying" })
    entries.Push({ title: t("browse.menu.settings"), action: "settings" })
    return entries
end function

' The rail's icon of a menu entry (homeMenu), as the Android TV app's
' (RailItems.kt): a library's by its kind.
function homeMenuIcon(entry as object) as string
    icons = {
        search: "search", home: "house", discover: "compass", requests: "list-video", live: "tv-minimal",
        playlists: "list-music", collections: "layers", nowPlaying: "audio-lines", settings: "settings"
    }
    if entry.action = "library"
        kind = ""
        if entry.library <> invalid then kind = httpString(entry.library.kind)
        if kind = "movies" then return "clapperboard"
        if kind = "shows" then return "tv"
        if kind = "music" then return "music"
        return "film"
    end if
    icon = icons[entry.action]
    if icon = invalid then return "film"
    return icon
end function

' The hero's buttons as chips (ChipRow), as Android TV's: the first a pill
' with its icon and words, the rest round icons, named under them while
' focused.
function homeHeroChips(buttons as object) as object
    icons = { resume: "play", start: "rotate-ccw", info: "info", watched: "circle-check", unwatched: "eye-off" }
    chips = []
    for i = 0 to buttons.actions.Count() - 1
        action = buttons.actions[i]
        icon = icons[action]
        if icon = invalid then icon = "info"
        if i = 0
            ' Play from the start when nothing is to resume; Open for what
            ' doesn't play here.
            if action = "start" then icon = "play"
            if action = "info" then icon = "arrow-right"
            chips.Push({ label: buttons.labels[i], icon: icon, width: 0 })
        else
            chips.Push({ label: "", icon: icon, name: buttons.labels[i], width: 0 })
        end if
    end for
    return chips
end function

' What info's MetaRow shows: its parts, after its context ("S2 E5 · The
' Long Night") when withContext; what the tile knew (meta), until its
' details come with their parts.
function homeMetaParts(info as object, withContext as boolean) as object
    parts = []
    if withContext and info.context <> invalid and info.context <> "" then parts.Push({ text: info.context })
    if GetInterface(info.metaParts, "ifArray") <> invalid
        parts.Append(info.metaParts)
    else if info.meta <> invalid and info.meta <> ""
        parts.Push({ text: info.meta })
    end if
    return parts
end function
