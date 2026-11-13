' The one place that talks to the server: the screens' requests, the TV
' code sign-in and the token refreshes. Transfers run side by side on one
' message port; refreshes run one at a time for each server, since a
' refresh token works once and two refreshes with the same one end the
' sign-in. A new pair is written to the registry before it is used. No
' token goes to the server's address before the server there proves it
' holds the sign-in's key (docs/API.md, "Telling servers apart"), and the
' scene's players and pictures get the token only while that holds.

sub init()
    m.top.functionName = "apiRun"
end sub

sub apiRun()
    m.port = CreateObject("roMessagePort")
    m.top.observeField("request", m.port)
    m.session = m.global.session
    m.clock = CreateObject("roTimespan")
    m.serversSection = CreateObject("roRegistrySection", "servers")
    m.tokensSection = CreateObject("roRegistrySection", "tokens")
    ' Transfers under way by identity, requests waiting for a refresh, and
    ' transfers sent again later ({ at, job }).
    m.transfers = {}
    m.waiting = []
    m.later = []
    ' The ticket of the request taken last (apiAccept).
    m.lastTicket = ""
    ' Servers (issuers) with a refresh under way.
    m.refreshing = {}
    ' The access token the last refresh gave, and when: { token, at }.
    m.lastIssued = invalid
    ' When the server in use last proved itself, and on which network
    ' (m.network counts the changes seen): { at, network } of m.clock, or
    ' invalid. doubted: a request got no answer, so the next token waits for
    ' a new proof; proving: that proof is under way; proofRetryAt: when one
    ' that failed is asked again; proofSeq: which proof under way counts (an
    ' answer to an earlier one is for another server or network).
    m.proved = invalid
    m.network = 0
    m.doubted = false
    m.proving = false
    m.proofRetryAt = 0
    m.proofSeq = 0
    ' Joining or leaving a network: the server proves itself again.
    m.device = CreateObject("roDeviceInfo")
    m.device.SetMessagePort(m.port)
    m.device.EnableLinkStatusEvent(true)
    m.server = ""
    m.tokens = invalid
    m.signIn = invalid
    m.signIns = 0
    ' A profile switch waiting for a refresh or another switch to end:
    ' { request, spec }; and whether one is under way.
    m.switchWaiting = invalid
    m.switching = false
    ' A cast's switch held until the profile before it has been heard:
    ' { request, spec, at } (apiSwitchAsked).
    m.switchHeld = invalid
    ' Profile switches made so far: each request is asked for the profile
    ' signed in then (its gen), and goes with that profile's tokens or not
    ' at all (authJobGoes).
    m.switches = 0
    m.profileAsked = 0
    ' What the server said of this Roku's network, for each server
    ' (aqNetworkKept): forgotten with each sign-in and profile.
    m.networkKept = {}
    ' The Live TV guide fetched so far, for the server and the guide's
    ' start it was fetched for: { key, programs, upto } (Live.brs's
    ' liveGuideKeep; upto: how far each channel's is fetched).
    m.liveGuide = { key: "", programs: {}, upto: {} }
    apiDeviceFacts()
    ' The words of its answers, and what it asks the server for
    ' (Accept-Language), in the language the scene shows.
    i18nUse(m.global.language)
    m.global.observeField("language", m.port)
    m.top.ready = true
    while true
        msg = wait(apiWaitMs(), m.port)
        kind = type(msg)
        if kind = "roSGNodeEvent"
            if msg.getField() = "request" then apiAccept(msg.getData())
            if msg.getField() = "language" then i18nUse(msg.getData())
        else if kind = "roUrlEvent"
            apiFinished(msg)
        else if kind = "roDeviceInfoEvent"
            apiNetworkChanged()
        end if
        apiTick()
    end while
end sub

' The User-Agent, the player's id, its name and what Settings shows of
' this Roku.
sub apiDeviceFacts()
    device = CreateObject("roDeviceInfo")
    version = CreateObject("roAppInfo").GetVersion()
    os = device.GetOSVersion()
    model = httpSentence(device.GetModelDisplayName())
    m.userAgent = httpUserAgent(version, os, model)
    ' The player's client_id, so the server sees one player across restarts.
    section = CreateObject("roRegistrySection", "device")
    m.playerId = section.Read("player")
    if m.playerId = ""
        m.playerId = device.GetRandomUUID()
        section.Write("player", m.playerId)
        section.Flush()
    end if
    ' What the user's remotes call this Roku: the name its owner gave it.
    m.deviceName = remoteDeviceName(device.GetFriendlyName())
    m.session.playerId = m.playerId
    m.session.deviceName = m.deviceName
    m.session.userAgent = m.userAgent
    m.session.appVersion = version
    m.session.device = model + " · Roku OS " + httpOsVersion(os)
end sub

function apiNow() as integer
    return CreateObject("roDateTime").AsSeconds()
end function

' Timers need a tick while transfers, a sign-in or a later transfer are
' under way.
function apiWaitMs() as integer
    if m.transfers.Count() > 0 or m.signIn <> invalid or m.later.Count() > 0 or m.switchHeld <> invalid then return 250
    if m.tokens <> invalid and m.tokens.proof <> invalid then return 30000
    return 0
end function

function apiOk(data as dynamic) as object
    return { ok: true, status: 200, data: data, error: "", message: "" }
end function

function apiFail(code as string, message as string) as object
    return { ok: false, status: 0, data: invalid, error: code, message: message }
end function

function apiSignedOut() as object
    return { ok: false, status: 401, data: invalid, error: "signed_out", message: t("common.errors.signedOut") }
end function

sub apiAnswer(request as dynamic, answer as object)
    if request <> invalid then request.result = answer
end sub

sub apiAccept(request as dynamic)
    if request = invalid then return
    ' A Roku tells of the request field again when the node it holds
    ' changes (its result, its content, a sign-in's updates): taken a
    ' second time, a request would go again with each answer, without end.
    ' The screens hand over a new node for each request, with its own
    ' ticket.
    ticket = request.ticket
    if ticket <> "" and ticket = m.lastTicket then return
    m.lastTicket = ticket
    spec = request.spec
    if spec = invalid then spec = {}
    op = httpString(spec.op)
    if op = "boot"
        apiBoot(request)
    else if op = "listServers"
        apiAnswer(request, apiOk({ servers: apiSavedServers() }))
    else if op = "useServer"
        apiUseServer(request, spec)
    else if op = "forgetServer"
        apiForgetServer(request, spec)
    else if op = "checkServer"
        apiCheckServer(request, spec)
    else if op = "signIn"
        apiSignInStart(request)
    else if op = "signInStop"
        m.signIn = invalid
        apiAnswer(request, apiOk(invalid))
    else if op = "signOut"
        apiSignOut(request)
    else if op = "play"
        apiPlay(request, spec)
    else if op = "network"
        apiNetwork(request)
    else if op = "tellNetwork"
        apiTellNetwork(request, spec)
    else if op = "speedTest"
        apiSpeedTest(request, spec)
    else if op = "switchProfile"
        apiSwitchAsked(request, spec)
    else if op = "liveWatch"
        apiLiveWatch(request, spec)
    else if op = "livePage"
        apiLivePage(request, spec)
    else if op = "liveGuide"
        apiLiveGuide(request, spec, spec.channels)
    else if op = "serverInfo"
        if not m.session.serverHeard then apiAskServerInfo()
        apiAnswer(request, apiOk(invalid))
    else
        apiCall(request, spec)
    end if
end sub

' --- Transfers ---

' A transfer to send; purpose says what its answer is for, gen the
' profile it is asked for.
function apiJob(purpose as string, method as string, url as string) as object
    return { purpose: purpose, method: method, url: url, body: "", contentType: "", auth: false, sent: "", retried: false, request: invalid, server: m.server, timeoutMs: 30000, play: false, network: "", toFile: "", gen: m.switches }
end function

function apiForm(job as object, params as object) as object
    job.body = urlQuery(params)
    job.contentType = "application/x-www-form-urlencoded"
    return job
end function

' A request of the screens: { method, path, query, json, jsonText, auth,
' timeoutMs, build, known, manage, badges, bitrate }, to the current
' server, with the token unless auth is false. jsonText is a body made into
' JSON on the render thread, for one with nulls (a field's associative
' array may not keep invalid). build ("rows", "tiles", "seasons",
' "episodes") makes the answer's list into ContentNodes here, off the
' render thread (apiBuild); for "rows", known holds the signatures of the
' rows Home shows, which aren't built again (the answer names the hero's
' title too), "homeInfo" reduces a title's details to what Home says of it,
' and for "discoverTiles" the
' titles shown already ({ "movie:603": true }); for "tiles", badges and
' bitrate say what a library's posters show of their files
' (contentTiles). manage: the requests are a manager's, who sees why one
' failed.
sub apiCall(request as object, spec as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    method = UCase(httpString(spec.method))
    if method = "" then method = "GET"
    job = apiJob("call", method, m.server + urlWithQuery(httpString(spec.path), spec.query))
    job.request = request
    job.build = httpString(spec.build)
    job.known = spec.known
    job.manage = httpTrue(spec.manage)
    job.badges = httpTrue(spec.badges)
    job.bitrate = httpTrue(spec.bitrate)
    job.play = httpTrue(spec.play)
    job.profile = httpTrue(spec.profile)
    job.auth = true
    if GetInterface(spec.auth, "ifBoolean") <> invalid then job.auth = spec.auth
    if spec.json <> invalid
        job.body = FormatJson(spec.json)
        job.contentType = "application/json"
    else if httpString(spec.jsonText) <> ""
        job.body = spec.jsonText
        job.contentType = "application/json"
    end if
    timeout = httpNumber(spec.timeoutMs, 0)
    if timeout > 0 then job.timeoutMs = timeout
    apiSend(job)
end sub

sub apiSend(job as object)
    if not authJobGoes(job, m.switches)
        ' Asked for the profile before a switch: never sent as the new one.
        apiAnswer(job.request, apiProfileChanged())
        return
    end if
    if job.auth
        decision = authBeforeRequest(m.tokens, apiNow(), m.refreshing.DoesExist(m.server))
        if decision = "signed_out"
            ' The token ran out with nothing to refresh it: the scene shows
            ' sign-in.
            if m.tokens <> invalid then apiEnd("ended")
            apiAnswer(job.request, apiSignedOut())
            return
        end if
        if decision <> "send"
            m.waiting.Push(job)
            if decision = "refresh" then apiRefresh()
            return
        end if
        if not apiTokenMayGo()
            m.waiting.Push(job)
            apiProve(m.tokens)
            return
        end if
        job.sent = m.tokens.access
    end if
    transfer = transferNew(m.transfers)
    transfer.SetMessagePort(m.port)
    transfer.SetUrl(job.url)
    transfer.RetainBodyOnError(true)
    transfer.EnableEncodings(true)
    if LCase(Left(job.url, 8)) = "https://" then transfer.SetCertificatesFile("common:/certs/ca-bundle.crt")
    transfer.AddHeader("User-Agent", m.userAgent)
    transfer.AddHeader("Accept", "application/json")
    ' What the server writes itself (Home's rows) comes in the language shown.
    transfer.AddHeader("Accept-Language", i18nLanguage())
    ' The token goes in this header only, never in an address.
    if job.auth then transfer.AddHeader("Authorization", "Bearer " + job.sent)
    if job.contentType <> "" then transfer.AddHeader("Content-Type", job.contentType)
    transfer.SetRequest(job.method)
    if job.toFile <> ""
        started = transfer.AsyncGetToFile(job.toFile)
    else if job.method = "GET"
        started = transfer.AsyncGetToString()
    else
        started = transfer.AsyncPostFromString(job.body)
    end if
    if not started
        apiAnswered(job, -1, "", invalid)
        return
    end if
    job.transfer = transfer
    job.startedAt = m.clock.TotalMilliseconds()
    m.transfers[transfer.GetIdentity().ToStr()] = job
end sub

sub apiFinished(msg as object)
    if msg.GetInt() <> 1 then return
    key = msg.GetSourceIdentity().ToStr()
    job = m.transfers[key]
    if job = invalid then return
    m.transfers.Delete(key)
    apiAnswered(job, msg.GetResponseCode(), msg.GetString(), msg.GetResponseHeaders())
end sub

sub apiAnswered(job as object, status as integer, body as string, headers as dynamic)
    purpose = job.purpose
    if purpose = "call"
        apiCallAnswered(job, status, body, headers)
    else if purpose = "refresh"
        apiRefreshAnswered(job, status, body)
    else if purpose = "check"
        apiCheckAnswered(job, status, body)
    else if purpose = "metadata"
        apiMetadataAnswered(job, status, body)
    else if purpose = "device"
        apiDeviceAnswered(job, status, body)
    else if purpose = "poll"
        apiPollAnswered(job, status, body)
    else if purpose = "switch"
        apiSwitchAnswered(job, status, body)
    else if purpose = "proof"
        apiProofAnswered(job, status, body)
    else if purpose = "serverInfo"
        apiServerInfoAnswered(job, status, body)
    else if purpose = "revoke"
        ' The app signed out whatever the answer; a busy server is asked
        ' once more.
        if authRevokeAgain(status, job.retried)
            job.retried = true
            m.later.Push({ at: m.clock.TotalMilliseconds() + 3000, job: job })
        end if
    end if
end sub

' Transfers that hang are cancelled, later ones go when due, and the
' device code is polled on time.
sub apiTick()
    now = m.clock.TotalMilliseconds()
    for each key in m.transfers.Keys()
        job = m.transfers[key]
        if now - job.startedAt > job.timeoutMs
            job.transfer.AsyncCancel()
            m.transfers.Delete(key)
            apiAnswered(job, -28, "", invalid)
        end if
    end for
    if m.later.Count() > 0
        due = []
        waiting = []
        for each entry in m.later
            if now >= entry.at
                due.Push(entry.job)
            else
                waiting.Push(entry)
            end if
        end for
        m.later = waiting
        for each job in due
            apiSend(job)
        end for
    end if
    apiPollWhenDue(now)
    apiRenewProof(now)
    apiSwitchWhenHeard(now)
end sub

sub apiCallAnswered(job as object, status as integer, body as string, headers as dynamic)
    if status <= 0 and job.auth and job.server = m.server and m.tokens <> invalid and m.tokens.proof <> invalid
        ' The Roku may be on another network now.
        m.doubted = true
        apiPublish()
        apiProve(m.tokens)
    end if
    ' A switch stops the tokens it replaces at once: what they were sent
    ' with isn't taken for the sign-in's end. It waits for the switch to
    ' end, and goes again only if the profile stayed (apiSend drops it
    ' otherwise): a request asked for one profile never goes as another.
    if status = 401 and job.auth and job.server = m.server and m.switching and not job.retried
        job.retried = true
        m.waiting.Push(job)
        return
    end if
    if status = 401 and job.auth and job.server = m.server
        challenge = invalid
        if GetInterface(headers, "ifAssociativeArray") <> invalid then challenge = headers["www-authenticate"]
        decision = authAfterRefusal(job.sent, httpInvalidToken(challenge), job.retried, m.tokens, m.lastIssued, apiNow(), m.refreshing.DoesExist(m.server))
        if decision = "ended"
            apiEnd("ended")
            apiAnswer(job.request, apiSignedOut())
            return
        end if
        if decision <> "answer"
            job.retried = true
            if decision = "resend"
                apiSend(job)
            else
                m.waiting.Push(job)
                if decision = "refresh" then apiRefresh()
            end if
            return
        end if
    end if
    if job.toFile <> ""
        apiSpeedTestAnswered(job, status)
        return
    end if
    answer = httpAnswer(status, body)
    if job.network <> "" then aqNetworkKeep(m.networkKept, job.network, answer, m.clock.TotalMilliseconds())
    if httpTrue(job.me) then apiKnowProfile(job, answer)
    if httpTrue(job.profile) then apiProfileLanguage(job, answer)
    if answer.ok and job.play and apiAbandoned(job.request)
        ' The player left before its session came: the server would go on
        ' preparing it.
        apiCloseSession(answer.data, job.gen)
        return
    end if
    if answer.ok and httpTrue(job.liveWatch) and apiAbandoned(job.request)
        ' Zapped on or left before the channel came: its connection goes.
        apiLeaveChannel(answer.data, job.gen)
        return
    end if
    if job.build = "livePage" or job.build = "liveGuide"
        apiLiveAnswered(job, answer)
        return
    end if
    if answer.ok and job.build <> "" then apiBuild(job, answer)
    apiAnswer(job.request, answer)
end sub

function apiAbandoned(request as dynamic) as boolean
    if request = invalid then return false
    return request.abandoned = true
end function

' DELETE /api/playback/{session} of a PlaybackInfo; nothing waits for it.
' gen: the profile the session was asked for, whose tokens end it.
sub apiCloseSession(info as dynamic, gen as integer)
    data = httpObject(info)
    if data = invalid then return
    sessionId = httpString(data.session_id)
    if sessionId = "" then return
    job = apiJob("close", "DELETE", m.server + "/api/playback/" + urlEncode(sessionId))
    job.auth = true
    job.gen = gen
    apiSend(job)
end sub

' The list of an answer as ContentNodes, in the request's content; the
' answer keeps a summary instead of the JSON, so little crosses over to
' the render thread.
sub apiBuild(job as object, answer as object)
    data = httpObject(answer.data)
    if data = invalid then data = {}
    server = job.server
    if job.build = "rows"
        ' Rows Home shows as they are aren't made again.
        changes = contentRowChanges(data.hubs, server, job.known)
        job.request.content = nodesRows(changes.changed)
        ' What My List holds, for the Options key on other screens.
        listed = invalid
        for each hub in httpArray(data.hubs)
            if GetInterface(hub, "ifAssociativeArray") <> invalid and httpString(hub.key) = "my_list"
                listed = []
                for each item in httpArray(hub.items)
                    if GetInterface(item, "ifAssociativeArray") <> invalid then listed.Push(httpNumber(item.id, 0))
                end for
            end if
        end for
        if listed = invalid then listed = []
        ' The hero's title, and what the rows let it say before its details.
        hero = invalid
        heroItem = homeHeroItem(data.hubs)
        if heroItem <> invalid then hero = homeInfo(heroItem, server, false)
        answer.data = { rows: changes.summary, myList: listed, hero: hero }
    else if job.build = "homeInfo"
        ' A title's details as Home's hero and info block say them.
        answer.data = homeInfo(data, server, true)
    else if job.build = "tiles"
        items = httpArray(data.items)
        tiles = contentTiles(items, server, job.badges, job.bitrate)
        job.request.content = nodesContent(tiles)
        ' listed: the page's items (the next page's offset), shown or not.
        answer.data = { total: httpNumber(data.total, -1), listed: items.Count(), shown: tiles.Count() }
    else if job.build = "searchRows"
        ' Search's results in rows by kind (Search.brs), the Live TV
        ' channels among them as cards; the channels themselves too, for
        ' the player.
        rows = searchRows(data, server, apiNow(), apiTwelveHour())
        job.request.content = nodesRows(rows)
        keys = []
        for each row in rows
            keys.Push(row.key)
        end for
        answer.data = { rows: keys, channels: liveChannels(data.channels, server) }
    else if job.build = "pictures"
        ' A picture subtitle track's list, read here: a film's runs to a
        ' few hundred KB (Pictures.brs).
        answer.data = picRead(data)
    else if job.build = "letters"
        ' A page of a library sorted by title, for its A to Z: where each
        ' letter's titles start, and no ContentNodes.
        items = httpArray(data.items)
        answer.data = { total: httpNumber(data.total, -1), listed: items.Count(), runs: libLetterRuns(items) }
    else if job.build = "detail"
        ' The rows that come with a title's details; the details stay.
        rows = pageDetailRows(data, server)
        job.request.content = nodesRows(rows)
        keys = []
        for each row in rows
            keys.Push({ key: row.key, wide: row.wide })
        end for
        answer.rows = keys
    else if job.build = "seasonTiles"
        tiles = pageSeasonTiles(data.items, server)
        job.request.content = nodesContent(tiles)
        answer.data = { shown: tiles.Count() }
    else if job.build = "collections"
        tiles = pageCollectionTiles(data.collections, server)
        job.request.content = nodesContent(tiles)
        answer.data = { shown: tiles.Count() }
    else if job.build = "person"
        ' A person's page: a row of titles per role.
        page = pagePerson(data, server)
        job.request.content = nodesRows(page.rows)
        answer.data = { name: page.name, photo: page.photo, initials: page.initials, line: page.line, shown: page.rows.Count() }
    else if job.build = "playlists"
        tiles = pagePlaylistTiles(data.playlists, server)
        job.request.content = nodesContent(tiles)
        answer.data = { shown: tiles.Count() }
    else if job.build = "playlist"
        rows = pagePlaylistEntries(data.entries, server)
        job.request.content = nodesContent(rows)
        ' A music playlist's tracks go to the music player.
        kind = httpString(data.kind)
        queue = pagePlaylistQueue(data.entries, server)
        if kind = "audio" then queue = musicTracks(data.entries, server)
        answer.data = { title: httpSentence(data.title), line: pagePlaylistLine(data), kind: kind, smart: httpString(data.smart_filter) <> "", queue: queue, shown: rows.Count() }
    else if job.build = "history"
        entries = httpArray(data.entries)
        rows = pageHistoryRows(entries, server, apiNow())
        job.request.content = nodesContent(rows)
        answer.data = { total: httpNumber(data.total, -1), listed: entries.Count(), shown: rows.Count() }
    else if job.build = "tracks"
        tracks = musicTracks(data.items, server)
        job.request.content = nodesContent(musicDiscRows(tracks))
        answer.data = { queue: tracks, shown: tracks.Count() }
    else if job.build = "trackQueue"
        ' An album's tracks for the music player alone: an artist's Play
        ' gathers each album's.
        answer.data = { queue: musicTracks(data.items, server) }
    else if job.build = "albums"
        ' An artist's albums, newest first; Play plays them in the
        ' server's order.
        albums = musicArtistAlbums(data.items, server)
        job.request.content = nodesContent(albums.tiles)
        answer.data = { order: albums.order, shown: albums.tiles.Count() }
    else if job.build = "collection"
        ' A collection's page: its titles as tiles, what it says of itself
        ' and what Play plays.
        page = pageCollection(data, server)
        job.request.content = nodesContent(page.tiles)
        page.Delete("tiles")
        answer.data = page
    else if job.build = "seasons"
        seasons = contentSeasons(data.items)
        job.request.content = nodesContent(seasons)
        answer.data = { size: seasons.Count() }
    else if job.build = "episodes"
        episodes = contentEpisodes(data.items, server)
        job.request.content = nodesContent(episodes)
        answer.data = { size: episodes.Count() }
    else if job.build = "discoverTiles"
        ' A page of TMDB's titles (a list, search), those shown already
        ' left out (each tile's key names it, for the next page), and the
        ' first of them whole (Discover's hero).
        seen = httpObject(job.known)
        if seen = invalid then seen = {}
        results = httpArray(data.results)
        tiles = reqTitleTiles(results, server, seen)
        job.request.content = nodesContent(tiles)
        first = {}
        for each title in results
            if tiles.Count() > 0 and GetInterface(title, "ifAssociativeArray") <> invalid and reqKey(httpString(title.kind), httpNumber(title.tmdb_id, 0)) = tiles[0].key
                first = title
                exit for
            end if
        end for
        answer.data = { page: httpNumber(data.page, 1), totalPages: httpNumber(data.total_pages, 0), listed: results.Count(), shown: tiles.Count(), first: first }
    else if job.build = "discoverDetail"
        ' A TMDB title's rows, its seasons and its cast; the details stay.
        rows = reqDetailRows(data, server, job.manage, apiNow(), fmtZoneSeconds())
        job.request.content = nodesRows(rows)
        keys = []
        for each row in rows
            keys.Push(row.key)
        end for
        answer.rows = keys
    else if job.build = "requestRows"
        ' A page of requests, as rows; busy while one of them downloads or
        ' goes into a library, when the list is asked again every 10 s.
        requests = httpArray(data.requests)
        now = apiNow()
        zone = fmtZoneSeconds()
        rows = []
        busy = false
        for each request in requests
            if GetInterface(request, "ifAssociativeArray") <> invalid
                rows.Push(reqRow(request, server, job.manage, now, zone))
                if reqBusy(httpString(request.stage)) then busy = true
            end if
        end for
        job.request.content = nodesContent(rows)
        counts = httpObject(data.counts)
        if counts = invalid then counts = {}
        answer.data = { total: httpNumber(data.total, -1), counts: counts, people: reqPeople(data.people), listed: requests.Count(), shown: rows.Count(), busy: busy }
    else if job.build = "requestTiles"
        ' Requests as tiles (Discover's Your requests).
        requests = httpArray(data.requests)
        now = apiNow()
        zone = fmtZoneSeconds()
        tiles = []
        busy = false
        for each request in requests
            if GetInterface(request, "ifAssociativeArray") <> invalid
                tiles.Push(reqRequestTile(request, server, job.manage, now, zone))
                if reqBusy(httpString(request.stage)) then busy = true
            end if
        end for
        job.request.content = nodesContent(tiles)
        answer.data = { total: httpNumber(data.total, -1), shown: tiles.Count(), busy: busy }
    end if
end sub

' Whether this Roku shows times on a 12-hour clock.
function apiTwelveHour() as boolean
    return CreateObject("roDeviceInfo").GetClockFormat() <> "24h"
end function

' --- Tokens ---

sub apiRefresh()
    tokens = m.tokens
    if tokens = invalid or m.refreshing.DoesExist(tokens.issuer) then return
    if tokens.refresh = ""
        apiEnd("ended")
        return
    end if
    m.refreshing[tokens.issuer] = true
    job = apiForm(apiJob("refresh", "POST", tokens.tokenEndpoint), { grant_type: "refresh_token", refresh_token: tokens.refresh, client_id: httpClientId() })
    job.tokens = tokens
    ' An answer given up on can be a refresh token the server has spent:
    ' a refresh is waited for much longer than other requests.
    job.timeoutMs = 120000
    ' The refresh token goes only to a server that proves itself now.
    apiSendProved(tokens, job)
end sub

sub apiRefreshAnswered(job as object, status as integer, body as string)
    old = job.tokens
    m.refreshing.Delete(old.issuer)
    ' Still the tokens in use: the same server, not signed in again since.
    current = m.tokens <> invalid and m.tokens.issuer = old.issuer and m.tokens.refresh = old.refresh
    ' Still the tokens kept for that server: not signed out, the server not
    ' forgotten, no code sign-in since.
    kept = authRefreshKept(tokensLoad(m.tokensSection, old.issuer), old)
    outcome = authRefreshOutcome(status, body)
    fresh = invalid
    reason = "ended"
    if outcome = "ok"
        answer = authParseTokens(body, old.scope, apiNow())
        ' Unreadable, the old refresh token spent: the sign-in is over.
        if answer.ok then fresh = tokensFrom(answer, old.issuer, old.tokenEndpoint, old.revocationEndpoint, old)
        if fresh <> invalid and authExtraScopes(fresh.scope, old.scope).Count() > 0
            apiRevoke(fresh)
            fresh = invalid
            reason = "wider_scope"
        end if
    end if
    if outcome = "failed"
        ' The network or the server: the tokens stay, the waiting requests fail.
        if current then apiFailWaiting(httpAnswer(status, body))
    else if fresh = invalid
        if current
            apiEnd(reason)
        else if kept
            tokensWipe(m.tokensSection, old.issuer)
        end if
    else if not kept
        ' Ended here while the refresh was under way: so are its tokens, and
        ' what replaced them stays.
        apiRevoke(fresh)
    else
        ' Saved before it is used: the old refresh token is spent.
        tokensSave(m.tokensSection, fresh)
        if current
            m.tokens = fresh
            m.lastIssued = { token: fresh.access, at: apiNow() }
            apiPublish()
        end if
    end if
    ' What waits goes again, with the new token; when these tokens were
    ' replaced meanwhile (signed in again), with the ones in use.
    apiSendWaiting()
    ' A profile switch asked for meanwhile goes now, with the new pair.
    if m.switchWaiting <> invalid
        waiting = m.switchWaiting
        m.switchWaiting = invalid
        apiSwitchProfile(waiting.request, waiting.spec)
    end if
end sub

sub apiSendWaiting()
    jobs = m.waiting
    m.waiting = []
    for each waiting in jobs
        apiSend(waiting)
    end for
end sub

' The sign-in is over (ended by the server, or more than was asked): the
' tokens go, and the screens show sign-in.
sub apiEnd(reason as string)
    if m.tokens <> invalid then tokensWipe(m.tokensSection, m.tokens.issuer)
    m.tokens = invalid
    m.lastIssued = invalid
    m.proved = invalid
    apiForgetProof()
    m.networkKept = {}
    apiFailWaiting(apiSignedOut())
    apiPublish()
    m.session.signOutReason = reason
    m.session.signedIn = false
end sub

sub apiFailWaiting(answer as object)
    jobs = m.waiting
    m.waiting = []
    for each job in jobs
        apiAnswer(job.request, answer)
    end for
    if m.switchWaiting <> invalid
        apiAnswer(m.switchWaiting.request, answer)
        m.switchWaiting = invalid
    end if
    if m.switchHeld <> invalid
        apiAnswer(m.switchHeld.request, answer)
        m.switchHeld = invalid
    end if
end sub

' The answer to a request asked for the profile before a switch, dropped.
function apiProfileChanged() as object
    return apiFail("profile_changed", t("signin.problems.profileChanged"))
end function

' Ends the grant of tokens on its server (RFC 7009); nothing waits for it.
' A server that can't prove it holds their key gets nothing: the person can
' sign the Roku out in Account → Apps.
sub apiRevoke(tokens as object)
    token = tokens.refresh
    if token = "" then token = tokens.access
    apiSendProved(tokens, apiForm(apiJob("revoke", "POST", tokens.revocationEndpoint), { token: token, client_id: httpClientId() }))
end sub

sub apiSignOut(request as object)
    tokens = m.tokens
    if tokens <> invalid
        apiRevoke(tokens)
        tokensWipe(m.tokensSection, tokens.issuer)
    end if
    m.tokens = invalid
    m.lastIssued = invalid
    m.proved = invalid
    apiForgetProof()
    m.networkKept = {}
    apiFailWaiting(apiSignedOut())
    apiPublish()
    m.session.signOutReason = "signed_out"
    m.session.signedIn = false
    apiAnswer(request, apiOk(invalid))
end sub

' --- Servers ---

function apiSavedServers() as object
    return serversParse(m.serversSection.Read("list"))
end function

sub apiBoot(request as object)
    servers = apiSavedServers()
    current = urlOrigin(m.serversSection.Read("current"))
    if current <> invalid
        name = urlDisplayAddress(current)
        for each server in servers
            if server.url = current then name = server.name
        end for
        apiSelect(current, name)
    end if
    ' Straight in as the profile signed in when it has no PIN; else "Who's
    ' watching?" first when the household had more than one profile, on
    ' that profile's PIN pad (pin) when Settings' "Always start as" names
    ' one of them.
    atStart = ""
    if m.tokens <> invalid
        household = profKeptHousehold(CreateObject("roRegistrySection", "household"), m.server)
        startAs = profStartAs(CreateObject("roRegistrySection", "startAs"), m.server)
        atStart = profAtStart(m.tokens.scope, household, startAs)
    end if
    screen = serversStartScreen(m.server, m.tokens <> invalid, atStart <> "")
    apiAnswer(request, apiOk({ screen: screen, pin: atStart = "pin", servers: servers }))
end sub

' Makes url the server the screens talk to, signed in when it has tokens.
sub apiSelect(url as string, name as string)
    ' The language facts were the server before's, or are asked again.
    m.session.accountHeard = false
    m.session.accountLanguage = ""
    m.session.serverHeard = false
    m.session.serverLanguage = ""
    if url <> m.server
        apiFailWaiting(apiFail("server_changed", t("signin.problems.serverChanged")))
        ' The token in use goes before the address changes: the event
        ' stream follows the two apart (MainScene's onStream), so the server
        ' before's token never goes to the one chosen, proved or not.
        m.tokens = invalid
        apiPublish()
    end if
    m.signIn = invalid
    m.server = url
    m.tokens = invalid
    if url <> "" then m.tokens = tokensLoad(m.tokensSection, url)
    m.lastIssued = invalid
    m.proved = invalid
    m.doubted = false
    apiForgetProof()
    m.session.server = url
    m.session.serverName = name
    m.session.signOutReason = ""
    if m.tokens <> invalid
        m.session.scope = m.tokens.scope
    else
        m.session.scope = ""
    end if
    apiPublish()
    m.session.signedIn = m.tokens <> invalid
    apiAskServerInfo()
    apiAskProfile()
end sub

' The server's language (GET /api/server): what a Roku in a language Pley
' doesn't have shows, and whether the server keeps an account's (a Pley
' from before languages names none). Anyone may ask, so it goes without the
' token; the scene asks again as the event stream comes up when it got no
' answer.
sub apiAskServerInfo()
    if m.server = "" then return
    job = apiJob("serverInfo", "GET", m.server + "/api/server")
    job.timeoutMs = 10000
    apiSend(job)
end sub

sub apiServerInfoAnswered(job as object, status as integer, body as string)
    if job.server <> m.server then return
    data = httpObject(httpJson(body))
    if status <> 200 or data = invalid or httpString(data.version) = "" then return
    m.session.serverLanguage = httpString(data.language)
    m.session.serverHeard = true
end sub

' Who the sign-in acts as (GET /api/auth/me), for the screens: asked as a
' server is chosen signed in, and after each sign-in.
sub apiAskProfile()
    m.networkKept = {}
    m.session.profileId = 0
    m.session.profileName = ""
    ' Another profile, maybe with another language: the one shown stays
    ' until it is heard.
    m.session.accountHeard = false
    m.session.accountLanguage = ""
    if m.tokens = invalid then return
    job = apiJob("call", "GET", m.server + "/api/auth/me")
    job.auth = true
    job.build = ""
    job.me = true
    m.profileAsked = m.profileAsked + 1
    job.asked = m.profileAsked
    apiSend(job)
end sub

' The profile, unless another sign-in came meanwhile; whether it has a PIN
' is kept for the next start.
sub apiKnowProfile(job as object, answer as object)
    if not answer.ok or job.asked <> m.profileAsked then return
    data = httpObject(answer.data)
    if data = invalid then return
    m.session.profileId = httpNumber(data.id, 0)
    m.session.profileName = httpSentence(data.name)
    m.session.accountLanguage = httpString(data.language)
    m.session.accountHeard = true
    profKeepPin(CreateObject("roRegistrySection", "household"), m.server, data)
end sub

' A request that answers with the profile (spec.profile: PUT
' /api/me/language): the account's language as it is now, unless another
' profile or server came meanwhile.
sub apiProfileLanguage(job as object, answer as object)
    if not answer.ok or job.gen <> m.switches or job.server <> m.server then return
    data = httpObject(answer.data)
    if data = invalid or not data.DoesExist("language") then return
    m.session.accountLanguage = httpString(data.language)
    m.session.accountHeard = true
end sub

sub apiUseServer(request as object, spec as object)
    url = urlOrigin(httpString(spec.url))
    if url = invalid
        apiAnswer(request, apiFail("address", t("signin.problems.notAnAddress")))
        return
    end if
    name = httpSentence(spec.name)
    if name = "" then name = urlDisplayAddress(url)
    servers = serversWith(apiSavedServers(), { url: url, name: name, id: httpString(spec.id), lastUsed: apiNow() })
    m.serversSection.Write("list", serversSerialize(servers))
    m.serversSection.Write("current", url)
    m.serversSection.Flush()
    apiSelect(url, name)
    apiAnswer(request, apiOk({ signedIn: m.tokens <> invalid }))
end sub

sub apiForgetServer(request as object, spec as object)
    url = httpString(spec.url)
    tokens = tokensLoad(m.tokensSection, url)
    if tokens <> invalid
        apiRevoke(tokens)
        tokensWipe(m.tokensSection, url)
    end if
    servers = serversWithout(apiSavedServers(), url)
    m.serversSection.Write("list", serversSerialize(servers))
    if m.serversSection.Read("current") = url then m.serversSection.Delete("current")
    m.serversSection.Flush()
    if m.server = url then apiSelect("", "")
    apiAnswer(request, apiOk({ servers: servers }))
end sub

' Whether what someone typed is a pley server: GET /api/server at each
' address it could be, in turn.
sub apiCheckServer(request as object, spec as object)
    address = httpString(spec.address).Trim()
    candidates = urlCandidates(address)
    if candidates.Count() = 0
        apiAnswer(request, apiFail("address", t("signin.problems.badAddress")))
        return
    end if
    job = apiJob("check", "GET", candidates[0] + "/api/server")
    ' Each address it could be gets a few seconds, not a request's 30.
    job.timeoutMs = 8000
    job.request = request
    job.candidates = candidates
    job.index = 0
    job.address = address
    job.answered = false
    apiSend(job)
end sub

sub apiCheckAnswered(job as object, status as integer, body as string)
    server = job.candidates[job.index]
    data = httpObject(httpJson(body))
    if status = 200 and data <> invalid and httpString(data.version) <> ""
        if httpTrue(data.setup_required)
            apiAnswer(job.request, apiFail("setup", t("signin.problems.notSetUp", { address: urlDisplayAddress(server) })))
        else if urlUnencrypted(server)
            apiAnswer(job.request, apiFail("unencrypted", t("signin.problems.unencryptedAddress", { address: job.address })))
        else
            name = httpSentence(data.name)
            if name = "" then name = urlDisplayAddress(server)
            apiAnswer(job.request, apiOk({ url: server, name: name, version: data.version }))
        end if
        return
    end if
    if status > 0 then job.answered = true
    job.index = job.index + 1
    if job.index < job.candidates.Count()
        job.url = job.candidates[job.index] + "/api/server"
        apiSend(job)
    else if job.answered
        apiAnswer(job.request, apiFail("not_pley", t("signin.problems.notPley", { address: job.address })))
    else
        apiAnswer(job.request, apiFail("unreachable", t("signin.problems.unreachableAddress", { address: job.address })))
    end if
end sub

' --- Signing in with a TV code (RFC 8628) ---

' The request's result is the code to show (or why there is none); its
' updates say how the sign-in goes: waiting, retrying, denied, expired,
' invalid, failed, signed_in.
sub apiSignInStart(request as object)
    m.signIn = invalid
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    if urlUnencrypted(m.server)
        apiAnswer(request, apiFail("unencrypted", t("signin.problems.unencrypted")))
        return
    end if
    m.signIns = m.signIns + 1
    m.signIn = { id: m.signIns, request: request, server: m.server, meta: invalid, scope: httpScope(), code: invalid, intervalMs: 5000, failures: 0, nextAt: 0, polling: false, troubled: false }
    job = apiJob("metadata", "GET", m.server + "/.well-known/oauth-authorization-server")
    job.signIn = m.signIn.id
    apiSend(job)
end sub

' The sign-in a job belongs to, when it is still the one under way.
function apiSignInOf(job as object) as dynamic
    if m.signIn = invalid or m.signIn.id <> job.signIn then return invalid
    return m.signIn
end function

sub apiSignInFailed(signIn as object, message as string)
    m.signIn = invalid
    apiAnswer(signIn.request, apiFail("sign_in", message))
end sub

' The server didn't answer the sign-in: "network", as for any request.
function apiSignInUnreachable(signIn as object, status as integer, body as string) as boolean
    answer = httpAnswer(status, body)
    if answer.error <> "network" then return false
    m.signIn = invalid
    apiAnswer(signIn.request, answer)
    return true
end function

sub apiSignInUpdate(signIn as object, update as object)
    signIn.request.update = update
end sub

sub apiMetadataAnswered(job as object, status as integer, body as string)
    signIn = apiSignInOf(job)
    if signIn = invalid or apiSignInUnreachable(signIn, status, body) then return
    if status = 404
        apiSignInFailed(signIn, authTooOld())
        return
    end if
    meta = authParseMetadata(body, signIn.server)
    if not meta.ok
        apiSignInFailed(signIn, meta.message)
        return
    end if
    signIn.meta = meta
    ' Requests and approvals where this server has them.
    signIn.scope = authScopeFor(meta.scopes)
    device = apiForm(apiJob("device", "POST", meta.deviceEndpoint), { client_id: httpClientId(), scope: signIn.scope })
    device.signIn = signIn.id
    apiSend(device)
end sub

sub apiDeviceAnswered(job as object, status as integer, body as string)
    signIn = apiSignInOf(job)
    if signIn = invalid or apiSignInUnreachable(signIn, status, body) then return
    if status < 200 or status >= 300
        apiSignInFailed(signIn, authDeviceRefusal(status, body))
        return
    end if
    code = authParseDeviceCode(body, apiNow())
    if not code.ok
        apiSignInFailed(signIn, code.message)
        return
    end if
    signIn.code = code
    signIn.intervalMs = code.interval * 1000
    signIn.nextAt = m.clock.TotalMilliseconds() + signIn.intervalMs
    apiAnswer(signIn.request, apiOk({
        userCode: code.userCode,
        verificationUri: code.verificationUri,
        verificationUriComplete: code.verificationUriComplete,
        qr: authQrAddress(signIn.server, code.userCode),
        expiresAt: code.expiresAt
    }))
    apiSignInUpdate(signIn, { status: "waiting" })
end sub

' Asks the token endpoint about the code every interval, never sooner.
sub apiPollWhenDue(now as integer)
    signIn = m.signIn
    if signIn = invalid or signIn.code = invalid or signIn.polling or now < signIn.nextAt then return
    if apiNow() >= signIn.code.expiresAt
        m.signIn = invalid
        apiSignInUpdate(signIn, { status: "expired" })
        return
    end if
    signIn.polling = true
    poll = apiForm(apiJob("poll", "POST", signIn.meta.tokenEndpoint), { grant_type: authDeviceGrant(), device_code: signIn.code.deviceCode, client_id: httpClientId() })
    poll.signIn = signIn.id
    poll.revocationEndpoint = signIn.meta.revocationEndpoint
    apiSend(poll)
end sub

sub apiPollAnswered(job as object, status as integer, body as string)
    signIn = apiSignInOf(job)
    if signIn = invalid
        ' Approved after the TV stopped waiting: those tokens are ended.
        if status >= 200 and status < 300
            answer = authParseTokens(body, httpScope(), apiNow())
            if answer.ok then apiRevoke({ access: answer.access, refresh: answer.refresh, revocationEndpoint: job.revocationEndpoint })
        end if
        return
    end if
    signIn.polling = false
    answer = authPollAnswer(status, body)
    now = m.clock.TotalMilliseconds()
    if answer.kind = "pending" or answer.kind = "slow_down"
        ' slow_down: 5 seconds more, for good (RFC 8628 §3.5).
        if answer.kind = "slow_down" then signIn.intervalMs = signIn.intervalMs + 5000
        signIn.failures = 0
        signIn.nextAt = now + signIn.intervalMs
        if signIn.troubled
            signIn.troubled = false
            apiSignInUpdate(signIn, { status: "waiting" })
        end if
    else if answer.kind = "network"
        signIn.failures = signIn.failures + 1
        delayMs = authPollWait(signIn.intervalMs, signIn.failures)
        signIn.nextAt = now + delayMs
        signIn.troubled = true
        apiSignInUpdate(signIn, { status: "retrying", seconds: Int(delayMs / 1000) })
    else if answer.kind = "tokens"
        m.signIn = invalid
        apiSignedIn(signIn, body)
    else
        m.signIn = invalid
        apiSignInUpdate(signIn, { status: answer.kind, message: answer.message })
    end if
end sub

' Keeps the tokens of an approved code; the sign-in they replace on that
' server is ended.
sub apiSignedIn(signIn as object, body as string)
    answer = authParseTokens(body, signIn.scope, apiNow())
    if not answer.ok
        apiSignInUpdate(signIn, { status: "failed", message: answer.message })
        return
    end if
    meta = signIn.meta
    fresh = tokensFrom(answer, meta.issuer, meta.tokenEndpoint, meta.revocationEndpoint)
    if authExtraScopes(fresh.scope, signIn.scope).Count() > 0
        apiRevoke(fresh)
        apiSignInUpdate(signIn, { status: "failed", message: t("signin.problems.widerScope") })
        return
    end if
    previous = tokensLoad(m.tokensSection, fresh.issuer)
    tokensSave(m.tokensSection, fresh)
    ' The sign-in before ends only where the server proves it holds its key
    ' (apiRevoke).
    if previous <> invalid and previous.refresh <> fresh.refresh then apiRevoke(previous)
    ' Another profile, maybe: whether it has a PIN isn't known until it says.
    profKeepPin(CreateObject("roRegistrySection", "household"), fresh.issuer, invalid)
    if fresh.issuer = m.server
        m.tokens = fresh
        m.lastIssued = invalid
        ' A new sign-in, a new key: the next token waits for its proof.
        m.proved = invalid
        m.session.signOutReason = ""
        m.session.scope = fresh.scope
        apiPublish()
        m.session.signedIn = true
        apiAskProfile()
    end if
    apiSignInUpdate(signIn, { status: "signed_in" })
end sub

' --- Switching profiles ---

' A switch asked for. A cast's (spec.drain) is held until the profile
' before it has been heard: the scene ended what played as the switch was
' asked, and its last progress, stopped report and session end go first,
' with that profile's tokens (docs/ROKU.md, "Casts"). What hasn't gone in
' authSwitchHoldMs is dropped. One held gives way to a switch asked
' meanwhile.
sub apiSwitchAsked(request as object, spec as object)
    if m.switchHeld <> invalid
        apiAnswer(m.switchHeld.request, apiFail("switch_replaced", t("signin.problems.anotherChosen")))
        m.switchHeld = invalid
    end if
    if not httpTrue(spec.drain)
        apiSwitchProfile(request, spec)
        return
    end if
    m.switchHeld = { request: request, spec: spec, at: m.clock.TotalMilliseconds() }
    apiSwitchWhenHeard(m.switchHeld.at)
end sub

' The switch held goes once the profile's requests are answered, or at
' the hold's end, those left dropped (authSwitchHeld).
sub apiSwitchWhenHeard(now as integer)
    held = m.switchHeld
    if held = invalid then return
    jobs = []
    for each key in m.transfers
        jobs.Push(m.transfers[key])
    end for
    jobs.Append(m.waiting)
    for each entry in m.later
        jobs.Push(entry.job)
    end for
    verdict = authSwitchHeld(jobs, m.switches, held.at, now)
    if verdict = "wait" then return
    m.switchHeld = invalid
    if verdict = "drop"
        ' Those in flight went with the profile's tokens; those not sent
        ' yet go no further.
        dropped = m.waiting
        m.waiting = []
        later = []
        for each entry in m.later
            if entry.job.auth
                dropped.Push(entry.job)
            else
                later.Push(entry)
            end if
        end for
        m.later = later
        for each job in dropped
            apiAnswer(job.request, apiProfileChanged())
        end for
    end if
    apiSwitchProfile(held.request, held.spec)
end sub

' Moves the sign-in to another profile of the household, or checks the PIN
' of the one it acts as (docs/API.md, the switch-profile grant): with the
' refresh token, which a switch spends as a refresh does, so never beside
' one. spec: { profile, pin ("" for none), ticket (a cast's switch_ticket,
' in place of the PIN, as the receiver of another profile's remote) }. The
' answer: ok ({ through: true } when there was no PIN to check), or the
' refusal (authSwitchOutcome) with its reason as the error.
sub apiSwitchProfile(request as object, spec as object)
    tokens = m.tokens
    if tokens = invalid
        apiAnswer(request, apiSignedOut())
        return
    end if
    if m.refreshing.DoesExist(tokens.issuer)
        ' One switch waits at most: an earlier one asked for meanwhile gives way.
        if m.switchWaiting <> invalid then apiAnswer(m.switchWaiting.request, apiFail("switch_replaced", t("signin.problems.anotherChosen")))
        m.switchWaiting = { request: request, spec: spec }
        return
    end if
    m.refreshing[tokens.issuer] = true
    m.switching = true
    params = { grant_type: authSwitchGrant(), client_id: httpClientId(), refresh_token: tokens.refresh, profile: urlValue(spec.profile) }
    pin = httpString(spec.pin)
    if pin <> "" then params.pin = pin
    ticket = httpString(spec.ticket)
    if ticket <> "" then params.ticket = ticket
    job = apiForm(apiJob("switch", "POST", tokens.tokenEndpoint), params)
    job.tokens = tokens
    job.request = request
    job.profile = spec.profile
    ' As a refresh: an answer given up on can be a spent refresh token.
    job.timeoutMs = 120000
    ' The refresh token, the PIN and the ticket go only to a server that
    ' proves itself now.
    apiSendProved(tokens, job)
end sub

sub apiSwitchAnswered(job as object, status as integer, body as string)
    old = job.tokens
    m.refreshing.Delete(old.issuer)
    m.switching = false
    current = m.tokens <> invalid and m.tokens.issuer = old.issuer and m.tokens.refresh = old.refresh
    outcome = authSwitchOutcome(status, body)
    if outcome.kind = "tokens"
        answer = authParseTokens(body, old.scope, apiNow())
        fresh = invalid
        if answer.ok then fresh = tokensFrom(answer, old.issuer, old.tokenEndpoint, old.revocationEndpoint, old)
        if fresh <> invalid and authExtraScopes(fresh.scope, old.scope).Count() > 0
            apiRevoke(fresh)
            fresh = invalid
        end if
        if fresh = invalid
            ' The old refresh token is spent: the sign-in is over.
            if current then apiEnd("ended")
            apiAnswer(job.request, apiSignedOut())
        else if not current
            ' Signed out or in again meanwhile: those stay.
            apiRevoke(fresh)
            apiAnswer(job.request, apiFail("server_changed", t("signin.problems.signInChanged")))
        else
            ' Saved before they are used: the old refresh token is spent.
            tokensSave(m.tokensSection, fresh)
            ' Another profile, maybe: whether it has a PIN isn't known until it says.
            profKeepPin(CreateObject("roRegistrySection", "household"), old.issuer, invalid)
            m.tokens = fresh
            ' Another profile (not a PIN checked for this one): what was
            ' asked for the one before goes no further.
            if httpNumber(job.profile, 0) <> m.session.profileId then m.switches = m.switches + 1
            m.lastIssued = { token: fresh.access, at: apiNow() }
            m.session.scope = fresh.scope
            apiPublish()
            apiAskProfile()
            apiAnswer(job.request, apiOk({ through: false }))
        end if
    else if outcome.kind = "ended"
        if current then apiEnd("ended")
        apiAnswer(job.request, apiSignedOut())
    else if outcome.kind = "through"
        apiAnswer(job.request, apiOk({ through: true }))
    else
        failure = apiFail(outcome.reason, outcome.message)
        failure.status = status
        apiAnswer(job.request, failure)
    end if
    apiSendWaiting()
    if m.switchWaiting <> invalid
        waiting = m.switchWaiting
        m.switchWaiting = invalid
        apiSwitchProfile(waiting.request, waiting.spec)
    end if
end sub

' --- Telling servers apart (docs/API.md) ---

' Whether the access token may go to the server in use (authTokenMayGo): it
' proved lately, on this network, that it holds the sign-in's key.
function apiTokenMayGo() as boolean
    if m.tokens = invalid then return true
    return authTokenMayGo(m.tokens.proof, m.proved, m.network, m.doubted, m.clock.TotalMilliseconds())
end function

' The access token the scene's players, pictures and event stream use:
' only while it may go to the server, "" meanwhile.
sub apiPublish()
    token = ""
    if m.tokens <> invalid and apiTokenMayGo() then token = m.tokens.access
    if m.session.accessToken <> token then m.session.accessToken = token
end sub

' The proof under way, if any, no longer counts: the server, the sign-in or
' the network changed. Its answer is let go when it comes.
sub apiForgetProof()
    m.proofSeq = m.proofSeq + 1
    m.proving = false
    m.proofRetryAt = 0
end sub

' Asks the server at tokens' address to prove it holds their key; then
' sends after (a refresh, a switch, a revocation), or what waits without
' it. One proof of the tokens in use at a time.
sub apiProve(tokens as object, after = invalid as dynamic)
    if after = invalid
        if m.proving then return
        m.proving = true
    end if
    ' 16 of a random UUID's bytes: the challenge.
    bytes = CreateObject("roByteArray")
    bytes.FromHexString(CreateObject("roDeviceInfo").GetRandomUUID().Replace("-", ""))
    challenge = authBase64Url(bytes)
    job = apiJob("proof", "GET", tokens.issuer + urlWithQuery("/api/server", { challenge: challenge, proof_id: tokens.proof.proofId }))
    job.tokens = tokens
    job.challenge = challenge
    job.after = after
    job.network = m.network
    job.seq = m.proofSeq
    job.timeoutMs = 10000
    apiSend(job)
end sub

' Sends job, which carries tokens' refresh token, once their server proves
' itself now; at once for a sign-in from a server without proofs.
sub apiSendProved(tokens as object, job as object)
    if tokens.proof = invalid
        apiSend(job)
    else
        apiProve(tokens, job)
    end if
end sub

sub apiProofAnswered(job as object, status as integer, body as string)
    tokens = job.tokens
    ' For a server, sign-in or network since left: the proof asked since
    ' answers for what waits.
    if job.after = invalid and job.seq <> m.proofSeq then return
    if job.after = invalid then m.proving = false
    current = m.tokens <> invalid and m.tokens.issuer = tokens.issuer and m.tokens.refresh = tokens.refresh
    verdict = authProofVerdict(status, body, tokens.proof, tokens.issuer, job.challenge)
    if verdict = "proved"
        if current and job.network = m.network
            m.proved = { at: m.clock.TotalMilliseconds(), network: job.network }
            m.doubted = false
        end if
        apiPublish()
        if job.after <> invalid
            apiSend(job.after)
        else
            apiSendWaiting()
        end if
        return
    end if
    ' Asked again in a while, or by the next request.
    if job.after = invalid or current then m.proofRetryAt = m.clock.TotalMilliseconds() + 30000
    if verdict = "another"
        ' Another device answers at the address: it gets no token.
        if current then m.proved = invalid
        failure = apiFail("network", authAnotherDevice())
    else
        failure = httpAnswer(status, body)
    end if
    apiPublish()
    if job.after = invalid
        apiFailWaiting(failure)
    else if job.after.purpose = "refresh" or job.after.purpose = "switch"
        m.refreshing.Delete(tokens.issuer)
        if job.after.purpose = "switch"
            m.switching = false
            apiAnswer(job.after.request, failure)
        end if
        apiFailWaiting(failure)
    end if
end sub

' The Roku joined or left a network: the server proves itself again
' before the token goes, and the players' token waits for it.
sub apiNetworkChanged()
    m.network = m.network + 1
    m.proved = invalid
    ' A proof under way was asked on the network before.
    apiForgetProof()
    apiPublish()
    if m.tokens <> invalid and m.tokens.proof <> invalid then apiProve(m.tokens)
end sub

' Asks for the next proof before the one held lapses, so the players'
' token never does while the server answers; one that failed is asked
' again every 30 seconds. A lapsed proof takes the players' token.
sub apiRenewProof(now as integer)
    apiPublish()
    if m.tokens = invalid or m.tokens.proof = invalid or m.proving then return
    if authProofDue(m.proved, m.network, m.doubted, now, m.proofRetryAt) then apiProve(m.tokens)
end sub

' --- Playback ---

' POST /api/playback with what this Roku plays, asked of it now. mode is
' "auto", or "transcode" for a quality (maxBitrate, maxHeight: the Quality
' setting's or Automatic's, AutoQuality.brs) or once the original didn't
' play here; subtitleIndex, picture subtitles for the server to draw;
' audioIndex, the audio a cast named (Cast.brs's castGivenTracks);
' picturesOff, once the server turned out not to make PGS's pictures;
' controls, what the player obeys of the user's remote (Remote.brs).
sub apiPlay(request as object, spec as object)
    mode = httpString(spec.mode)
    if mode = "" then mode = "auto"
    body = {
        item_id: spec.itemId,
        start_ms: Int(httpNumber(spec.startMs, 0)),
        mode: mode,
        client_id: m.playerId,
        caps: capsBuild(CreateObject("roDeviceInfo"), not httpTrue(spec.picturesOff))
    }
    if httpNumber(spec.fileId, 0) > 0 then body.file_id = spec.fileId
    if httpNumber(spec.subtitleIndex, -1) >= 0 then body.subtitle_index = Int(spec.subtitleIndex)
    if httpNumber(spec.audioIndex, -1) >= 0 then body.audio_index = Int(spec.audioIndex)
    if httpNumber(spec.maxBitrate, 0) > 0 then body.max_bitrate = Int(spec.maxBitrate)
    if httpNumber(spec.maxHeight, 0) > 0 then body.max_height = Int(spec.maxHeight)
    if httpTrue(spec.keyed) then body.keyed_urls = true
    apiRemote(body, spec)
    ' Starting a transcode takes the server a few seconds.
    apiCall(request, { method: "POST", path: "/api/playback", json: body, timeoutMs: 60000, play: true })
end sub

' What a play or watch request says for the user's remotes (docs/API.md,
' "Remote control"): the commands its player obeys (spec.controls, none
' from an https:// server, whose commands can't reach it), that it plays
' on a TV, and this Roku's name.
sub apiRemote(body as object, spec as object)
    if GetInterface(spec.controls, "ifArray") <> invalid and remoteMayControl(m.server) then body.controls = spec.controls
    body.device_kind = remoteDeviceKind()
    if m.deviceName <> "" then body.device_name = m.deviceName
end sub

' --- Live TV (Live.brs) ---

' POST /api/live/channels/{id}/watch with this Roku's client_id, which
' ends what it watched before (zapping holds one connection), and what the
' player obeys of the user's remote. Tuning a channel nobody watches takes
' the server a few seconds.
sub apiLiveWatch(request as object, spec as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    job = apiJob("call", "POST", m.server + "/api/live/channels/" + urlValue(spec.channelId) + "/watch")
    job.request = request
    job.build = ""
    job.auth = true
    job.liveWatch = true
    body = { client_id: m.playerId }
    ' Every connection in use: the one to take over (the server says
    ' whose it may be).
    takeOver = Int(httpNumber(spec.takeOver, 0))
    if takeOver > 0 then body.take_over = takeOver
    apiRemote(body, spec)
    job.body = FormatJson(body)
    job.contentType = "application/json"
    job.timeoutMs = 60000
    apiSend(job)
end sub

' DELETE /api/live/sessions/{session} of a watch's answer; nothing waits.
' gen: the profile the channel was watched as, whose tokens end it.
sub apiLeaveChannel(watching as dynamic, gen as integer)
    data = httpObject(watching)
    if data = invalid then return
    sessionId = httpString(data.session_id)
    if sessionId = "" then return
    job = apiJob("leave", "DELETE", m.server + "/api/live/sessions/" + urlEncode(sessionId))
    job.auth = true
    job.gen = gen
    apiSend(job)
end sub

' A page of a category's channels (GET /api/live/browse; spec: category,
' offset, limit, around), made into what spec.kind shows: "cards", the
' player's "list", or the "guide" with their programmes from spec.start
' to spec.upto (GET /api/live/guide?ids=, for these channels only).
' Answers { total, offset, channels }; the guide's, with upto.
sub apiLivePage(request as object, spec as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    category = httpString(spec.category)
    query = liveBrowseQuery(category, Int(httpNumber(spec.offset, 0)), Int(httpNumber(spec.limit, livePageSize())))
    around = Int(httpNumber(spec.around, 0))
    if around > 0
        query.Delete("offset")
        query.around = around
    end if
    job = apiJob("call", "GET", m.server + urlWithQuery("/api/live/browse", query))
    job.request = request
    job.build = "livePage"
    job.auth = true
    apiSend(job)
end sub

' A page came: built, or for the guide its programmes asked for first; a
' part of the guide came: kept, then built.
sub apiLiveAnswered(job as object, answer as object)
    request = job.request
    if not answer.ok
        apiAnswer(request, answer)
        return
    end if
    spec = request.spec
    if job.build = "liveGuide"
        data = httpObject(answer.data)
        if data <> invalid then liveGuideKeep(m.liveGuide.programs, data.programs)
        for each channel in job.channels
            m.liveGuide.upto["c" + channel.id.ToStr()] = job.upto
        end for
        apiLiveGuideBuilt(request, job.channels, job.page)
        return
    end if
    page = livePage(answer.data, job.server, httpString(spec.category))
    kind = httpString(spec.kind)
    if kind = "guide"
        apiLiveGuide(request, spec, page.channels, page)
        return
    end if
    now = apiNow()
    twelve = apiTwelveHour()
    items = []
    for each channel in page.channels
        if kind = "list"
            items.Push(liveListItem(channel, now, twelve))
        else
            items.Push(liveCard(channel, now, twelve))
        end if
    end for
    request.content = nodesContent(items)
    apiAnswer(request, apiOk(page))
end sub

' The guide of channels (a page's) from spec.start to spec.upto, the part
' not kept yet fetched first.
sub apiLiveGuide(request as object, spec as object, channels as dynamic, page = invalid as dynamic)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    start = Int(httpNumber(spec.start, 0))
    upto = Int(httpNumber(spec.upto, 0))
    key = m.server + "|" + start.ToStr()
    if m.liveGuide.key <> key then m.liveGuide = { key: key, programs: {}, upto: {} }
    list = []
    from = upto
    for each channel in httpArray(channels)
        if httpObject(channel) <> invalid
            list.Push(channel)
            had = m.liveGuide.upto["c" + channel.id.ToStr()]
            if had = invalid then had = start
            if had < from then from = had
        end if
    end for
    if page = invalid then page = { total: list.Count(), offset: 0, channels: list }
    if from >= upto or list.Count() = 0
        apiLiveGuideBuilt(request, list, page)
        return
    end if
    job = apiJob("call", "GET", m.server + urlWithQuery("/api/live/guide", { from: from, to: upto, ids: liveIds(list) }))
    job.request = request
    job.build = "liveGuide"
    job.auth = true
    job.upto = upto
    job.channels = list
    job.page = page
    apiSend(job)
end sub

sub apiLiveGuideBuilt(request as object, channels as object, page as object)
    spec = request.spec
    start = Int(httpNumber(spec.start, 0))
    upto = Int(httpNumber(spec.upto, 0))
    request.content = nodesGuide(liveGuideRows(channels, m.liveGuide.programs, start, upto))
    apiAnswer(request, apiOk({ total: page.total, offset: page.offset, channels: channels, upto: upto }))
end sub

' --- Where Automatic starts (AutoQuality.brs) ---

' GET /api/playback/network for this Roku, 1.5 s at most; an answer kept
' from the last 10 minutes at once.
sub apiNetwork(request as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    kept = aqNetworkKept(m.networkKept, m.server, m.clock.TotalMilliseconds())
    if kept <> invalid
        apiAnswer(request, kept)
        return
    end if
    job = apiJob("call", "GET", m.server + "/api/playback/network?client_id=" + urlEncode(m.playerId))
    job.request = request
    job.build = ""
    job.auth = true
    job.timeoutMs = 1500
    job.network = m.server
    apiSend(job)
end sub

' PUT /api/playback/network: a speed test's result ({ probeKbps }) or
' Automatic's ceiling ({ ceilingKbps }, 0 forgets). What is kept goes, and
' the server's answer takes its place.
sub apiTellNetwork(request as object, spec as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    m.networkKept.Delete(m.server)
    job = apiJob("call", "PUT", m.server + "/api/playback/network")
    job.request = request
    job.build = ""
    job.auth = true
    job.body = aqTellBody(m.playerId, spec)
    job.contentType = "application/json"
    timeout = httpNumber(spec.timeoutMs, 0)
    if timeout > 0 then job.timeoutMs = timeout
    job.network = m.server
    apiSend(job)
end sub

' The speed test: 8 MiB to a file in tmp:, the whole request timed, given
' up after timeoutMs (what came by then counts, where the Roku wrote it).
sub apiSpeedTest(request as object, spec as object)
    if m.server = ""
        apiAnswer(request, apiFail("no_server", t("signin.problems.chooseServer")))
        return
    end if
    job = apiJob("call", "GET", m.server + "/api/playback/speedtest?bytes=" + urlValue(aqProbeBytes()))
    job.request = request
    job.build = ""
    job.auth = true
    job.toFile = "tmp:/pley-speedtest"
    job.timeoutMs = Int(httpNumber(spec.timeoutMs, 3000))
    apiSpeedTestDelete(job.toFile)
    apiSend(job)
end sub

' { bytes, ms } of a speed test that ended (status -28: given up).
sub apiSpeedTestAnswered(job as object, status as integer)
    ms = m.clock.TotalMilliseconds() - job.startedAt
    bytes = 0
    if status = 200 or status = -28
        found = CreateObject("roFileSystem").Stat(job.toFile)
        if GetInterface(found, "ifAssociativeArray") <> invalid then bytes = httpNumber(found.size, 0)
    end if
    apiSpeedTestDelete(job.toFile)
    answer = apiOk({ bytes: bytes, ms: ms })
    answer.status = status
    apiAnswer(job.request, answer)
end sub

sub apiSpeedTestDelete(path as string)
    files = CreateObject("roFileSystem")
    if files.Exists(path) then files.Delete(path)
end sub
