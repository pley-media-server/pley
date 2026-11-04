' Live TV (docs/API.md, "Live TV"): the categories, the channels and their
' guide as the screens show them, a page at a time (a provider's lineup
' runs to tens of thousands of channels in hundreds of groups), what the
' player says, and where it goes on up and down. Times are unix seconds,
' as the server sends them.

' Logos are asked at about the size they're drawn.
function liveLogoWidth() as integer
    return 240
end function

' Channels asked for at once (GET /api/live/browse): the guide's rows, a
' page of cards, the player's list.
function livePageSize() as integer
    return 50
end function

' The guide is fetched this many seconds at a time as it is scrolled to,
' this far ahead at most (the web app's chunks and span).
function liveGuideChunk() as integer
    return 6 * 3600
end function

function liveGuideSpan() as integer
    return 48 * 3600
end function

' Where the guide starts: the half hour before the one now is in, so the
' programme on now shows whole.
function liveGuideStart(now as integer) as integer
    return Int(now / 1800) * 1800 - 1800
end function

' --- Categories ---

' A category: "all", "favorites", "recent" or "group:<name>", as the web
' app writes them, or "search:<query>", every channel whose name, number
' or programme on now has the query in it. The query GET /api/live/browse
' takes for it.
function liveBrowseQuery(category as string, offset as integer, limit as integer) as object
    query = { category: "all", offset: offset, limit: limit }
    if Left(category, 7) = "search:" then query.q = Mid(category, 8)
    if category = "favorites" or category = "recent" then query.category = category
    if Left(category, 6) = "group:"
        query.category = "group"
        query.group = Mid(category, 7)
    end if
    return query
end function

' A name's region and the rest, when it begins with one the way providers
' write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the server
' splits group names: [region, rest], or invalid.
function liveSplitRegion(name as string) as dynamic
    n = name.Trim()
    while Left(n, 1) = "|"
        n = Mid(n, 2).Trim()
    end while
    cache = liveCache()
    found = invalid
    for each re in [cache.bracket, cache.bar, cache.colon]
        if found = invalid
            match = re.Match(n)
            if match.Count() = 3 then found = match
        end if
    end for
    if found = invalid then return invalid
    rest = cache.lead.Replace(found[2], "").Trim()
    region = found[1].Trim()
    if region = "" or rest = "" then return invalid
    return [region, rest]
end function

function liveCache() as object
    globals = GetGlobalAA()
    if globals.liveCache = invalid
        globals.liveCache = {
            bracket: CreateObject("roRegex", "^\[([^\]]{1,16})\]\s*(.+)$", ""),
            bar: CreateObject("roRegex", "^([^|]{1,16})\|\s*(.+)$", ""),
            colon: CreateObject("roRegex", "^([A-Z]{1,4}):\s*(.+)$", ""),
            lead: CreateObject("roRegex", "^[|:\-\s]+", "")
        }
    end if
    return globals.liveCache
end function

' A channel's name as a list of category shows it: in a group, without
' the region its group already says ("Sky Sports 1" in UK · Sport, not
' "UK| Sky Sports 1").
function liveShortName(name as string, group as string, category as string) as string
    if Left(category, 6) <> "group:" then return name
    ofName = liveSplitRegion(name)
    ofGroup = liveSplitRegion(group)
    if ofName = invalid or ofGroup = invalid then return name
    if LCase(ofName[0]) <> LCase(ofGroup[0]) then return name
    return ofName[1]
end function

' "57,176".
function liveThousands(n as dynamic) as string
    text = Int(httpNumber(n, 0)).ToStr()
    out = ""
    while Len(text) > 3
        out = "," + Right(text, 3) + out
        text = Left(text, Len(text) - 3)
    end while
    return text + out
end function

' The groups of GET /api/live/groups: [{ name, region, label, count }].
function liveGroups(answer as dynamic) as object
    out = []
    data = httpObject(answer)
    if data = invalid then return out
    for each raw in httpArray(data.groups)
        g = httpObject(raw)
        if g <> invalid
            name = httpSentence(g.name)
            label = httpSentence(g.label)
            if label = "" then label = name
            if name <> "" then out.Push({ name: name, region: httpSentence(g.region), label: label, count: Int(httpNumber(g.count, 0)) })
        end if
    end for
    return out
end function

' What a category is called: "Favourites", "UK · Sport".
function liveCategoryName(category as string, groups as object) as string
    if Left(category, 7) = "search:" then return "Results for “" + Mid(category, 8) + "”"
    if category = "favorites" then return "Favourites"
    if category = "recent" then return "Recently watched"
    if Left(category, 6) <> "group:" then return "All channels"
    name = Mid(category, 7)
    for each g in groups
        if g.name = name
            if g.region <> "" then return g.region + " · " + g.label
            return g.label
        end if
    end for
    return name
end function

' Whether a category still has channels: a group gone, or nothing marked
' any more, shows every channel.
function liveCategoryValid(category as string, summary as object, groups as object) as string
    if Left(category, 7) = "search:" and Len(category) > 7 then return category
    if category = "favorites" and summary.favorites > 0 then return category
    if category = "recent" and summary.recent > 0 then return category
    if Left(category, 6) = "group:"
        name = Mid(category, 7)
        for each g in groups
            if g.name = name then return category
        end for
    end if
    return "all"
end function

' The groups as the rail sorts them: the regions with more than one
' group ("UK" of "UK| Sport", "UK| News"…), by name, and the other groups
' in the provider's order; with fewer than two regions, every group in
' its order. { regions: [{ region, groups, count }], others }.
function liveGroupSections(groups as object) as object
    byRegion = {}
    order = []
    for each g in groups
        if g.region <> ""
            key = "r" + g.region
            if not byRegion.DoesExist(key)
                byRegion[key] = []
                order.Push(g.region)
            end if
            byRegion[key].Push(g)
        end if
    end for
    regions = []
    for each region in order
        members = byRegion["r" + region]
        if members.Count() > 1
            total = 0
            for each member in members
                total = total + member.count
            end for
            ' In alphabetical order as they come.
            entry = { region: region, groups: members, count: total }
            at = regions.Count()
            while at > 0 and LCase(regions[at - 1].region) > LCase(region)
                at = at - 1
            end while
            sorted = []
            for i = 0 to regions.Count() - 1
                if i = at then sorted.Push(entry)
                sorted.Push(regions[i])
            end for
            if at = regions.Count() then sorted.Push(entry)
            regions = sorted
        end if
    end for
    if regions.Count() < 2 then return { regions: [], others: groups }
    gathered = {}
    for each r in regions
        gathered["r" + r.region] = true
    end for
    others = []
    for each g in groups
        if g.region = "" or not gathered.DoesExist("r" + g.region) then others.Push(g)
    end for
    return { regions: regions, others: others }
end function

' A group as a link to it reads: "UK · Sport", or its name.
function liveGroupLabel(g as object) as string
    if g.region <> "" then return g.region + " · " + g.label
    return g.name
end function

' The groups whose names have query in them, 12 at most.
function liveGroupsMatching(groups as object, query as string) as object
    out = []
    wanted = LCase(query.Trim())
    if wanted = "" then return out
    for each g in groups
        if out.Count() < 12 and Instr(1, LCase(g.name), wanted) > 0 then out.Push(g)
    end for
    return out
end function

' Where rewind (by -1) and fast forward (by 1) take the rail's focus from
' at: a screen of rows (rows) on or back, past a heading the way it goes,
' no further than the ends. kinds: the rail's entries' kinds.
function liveRailPage(kinds as object, at as integer, rows as integer, by as integer) as integer
    last = kinds.Count() - 1
    if last < 0 then return 0
    target = at + by * rows
    if target > last then target = last
    if target < 0 then target = 0
    while target >= 0 and target <= last and kinds[target] = "heading"
        target = target + by
    end while
    if target > last then target = last
    if target < 0 then target = 0
    return target
end function

' The Search entry pinned above the categories: "Search", or the words
' searched for.
function liveSearchLabel(query as string) as string
    if query = "" then return "Search"
    return "Search: “" + query + "”"
end function

' The category a search goes back to, on Back or Clear: the one shown as
' the first of the searches in a row began. category: the one shown as
' another search begins; before: the one kept until then.
function liveBeforeSearch(category as string, before as string) as string
    if Left(category, 7) = "search:" then return before
    if category = "" then return "all"
    return category
end function

' The categories down the side, under the Search entry pinned above them:
' the groups whose names match the words searched for, as links,
' Favourites, Recently watched, All channels, then "Regions" in
' alphabetical order, the one open with its groups under it, and "More
' groups", the rest in the provider's order (one plain list, "Groups",
' with fewer than two regions). [{ kind ("category", "region" or
' "heading"), key (the category, or the region), label, count (-1 for
' none), inner (set in under the entry above) }].
function liveRail(summary as object, groups as object, open as string, query as string) as object
    rail = []
    for each g in liveGroupsMatching(groups, query)
        rail.Push({ kind: "category", key: "group:" + g.name, label: liveGroupLabel(g), count: g.count, inner: true })
    end for
    rail.Push({ kind: "category", key: "favorites", label: "Favourites", count: summary.favorites, inner: false })
    rail.Push({ kind: "category", key: "recent", label: "Recently watched", count: summary.recent, inner: false })
    rail.Push({ kind: "category", key: "all", label: "All channels", count: summary.total, inner: false })
    sections = liveGroupSections(groups)
    if sections.regions.Count() > 0
        rail.Push({ kind: "heading", key: "", label: "Regions", count: -1, inner: false })
        for each r in sections.regions
            rail.Push({ kind: "region", key: r.region, label: r.region + "  ·  " + r.groups.Count().ToStr() + " groups", count: r.count, inner: false })
            if r.region = open
                for each g in r.groups
                    rail.Push({ kind: "category", key: "group:" + g.name, label: g.label, count: g.count, inner: true })
                end for
            end if
        end for
    end if
    others = sections.others
    if others.Count() > 1 or (others.Count() = 1 and sections.regions.Count() > 0)
        heading = "Groups"
        if sections.regions.Count() > 0 then heading = "More groups"
        rail.Push({ kind: "heading", key: "", label: heading, count: -1, inner: false })
        for each g in others
            label = g.name
            if label = "" then label = "No group"
            rail.Push({ kind: "category", key: "group:" + g.name, label: label, count: g.count, inner: false })
        end for
    end if
    return rail
end function

' The region a group category is gathered under in the rail (open there),
' or "".
function liveRegionOf(category as string, groups as object) as string
    if Left(category, 6) <> "group:" then return ""
    name = Mid(category, 7)
    for each r in liveGroupSections(groups).regions
        for each g in r.groups
            if g.name = name then return r.region
        end for
    end for
    return ""
end function

' { total, favorites, recent } of GET /api/live/groups.
function liveSummary(answer as dynamic) as object
    data = httpObject(answer)
    if data = invalid then data = {}
    return { total: Int(httpNumber(data.total, 0)), favorites: Int(httpNumber(data.favorites, 0)), recent: Int(httpNumber(data.recent, 0)) }
end function

' --- Channels ---

' A programme as the screens keep it: { start, ends, title, line,
' description, badges, live, isNew }, or invalid for none (or one that
' ends before it starts).
function liveProgram(raw as dynamic) as dynamic
    p = httpObject(raw)
    if p = invalid then return invalid
    start = Int(httpNumber(p.start, 0))
    ends = Int(httpNumber(p["stop"], 0))
    if ends <= start then return invalid
    title = httpSentence(p.title)
    if title = "" then title = "Untitled"
    line = []
    episode = fmtEpisode(p.season, p.episode)
    if episode <> "" then line.Push(episode)
    subtitle = httpSentence(p.subtitle)
    if subtitle <> "" and subtitle <> title then line.Push(subtitle)
    badges = []
    if httpTrue(p.is_live) then badges.Push("Live")
    if httpTrue(p.is_new) then badges.Push("New")
    rating = httpSentence(p.rating)
    if rating <> "" then badges.Push(rating)
    return {
        start: start,
        ends: ends,
        title: title,
        line: line.Join(" · "),
        description: httpClean(p.description, 600, false),
        badges: badges.Join(" · "),
        live: httpTrue(p.is_live),
        isNew: httpTrue(p.is_new)
    }
end function

' A channel as the screens keep it: { id, name, number, group, logo,
' favorite, hasGuide, now, after }, its logo's address on server at
' width (now and after left out for none); invalid without an id. In a
' group's category, its name leaves out the region the group says.
function liveChannel(raw as dynamic, server as string, width = 240 as integer, category = "all" as string) as dynamic
    c = httpObject(raw)
    if c = invalid then return invalid
    id = Int(httpNumber(c.id, 0))
    if id <= 0 then return invalid
    name = httpSentence(c.name)
    if name = "" then name = "Channel " + id.ToStr()
    group = httpSentence(c.group)
    channel = {
        id: id,
        name: liveShortName(name, group, category),
        number: httpSentence(c.number),
        group: group,
        logo: urlImage(server, c.logo, width),
        favorite: httpTrue(c.favorite),
        hasGuide: httpTrue(c.has_guide)
    }
    ' Left out when there are none: a node's field doesn't keep invalid,
    ' and channels go from the API task to the screens in one.
    on = liveProgram(c.now)
    if on <> invalid then channel.now = on
    coming = liveProgram(c["next"])
    if coming <> invalid then channel.after = coming
    return channel
end function

' The channels of a list, in its order.
function liveChannels(list as dynamic, server as string, category = "all" as string) as object
    out = []
    for each raw in httpArray(list)
        channel = liveChannel(raw, server, liveLogoWidth(), category)
        if channel <> invalid then out.Push(channel)
    end for
    return out
end function

' A page of GET /api/live/browse: { total, offset, channels }.
function livePage(answer as dynamic, server as string, category as string) as object
    data = httpObject(answer)
    if data = invalid then data = {}
    return { total: Int(httpNumber(data.total, 0)), offset: Int(httpNumber(data.offset, 0)), channels: liveChannels(data.channels, server, category) }
end function

' "101  Pley News HD", or the name alone without a number.
function liveTitle(channel as object) as string
    if channel.number = "" then return channel.name
    return channel.number + "  " + channel.name
end function

' What's on a channel at now: its now, or the one after once that has
' begun; invalid when the guide has nothing on now.
function liveOnNow(channel as object, now as integer) as dynamic
    for each p in [channel.now, channel.after]
        if p <> invalid and p.start <= now and p.ends > now then return p
    end for
    return invalid
end function

' What comes on next: the first of now and after that hasn't begun.
function liveComingUp(channel as object, now as integer) as dynamic
    for each p in [channel.now, channel.after]
        if p <> invalid and p.start > now then return p
    end for
    return invalid
end function

' How far into p now is, from 0 to 1.
function liveProgress(p as dynamic, now as integer) as float
    if p = invalid then return 0
    length = p.ends - p.start
    if length <= 0 then return 0
    done = (now - p.start) / length
    if done < 0 then return 0
    if done > 1 then return 1
    return done
end function

' "12 min left", "1 h 5 min left", "2 h left".
function liveTimeLeft(p as object, now as integer) as string
    minutes = Int((p.ends - now + 30) / 60)
    if minutes < 1 then minutes = 1
    if minutes < 60 then return minutes.ToStr() + " min left"
    hours = Int(minutes / 60)
    minutes = minutes mod 60
    if minutes = 0 then return hours.ToStr() + " h left"
    return hours.ToStr() + " h " + minutes.ToStr() + " min left"
end function

' "8:05 PM" on a Roku set to a 12-hour clock, "20:05" on a 24-hour one.
function liveClockText(hours as integer, minutes as integer, twelve as boolean) as string
    if not twelve then return fmtTwo(hours) + ":" + fmtTwo(minutes)
    suffix = " AM"
    if hours >= 12 then suffix = " PM"
    shown = hours mod 12
    if shown = 0 then shown = 12
    return shown.ToStr() + ":" + fmtTwo(minutes) + suffix
end function

' A time in the Roku's own time zone.
function liveClock(seconds as integer, twelve as boolean) as string
    at = CreateObject("roDateTime")
    at.FromSeconds(seconds)
    at.ToLocalTime()
    return liveClockText(at.GetHours(), at.GetMinutes(), twelve)
end function

' "8:00 PM – 9:30 PM".
function liveSpan(p as object, twelve as boolean) as string
    return liveClock(p.start, twelve) + " – " + liveClock(p.ends, twelve)
end function

' Where a channel is in channels, or -1.
function liveIndexOf(channels as object, id as dynamic) as integer
    for i = 0 to channels.Count() - 1
        if channels[i].id = id then return i
    end for
    return -1
end function

' Channel up and down (by 1 or -1) from the channel with id, in the page of
' its category the player holds ({ total, offset, channels }): the
' channel next to it, or at the page's ends the page to fetch and which of
' its channels to take ({ offset, last }), round the ends of the list.
' { channel } or { offset, last }; invalid without channels. A channel
' not in the page steps from its first.
function liveZap(page as object, id as dynamic, by as integer) as dynamic
    count = page.channels.Count()
    if count = 0 then return invalid
    at = liveIndexOf(page.channels, id)
    if at < 0 then return { channel: page.channels[0] }
    j = at + by
    if j >= 0 and j < count then return { channel: page.channels[j] }
    if page.total <= count then return { channel: page.channels[(j + count) mod count] }
    size = livePageSize()
    pages = Int((page.total + size - 1) / size)
    pageNo = Int(page.offset / size)
    if by < 0 then return { offset: ((pageNo - 1 + pages) mod pages) * size, last: true }
    return { offset: ((pageNo + 1) mod pages) * size, last: false }
end function

' "Channels 51–100 of 2,345", or "" for a list on one page.
function liveRange(offset as integer, shown as integer, total as integer) as string
    if total <= shown or shown = 0 then return ""
    return "Channels " + liveThousands(offset + 1) + "–" + liveThousands(offset + shown) + " of " + liveThousands(total)
end function

' A channel's card (components/items/LiveCard): its logo and number, the
' name, what's on with how far along, and what's next.
function liveCard(channel as object, now as integer, twelve as boolean) as object
    on = liveOnNow(channel, now)
    coming = liveComingUp(channel, now)
    line = "No guide"
    if channel.hasGuide then line = "Nothing in the guide now"
    progress = 0.0
    if on <> invalid
        line = on.title
        progress = liveProgress(on, now)
    end if
    nextLine = ""
    if coming <> invalid then nextLine = liveClock(coming.start, twelve) + " · " + coming.title
    return {
        channelId: channel.id,
        title: channel.name,
        number: channel.number,
        image: channel.logo,
        favorite: channel.favorite,
        line: line,
        progress: progress,
        nextLine: nextLine
    }
end function

' A channel in the player's channel list (components/items/LiveChannelRow):
' what's on with how far along, else what comes on next.
function liveListItem(channel as object, now as integer, twelve as boolean) as object
    on = liveOnNow(channel, now)
    line = ""
    progress = 0.0
    if on <> invalid
        line = on.title
        progress = liveProgress(on, now)
    else
        coming = liveComingUp(channel, now)
        if coming <> invalid then line = liveClock(coming.start, twelve) + " · " + coming.title
    end if
    return { channelId: channel.id, number: channel.number, title: channel.name, image: channel.logo, line: line, progress: progress, favorite: channel.favorite, current: false }
end function

' --- The guide ---

' Programmes of GET /api/live/guide added to kept ({ "<channel id>": {
' "<start>": programme } }), each once.
sub liveGuideKeep(kept as object, programs as dynamic)
    byChannel = httpObject(programs)
    if byChannel = invalid then return
    for each id in byChannel
        list = httpArray(byChannel[id])
        key = "c" + id
        if not kept.DoesExist(key) then kept[key] = {}
        known = kept[key]
        for each raw in list
            p = liveProgram(raw)
            if p <> invalid then known["s" + p.start.ToStr()] = p
        end for
    end for
end sub

' A channel's programmes kept, by start, none overlapping the one before
' (the TimeGrid draws nothing of programmes that overlap).
function liveGuidePrograms(kept as object, channelId as integer) as object
    known = kept["c" + channelId.ToStr()]
    if known = invalid then return []
    list = []
    for each key in known
        list.Push(known[key])
    end for
    list.SortBy("start")
    out = []
    lastStop = 0
    for each p in list
        if p.start >= lastStop
            out.Push(p)
            lastStop = p.ends
        else if p.ends > lastStop
            ' Overlapping the one before: from where that one stops.
            cut = {}
            cut.Append(p)
            cut.start = lastStop
            out.Push(cut)
            lastStop = p.ends
        end if
    end for
    return out
end function

' The TimeGrid's channels for channels: [{ fields, programs }], fields
' the channel's ContentNode's (its name as the guide writes it when the
' logo doesn't show, the logo, its id) and programs its children's
' (TITLE, PLAYSTART, PLAYDURATION; the details as the screen above the
' grid shows them). Between from and upto (what is fetched), time the
' guide says nothing of is a programme too (id "gap"), else the TimeGrid
' would say it is still loading: "No guide information", or across a
' channel without a guide, its name.
function liveGuideRows(channels as object, kept as object, from as integer, upto as integer) as object
    out = []
    for each channel in channels
        gapTitle = "No guide information"
        if not channel.hasGuide then gapTitle = channel.name
        programs = []
        at = from
        for each p in liveGuidePrograms(kept, channel.id)
            if p.start - at >= 60 then programs.Push(liveGuideGap(at, p.start, gapTitle))
            programs.Push({
                title: p.title,
                playStart: p.start,
                playDuration: p.ends - p.start,
                description: p.description,
                shortDescriptionLine1: p.line,
                shortDescriptionLine2: p.badges
            })
            if p.ends > at then at = p.ends
        end for
        if upto - at >= 60 then programs.Push(liveGuideGap(at, upto, gapTitle))
        out.Push({ fields: { id: channel.id.ToStr(), title: liveTitle(channel), hdSmallIconUrl: channel.logo }, programs: programs })
    end for
    return out
end function

function liveGuideGap(from as integer, upto as integer, title as string) as object
    return { id: "gap", title: title, playStart: from, playDuration: upto - from, description: "", shortDescriptionLine1: "", shortDescriptionLine2: "" }
end function

' The part of the guide to fetch for the time shown (left edge and the
' width shown, in seconds), with loadedTo fetched so far from start:
' { from, to }, or invalid when what is fetched covers it, with an hour to
' spare, or the guide's span is reached.
function liveGuideMissing(start as integer, loadedTo as integer, leftEdge as integer, shown as integer) as dynamic
    last = start + liveGuideSpan()
    if loadedTo >= last then return invalid
    if leftEdge + shown + 3600 <= loadedTo then return invalid
    upto = loadedTo + liveGuideChunk()
    if upto > last then upto = last
    return { from: loadedTo, to: upto }
end function

' Whether the guide is away from now, so that Back brings it back first:
' now isn't in the time shown (left, and shown seconds on), or the
' programme in focus (from, upto; upto 0 for none) isn't on at now. Time
' without a guide is a programme too (liveGuideRows), on now where it
' spans now.
function liveGuideAway(left as integer, shown as integer, from as integer, upto as integer, now as integer) as boolean
    if now < left or now >= left + shown then return true
    return upto > 0 and (from > now or upto <= now)
end function

' "1,2,3": the ids of channels, for GET /api/live/guide?ids=.
function liveIds(channels as object) as string
    ids = []
    for each channel in channels
        ids.Push(channel.id.ToStr())
    end for
    return ids.Join(",")
end function

' --- The player ---

' A watching that didn't start, as the player says it: { kind, title,
' message, retry, connections, lines }. kind "busy" (every connection of
' the source in use, 409: the server's message, and what holds each
' connection as liveBusyConnections reads them, lines the notice's), "missing"
' (404), "closed" (403), "stopped" (410: its connection taken over, or
' stopped by an administrator, liveStopped), "unavailable" (503) or "other".
function liveTrouble(answer as object, channelName as string) as object
    status = answer.status
    message = httpSentence(answer.message)
    if status = 409
        connections = liveBusyConnections(answer.data)
        return { kind: "busy", title: "All connections are in use", message: message, retry: true, connections: connections, lines: liveBusyLines(message, connections) }
    end if
    if status = 410
        ' The server's own words, not a status's.
        body = httpObject(answer.data)
        words = ""
        if body <> invalid then words = httpString(body.message)
        return liveStopped(words)
    end if
    problem = { kind: "other", title: channelName + " didn't start", message: message, retry: true, connections: [] }
    if status = 404 then problem = { kind: "missing", title: "This channel isn't here any more", message: "It may have been hidden, or its source removed.", retry: false, connections: [] }
    if status = 403 then problem = { kind: "closed", title: "Live TV isn't open to this account", message: "An administrator can open it in Settings → Live TV, on the web.", retry: false, connections: [] }
    if status = 503 then problem = { kind: "unavailable", title: channelName + " isn't playing", message: message, retry: true, connections: [] }
    problem.lines = problem.message
    return problem
end function

' Watching ended on the server: its connection taken over (the event
' live_taken_over, or the playlist's 410), or stopped there. The server's
' own words, as every app says them, else "This channel isn't being watched
' any more."; Watch again tunes the channel as any watch does.
function liveStopped(message as dynamic) as object
    words = httpSentence(message)
    if words = "" then words = "This channel isn't being watched any more."
    return { kind: "stopped", title: "Live TV stopped", message: words, lines: words, retry: true, connections: [] }
end function

' The connections of a 409 ({ connections: [{ channel_id, channel_name,
' watchers, people, yours, can_take_over }] }): [{ channelId, channelName,
' holders, canTakeOver }].
function liveBusyConnections(data as dynamic) as object
    out = []
    body = httpObject(data)
    if body = invalid then return out
    for each raw in httpArray(body.connections)
        c = httpObject(raw)
        if c <> invalid
            id = Int(httpNumber(c.channel_id, 0))
            if id > 0 then out.Push({ channelId: id, channelName: httpSentence(c.channel_name), holders: liveHolders(c), canTakeOver: httpTrue(c.can_take_over) })
        end if
    end for
    return out
end function

' Who holds a connection: "You, on another device", their names (to
' administrators), or "Someone else" / "3 people".
function liveHolders(c as object) as string
    if httpTrue(c.yours) then return "You, on another device"
    names = []
    for each name in httpArray(c.watchers)
        text = httpSentence(name)
        if text <> "" then names.Push(text)
    end for
    if names.Count() > 0 then return names.Join(", ")
    people = Int(httpNumber(c.people, 0))
    if people <= 1 then return "Someone else"
    return people.ToStr() + " people"
end function

' The busy notice's text: the server's message, then what each connection
' watches and who.
function liveBusyLines(message as string, connections as object) as string
    lines = []
    if message <> "" then lines.Push(message)
    if connections.Count() > 0
        if lines.Count() > 0 then lines.Push("")
        if connections.Count() = 1
            lines.Push("Its one connection is watching:")
        else
            lines.Push("Its " + connections.Count().ToStr() + " connections are watching:")
        end if
        for each c in connections
            lines.Push("•  " + c.channelName + "  —  " + c.holders)
        end for
    end if
    return lines.Join(Chr(10))
end function

' The buttons of a trouble's notice, and what each does ([{ label, action,
' channelId }]): "Watch here instead" for each connection that may be taken
' over (naming its channel when there are several), Try again (Watch again
' when watching ended on the server), Other channels, Back.
function liveTroubleButtons(problem as object) as object
    out = []
    takeable = []
    for each c in problem.connections
        if c.canTakeOver then takeable.Push(c)
    end for
    for each c in takeable
        label = "Watch here instead"
        if takeable.Count() > 1 then label = "Watch here instead of " + c.channelName
        out.Push({ label: label, action: "takeOver", channelId: c.channelId })
    end for
    if problem.retry
        label = "Try again"
        if problem.kind = "stopped" then label = "Watch again"
        out.Push({ label: label, action: "retry", channelId: 0 })
    end if
    out.Push({ label: "Other channels", action: "list", channelId: 0 })
    out.Push({ label: "Back", action: "back", channelId: 0 })
    return out
end function

' The registry key of the Live TV screen's tab and category, and the
' channel last watched, for a server and profile.
function liveViewKey(server as string, profileId as dynamic) as string
    return server + "|" + urlValue(profileId)
end function

' { tab: "guide" | "channels", category, channel } kept for key, or the
' defaults (channel 0: none watched yet).
function liveLoadView(section as object, key as string) as object
    view = { tab: "guide", category: "all", channel: 0 }
    saved = httpObject(httpJson(section.Read(key)))
    if saved = invalid then return view
    if httpString(saved.tab) = "channels" then view.tab = "channels"
    category = httpString(saved.category)
    if category = "favorites" or category = "recent" or (Left(category, 6) = "group:" and Len(category) > 6) then view.category = category
    channel = Int(httpNumber(saved.channel, 0))
    if channel > 0 then view.channel = channel
    return view
end function

' The tab and the category; the channel last watched stays as kept.
sub liveSaveView(section as object, key as string, view as object)
    liveWriteView(section, key, view.tab, view.category, liveLoadView(section, key).channel)
end sub

' The channel last watched, the player's: Live TV opens at it, after a
' restart too. Written only when it changes, as zapping tunes one channel
' after another.
sub liveSaveChannel(section as object, key as string, channelId as integer)
    kept = liveLoadView(section, key)
    if channelId <= 0 or kept.channel = channelId then return
    liveWriteView(section, key, kept.tab, kept.category, channelId)
end sub

sub liveWriteView(section as object, key as string, shownTab as string, category as string, channel as integer)
    view = { tab: shownTab, category: category }
    if channel > 0 then view.channel = channel
    section.Write(key, FormatJson(view))
    section.Flush()
end sub
