' Sunday 1 November 2026, 12:00 UTC: the tests' now.
function reqTestNow() as integer
    return 1793534400
end function

function reqTestSummary(fields as object) as object
    summary = reqSummary({ enabled: true, access: "ask", movies_left: invalid, seasons_left: invalid, movies_limit: invalid, seasons_limit: invalid, manage: false, pending: 0, failed: 0, unseen: 0, own: 0, changed_at: 1793534000, sign_in_again: invalid })
    summary.Append(fields)
    return summary
end function

function reqTestRequest(fields as object) as object
    request = {
        id: 42,
        kind: "show",
        tmdb_id: 1396,
        title: "Breaking Bad",
        year: 2008,
        poster_url: "/api/discover/images/poster/abc.jpg",
        seasons: [1, 2],
        available_seasons: [],
        status: "pending",
        stage: "pending",
        stage_at: invalid,
        reason: invalid,
        note: invalid,
        mine: true,
        watching: false,
        unseen: false,
        actions: ["withdraw"],
        requested_by: { id: 1, name: "Anna" },
        created_at: reqTestNow() - 3 * 86400,
        updated_at: reqTestNow() - 3 * 86400,
        retry_at: invalid,
        item_id: invalid,
        download: invalid
    }
    request.Append(fields)
    return request
end function

function reqTestSeason(number as integer, state as string) as object
    return { number: number, name: "", episodes: 8, aired: 8, air_date: "2009-03-08", poster_url: invalid, in_library: 3, state: state, declined: false, request_id: invalid }
end function

function reqTestDetail(fields as object) as object
    detail = {
        kind: "movie",
        tmdb_id: 27205,
        title: "Inception",
        year: 2010,
        overview: "A thief who steals secrets.",
        poster_url: "/api/discover/images/poster/inc.jpg",
        backdrop_url: "/api/discover/images/backdrop/inc.jpg",
        rating: 8.4,
        status: "none",
        item_id: invalid,
        request_id: invalid,
        tagline: "Your mind is the scene of the crime.",
        genres: ["Action", "Science Fiction", "Adventure", "Thriller"],
        runtime_minutes: 148,
        certification: "PG-13",
        cast: [{ name: "Leonardo DiCaprio", character: "Cobb", photo_url: "/api/discover/images/profile/leo.jpg" }],
        seasons: invalid,
        requests: [],
        declined: invalid,
        restriction: invalid,
        watching: false,
        movies_left: invalid,
        seasons_left: invalid,
        movies_next_at: invalid,
        seasons_next_at: invalid
    }
    detail.Append(fields)
    return detail
end function

sub test_requests_summary_is_read_with_its_nulls()
    summary = reqSummary({ enabled: true, access: "auto", movies_left: 2, seasons_left: invalid, movies_limit: 3, seasons_limit: invalid, movies_next_at: invalid, seasons_next_at: invalid, manage: true, pending: 4, failed: 1, unseen: 2, own: 7, changed_at: 1793534000, sign_in_again: invalid })
    tkTrue(summary.known)
    tkFalse(summary.absent)
    tkTrue(summary.enabled)
    tkEqual(summary.access, "auto")
    tkEqual(summary.moviesLeft, 2)
    tkEqual(summary.seasonsLeft, -1, "no limit")
    tkEqual(summary.moviesLimit, 3)
    tkEqual(summary.seasonsNextAt, 0)
    tkTrue(summary.manage)
    tkEqual(summary.changedAt, 1793534000)
    tkEqual(summary.signInAgain, "")
    tkEqual(reqSummary({ access: "whatever", sign_in_again: "elsewhere" }).access, "off")
    tkEqual(reqSummary({ sign_in_again: "approvals" }).signInAgain, "approvals")
    tkFalse(reqSummary("not an object").known)
end sub

sub test_requests_summary_answers()
    tkTrue(reqSummaryAnswer({ ok: true, status: 200, data: { enabled: true, access: "ask" }, error: "" }).enabled)
    older = reqSummaryAnswer({ ok: false, status: 404, data: invalid, error: "" })
    tkTrue(older.absent, "an older server")
    tkFalse(older.enabled)
    tkTrue(reqSummaryAnswer({ ok: false, status: 403, data: { code: "insufficient_scope" }, error: "insufficient_scope" }).absent, "a build from before apps could ask")
    tkInvalid(reqSummaryAnswer({ ok: false, status: 0, data: invalid, error: "network" }), "the server away: the summary there was")
    tkInvalid(reqSummaryAnswer({ ok: false, status: 500, data: invalid, error: "" }))
    ' A screen's field before the scene gave it one.
    tkEqual(reqSummaryOf({}), reqNoSummary(false))
    tkEqual(reqSummaryOf(invalid), reqNoSummary(false))
    tkTrue(reqSummaryOf(reqTestSummary({})).enabled)
end sub

sub test_requests_entries_follow_the_summary()
    asking = reqTestSummary({})
    tkTrue(reqShowsDiscover(asking))
    tkTrue(reqMayAsk(asking))
    tkTrue(reqShowsRequests(asking))
    tkEqual(reqPageName(asking), "Your requests")
    again = reqTestSummary({ signInAgain: "requests" })
    tkTrue(reqShowsDiscover(again), "Discover says to sign in again")
    tkFalse(reqMayAsk(again))
    tkFalse(reqShowsRequests(again))
    off = reqTestSummary({ access: "off" })
    tkFalse(reqShowsDiscover(off))
    tkFalse(reqShowsRequests(off), "nothing asked for")
    off.own = 2
    tkTrue(reqShowsRequests(off), "its own requests still")
    tkFalse(reqShowsDiscover(reqNoSummary(false)))
    tkFalse(reqShowsRequests(reqNoSummary(true)))
    manager = reqTestSummary({ manage: true, access: "auto" })
    tkEqual(reqPageName(manager), "Requests")
    tkTrue(reqShowsRequests(manager))
end sub

sub test_requests_badge_counts()
    tkEqual(reqBadge(reqTestSummary({ unseen: 2 })), { count: 2, danger: false })
    tkEqual(reqBadge(reqTestSummary({ manage: true, pending: 3, failed: 0, unseen: 1 })), { count: 4, danger: false })
    tkEqual(reqBadge(reqTestSummary({ manage: true, pending: 3, failed: 2, unseen: 1 })), { count: 6, danger: true })
    ' Discover's way to them says the same.
    tkEqual(reqRequestsChip(reqTestSummary({ manage: true, pending: 3, failed: 2, unseen: 1 })), { label: "Requests", width: 0, count: "6", accent: true, danger: true })
    tkEqual(reqRequestsChip(reqTestSummary({ unseen: 0, own: 2 })).count, "", "nothing new: no number")
    tkEqual(reqRequestsChip(reqTestSummary({ unseen: 120 })).count, "99+")
end sub

sub test_requests_allowance_words()
    now = reqTestNow()
    both = reqAllowance({ moviesLeft: 2, seasonsLeft: 5, moviesLimit: 3, seasonsLimit: 10, moviesNextAt: 0, seasonsNextAt: 0 }, now)
    tkEqual(both.text, "You can ask for 2 more movies and 5 more seasons.")
    tkEqual(both.limits, "Limits: 3 movies and 10 seasons per 7 days.")
    movies = reqAllowance({ moviesLeft: 0, seasonsLeft: -1, moviesLimit: 3, seasonsLimit: -1, moviesNextAt: now + 3 * 86400, seasonsNextAt: 0 }, now)
    tkEqual(movies.text, "You've asked for all the movies you can for now: you can ask for more in 3 days.")
    tkEqual(movies.limits, "Limit: 3 movies per 7 days.")
    none = reqAllowance({ moviesLeft: 0, seasonsLeft: 0, moviesLimit: 1, seasonsLimit: 10, moviesNextAt: now + 3 * 86400, seasonsNextAt: now + 5 * 86400 }, now)
    tkEqual(none.text, "You've asked for all you can for now: more movies in 3 days, more seasons in 5 days.")
    tkEqual(none.limits, "Limits: 1 movie and 10 seasons per 7 days.")
    mixed = reqAllowance({ moviesLeft: 1, seasonsLeft: 0, moviesLimit: 3, seasonsLimit: 10, moviesNextAt: 0, seasonsNextAt: now + 86400 }, now)
    tkEqual(mixed.text, "You can ask for 1 more movie, and for more seasons tomorrow.")
    tkEqual(reqAllowance(reqNoSummary(false), now), { text: "", limits: "" }, "no limits")
    tkEqual(reqAskWords("auto"), "Approved as you ask, and sent on to download.")
end sub

sub test_requests_seasons_labels()
    tkEqual(reqSeasonsLabel(invalid), "")
    tkEqual(reqSeasonsLabel([2]), "Season 2")
    tkEqual(reqSeasonsLabel([3, 1, 2, 5]), "Seasons 1–3, 5")
    tkEqual(reqSeasonsLabel([0, 1, 2]), "Seasons 1–2 and specials")
    tkEqual(reqSeasonsLabel([0]), "Specials")
    tkEqual(reqSeasonsLabel([2, 2, 4]), "Seasons 2, 4")
    tkEqual(reqNumbersText([3, 1]), "1,3")
    tkEqual(reqNumbersOf("1,3"), [1, 3])
end sub

sub test_requests_title_tiles()
    seen = { "movie:603": true }
    results = [
        { kind: "movie", tmdb_id: 603, title: "The Matrix", year: 1999, poster_url: "/api/discover/images/poster/m.jpg", status: "library", item_id: 12 },
        { kind: "show", tmdb_id: 1396, title: "Breaking Bad", year: 2008, poster_url: "/api/discover/images/poster/b.jpg", status: "partial", item_id: invalid },
        { kind: "movie", tmdb_id: 27205, title: "Inception", year: invalid, poster_url: invalid, status: "declined", item_id: invalid },
        { kind: "person", tmdb_id: 7, title: "Someone" },
        { kind: "show", tmdb_id: 1396, title: "Breaking Bad again" }
    ]
    tiles = reqTitleTiles(results, "http://nas:7539", seen)
    tkEqual(tiles.Count(), 2, "shown before, people and repeats left out")
    show = tiles[0]
    tkEqual(show.title, "Breaking Bad")
    tkEqual(show.subtitle, "2008 · Show")
    tkEqual(show.image, "http://nas:7539/api/discover/images/poster/b.jpg?w=220")
    tkEqual(show.kind, "discover")
    tkEqual(show.tmdbKind, "show")
    tkEqual(show.tmdbId, 1396)
    tkEqual(show.badge, "Requested", "partly in a library it can't open")
    tkEqual(tiles[1].subtitle, "Movie")
    tkEqual(tiles[1].badge, "Declined")
    tkEqual(tiles[1].badgeColor, "0xEB6B70FF")
    tkTrue(seen.DoesExist("show:1396"))
    tkEqual(reqTitleBadge("library", invalid), "In library")
    tkEqual(reqTitleBadge("partial", 9), "Partly in library")
    tkEqual(reqTitleBadge("restricted", invalid), "")
    tkEqual(reqTitleBadge("none", invalid), "")
    tkEqual(reqBadgeColor("library", invalid), "0x3FB27FFF")
end sub

sub test_requests_grids_load_five_pages_until_the_focus_moves()
    tkTrue(reqWantsPage(0, -1, 0, 0, 14, 0), "the first page")
    tkTrue(reqWantsPage(3, 500, 0, 2, 14, 3), "short of a screenful past the focus")
    tkFalse(reqWantsPage(5, 500, 0, 2, 14, 5), "five pages since the focus moved")
    tkFalse(reqWantsPage(2, 500, 0, 40, 14, 1), "a screenful past the focus")
    tkTrue(reqWantsPage(2, 500, 30, 40, 14, 0), "the focus near the end")
    tkFalse(reqWantsPage(4, 4, 30, 40, 14, 0), "the last page came")
    further = reqFurtherTile()
    tkEqual(further.kind, "further")
    tkInvalid(navForTile(further), "the grid asks for more itself")
end sub

' A grid's later page that fails ends it in a tile saying so, which asks
' again where that may help.
sub test_requests_a_grid_s_failed_page_offers_retry()
    away = reqMoreProblemTile({ status: 0, error: "network", message: "" }, false)
    tkEqual(away.title, "Couldn't load more. Select to try again.")
    tkEqual(away.kind, "retry")
    tkInvalid(navForTile(away), "the grid asks again itself")
    tkEqual(reqMoreProblemTile({ status: 503, error: "tmdb_down" }, false).title, "TMDB isn't answering. Select to try again.")
    key = reqMoreProblemTile({ status: 503, error: "tmdb_key" }, false)
    tkEqual(key.title, "TMDB refused this server's key.")
    tkEqual(key.kind, "note")
end sub

sub test_requests_badges_come_from_the_title()
    library = reqTestDetail({ status: "library", item_id: 12 })
    tkEqual(reqTitleNote("movie:27205", { ok: true, status: 200, data: library }), { key: "movie:27205", status: "library", libraryId: 12 })
    ' A manager's title someone else's request was declined for: the
    ' title's status, not that request's.
    others = reqTestDetail({ status: "none", requests: [reqTestRequest({ kind: "movie", tmdb_id: 27205, status: "declined", stage: "declined", mine: false })] })
    tkEqual(reqTitleNote("movie:27205", { ok: true, status: 200, data: others }), { key: "movie:27205", status: "none", libraryId: 0 })
    tkInvalid(reqTitleNote("movie:27205", { ok: false, status: 503, error: "tmdb_down", data: { code: "tmdb_down" } }), "TMDB away: the badge stays")
    tkInvalid(reqTitleNote("movie:27205", { ok: true, status: 200, data: invalid }))
end sub

sub test_requests_stage_words()
    now = reqTestNow()
    tkEqual(reqStage(reqTestRequest({}), false, now, 0), { text: "Waiting for approval", tone: "pending", line: "", progress: -1 })
    tkEqual(reqStage(reqTestRequest({ status: "approved", stage: "approved" }), false, now, 0).text, "Approved")
    tkEqual(reqStage(reqTestRequest({ status: "approved", stage: "searching" }), false, now, 0).text, "Searching")
    tkEqual(reqStage(reqTestRequest({ status: "approved", stage: "importing" }), false, now, 0).text, "Adding to the library")
    tkEqual(reqStage(reqTestRequest({ status: "available", stage: "available" }), false, now, 0).tone, "ready")
    tkEqual(reqStage(reqTestRequest({ status: "available", stage: "gone" }), false, now, 0), { text: "No longer in a library", tone: "gone", line: "", progress: -1 })
    tkEqual(reqStage(reqTestRequest({ status: "declined", stage: "declined" }), false, now, 0).text, "Declined")
    tkEqual(reqStage(reqTestRequest({ status: "approved", stage: invalid }), false, now, 0).text, "Approved", "no stage: the status")
end sub

sub test_requests_waiting_says_its_day()
    now = reqTestNow()
    movie = reqTestRequest({ kind: "movie", status: "approved", stage: "waiting", stage_at: now + 3 * 86400 })
    tkEqual(reqStage(movie, false, now, 0).text, "Out Nov 4")
    movie.stage_at = now + 2 * 3600
    tkEqual(reqStage(movie, false, now, 0).text, "Out Nov 1", "a movie's day, never a time")
    show = reqTestRequest({ status: "approved", stage: "waiting", stage_at: now + 9 * 3600 + 300 })
    tkEqual(reqStage(show, false, now, 0).text, "Airs 9:05 PM", "within a day: its time")
    show.stage_at = now + 20 * 3600
    tkEqual(reqStage(show, false, now, 0).text, "Airs tomorrow at 8:00 AM", "past midnight: tomorrow")
    tkEqual(reqStage(show, false, now, -10 * 3600).text, "Airs 10:00 PM", "the same day in Honolulu")
    show.stage_at = now + 70 * 86400
    tkEqual(reqStage(show, false, now, 0).text, "Airs Jan 10, 2027")
    show.stage_at = invalid
    tkEqual(reqStage(show, false, now, 0).text, "Not out yet")
end sub

sub test_requests_downloads_say_how_far()
    now = reqTestNow()
    request = reqTestRequest({ status: "approved", stage: "downloading", download: { progress: 0.42, state: "downloading", eta: now + 20 * 60, episodes: 3 } })
    stage = reqStage(request, false, now, 0)
    tkEqual(stage.text, "Downloading 42% · in 20 min")
    tkEqual(stage.tone, "active")
    tkTrue(stage.progress > 0.41 and stage.progress < 0.43)
    request.download = { progress: 0.1, state: "queued", eta: invalid, episodes: 1 }
    tkEqual(reqStage(request, false, now, 0).text, "Waiting to download")
    request.download.state = "paused"
    tkEqual(reqStage(request, false, now, 0).text, "Download paused")
    request.download.state = "warning"
    tkEqual(reqStage(request, false, now, 0), { text: "The download has a problem", tone: "problem", line: "", progress: 0.1 })
    partial = reqTestRequest({ status: "approved", stage: "partial", available_seasons: [1, 2], download: { progress: 0.5, state: "downloading", eta: invalid, episodes: 8 } })
    stage = reqStage(partial, false, now, 0)
    tkEqual(stage.text, "Seasons 1–2 ready")
    tkEqual(stage.progress, 0.5, "the bar shows while it downloads")
end sub

sub test_requests_failed_ones_say_what_happens()
    now = reqTestNow()
    request = reqTestRequest({ status: "failed", stage: "failed", reason: "Sonarr isn't answering" })
    tkEqual(reqStage(request, true, now, 0), { text: "Couldn't send: Sonarr isn't answering", tone: "problem", line: "", progress: -1 })
    request.reason = invalid
    tkEqual(reqStage(request, false, now, 0), { text: "Not sent yet", tone: "problem", line: "It couldn't be sent to download yet: an administrator will look at it.", progress: -1 })
    request.retry_at = now + 300
    tkEqual(reqStage(request, false, now, 0).line, "Trying again in 5 min.")
end sub

sub test_requests_rows()
    now = reqTestNow()
    request = reqTestRequest({ status: "declined", stage: "declined", reason: "Not in this quality", actions: ["withdraw", "delete"], note: "Added by hand" })
    row = reqRow(request, "http://nas:7539", true, now, 0)
    tkEqual(row.title, "Breaking Bad")
    tkEqual(row.subtitle, "2008 · Show · Seasons 1–2")
    tkEqual(row.image, "http://nas:7539/api/discover/images/poster/abc.jpg?w=120")
    tkEqual(row.stageText, "Declined")
    tkEqual(row.stageColor, "0xEB6B70FF")
    tkEqual(row.who, "Asked for by you · 3 days ago")
    tkEqual(row.line, "“Not in this quality” · Added by hand")
    tkEqual(row.actions, ["withdraw", "delete"])
    tkEqual(row.seasons, [1, 2])
    followed = reqRow(reqTestRequest({ mine: false, watching: true, requested_by: invalid, status: "available", stage: "available", seasons: invalid, kind: "movie" }), "http://nas:7539", false, now, 0)
    tkEqual(followed.subtitle, "2008 · Movie · Following")
    tkEqual(followed.who, "Asked for by someone else · 3 days ago")
    tkEqual(followed.line, "Ready, in a library you can't open: ask an administrator")
    other = reqRow(reqTestRequest({ mine: false, requested_by: { id: 2, name: "Ben" } }), "http://nas:7539", true, now, 0)
    tkEqual(other.who, "Asked for by Ben · 3 days ago")
    tkTrue(reqBusy("downloading"))
    tkFalse(reqBusy("waiting"))
end sub

' An open list is asked again while a download moves, which sends no
' events, and while no event can tell it what changed.
sub test_requests_lists_are_asked_again_without_events()
    tkEqual(reqListPoll(true, true), 10, "a download moves")
    tkEqual(reqListPoll(true, false), 10)
    tkEqual(reqListPoll(false, false), 30, "the event stream is down")
    tkEqual(reqListPoll(false, true), 0, "events tell of changes")
end sub

' Discover's Your requests: the stage under the poster, in its colour;
' and the managers' person filter.
sub test_requests_request_tiles_and_people()
    tile = reqRequestTile(reqTestRequest({ status: "approved", stage: "downloading", download: { progress: 0.42, state: "downloading", eta: invalid, episodes: 3 } }), "http://nas:7539", false, reqTestNow(), 0)
    tkEqual(tile.subtitle, "Downloading 42%")
    tkEqual(tile.subtitleColor, "0x38BDF8FF")
    tkEqual(tile.progress, 0.42)
    tkEqual(tile.badge, "")
    tkEqual(tile.key, "request:42")
    tkEqual(navForTile(tile), { to: "requestTitle", params: { kind: "show", tmdbId: 1396, title: "Breaking Bad" } })
    tkEqual(reqPeople([{ id: 2, name: "Ben", avatar_url: invalid }, { id: 0, name: "Nobody" }, { id: 3, name: "" }, "x"]), [{ id: 2, name: "Ben" }])
end sub

sub test_requests_actions_in_the_server_s_order()
    actions = reqActions(reqTestRequest({ status: "pending", actions: ["approve", "decline", "handled", "delete"] }), 0)
    labels = []
    for each option in actions
        labels.Push(option.label)
    end for
    tkEqual(labels, ["Approve", "Decline", "Decline with a reason…", "Mark as added by hand", "Delete"])
    tkEqual(actions[2].action, "declineReason")
    removed = reqActions(reqTestRequest({ status: "declined", stage: "declined", actions: ["withdraw"] }), 0)
    tkEqual(removed, [{ label: "Remove", action: "withdraw" }], "a declined one is removed")
    ready = reqActions(reqTestRequest({ status: "available", stage: "available", actions: ["ask_again", "unwatch", "retry", "bogus"] }), 12)
    tkEqual(ready, [{ label: "Watch", action: "watch" }, { label: "Request again", action: "ask_again" }, { label: "Stop following", action: "unwatch" }, { label: "Try again", action: "retry" }])
end sub

sub test_requests_confirms_and_calls()
    tkEqual(reqConfirm("withdraw", "Dune", "pending"), { message: "Withdraw your request for “Dune”?", button: "Withdraw" })
    tkEqual(reqConfirm("withdraw", "Dune", "declined"), { message: "Remove “Dune” from your requests?", button: "Remove" })
    tkEqual(reqConfirm("delete", "Dune", "approved"), { message: "Delete this request? Radarr and Sonarr keep what they have.", button: "Delete" })
    tkInvalid(reqConfirm("approve", "Dune", "pending"))
    tkEqual(reqActionCall("withdraw", 42, ""), { method: "DELETE", path: "/api/requests/42" })
    tkEqual(reqActionCall("unwatch", 42, ""), { method: "DELETE", path: "/api/requests/42/watch" })
    tkEqual(reqActionCall("retry", 42, ""), { method: "POST", path: "/api/requests/42/approve", jsonText: "{}" })
    tkEqual(reqActionCall("handled", 42, ""), { method: "POST", path: "/api/requests/42/handled", jsonText: "{}" })
    tkEqual(reqActionCall("declineReason", 42, "  Not in 4K "), { method: "POST", path: "/api/requests/42/decline", jsonText: "{""reason"":""Not in 4K""}" })
    tkEqual(reqActionCall("declineReason", 42, " "), { method: "POST", path: "/api/requests/42/decline", jsonText: "{}" })
    tkInvalid(reqActionCall("watch", 42, ""))
    tkEqual(ParseJson(reqAskBody("show", 1396, [1, 2])), { kind: "show", tmdb_id: 1396, seasons: [1, 2] })
    tkEqual(ParseJson(reqAskBody("movie", 27205, invalid)), { kind: "movie", tmdb_id: 27205 })
end sub

sub test_requests_asking_words()
    tkEqual(reqAsked(201, { status: "pending" }, "Dune"), "Asked for “Dune”: an administrator decides.")
    tkEqual(reqAsked(201, { status: "approved" }, "Dune"), "“Dune” is approved and on its way.")
    tkEqual(reqAsked(201, { status: "failed" }, "Dune"), "Asked for “Dune”, but it couldn't be sent to download yet.")
    tkEqual(reqAsked(200, { status: "approved", watching: true }, "Dune"), "You'll hear when “Dune” is ready.")
    now = reqTestNow()
    limits = reqTestSummary({ moviesLimit: 3, seasonsLimit: 5 })
    tkEqual(reqAskProblem({ status: 409, error: "declined", message: "Declined on Nov 1", data: { code: "declined", retry_at: now + 12 * 86400 } }, limits, "movie", now), "You can ask again in 12 days.")
    tkEqual(reqAskProblem({ status: 429, error: "too_many_requests", message: "", data: { code: "too_many_requests", retry_at: now + 3 * 86400 } }, limits, "show", now), "You can ask for more in 3 days.")
    tkEqual(reqAskProblem({ status: 429, error: "too_many_requests", message: "", data: { code: "too_many_requests", retry_at: invalid } }, limits, "show", now), "You can ask for at most 5 seasons per 7 days: pick fewer.")
    tkEqual(reqAskProblem({ status: 403, error: "no_access", message: "No.", data: {} }, limits, "show", now), "Requests aren't open to this account.")
    tkEqual(reqAskProblem({ status: 409, error: "in_library", message: "A library has this movie already.", data: {} }, limits, "movie", now), "A library has this movie already.")
end sub

sub test_requests_refused_actions_say_when_from_retry_at()
    now = reqTestNow()
    limits = reqTestSummary({ moviesLimit: 3, seasonsLimit: 5 })
    early = { status: 409, error: "conflict", message: "a declined request stays for 30 days: it may be removed from Nov 13", data: { code: "conflict", request: reqTestRequest({ status: "declined", stage: "declined" }), retry_at: now + 12 * 86400 } }
    tkEqual(reqActProblem(early, "withdraw", limits, "show", now), "A declined request stays for 30 days: you can remove it in 12 days.")
    changed = { status: 409, error: "conflict", message: "this request changed meanwhile: it is approved now", data: { code: "conflict", request: reqTestRequest({}), retry_at: invalid } }
    tkEqual(reqActProblem(changed, "withdraw", limits, "show", now), "this request changed meanwhile: it is approved now")
    tkEqual(reqActProblem({ status: 404, error: "not_found", message: "no such request", data: {} }, "approve", limits, "show", now), "This request was deleted meanwhile.")
    declined = { status: 409, error: "declined", message: "declined for this account lately", data: { code: "declined", retry_at: now + 3 * 86400 } }
    tkEqual(reqActProblem(declined, "ask_again", limits, "movie", now), "You can ask again in 3 days.", "Request again: as asking says it")
end sub

sub test_requests_problems_say_what_to_do()
    tkEqual(reqProblem({ status: 503, error: "unavailable" }, false), { title: "Requests are off", text: "This server doesn't take requests for movies and shows.", retry: false, signIn: false })
    tkEqual(reqProblem({ status: 503, error: "tmdb_key" }, false).text, "An administrator needs to check it.")
    tkEqual(reqProblem({ status: 503, error: "tmdb_key" }, true).title, "TMDB refused this server's key")
    tkTrue(reqProblem({ status: 503, error: "tmdb_down" }, false).retry)
    tkEqual(reqProblem({ status: 403, error: "no_access" }, false).title, "Requests aren't open to this account")
    tkTrue(reqProblem({ status: 403, error: "insufficient_scope" }, false).signIn)
    tkInvalid(reqProblem({ status: 0, error: "network" }, false))
end sub

' Search's Discover row says every problem, and offers Try again only where
' asking again may help.
sub test_requests_search_says_what_stops_tmdb()
    down = reqSearchProblemTile({ status: 503, error: "tmdb_down" }, false)
    tkEqual(down.title, "TMDB isn't answering. Select to try again.")
    tkEqual(down.kind, "retry")
    key = reqSearchProblemTile({ status: 503, error: "tmdb_key" }, true)
    tkEqual(key.title, "TMDB refused this server's key.")
    tkEqual(key.kind, "note")
    tkInvalid(navForTile(key), "it opens nothing")
    tkEqual(reqSearchProblemTile({ status: 403, error: "no_access" }, false).title, "Requests aren't open to this account.")
    tkEqual(reqSearchProblemTile({ status: 503, error: "unavailable" }, false).kind, "note")
    other = reqSearchProblemTile({ status: 500, error: "internal", message: "database is locked" }, false)
    tkEqual(other.title, "Couldn't load these. Select to try again.")
    tkEqual(other.kind, "retry")
end sub

' Search's Discover row reads on while too few titles the libraries lack
' came: 20 at most, from three of TMDB's pages at most.
sub test_requests_search_reads_up_to_three_pages()
    tkTrue(reqSearchWantsPage(0, 1, 40, 1), "the first page all in a library")
    tkTrue(reqSearchWantsPage(12, 2, 40, 2))
    tkFalse(reqSearchWantsPage(12, 3, 40, 3), "three pages read")
    tkFalse(reqSearchWantsPage(20, 1, 40, 1), "enough to show")
    tkFalse(reqSearchWantsPage(4, 1, 1, 1), "the last page came")
    tkEqual(reqSearchShown(), 20)
end sub

' The See all of Search's Discover row: TMDB's results for the words as its search takes
' them, at most 200 characters.
sub test_requests_see_all_of_a_list_or_a_search()
    list = reqSeeAll("upcoming", "")
    tkEqual(list, { title: "Coming to cinemas", path: "/api/discover/upcoming", empty: "Nothing here right now." })
    search = reqSeeAll("", "  dune ")
    tkEqual(search.title, "Results for “dune”")
    tkEqual(search.path, "/api/discover/search?q=dune")
    tkEqual(search.empty, "Nothing found for “dune”.")
    tkEqual(reqSeeAll("", "the matrix").path, "/api/discover/search?q=the%20matrix")
    long = String(250, "a")
    tkEqual(reqSearchWords(long), String(200, "a"))
    tkEqual(reqSearchWords(String(199, "a") + " b"), String(199, "a"), "no space left at the end")
    tkEqual(reqSearchWords("  dune  "), "dune")
end sub

sub test_requests_title_page_of_a_movie()
    summary = reqTestSummary({ moviesLimit: 3 })
    page = reqTitlePage(reqTestDetail({}), summary, "http://nas:7539", reqTestNow(), 0)
    tkEqual(page.title, "Inception")
    tkEqual(page.line, "2010 · 2 h 28 min · PG-13 · ★ 8.4")
    tkEqual(page.genres, "Action, Science Fiction, Adventure")
    tkEqual(page.poster, "http://nas:7539/api/discover/images/poster/inc.jpg?w=420")
    tkEqual(page.backdrop, "http://nas:7539/api/discover/images/backdrop/inc.jpg?w=640")
    tkEqual(page.buttons, ["Request"])
    tkEqual(page.actions, ["request"])
    tkEqual(page.note, "")
    tkInvalid(page.stage)
    library = reqTitlePage(reqTestDetail({ status: "library", item_id: 12 }), summary, "http://nas:7539", reqTestNow(), 0)
    tkEqual(library.actions, ["open"])
    tkEqual(library.itemId, 12)
    restricted = reqTitlePage(reqTestDetail({ status: "restricted", restriction: "Rated R: above this account's age limit" }), summary, "", reqTestNow(), 0)
    tkEqual(restricted.buttons, [])
    tkEqual(restricted.note, "Rated R: above this account's age limit")
    declined = reqTitlePage(reqTestDetail({ status: "declined", declined: { reason: "Not in 4K", at: reqTestNow(), again_at: reqTestNow() + 12 * 86400 } }), summary, "", reqTestNow(), 0)
    tkEqual(declined.buttons, [])
    tkEqual(declined.note, "Declined: “Not in 4K”. You can ask again in 12 days.")
    tkEqual(reqDeclinedText({ reason: invalid, at: 0, again_at: reqTestNow() + 86400 }, reqTestNow()), "Declined. You can ask again tomorrow.")
end sub

sub test_requests_title_page_leaves_out_what_can_t_be_pressed()
    now = reqTestNow()
    off = reqTitlePage(reqTestDetail({}), reqTestSummary({ access: "off" }), "", now, 0)
    tkEqual(off.buttons, [])
    tkEqual(off.note, "Requests aren't open to this account.")
    used = reqTitlePage(reqTestDetail({ movies_left: 0, movies_next_at: now + 3 * 86400 }), reqTestSummary({ moviesLimit: 3 }), "", now, 0)
    tkEqual(used.buttons, [], "Request can't be disabled: it goes")
    tkEqual(used.note, "You've asked for all the movies you can for now: you can ask for more in 3 days.")
    again = reqTitlePage(reqTestDetail({}), reqTestSummary({ signInAgain: "requests" }), "", now, 0)
    tkEqual(again.actions, ["signIn"])
    tkEqual(again.note, "Sign in again to ask for movies and shows.")
end sub

' A movie is asked for after a question: what asking does, what may still
' be asked for.
sub test_requests_asking_for_a_movie_asks_first()
    now = reqTestNow()
    allowance = { moviesLeft: 2, seasonsLeft: -1, moviesNextAt: 0, seasonsNextAt: 0, moviesLimit: 3, seasonsLimit: -1 }
    tkEqual(reqAskConfirm("Dune", "ask", allowance, now), { title: "Ask for “Dune”?", lines: ["An administrator decides on it. You'll hear when it's ready to watch.", "You can ask for 2 more movies.", "Limit: 3 movies per 7 days."] })
    tkEqual(reqAskConfirm("Dune", "auto", reqNoSummary(false), now).lines, ["Approved as you ask, and sent on to download."], "no limit")
end sub

sub test_requests_title_page_of_a_request()
    now = reqTestNow()
    summary = reqTestSummary({})
    mine = reqTestRequest({ kind: "movie", status: "approved", stage: "searching", actions: ["withdraw"] })
    requested = reqTitlePage(reqTestDetail({ status: "requested", request_id: 42, requests: [mine] }), summary, "", now, 0)
    tkEqual(requested.note, "Requested", "its button can't be pressed: its line")
    tkEqual(requested.stage.text, "Searching")
    tkEqual(requested.requestId, 42)
    tkEqual(requested.buttons, ["Withdraw"])
    tkFalse(requested.busy)
    mine.stage = "downloading"
    tkTrue(reqTitlePage(reqTestDetail({ status: "requested", request_id: 42, requests: [mine] }), summary, "", now, 0).busy, "asked again every 10 s")
    followed = reqTestRequest({ kind: "movie", mine: false, watching: true, status: "approved", stage: "approved", actions: ["unwatch"] })
    following = reqTitlePage(reqTestDetail({ status: "requested", request_id: 42, watching: true, requests: [followed] }), summary, "", now, 0)
    tkEqual(following.note, "Following")
    tkEqual(following.buttons, ["Stop following"])
    tkEqual(following.actions, ["unwatch"])
    other = reqTitlePage(reqTestDetail({ status: "requested", request_id: invalid, requests: [] }), summary, "", now, 0)
    tkEqual(other.buttons, ["Tell me too"])
    tkEqual(other.actions, ["follow"])
    tkEqual(other.note, "Someone asked for it already.")
    pending = reqTestRequest({ kind: "movie", mine: false, status: "pending", stage: "pending", actions: ["approve", "decline", "handled", "delete"] })
    managed = reqTitlePage(reqTestDetail({ status: "requested", request_id: 42, requests: [pending] }), reqTestSummary({ manage: true, access: "auto" }), "", now, 0)
    tkEqual(managed.buttons, ["Tell me too", "Approve", "Decline", "Decline with a reason…", "Mark as added by hand"], "no Delete on the page")
end sub

sub test_requests_title_page_of_a_show()
    now = reqTestNow()
    seasons = [reqTestSeason(0, "missing"), reqTestSeason(1, "library"), reqTestSeason(2, "partial"), reqTestSeason(3, "missing")]
    detail = reqTestDetail({ kind: "show", title: "Breaking Bad", status: "partial", item_id: 99, seasons: seasons, runtime_minutes: invalid, certification: "TV-MA", rating: 8.9, year: 2008 })
    page = reqTitlePage(detail, reqTestSummary({}), "", now, 0)
    tkEqual(page.line, "2008 · 3 seasons · TV-MA · ★ 8.9")
    tkEqual(page.buttons, ["Open", "Request more seasons"])
    tkEqual(page.actions, ["open", "pick"])
    detail.status = "none"
    detail.item_id = invalid
    tkEqual(reqTitlePage(detail, reqTestSummary({}), "", now, 0).actions, ["pick"], "a show's Request opens the picker")
    detail.seasons_left = 0
    tkEqual(reqTitlePage(detail, reqTestSummary({ seasonsLimit: 10 }), "", now, 0).note, "You've asked for all the seasons you can for now.")
    tkFalse(reqCanAskMore([reqTestSeason(0, "missing"), reqTestSeason(1, "unaired")], false), "specials and unaired seasons alone")
    tkTrue(reqCanAskAny([reqTestSeason(1, "unaired")], false))
    declined = reqTestSeason(2, "missing")
    declined.declined = true
    tkFalse(reqSeasonAskable(declined, false))
    tkFalse(reqSeasonAskable(reqTestSeason(2, "missing"), true), "restricted")
    tkEqual(reqFollowSeasons([reqTestSeason(1, "requested"), reqTestSeason(2, "missing"), reqTestSeason(3, "requested")]), [1, 3])
end sub

sub test_requests_season_tiles_say_how_each_stands()
    now = reqTestNow()
    asked = reqTestSeason(3, "requested")
    asked.request_id = 42
    unaired = reqTestSeason(4, "unaired")
    unaired.air_date = "2026-11-20"
    noDate = reqTestSeason(5, "unaired")
    noDate.air_date = invalid
    seasons = [reqTestSeason(0, "missing"), reqTestSeason(1, "library"), reqTestSeason(2, "partial"), asked, unaired, noDate]
    request = reqTestRequest({ id: 42, status: "approved", stage: "downloading", download: { progress: 0.25, state: "downloading", eta: invalid, episodes: 8 } })
    detail = reqTestDetail({ kind: "show", seasons: seasons, requests: [request] })
    tiles = reqSeasonTiles(detail, "http://nas:7539", false, now, 0)
    tkEqual(tiles.Count(), 6)
    tkEqual(tiles[0].title, "Season 1")
    tkEqual(tiles[0].subtitle, "In library")
    tkEqual(tiles[0].badge, "In library")
    tkEqual(tiles[0].image, "http://nas:7539/api/discover/images/poster/inc.jpg?w=220", "the show's poster")
    tkEqual(tiles[1].subtitle, "3 of 8 episodes in library")
    tkEqual(tiles[2].subtitle, "Downloading 25%")
    tkEqual(tiles[2].badge, "Requested")
    tkEqual(tiles[2].progress, 0.25)
    tkEqual(tiles[3].subtitle, "Airs Nov 20")
    tkEqual(tiles[4].subtitle, "Not aired yet")
    tkEqual(tiles[5].title, "Specials", "specials last")
    tkEqual(tiles[5].subtitle, "8 episodes")
    rows = reqDetailRows(detail, "http://nas:7539", false, now, 0)
    tkEqual(rows.Count(), 2)
    tkEqual(rows[1].title, "Cast")
    tkEqual(rows[1].tiles[0].subtitle, "Cobb")
end sub

sub test_requests_picker_ticks_what_may_be_asked()
    now = reqTestNow()
    declined = reqTestSeason(4, "missing")
    declined.declined = true
    seasons = [reqTestSeason(0, "missing"), reqTestSeason(1, "library"), reqTestSeason(2, "partial"), reqTestSeason(3, "missing"), declined, reqTestSeason(5, "missing"), reqTestSeason(6, "unaired"), reqTestSeason(7, "requested")]
    detail = reqTestDetail({ kind: "show", status: "partial", seasons: seasons })
    rows = reqPickerRows(detail, 2, now, 0)
    ticked = []
    askable = []
    for each row in rows
        if row.ticked then ticked.Push(row.number)
        if row.askable then askable.Push(row.number)
    end for
    tkEqual(ticked, [2, 3], "lowest first, as many as may be asked for")
    tkEqual(askable, [2, 3, 5, 6, 0], "unaired seasons and specials offered, specials last")
    tkEqual(rows[0].line, "In library")
    tkEqual(rows[3].line, "Declined")
    tkEqual(rows[6].line, "Requested")
    all = reqPickerRows(detail, -1, now, 0)
    count = 0
    for each row in all
        if row.ticked then count = count + 1
    end for
    tkEqual(count, 3, "no limit: every aired season missing")
    tkEqual(reqPickerButton(0, 5, ""), { label: "", line: "Pick a season." })
    tkEqual(reqPickerButton(1, 5, ""), { label: "Request 1 season", line: "" })
    tkEqual(reqPickerButton(3, -1, ""), { label: "Request 3 seasons", line: "" })
    tkEqual(reqPickerButton(3, 2, "You can ask for 2 more seasons."), { label: "", line: "You can ask for 2 more seasons." })
end sub

sub test_requests_a_library_page_s_request()
    now = reqTestNow()
    summary = reqTestSummary({})
    data = { kind: "show", tmdb_id: 1396, status: "partial", request: reqTestRequest({ status: "approved", stage: "searching" }), askable: [3, 4] }
    shown = reqItemRequest(data, summary, now, 0)
    tkEqual(shown.stage.text, "Searching")
    tkTrue(shown.askMore)
    tkEqual(shown.tmdbId, 1396)
    data.askable = []
    data.request = invalid
    shown = reqItemRequest(data, summary, now, 0)
    tkInvalid(shown.stage)
    tkFalse(shown.askMore)
    data.askable = [3]
    tkFalse(reqItemRequest(data, reqTestSummary({ access: "off", own: 2 }), now, 0).askMore, "may not ask")
    tile = reqAskMoreTile("show", 1396, "Breaking Bad")
    tkEqual(tile.title, "Request more seasons")
    tkEqual(navForTile(tile), { to: "requestTitle", params: { kind: "show", tmdbId: 1396, title: "Breaking Bad", pick: true } })
end sub

sub test_requests_hero()
    now = reqTestNow()
    hero = reqHero({ kind: "movie", tmdb_id: 27205, title: "Inception", year: 2010, overview: "A thief.", backdrop_url: "/api/discover/images/backdrop/inc.jpg", rating: 8.4, status: "none" }, reqTestSummary({}), "http://nas:7539", now)
    tkEqual(hero.line, "2010 · Movie · ★ 8.4")
    tkEqual(hero.actions, ["request", "details"])
    tkEqual(hero.note, "")
    tkEqual(hero.backdrop, "http://nas:7539/api/discover/images/backdrop/inc.jpg?w=640")
    owned = reqHero({ kind: "movie", tmdb_id: 603, title: "The Matrix", status: "library", item_id: 12 }, reqTestSummary({}), "", now)
    tkEqual(owned.buttons, ["Open", "Details"])
    tkEqual(owned.line, "Movie · In library")
    partly = reqHero({ kind: "show", tmdb_id: 1396, title: "Breaking Bad", status: "partial", item_id: 12 }, reqTestSummary({}), "", now)
    tkEqual(partly.buttons, ["Open", "Request more seasons", "Details"])
    tkEqual(partly.actions, ["open", "request", "details"])
    used = reqHero({ kind: "movie", tmdb_id: 27205, title: "Inception", status: "none" }, reqTestSummary({ moviesLeft: 0, moviesNextAt: now + 3 * 86400 }), "", now)
    tkEqual(used.actions, ["details"], "no movie left to ask for: Request goes")
    tkEqual(used.note, "You've asked for all the movies you can for now: you can ask for more in 3 days.", "and its line shows")
    seasons = reqHero({ kind: "show", tmdb_id: 1396, title: "Breaking Bad", status: "none" }, reqTestSummary({ moviesLeft: 0 }), "", now)
    tkEqual(seasons.actions, ["request", "details"], "seasons are left")
    tkEqual(seasons.note, "")
    tkEqual(reqHero({ kind: "show", tmdb_id: 1396, title: "Breaking Bad", status: "partial", item_id: 12 }, reqTestSummary({ seasonsLeft: 0 }), "", now).buttons, ["Open", "Details"], "no season left")
    tkEqual(reqHero({ kind: "movie", tmdb_id: 27205, title: "Inception", status: "none" }, reqTestSummary({ access: "off" }), "", now).note, "", "nothing to say where Discover doesn't show")
end sub

' The hero is the first title of the first list with any: trending's,
' else the next list's once trending came with none or failed.
sub test_requests_hero_comes_from_the_first_list_with_a_title()
    dune = { kind: "movie", tmdb_id: 438631, title: "Dune" }
    arrival = { kind: "movie", tmdb_id: 329865, title: "Arrival" }
    lists = [{ key: "trending", state: "asked", first: invalid }, { key: "movies", state: "shown", first: dune }]
    tkInvalid(reqHeroOf(lists), "trending still comes")
    lists[0].first = arrival
    tkEqual(reqHeroOf(lists), arrival, "trending's first, its later pages still coming")
    lists[0].first = invalid
    lists[0].state = "shown"
    tkEqual(reqHeroOf(lists), dune, "trending had none the profile may ask for")
    lists = [{ key: "trending", state: "shown", first: invalid }, { key: "movies", state: "waiting", first: invalid }, { key: "shows", state: "shown", first: dune }]
    tkInvalid(reqHeroOf(lists), "the lists before it come first")
    lists[1].state = "shown"
    tkEqual(reqHeroOf(lists), dune)
    none = [{ key: "trending", state: "shown", first: invalid }, { key: "requests", state: "waiting", first: invalid }]
    tkEqual(reqHeroOf(none), {}, "no title anywhere: no hero; Your requests aren't waited for")
end sub

sub test_requests_list_queries_and_empty_lists()
    tkEqual(reqListQuery("attention", "", 0, "", 0), { attention: 1, order: "oldest", offset: 0, limit: 50 })
    tkEqual(reqListQuery("mine", "show", 0, "", 50), { mine: 1, kind: "show", offset: 50, limit: 50 })
    tkEqual(reqListQuery("pending", "", 3, "dune", 0), { status: "pending", user_id: 3, q: "dune", offset: 0, limit: 50 })
    tkEqual(reqListQuery("all", "", 0, "", 0), { offset: 0, limit: 50 })
    tkEqual(reqTabs(true)[0], { key: "attention", label: "Needs attention" })
    tkEqual(reqTabs(true).Peek(), { key: "mine", label: "Yours" })
    tkEqual(reqTabs(false).Count(), 6)
    tkEqual(reqEmptyText("pending", true, false), "No requests of yours wait for a decision.")
    tkEqual(reqEmptyText("failed", false, false), "No requests failed.")
    tkEqual(reqEmptyText("all", false, false), "No one has asked for anything yet.")
    tkEqual(reqEmptyText("all", true, false), "You haven't asked for anything yet.")
    tkEqual(reqEmptyText("pending", false, true), "Nothing matches.")
end sub

sub test_requests_tabs_say_how_many_each_holds()
    manager = reqTestSummary({ manage: true, access: "auto", pending: 2, failed: 1, own: 5 })
    counts = { pending: 2, approved: 4, available: 30, declined: 0, failed: 1 }
    tkEqual(reqTabCount("attention", manager, invalid, false, "attention", 3), 3, "what waits, before the counts come")
    tkEqual(reqTabCount("all", manager, counts, false, "attention", 3), 37)
    tkEqual(reqTabCount("approved", manager, counts, false, "attention", 3), 4)
    tkEqual(reqTabCount("declined", manager, counts, false, "attention", 3), 0)
    tkEqual(reqTabCount("all", manager, invalid, false, "all", -1), 0, "not known yet")
    tkEqual(reqTabCount("mine", manager, counts, false, "all", 37), 5, "the account's own")
    ' Filtered: the counts of what the filters keep.
    narrow = { pending: 1, approved: 0, available: 2, declined: 0, failed: 0 }
    tkEqual(reqTabCount("attention", manager, narrow, true, "all", 3), 1)
    tkEqual(reqTabCount("mine", manager, narrow, true, "mine", 2), 2, "what Yours lists")
    tkEqual(reqTabCount("mine", manager, narrow, true, "all", 3), 0, "not known while another tab shows")
    tkEqual(reqTabCount("pending", reqTestSummary({}), { pending: 1 }, false, "all", 1), 1, "a status the answer lacks counts none")
    chips = reqTabChips(true, manager, counts, false, "attention", 3)
    tkEqual(chips[0], { label: "Needs attention", width: 0, count: "3" })
    tkEqual(chips[5].count, "", "none: no number")
    tkEqual(reqTabChips(false, reqTestSummary({}), invalid, false, "all", -1).Count(), 6)
    tkEqual(reqCountText(1200, 999), "999+")
    tkEqual(reqCountText(0, 99), "")
    tkEqual(reqCountText(100, 99), "99+")
end sub

' Needs attention's and Yours' lists count only what they hold: the
' statuses' counts are asked for beside them, with the same filters.
sub test_requests_counts_are_asked_beside_narrow_tabs()
    tkEqual(reqCountsQuery("attention", "movie", 3, "dune"), { kind: "movie", user_id: 3, q: "dune", offset: 0, limit: 1 })
    tkEqual(reqCountsQuery("mine", "", 0, ""), { offset: 0, limit: 1 })
    tkInvalid(reqCountsQuery("all", "", 0, ""), "All's list counts every status")
    tkInvalid(reqCountsQuery("pending", "", 0, ""))
end sub

sub test_requests_open_on_a_tab_the_person_has()
    tkEqual(reqListView("mine", true), "mine")
    tkEqual(reqListView("mine", false), "all")
    tkEqual(reqListView("", true), "attention")
    tkEqual(reqListView("", false), "all")
    tkEqual(reqListView("declined", false), "declined")
    tkEqual(reqListView("attention", false), "all")
end sub

sub test_requests_filters_say_what_they_keep()
    tkEqual(reqFilterLabel("", "", ""), "Filter")
    tkEqual(reqFilterLabel("movie", "", ""), "Movies")
    tkEqual(reqFilterLabel("show", "Ann", "star"), "Shows · Ann · “star”")
    tkEqual(reqFilterLabel("", "", "dune"), "“dune”")
end sub

sub test_requests_ready_notices()
    tkEqual(reqReadyText("Dune", "movie", invalid), "“Dune” is ready to watch.")
    tkEqual(reqReadyText("Severance", "show", [2]), "Season 2 of “Severance” is ready to watch.")
    tkEqual(reqReadyText("Severance", "show", [1, 2]), "Seasons 1–2 of “Severance” are ready to watch.")
    tkEqual(reqReadyText("Severance", "show", invalid), "“Severance” is ready to watch.")
    tkEqual(reqNotice([{ title: "Dune", kind: "movie", seasons: invalid, itemId: 12, poster: "/api/discover/images/poster/d.jpg" }], "http://nas:7539"), { text: "“Dune” is ready to watch.", itemId: 12, poster: "http://nas:7539/api/discover/images/poster/d.jpg?w=80" })
    tkEqual(reqNotice([{ title: "Dune", kind: "movie", seasons: invalid, itemId: 12, poster: "" }], "http://nas:7539").poster, "", "no poster: a tick")
    tkEqual(reqNotice([{}, {}, {}], "http://nas:7539"), { text: "3 of your requests have news", itemId: 0, poster: "" })
end sub

' A notice shows once the remote has rested 1.5 s, and never where it
' would be in the way.
sub test_requests_notices_wait_for_the_remote_to_rest()
    tkFalse(reqRested(0, 0, -1), "a key within the second")
    tkFalse(reqRested(1, -1, -1), "a key 1 to 2 s ago, not known when")
    tkFalse(reqRested(1, 1200, -1))
    tkTrue(reqRested(1, 1500, -1), "the key 1.5 s ago, a look having found a second gone")
    tkTrue(reqRested(2, -1, -1))
    tkFalse(reqRested(5, -1, 800), "a screen changed just now")
    tkTrue(reqRested(5, -1, 1600))
    tkTrue(reqNoticeOver("home"))
    tkTrue(reqNoticeOver("discover"))
    tkTrue(reqNoticeOver("details"))
    for each route in ["player", "livePlayer", "link", "profiles", "requests", "requestSeasons", "sortLibraries"]
        tkFalse(reqNoticeOver(route), route)
    end for
end sub

sub test_requests_news_is_told_once()
    now = reqTestNow()
    mark = { at: 0, ready: {} }
    movie = reqTestRequest({ id: 7, kind: "movie", title: "Dune", status: "available", stage: "available", item_id: 12, updated_at: now - 60 })
    show = reqTestRequest({ id: 8, title: "Severance", status: "approved", stage: "downloading", available_seasons: [1], item_id: 99, updated_at: now - 30 })
    declined = reqTestRequest({ id: 9, status: "declined", stage: "declined", updated_at: now - 20 })
    unopenable = reqTestRequest({ id: 10, kind: "movie", status: "available", stage: "available", item_id: invalid, updated_at: now - 10 })
    told = reqToAnnounce([movie, show, declined, unopenable], mark)
    tkEqual(told.Count(), 2, "ready ones a library it can open has: a declined one shows as the badge")
    tkEqual(told[0].requestId, 7)
    tkEqual(told[0].poster, "/api/discover/images/poster/abc.jpg")
    tkInvalid(told[0].seasons, "the whole movie")
    tkEqual(told[1].seasons, [1])
    tkEqual(mark.at, now - 10)
    tkEqual(mark.ready["8"], "1")
    tkEqual(mark.ready["10"], "all", "kept, so it isn't looked at again")
    tkEqual(reqToAnnounce([movie, show], mark).Count(), 0, "told already")
    show.available_seasons = [1, 2]
    show.updated_at = now
    again = reqToAnnounce([movie, show], mark)
    tkEqual(again.Count(), 1)
    tkEqual(again[0].seasons, [2], "only what is new")
    show.stage = "downloading"
    show.updated_at = now + 60
    tkEqual(reqToAnnounce([show], mark).Count(), 0, "its stage moved on, nothing new to watch")
end sub

sub test_requests_events_tell_news_once()
    mark = { at: 0, ready: {} }
    event = { type: "request_available", request_id: 8, title: "Severance", kind: "show", tmdb_id: 95396, item_id: 99, poster_url: "/api/discover/images/poster/s.jpg", seasons: [1], complete: false }
    first = reqAnnounceEvent(event, mark)
    tkEqual(first.Count(), 1)
    tkEqual(first[0].seasons, [1])
    tkEqual(first[0].itemId, 99)
    tkEqual(first[0].poster, "/api/discover/images/poster/s.jpg")
    tkEqual(reqAnnounceEvent(event, mark).Count(), 0, "the same news")
    event.seasons = [2]
    tkEqual(reqAnnounceEvent(event, mark)[0].seasons, [2])
    tkEqual(mark.ready["8"], "1,2")
    event.complete = true
    whole = reqAnnounceEvent(event, mark)
    tkInvalid(whole[0].seasons)
    tkEqual(reqAnnounceEvent(event, mark).Count(), 0)
    ' The unseen check after the events: nothing to tell twice.
    request = reqTestRequest({ id: 8, title: "Severance", status: "available", stage: "available", available_seasons: [1, 2], item_id: 99, updated_at: reqTestNow() })
    tkEqual(reqToAnnounce([request], mark).Count(), 0)
end sub

sub test_requests_news_is_kept_as_told_once_its_notice_is_seen()
    now = reqTestNow()
    mark = { at: now - 600, ready: { "5": "all" } }
    before = reqMarkCopy(mark)
    movie = reqTestRequest({ id: 7, kind: "movie", title: "Dune", status: "available", stage: "available", item_id: 12, updated_at: now - 60 })
    told = reqToAnnounce([movie], mark)
    tkEqual(told.Count(), 1)
    tkEqual(before, { at: now - 600, ready: { "5": "all" } }, "the copy stays as it was")
    notice = reqNotice(told, "")
    notice.before = before
    kept = reqMarkToKeep(mark, [notice])
    tkFalse(kept.ready.DoesExist("7"), "waiting: not kept as told")
    tkEqual(reqToAnnounce([movie], reqMarkCopy(kept)).Count(), 1, "the app closed first: told again")
    tkEqual(reqMarkToKeep(mark, []).ready["7"], "all", "seen: kept as told")
end sub

sub test_requests_notice_marks_are_kept_per_server_and_profile()
    section = tkRegistrySection()
    mark = { at: 1793534000, ready: {} }
    for i = 1 to 45
        mark.ready[urlValue(i)] = "all"
    end for
    reqSaveAnnounced(section, "http://nas:7539", 2, mark)
    tkEqual(section.flushes, 1)
    back = reqLoadAnnounced(section, "http://nas:7539", 2)
    tkEqual(back.at, 1793534000)
    tkFalse(back.ready.DoesExist("5"), "the oldest go")
    tkTrue(back.ready.DoesExist("6"))
    tkTrue(back.ready.DoesExist("45"))
    tkEqual(reqLoadAnnounced(section, "http://nas:7539", 3), { at: 0, ready: {} }, "another profile's")
    section.Write(reqAnnouncedKey("http://nas:7539", 4), "not json")
    tkEqual(reqLoadAnnounced(section, "http://nas:7539", 4), { at: 0, ready: {} })
end sub

sub test_requests_events_are_known()
    tkTrue(reqIsEvent({ type: "requests_changed" }))
    tkTrue(reqIsEvent({ type: "request_available" }))
    tkTrue(reqIsEvent({ type: "requests_settings_changed" }))
    tkFalse(reqIsEvent({ type: "library_changed" }))
end sub

' French: whole sentences with their own word order, no-break spaces
' (Chr(160)) inside « » and before : and ?, counts in their plural form.
sub test_requests_words_in_french()
    i18nUse("fr")
    nb = Chr(160)
    now = reqTestNow()
    both = reqAllowance({ moviesLeft: 2, seasonsLeft: 5, moviesLimit: 3, seasonsLimit: 10, moviesNextAt: 0, seasonsNextAt: 0 }, now)
    tkEqual(both.text, "Vous pouvez encore demander 2 films et 5 saisons.")
    tkEqual(both.limits, "Limites" + nb + ": 3 films et 10 saisons sur 7 jours.")
    mixed = reqAllowance({ moviesLeft: 1, seasonsLeft: 0, moviesLimit: 3, seasonsLimit: 10, moviesNextAt: 0, seasonsNextAt: now + 86400 }, now)
    tkEqual(mixed.text, "Vous pouvez encore demander 1 film, et d’autres saisons demain.")
    movies = reqAllowance({ moviesLeft: 0, seasonsLeft: -1, moviesLimit: 1, seasonsLimit: -1, moviesNextAt: now + 3 * 86400, seasonsNextAt: 0 }, now)
    tkEqual(movies.text, "Vous avez demandé tous les films possibles pour l’instant" + nb + ": vous pourrez en demander d’autres dans 3 jours.")
    tkEqual(movies.limits, "Limite" + nb + ": 1 film sur 7 jours.")
    tkEqual(reqAskConfirm("Dune", "auto", reqNoSummary(false), now).title, "Demander «" + nb + "Dune" + nb + "»" + nb + "?")
    tkEqual(reqSeasonsLabel([0, 1, 2]), "Saisons 1–2 et épisodes spéciaux")
    severance = "«" + nb + "Severance" + nb + "»"
    tkEqual(reqReadyText("Severance", "show", [2]), "Saison 2 de " + severance + " est prête à regarder.")
    tkEqual(reqReadyText("Severance", "show", [1, 2]), "Saisons 1–2 de " + severance + " sont prêtes à regarder.")
    tkEqual(reqReadyText("Severance", "show", [0]), "Les épisodes spéciaux de " + severance + " sont prêts à regarder.")
    tkEqual(reqNotice([{}, {}, {}], "").text, "3 de vos demandes ont du nouveau")
    tkEqual(reqPickerButton(1, 5, ""), { label: "Demander 1 saison", line: "" })
    tkEqual(reqPickerButton(3, 2, "").line, "Vous pouvez encore demander 2 saisons.")
    tkEqual(reqSeasonState(reqTestSeason(2, "partial"), {}, false, false, now, 0).text, "Déjà 3 épisodes sur 8")
    tkEqual(reqWho(reqTestRequest({}), now, 0), "Demandé par vous · il y a 3 jours")
    seasons = [reqTestSeason(1, "library"), reqTestSeason(2, "missing"), reqTestSeason(3, "missing")]
    tkEqual(reqDetailLine(reqTestDetail({ kind: "show", seasons: seasons, runtime_minutes: invalid, certification: "TV-MA", rating: 8.9, year: 2008 })), "2008 · 3 saisons · TV-MA · ★ 8,9")
end sub

sub test_requests_stages_in_french()
    i18nUse("fr")
    nb = Chr(160)
    now = reqTestNow()
    show = reqTestRequest({ status: "approved", stage: "waiting", stage_at: now + 9 * 3600 + 300 })
    tkEqual(reqStage(show, false, now, 0).text, "Diffusion à 21:05", "24 hours in French")
    show.stage_at = now + 20 * 3600
    tkEqual(reqStage(show, false, now, 0).text, "Diffusion demain à 08:00")
    downloading = reqTestRequest({ status: "approved", stage: "downloading", download: { progress: 0.42, state: "downloading", eta: now + 20 * 60, episodes: 3 } })
    tkEqual(reqStage(downloading, false, now, 0).text, "Téléchargement 42" + nb + "% · dans 20 min")
    partial = reqTestRequest({ status: "approved", stage: "partial", available_seasons: [1, 2] })
    tkEqual(reqStage(partial, false, now, 0).text, "Prêt à regarder" + nb + ": Saisons 1–2")
    failed = reqTestRequest({ status: "failed", stage: "failed", reason: "Sonarr isn't answering", retry_at: now + 300 })
    tkEqual(reqStage(failed, true, now, 0), { text: "Envoi impossible" + nb + ": Sonarr isn't answering", tone: "problem", line: "Nouvel essai dans 5 min.", progress: -1 })
end sub

' Spanish: «» and ¿…?, the tabs' shorter words, and what the server's
' answers become.
sub test_requests_words_in_spanish()
    i18nUse("es")
    now = reqTestNow()
    labels = []
    for each tab in reqTabs(true)
        labels.Push(tab.label)
    end for
    tkEqual(labels, ["Por revisar", "Todas", "En espera", "Aprobadas", "Listas", "Rechazadas", "Fallidas", "Las tuyas"])
    tkEqual(reqSeasonsLabel([2]), "Temporada 2")
    tkEqual(reqSeasonsLabel([3, 1, 2, 5]), "Temporadas 1–3, 5")
    tkEqual(reqEmptyText("pending", true, false), "Ninguna de tus solicitudes espera una decisión.")
    tkEqual(reqAsked(201, { status: "pending" }, "Dune"), "Solicitaste «Dune»: un administrador decidirá.")
    limits = reqTestSummary({ moviesLimit: 1, seasonsLimit: 5 })
    tooMany = { status: 429, error: "too_many_requests", message: "", data: { code: "too_many_requests", retry_at: invalid } }
    tkEqual(reqAskProblem(tooMany, limits, "show", now), "Puedes solicitar como máximo 5 temporadas cada 7 días: elige menos.")
    tkEqual(reqAskProblem(tooMany, limits, "movie", now), "Puedes solicitar como máximo 1 película cada 7 días: elige menos.")
    tkEqual(reqConfirm("withdraw", "Dune", "declined"), { message: "¿Quitar «Dune» de tus solicitudes?", button: "Quitar" })
    actions = []
    for each option in reqActions(reqTestRequest({ status: "pending", actions: ["approve", "decline", "handled", "delete"] }), 0)
        actions.Push(option.label)
    end for
    tkEqual(actions, ["Aprobar", "Rechazar", "Rechazar con un motivo…", "Marcar como añadido a mano", "Eliminar"])
    tkEqual(reqMoreProblemTile({ status: 503, error: "tmdb_down" }, false).title, "TMDB no responde. OK para reintentar.")
    tiles = reqTitleTiles([{ kind: "show", tmdb_id: 1396, title: "Breaking Bad", year: 2008, poster_url: invalid, status: "requested", item_id: invalid }], "", {})
    tkEqual(tiles[0].subtitle, "2008 · Serie")
    tkEqual(tiles[0].badge, "Solicitado")
    tkEqual(reqDeclinedText({ reason: "Not in 4K", at: now, again_at: now + 12 * 86400 }, now), "Rechazado: «Not in 4K». Podrás volver a solicitarlo en 12 días.")
    movie = reqTestRequest({ kind: "movie", status: "approved", stage: "waiting", stage_at: now + 3 * 86400 })
    tkEqual(reqStage(movie, false, now, 0).text, "Se estrena el 4 nov")
    tkEqual(reqSeeAll("", "dune").title, "Resultados de «dune»")
end sub
