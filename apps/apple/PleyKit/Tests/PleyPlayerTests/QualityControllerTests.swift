import Foundation
import os
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// A warmer the test lets through, and the streams it was asked for.
final class GateWarmer: StreamWarmer, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (open: false, asked: [URL](), waiting: [CheckedContinuation<Void, Never>]()))

    var asked: [URL] { state.withLock { $0.asked } }

    func warm(_ url: URL, startMs: Int) async throws {
        await withCheckedContinuation { (go: CheckedContinuation<Void, Never>) in
            let now = state.withLock { s -> Bool in
                s.asked.append(url)
                if s.open { return true }
                s.waiting.append(go)
                return false
            }
            if now { go.resume() }
        }
    }

    /// Every warm ends, now and from now on.
    func open() {
        let waiting = state.withLock { s in
            s.open = true
            defer { s.waiting = [] }
            return s.waiting
        }
        waiting.forEach { $0.resume() }
    }
}

/// 1080p at 10 Mbps: Original, Automatic, 8, 4 and 2 Mbps.
@MainActor
private func rig(quality: String = Qualities.automatic.id, ceiling: MemoryCeiling = MemoryCeiling(),
                 warmer: any StreamWarmer = NoWarmer(), network: Bool = true) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 3_600_000, bitrate: 10_000_000, width: 1920,
                             height: 1080))
    server.setPlaybackMethod("direct")
    server.setNetwork(routes: network)
    return try Rig(server: server, request: PlayRequest(itemId: 7), quality: MemoryQuality(quality),
                   ceiling: ceiling, warmer: warmer)
}

/// Starts the rig's playback and waits for its first frame.
@MainActor
@discardableResult
private func playing(_ rig: Rig) async throws -> ServerPlaybackSession {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    return try await shown(rig)
}

/// The last load's media shows.
@MainActor
@discardableResult
private func shown(_ rig: Rig) async throws -> ServerPlaybackSession {
    rig.engine.emit(.ready)
    let position = rig.engine.loads.last?.startMs ?? 0
    rig.engine.playing(at: position)
    rig.engine.current.bufferedMs = position + 30_000
    try await until("the first frame") { rig.controller.state.phase == .playing }
    return try #require(rig.controller.session)
}

/// Plays for `ms`, a tick each 500 ms: the playhead moves unless `waiting`.
@MainActor
private func run(_ rig: Rig, _ ms: Int, waiting: Bool = false, dropping: Int = 0, bandwidth: Int? = 50_000) {
    var left = ms
    while left > 0 {
        left -= 500
        rig.clock.advance(ms: 500)
        let e = rig.engine
        e.current.waiting = waiting
        e.current.playing = !waiting
        if !waiting {
            e.current.positionMs += 500
            e.current.watchedSeconds += 0.5
            e.current.droppedFrames += dropping
        }
        e.current.bufferedMs = waiting ? e.current.positionMs : e.current.positionMs + 30_000
        e.current.bandwidthKbps = bandwidth
        rig.controller.tick()
    }
}

@Suite("Quality and Automatic in the player")
@MainActor
struct QualityControllerTests {
    @Test func automaticIsTheDefaultAndPlaysTheOriginalAtHome() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        try await playing(rig)
        let request = try #require(await rig.requests().first)
        #expect(request.mode == .auto)
        #expect(request.maxBitrate == nil)
        let menu = rig.controller.state.qualities
        #expect(menu.map(\.id) == ["original", "auto", "8m", "4m", "2m"])
        #expect(menu.first { $0.selected }?.id == "auto")
        #expect(menu[1].label == "Automatic · Original")
        #expect(menu[1].detail == PlayerTexts.automaticHint)
        #expect(menu[0].detail == "1080p · 10 Mbps")
        #expect(rig.controller.state.quality == "Automatic · Original")
        #expect(rig.controller.state.qualityNote == nil)
        #expect(rig.controller.speedTests == 0)
        #expect(rig.server.requests("/api/playback/network").count == 1)
    }

    @Test func aQualityPickedIsRememberedAndStartsTheNextPlayback() async throws {
        let rig = try await rig(quality: "4m")
        defer { rig.cleanUp() }
        try await playing(rig)
        let request = try #require(await rig.requests().first)
        #expect(request.mode == .transcode)
        #expect(request.maxBitrate == 4_000)
        #expect(request.maxHeight == 720)
        #expect(rig.controller.state.quality == "4 Mbps 720p")
        // A quality picked asks the network nothing.
        #expect(rig.server.requests("/api/playback/network").isEmpty)
    }

    @Test func aQualityChangePlaysOnUntilTheNextStreamIsReadyThenDeletesTheOldSession() async throws {
        let warmer = GateWarmer()
        let rig = try await rig(warmer: warmer)
        defer { rig.cleanUp() }
        let old = try await playing(rig)
        run(rig, 10_000)
        rig.controller.selectQuality("4m")
        try await until("the stream asked for") { warmer.asked.count == 1 }
        // The old session plays on meanwhile.
        #expect(rig.engine.loads.count == 1)
        #expect(rig.controller.session === old)
        run(rig, 2_000)
        warmer.open()
        try await until("the new stream") { rig.engine.loads.count == 2 }
        let requests = await rig.requests()
        #expect(requests.map(\.keepPrevious) == [nil, true])
        #expect(requests.last?.mode == .transcode)
        #expect(requests.last?.maxBitrate == 4_000)
        // From where the old one got to.
        #expect(rig.engine.loads.last?.startMs == 12_000)
        let new = try #require(rig.controller.session)
        #expect(new.id != old.id)
        #expect(new.info.method == .transcode)
        await rig.reporter.flush()
        #expect(rig.server.requests("/api/playback/\(old.id)").map(\.method) == ["DELETE"])
        #expect(await rig.reports().filter { $0.sessionId == old.id }.allSatisfy { $0.state != .stopped })
        #expect(rig.controller.state.qualities.first { $0.selected }?.id == "4m")
        // What the conversion makes, for Details.
        try await shown(rig)
        try await until("the conversion described") { rig.controller.state.videoOut != nil }
        #expect(rig.controller.state.videoOut == "H.264 1280×720")
    }

    @Test func aSecondChangeWhileTheFirstGetsReadyWins() async throws {
        let warmer = GateWarmer()
        let rig = try await rig(warmer: warmer)
        defer { rig.cleanUp() }
        let old = try await playing(rig)
        rig.controller.selectQuality("4m")
        try await until("the first stream asked for") { warmer.asked.count == 1 }
        rig.controller.selectQuality("2m")
        try await until("the second stream asked for") { warmer.asked.count == 2 }
        warmer.open()
        try await until("the new stream") { rig.engine.loads.count == 2 }
        await rig.reporter.flush()
        let requests = await rig.requests()
        #expect(requests.map(\.maxBitrate) == [nil, 4_000, 2_000])
        #expect(rig.controller.session?.info.method == .transcode)
        // The one dropped and the one replaced are both gone.
        try await until("both deleted") { rig.server.openSessions.count == 1 }
        #expect(rig.server.openSessions == [rig.controller.session!.id])
        #expect(!rig.server.openSessions.contains(old.id))
    }

    @Test func noTranscodeFreeForTwoEndsTheOldSessionFirst() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let old = try await playing(rig)
        run(rig, 5_000)
        rig.server.refuseBeside()
        rig.controller.selectQuality("8m")
        try await until("the new stream") { rig.engine.loads.count == 2 }
        let requests = await rig.requests()
        #expect(requests.map(\.keepPrevious) == [nil, true, nil])
        #expect(await rig.reports().last { $0.sessionId == old.id }?.state == .stopped)
        #expect(rig.server.requests("/api/playback/\(old.id)").map(\.method) == ["DELETE"])
        #expect(rig.engine.loads.last?.startMs == 5_000)
    }

    @Test func originalPickedPlaysOnAndAutomaticStops() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        try await playing(rig)
        rig.controller.setViewPixelSize(width: 1280, height: 720)
        #expect(rig.engine.maximumResolution == (1280, 720))
        rig.controller.selectQuality(MenuChoice.original)
        #expect(rig.engine.maximumResolution == (0, 0))
        #expect(rig.controller.state.quality == "Original")
        // The same stream: nothing restarts.
        run(rig, 1_000)
        #expect(rig.engine.loads.count == 1)
        #expect(await rig.requests().count == 1)
        // A long wait, which Automatic would step down for.
        run(rig, 5_000)
        run(rig, 4_000, waiting: true)
        #expect(await rig.requests().count == 1)
    }

    @Test func automaticStepsDownForWaitsAndSaysSo() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        try await playing(rig)
        run(rig, 20_000, bandwidth: 6_000)
        run(rig, 3_000, waiting: true, bandwidth: 6_000)
        try await until("the lower stream") { rig.engine.loads.count == 2 }
        let request = try #require(await rig.requests().last)
        #expect(request.mode == .transcode)
        #expect(request.keepPrevious == true)
        let quality = try #require(request.maxBitrate)
        #expect(quality < 10_000)
        #expect(rig.controller.state.notice?.hasPrefix("Quality lowered to ") == true)
        rig.controller.dismissNotice()
        #expect(rig.controller.state.notice == nil)
        #expect(rig.controller.state.qualityNote == "Lowered for your connection")
        let automatic = try #require(rig.controller.state.qualities.first { $0.id == Qualities.automatic.id })
        #expect(automatic.selected)
        #expect(automatic.label.hasPrefix("Automatic · "))
        #expect(automatic.detail?.hasPrefix("Now ") == true)
    }

    @Test func automaticStartsAtTheRememberedCeilingWhenTheServerCantSay() async throws {
        let ceiling = MemoryCeiling()
        ceiling.write(kbps: 4_500, atMs: 1_000_000 - 60_000)
        let rig = try await rig(ceiling: ceiling, network: false)
        defer { rig.cleanUp() }
        try await playing(rig)
        let request = try #require(await rig.requests().first)
        #expect(request.mode == .transcode)
        #expect(request.maxBitrate == 4_000)
        #expect(rig.controller.state.quality == "Automatic · 4 Mbps 720p")
        #expect(rig.controller.state.qualityNote == "Lowered for your connection")
    }

    @Test func droppedFramesAtTheOriginalAskForTheServersConversionOnce() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        try await playing(rig)
        // 10 frames lost every half second: far more than 1 % of 30 a second.
        run(rig, 4_000, dropping: 10)
        try await until("the conversion") { rig.engine.loads.count == 2 }
        let request = try #require(await rig.requests().last)
        #expect(request.mode == .transcode)
        #expect(request.maxBitrate == nil)
        #expect(rig.controller.session?.info.method == .transcode)
        #expect(rig.controller.state.qualityNote == "Lowered for this device")
        #expect(rig.controller.state.quality == "Automatic · 8 Mbps 1080p")
    }

    @Test func droppedFramesWithAQualityPickedAreLeftToTheRecoveryLadderOnce() async throws {
        let rig = try await rig(quality: MenuChoice.original)
        defer { rig.cleanUp() }
        try await playing(rig)
        run(rig, 4_000, dropping: 10)
        try await until("the conversion") { rig.engine.loads.count == 2 }
        #expect(rig.controller.session?.info.method == .transcode)
        // A transcode that drops frames too: nothing more.
        try await shown(rig)
        run(rig, 10_000, dropping: 10)
        #expect(rig.engine.loads.count == 2)
    }
}
