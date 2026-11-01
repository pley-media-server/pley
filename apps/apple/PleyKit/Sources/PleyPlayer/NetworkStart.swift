import Foundation
import os
import PleyModel
import PleyNetwork

/// What the server knows of this client's network, for Automatic's start
/// (a port of the Android app's `NetworkStart`): `GET /api/playback/network`
/// (`lookUp`, at most 1.5 s, made alongside the item's details, so it adds
/// no wait at home), then, when it asks for one (`resolve`), a speed test
/// on a network it doesn't know: 2 s of `GET /api/playback/speedtest` from
/// its first byte, its result `PUT` back. All of it takes 3.5 s at most from
/// the lookup; after that, Automatic starts with what is known.
///
/// Automatic's later ceilings go to the server too (`report`), behind the
/// reports. A server without these routes (`404`) is asked nothing more for
/// the rest of the playback.
public final class NetworkStart: Sendable {
    /// How long each part may take; tests shorten them.
    public struct Timing: Sendable {
        /// The longest the lookup may take.
        public var lookupMs: Int64 = 1_500
        /// The longest the lookup, the speed test and its report take together.
        public var budgetMs: Int64 = 3_500
        /// How long the speed test reads from its first byte.
        public var readMs: Int64 = 2_000
        /// Kept for the report's answer out of the speed test's time.
        public var replyMs: Int64 = 300
        public var minReplyWaitMs: Int64 = 100

        public init() {}
    }

    /// What the speed test asks for: 32 MiB, more than 2 s bring on most connections.
    public static let probeBytes = 32 << 20
    /// Less than this arrived: the test says nothing.
    public static let minBytes = 256 << 10
    /// The share of a speed test the server keeps as the ceiling.
    public static let probeShare = 0.7
    /// The most the server takes.
    public static let maxKbps = 10_000_000

    /// A lookup: when it started, and the server's answer (`nil`: none).
    public struct Lookup: Sendable {
        public let startedAt: Int64
        public let answer: NetworkAnswer?
    }

    private struct State {
        var unsupported = false
        var speedTests = 0
    }

    private let api: PleyAPI
    private let clientId: String
    private let reporter: SessionReporter
    private let clock: any PlayerClock
    private let timing: Timing
    private let state = OSAllocatedUnfairLock(initialState: State())

    public init(api: PleyAPI, clientId: String, reporter: SessionReporter, clock: any PlayerClock,
                timing: Timing = Timing()) {
        self.api = api
        self.clientId = clientId
        self.reporter = reporter
        self.clock = clock
        self.timing = timing
    }

    /// How many speed tests ran.
    public var speedTests: Int { state.withLock { $0.speedTests } }

    /// `GET /api/playback/network`, 1.5 s at most.
    public func lookUp() async -> Lookup {
        let startedAt = clock.now()
        if state.withLock({ $0.unsupported }) { return Lookup(startedAt: startedAt, answer: nil) }
        let (api, clientId) = (api, clientId)
        let answer = await within(timing.lookupMs) { await self.ask { try await api.playbackNetwork(clientId: clientId) } }
        return Lookup(startedAt: startedAt, answer: answer ?? nil)
    }

    /// Where Automatic starts after `lookup`: the server's ceiling, after a
    /// speed test when it asks for one; the device's memory when it couldn't say.
    public func resolve(_ lookup: Lookup) async -> StartCeiling {
        guard let answer = lookup.answer else { return .device }
        if !answer.probe || answer.local { return .server(kbps: answer.ceilingKbps) }
        let left = timing.budgetMs - (clock.now() - lookup.startedAt)
        // Too little left to measure anything: start as if nothing were known.
        if left < timing.readMs / 2 { return .server(kbps: answer.ceilingKbps) }
        state.withLock { $0.speedTests += 1 }
        let (api, readMs) = (api, Int(timing.readMs))
        let sample = await ask {
            try await api.speedTest(bytes: Self.probeBytes, readMs: readMs, timeoutMs: Int(left - self.timing.replyMs))
        }
        guard let sample, sample.bytes >= Self.minBytes else { return .server(kbps: answer.ceilingKbps) }
        let kbps = min(max(sample.kbps, 1), Self.maxKbps)
        // What the server keeps for it, should its answer be late: 70 % of it, none when it all came.
        let expected = sample.complete ? nil : max(Int(Double(kbps) * Self.probeShare), 1)
        let (replies, reply) = AsyncStream.makeStream(of: NetworkAnswer?.self)
        let report = NetworkReport.probe(clientId: clientId, kbps: kbps, complete: sample.complete)
        reporter.send {
            reply.yield(await self.ask { try await api.reportNetwork(report) })
            reply.finish()
        }
        let wait = max(timing.budgetMs - (clock.now() - lookup.startedAt), timing.minReplyWaitMs)
        let answered = await within(wait) { () async -> NetworkAnswer?? in
            for await answer in replies { return answer }
            return nil
        }
        if case .some(.some(.some(let server))) = answered { return .server(kbps: server.ceilingKbps) }
        return .server(kbps: expected)
    }

    /// Automatic's remembered ceiling changed: a step down (`kbps`), a step
    /// up to a quality (its bitrate), or `nil` (back to the original, or
    /// Automatic picked again). Sent behind the reports, best effort.
    public func report(_ kbps: Int?) {
        if state.withLock({ $0.unsupported }) { return }
        let report = NetworkReport.ceiling(clientId: clientId, kbps: kbps.map { min(max($0, 1), Self.maxKbps) })
        let api = api
        reporter.send { _ = await self.ask { try await api.reportNetwork(report) } }
    }

    /// `call`'s answer; `nil` when it failed, and a `404` marks the server as without these routes.
    private func ask<T: Sendable>(_ call: @Sendable () async throws -> T) async -> T? {
        do {
            return try await call()
        } catch PleyError.api(let status, _, _) where status == 404 || status == 405 {
            state.withLock { $0.unsupported = true }
        } catch PleyError.unreadable(let status) where status == 404 || status == 405 {
            state.withLock { $0.unsupported = true }
        } catch {}
        return nil
    }

    /// `work`'s result, or `nil` when it takes longer than `ms` (it is cancelled then).
    private func within<T: Sendable>(_ ms: Int64, _ work: @escaping @Sendable () async -> T) async -> T? {
        let clock = clock
        return await withTaskGroup(of: T?.self) { group in
            group.addTask { await work() }
            group.addTask {
                try? await clock.sleep(ms: ms)
                return nil
            }
            defer { group.cancelAll() }
            return await group.next() ?? nil
        }
    }
}
