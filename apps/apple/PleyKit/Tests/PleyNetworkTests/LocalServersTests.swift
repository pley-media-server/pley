import Foundation
import Network
import os
import PleyCommon
import PleyNetwork
import PleyTesting
import Testing

/// A browser whose events the test sends.
final class FakeBrowser: ServiceBrowsing {
    private let stream: AsyncStream<BonjourEvent>
    private let continuation: AsyncStream<BonjourEvent>.Continuation

    init() {
        (stream, continuation) = AsyncStream.makeStream(of: BonjourEvent.self)
    }

    func results() -> AsyncStream<BonjourEvent> { stream }

    func found(_ instance: String, id: String, name: String = "From TXT", version: String? = "0.1.0") {
        let announcement = BonjourAnnouncement(instance: instance, id: id, name: name, version: version)
        continuation.yield(.found(announcement, endpoint(instance)))
    }

    func send(_ event: BonjourEvent) { continuation.yield(event) }

    func endpoint(_ instance: String) -> NWEndpoint {
        .service(name: instance, type: "_pley._tcp", domain: "local.", interface: nil)
    }
}

/// Every state a browse sent, read by the test as they come.
final class Watcher: Sendable {
    private let states = OSAllocatedUnfairLock<[DiscoveryState]>(initialState: [])
    private let task: Task<Void, Never>

    init(_ stream: AsyncStream<DiscoveryState>) {
        let states = states
        task = Task {
            for await state in stream { states.withLock { $0.append(state) } }
        }
    }

    deinit { task.cancel() }

    var all: [DiscoveryState] { states.withLock { $0 } }

    /// The latest state.
    var latest: DiscoveryState? { all.last }

    /// The latest state once it satisfies `predicate`, within `timeout`.
    func wait(within timeout: Duration = .seconds(5),
              until predicate: (DiscoveryState) -> Bool) async -> DiscoveryState? {
        await poll(within: timeout) { latest.map(predicate) ?? false } ? latest : nil
    }

    /// The latest state once `done` is true and nothing is being checked.
    func settled(within timeout: Duration = .seconds(5), after done: () async -> Bool) async -> DiscoveryState? {
        await poll(within: timeout) { await done() && latest?.searching == false } ? latest : nil
    }

    private func poll(within timeout: Duration, _ condition: () async -> Bool) async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now + timeout
        while clock.now < deadline {
            if await condition() { return true }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return false
    }

    func stop() { task.cancel() }
}

/// Counts resolves running at once.
actor Gauge {
    private(set) var now = 0
    private(set) var most = 0
    private(set) var total = 0

    func enter() {
        now += 1
        total += 1
        most = max(most, now)
    }

    func leave() { now -= 1 }
}

/// Resolves every instance to `127.0.0.1` and the port given for it.
func resolver(_ ports: [String: Int]) -> LocalServers.Resolve {
    { endpoint in
        guard case .service(let name, _, _, _) = endpoint, let port = ports[name] else { return nil }
        return ResolvedService(host: "127.0.0.1", port: port)
    }
}

func port(of server: FixtureServer) -> Int {
    URLComponents(string: server.origin)?.port ?? 0
}

@Suite struct LocalServersTests {
    @Test func anAnnouncedServerIsConfirmedAndNamedByItsAnswer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["Living Room": port(of: server)]), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        // A browse starts out searching, with nothing found.
        _ = await watcher.wait { _ in true }
        #expect(watcher.all.first == DiscoveryState(servers: [], blocked: false, searching: true))
        browser.found("Living Room", id: "abc-123", name: "Not What The Server Says")
        let state = await watcher.wait { !$0.servers.isEmpty }
        #expect(state?.servers == [FoundServer(origin: server.origin, name: "Living Room", id: "abc-123",
                                               version: "0.42.0")])
        let asked = try #require(server.requests("/api/server").first)
        #expect(asked.headers["authorization"] == nil)
    }

    @Test func somethingThatIsNotPleyIsLeftOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/server", status: 200, json: #"{"hello":"world"}"#)
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["Impostor": port(of: server)]), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Impostor", id: "abc")
        let state = await watcher.settled { server.requests("/api/server").count == 1 }
        #expect(state?.servers == [])
    }

    @Test func anAddressOffTheHomeNetworkIsNeverAsked() async throws {
        let browser = FakeBrowser()
        let gauge = Gauge()
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            return ResolvedService(host: "8.8.8.8", port: 7539)
        }, settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Elsewhere", id: "abc")
        let state = await watcher.settled { await gauge.total == 1 }
        #expect(state?.servers == [])
    }

    @Test func theTXTNameStandsInForAnEmptyOne() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/server", status: 200, json: #"{"name":"","version":"1.2.3"}"#)
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["Attic": port(of: server)]), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Attic", id: "abc", name: "Attic server")
        let state = await watcher.wait { !$0.servers.isEmpty }
        #expect(state?.servers.first?.name == "Attic server")
        #expect(state?.servers.first?.version == "1.2.3")
    }

    @Test func serversAreSortedAndALostOneGoes() async throws {
        let living = try await fixture()
        let attic = try await fixture()
        defer { living.stop(); attic.stop() }
        attic.answer(path: "/api/server", status: 200, json: #"{"name":"attic","version":"0.42.0"}"#)
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["one": port(of: living), "two": port(of: attic)]),
                                 settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("one", id: "living")
        browser.found("two", id: "attic")
        let both = await watcher.wait { $0.servers.count == 2 }
        #expect(both?.servers.map(\.name) == ["attic", "Living Room"])
        browser.send(.lost(instance: "two"))
        #expect(await watcher.wait { $0.servers.map(\.id) == ["living"] } != nil)
    }

    @Test func oneServerUnderTwoInstanceNamesShowsOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["Living Room": port(of: server),
                                                    "Living Room (2)": port(of: server)]), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Living Room", id: "same")
        browser.found("Living Room (2)", id: "same")
        let state = await watcher.settled { server.requests("/api/server").count == 2 }
        #expect(state?.servers.count == 1)
    }

    @Test func aSlowAnnouncementIsLeftOut() async throws {
        let browser = FakeBrowser()
        let gauge = Gauge()
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            try? await Task.sleep(for: .seconds(30))
            return ResolvedService(host: "127.0.0.1", port: 1)
        }, confirmTimeout: .milliseconds(200), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        let clock = ContinuousClock()
        let start = clock.now
        browser.found("Slow", id: "abc")
        let state = await watcher.settled { await gauge.total == 1 }
        #expect(state?.servers == [])
        #expect(clock.now - start < .seconds(5))
    }

    @Test func anAnnouncementNotConfirmedIsCheckedAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let serverPort = port(of: server)
        let gauge = Gauge()
        let browser = FakeBrowser()
        // The first resolve finds nothing, as a slow mDNS answer would.
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            await gauge.leave()
            return await gauge.total == 1 ? nil : ResolvedService(host: "127.0.0.1", port: serverPort)
        }, settle: .zero, retryDelays: [.milliseconds(100)])
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Living Room", id: "abc")
        let state = await watcher.wait { !$0.servers.isEmpty }
        #expect(state?.servers.first?.origin == server.origin)
        #expect(await gauge.total == 2)
    }

    @Test func aLostAnnouncementIsNotCheckedAgain() async throws {
        let gauge = Gauge()
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            await gauge.leave()
            return nil
        }, settle: .zero, retryDelays: [.milliseconds(200), .milliseconds(200)])
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Gone", id: "abc")
        _ = await watcher.settled { await gauge.total == 1 }
        browser.send(.lost(instance: "Gone"))
        try await Task.sleep(for: .milliseconds(600))
        #expect(await gauge.total == 1)
    }

    @Test func checksStopAfterTheLastRetry() async throws {
        let gauge = Gauge()
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            await gauge.leave()
            return nil
        }, settle: .zero, retryDelays: [.milliseconds(20), .milliseconds(20)])
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.found("Nobody", id: "abc")
        try await Task.sleep(for: .milliseconds(500))
        #expect(await gauge.total == 3)
        #expect(watcher.latest?.searching == false)
    }

    @Test func atMostFourAreCheckedAtOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let gauge = Gauge()
        let serverPort = port(of: server)
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser, resolve: { _ in
            await gauge.enter()
            try? await Task.sleep(for: .milliseconds(50))
            await gauge.leave()
            return ResolvedService(host: "127.0.0.1", port: serverPort)
        }, settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        for n in 0..<10 { browser.found("server \(n)", id: "id\(n)") }
        let state = await watcher.settled { await gauge.total == 10 }
        #expect(state?.servers.count == 1)
        #expect(await gauge.most == LocalServers.maxConcurrent)
        #expect(server.requests("/api/server").count == 10)
    }

    @Test func aRefusedLocalNetworkIsSaid() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let browser = FakeBrowser()
        let local = LocalServers(transport: transport(), browser: browser,
                                 resolve: resolver(["Living Room": port(of: server)]), settle: .zero)
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        browser.send(.blocked)
        #expect(await watcher.wait { $0.blocked } != nil)
        browser.found("Living Room", id: "abc")
        let state = await watcher.wait { !$0.servers.isEmpty }
        #expect(state?.blocked == false)
    }

    /// Bonjour for real: a fixture server announced on this Mac's loopback
    /// interface, found by `NWBrowser` and confirmed. It needs mDNSResponder
    /// and, on recent systems, local network access for the test runner, so
    /// it runs only with `PLEY_BONJOUR_TEST=1`.
    @Test(.enabled(if: ProcessInfo.processInfo.environment["PLEY_BONJOUR_TEST"] == "1"),
          .timeLimit(.minutes(1)))
    func aServerAnnouncedByBonjourIsFound() async throws {
        let id = "pley-test-" + UUID().uuidString.prefix(8).lowercased()
        let server = FixtureServer()
        try await server.start(bonjour: (instance: "Pley test \(id)",
                                         txt: ["pley": "1", "id": id, "name": "Test", "version": "0.42.0"]))
        defer { server.stop() }
        let local = LocalServers(transport: transport())
        let watcher = Watcher(local.browse())
        defer { watcher.stop() }
        let state = await watcher.wait(within: .seconds(20)) { state in
            state.blocked || state.servers.contains { $0.id == id }
        }
        for found in watcher.latest?.servers ?? [] {
            print("found \(found.name) \(found.version) at \(found.origin)")
        }
        #expect(state?.blocked == false, "the system refused local network access")
        let found = try #require(state?.servers.first { $0.id == id })
        #expect(found.name == "Living Room")
        #expect(found.version == "0.42.0")
        #expect(PrivateAddress.isPrivate(literal: URL(string: found.origin)?.host() ?? "") == true)
    }
}
