import Foundation
import PleyModel
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, fixture path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

@Suite("Live TV's answers decode as the Android app reads them")
struct LiveDecodingTests {
    @Test func summary() throws {
        let summary = try decode(LiveSummary.self, fixture: "live/summary.json")
        #expect(summary.available && summary.allowed)
        #expect(summary.channels == 3 && summary.sources == 0)
        #expect(summary.recent == [2, 1])
    }

    @Test func channelsWithWhatIsOn() throws {
        let channels = try decode([LiveChannel].self, fixture: "live/channels.json")
        #expect(channels.count >= 2)
        let news = channels[0]
        #expect(news.name == "Pley News HD" && news.number == "101" && news.group == "News")
        #expect(news.logo == "/api/live/channels/1/logo" && !news.favorite && news.hasGuide)
        let now = try #require(news.now)
        #expect(now.title == "Morning Briefing" && now.subtitle == "Part 7" && now.isNew && !now.isLive)
        #expect(now.episodeLabel == "S1 E7")
        #expect(news.next?.title == "World at One")
        #expect(news.numberedName == "101 Pley News HD")
        let sport = channels[1]
        #expect(sport.favorite && sport.now?.isLive == true)
        #expect(sport.now?.image == "/api/live/channels/2/programs/1761750000/image")
    }

    @Test func whatIsOnAtATime() throws {
        let news = try decode([LiveChannel].self, fixture: "live/channels.json")[0]
        let now = try #require(news.now)
        #expect(news.on(at: now.start)?.title == "Morning Briefing")
        #expect(news.after(at: now.start)?.title == "World at One")
        // Once now ended, its next is on; nothing comes after it in the listing.
        #expect(news.on(at: now.stop)?.title == "World at One")
        #expect(news.after(at: now.stop) == nil)
        // Before it begins, nothing is on and now comes next.
        #expect(news.on(at: now.start - 60) == nil)
        #expect(news.after(at: now.start - 60)?.title == "Morning Briefing")
        #expect(now.progress(at: now.start + (now.stop - now.start) / 2) == 0.5)
        #expect(now.progress(at: now.stop + 999) == 1)
    }

    @Test func groupsAndPages() throws {
        let groups = try decode(LiveGroups.self, fixture: "live/groups.json")
        #expect(groups.total == 3 && groups.favorites == 1 && groups.recent == 2)
        #expect(groups.groups.map(\.name) == ["News", "Sport", "UK| Sport", "UK| News", "AR| Kids"])
        #expect(groups.groups[2].region == "UK" && groups.groups[2].label == "Sport" && groups.groups[2].count == 80)
        #expect(groups.groups[0].region == nil && groups.groups[0].label == "News")
        let page = try decode(LiveChannelPage.self, fixture: "live/browse.json")
        #expect(page.total == 57_176 && page.offset == 100)
        #expect(page.channels.first?.name == "UK| Sky Sports Main Event")
        #expect(page.channels.first?.now == nil)
    }

    @Test func guideByChannel() throws {
        let guide = try decode(LiveGuide.self, fixture: "live/guide.json")
        #expect(guide.from == 1_761_742_800 && guide.to == 1_761_764_400)
        #expect(guide.programs[1]?.map(\.title) == ["Early News", "Morning Briefing"])
        #expect(guide.programs[2]?.first?.rating == "PG")
        #expect(guide.programs[1]?.first?.categories == [])
    }

    @Test func watchingAndPlan() throws {
        let watching = try decode(LiveWatch.self, fixture: "live/watching.json")
        #expect(watching.sessionId == "5d0f7f2a9c8e4b1d8a6e3f2b1c0d9e8f")
        #expect(watching.url.hasSuffix("/index.m3u8") && watching.kind == "hls")
        #expect(watching.channel?.name == "Pley News HD")
        let plan = try decode(LivePlan.self, fixture: "live/plan.json")
        #expect(plan.videoCodec == "h264" && plan.width == 1280 && plan.frameRate == 25)
        #expect(plan.video == "copy" && plan.audio == "copy" && !plan.interlaced)
    }

    @Test func busyConnections() throws {
        let body = try decode(APIErrorBody.self, fixture: "live/busy.json")
        #expect(body.code == "conflict")
        let connection = try #require(body.connections.first)
        #expect(connection.channelId == 2 && connection.channelName == "Sport 1080i")
        #expect(connection.people == 1 && connection.yours && connection.canTakeOver)
    }

    @Test func theWatchRequestSendsNoCaps() throws {
        let body = try PleyJSON.encoder.encode(LiveWatchRequest(clientId: "apple-1", takeOver: 2))
        let object = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(Set(object.keys) == ["client_id", "keyed_urls", "take_over"])
        #expect(object["keyed_urls"] as? Bool == true && object["take_over"] as? Int == 2)
        let plain = try PleyJSON.encoder.encode(LiveWatchRequest(clientId: "apple-1"))
        #expect(String(decoding: plain, as: UTF8.self) == #"{"client_id":"apple-1","keyed_urls":true}"#)
    }

    @Test func categoriesKeepTheirKeys() {
        for category in [LiveCategory.all, .favorites, .recent, .group("UK| Sport")] {
            #expect(LiveCategory.parse(category.key) == category)
        }
        #expect(LiveCategory.group("UK| Sport").key == "group:UK| Sport")
        #expect(LiveCategory.group("x").wire == "group")
        #expect(LiveCategory.parse(nil) == .all && LiveCategory.parse("nonsense") == .all)
    }

    @Test func searchReadsChannelsWhole() throws {
        let results = try decode(SearchResults.self, fixture: "search/results.json")
        #expect(results.channels.first?.name == "Dune Classics" && results.channels.first?.number == "212")
    }
}
