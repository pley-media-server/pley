import Foundation
import PleyAuth
@testable import PleyData
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func folder() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("pley-snapshots-\(UUID().uuidString)")
}

@Suite struct SnapshotStoreTests {
    @Test func aSnapshotIsKeptPerServerAndProfileAndWrittenWhole() async throws {
        let root = folder()
        defer { try? FileManager.default.removeItem(at: root) }
        let store = SnapshotStore(root: root)
        await store.write(origin: "http://nas:7539", profileId: 2, key: SnapshotKeys.home, data: Data("one".utf8))
        await store.write(origin: "http://nas:7539", profileId: 2, key: SnapshotKeys.home, data: Data("two".utf8))
        #expect(await store.read(origin: "http://nas:7539", profileId: 2, key: "home") == Data("two".utf8))
        #expect(await store.read(origin: "http://nas:7539", profileId: 3, key: "home") == nil)
        #expect(await store.read(origin: "http://other:7539", profileId: 2, key: "home") == nil)
        // Keys of any text are file names of their own.
        await store.write(origin: "http://nas:7539", profileId: 2, key: SnapshotKeys.search("a/../b"), data: Data("s".utf8))
        #expect(await store.read(origin: "http://nas:7539", profileId: 2, key: SnapshotKeys.search("a/../b")) == Data("s".utf8))
        let dir = root.appendingPathComponent(SnapshotStore.serverKey("http://nas:7539")).appendingPathComponent("2")
        let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        #expect(names.count == 2 && names.allSatisfy { $0.hasSuffix(".json") && !$0.contains("/") })
        await store.clear(origin: "http://nas:7539")
        #expect(await store.read(origin: "http://nas:7539", profileId: 2, key: "home") == nil)
    }

    @Test func theStoreHolds25MBAtMostTheOldestReadGoingFirst() async throws {
        let root = folder()
        defer { try? FileManager.default.removeItem(at: root) }
        let store = SnapshotStore(root: root, maxBytes: 300)
        let o = "http://nas:7539"
        await store.write(origin: o, profileId: 2, key: "a", data: Data(count: 100))
        await store.write(origin: o, profileId: 2, key: "b", data: Data(count: 100))
        await store.write(origin: o, profileId: 2, key: "c", data: Data(count: 100))
        // Read: the last to go.
        _ = await store.read(origin: o, profileId: 2, key: "a")
        await store.write(origin: o, profileId: 2, key: "d", data: Data(count: 100))
        #expect(await store.read(origin: o, profileId: 2, key: "b") == nil)
        #expect(await store.read(origin: o, profileId: 2, key: "a") != nil)
        #expect(await store.size() <= 300)
        #expect(SnapshotStore.maxBytes == 25 * 1024 * 1024)
    }

    @Test func eachServerKeepsItsLastThreeProfiles() async throws {
        let root = folder()
        defer { try? FileManager.default.removeItem(at: root) }
        let store = SnapshotStore(root: root)
        let o = "http://nas:7539"
        for profile in 1...4 {
            await store.activate(origin: o, profileId: profile)
            await store.write(origin: o, profileId: profile, key: "home", data: Data("\(profile)".utf8))
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(await store.read(origin: o, profileId: 1, key: "home") == nil)
        for profile in 2...4 { #expect(await store.read(origin: o, profileId: profile, key: "home") != nil) }
    }
}

@Suite struct OfflineOpenTests {
    private func session(_ store: any TokenStore) -> SessionManager {
        SessionManager(transport: HTTPTransport(userAgent: "pleyApple/9.9.9 (iOS 18.1; iPhone)"), tokenStore: store,
                       servers: ServerRepository(store: MemoryServerListStore()), client: .ios, sleep: { _ in })
    }

    @Test func theKeptSignInOpensWithoutTheServerAndCatchesUpOnceItAnswers() async throws {
        let server = FixtureServer()
        try await server.start()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let session = session(store)
        #expect(await session.openSaved(origin: server.origin) == false)
        let issued = server.issueTokens()
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: issued.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: issued.refreshToken,
            scope: FixtureServer.allScopes))
        let before = server.journal.count
        #expect(await session.openSaved(origin: server.origin + "/"))
        // Nothing was asked of the server.
        #expect(server.journal.count == before)
        #expect(await session.origin == server.origin)
        let tokens = await session.tokens
        let info = await session.serverInfo
        #expect(tokens != nil && info == nil)
        try await session.reconnected()
        #expect(await session.serverInfo?.name == "Living Room")
        #expect(await session.metadata?.issuer == server.origin)
        let api = try #require(await session.api)
        #expect(try await api.me().name == "Alex")
    }

    @Test func aHomeAddressThatDoesntAnswerGivesUpAfterThreeSeconds() async throws {
        let session = session(MemoryTokenStore())
        let start = ContinuousClock.now
        // A private address nothing answers on: it may hang or fail at once, but never past the check's limit.
        await #expect(throws: (any Error).self) { try await session.open(origin: "http://10.255.255.1:7539") }
        #expect(ContinuousClock.now - start < .seconds(6))
        #expect(SessionManager.homeCheck == .seconds(3))
    }
}
