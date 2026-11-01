import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// `NetworkStart` against the fixture server, on the real clock with short times
/// (the Android app's NetworkStartTest).
private struct Net {
    let server: FixtureServer
    let api: PleyAPI
    let reporter = SessionReporter()
    let start: NetworkStart

    init(server: FixtureServer, timing: NetworkStart.Timing = Net.timing()) {
        self.server = server
        api = PleyPlayerTests.api(server)
        start = NetworkStart(api: api, clientId: "apple-test", reporter: reporter, clock: SystemPlayerClock(),
                             timing: timing)
    }

    static func timing(lookupMs: Int64 = 1_000, budgetMs: Int64 = 3_000, readMs: Int64 = 300) -> NetworkStart.Timing {
        var timing = NetworkStart.Timing()
        timing.lookupMs = lookupMs
        timing.budgetMs = budgetMs
        timing.readMs = readMs
        timing.replyMs = 50
        timing.minReplyWaitMs = 20
        return timing
    }

    func ceiling() async -> StartCeiling { await start.resolve(start.lookUp()) }

    /// The bodies of `PUT /api/playback/network`, after everything queued went out.
    func reports() async -> [[String: Any]] {
        await reporter.flush()
        return server.networkReports.compactMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
    }
}

private func net(local: Bool = false, probe: Bool = false, routes: Bool = true,
                 timing: NetworkStart.Timing = Net.timing()) async throws -> Net {
    let server = FixtureServer()
    try await server.start()
    server.setNetwork(routes: routes, local: local, probe: probe)
    return Net(server: server, timing: timing)
}

@Suite("The network Automatic starts on")
struct NetworkStartTests {
    @Test func theServersOwnNetworkStartsAtTheOriginalWithoutASpeedTest() async throws {
        let n = try await net(local: true, probe: true)
        defer { n.server.stop() }
        #expect(await n.ceiling() == .server(kbps: nil))
        #expect(n.start.speedTests == 0)
        #expect(n.server.speedTests == 0)
    }

    @Test func theServersCeilingForTheNetworkIsWhereItStarts() async throws {
        let n = try await net()
        defer { n.server.stop() }
        _ = try await n.api.reportNetwork(.ceiling(clientId: "apple-test", kbps: 5_000))
        n.server.setNetwork(local: false, probe: true)
        #expect(await n.ceiling() == .server(kbps: 5_000))
        #expect(n.server.speedTests == 0)
    }

    @Test func anOlderServerLeavesItToTheDeviceAndIsAskedNothingMore() async throws {
        let n = try await net(routes: false)
        defer { n.server.stop() }
        #expect(await n.ceiling() == .device)
        n.start.report(4_000)
        #expect(await n.reports().isEmpty)
        #expect(n.server.requests("/api/playback/network").count == 1)
    }

    @Test func aServerOutOfReachLeavesItToTheDevice() async throws {
        let n = try await net()
        n.server.stop()
        #expect(await n.ceiling() == .device)
    }

    @Test func aSlowLookupLeavesItToTheDevice() async throws {
        let n = try await net(timing: Net.timing(lookupMs: 100))
        defer { n.server.stop() }
        n.server.delay(path: "/api/playback/network", ms: 1_000)
        #expect(await n.ceiling() == .device)
    }

    @Test func anUnknownNetworkIsMeasuredAndTheServersAnswerIsWhereItStarts() async throws {
        let n = try await net(probe: true)
        defer { n.server.stop() }
        let ceiling = await n.ceiling()
        #expect(n.start.speedTests == 1)
        #expect(n.server.speedTests == 1)
        let report = try #require(await n.reports().first)
        let kbps = try #require(report["probe_kbps"] as? Int)
        #expect(kbps > 0)
        #expect(report["client_id"] as? String == "apple-test")
        // Whatever the loopback carried, the start is what the server made of it.
        let answer = try await n.api.playbackNetwork(clientId: "apple-test")
        #expect(ceiling == .server(kbps: answer.ceilingKbps))
    }

    @Test func aSpeedTestThatAllCameIsNoLimit() async throws {
        // Time enough for all of it on the loopback.
        let n = try await net(probe: true, timing: Net.timing(budgetMs: 20_000, readMs: 10_000))
        defer { n.server.stop() }
        n.server.answer(path: "/api/playback/speedtest", status: 200, body: Data(count: NetworkStart.probeBytes))
        let ceiling = await n.ceiling()
        let report = try #require(await n.reports().first)
        #expect(report["probe_complete"] as? Bool == true)
        #expect(ceiling == .server(kbps: nil))
    }

    @Test func underTheLeastASpeedTestSaysNothing() async throws {
        let n = try await net(probe: true)
        defer { n.server.stop() }
        n.server.answer(path: "/api/playback/speedtest", status: 200, body: Data(count: 100 << 10))
        #expect(await n.ceiling() == .server(kbps: nil))
        #expect(n.start.speedTests == 1)
        #expect(await n.reports().isEmpty)
    }

    @Test func aLateAnswerToTheSpeedTestStartsAtWhatTheServerKeeps() async throws {
        let n = try await net(probe: true, timing: Net.timing(budgetMs: 1_200, readMs: 300))
        defer { n.server.stop() }
        // Less than asked for: the server keeps 70 % of it.
        n.server.answer(path: "/api/playback/speedtest", status: 200, body: Data(count: 1 << 20))
        n.server.delay(path: "/api/playback/network", ms: 3_000, method: "PUT")
        let ceiling = await n.ceiling()
        let report = try #require(await n.reports().first)
        let kbps = try #require(report["probe_kbps"] as? Int)
        #expect(report["probe_complete"] as? Bool == false)
        #expect(ceiling == .server(kbps: max(Int(Double(kbps) * NetworkStart.probeShare), 1)))
    }

    @Test func tooLittleTimeLeftAfterTheLookupMeasuresNothing() async throws {
        let n = try await net(probe: true, timing: Net.timing(lookupMs: 2_000, budgetMs: 600, readMs: 1_000))
        defer { n.server.stop() }
        n.server.delay(path: "/api/playback/network", ms: 200, method: "GET")
        #expect(await n.ceiling() == .server(kbps: nil))
        #expect(n.start.speedTests == 0)
    }

    @Test func automaticsCeilingsAreToldToTheServer() async throws {
        let n = try await net()
        defer { n.server.stop() }
        n.start.report(4_000)
        n.start.report(nil)
        n.start.report(50_000_000)
        let reports = await n.reports()
        #expect(reports.count == 3)
        #expect(reports.map { $0["ceiling_kbps"] as? Int } == [4_000, nil, NetworkStart.maxKbps])
        #expect(reports.allSatisfy { $0.keys.contains("ceiling_kbps") && $0["probe_kbps"] == nil })
    }
}
