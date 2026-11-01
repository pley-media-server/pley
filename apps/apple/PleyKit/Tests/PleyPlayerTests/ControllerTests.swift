import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

@MainActor
private func rig(method: String = "direct", positionMs: Int = 0) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000, positionMs: positionMs))
    server.setPlaybackMethod(method)
    return try Rig(server: server, request: PlayRequest(itemId: 7))
}

/// Starts the rig's playback and waits for its first frame.
@MainActor
private func playing(_ rig: Rig) async throws -> ServerPlaybackSession {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
    return try #require(rig.controller.session)
}

@Suite("The playback controller")
@MainActor
struct ControllerTests {
    @Test func itPlaysTheKeyedFileAndReportsOnCadence() async throws {
        let rig = try await rig(positionMs: 30_000)
        defer { rig.cleanUp() }
        let session = try await playing(rig)
        let load = try #require(rig.engine.loads.first)
        #expect(load.url.absoluteString.hasPrefix(rig.server.origin + "/api/media/pley_mk_"))
        #expect(load.url.query == nil)
        #expect(load.startMs == 30_000)
        #expect(load.play)
        let request = try #require(await rig.requests().first)
        #expect(request.keyedUrls == true)
        #expect(request.mode == .auto)
        #expect(request.clientId == "apple-test")
        #expect(request.startMs == 30_000)
        #expect(request.fileId == 70)
        #expect(rig.controller.state.title == "Test Movie (2020)")
        #expect(rig.controller.state.method == .direct)

        rig.engine.playing(at: 35_000)
        rig.clock.advance(ms: 5_000)
        rig.controller.tick()
        rig.engine.playing(at: 40_000)
        rig.clock.advance(ms: 5_000)
        rig.controller.tick()
        rig.controller.pause()
        #expect(rig.controller.state.phase == .paused)
        rig.engine.current.playing = false
        rig.clock.advance(ms: 29_000)
        rig.controller.tick()
        rig.clock.advance(ms: 1_000)
        rig.controller.tick()
        rig.controller.seek(toMs: 100_000)
        #expect(rig.engine.calls.contains("seek 100000"))
        rig.clock.advance(ms: 1_000)
        rig.controller.tick()
        await rig.controller.close()

        let reports = await rig.reports()
        #expect(reports.map(\.state) == [.playing, .playing, .paused, .paused, .paused, .stopped])
        #expect(reports.map(\.positionMs) == [30_000, 40_000, 40_000, 40_000, 100_000, 100_000])
        #expect(reports.allSatisfy { $0.sessionId == session.id })
        #expect(rig.server.requests("/api/playback/\(session.id)").map(\.method) == ["DELETE"])
        #expect(rig.server.openSessions.isEmpty)
        #expect(rig.engine.closed)
    }

    @Test func aSessionStoppedByTheServerStopsWithoutTryAgain() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let session = try await playing(rig)
        rig.server.stopSession(session.id, message: "Maintenance tonight")
        rig.engine.playing(at: 9_000)
        rig.clock.advance(ms: 10_000)
        rig.controller.tick()
        _ = await rig.reports()
        try await until("the stop") { rig.controller.state.stoppedByServer }
        #expect(rig.controller.state.phase == .failed(
            message: "Playback was stopped by the server's administrator. Maintenance tonight", canRetry: false))
        #expect(rig.engine.calls.last == "pause")
        // Play and seek do nothing now; closing sends nothing more.
        rig.controller.play()
        rig.controller.seek(toMs: 0)
        await rig.controller.close()
        let reports = await rig.reports()
        #expect(reports.map(\.state) == [.playing, .playing, .stopped])
        #expect(reports.last?.positionMs == 9_000)
        #expect(rig.server.requests("/api/playback/\(session.id)").isEmpty)
        #expect(await rig.requests().count == 1)
    }

    @Test func anExpiredSessionStartsAgainAtTheNextSeek() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let first = try await playing(rig)
        rig.server.expireSession(first.id)
        rig.engine.playing(at: 10_000)
        rig.clock.advance(ms: 10_000)
        rig.controller.tick()
        _ = await rig.reports()
        rig.controller.seek(toMs: 42_000)
        try await until("a new session") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads[1].startMs == 42_000)
        let requests = await rig.requests()
        #expect(requests.map(\.startMs) == [0, 42_000])
        let second = try #require(rig.controller.session)
        #expect(second.id != first.id)
        rig.engine.emit(.ready)
        try await until("the second session's first frame") {
            await rig.reports().last.map { $0.sessionId == second.id } ?? false
        }
        await rig.controller.close()
        #expect(rig.server.openSessions.isEmpty)
    }

    @Test func directPlayThatFailsFallsBackToARemuxThenATranscode() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        rig.controller.start()
        try await until("the file") { rig.engine.loads.count == 1 }
        let audio = EngineError.unsupported(detail: "AVFoundationErrorDomain -11829", audio: true, container: false)
        rig.engine.emit(.failed(audio))
        try await until("a remux") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads[1].url.lastPathComponent == "index.m3u8")
        rig.engine.emit(.failed(audio))
        try await until("a transcode") { rig.engine.loads.count == 3 }
        rig.engine.emit(.failed(audio))
        try await until("the failure") { rig.controller.state.phase != .preparing }
        #expect(rig.controller.state.phase == .failed(message: "This video can't be played on this device.",
                                                      canRetry: false))
        #expect(await rig.requests().map(\.mode) == [.auto, .remux, .transcode])
        await rig.controller.close()
        #expect(rig.server.openSessions.isEmpty)
    }

    @Test func aRemuxWithoutPictureForFiveSecondsBecomesATranscode() async throws {
        let rig = try await rig(method: "remux")
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        rig.engine.playing(at: 1_000, frames: 0)
        rig.controller.tick()
        rig.engine.playing(at: 5_999, frames: 0)
        rig.clock.advance(ms: 4_999)
        rig.controller.tick()
        #expect(rig.engine.loads.count == 1)
        rig.engine.playing(at: 6_000, frames: 0)
        rig.clock.advance(ms: 1)
        rig.controller.tick()
        try await until("a transcode") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads[1].startMs == 6_000)
        #expect(await rig.requests().map(\.mode) == [.auto, .transcode])
        await rig.controller.close()
    }

    @Test func aRemuxWithPictureIsLeftAlone() async throws {
        let rig = try await rig(method: "remux")
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        for second in 1...8 {
            rig.engine.playing(at: second * 1_000, frames: second * 30)
            rig.clock.advance(ms: 1_000)
            rig.controller.tick()
        }
        #expect(rig.engine.loads.count == 1)
        await rig.controller.close()
    }

    @Test func twelveSecondsStalledBecomesATranscode() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        rig.engine.playing(at: 20_000)
        rig.controller.tick()
        // Stuck with data loaded ahead: the decoding, not the network.
        rig.engine.current.playing = false
        rig.engine.current.waiting = true
        rig.engine.current.bufferedMs = 60_000
        rig.clock.advance(ms: 11_999)
        rig.controller.tick()
        #expect(rig.engine.loads.count == 1)
        #expect(rig.controller.state.phase == .buffering)
        rig.clock.advance(ms: 1)
        rig.controller.tick()
        try await until("a transcode") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads[1].startMs == 20_000)
        #expect(await rig.requests().map(\.mode) == [.auto, .transcode])
        await rig.controller.close()
    }

    @Test func backFromALongBackgroundAForgottenSessionStartsAgainPaused() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let first = try await playing(rig)
        rig.engine.playing(at: 3_000)
        rig.controller.tick()
        await rig.controller.appWillBackground()
        #expect(rig.engine.calls.last == "pause")
        #expect(await rig.reports().last?.state == .paused)
        rig.server.expireSession(first.id)
        rig.clock.advance(ms: 61_000)
        await rig.controller.appDidBecomeActive()
        try await until("a new session") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads[1].startMs == 3_000)
        #expect(!rig.engine.loads[1].play)
        await rig.controller.close()
    }

    @Test func backSoonTheSessionIsntChecked() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let first = try await playing(rig)
        await rig.controller.appWillBackground()
        rig.clock.advance(ms: 30_000)
        await rig.controller.appDidBecomeActive()
        #expect(rig.server.requests("/api/playback/\(first.id)").isEmpty)
        #expect(rig.engine.loads.count == 1)
        await rig.controller.close()
    }

    @Test func anItemThatDoesntExistFailsWithoutTryAgain() async throws {
        let server = FixtureServer()
        try await server.start()
        let rig = try Rig(server: server, request: PlayRequest(itemId: 404))
        defer { rig.cleanUp() }
        rig.controller.start()
        try await until("the failure") { rig.controller.state.phase != .preparing }
        guard case .failed(_, let canRetry) = rig.controller.state.phase else {
            Issue.record("expected a failure, got \(rig.controller.state.phase)")
            return
        }
        #expect(!canRetry)
        #expect(rig.engine.loads.isEmpty)
    }

    @Test func thePlayAndPauseOfTheSystemsControlsAreFollowed() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        rig.engine.playing(at: 5_000)
        rig.engine.emit(.pauseRequested)
        try await until("the pause") { rig.controller.state.phase == .paused }
        #expect(await rig.reports().last?.state == .paused)
        // Standing still while paused there is no stall: no new session, however long.
        rig.engine.current.playing = false
        for _ in 0..<40 {
            rig.clock.advance(ms: 500)
            rig.controller.tick()
        }
        #expect(await rig.requests().count == 1)
        #expect(rig.controller.state.phase == .paused)

        rig.engine.emit(.playRequested)
        try await until("playing again") { rig.engine.calls.contains("play") }
        rig.engine.playing(at: 6_000)
        rig.controller.tick()
        #expect(rig.controller.state.phase == .playing)
        await rig.controller.close()
    }

    @Test func aJumpOfTheScrubberIsReportedASecondLater() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        let before = await rig.reports().count
        rig.engine.playing(at: 300_000)
        rig.engine.emit(.jumped)
        try await Task.sleep(for: .milliseconds(50))
        rig.clock.advance(ms: 1_000)
        rig.controller.tick()
        let reports = await rig.reports()
        #expect(reports.count == before + 1)
        #expect(reports.last?.positionMs == 300_000)
        #expect(reports.last?.state == .playing)
        await rig.controller.close()
    }

    @Test func theScrubberMovedAfterTheEndStartsANewSessionThere() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let first = try await playing(rig)
        rig.engine.emit(.ended)
        try await until("the end") { rig.controller.state.phase == .ended }
        await rig.reporter.flush()
        #expect(!rig.server.openSessions.contains(first.id))
        rig.engine.current.positionMs = 120_000
        rig.engine.emit(.jumped)
        try await until("a new session") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads.last?.startMs == 120_000)
        #expect(await rig.requests().last?.startMs == 120_000)
        await rig.controller.close()
    }
}
