' Requests and Discover (docs/API.md "Requests"): what the summary lets
' the person do, TMDB's titles as tiles and pages, a request's stage and
' what can be done with it, and the words of each, those of every pley
' app. Plain fields, which the API task makes into ContentNodes
' (lib/Nodes.brs). Times are unix seconds; zone is fmtZoneSeconds()'s.

' --- The summary (GET /api/requests/summary) ---

' The summary when there is none: requests off. absent: the server has
' none to give (an older one), so it isn't asked again until the event
' stream connects again or the app starts.
function reqNoSummary(absent as boolean) as object
    return {
        known: false,
        absent: absent,
        enabled: false,
        access: "off",
        moviesLeft: -1,
        seasonsLeft: -1,
        moviesLimit: -1,
        seasonsLimit: -1,
        moviesNextAt: 0,
        seasonsNextAt: 0,
        manage: false,
        pending: 0,
        failed: 0,
        unseen: 0,
        own: 0,
        changedAt: 0,
        signInAgain: ""
    }
end function

' The summary as the scene keeps it: numbers without a limit are -1,
' times not set 0 (a node's field drops invalid).
function reqSummary(data as dynamic) as object
    summary = reqNoSummary(false)
    body = httpObject(data)
    if body = invalid then return summary
    summary.known = true
    summary.enabled = httpTrue(body.enabled)
    access = httpString(body.access)
    if access <> "ask" and access <> "auto" then access = "off"
    summary.access = access
    summary.moviesLeft = httpNumber(body.movies_left, -1)
    summary.seasonsLeft = httpNumber(body.seasons_left, -1)
    summary.moviesLimit = httpNumber(body.movies_limit, -1)
    summary.seasonsLimit = httpNumber(body.seasons_limit, -1)
    summary.moviesNextAt = httpNumber(body.movies_next_at, 0)
    summary.seasonsNextAt = httpNumber(body.seasons_next_at, 0)
    summary.manage = httpTrue(body.manage)
    summary.pending = httpNumber(body.pending, 0)
    summary.failed = httpNumber(body.failed, 0)
    summary.unseen = httpNumber(body.unseen, 0)
    summary.own = httpNumber(body.own, 0)
    summary.changedAt = httpNumber(body.changed_at, 0)
    again = httpString(body.sign_in_again)
    if again <> "requests" and again <> "approvals" then again = ""
    summary.signInAgain = again
    return summary
end function

' The summary an answer gives, or invalid to keep the one there was (the
' server didn't answer). An older server answers 404, and a build from
' before apps could ask 403: requests are off.
function reqSummaryAnswer(answer as object) as dynamic
    if answer.ok then return reqSummary(answer.data)
    if answer.status = 404 or answer.status = 403 then return reqNoSummary(true)
    return invalid
end function

' The summary a screen was given (the scene's), or none yet: requests off.
function reqSummaryOf(value as dynamic) as object
    summary = httpObject(value)
    if summary = invalid or not summary.DoesExist("signInAgain") then return reqNoSummary(false)
    return summary
end function

' Whether the menu has Discover: requests are on and the account may ask
' (a sign-in from before finds there how to sign in again).
function reqShowsDiscover(summary as object) as boolean
    return summary.enabled = true and summary.access <> "off"
end function

' Whether the person may ask from this sign-in: Discover's rows and
' titles, Search's Discover row.
function reqMayAsk(summary as object) as boolean
    return reqShowsDiscover(summary) and summary.signInAgain <> "requests"
end function

' Whether the menu has the requests: the account's own (Your requests),
' or everyone's for a manager (Requests).
function reqShowsRequests(summary as object) as boolean
    if summary.enabled <> true or summary.signInAgain = "requests" then return false
    return summary.manage = true or summary.access <> "off" or summary.own > 0
end function

' "Requests" for a manager, else "Your requests".
function reqPageName(summary as object) as string
    if summary.manage = true then return "Requests"
    return "Your requests"
end function

' The number on what opens the requests: { count, danger }. A manager's
' counts what waits for them (pending, failed) and their own news, in
' the danger colour while some failed; anyone else's, their news.
function reqBadge(summary as object) as object
    if summary.manage = true then return { count: summary.pending + summary.failed + summary.unseen, danger: summary.failed > 0 }
    return { count: summary.unseen, danger: false }
end function

' Discover's way to the requests, as its chip (ChipRow) shows it: their
' name and the menu's number, in the accent (red while some failed).
function reqRequestsChip(summary as object) as object
    badge = reqBadge(summary)
    return { label: reqPageName(summary), width: 0, count: reqCountText(badge.count, 99), accent: true, danger: badge.danger }
end function

' Whether a server event is about requests: the summary is asked again.
function reqIsEvent(event as object) as boolean
    kind = httpString(event.type)
    return kind = "requests_changed" or kind = "request_available" or kind = "request_decided" or kind = "request_created" or kind = "request_failed" or kind = "requests_settings_changed"
end function

' --- Words ---

' "1 movie", "3 movies".
function reqCount(n as dynamic, one as string, many as string) as string
    value = Int(httpNumber(n, 0))
    if value = 1 then return "1 " + one
    return urlValue(value) + " " + many
end function

' "Movie", "Show".
function reqKindLabel(kind as string) as string
    if kind = "movie" then return "Movie"
    return "Show"
end function

' “title”, in curly quotes.
function reqQuoted(title as string) as string
    return "“" + title + "”"
end function

' What the account may still ask for in the last 7 days, and its limits:
' { text, limits }, "" for none. allowance has the summary's fields (a
' title page's left and times, with the summary's limits).
function reqAllowance(allowance as object, now as integer) as object
    more = []
    none = []
    if allowance.moviesLeft > 0
        more.Push(reqCount(allowance.moviesLeft, "more movie", "more movies"))
    else if allowance.moviesLeft = 0
        none.Push({ what: "movies", at: allowance.moviesNextAt })
    end if
    if allowance.seasonsLeft > 0
        more.Push(reqCount(allowance.seasonsLeft, "more season", "more seasons"))
    else if allowance.seasonsLeft = 0
        none.Push({ what: "seasons", at: allowance.seasonsNextAt })
    end if
    text = ""
    if more.Count() > 0
        text = "You can ask for " + more.Join(" and ")
        for each used in none
            if used.at > 0
                text = text + ", and for more " + used.what + " " + fmtUntil(used.at, now)
            else
                text = text + ", but no more " + used.what + " for now"
            end if
        end for
        text = text + "."
    else if none.Count() = 1
        text = "You've asked for all the " + none[0].what + " you can for now"
        if none[0].at > 0 then text = text + ": you can ask for more " + fmtUntil(none[0].at, now)
        text = text + "."
    else if none.Count() = 2
        when = []
        for each used in none
            if used.at > 0 then when.Push("more " + used.what + " " + fmtUntil(used.at, now))
        end for
        text = "You've asked for all you can for now"
        if when.Count() > 0 then text = text + ": " + when.Join(", ")
        text = text + "."
    end if
    limits = []
    if allowance.moviesLimit >= 0 then limits.Push(reqCount(allowance.moviesLimit, "movie", "movies"))
    if allowance.seasonsLimit >= 0 then limits.Push(reqCount(allowance.seasonsLimit, "season", "seasons"))
    limitText = ""
    if limits.Count() = 2
        limitText = "Limits: " + limits.Join(" and ") + " per 7 days."
    else if limits.Count() = 1
        limitText = "Limit: " + limits[0] + " per 7 days."
    end if
    return { text: text, limits: limitText }
end function

' What asking from this session does, before a movie is asked for.
function reqAskWords(access as string) as string
    if access = "auto" then return "Approved as you ask, and sent on to download."
    return "An administrator decides on it. You'll hear when it's ready to watch."
end function

' The question before a movie is asked for (a TV's OK is pressed often):
' { title, lines }, what asking does and what may still be asked for.
function reqAskConfirm(title as string, access as string, allowance as object, now as integer) as object
    lines = [reqAskWords(access)]
    words = reqAllowance(allowance, now)
    if words.text <> "" then lines.Push(words.text)
    if words.limits <> "" then lines.Push(words.limits)
    return { title: "Ask for " + reqQuoted(title) + "?", lines: lines }
end function

' "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials";
' "" for none (movies).
function reqSeasonsLabel(seasons as dynamic) as string
    list = httpArray(seasons)
    if list.Count() = 0 then return ""
    numbers = []
    specials = false
    for each value in list
        n = httpNumber(value, -1)
        if n = 0 then specials = true
        if n > 0 then numbers = reqWithNumber(numbers, Int(n))
    end for
    if numbers.Count() = 0 then return "Specials"
    label = "Seasons "
    if numbers.Count() = 1 then label = "Season "
    label = label + reqRanges(numbers)
    if specials then label = label + " and specials"
    return label
end function

' numbers (sorted, each once) with n among them.
function reqWithNumber(numbers as object, n as integer) as object
    out = []
    placed = false
    for each other in numbers
        if other = n then placed = true
        if not placed and other > n
            out.Push(n)
            placed = true
        end if
        out.Push(other)
    end for
    if not placed then out.Push(n)
    return out
end function

' "1–3, 5" for [1, 2, 3, 5] (sorted, each once).
function reqRanges(numbers as object) as string
    ranges = []
    first = numbers[0]
    last = numbers[0]
    for i = 1 to numbers.Count()
        if i < numbers.Count() and numbers[i] = last + 1
            last = numbers[i]
        else
            if first = last
                ranges.Push(urlValue(first))
            else
                ranges.Push(urlValue(first) + "–" + urlValue(last))
            end if
            if i < numbers.Count()
                first = numbers[i]
                last = numbers[i]
            end if
        end if
    end for
    return ranges.Join(", ")
end function

' "1,2" for [2, 1]: season numbers as the notices keep them.
function reqNumbersText(numbers as dynamic) as string
    sorted = []
    for each value in httpArray(numbers)
        n = httpNumber(value, -1)
        if n >= 0 then sorted = reqWithNumber(sorted, Int(n))
    end for
    parts = []
    for each n in sorted
        parts.Push(urlValue(n))
    end for
    return parts.Join(",")
end function

' [1, 2] for "1,2".
function reqNumbersOf(text as string) as object
    out = []
    if text = "" then return out
    for each part in text.Split(",")
        out.Push(part.ToInt())
    end for
    return out
end function

' Why requests can't be used, as a page says it: { title, text, retry,
' signIn }, or invalid for an answer that isn't one of these (the server
' away, a title gone: the screen's usual message). manage: a manager is
' told where to fix TMDB's key.
function reqProblem(answer as object, manage as boolean) as dynamic
    code = answer.error
    if code = "unavailable" then return { title: "Requests are off", text: "This server doesn't take requests for movies and shows.", retry: false, signIn: false }
    if code = "tmdb_key"
        text = "An administrator needs to check it."
        if manage then text = "Fix it in Pley's web app, in Settings → Server."
        return { title: "TMDB refused this server's key", text: text, retry: false, signIn: false }
    end if
    if code = "tmdb_down" then return { title: "TMDB isn't answering", text: "", retry: true, signIn: false }
    if code = "no_access" then return { title: "Requests aren't open to this account", text: "An administrator can let it ask for movies and shows.", retry: false, signIn: false }
    if code = "insufficient_scope" then return reqSignInAgain()
    return invalid
end function

' A sign-in from before apps could ask: Discover shows only this.
function reqSignInAgain() as object
    return { title: "Sign in again to ask for movies and shows", text: "This app signed in before it could ask. Signing in again lets it.", retry: false, signIn: true }
end function

' What asking (POST /api/requests) did: 201 a new request, 200 following
' one asked for already.
function reqAsked(status as integer, data as dynamic, title as string) as string
    quoted = reqQuoted(title)
    if status = 200 then return "You'll hear when " + quoted + " is ready."
    request = httpObject(data)
    state = ""
    if request <> invalid then state = httpString(request.status)
    if state = "approved" or state = "available" then return quoted + " is approved and on its way."
    if state = "failed" then return "Asked for " + quoted + ", but it couldn't be sent to download yet."
    return "Asked for " + quoted + ": an administrator decides."
end function

' Why asking didn't work, in a sentence. The days come from retry_at,
' never from the message. allowance: the summary's limits; kind, what was
' asked for.
function reqAskProblem(answer as object, allowance as object, kind as string, now as integer) as string
    data = httpObject(answer.data)
    retry = 0
    if data <> invalid then retry = httpNumber(data.retry_at, 0)
    if answer.error = "declined"
        if retry > 0 then return "You can ask again " + fmtUntil(retry, now) + "."
        return answer.message
    end if
    if answer.error = "too_many_requests"
        if retry > 0 then return "You can ask for more " + fmtUntil(retry, now) + "."
        limit = allowance.seasonsLimit
        what = reqCount(limit, "season", "seasons")
        if kind = "movie"
            limit = allowance.moviesLimit
            what = reqCount(limit, "movie", "movies")
        end if
        if limit >= 0 then return "You can ask for at most " + what + " per 7 days: pick fewer."
        return answer.message
    end if
    problem = reqProblem(answer, false)
    if problem <> invalid then return problem.title + "."
    return answer.message
end function

' Why an action on a request (reqActions') didn't work, in a sentence.
' When it may be done comes from retry_at, never from the message.
' allowance and kind: Request again's, as reqAskProblem takes them.
function reqActProblem(answer as object, action as string, allowance as object, kind as string, now as integer) as string
    if action = "ask_again" then return reqAskProblem(answer, allowance, kind, now)
    if answer.status = 404 then return "This request was deleted meanwhile."
    data = httpObject(answer.data)
    retry = 0
    if data <> invalid then retry = httpNumber(data.retry_at, 0)
    if answer.error = "conflict" and retry > 0
        if action = "withdraw" then return "A declined request stays for 30 days: you can remove it " + fmtUntil(retry, now) + "."
        return "You can do this " + fmtUntil(retry, now) + "."
    end if
    return answer.message
end function

' --- TMDB's titles (DiscoverTitle) ---

' Discover's rows: [{ key (GET /api/discover/{key}), title }].
function reqLists() as object
    return [
        { key: "trending", title: "Trending this week" },
        { key: "movies", title: "Popular movies" },
        { key: "shows", title: "Popular shows" },
        { key: "upcoming", title: "Coming to cinemas" },
        { key: "anime", title: "Popular anime" }
    ]
end function

' The words searched as TMDB's search takes them (GET
' /api/discover/search): trimmed, and at most 200 characters.
function reqSearchWords(text as string) as string
    return Left(text.Trim(), 200).Trim()
end function

' How many of TMDB's titles Search's Discover row shows; See all has the
' rest.
function reqSearchShown() as integer
    return 20
end function

' Whether Search's Discover row reads TMDB's next page: fewer than
' reqSearchShown() titles the libraries lack came (count) and pages are
' left, three at most (read). A page can come short, even empty, its
' titles in a library.
function reqSearchWantsPage(count as integer, page as integer, totalPages as integer, read as integer) as boolean
    return count < reqSearchShown() and page < totalPages and read < 3
end function

' What a See all grid of TMDB's titles shows: one of Discover's lists (a
' key of reqLists), or TMDB's results for query, the words searched.
' { title, path (asked a page at a time), empty (said when it has none) }.
function reqSeeAll(list as string, query as string) as object
    words = reqSearchWords(query)
    if words <> "" then return { title: "Results for " + reqQuoted(words), path: urlWithQuery("/api/discover/search", { q: words }), empty: "Nothing found for " + reqQuoted(words) + "." }
    title = "Discover"
    for each known in reqLists()
        if known.key = list then title = known.title
    end for
    return { title: title, path: "/api/discover/" + list, empty: "Nothing here right now." }
end function

' A title's badge on its poster: "In library", "Partly in library" (where
' the account can open it, else what was asked for: "Requested"),
' "Requested", "Declined"; "" for none.
function reqTitleBadge(status as string, itemId as dynamic) as string
    if status = "library" then return "In library"
    if status = "partial"
        if httpNumber(itemId, 0) > 0 then return "Partly in library"
        return "Requested"
    end if
    if status = "requested" then return "Requested"
    if status = "declined" then return "Declined"
    return ""
end function

' The badge's colour: green in a library, the accent asked for, red
' declined.
function reqBadgeColor(status as string, itemId as dynamic) as string
    if status = "library" or (status = "partial" and httpNumber(itemId, 0) > 0) then return "0x3FB27FFF"
    if status = "declined" then return "0xEB6B70FF"
    return "0x38BDF8FF"
end function

function reqKey(kind as string, tmdbId as dynamic) as string
    return kind + ":" + urlValue(tmdbId)
end function

' A title as a tile: its poster, its name, year and kind, and its badge.
' It opens the title's page (Nav.brs); key ("movie:603") finds it again.
function reqTitleTile(title as object, server as string) as object
    kind = httpString(title.kind)
    tmdbId = httpNumber(title.tmdb_id, 0)
    status = httpString(title.status)
    libraryId = httpNumber(title.item_id, 0)
    parts = []
    year = httpNumber(title.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    parts.Push(reqKindLabel(kind))
    return {
        title: httpSentence(title.title),
        subtitle: parts.Join(" · "),
        image: urlImage(server, title.poster_url, contentPosterWidth()),
        wide: false,
        progress: 0.0,
        watched: false,
        itemId: 0,
        kind: "discover",
        tmdbKind: kind,
        tmdbId: tmdbId,
        status: status,
        libraryId: libraryId,
        badge: reqTitleBadge(status, libraryId),
        badgeColor: reqBadgeColor(status, libraryId),
        key: reqKey(kind, tmdbId)
    }
end function

' The tiles of a page of titles (a list, search), those in seen ({ key:
' true }, shown already) left out: TMDB repeats titles on later pages.
' seen gets the new ones.
function reqTitleTiles(results as dynamic, server as string, seen as object) as object
    tiles = []
    for each title in httpArray(results)
        if GetInterface(title, "ifAssociativeArray") <> invalid
            kind = httpString(title.kind)
            tmdbId = httpNumber(title.tmdb_id, 0)
            key = reqKey(kind, tmdbId)
            if (kind = "movie" or kind = "show") and tmdbId > 0 and not seen.DoesExist(key)
                seen[key] = true
                tiles.Push(reqTitleTile(title, server))
            end if
        end if
    end for
    return tiles
end function

' The tile at the end of a row that opens all of it: a list's grid, or
' TMDB's results for query.
function reqMoreTile(title as string, list as string, query as string) as object
    return { title: title, subtitle: "", image: "", wide: false, progress: 0.0, watched: false, itemId: 0, kind: "more", list: list, query: query, badge: "", badgeColor: "0x38BDF8FF", key: "more" }
end function

' A row's tiles while it loads: empty frames, as many as a row shows.
function reqSkeletonTiles() as object
    tiles = []
    for i = 1 to 7
        tiles.Push({ title: "", subtitle: "", image: "", wide: false, progress: 0.0, watched: false, itemId: 0, kind: "skeleton", badge: "", badgeColor: "0x38BDF8FF", key: "skeleton" })
    end for
    return tiles
end function

' Request more seasons, at the end of a library's show's seasons: it
' opens the show's page on TMDB with its season picker.
function reqAskMoreTile(tmdbKind as string, tmdbId as dynamic, title as string) as object
    return { title: "Request more seasons", subtitle: "", image: "", wide: false, progress: 0.0, watched: false, itemId: 0, kind: "askMore", tmdbKind: tmdbKind, tmdbId: Int(httpNumber(tmdbId, 0)), showTitle: title, badge: "", badgeColor: "0x38BDF8FF", key: "askMore" }
end function

' A row's one tile instead of titles, its words in its frame: kind
' "retry" loads the row again ("Couldn't load these…"), "note" only says
' so ("Nothing here right now.").
function reqNoticeTile(text as string, kind as string) as object
    return { title: text, subtitle: "", image: "", wide: false, progress: 0.0, watched: false, itemId: 0, kind: kind, badge: "", badgeColor: "0x38BDF8FF", key: kind }
end function

' Search's Discover row when TMDB's search failed (answer): a tile saying what
' stops it, which asks again where that may help ("TMDB isn't answering.
' Select to try again.") and else only says so ("TMDB refused this
' server's key.").
function reqSearchProblemTile(answer as object, manage as boolean) as object
    return reqProblemTile(answer, manage, "Couldn't load these.")
end function

' The tile that ends a grid of TMDB's titles whose next page failed
' (answer): Select asks for it again ("Couldn't load more. Select to try
' again."), unless trying again can't help ("TMDB refused this server's
' key.").
function reqMoreProblemTile(answer as object, manage as boolean) as object
    return reqProblemTile(answer, manage, "Couldn't load more.")
end function

' What stops TMDB's titles (answer) as a tile, which asks again where that
' may help; failed says any other failure.
function reqProblemTile(answer as object, manage as boolean, failed as string) as object
    problem = reqProblem(answer, manage)
    if problem = invalid then return reqNoticeTile(failed + " Select to try again.", "retry")
    if problem.retry then return reqNoticeTile(problem.title + ". Select to try again.", "retry")
    return reqNoticeTile(problem.title + ".", "note")
end function

' Whether a grid of TMDB's titles (a list's or a search's See all) asks
' for its next page: one is left and fewer than ahead titles lie past the
' focus, five pages at most (asked) until the focus moves. An age limit
' can leave hundreds of pages near empty.
function reqWantsPage(page as integer, totalPages as integer, focused as integer, count as integer, ahead as integer, asked as integer) as boolean
    if totalPages >= 0 and page >= totalPages then return false
    return asked < 5 and focused >= count - ahead
end function

' The tile that ends such a grid once five pages came with more left: it
' asks for the next ones.
function reqFurtherTile() as object
    return reqNoticeTile("More may follow. Select to look further.", "further")
end function

' A title's badge as its page answered (GET /api/discover/{kind}/
' {tmdb_id}): { key, status, libraryId } for the screens showing its
' tile, or invalid when it didn't answer (the badge stays as it is).
function reqTitleNote(key as string, answer as object) as dynamic
    detail = invalid
    if answer.ok then detail = httpObject(answer.data)
    if detail = invalid then return invalid
    return { key: key, status: httpString(detail.status), libraryId: httpNumber(detail.item_id, 0) }
end function

' Which title Discover's hero is: the first of the first of TMDB's lists
' with any (the lists leave out what the profile may not ask for), once
' those before it came with none or failed. lists: Discover's rows ({ key,
' state ("shown" once it came), first (its first title, or invalid) }),
' Your requests left out. invalid while it waits for a list; {} when none
' has a title.
function reqHeroOf(lists as object) as dynamic
    for each list in lists
        if list.key <> "requests"
            if list.first <> invalid then return list.first
            if list.state <> "shown" then return invalid
        end if
    end for
    return {}
end function

' Discover's hero (reqHeroOf's title): { title, line,
' overview, backdrop, buttons, actions, note, tmdbKind, tmdbId,
' libraryId }. Its main action, as far as a list's title tells it (its
' page has the rest): actions "open" (the library's title), "request"
' (Request, or Request more seasons for a show partly there: the title's
' page, asking at once), "details". With nothing of its kind left to ask
' for, Request goes and note says so (Roku's buttons can't be disabled).
function reqHero(title as object, summary as object, server as string, now as integer) as object
    tile = reqTitleTile(title, server)
    parts = [tile.subtitle]
    rating = httpNumber(title.rating, 0)
    if rating > 0 then parts.Push("★ " + fmtOneDecimal(rating))
    if tile.badge <> "" then parts.Push(tile.badge)
    remaining = summary.moviesLeft
    if tile.tmdbKind = "show" then remaining = summary.seasonsLeft
    buttons = []
    actions = []
    note = ""
    if tile.libraryId > 0 and (tile.status = "library" or tile.status = "partial")
        buttons.Push("Open")
        actions.Push("open")
    end if
    if reqMayAsk(summary) and (tile.status = "none" or tile.status = "partial")
        if remaining = 0
            note = reqAllowance(summary, now).text
        else if tile.status = "none"
            buttons.Push("Request")
            actions.Push("request")
        else
            buttons.Push("Request more seasons")
            actions.Push("request")
        end if
    end if
    buttons.Push("Details")
    actions.Push("details")
    return {
        title: tile.title,
        line: parts.Join(" · "),
        note: note,
        overview: httpClean(title.overview, 400, false),
        backdrop: urlImage(server, title.backdrop_url, contentBackdropWidth()),
        buttons: buttons,
        actions: actions,
        tmdbKind: tile.tmdbKind,
        tmdbId: tile.tmdbId,
        libraryId: tile.libraryId
    }
end function

' --- A title's page (DiscoverDetail) ---

' Whether a request is active: pending, approved (whatever its stage) or
' failed.
function reqActive(request as object) as boolean
    status = httpString(request.status)
    return status = "pending" or status = "approved" or status = "failed"
end function

' The request whose stage the page shows: the one the title names
' (request_id), else the newest active one listed; invalid for none.
function reqShownRequest(detail as object) as dynamic
    wanted = httpNumber(detail.request_id, 0)
    newest = invalid
    for each request in httpArray(detail.requests)
        if GetInterface(request, "ifAssociativeArray") <> invalid
            if wanted > 0 and httpNumber(request.id, 0) = wanted then return request
            if newest = invalid and reqActive(request) then newest = request
        end if
    end for
    return newest
end function

' Whether the account made one of the title's active requests.
function reqHasMine(detail as object) as boolean
    for each request in httpArray(detail.requests)
        if GetInterface(request, "ifAssociativeArray") <> invalid and httpTrue(request.mine) and reqActive(request) then return true
    end for
    return false
end function

' The title's requests by id ({ "42": MediaRequest }).
function reqRequestsById(requests as dynamic) as object
    out = {}
    for each request in httpArray(requests)
        if GetInterface(request, "ifAssociativeArray") <> invalid then out[urlValue(httpNumber(request.id, 0))] = request
    end for
    return out
end function

' What the page knows of what may be asked for: the title's left and
' times, the summary's limits.
function reqTitleAllowance(detail as object, summary as object) as object
    return {
        moviesLeft: httpNumber(detail.movies_left, -1),
        seasonsLeft: httpNumber(detail.seasons_left, -1),
        moviesNextAt: httpNumber(detail.movies_next_at, 0),
        seasonsNextAt: httpNumber(detail.seasons_next_at, 0),
        moviesLimit: summary.moviesLimit,
        seasonsLimit: summary.seasonsLimit
    }
end function

' "2008 · 5 seasons · TV-MA · ★ 8.9", "2010 · 2 h 28 min · PG-13 · ★ 8.4".
function reqDetailLine(detail as object) as string
    parts = []
    year = httpNumber(detail.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    if httpString(detail.kind) = "show"
        count = 0
        for each season in httpArray(detail.seasons)
            if GetInterface(season, "ifAssociativeArray") <> invalid and httpNumber(season.number, 0) > 0 then count = count + 1
        end for
        if count > 0 then parts.Push(reqCount(count, "season", "seasons"))
    else
        minutes = httpNumber(detail.runtime_minutes, 0)
        if minutes > 0 then parts.Push(fmtDuration(minutes * 60000))
    end if
    certification = httpSentence(detail.certification)
    if certification <> "" then parts.Push(certification)
    rating = httpNumber(detail.rating, 0)
    if rating > 0 then parts.Push("★ " + fmtOneDecimal(rating))
    return parts.Join(" · ")
end function

' A declined title's line: "Declined: “Not in 4K”. You can ask again in
' 12 days."; without a reason, "Declined. You can ask again …".
function reqDeclinedText(declined as dynamic, now as integer) as string
    info = httpObject(declined)
    if info = invalid then return "Declined."
    text = "Declined"
    reason = httpSentence(info.reason)
    if reason <> "" then text = text + ": " + reqQuoted(reason)
    text = text + "."
    again = httpNumber(info.again_at, 0)
    if again > now then text = text + " You can ask again " + fmtUntil(again, now) + "."
    return text
end function

' Whether a season can be asked for: missing, partly in a library or not
' aired yet, not declined, the title not restricted.
function reqSeasonAskable(season as object, restricted as boolean) as boolean
    state = httpString(season.state)
    return not restricted and not httpTrue(season.declined) and (state = "missing" or state = "partial" or state = "unaired")
end function

' Whether "Request more seasons" shows: a season other than specials can
' be asked for and has aired, in a library in part or not at all.
function reqCanAskMore(seasons as dynamic, restricted as boolean) as boolean
    for each season in httpArray(seasons)
        if GetInterface(season, "ifAssociativeArray") <> invalid and httpNumber(season.number, 0) > 0 and reqSeasonAskable(season, restricted)
            state = httpString(season.state)
            if state = "missing" or state = "partial" then return true
        end if
    end for
    return false
end function

' Whether any season can be asked for: a show's Request.
function reqCanAskAny(seasons as dynamic, restricted as boolean) as boolean
    for each season in httpArray(seasons)
        if GetInterface(season, "ifAssociativeArray") <> invalid and reqSeasonAskable(season, restricted) then return true
    end for
    return false
end function

' A show's seasons, numbered ones in order and specials last.
function reqSeasonsInOrder(seasons as dynamic) as object
    numbered = []
    specials = []
    for each season in httpArray(seasons)
        if GetInterface(season, "ifAssociativeArray") <> invalid
            if httpNumber(season.number, 0) = 0
                specials.Push(season)
            else
                numbered.Push(season)
            end if
        end if
    end for
    numbered.Append(specials)
    return numbered
end function

' TMDB's name of a season, else "Season 2" or "Specials".
function reqSeasonName(season as object) as string
    name = httpSentence(season.name)
    if name <> "" then return name
    number = httpNumber(season.number, 0)
    if number = 0 then return "Specials"
    return "Season " + urlValue(number)
end function

' How a season stands for the account, from its state (never from its
' counts): { text, badge, color, progress }. byId: the title's requests
' (reqRequestsById); withStage: a season asked for says its request's
' stage, else "Requested".
function reqSeasonState(season as object, byId as object, withStage as boolean, manage as boolean, now as integer, zone as integer) as object
    state = httpString(season.state)
    out = { text: "", badge: "", color: "0x38BDF8FF", progress: 0.0 }
    if state = "requested"
        out.text = "Requested"
        out.badge = "Requested"
        key = urlValue(httpNumber(season.request_id, 0))
        if withStage and byId.DoesExist(key)
            stage = reqStage(byId[key], manage, now, zone)
            out.text = stage.text
            if stage.progress > 0 then out.progress = stage.progress
        end if
    else if state = "library"
        out.text = "In library"
        out.badge = "In library"
        out.color = "0x3FB27FFF"
    else if httpTrue(season.declined)
        out.text = "Declined"
        out.badge = "Declined"
        out.color = "0xEB6B70FF"
    else if state = "unaired"
        out.text = "Not aired yet"
        day = fmtDateText(season.air_date, now, zone)
        if day <> "" then out.text = "Airs " + day
    else if state = "partial"
        out.text = urlValue(httpNumber(season.in_library, 0)) + " of " + urlValue(httpNumber(season.aired, 0)) + " episodes in library"
    else
        episodes = httpNumber(season.episodes, 0)
        if episodes > 0 then out.text = reqCount(episodes, "episode", "episodes")
    end if
    return out
end function

' A show's seasons as poster tiles, specials last: their name, how each
' stands, a badge for one in a library, asked for or declined. A season
' without a poster shows the show's.
function reqSeasonTiles(detail as object, server as string, manage as boolean, now as integer, zone as integer) as object
    byId = reqRequestsById(detail.requests)
    tiles = []
    for each season in reqSeasonsInOrder(detail.seasons)
        state = reqSeasonState(season, byId, true, manage, now, zone)
        image = urlImage(server, season.poster_url, contentPosterWidth())
        if image = "" then image = urlImage(server, detail.poster_url, contentPosterWidth())
        tiles.Push({
            title: reqSeasonName(season),
            subtitle: state.text,
            image: image,
            wide: false,
            progress: state.progress,
            watched: false,
            itemId: 0,
            kind: "discoverSeason",
            number: httpNumber(season.number, 0),
            badge: state.badge,
            badgeColor: state.color,
            key: "season:" + urlValue(httpNumber(season.number, 0))
        })
    end for
    return tiles
end function

' The cast as tiles: their photo, their name and the part they played.
' TMDB's people aren't the libraries': they open nothing.
function reqCastTiles(detail as object, server as string) as object
    tiles = []
    for each member in httpArray(detail.cast)
        if GetInterface(member, "ifAssociativeArray") <> invalid and httpSentence(member.name) <> ""
            tiles.Push({ title: httpSentence(member.name), subtitle: httpSentence(member.character), image: urlImage(server, member.photo_url, contentPosterWidth()), wide: false, progress: 0.0, watched: false, itemId: 0, kind: "cast", badge: "", badgeColor: "0x38BDF8FF", key: "cast" })
        end if
    end for
    return tiles
end function

' The rows under a title's page: [{ key, title, tiles }], its seasons
' then its cast, rows without tiles left out.
function reqDetailRows(detail as object, server as string, manage as boolean, now as integer, zone as integer) as object
    rows = []
    seasons = reqSeasonTiles(detail, server, manage, now, zone)
    if seasons.Count() > 0 then rows.Push({ key: "seasons", title: "Seasons", tiles: seasons })
    cast = reqCastTiles(detail, server)
    if cast.Count() > 0 then rows.Push({ key: "cast", title: "Cast", tiles: cast })
    return rows
end function

' A title's page as the screen shows it: what it is; its one main action
' (the first that applies) and the actions of its request as buttons;
' the line shown where a button can't be pressed (Roku's buttons can't be
' disabled: the button goes, its line shows); and the stage of the
' request it shows. actions match buttons: "signIn", "open" (the
' library's title), "request" (a movie, after a confirm), "pick" (a
' show's seasons), "follow" (Tell me too), and those of reqActions.
function reqTitlePage(detail as object, summary as object, server as string, now as integer, zone as integer) as object
    kind = httpString(detail.kind)
    status = httpString(detail.status)
    itemId = httpNumber(detail.item_id, 0)
    manage = summary.manage = true
    shown = reqShownRequest(detail)
    allowance = reqTitleAllowance(detail, summary)
    remaining = allowance.moviesLeft
    if kind = "show" then remaining = allowance.seasonsLeft
    ' Asking needs access, and something of this kind left to ask for.
    blocked = ""
    if summary.access = "off"
        blocked = "Requests aren't open to this account."
    else if remaining = 0
        blocked = reqAllowance(allowance, now).text
    end if
    buttons = []
    actions = []
    note = ""
    if summary.signInAgain = "requests"
        again = reqSignInAgain()
        note = again.title + "." + Chr(10) + again.text
        buttons.Push("Sign in again")
        actions.Push("signIn")
    else if status = "restricted"
        note = httpSentence(detail.restriction)
    else if status = "library"
        if itemId > 0
            buttons.Push("Open")
            actions.Push("open")
        end if
    else if status = "declined"
        note = reqDeclinedText(detail.declined, now)
    else if status = "requested"
        if reqHasMine(detail)
            note = "Requested"
        else if httpTrue(detail.watching)
            note = "Following"
        else if summary.access = "off"
            note = blocked
        else
            buttons.Push("Tell me too")
            actions.Push("follow")
            note = "Someone asked for it already."
        end if
    else if status = "partial"
        if itemId > 0
            buttons.Push("Open")
            actions.Push("open")
        end if
        if reqCanAskMore(detail.seasons, false)
            if blocked <> ""
                note = blocked
            else
                buttons.Push("Request more seasons")
                actions.Push("pick")
            end if
        end if
    else if kind = "movie" or reqCanAskAny(detail.seasons, false)
        if blocked <> ""
            note = blocked
        else
            buttons.Push("Request")
            if kind = "show"
                actions.Push("pick")
            else
                actions.Push("request")
            end if
        end if
    end if
    stage = invalid
    requestId = 0
    if shown <> invalid and summary.signInAgain <> "requests"
        stage = reqStage(shown, manage, now, zone)
        requestId = httpNumber(shown.id, 0)
        for each option in reqActions(shown, 0)
            if option.action <> "delete" and option.action <> "watch" and option.action <> "ask_again"
                buttons.Push(option.label)
                actions.Push(option.action)
            end if
        end for
    end if
    genres = []
    for each genre in httpArray(detail.genres)
        name = httpSentence(genre)
        if name <> "" and genres.Count() < 3 then genres.Push(name)
    end for
    title = httpSentence(detail.title)
    return {
        kind: kind,
        tmdbId: httpNumber(detail.tmdb_id, 0),
        title: title,
        line: reqDetailLine(detail),
        genres: genres.Join(", "),
        tagline: httpSentence(detail.tagline),
        overview: httpClean(detail.overview, 1200, true),
        poster: urlImage(server, detail.poster_url, contentDetailPosterWidth()),
        backdrop: urlImage(server, detail.backdrop_url, contentBackdropWidth()),
        itemId: itemId,
        buttons: buttons,
        actions: actions,
        note: note,
        stage: stage,
        requestId: requestId,
        requestStatus: reqShownStatus(shown),
        busy: reqDetailBusy(detail),
        allowance: allowance,
        remaining: remaining,
        access: summary.access
    }
end function

' Whether one of a title's requests downloads or goes into a library now:
' its page is asked again every 10 s meanwhile.
function reqDetailBusy(detail as object) as boolean
    for each request in httpArray(detail.requests)
        if GetInterface(request, "ifAssociativeArray") <> invalid and reqBusy(httpString(request.stage)) then return true
    end for
    return false
end function

function reqShownStatus(request as dynamic) as string
    if request = invalid then return ""
    return httpString(request.status)
end function

' The season picker: every season, specials last, each { number, title,
' line, askable, ticked }. The askable missing and partial seasons other
' than specials are ticked, lowest first, up to remaining of them (-1: no
' limit); unaired ones and specials are offered unticked; the others say
' how they stand ("In library", "Requested", "Declined") and can't be
' ticked.
function reqPickerRows(detail as object, remaining as integer, now as integer, zone as integer) as object
    restricted = httpString(detail.status) = "restricted"
    rows = []
    picked = 0
    for each season in reqSeasonsInOrder(detail.seasons)
        number = httpNumber(season.number, 0)
        state = httpString(season.state)
        askable = reqSeasonAskable(season, restricted)
        ticked = false
        if askable and number > 0 and (state = "missing" or state = "partial") and (remaining < 0 or picked < remaining)
            ticked = true
            picked = picked + 1
        end if
        line = reqSeasonState(season, {}, false, false, now, zone).text
        rows.Push({ number: number, title: reqSeasonName(season), line: line, askable: askable, ticked: ticked })
    end for
    return rows
end function

' The picker's button: { label ("Request 3 seasons", "Request 1
' season"), line shown instead of it }: "Pick a season." with none
' ticked, the allowance with more ticked than may be asked for.
function reqPickerButton(ticked as integer, remaining as integer, allowance as string) as object
    if ticked = 0 then return { label: "", line: "Pick a season." }
    if remaining >= 0 and ticked > remaining
        line = allowance
        if line = "" then line = "You can ask for " + reqCount(remaining, "more season", "more seasons") + "."
        return { label: "", line: line }
    end if
    return { label: "Request " + reqCount(ticked, "season", "seasons"), line: "" }
end function

' What a library's title page shows of requests (GET /api/items/{id}/
' request): { stage (invalid for none), askMore ([Request more seasons]),
' kind, tmdbId }.
function reqItemRequest(data as dynamic, summary as object, now as integer, zone as integer) as object
    out = { stage: invalid, askMore: false, kind: "", tmdbId: 0 }
    body = httpObject(data)
    if body = invalid then return out
    out.kind = httpString(body.kind)
    out.tmdbId = httpNumber(body.tmdb_id, 0)
    request = httpObject(body.request)
    if request <> invalid then out.stage = reqStage(request, summary.manage = true, now, zone)
    out.askMore = httpArray(body.askable).Count() > 0 and reqMayAsk(summary)
    return out
end function

' --- Requests (MediaRequest) ---

' A request's stage as its pill says it: { text, tone, line, progress }.
' line says more where the pill can't (why it isn't sent, its next try);
' progress (0-1) is its download's, -1 without one. manage: a manager
' sees why a request failed.
function reqStage(request as object, manage as boolean, now as integer, zone as integer) as object
    stage = httpString(request.stage)
    if stage = "" then stage = httpString(request.status)
    out = { text: "", tone: "", line: "", progress: -1.0 }
    download = httpObject(request.download)
    if download <> invalid then out.progress = reqProgress(download.progress)
    if stage = "pending"
        out.text = "Waiting for approval"
        out.tone = "pending"
    else if stage = "approved"
        out.text = "Approved"
        out.tone = "active"
    else if stage = "waiting"
        out.text = reqWaitingText(request, now, zone)
        out.tone = "active"
    else if stage = "searching"
        out.text = "Searching"
        out.tone = "active"
    else if stage = "downloading"
        out.text = "Downloading"
        out.tone = "active"
        if download <> invalid
            out.text = reqDownloadText(download, now)
            if httpString(download.state) = "warning" then out.tone = "problem"
        end if
    else if stage = "importing"
        out.text = "Adding to the library"
        out.tone = "active"
    else if stage = "partial"
        out.text = "Partly in library"
        label = reqSeasonsLabel(request.available_seasons)
        if label <> "" then out.text = label + " ready"
        out.tone = "ready"
    else if stage = "available"
        out.text = "Ready to watch"
        out.tone = "ready"
    else if stage = "gone"
        out.text = "No longer in a library"
        out.tone = "gone"
    else if stage = "declined"
        out.text = "Declined"
        out.tone = "problem"
    else if stage = "failed"
        out.tone = "problem"
        reason = httpSentence(request.reason)
        if manage
            out.text = "Couldn't send"
            if reason <> "" then out.text = out.text + ": " + reason
        else
            out.text = "Not sent yet"
            out.line = "It couldn't be sent to download yet: an administrator will look at it."
        end if
        retry = httpNumber(request.retry_at, 0)
        if retry > 0 then out.line = "Trying again " + fmtUntil(retry, now) + "."
    end if
    return out
end function

' A waiting request's day: "Out Nov 4" (a movie), "Airs Nov 4" (a show's
' next episode; within a day, its time: "Airs 9:05 PM", or "Airs tomorrow
' at 9:05 PM" past midnight), "Not out yet" when not known. Radarr knows a
' release by its day only: a movie's is never a time.
function reqWaitingText(request as object, now as integer, zone as integer) as string
    at = httpNumber(request.stage_at, 0)
    if at <= 0 then return "Not out yet"
    if httpString(request.kind) = "movie" then return "Out " + fmtDay(at, now, zone)
    if at > now and at - now < 86400
        clock = fmtTimeOfDay(at, zone)
        if fmtMonthDay(at, zone, true) = fmtMonthDay(now, zone, true) then return "Airs " + clock
        return "Airs tomorrow at " + clock
    end if
    return "Airs " + fmtDay(at, now, zone)
end function

' What a download does: "Downloading 42% · in 20 min", "Waiting to
' download", "Download paused", "The download has a problem", "Adding to
' the library".
function reqDownloadText(download as object, now as integer) as string
    state = httpString(download.state)
    if state = "queued" then return "Waiting to download"
    if state = "paused" then return "Download paused"
    if state = "warning" then return "The download has a problem"
    if state = "importing" then return "Adding to the library"
    text = "Downloading " + urlValue(Int(reqProgress(download.progress) * 100 + 0.5)) + "%"
    eta = httpNumber(download.eta, 0)
    if eta > now then text = text + " · " + fmtUntil(eta, now)
    return text
end function

function reqProgress(value as dynamic) as float
    progress = httpNumber(value, 0)
    if progress < 0 then return 0
    if progress > 1 then return 1
    return progress
end function

' A stage's colour: pending amber; approved, waiting, searching,
' downloading and importing blue; partial and available green; declined
' and failed red; gone grey.
function reqToneColor(tone as string) as string
    colors = { pending: "0xE8B04BFF", active: "0x38BDF8FF", ready: "0x3FB27FFF", problem: "0xEB6B70FF", gone: "0x8B95A5FF" }
    if colors.DoesExist(tone) then return colors[tone]
    return "0xA1A9B6FF"
end function

' Who asked, and when: "Asked for by Ann · 3 days ago", "Asked for by you
' · now", "Asked for by someone else · yesterday".
function reqWho(request as object, now as integer, zone as integer) as string
    who = "Asked for by someone else"
    by = httpObject(request.requested_by)
    if httpTrue(request.mine)
        who = "Asked for by you"
    else if by <> invalid and httpSentence(by.name) <> ""
        who = "Asked for by " + httpSentence(by.name)
    end if
    return who + " · " + fmtAgo(request.created_at, now, zone)
end function

' A request as a row of the requests (RequestRow): its poster, title,
' "2008 · Show · Seasons 1–2", its stage's pill and progress, who asked
' and when, and a line of what to know (why it isn't sent, the reason it
' was declined, a manager's note); with what the screen needs to act on
' it.
function reqRow(request as object, server as string, manage as boolean, now as integer, zone as integer) as object
    kind = httpString(request.kind)
    stage = reqStage(request, manage, now, zone)
    parts = []
    year = httpNumber(request.year, 0)
    if year > 0 then parts.Push(urlValue(year))
    parts.Push(reqKindLabel(kind))
    seasons = reqSeasonsLabel(request.seasons)
    if seasons <> "" then parts.Push(seasons)
    if httpTrue(request.watching) and not httpTrue(request.mine) then parts.Push("Following")
    lines = []
    if stage.line <> "" then lines.Push(stage.line)
    status = httpString(request.status)
    reason = httpSentence(request.reason)
    if status = "declined" and reason <> "" then lines.Push(reqQuoted(reason))
    libraryId = httpNumber(request.item_id, 0)
    if libraryId = 0 and (stage.tone = "ready") then lines.Push("Ready, in a library you can't open: ask an administrator")
    note = httpSentence(request.note)
    if note <> "" then lines.Push(note)
    return {
        title: httpSentence(request.title),
        subtitle: parts.Join(" · "),
        image: urlImage(server, request.poster_url, reqRowPosterWidth()),
        stageText: stage.text,
        stageColor: reqToneColor(stage.tone),
        progress: stage.progress,
        who: reqWho(request, now, zone),
        line: lines.Join(" · "),
        unseen: httpTrue(request.unseen),
        requestId: httpNumber(request.id, 0),
        tmdbKind: kind,
        tmdbId: httpNumber(request.tmdb_id, 0),
        libraryId: libraryId,
        status: status,
        stage: httpString(request.stage),
        actions: reqActionNames(request.actions),
        seasons: reqSeasonNumbers(request.seasons),
        mine: httpTrue(request.mine)
    }
end function

' A row's poster, 120 pixels wide.
function reqRowPosterWidth() as integer
    return 120
end function

' Whether a request downloads or goes into a library now: its list and
' page are asked again every 10 s meanwhile.
function reqBusy(stage as string) as boolean
    return stage = "downloading" or stage = "importing"
end function

' How soon an open list of requests (the requests, Discover's Your
' requests) is asked again, in seconds, 0 for not: in 10 while one of them
' downloads or goes into a library (download progress sends no events),
' in 30 while the event stream is down (live false), when nothing tells
' of changes.
function reqListPoll(busy as boolean, live as boolean) as integer
    if busy then return 10
    if not live then return 30
    return 0
end function

function reqActionNames(actions as dynamic) as object
    out = []
    for each action in httpArray(actions)
        if GetInterface(action, "ifString") <> invalid then out.Push(action)
    end for
    return out
end function

function reqSeasonNumbers(seasons as dynamic) as object
    out = []
    for each value in httpArray(seasons)
        n = httpNumber(value, -1)
        if n >= 0 then out.Push(Int(n))
    end for
    return out
end function

' A request as a tile (Discover's Your requests): its poster, its title
' and its stage in the stage's colour, the download's progress. It opens
' the title's page.
function reqRequestTile(request as object, server as string, manage as boolean, now as integer, zone as integer) as object
    kind = httpString(request.kind)
    tmdbId = httpNumber(request.tmdb_id, 0)
    stage = reqStage(request, manage, now, zone)
    progress = 0.0
    if stage.progress > 0 then progress = stage.progress
    return {
        title: httpSentence(request.title),
        subtitle: stage.text,
        subtitleColor: reqToneColor(stage.tone),
        image: urlImage(server, request.poster_url, contentPosterWidth()),
        wide: false,
        progress: progress,
        watched: false,
        itemId: 0,
        kind: "discover",
        tmdbKind: kind,
        tmdbId: tmdbId,
        status: "requested",
        libraryId: httpNumber(request.item_id, 0),
        badge: "",
        badgeColor: reqToneColor(stage.tone),
        key: "request:" + urlValue(httpNumber(request.id, 0))
    }
end function

' The people the managers' list can be narrowed to (the answer's people):
' [{ id, name }], sorted as the server sends them.
function reqPeople(people as dynamic) as object
    out = []
    for each person in httpArray(people)
        if GetInterface(person, "ifAssociativeArray") <> invalid
            id = httpNumber(person.id, 0)
            name = httpSentence(person.name)
            if id > 0 and name <> "" then out.Push({ id: id, name: name })
        end if
    end for
    return out
end function

' What can be done with a request, as the server lists it (its order),
' for a dialog: [{ label, action }]. Watch comes first once it is ready
' in a library the account can open; Decline with a reason… follows
' Decline. request: a MediaRequest, or a row (reqRow). itemId: the
' library's title, 0 for none.
function reqActions(request as object, itemId as dynamic) as object
    options = []
    status = httpString(request.status)
    stage = httpString(request.stage)
    if httpNumber(itemId, 0) > 0 and (stage = "available" or stage = "partial") then options.Push({ label: "Watch", action: "watch" })
    labels = { withdraw: "Withdraw", unwatch: "Stop following", approve: "Approve", retry: "Try again", decline: "Decline", handled: "Mark as added by hand", delete: "Delete", ask_again: "Request again" }
    for each action in httpArray(request.actions)
        if GetInterface(action, "ifString") <> invalid and labels.DoesExist(action)
            label = labels[action]
            if action = "withdraw" and status = "declined" then label = "Remove"
            options.Push({ label: label, action: action })
            if action = "decline" then options.Push({ label: "Decline with a reason…", action: "declineReason" })
        end if
    end for
    return options
end function

' The question an action asks first: { message, button }, or invalid for
' none.
function reqConfirm(action as string, title as string, status as string) as dynamic
    if action = "withdraw"
        if status = "declined" then return { message: "Remove " + reqQuoted(title) + " from your requests?", button: "Remove" }
        return { message: "Withdraw your request for " + reqQuoted(title) + "?", button: "Withdraw" }
    end if
    if action = "delete" then return { message: "Delete this request? Radarr and Sonarr keep what they have.", button: "Delete" }
    return invalid
end function

' The request an action makes: { method, path, jsonText }, or invalid
' for one that only goes somewhere. A declined one's reason, from the
' keyboard, goes with declineReason.
function reqActionCall(action as string, requestId as dynamic, reason as string) as dynamic
    base = "/api/requests/" + urlValue(requestId)
    if action = "withdraw" or action = "delete" then return { method: "DELETE", path: base }
    if action = "unwatch" then return { method: "DELETE", path: base + "/watch" }
    if action = "approve" or action = "retry" then return { method: "POST", path: base + "/approve", jsonText: "{}" }
    if action = "decline" then return { method: "POST", path: base + "/decline", jsonText: "{}" }
    if action = "declineReason"
        if reason.Trim() = "" then return { method: "POST", path: base + "/decline", jsonText: "{}" }
        return { method: "POST", path: base + "/decline", jsonText: FormatJson({ reason: reason.Trim() }) }
    end if
    if action = "handled" then return { method: "POST", path: base + "/handled", jsonText: "{}" }
    return invalid
end function

' Asking for a title (POST /api/requests): its body. A show's seasons
' always go with it.
function reqAskBody(kind as string, tmdbId as dynamic, seasons as dynamic) as string
    body = { kind: kind, tmdb_id: Int(httpNumber(tmdbId, 0)) }
    if kind = "show" then body.seasons = reqSeasonNumbers(seasons)
    return FormatJson(body)
end function

' Tell me too: asking for a show's seasons someone asked for already, or
' a movie; it costs nothing.
function reqFollowSeasons(seasons as dynamic) as object
    out = []
    for each season in httpArray(seasons)
        if GetInterface(season, "ifAssociativeArray") <> invalid and httpString(season.state) = "requested" then out.Push(Int(httpNumber(season.number, 0)))
    end for
    return out
end function

' The empty requests' list, for a tab: mine, the account's own (the
' managers' Yours, and everyone else's every tab); filtered, something
' is looked for.
function reqEmptyText(view as string, mine as boolean, filtered as boolean) as string
    if filtered then return "Nothing matches."
    of = ""
    if mine then of = " of yours"
    if view = "attention" then return "Nothing needs your attention."
    if view = "pending" then return "No requests" + of + " wait for a decision."
    if view = "approved" then return "No requests" + of + " are on their way."
    if view = "available" then return "No requests" + of + " are ready to watch yet."
    if view = "declined" then return "No requests" + of + " were declined."
    if view = "failed" then return "No requests" + of + " failed."
    if mine then return "You haven't asked for anything yet."
    return "No one has asked for anything yet."
end function

' The requests' tabs: [{ key, label }]. A manager opens on Needs
' attention and has Yours; everyone else sees their own.
function reqTabs(manage as boolean) as object
    tabs = []
    if manage then tabs.Push({ key: "attention", label: "Needs attention" })
    for each entry in [["all", "All"], ["pending", "Pending"], ["approved", "Approved"], ["available", "Available"], ["declined", "Declined"], ["failed", "Failed"]]
        tabs.Push({ key: entry[0], label: entry[1] })
    end for
    if manage then tabs.Push({ key: "mine", label: "Yours" })
    return tabs
end function

' The number on a requests' tab, 0 for none: Needs attention's, what
' waits for the managers (the summary's pending and failed; with filters,
' those of counts); Yours', the account's own (the summary's; with
' filters, what its list holds while it shows); All's, every status's; a
' status's, its own. counts: each status's with the filters (GET
' /api/requests' counts), invalid until known; view and total: the tab
' shown and what its list holds.
function reqTabCount(key as string, summary as object, counts as dynamic, filtered as boolean, view as string, total as integer) as integer
    known = httpObject(counts)
    if key = "attention"
        if not filtered then return summary.pending + summary.failed
        if known = invalid then return 0
        return httpNumber(known.pending, 0) + httpNumber(known.failed, 0)
    end if
    if key = "mine"
        if not filtered then return summary.own
        if view = "mine" and total > 0 then return total
        return 0
    end if
    if known = invalid then return 0
    if key <> "all" then return httpNumber(known[key], 0)
    n = 0
    for each status in ["pending", "approved", "available", "declined", "failed"]
        n = n + httpNumber(known[status], 0)
    end for
    return n
end function

' A number as a chip shows it: "" for none, "99+" past most (99, 999).
function reqCountText(n as integer, most as integer) as string
    if n <= 0 then return ""
    if n > most then return urlValue(most) + "+"
    return urlValue(n)
end function

' The requests' tabs as their chips show them: [{ label, width, count }],
' count the tab's number (reqTabCount's) in words.
function reqTabChips(manage as boolean, summary as object, counts as dynamic, filtered as boolean, view as string, total as integer) as object
    chips = []
    for each entry in reqTabs(manage)
        n = reqTabCount(entry.key, summary, counts, filtered, view, total)
        chips.Push({ label: entry.label, width: 0, count: reqCountText(n, 999) })
    end for
    return chips
end function

' The statuses' counts for the filters come with a tab's list, counted
' with every filter but the status. Needs attention's and Yours' narrow
' them further: with those, the counts are asked for on their own (the
' query, listing one request); invalid where the list's serve.
function reqCountsQuery(view as string, kind as string, userId as integer, q as string) as dynamic
    if view <> "attention" and view <> "mine" then return invalid
    query = reqListQuery("all", kind, userId, q, 0)
    query.limit = 1
    return query
end function

' The query of a tab (view) and filters (GET /api/requests): kind "" for
' both, userId 0 for everyone, q "" for any title.
function reqListQuery(view as string, kind as string, userId as integer, q as string, offset as integer) as object
    query = {}
    if view = "attention"
        query.attention = 1
        query.order = "oldest"
    else if view = "mine"
        query.mine = 1
    else if view <> "all"
        query.status = view
    end if
    if kind <> "" then query.kind = kind
    if userId > 0 then query.user_id = userId
    if q <> "" then query.q = q
    query.offset = offset
    query.limit = 50
    return query
end function

' The tab the requests open on: the one asked for (a notice's Open, See
' all: "mine") where the person has it, else the first, Needs attention
' for a manager.
function reqListView(view as string, manage as boolean) as string
    for each entry in reqTabs(manage)
        if entry.key = view then return view
    end for
    if manage then return "attention"
    return "all"
end function

' The managers' filters, as their chip says them: "Filter" for none, else
' "Movies · Ann · “star”".
function reqFilterLabel(kind as string, person as string, q as string) as string
    parts = []
    if kind = "movie" then parts.Push("Movies")
    if kind = "show" then parts.Push("Shows")
    if person <> "" then parts.Push(person)
    if q <> "" then parts.Push(reqQuoted(q))
    if parts.Count() = 0 then return "Filter"
    return parts.Join(" · ")
end function

' --- Notices ---

' "“X” is ready to watch.", "Season 2 of “X” is ready to watch.",
' "Seasons 1–2 of “X” are ready to watch.": what became ready, seasons
' invalid once the whole request is.
function reqReadyText(title as string, kind as string, seasons as dynamic) as string
    quoted = reqQuoted(title)
    label = reqSeasonsLabel(seasons)
    if kind <> "show" or label = "" then return quoted + " is ready to watch."
    verb = " are"
    if Left(label, 7) = "Season " and Instr(1, label, " and ") = 0 then verb = " is"
    return label + " of " + quoted + verb + " ready to watch."
end function

' One notice for what is newly ready: a title's words and poster, or "3
' of your requests have news" (what came while the app wasn't
' listening). { text, itemId (what Open opens; 0 for the requests),
' poster (its address, "" for none) }.
function reqNotice(announced as object, server as string) as object
    if announced.Count() = 1
        one = announced[0]
        return { text: reqReadyText(one.title, one.kind, one.seasons), itemId: one.itemId, poster: urlImage(server, one.poster, reqNoticePosterWidth()) }
    end if
    return { text: urlValue(announced.Count()) + " of your requests have news", itemId: 0, poster: "" }
end function

' The notice's poster, 80 pixels wide.
function reqNoticePosterWidth() as integer
    return 80
end function

' Whether a notice may show over the screen on top (its route): never
' over video, signing in, "Who's watching?", the server being away, the
' requests (which show the news themselves), Which seasons? (a panel on
' the other TVs) or libraries being sorted.
function reqNoticeOver(route as string) as boolean
    for each other in ["player", "livePlayer", "link", "servers", "profiles", "unreachable", "requests", "requestSeasons", "sortLibraries"]
        if route = other then return false
    end for
    return true
end function

' Whether the remote has rested long enough for a notice to show (1.5 s),
' so it never takes a press meant for the page. seconds: Roku's whole
' seconds since the last key (TimeSinceLastKeypress); keyMs: how long ago
' a look found a key within the second (the looks come four times a
' second), -1 for none since they began; pressMs: how long ago the app
' counted something as a press (a screen changing, a notice closing).
function reqRested(seconds as integer, keyMs as integer, pressMs as integer) as boolean
    if pressMs >= 0 and pressMs < 1500 then return false
    if seconds >= 2 then return true
    return seconds >= 1 and keyMs >= 1500
end function

' What a request has ready to watch, as the notices keep it: "all" once
' it is, else its seasons ready ("1,2"); "" for nothing.
function reqReadyOf(request as object) as string
    stage = httpString(request.stage)
    if stage = "gone" then return ""
    if httpString(request.status) = "available" or stage = "available" then return "all"
    if httpString(request.kind) <> "show" then return ""
    return reqNumbersText(request.available_seasons)
end function

' The seasons of ready (reqReadyOf's) not in told: invalid when the whole
' request is newly ready, [] for nothing new.
function reqNewlyReady(ready as string, told as string) as dynamic
    if ready = "" or told = "all" then return []
    if ready = "all" then return invalid
    fresh = []
    known = {}
    for each n in reqNumbersOf(told)
        known[urlValue(n)] = true
    end for
    for each n in reqNumbersOf(ready)
        if not known.DoesExist(urlValue(n)) then fresh.Push(n)
    end for
    return fresh
end function

' The notices' mark for a server's profile (registry section
' "requests"): { at (the newest updated_at looked at), ready ({ "42":
' what was told, reqReadyOf's }) }.
function reqLoadAnnounced(section as object, server as string, profileId as integer) as object
    mark = { at: 0, ready: {} }
    data = httpObject(httpJson(section.Read(reqAnnouncedKey(server, profileId))))
    if data = invalid then return mark
    mark.at = httpNumber(data.at, 0)
    ready = httpObject(data.ready)
    if ready <> invalid then mark.ready = ready
    return mark
end function

' Keeps mark, the 40 newest requests' at most.
sub reqSaveAnnounced(section as object, server as string, profileId as integer, mark as object)
    ids = []
    for each key in mark.ready
        ids = reqWithNumber(ids, key.ToInt())
    end for
    kept = {}
    first = ids.Count() - 40
    if first < 0 then first = 0
    for i = first to ids.Count() - 1
        key = urlValue(ids[i])
        kept[key] = mark.ready[key]
    end for
    section.Write(reqAnnouncedKey(server, profileId), FormatJson({ at: mark.at, ready: kept }))
    section.Flush()
end sub

function reqAnnouncedKey(server as string, profileId as integer) as string
    return server + "|" + profileId.ToStr()
end function

' What of the unseen requests (GET /api/requests?unseen=1) to tell now:
' what became ready to watch, in a library the account can open, since
' the notices last told of it. Changes mark (reqLoadAnnounced's): what it
' tells is kept, and requests not updated since aren't looked at again.
' [{ title, kind, seasons (invalid: the whole request), itemId,
' requestId, poster (its poster_url) }].
function reqToAnnounce(requests as dynamic, mark as object) as object
    out = []
    newest = mark.at
    for each request in httpArray(requests)
        if GetInterface(request, "ifAssociativeArray") <> invalid
            updated = httpNumber(request.updated_at, 0)
            if updated > newest then newest = updated
            itemId = httpNumber(request.item_id, 0)
            key = urlValue(httpNumber(request.id, 0))
            ready = reqReadyOf(request)
            told = ""
            if mark.ready.DoesExist(key) then told = mark.ready[key]
            fresh = reqNewlyReady(ready, told)
            if updated > mark.at and ready <> "" and (fresh = invalid or fresh.Count() > 0)
                mark.ready[key] = ready
                if itemId > 0 then out.Push({ title: httpSentence(request.title), kind: httpString(request.kind), seasons: fresh, itemId: itemId, requestId: httpNumber(request.id, 0), poster: httpString(request.poster_url) })
            end if
        end if
    end for
    mark.at = newest
    return out
end function

' A request_available event: what to tell ([] when its news was told
' already, else one entry as reqToAnnounce's). Changes mark.
function reqAnnounceEvent(event as object, mark as object) as object
    key = urlValue(httpNumber(event.request_id, 0))
    told = ""
    if mark.ready.DoesExist(key) then told = mark.ready[key]
    ready = "all"
    if not httpTrue(event.complete) and httpString(event.kind) = "show"
        known = reqNumbersOf(told)
        if told = "all" then known = []
        for each n in httpArray(event.seasons)
            known.Push(n)
        end for
        ready = reqNumbersText(known)
    end if
    fresh = reqNewlyReady(ready, told)
    if fresh <> invalid and fresh.Count() = 0 then return []
    mark.ready[key] = ready
    return [{ title: httpSentence(event.title), kind: httpString(event.kind), seasons: fresh, itemId: httpNumber(event.item_id, 0), requestId: httpNumber(event.request_id, 0), poster: httpString(event.poster_url) }]
end function

' mark (reqLoadAnnounced's) as it is now, which telling more leaves be.
function reqMarkCopy(mark as object) as object
    ready = {}
    for each key in mark.ready
        ready[key] = mark.ready[key]
    end for
    return { at: mark.at, ready: ready }
end function

' The mark to save while notices wait to be seen (waiting, oldest first:
' each with before, reqMarkCopy's from before its news): the oldest's
' before, so what they tell is told again should the app close first.
' Those seen were all told before it. With none waiting, mark.
function reqMarkToKeep(mark as object, waiting as object) as object
    if waiting.Count() = 0 then return mark
    return waiting[0].before
end function
