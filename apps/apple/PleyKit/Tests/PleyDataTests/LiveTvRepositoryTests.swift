import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func liveRepository(_ server: FixtureServer) async throws -> LiveTvRepository {
    let transport = HTTPTransport(userAgent: "pleyApple/9.9.9 (iOS 18.1; iPhone)")
    let tokens = server.issueTokens()
    let store = MemoryTokenStore()
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken,
        scope: FixtureServer.allScopes))
    let session = SessionManager(transport: transport, tokenStore: store,
                                 servers: ServerRepository(store: MemoryServerListStore()), client: .ios)
    _ = try await session.open(origin: server.origin)
    return await LiveTvRepository(api: {
        guard let api = await session.api else { throw PleyError.signedOut }
        return api
    })
}

@MainActor
@Suite struct LiveTvRepositoryTests {
    @Test func aFavouriteShowsAtOnceAndIsPutBackWhenRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let live = try await liveRepository(server)
        let news = try await live.channel(1)
        #expect(!news.favorite)
        try await live.setFavorite(1, true)
        #expect(live.marked(news).favorite)
        #expect(server.liveFavorites.contains(1))
        #expect(try await live.page(.favorites, page: 0).channels.map(\.id).contains(1))

        server.refuseLiveFavorite(status: 503)
        let before = live.version
        await #expect(throws: PleyError.self) { try await live.setFavorite(1, false) }
        // Put back: still a favourite here and on the server, and nothing asked again.
        #expect(live.marked(news).favorite)
        #expect(server.liveFavorites.contains(1))
        #expect(live.version == before)
    }

    @Test func theCountsOfFavouritesOnTheirWayIncludeThem() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let live = try await liveRepository(server)
        let before = try await live.groups().favorites
        // Refused after a while: the server's count never has it.
        server.refuseLiveFavorite(status: 503)
        server.delay(path: "/api/live/channels/1/favorite", ms: 300)
        let saving = Task { try await live.setFavorite(1, true) }
        try await Task.sleep(for: .milliseconds(100))
        #expect(try await live.groups().favorites == before + 1)
        await #expect(throws: PleyError.self) { try await saving.value }
        #expect(try await live.groups().favorites == before)
    }

    @Test func aNumberIsFoundAmongEveryChannelExactly() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLiveLineup(count: 150)
        let live = try await liveRepository(server)
        #expect(try await live.numbered("12")?.name == "Channel 0012")
        #expect(try await live.numbered("999") == nil)
    }

    @Test func watchingGivesTheKeyedAddressAndLeavingEndsIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let live = try await liveRepository(server)
        let (watch, url) = try await live.watch(1, clientId: "apple-test")
        #expect(url.path.hasPrefix("/api/live/media/") && url.path.hasSuffix("/index.m3u8"))
        #expect(server.liveSessions[watch.sessionId] == 1)
        live.leave(watch.sessionId)
        for _ in 0..<200 where !server.liveSessions.isEmpty { try await Task.sleep(for: .milliseconds(10)) }
        #expect(server.liveSessions.isEmpty)
    }

    @Test func liveViewsAreKeptPerServerAndProfile() {
        let name = "pley-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        let settings = AppSettings(defaults: defaults)
        #expect(settings.liveView(origin: "http://a", profileId: 2) == AppSettings.LiveView())
        settings.keepLiveView(origin: "http://a", profileId: 2, category: "group:UK| Sport")
        settings.keepLiveView(origin: "http://a", profileId: 2, view: "channels")
        settings.keepLiveView(origin: "http://a", profileId: 2, channel: 101)
        #expect(settings.liveView(origin: "http://a", profileId: 2)
            == AppSettings.LiveView(category: "group:UK| Sport", view: "channels", channel: 101))
        #expect(settings.liveView(origin: "http://a", profileId: 3) == AppSettings.LiveView())
        #expect(settings.liveView(origin: "http://b", profileId: 2) == AppSettings.LiveView())
        #expect(defaults.string(forKey: "pley.settings.live.http://a.2.category") == "group:UK| Sport")
    }
}

private func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}
