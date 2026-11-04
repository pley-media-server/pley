import CoreGraphics
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

    @Test func aServerThatIsNotOpenIsSignedOutAndForgotten() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let given = server.issueTokens()
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: given.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: given.refreshToken,
            scope: FixtureServer.allScopes))
        let servers = ServerRepository(store: MemoryServerListStore([
            SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date()),
        ]))
        try await session(store, servers: servers).forget(origin: server.origin)
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(server.requests("/api/server").isEmpty)
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
        #expect(image.width == 300 && image.height == 450)
    }

    @Test func aTallPictureKeepsTheWidthAsked() throws {
        let image = try ImageLoader.decode(FixtureImages.make(width: 200, height: 300), maxWidth: 100)
        #expect(image.width == 100 && image.height == 150)
        let wide = try ImageLoader.decode(FixtureImages.make(width: 300, height: 200), maxWidth: 100)
        #expect(wide.width == 100 && wide.height <= 67)
    }

    @Test func aPictureIsNeverMadeLarger() throws {
        let image = try ImageLoader.decode(FixtureImages.make(width: 80, height: 120), maxWidth: 100)
        #expect(image.width == 80 && image.height == 120)
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
        #expect(image.width == 220 && image.height == 330)
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

    @Test func theBudgetFollowsTheDevicesMemory() {
        // The iPad 5th generation: 2 GB.
        #expect(ImageLoader.budget(physicalMemory: 2 << 30, appleTV: false) == 32 << 20)
        #expect(ImageLoader.budget(physicalMemory: 3 << 30, appleTV: true) == 32 << 20)
        #expect(ImageLoader.budget(physicalMemory: 4 << 30, appleTV: true) == 64 << 20)
        #expect(ImageLoader.budget(physicalMemory: 8 << 30, appleTV: false) == 96 << 20)
        // Backdrops: smaller on the 2 GB iPad.
        #expect(ImageLoader.backdropLimit(physicalMemory: 2 << 30) == 1536)
        #expect(ImageLoader.backdropLimit(physicalMemory: 8 << 30) == 1920)
    }

    @Test func nearSizesAskForOneWidth() {
        #expect(ImageLoader.pixels(points: 110, scale: 3) == 384)
        #expect(ImageLoader.pixels(points: 112, scale: 3) == 384)
        #expect(ImageLoader.pixels(points: 250, scale: 1) == 256)
        #expect(ImageLoader.pixels(points: 64, scale: 1) == 64)
        #expect(ImageLoader.pixels(points: 5000, scale: 2, limit: 1920) == 1920)
    }

    @Test func theCacheKeepsThePicturesUsedLastWithinItsBudget() async throws {
        let sample = try ImageLoader.decode(FixtureImages.make(width: 100, height: 150), maxWidth: 100)
        let each = sample.bytesPerRow * sample.height
        // Room for 3.
        let fetched = OSAllocatedUnfairLock(initialState: [String]())
        let loader = ImageLoader(cacheBytes: each * 3 + each / 2) { path, _ in
            fetched.withLock { $0.append(path) }
            return FixtureImages.make(width: 100, height: 150)
        }
        for id in 1...3 { _ = try await loader.image(path: "/\(id)", widthPx: 100) }
        _ = try await loader.image(path: "/1", widthPx: 100)
        _ = try await loader.image(path: "/4", widthPx: 100)
        let usage = await loader.cacheUsage
        #expect(usage.bytes == each * 3)
        // 2 went, used longest ago; 1 stayed, used again.
        _ = try await loader.image(path: "/1", widthPx: 100)
        _ = try await loader.image(path: "/2", widthPx: 100)
        #expect(fetched.withLock { $0 } == ["/1", "/2", "/3", "/4", "/2"])
        await loader.trim(to: each + each / 2)
        #expect(await loader.cacheUsage.bytes == each)
        await loader.empty()
        #expect(await loader.cacheUsage.bytes == 0)
    }

    @Test func aFetchIsCancelledWhenItsLastRequestIs() async throws {
        let started = OSAllocatedUnfairLock(initialState: 0)
        let cancelled = OSAllocatedUnfairLock(initialState: 0)
        let loader = ImageLoader { _, _ in
            started.withLock { $0 += 1 }
            do {
                try await Task.sleep(for: .seconds(10))
            } catch {
                cancelled.withLock { $0 += 1 }
                throw error
            }
            return FixtureImages.make(width: 10, height: 10)
        }
        let path = "/api/items/1/images/poster"
        let first = Task { try await loader.image(path: path, widthPx: 100) }
        let second = Task { try await loader.image(path: path, widthPx: 100) }
        #expect(await waitFor { started.withLock { $0 } == 1 })
        first.cancel()
        try await Task.sleep(for: .milliseconds(100))
        // The second still waits for it.
        #expect(cancelled.withLock { $0 } == 0)
        second.cancel()
        #expect(await waitFor { cancelled.withLock { $0 } == 1 })
        await #expect(throws: CancellationError.self) { try await second.value }
        #expect(await waitFor { await loader.running == 0 })
    }

    @Test func atMostSixFetchesRunAndTheNewestWaitingGoesFirst() async throws {
        let order = OSAllocatedUnfairLock(initialState: [String]())
        let gate = OSAllocatedUnfairLock(initialState: Set<String>())
        let loader = ImageLoader { path, _ in
            order.withLock { $0.append(path) }
            while !gate.withLock({ $0.contains(path) }) {
                try await Task.sleep(for: .milliseconds(5))
            }
            return FixtureImages.make(width: 10, height: 10)
        }
        var tasks: [Task<CGImage, any Error>] = []
        for id in 1...6 {
            tasks.append(Task { try await loader.image(path: "/\(id)", widthPx: 64) })
        }
        #expect(await waitFor { order.withLock { $0.count } == 6 })
        for id in 7...9 {
            tasks.append(Task { try await loader.image(path: "/\(id)", widthPx: 64) })
            try await Task.sleep(for: .milliseconds(30))
        }
        #expect(order.withLock { $0.count } == 6)
        #expect(await loader.running == ImageLoader.maxFetches)
        gate.withLock { _ = $0.insert("/1") }
        #expect(await waitFor { order.withLock { $0.count } == 7 })
        #expect(order.withLock { $0.last } == "/9")
        // One still waiting, cancelled, never starts.
        tasks[6].cancel()
        gate.withLock { _ = $0.insert("/2") }
        #expect(await waitFor { order.withLock { $0.count } == 8 })
        #expect(order.withLock { $0.last } == "/8")
        gate.withLock { $0.formUnion((1...9).map { "/\($0)" }) }
        for (n, task) in tasks.enumerated() where n != 6 { _ = try await task.value }
        #expect(!order.withLock { $0 }.contains("/7"))
        #expect(await waitFor { await loader.running == 0 })
    }
}

/// Waits until `condition` holds, for at most 5 seconds.
private func waitFor(_ condition: () async -> Bool) async -> Bool {
    for _ in 0..<500 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return await condition()
}

@MainActor
@Suite struct ServerEventsTests {
    @Test func changesOfAThirdOfASecondGoOutTogether() async throws {
        let events = ServerEvents()
        var changes: [ServerChange] = []
        events.onChange = { changes.append($0) }
        // A play reaching 90 % sends both.
        events.heard(.userStateChanged(itemIds: [202, 203, 201], children: false))
        events.heard(.libraryChanged(libraryId: 2))
        try await Task.sleep(for: .milliseconds(100))
        #expect(changes.isEmpty)
        try await Task.sleep(for: .milliseconds(400))
        #expect(changes.count == 1)
        let change = try #require(changes.first)
        #expect(change.home)
        #expect(change.itemIds == [202, 203, 201])
        #expect(change.touches(library: 2) && !change.touches(library: 1))
        #expect(change.names(203) && !change.names(5))
        // A show marked as a whole: everything under it too.
        events.heard(.userStateChanged(itemIds: [201], children: true))
        events.heard(.watchlistChanged)
        try await Task.sleep(for: .milliseconds(450))
        #expect(changes.count == 2)
        #expect(changes[1].names(999, parent: 201) && changes[1].watchlist)
        #expect(changes[1].serial > changes[0].serial)
    }

    @Test func scansAreToldAsTheyMoveAndTheirEndChangesTheLibrary() async throws {
        let events = ServerEvents(window: 0.01)
        var scans: [[Int: ScanStatus]] = []
        var changes: [ServerChange] = []
        events.onScans = { scans.append($0) }
        events.onChange = { changes.append($0) }
        events.heard(.scanStarted(libraryId: 1))
        events.heard(.scanProgress(libraryId: 1, phase: "probing", processed: 118, total: 412))
        #expect(scans.last?[1]?.processed == 118)
        #expect(changes.isEmpty)
        events.heard(.scanFinished(libraryId: 1))
        #expect(scans.last?.isEmpty == true)
        try await Task.sleep(for: .milliseconds(100))
        #expect(changes.count == 1 && changes[0].libraries && changes[0].touches(library: 1))
    }

    @Test func aStoppedPlaybackIsToldAtOnceAndAReconnectReadsEverything() async throws {
        let events = ServerEvents(window: 0.01)
        var stopped: [String] = []
        var changes: [ServerChange] = []
        events.onSessionStopped = { id, _, _ in stopped.append(id) }
        events.onChange = { changes.append($0) }
        events.heard(.event(.sessionStopped(sessionId: "s1", message: "bedtime", byAdmin: true)))
        #expect(stopped == ["s1"])
        events.heard(.connected)
        try await Task.sleep(for: .milliseconds(80))
        #expect(changes.isEmpty)
        events.heard(.connected)
        events.heard(.event(.sessionsChanged))
        try await Task.sleep(for: .milliseconds(80))
        #expect(changes.count == 1 && changes[0].everything && changes[0].names(12345))
    }

    @Test func aRemotesCommandAndTheSessionsChangingAreToldAtOnce() async throws {
        let events = ServerEvents(window: 0.01)
        var commands: [String] = []
        var sessionsChanged = 0
        var changes: [ServerChange] = []
        events.onPlayerCommand = { id, command in commands.append("\(id) \(command.command)") }
        events.onSessionsChanged = { sessionsChanged += 1 }
        events.onChange = { changes.append($0) }
        events.heard(.event(.playerCommand(sessionId: "s1", command: .skip(byMs: 30_000))))
        events.heard(.event(.sessionsChanged))
        #expect(commands == ["s1 skip"])
        #expect(sessionsChanged == 1)
        // The first connection misses nothing; one made again may have.
        events.heard(.connected)
        #expect(sessionsChanged == 1)
        events.heard(.connected)
        #expect(sessionsChanged == 2)
        try await Task.sleep(for: .milliseconds(80))
        // Neither is a change for the screens' rows but the reconnection.
        #expect(changes.count == 1 && changes[0].everything)
    }
}

/// The Mac's Larger and Smaller Tiles keys: the menu takes ⌘= and ⌘−, and
/// the app's key monitor ⌘+ (⇧⌘=), which no menu item can show beside ⌘=.
@Suite struct TileKeysTests {
    @Test func commandEqualsAndPlusAreLargerAndMinusSmaller() {
        #expect(GridDensity.keyStep("=", command: true) == 1)
        #expect(GridDensity.keyStep("+", command: true) == 1)
        #expect(GridDensity.keyStep("-", command: true) == -1)
        #expect(GridDensity.keyStep("\u{2212}", command: true) == -1)
    }

    @Test func otherKeysAndModifiersAreLeftAlone() {
        #expect(GridDensity.keyStep("=", command: false) == nil)
        #expect(GridDensity.keyStep("+", command: true, option: true) == nil)
        #expect(GridDensity.keyStep("=", command: true, control: true) == nil)
        #expect(GridDensity.keyStep("0", command: true) == nil)
        #expect(GridDensity.keyStep(nil, command: true) == nil)
    }

    @Test func aStepStopsAtTheEnds() {
        #expect(GridDensity.medium.stepped(by: 1) == .large)
        #expect(GridDensity.large.stepped(by: 1) == nil)
        #expect(GridDensity.small.stepped(by: -1) == nil)
    }
}
