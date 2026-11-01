import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

private func started() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    server.addPlayable(.init(itemId: 7, fileId: 70))
    return server
}

private func folder() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("pley-late-\(UUID().uuidString)")
}

@Suite("Sessions and their reports")
@MainActor
struct SessionTests {
    @Test func reportsGoOutInTheOrderTheyWereMade() async throws {
        let server = try await started()
        defer { server.stop() }
        let api = api(server)
        let reporter = SessionReporter()
        let session = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                            reporter: reporter, late: nil)
        session.progress(positionMs: 1_000, state: .playing)
        session.progress(positionMs: 2_000, state: .paused)
        session.end(positionMs: 2_500)
        // Once ended, nothing more goes.
        session.progress(positionMs: 3_000, state: .playing)
        session.end(positionMs: 3_000)
        await reporter.flush()
        let order = server.journal.filter { $0.path.hasPrefix("/api/items/7/progress") || $0.path.hasPrefix("/api/playback/") }
            .map { $0.method + " " + ($0.path.hasPrefix("/api/playback/") ? "session" : "progress") }
        #expect(order == ["POST progress", "POST progress", "POST progress", "DELETE session"])
        let states = server.progressReports.compactMap { try? PleyJSON.decoder.decode(ProgressReport.self, from: $0.body) }
        #expect(states.map(\.state) == [.playing, .paused, .stopped])
        #expect(states.map(\.positionMs) == [1_000, 2_000, 2_500])
        #expect(states.allSatisfy { $0.sessionId == session.id && $0.durationMs == 4_000 })
        #expect(server.openSessions.isEmpty)
    }

    @Test func aSessionStoppedOrDiscardedIsntDeletedTwice() async throws {
        let server = try await started()
        defer { server.stop() }
        let api = api(server)
        let reporter = SessionReporter()
        let stopped = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                            reporter: reporter, late: nil)
        stopped.stoppedByServer(positionMs: 900)
        let unused = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                           reporter: reporter, late: nil)
        unused.discard()
        await reporter.flush()
        #expect(server.requests("/api/playback/\(stopped.id)").isEmpty)
        #expect(server.requests("/api/playback/\(unused.id)").map(\.method) == ["DELETE"])
        // The stop keeps the resume point; the unused session moves nothing.
        #expect(server.progressReports.count == 1)
    }

    @Test func answersAboutTheSessionAreHeard() async throws {
        let server = try await started()
        defer { server.stop() }
        let api = api(server)
        let reporter = SessionReporter()
        let session = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                            reporter: reporter, late: nil)
        var heard: [ReportAnswer] = []
        session.onAnswer = { heard.append($0) }
        server.stopSession(session.id, message: "Maintenance")
        session.progress(positionMs: 0, state: .playing)
        await reporter.flush()
        let other = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                          reporter: reporter, late: nil)
        other.onAnswer = { heard.append($0) }
        server.expireSession(other.id)
        other.progress(positionMs: 0, state: .playing)
        await reporter.flush()
        #expect(heard == [.stopped(message: "Maintenance"), .expired])
    }

    @Test func aPauseThatCantReachTheServerIsKeptForLater() async throws {
        let server = try await started()
        let api = api(server)
        let reporter = SessionReporter()
        let clock = FakeClock()
        let late = LateReports(origin: server.origin, profileId: 2, directory: folder(), clock: clock)
        let session = try await ServerPlaybackSession.start(api: api, request: PlaybackRequest(itemId: 7),
                                                            reporter: reporter, late: late)
        server.stop()
        session.progress(positionMs: 1_000, state: .playing)
        session.progress(positionMs: 61_000, state: .paused)
        await reporter.flush()
        let kept = await late.pending()
        #expect(kept.map(\.positionMs) == [61_000])
        #expect(kept.first?.state == .paused)
        #expect(kept.first?.durationMs == 4_000)
    }
}

@Suite("Reports that couldn't go out")
struct LateReportsTests {
    private func report(_ positionMs: Int, _ state: ProgressState = .paused) -> ProgressReport {
        ProgressReport(positionMs: positionMs, durationMs: 100_000, state: state, sessionId: "never-kept")
    }

    @Test func theyGoWithTheirAgeOnceTheServerAnswers() async throws {
        let server = FixtureServer()
        try await server.start()
        defer { server.stop() }
        let clock = FakeClock()
        let directory = folder()
        defer { try? FileManager.default.removeItem(at: directory) }
        let late = LateReports(origin: server.origin, profileId: 2, directory: directory, clock: clock)
        await late.keep(itemId: 7, report(10_000))
        clock.advance(ms: 5_000)
        await late.keep(itemId: 8, report(20_000, .stopped))
        clock.advance(ms: 60_000)
        // The file holds no session or token, and another instance reads it.
        let file = try #require(FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil).first)
        #expect(file.lastPathComponent.hasPrefix("late-reports-"))
        let text = try String(contentsOf: file, encoding: .utf8)
        #expect(!text.contains("never-kept"))
        let again = LateReports(origin: server.origin, profileId: 2, directory: directory, clock: clock)
        #expect(await again.pending().map(\.itemId) == [7, 8])
        await again.send(using: api(server))
        let sent = server.progressReports
        #expect(sent.map(\.itemId) == [7, 8])
        let bodies = sent.compactMap { try? JSONSerialization.jsonObject(with: $0.body) as? [String: Any] }
        #expect(bodies.map { $0["age_ms"] as? Int } == [65_000, 60_000])
        #expect(bodies.allSatisfy { $0["session_id"] == nil })
        #expect(bodies.map { $0["state"] as? String } == ["paused", "stopped"])
        #expect(await again.pending().isEmpty)
        #expect(!FileManager.default.fileExists(atPath: file.path))
    }

    @Test func theyStayWhileTheServerCantBeReached() async throws {
        let server = FixtureServer()
        try await server.start()
        let origin = server.origin
        let api = api(server)
        server.stop()
        let directory = folder()
        defer { try? FileManager.default.removeItem(at: directory) }
        let late = LateReports(origin: origin, profileId: 2, directory: directory, clock: FakeClock())
        await late.keep(itemId: 7, report(10_000))
        await late.send(using: api)
        #expect(await late.pending().count == 1)
    }

    @Test func theNewestPerItemFor100ItemsAnd90Days() async throws {
        let clock = FakeClock()
        let directory = folder()
        defer { try? FileManager.default.removeItem(at: directory) }
        let late = LateReports(origin: "http://127.0.0.1:1", profileId: 2, directory: directory, clock: clock)
        await late.keep(itemId: 1, report(1_000))
        await late.keep(itemId: 1, report(2_000, .stopped))
        #expect(await late.pending().map(\.positionMs) == [2_000])
        for item in 2...120 { await late.keep(itemId: item, report(item)) }
        let kept = await late.pending()
        #expect(kept.count == LateReports.maxItems)
        #expect(kept.first?.itemId == 21)
        #expect(kept.last?.itemId == 120)
        // Another server, or another profile: a file of its own.
        let other = LateReports(origin: "http://127.0.0.1:1", profileId: 3, directory: directory, clock: clock)
        #expect(await other.pending().isEmpty)
        clock.advance(ms: LateReports.maxAgeMs + 1)
        let later = LateReports(origin: "http://127.0.0.1:1", profileId: 2, directory: directory, clock: clock)
        #expect(await later.pending().isEmpty)
    }

    @Test func whatCountsAsAnswered() {
        #expect(LateReports.unreachable(PleyError.offline))
        #expect(!LateReports.unreachable(PleyError.api(status: 500, code: "internal", message: nil)))
        #expect(LateReports.answered(PleyError.api(status: 404, code: "not_found", message: nil)))
        #expect(!LateReports.answered(PleyError.api(status: 503, code: "unavailable", message: nil)))
    }
}
