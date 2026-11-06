' The test channels, as GET /api/live/browse lists them.
function liveSample() as object
    return [
        {
            id: 1, source_id: 1, name: "Pley News HD", number: "101", group: "News", logo: "/api/live/channels/1/logo", favorite: false, has_guide: true,
            now: { channel_id: 1, start: 1000, stop: 2000, title: "Morning Briefing", subtitle: "Part 7", description: "Everything you need to know.", categories: ["News"], image: invalid, season: 1, episode: 7, is_new: false, is_live: false, rating: invalid },
            next: { channel_id: 1, start: 2000, stop: 2600, title: "World at One", subtitle: invalid, description: invalid, categories: [], image: invalid, season: invalid, episode: invalid, is_new: true, is_live: false, rating: "TV-PG" }
        },
        {
            id: 2, source_id: 1, name: "Sport 1080i", number: "102", group: "Sport", logo: invalid, favorite: true, has_guide: true,
            now: { channel_id: 2, start: 1000, stop: 2000, title: "Match of the Day", is_live: true },
            next: invalid
        },
        { id: 3, source_id: 1, name: "Film Four", number: "103", group: "News", logo: invalid, favorite: false, has_guide: false, now: invalid, next: invalid },
        { id: 0, name: "No id" },
        "not a channel"
    ]
end function

' A provider's groups, as GET /api/live/groups answers.
function liveSampleGroups() as object
    return {
        total: 56171, favorites: 2, recent: 9,
        groups: [
            { name: "UK| Sport", region: "UK", label: "Sport", count: 120 },
            { name: "UK| News", region: "UK", label: "News", count: 40 },
            { name: "News", region: invalid, label: "News", count: 2 },
            { name: "FR| Cinema", region: "FR", label: "Cinema", count: 30 },
            { name: "AR| Kids", region: "AR", label: "Kids", count: 5 },
            { name: "AR| Movies", region: "AR", label: "Movies", count: 7 },
            { label: "No name", count: 3 }
        ]
    }
end function

sub test_live_channels_read()
    channels = liveChannels(liveSample(), "http://tv:7539")
    tkEqual(channels.Count(), 3, "those without an id are left out")
    news = channels[0]
    tkEqual(news.id, 1)
    tkEqual(news.number, "101")
    tkEqual(news.logo, "http://tv:7539/api/live/channels/1/logo?w=240")
    tkEqual(news.now.title, "Morning Briefing")
    tkEqual(news.now.line, "S1 E7 · Part 7")
    tkEqual(news.now.ends, 2000)
    tkEqual(news.after.badges, "New · TV-PG")
    tkEqual(channels[1].logo, "", "no logo")
    tkTrue(channels[1].now.live)
    tkInvalid(channels[2].now)
    tkFalse(channels[2].DoesExist("now"), "left out, not invalid: a node's field doesn't keep invalid")
    tkEqual(liveTitle(news), "101  Pley News HD")
    tkEqual(liveTitle({ number: "", name: "Radio" }), "Radio")
    tkInvalid(liveProgram({ start: 10, stop: 10, title: "zero" }), "a programme that ends as it starts")
    page = livePage({ total: 120, offset: 50, channels: liveSample() }, "", "all")
    tkEqual(page.total, 120)
    tkEqual(page.offset, 50)
    tkEqual(page.channels.Count(), 3)
end sub

sub test_live_regions_and_short_names()
    tkEqual(liveSplitRegion("UK| Sky Sports 1"), ["UK", "Sky Sports 1"])
    tkEqual(liveSplitRegion("[FR] TF1 HD"), ["FR", "TF1 HD"])
    tkEqual(liveSplitRegion("DE: Das Erste"), ["DE", "Das Erste"])
    tkEqual(liveSplitRegion("||UK| BBC One"), ["UK", "BBC One"])
    tkInvalid(liveSplitRegion("BBC One"))
    tkInvalid(liveSplitRegion("Movies: The Best"), "a colon after more than a code")
    tkEqual(liveShortName("UK| Sky Sports 1", "UK| Sport", "group:UK| Sport"), "Sky Sports 1")
    tkEqual(liveShortName("UK| Sky Sports 1", "UK| Sport", "all"), "UK| Sky Sports 1", "every channel: the whole name")
    tkEqual(liveShortName("US| ESPN", "UK| Sport", "group:UK| Sport"), "US| ESPN", "another region stays")
    tkEqual(liveShortName("BBC One", "News", "group:News"), "BBC One")
    channel = liveChannel({ id: 9, name: "UK| BBC News", group: "UK| News" }, "", 240, "group:UK| News")
    tkEqual(channel.name, "BBC News")
    tkEqual(liveThousands(57176), "57,176")
    tkEqual(liveThousands(999), "999")
    tkEqual(liveThousands(1234567), "1,234,567")
end sub

sub test_live_categories()
    tkEqual(liveBrowseQuery("all", 0, 50), { category: "all", offset: 0, limit: 50 })
    tkEqual(liveBrowseQuery("favorites", 50, 50), { category: "favorites", offset: 50, limit: 50 })
    tkEqual(liveBrowseQuery("group:UK| Sport", 0, 50), { category: "group", group: "UK| Sport", offset: 0, limit: 50 })
    groups = liveGroups(liveSampleGroups())
    tkEqual(groups.Count(), 6, "one without a name is left out")
    tkEqual(groups[2].region, "")
    tkEqual(liveCategoryName("group:UK| Sport", groups), "UK · Sport")
    tkEqual(liveCategoryName("group:News", groups), "News")
    tkEqual(liveCategoryName("recent", groups), "Recently watched")
    tkEqual(liveCategoryName("all", groups), "All channels")
    summary = liveSummary(liveSampleGroups())
    tkEqual(summary, { total: 56171, favorites: 2, recent: 9 })
    tkEqual(liveCategoryValid("group:UK| News", summary, groups), "group:UK| News")
    tkEqual(liveCategoryValid("group:Kids", summary, groups), "all", "a group gone")
    tkEqual(liveCategoryValid("favorites", { total: 5, favorites: 0, recent: 0 }, groups), "all", "nothing marked any more")
    tkEqual(liveRegionOf("group:UK| News", groups), "UK")
    tkEqual(liveRegionOf("all", groups), "")
end sub

sub test_live_rail_gathers_regions()
    groups = liveGroups(liveSampleGroups())
    summary = liveSummary(liveSampleGroups())
    rail = liveRail(summary, groups, "", "")
    keys = []
    for each entry in rail
        keys.Push(entry.kind + ":" + entry.key)
    end for
    tkEqual(keys, ["category:favorites", "category:recent", "category:all", "heading:", "region:AR", "region:UK", "heading:", "category:group:News", "category:group:FR| Cinema"], "regions by name, then the rest in the provider's order, a region of one group with them")
    tkEqual(rail[3].label, "Regions")
    tkEqual(rail[6].label, "More groups")
    tkEqual(rail[5].label, "UK  ·  2 groups")
    tkEqual(rail[5].count, 160)
    tkEqual(rail[2].count, 56171)
    opened = liveRail(summary, groups, "UK", "")
    tkEqual(opened.Count(), 11)
    tkEqual(opened[6], { kind: "category", key: "group:UK| Sport", label: "Sport", count: 120, inner: true })
    plain = liveRail(summary, [groups[0], groups[1], groups[2]], "", "")
    tkEqual(plain.Count(), 7, "one region: one plain list")
    tkEqual(plain[3].label, "Groups")
    tkEqual(plain[4].label, "UK| Sport")
    tkEqual(liveRail(summary, [groups[2]], "", "").Count(), 3, "one group: no groups listed")
end sub

sub test_live_search()
    groups = liveGroups(liveSampleGroups())
    summary = liveSummary(liveSampleGroups())
    tkEqual(liveSearchLabel(""), "Search")
    tkEqual(liveSearchLabel("news"), "Search: “news”")
    rail = liveRail(summary, groups, "", "news")
    tkEqual(rail[0], { kind: "category", key: "group:UK| News", label: "UK · News", count: 40, inner: true }, "the groups whose names match, as links at the top")
    tkEqual(rail[1].key, "group:News")
    tkEqual(rail[2].key, "favorites")
    tkEqual(liveBrowseQuery("search:bbc one", 0, 50), { category: "all", q: "bbc one", offset: 0, limit: 50 })
    tkEqual(liveCategoryName("search:bbc", groups), "Results for “bbc”")
    tkEqual(liveCategoryValid("search:bbc", summary, groups), "search:bbc")
    tkEqual(liveGroupsMatching(groups, "  "), [])
end sub

sub test_live_search_goes_back_to_the_category_before()
    before = liveBeforeSearch("group:UK| Sport", "all")
    tkEqual(before, "group:UK| Sport", "the category shown as the search began")
    tkEqual(liveBeforeSearch("search:bbc", before), "group:UK| Sport", "kept through another search")
    tkEqual(liveBeforeSearch("favorites", before), "favorites", "a search from another category")
    tkEqual(liveBeforeSearch("", "favorites"), "all")
end sub

sub test_live_rail_items()
    groups = liveGroups(liveSampleGroups())
    rail = liveRail(liveSummary(liveSampleGroups()), groups, "UK", "")
    items = liveRailItems(rail, "UK", "group:UK| News")
    tkEqual(items[0], { title: "Favourites", countText: "2", mark: "", inner: false, heading: false, current: false })
    tkEqual(items[2].countText, "56,171")
    tkTrue(items[3].heading, "Regions")
    tkEqual(items[3].countText, "", "a heading has no count")
    tkEqual(items[4].mark, "+", "a region closed")
    tkEqual(items[5].mark, "−", "the region open")
    tkTrue(items[7].current, "the category shown")
    tkTrue(items[7].inner)
    tkEqual(liveRailPast(rail, 3, 2), 4, "past a heading, on")
    tkEqual(liveRailPast(rail, 3, 4), 2, "past a heading, back")
    tkEqual(liveRailPast(rail, 2, 1), 2, "a category stays")
    tkEqual(liveRailPast([{ kind: "heading" }, { kind: "category" }], 0, 1), 1, "a heading at the top: on")
    tkEqual(liveRailPast([{ kind: "category" }, { kind: "heading" }], 1, 0), 1, "a heading at the end stays")
end sub

sub test_live_rail_pages()
    kinds = ["category", "category", "heading", "region", "region", "region", "heading", "category", "category", "category"]
    tkEqual(liveRailPage(kinds, 0, 4, 1), 4)
    tkEqual(liveRailPage(kinds, 1, 4, 1), 5, "a heading is passed over, on")
    tkEqual(liveRailPage(kinds, 9, 4, -1), 5)
    tkEqual(liveRailPage(kinds, 7, 4, -1), 3, "a heading is passed over, back")
    tkEqual(liveRailPage(kinds, 8, 4, 1), 9, "no further than the end")
    tkEqual(liveRailPage(kinds, 1, 4, -1), 0, "no further than the start")
end sub

sub test_live_now_and_next()
    news = liveChannels(liveSample(), "")[0]
    tkEqual(liveOnNow(news, 1500).title, "Morning Briefing")
    tkEqual(liveComingUp(news, 1500).title, "World at One")
    tkEqual(liveOnNow(news, 2100).title, "World at One", "the next one, once it began")
    tkInvalid(liveComingUp(news, 2100))
    tkInvalid(liveOnNow(news, 900), "before the guide's now")
    tkEqual(liveComingUp(news, 900).title, "Morning Briefing")
    tkInvalid(liveOnNow(news, 3000), "after both")
    tkEqual(liveProgress(news.now, 1500), 0.5)
    tkEqual(liveProgress(news.now, 500), 0)
    tkEqual(liveProgress(news.now, 2500), 1)
    tkEqual(liveTimeLeft(news.now, 1500), "8 min left")
    tkEqual(liveTimeLeft({ start: 0, ends: 7200 }, 0), "2 h left")
    tkEqual(liveTimeLeft({ start: 0, ends: 4000 }, 0), "1 h 7 min left")
    tkEqual(liveTimeLeft({ start: 0, ends: 10 }, 0), "1 min left")
end sub

sub test_live_whats_on_moves_on()
    channels = liveChannels(liveSample(), "")
    tkFalse(liveNowEnded(channels, 0, 3, 1500), "on now still")
    tkTrue(liveNowEnded(channels, 0, 3, 2000), "a programme listed as on now ended")
    tkFalse(liveNowEnded(channels, 2, 3, 2500), "a channel without a guide has nothing to end")
    tkTrue(liveNowEnded(channels, -5, 99, 2500), "from and upto kept within the channels")
    tkTrue(liveMayRefresh(-1, 100))
    tkFalse(liveMayRefresh(1000, 60000), "within a minute")
    tkTrue(liveMayRefresh(1000, 61000))
end sub

sub test_live_clock_text()
    tkEqual(liveClockText(20, 5, true), "8:05 PM")
    tkEqual(liveClockText(0, 30, true), "12:30 AM")
    tkEqual(liveClockText(12, 0, true), "12:00 PM")
    tkEqual(liveClockText(9, 0, false), "09:00")
    tkEqual(liveClockText(20, 5, false), "20:05")
    i18nUse("fr")
    tkEqual(liveClockText(20, 5, fmtTwelveHour()), "20:05", "French writes 24 hours, whatever the Roku's clock")
    i18nUse("es")
    tkEqual(liveClockText(9, 5, fmtTwelveHour()), "9:05")
    tkEqual(liveClockText(21, 5, true), "9:05 p. m.")
end sub

' A page of channels with ids from first, of a list of total.
function liveSamplePage(offset as integer, count as integer, total as integer) as object
    channels = []
    for i = 1 to count
        channels.Push({ id: offset + i, name: "c" })
    end for
    return { total: total, offset: offset, channels: channels }
end function

sub test_live_zapping_pages_round_the_ends()
    one = liveSamplePage(0, 3, 3)
    tkEqual(liveZap(one, 1, 1).channel.id, 2)
    tkEqual(liveZap(one, 3, 1).channel.id, 1, "after the last, the first")
    tkEqual(liveZap(one, 1, -1).channel.id, 3, "before the first, the last")
    tkEqual(liveZap(one, 99, 1).channel.id, 1, "a channel not in the page")
    tkInvalid(liveZap(liveSamplePage(0, 0, 0), 1, 1))
    size = livePageSize()
    middle = liveSamplePage(size, size, size * 3 + 7)
    tkEqual(liveZap(middle, size + 2, -1).channel.id, size + 1)
    tkEqual(liveZap(middle, size * 2, 1), { offset: size * 2, last: false }, "the next page's first")
    tkEqual(liveZap(middle, size + 1, -1), { offset: 0, last: true }, "the page before's last")
    first = liveSamplePage(0, size, size * 3 + 7)
    tkEqual(liveZap(first, 1, -1), { offset: size * 3, last: true }, "before the first page, the last")
    last = liveSamplePage(size * 3, 7, size * 3 + 7)
    tkEqual(liveZap(last, size * 3 + 7, 1), { offset: 0, last: false }, "after the last page, the first")
    tkEqual(liveRange(50, 50, 2345), "Channels 51–100 of 2,345")
    tkEqual(liveRange(0, 5, 5), "")
end sub

sub test_live_cards_and_list_items()
    channels = liveChannels(liveSample(), "")
    card = liveCard(channels[0], 1500, false)
    tkEqual(card.line, "Morning Briefing")
    tkEqual(card.progress, 0.5)
    tkTrue(Instr(1, card.nextLine, " · World at One") > 0, "what's next, with its time")
    tkEqual(liveCard(channels[2], 1500, false).line, "No guide")
    tkEqual(liveCard(channels[1], 2500, false).line, "Nothing in the guide now")
    item = liveListItem(channels[0], 2100, false)
    tkEqual(item.line, "World at One")
    tkEqual(liveListItem(channels[0], 900, false).progress, 0)
    tkTrue(Instr(1, liveListItem(channels[0], 900, false).line, " · Morning Briefing") > 0, "what comes on, with its time")
    tkEqual(liveListItem(channels[2], 1500, false).line, "")
end sub

sub test_live_guide_programmes()
    kept = {}
    liveGuideKeep(kept, { "1": [{ start: 0, stop: 100, title: "A" }, { start: 100, stop: 200, title: "B" }] })
    liveGuideKeep(kept, { "1": [{ start: 100, stop: 200, title: "B" }, { start: 150, stop: 300, title: "C overlaps" }, { start: 300, stop: 400, title: "D" }], "2": [] })
    programs = liveGuidePrograms(kept, 1)
    titles = []
    for each p in programs
        titles.Push(p.title)
    end for
    tkEqual(titles, ["A", "B", "C overlaps", "D"], "each once, by start")
    tkEqual(programs[2].start, 200, "an overlap starts where the one before stops")
    tkEqual(liveGuidePrograms(kept, 2), [])
    rows = liveGuideRows(liveChannels(liveSample(), "http://tv"), kept, 0, 600)
    tkEqual(rows.Count(), 3)
    tkEqual(rows[0].fields.title, "101  Pley News HD")
    tkEqual(rows[0].fields.id, "1")
    tkEqual(rows[0].programs.Count(), 5, "four, then the time the guide says nothing of")
    tkEqual(rows[0].programs[3].playStart, 300)
    tkEqual(rows[0].programs[3].playDuration, 100)
    tkEqual(rows[0].programs[4].id, "gap")
    tkEqual(rows[0].programs[4].title, "No guide information")
    tkEqual(rows[0].programs[4].playDuration, 200)
    tkEqual(rows[2].programs.Count(), 1, "no guide: one gap over what is fetched")
    tkEqual(rows[2].programs[0].title, "Film Four", "a channel without a guide: its name across it")
    kept = {}
    liveGuideKeep(kept, { "1": [{ start: 1000, stop: 1030, title: "A" }, { start: 1100, stop: 1200, title: "B" }] })
    programs = liveGuideRows([{ id: 1, number: "", name: "x", logo: "", hasGuide: true }], kept, 900, 1200)[0].programs
    tkEqual(programs.Count(), 4, "gaps of a minute or more, before and between")
    tkEqual(programs[0].playDuration, 100)
    tkEqual(programs[2].playStart, 1030)
    tkEqual(liveIds(liveChannels(liveSample(), "")), "1,2,3")
end sub

sub test_live_guide_window()
    start = liveGuideStart(7300)
    tkEqual(start, 5400, "the half hour before the one now is in")
    tkInvalid(liveGuideMissing(0, 21600, 0, 7200), "the view and an hour more are fetched")
    tkEqual(liveGuideMissing(0, 21600, 12000, 7200), { from: 21600, to: 43200 })
    tkEqual(liveGuideMissing(0, 162000, 160000, 7200), { from: 162000, to: 172800 }, "not past the span")
    tkInvalid(liveGuideMissing(0, 172800, 170000, 7200), "the span fetched")
end sub

sub test_live_failed_pages_asked_again()
    tkTrue(liveMayAskAgain(-1, 500), "nothing failed")
    tkFalse(liveMayAskAgain(20000, 25000), "within 10 s")
    tkTrue(liveMayAskAgain(20000, 30000), "once 10 s have passed")
end sub

sub test_live_guide_back_to_now()
    now = 10000
    tkFalse(liveGuideAway(9000, 7200, 9000, 10800, now), "on what's on now")
    tkTrue(liveGuideAway(9000, 7200, 10800, 12600, now), "on what comes after")
    tkTrue(liveGuideAway(9000, 7200, 7200, 9000, now), "on what ended")
    tkTrue(liveGuideAway(18000, 7200, 18000, 19800, now), "hours on")
    tkTrue(liveGuideAway(1000, 7200, 9000, 10800, now), "now out of the time shown")
    tkFalse(liveGuideAway(9000, 7200, 0, 0, now), "no programme: the time shown counts")
    tkFalse(liveGuideAway(now, 7200, 0, 86400, now), "time without a guide, across now")
end sub

sub test_live_troubles()
    busy = liveTrouble({ status: 409, message: "The one connection of Test Provider is in use, watching Pley News HD (lab)." }, "Film Four")
    tkEqual(busy.kind, "busy")
    tkEqual(busy.title, "All connections are in use")
    tkEqual(busy.message, "The one connection of Test Provider is in use, watching Pley News HD (lab).")
    tkTrue(busy.retry)
    tkEqual(liveTrouble({ status: 404, message: "" }, "x").kind, "missing")
    tkFalse(liveTrouble({ status: 404, message: "" }, "x").retry)
    tkEqual(liveTrouble({ status: 503, message: "the channel didn't start" }, "Film Four").title, "Film Four isn't playing")
    tkEqual(liveTrouble({ status: 500, message: "boom" }, "Film Four").title, "Film Four didn't start")
    tkEqual(liveTroubleButtons(busy), [{ label: "Try again", action: "retry", channelId: 0 }, { label: "Other channels", action: "list", channelId: 0 }, { label: "Back", action: "back", channelId: 0 }], "without what holds the connections")
end sub

sub test_live_retries()
    tkTrue(liveRetried(liveTrouble({ status: 503, message: "the channel didn't start" }, "x")), "the provider sent nothing yet")
    tkTrue(liveRetried(liveTrouble({ status: 500, message: "boom" }, "x")))
    tkFalse(liveRetried(liveTrouble({ status: 409, message: "", data: { connections: [] } }, "x")), "every connection in use")
    tkFalse(liveRetried(liveTrouble({ status: 410, message: "", data: invalid }, "x")), "stopped on the server")
    tkFalse(liveRetried(liveTrouble({ status: 404, message: "" }, "x")), "gone")
    tkFalse(liveRetried(liveTrouble({ status: 403, message: "" }, "x")), "not open to the account")
    tkEqual(liveRetryDelay(0), 1000, "after a second")
    tkEqual(liveRetryDelay(1), 3000, "then three")
    tkEqual(liveRetryDelay(2), -1, "then the player says why")
end sub

sub test_live_stopped_on_the_server()
    ' Taken over, or stopped there: the server's words, with Watch again (as any watch, never taking a connection).
    taken = liveTrouble({ status: 410, message: "Gone.", data: { code: "session_stopped", message: "Alex is watching Two HD now, on the connection this channel used." } }, "One")
    tkEqual(taken.kind, "stopped")
    tkEqual(taken.title, "Live TV stopped")
    tkEqual(taken.lines, "Alex is watching Two HD now, on the connection this channel used.")
    tkTrue(taken.retry, "watching again may well work")
    tkEqual(liveTroubleButtons(taken), [{ label: "Watch again", action: "retry", channelId: 0 }, { label: "Other channels", action: "list", channelId: 0 }, { label: "Back", action: "back", channelId: 0 }])
    ' Without words: not a status's sentence.
    bare = liveTrouble({ status: 410, message: "It was stopped.", data: { code: "session_stopped", message: invalid } }, "One")
    tkEqual(bare.message, "This channel isn't being watched any more.")
    tkEqual(liveStopped("").lines, "This channel isn't being watched any more.")
    tkEqual(liveStopped(invalid).message, "This channel isn't being watched any more.")
    tkEqual(liveStopped("Watching moved to Two HD on another device.").message, "Watching moved to Two HD on another device.")
end sub

sub test_live_busy_connections()
    data = { code: "conflict", message: "Test Provider allows 2 connections at once, and all are in use: watching Pley News HD, Sport 1080i.", connections: [
        { channel_id: 1, channel_name: "Pley News HD", watchers: [], people: 1, yours: true, can_take_over: true },
        { channel_id: 2, channel_name: "Sport 1080i", watchers: ["anna", "ben"], people: 2, yours: false, can_take_over: true },
        { channel_id: 3, channel_name: "Film Four", watchers: [], people: 3, yours: false, can_take_over: false },
        { channel_id: 0, channel_name: "no id" }
    ] }
    busy = liveTrouble({ status: 409, message: data.message, data: data }, "Radio Jazz")
    tkEqual(busy.connections.Count(), 3)
    tkEqual(busy.connections[0].holders, "You, on another device")
    tkEqual(busy.connections[1].holders, "anna, ben", "named to administrators")
    tkEqual(busy.connections[2].holders, "3 people")
    tkEqual(liveHolders({ people: 1, watchers: [] }), "Someone else")
    tkEqual(busy.lines, data.message + Chr(10) + Chr(10) + "Its 3 connections are watching:" + Chr(10) + "•  Pley News HD  —  You, on another device" + Chr(10) + "•  Sport 1080i  —  anna, ben" + Chr(10) + "•  Film Four  —  3 people")
    buttons = liveTroubleButtons(busy)
    tkEqual(buttons.Count(), 5)
    tkEqual(buttons[0], { label: "Watch here instead of Pley News HD", action: "takeOver", channelId: 1 })
    tkEqual(buttons[1].channelId, 2)
    tkEqual(buttons[2].action, "retry")
    one = liveTrouble({ status: 409, message: "", data: { connections: [{ channel_id: 7, channel_name: "Sport 1080i", watchers: [], people: 1, yours: true, can_take_over: true }] } }, "x")
    tkEqual(liveTroubleButtons(one)[0].label, "Watch here instead", "one to take: its channel goes without saying")
    tkEqual(one.lines, "Its one connection is watching:" + Chr(10) + "•  Sport 1080i  —  You, on another device")
    none = liveTrouble({ status: 409, message: "", data: { connections: [{ channel_id: 7, channel_name: "Sport 1080i", watchers: [], people: 2, yours: false, can_take_over: false }] } }, "x")
    tkEqual(none.lines, "Its one connection is watching:" + Chr(10) + "•  Sport 1080i  —  2 people" + Chr(10) + Chr(10) + "A channel already being watched can be joined, or try again once one is stopped.", "none to take over: what can be done instead")
    tkEqual(liveTroubleButtons(none)[0].action, "retry")
end sub

sub test_live_words_in_french()
    i18nUse("fr")
    nbsp = Chr(160)
    groups = liveGroups(liveSampleGroups())
    summary = liveSummary(liveSampleGroups())
    rail = liveRail(summary, groups, "", "")
    tkEqual(rail[0].label, "Favoris")
    tkEqual(rail[2].label, "Toutes les chaînes")
    tkEqual(rail[3].label, "Régions")
    tkEqual(rail[5].label, "UK  ·  2 groupes")
    tkEqual(rail[6].label, "Autres groupes")
    tkEqual(liveRailItems(rail, "", "all")[2].countText, "56" + Chr(8239) + "171", "a narrow no-break space between thousands")
    tkEqual(liveCategoryName("search:bbc", groups), "Résultats pour «" + nbsp + "bbc" + nbsp + "»")
    tkEqual(liveSearchLabel("news"), "Rechercher" + nbsp + ": «" + nbsp + "news" + nbsp + "»")
    tkEqual(liveRange(50, 50, 2345), "Chaînes 51–100 sur 2" + Chr(8239) + "345")
    tkEqual(liveTimeLeft({ start: 0, ends: 4000 }, 0), "Il reste 1 h 7 min")
    tkEqual(liveTimeLeft({ start: 0, ends: 60 }, 0), "Il reste 1 min")
    channels = liveChannels(liveSample(), "")
    tkEqual(channels[0].after.badges, "Inédit · TV-PG")
    tkEqual(liveCard(channels[2], 1500, false).line, "Pas de guide")
    tkEqual(liveProgram({ start: 0, stop: 10 }).title, "Sans titre")
    tkEqual(liveChannel({ id: 9 }, "").name, "Chaîne 9")
    stopped = liveTrouble({ status: 410, message: "", data: invalid }, "One")
    tkEqual(stopped.title, "La TV en direct s’est arrêtée")
    tkEqual(stopped.lines, "Cette chaîne n’est plus regardée.")
    tkEqual(liveTrouble({ status: 503, message: "" }, "Film Four").title, "Film Four est indisponible")
    tkEqual(liveStoppedBy({ by_admin: false, message: "" }).lines, "La TV en direct a été arrêtée depuis un autre appareil.")
end sub

sub test_live_words_in_spanish()
    i18nUse("es")
    groups = liveGroups(liveSampleGroups())
    rail = liveRail(liveSummary(liveSampleGroups()), groups, "", "")
    tkEqual(rail[1].label, "Vistos recientemente")
    tkEqual(rail[5].label, "UK  ·  2 grupos")
    tkEqual(liveRailItems(rail, "", "all")[2].countText, "56.171")
    tkEqual(liveRange(50, 50, 2345), "Canales 51–100 de 2345", "Spanish groups thousands from five digits")
    tkEqual(liveTimeLeft({ start: 0, ends: 60 }, 0), "Queda 1 min")
    tkEqual(liveTimeLeft({ start: 0, ends: 500 }, 0), "Quedan 8 min")
    tkEqual(liveTimeLeft({ start: 0, ends: 7200 }, 0), "Quedan 2 h")
    channels = liveChannels(liveSample(), "")
    tkEqual(liveCard(channels[1], 2500, false).line, "Nada en la guía ahora")
    tkEqual(liveTrouble({ status: 500, message: "boom" }, "Film Four").title, "Film Four no se inició")
    data = { connections: [
        { channel_id: 1, channel_name: "Pley News HD", watchers: [], people: 1, yours: true, can_take_over: true },
        { channel_id: 2, channel_name: "Sport 1080i", watchers: [], people: 3, yours: false, can_take_over: true }
    ] }
    busy = liveTrouble({ status: 409, message: "", data: data }, "x")
    tkEqual(busy.title, "Todas las conexiones están en uso")
    tkEqual(busy.lines, "Sus 2 conexiones están ocupadas con:" + Chr(10) + "•  Pley News HD  —  Tú, en otro dispositivo" + Chr(10) + "•  Sport 1080i  —  3 personas")
    labels = []
    for each button in liveTroubleButtons(busy)
        labels.Push(button.label)
    end for
    tkEqual(labels, ["Ver aquí en lugar de Pley News HD", "Ver aquí en lugar de Sport 1080i", "Volver a intentar", "Otros canales", "Atrás"])
    one = liveTrouble({ status: 409, message: "", data: { connections: [{ channel_id: 7, channel_name: "Sport 1080i", watchers: [], people: 2, yours: false, can_take_over: false }] } }, "x")
    tkEqual(one.lines, "Su única conexión está ocupada con:" + Chr(10) + "•  Sport 1080i  —  2 personas" + Chr(10) + Chr(10) + "Puedes unirte a un canal que ya se está viendo, o volver a intentarlo cuando se detenga alguno.")
    tkEqual(liveTroubleButtons(liveStopped(""))[0].label, "Volver a ver")
end sub

sub test_live_view_kept()
    section = tkRegistrySection()
    key = liveViewKey("http://tv:7539", 2)
    tkEqual(liveLoadView(section, key), { tab: "guide", category: "all", channel: 0 })
    liveSaveView(section, key, { tab: "channels", category: "group:UK| Sport" })
    tkEqual(liveLoadView(section, key), { tab: "channels", category: "group:UK| Sport", channel: 0 })
    section.Write(key, "{""tab"":""elsewhere"",""category"":""group:"",""channel"":""7""}")
    tkEqual(liveLoadView(section, key), { tab: "guide", category: "all", channel: 0 })
    section.Write(key, "garbage")
    tkEqual(liveLoadView(section, key), { tab: "guide", category: "all", channel: 0 })
end sub

sub test_live_last_channel_kept()
    section = tkRegistrySection()
    key = liveViewKey("http://tv:7539", 2)
    liveSaveView(section, key, { tab: "channels", category: "favorites", channel: 0 })
    liveSaveChannel(section, key, 101)
    tkEqual(liveLoadView(section, key), { tab: "channels", category: "favorites", channel: 101 }, "the tab and the category stay")
    liveSaveView(section, key, { tab: "guide", category: "recent", channel: 0 })
    tkEqual(liveLoadView(section, key).channel, 101, "the channel stays as the tab changes")
    flushes = section.flushes
    liveSaveChannel(section, key, 101)
    tkEqual(section.flushes, flushes, "the same channel isn't written again")
    liveSaveChannel(section, key, 0)
    tkEqual(liveLoadView(section, key).channel, 101, "no channel keeps the one before")
    tkEqual(liveLoadView(section, liveViewKey("http://tv:7539", 3)).channel, 0, "another profile's own")
end sub
