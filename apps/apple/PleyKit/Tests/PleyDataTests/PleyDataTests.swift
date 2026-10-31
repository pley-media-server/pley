import Foundation
import os
import PleyAuth
import PleyCommon
@testable import PleyData
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private let testAgent = "pleyApple/9.9.9 (iOS 18.1; iPhone)"

private func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

@Suite struct ServerRepositoryTests {
    @Test func serversAreRememberedAndForgotten() async throws {
        let store = MemoryServerListStore()
        let repository = ServerRepository(store: store)
        #expect(await repository.saved().isEmpty)
        #expect(await repository.lastUsed() == nil)
        let a = SavedServer(origin: "http://192.168.1.20:7539", name: "Living Room", id: "a", lastUsed: Date(timeIntervalSince1970: 10))
        let b = SavedServer(origin: "http://192.168.1.30:7539", name: "Office", lastUsed: Date(timeIntervalSince1970: 20))
        try await repository.remember(a)
        try await repository.remember(b)
        #expect(await repository.lastUsed() == b)
        // The same server at a new address replaces the old entry.
        let moved = SavedServer(origin: "http://192.168.1.21:7539", name: "Living Room", id: "a", lastUsed: Date(timeIntervalSince1970: 30))
        try await repository.remember(moved)
        #expect(Set(await repository.saved()) == [b, moved])
        #expect(Set(try await store.load()) == [b, moved])
        try await repository.forget(origin: b.origin)
        #expect(await repository.saved() == [moved])
    }

    @Test func theListKeepsTheNewestFifty() async throws {
        let repository = ServerRepository(store: MemoryServerListStore())
        for i in 0..<55 {
            try await repository.remember(SavedServer(origin: "http://10.0.0.\(i):7539", name: "\(i)",
                                                      lastUsed: Date(timeIntervalSince1970: Double(i))))
        }
        let saved = await repository.saved()
        #expect(saved.count == 50)
        #expect(!saved.contains { $0.name == "4" })
        #expect(saved.contains { $0.name == "54" })
    }
}

@Suite struct SessionManagerTests {
    private func session(_ store: any TokenStore = MemoryTokenStore(),
                         servers: ServerRepository = ServerRepository(store: MemoryServerListStore())) -> SessionManager {
        SessionManager(transport: HTTPTransport(userAgent: testAgent), tokenStore: store, servers: servers, client: .ios,
                       sleep: { _ in })
    }

    @Test func aServerIsOpenedSignedInWithACodeAndShowsHome() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let servers = ServerRepository(store: MemoryServerListStore())
        let store = MemoryTokenStore()
        let session = session(store, servers: servers)

        guard case .signedOut(let metadata) = try await session.open(origin: server.origin + "/", id: "abc") else {
            Issue.record("signed in without signing in")
            return
        }
        #expect(metadata.issuer == server.origin)
        #expect(await servers.saved().isEmpty)

        server.approveDevice()
        let signIn = try await session.deviceSignIn()
        let code = try await signIn.start()
        _ = try await signIn.waitForApproval(code)
        let user = try await session.completeSignIn()
        #expect(user.name == "Alex")
        #expect(await servers.saved().map(\.name) == ["Living Room"])
        #expect(await servers.saved().first?.id == "abc")

        let api = try #require(await session.api)
        let hubs = try await HomeRepository(api: api).load()
        #expect(!hubs.isEmpty)
        #expect(hubs.allSatisfy { !$0.items.isEmpty })
        #expect(server.requests("/api/hubs/home").first?.token != nil)

        // Opened again later: the kept sign-in is used.
        let again = self.session(store, servers: servers)
        #expect(try await again.open(origin: server.origin) == .signedIn(user))
    }

    @Test func aSignInTheServerEndedIsSignedOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let tokens = server.issueTokens()
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken,
            scope: FixtureServer.allScopes))
        let session = session(store)
        _ = try await session.open(origin: server.origin)
        // The server ends it while the app is open: the screens hear.
        server.refuseEveryToken()
        let api = try #require(await session.api)
        await #expect(throws: PleyError.signedOut) { try await api.me() }
        var events = session.events.makeAsyncIterator()
        #expect(await events.next() == .signedOut(origin: server.origin, message: PleyError.signedOut.userMessage))
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await session.tokens == nil)
    }

    @Test func signingOutAndForgetting() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let servers = ServerRepository(store: MemoryServerListStore())
        let store = MemoryTokenStore()
        let session = session(store, servers: servers)
        _ = try await session.open(origin: server.origin)
        server.approveDevice()
        let signIn = try await session.deviceSignIn()
        _ = try await signIn.waitForApproval(try await signIn.start())
        _ = try await session.completeSignIn()
        try await session.forget()
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await servers.saved().isEmpty)
    }

    @Test func somethingThatIsNotPleyIsNotOpened() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: ServerMetadata.path, status: 404, json: #"{"code":"not_found"}"#)
        await #expect(throws: AuthError.tooOld) { try await session().open(origin: server.origin) }
    }
}

@Suite struct ImageLoaderTests {
    @Test func picturesAreMadeSmall() async throws {
        let loader = ImageLoader { _, _ in FixtureImages.make(width: 1200, height: 1800) }
        let image = try await loader.image(path: "/api/items/1/images/poster", widthPx: 300)
        #expect(max(image.width, image.height) == 300)
    }

    @Test func picturesFromTheServerAreFetchedAtTheirWidth() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = HTTPTransport(userAgent: testAgent)
        let store = MemoryTokenStore()
        let given = server.issueTokens()
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: given.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: given.refreshToken,
            scope: FixtureServer.allScopes))
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: EndpointClient(transport: http))
        let loader = ImageLoader(api: PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http)))
        let image = try await loader.image(path: "/api/items/7/images/poster?v=1", widthPx: 220)
        #expect(image.width == 220 * 2 / 3 || image.height == 220)
        #expect(server.requests("/api/items/7/images/poster").first?.query["w"] == "220")
    }

    @Test func onlyTheFourPictureTypesAreShown() async throws {
        let svg = Data(#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.utf8)
        let html = Data("<!doctype html><title>x</title>".utf8)
        for data in [svg, html, Data()] {
            let loader = ImageLoader { _, _ in data }
            await #expect(throws: ImageError.notAPicture) { try await loader.image(path: "/api/x", widthPx: 100) }
        }
        let tiff = FixtureImages.make(width: 10, height: 10, type: "public.tiff")
        await #expect(throws: ImageError.notAPicture) {
            try await ImageLoader { _, _ in tiff }.image(path: "/api/x", widthPx: 100)
        }
    }

    @Test func theServerRefusingATypeIsNotShown() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/items/1/images/poster", status: 200, headers: ["Content-Type": "image/svg+xml"],
                      body: Data("<svg/>".utf8))
        server.answer(path: "/api/items/2/images/poster", status: 200, headers: ["Content-Type": "text/html"],
                      body: Data("<p>".utf8))
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: nil,
                                            transport: HTTPTransport(userAgent: testAgent)))
        let loader = ImageLoader(api: api)
        for id in [1, 2] {
            await #expect(throws: PleyError.self) {
                try await loader.image(path: "/api/items/\(id)/images/poster", widthPx: 100)
            }
        }
    }

    @Test func requestsAtOnceShareOneFetchAndTheCacheKeepsIt() async throws {
        let fetches = OSAllocatedUnfairLock(initialState: 0)
        let loader = ImageLoader { _, _ in
            fetches.withLock { $0 += 1 }
            try await Task.sleep(for: .milliseconds(50))
            return FixtureImages.make(width: 100, height: 150)
        }
        try await withThrowingTaskGroup(of: Void.self) { group in
            for _ in 0..<8 {
                group.addTask { _ = try await loader.image(path: "/api/items/1/images/poster", widthPx: 100) }
            }
            try await group.waitForAll()
        }
        #expect(fetches.withLock { $0 } == 1)
        _ = try await loader.image(path: "/api/items/1/images/poster", widthPx: 100)
        #expect(fetches.withLock { $0 } == 1)
        _ = try await loader.image(path: "/api/items/1/images/poster", widthPx: 200)
        #expect(fetches.withLock { $0 } == 2)
    }
}
