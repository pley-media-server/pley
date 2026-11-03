import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func api(_ server: FixtureServer) -> PleyAPI {
    let http = transport()
    return PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                     transport: http))
}

@Suite struct LiveAPITests {
    @Test func browseAsksForACategoryPageOrTheOneAroundAChannel() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLiveLineup(count: 250)
        let api = api(server)
        let second = try await api.liveBrowse(.all, offset: 100, limit: 100)
        #expect(second.total == 250 && second.offset == 100 && second.channels.first?.name == "Channel 0101")
        var asked = try #require(server.requests("/api/live/browse").last)
        #expect(asked.query == ["category": "all", "offset": "100", "limit": "100"])

        let group = try await api.liveBrowse(.group("UK| Sport"), query: " 0002 ", limit: 100)
        #expect(group.channels.map(\.name) == ["Channel 0002"])
        asked = try #require(server.requests("/api/live/browse").last)
        #expect(asked.query["category"] == "group" && asked.query["group"] == "UK| Sport" && asked.query["q"] == "0002")

        let around = try await api.liveBrowse(.all, limit: 100, around: 1001 + 230)
        #expect(around.offset == 200 && around.channels.contains { $0.id == 1231 })
        asked = try #require(server.requests("/api/live/browse").last)
        #expect(asked.query["around"] == "1231" && asked.query["offset"] == nil)
    }

    @Test func theGuideAsksForTheChannelsInViewOnly() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLiveLineup(count: 20)
        let api = api(server)
        let from: Int64 = 1_800_000_000 / 21_600 * 21_600
        let guide = try await api.liveGuide(from: from, to: from + 21_600, ids: [1001, 1002])
        #expect(Set(guide.programs.keys) == [1001, 1002])
        #expect(guide.programs[1001]?.count == 12)
        let asked = try #require(server.requests("/api/live/guide").last)
        #expect(asked.query["ids"] == "1001,1002" && asked.query["from"] == String(from))
    }

    @Test func watchSendsNoCapsAndGivesAKeyedPlaylist() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        let watch = try await api.watchChannel(1, LiveWatchRequest(clientId: "apple-test"))
        #expect(watch.channel?.name == "Pley News HD")
        let body = try #require(server.liveWatchRequests.last)
        let sent = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(Set(sent.keys) == ["client_id", "keyed_urls"])
        let url = try await api.liveMediaURL(watch.url)
        #expect(url.absoluteString.hasPrefix(server.origin + "/api/live/media/pley_mk_"))
        // Anything but a keyed live path is refused.
        for bad in ["/api/live/sessions/x/index.m3u8", "/api/media/k/index.m3u8", "/api/live/media/../x/a.m3u8",
                    "https://elsewhere.example/api/live/media/k/index.m3u8"] {
            await #expect(throws: PleyError.self) { try await api.liveMediaURL(bad) }
        }
        #expect(try await api.livePlan(watch.sessionId)?.video == "copy")
        server.setLivePlanKnown(false)
        #expect(try await api.livePlan(watch.sessionId) == nil)
        try await api.leaveChannel(watch.sessionId)
        #expect(server.liveSessions.isEmpty)
    }

    @Test func everyConnectionInUseSaysWhatHoldsThemAndATakeOverTakesOne() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.holdLiveConnection(2)
        let api = api(server)
        do {
            _ = try await api.watchChannel(1, LiveWatchRequest(clientId: "apple-test"))
            Issue.record("a busy source watched")
        } catch PleyError.busy(let message, let connections) {
            #expect(message?.contains("Sport 1080i") == true)
            #expect(connections.map(\.channelId) == [2] && connections[0].canTakeOver)
        }
        let watch = try await api.watchChannel(1, LiveWatchRequest(clientId: "apple-test", takeOver: 2))
        #expect(server.liveSessions[watch.sessionId] == 1)
        let last = try #require(server.liveWatchRequests.last)
        let sent = try #require(try JSONSerialization.jsonObject(with: last) as? [String: Any])
        #expect(sent["take_over"] as? Int == 2)
    }

    @Test func aStoppedSessionsPlanSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        let watch = try await api.watchChannel(1, LiveWatchRequest(clientId: "apple-test"))
        server.stopLiveSession(watch.sessionId)
        await #expect(throws: PleyError.sessionStopped(message: "An administrator stopped this channel.")) {
            _ = try await api.livePlan(watch.sessionId)
        }
    }

    @Test func favouritesAndSummary() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        let summary = try await api.liveSummary()
        #expect(summary.available && summary.channels >= 2)
        try await api.setFavoriteChannel(1, favorite: true)
        #expect(server.liveFavorites.contains(1))
        try await api.setFavoriteChannel(1, favorite: false)
        #expect(!server.liveFavorites.contains(1))
        #expect(try await api.liveChannel(2).favorite)
        server.setLiveAllowed(false)
        #expect(try await api.liveSummary().available == false)
        await #expect(throws: PleyError.self) { _ = try await api.liveGroups() }
    }
}
