import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

/// The open server's API, signed in with `scope`.
private func api(_ server: FixtureServer, scope: String = FixtureServer.allScopes) async throws -> PleyAPI {
    let transport = HTTPTransport(userAgent: "pleyApple/9.9.9 (iOS 18.1; iPhone)")
    let tokens = server.issueTokens(scope: scope)
    let store = MemoryTokenStore()
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken, scope: scope))
    let session = SessionManager(transport: transport, tokenStore: store,
                                 servers: ServerRepository(store: MemoryServerListStore()), client: .ios)
    _ = try await session.open(origin: server.origin)
    return try #require(await session.api)
}

private func started() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

private func settings() -> AppSettings {
    let name = "pley-requests-\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: name)!
    defaults.removePersistentDomain(forName: name)
    return AppSettings(defaults: defaults)
}

/// Waits until `condition` holds, for at most 5 seconds.
@MainActor
private func soon(_ condition: @MainActor () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return condition()
}

@MainActor
@Suite struct RequestsRepositoryTests {
    @Test func theSummaryIsReadAtTheStartAndWhatCameMeanwhileIsToldOnce() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setRequestsUnseen(1)
        let saved = settings()
        let repository = RequestsRepository(api: try await api(server), settings: saved)
        var news: [RequestNews] = []
        repository.onNews = { news.append($0) }
        #expect(repository.gate == .unknown)
        repository.start(origin: server.origin, profileId: 2)
        #expect(await soon { repository.summary != nil })
        #expect(repository.gate.mayAsk)
        // `unseen` is 1: Arrival became ready while the app was away.
        #expect(await soon { news.count == 1 })
        #expect(news.first.map(RequestWords.notice) == "“Arrival” is ready to watch.")
        #expect(server.requests("/api/requests").last?.query["unseen"] == "1")
        #expect(saved.requestNewsMark(origin: server.origin, profileId: 2) > 0)
        // Started again (the app opened again): told already.
        let again = RequestsRepository(api: try await api(server), settings: saved)
        var later: [RequestNews] = []
        again.onNews = { later.append($0) }
        again.start(origin: server.origin, profileId: 2)
        #expect(await soon { again.summary != nil })
        try await Task.sleep(for: .milliseconds(150))
        #expect(later.isEmpty)
    }

    @Test func anOlderServerIsTakenAsRequestsOff() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setRequestsMissing(true)
        let repository = RequestsRepository(api: try await api(server), settings: settings())
        repository.start(origin: server.origin, profileId: 2)
        #expect(await soon { repository.summary != nil })
        #expect(repository.gate == .hidden)
        #expect(RequestsRepository.olderServer(PleyError.insufficientScope(scope: nil)))
        #expect(!RequestsRepository.olderServer(PleyError.insufficientScope(scope: "requests")))
    }

    @Test func eventsReadTheSummaryAgainOnceAndActingAtOnce() async throws {
        let server = try await started()
        defer { server.stop() }
        let repository = RequestsRepository(api: try await api(server), settings: settings())
        var changes: [RequestChange] = []
        repository.onChange = { changes.append($0) }
        repository.start(origin: server.origin, profileId: 2)
        #expect(await soon { repository.summary != nil })
        let reads = server.requests("/api/requests/summary").count
        for id in [1, 2, 3] { repository.heard(RequestChange(ids: [id])) }
        #expect(await soon { server.requests("/api/requests/summary").count == reads + 1 })
        try await Task.sleep(for: .milliseconds(150))
        #expect(server.requests("/api/requests/summary").count == reads + 1)
        repository.acted(id: 44, kind: .movie, tmdbId: 933_260)
        #expect(changes == [RequestChange(id: 44, kind: .movie, tmdbId: 933_260)])
        #expect(await soon { server.requests("/api/requests/summary").count == reads + 2 })
    }

    @Test func theBadgeClearsAtOnceWhenSeen() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setRequestsUnseen(1)
        let repository = RequestsRepository(api: try await api(server), settings: settings())
        repository.start(origin: server.origin, profileId: 2)
        #expect(await soon { repository.summary?.unseen == 1 })
        repository.zeroUnseen()
        #expect(repository.summary?.unseen == 0)
        repository.reset()
        #expect(repository.summary == nil && repository.gate == .unknown)
    }

    @Test func caughtUpNewsSaysReadyOrDeclined() throws {
        let list = try PleyJSON.decoder.decode(RequestList.self, from: Fixtures.data("requests/list.json"))
        let ready = try #require(list.requests.first { $0.id == 45 })
        #expect(RequestsRepository.news(ready) == .ready(requestId: 45, title: ready.title, kind: .movie,
                                                         itemId: ready.itemId, seasons: nil, complete: true))
        let declined = try #require(list.requests.first { $0.id == 46 })
        #expect(RequestsRepository.news(declined) == .declined(requestId: 46, title: declined.title,
                                                               reason: declined.reason))
        #expect(RequestsRepository.news(try #require(list.requests.first { $0.id == 44 })) == nil)
    }
}

@MainActor
@Suite struct RequestEventsTests {
    private func event(_ name: String) throws -> ServerEvent {
        try #require(ServerEvent.parse(try Fixtures.data("events/\(name).json")))
    }

    @Test func requestEventsAreChangesAndSomeAreNews() async throws {
        let events = ServerEvents(window: 0.01)
        var news: [RequestNews] = []
        var heard: [RequestChange] = []
        var changes: [ServerChange] = []
        var settings = 0
        events.onRequestNews = { news.append($0) }
        events.onRequestChange = { heard.append($0) }
        events.onRequestsSettings = { settings += 1 }
        events.onChange = { changes.append($0) }
        events.heard(try event("requests_changed"))
        #expect(news.isEmpty && heard == [RequestChange(id: 42, kind: .show, tmdbId: 1396)])
        events.heard(try event("request_available"))
        #expect(news.last == .ready(requestId: 42, title: "Breaking Bad", kind: .show, itemId: 230, seasons: [1, 2],
                                    complete: false))
        events.heard(try event("request_decided"))
        #expect(news.last == .declined(requestId: 46, title: "Alien: Romulus", reason: "We have enough aliens for now"))
        events.heard(try event("request_created"))
        #expect(news.last == .asked(requestId: 49, title: "Arcane", by: "Kit", seasons: [1, 2, 3, 5]))
        // Approved without waiting: managers aren't told.
        events.heard(.requestCreated(requestId: 51, title: "X", kind: .movie, tmdbId: 1, seasons: nil, by: nil,
                                     status: .approved))
        #expect(news.count == 3)
        events.heard(try event("request_failed"))
        #expect(news.last == .failed(requestId: 50, title: "Inside Out 2", kind: .movie, reason: "Radarr refused the key"))
        // The failure names no title: only its request.
        #expect(heard.last == RequestChange(ids: [50]))
        events.heard(try event("requests_settings_changed"))
        #expect(settings == 1 && heard.last == .everything)
        try await Task.sleep(for: .milliseconds(80))
        // Gathered: one change for them all.
        #expect(changes.count == 1)
        let change = try #require(changes.first)
        #expect(change.requests.all && change.requests.ids.isSuperset(of: [42, 46, 49, 50, 51]))
        #expect(change.requests.about(.show, tmdbId: 1396) && change.requests.about(request: 46))
    }

    @Test func theStreamSaysWhenItIsDownAndAReconnectReadsRequestsAgain() async throws {
        let events = ServerEvents(window: 0.01)
        var connection: [Bool] = []
        var changes: [ServerChange] = []
        events.onConnection = { connection.append($0) }
        events.onChange = { changes.append($0) }
        events.heard(.disconnected)
        #expect(connection.isEmpty)
        events.heard(.connected)
        #expect(events.connected)
        events.heard(.disconnected)
        events.heard(.disconnected)
        #expect(connection == [true, false] && !events.connected)
        events.heard(.connected)
        try await Task.sleep(for: .milliseconds(80))
        #expect(changes.count == 1 && changes[0].requests == .everything)
    }

    @Test func aChangeNamesItsRequestAndTitle() {
        var change = RequestChange(id: 44, kind: .movie, tmdbId: 933_260)
        #expect(!change.all && change.about(request: 44) && change.about(.movie, tmdbId: 933_260))
        #expect(!change.about(.show, tmdbId: 933_260) && !change.about(request: 45))
        // Nothing named: everything.
        #expect(RequestChange(id: nil, kind: nil, tmdbId: nil).all)
        #expect(RequestChange().isEmpty && !RequestChange.everything.isEmpty)
        change.merge(RequestChange(ids: [45]))
        #expect(change.ids == [44, 45] && !change.all)
        change.merge(.everything)
        #expect(change.all)
    }
}
