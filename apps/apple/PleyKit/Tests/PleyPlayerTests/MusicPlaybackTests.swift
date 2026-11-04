import Foundation
import PleyCaps
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// A music output that does what the test says.
@MainActor
final class FakeMusicOutput: MusicOutput {
    struct Load: Equatable {
        let url: URL
        let startMs: Int
        let play: Bool
    }

    let events: AsyncStream<MusicOutputEvent>
    private let continuation: AsyncStream<MusicOutputEvent>.Continuation
    private(set) var loads: [Load] = []
    private(set) var calls: [String] = []
    var positionMs = 0
    var durationMs = 0
    private(set) var playWhenReady = false
    private(set) var released = false

    init() {
        (events, continuation) = AsyncStream.makeStream(of: MusicOutputEvent.self)
    }

    func load(url: URL, startMs: Int, play: Bool) {
        loads.append(Load(url: url, startMs: startMs, play: play))
        positionMs = startMs
        playWhenReady = play
        continuation.yield(.playWhenReady(play))
    }

    func play() {
        calls.append("play")
        playWhenReady = true
        continuation.yield(.playWhenReady(true))
    }

    func pause() {
        calls.append("pause")
        playWhenReady = false
        continuation.yield(.playWhenReady(false))
    }

    func seek(toMs positionMs: Int) {
        calls.append("seek \(positionMs)")
        self.positionMs = positionMs
    }

    func stop() { calls.append("stop") }

    func release() {
        released = true
        continuation.finish()
    }

    func emit(_ event: MusicOutputEvent) { continuation.yield(event) }
}

/// Music on a fixture server playing All Melody's three tracks, with a fake output and clock.
@MainActor
private struct MusicRig {
    let server: FixtureServer
    let clock = FakeClock()
    let output = FakeMusicOutput()
    let reporter = SessionReporter()
    let music: MusicPlayback
    /// The album's tracks in its order: 304, 303, 305.
    let tracks: [ItemSummary]

    init() async throws {
        server = FixtureServer()
        try await server.start()
        server.addMusic()
        let api = api(server)
        tracks = try await api.children(302)
        let caps = DeviceCaps(probe: FixedProbe(input: try recordedProbe()))
        let output = output
        music = MusicPlayback(environment: MusicEnvironment(
            api: { api }, caps: { await caps.warmUp().report }, clientId: "apple-test-music", reporter: reporter,
            clock: clock, outputs: { output }, random: { _ in 0 }, deviceName: "Kitchen"))
    }

    func requests() async -> [PlaybackRequest] {
        await reporter.flush()
        return server.playbackRequests.compactMap { try? PleyJSON.decoder.decode(PlaybackRequest.self, from: $0) }
    }

    func reports() async -> [ProgressReport] {
        await reporter.flush()
        return server.progressReports.compactMap { try? PleyJSON.decoder.decode(ProgressReport.self, from: $0.body) }
    }

    /// The `n`th load came, and its sound moves.
    func playing(loads n: Int) async throws {
        try await until("load \(n)") { output.loads.count == n }
        output.emit(.playing(true))
        try await until("it plays") { music.state.playing && !music.state.loading }
    }
}

@MainActor
struct MusicPlaybackTests {
    @Test func eachTrackHasASessionOfItsOwn() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        #expect(rig.music.state.track?.id == 304)
        #expect(rig.music.state.loading)
        try await rig.playing(loads: 1)
        let first = try #require(rig.music.sessionId)
        #expect(rig.output.loads[0].url.absoluteString.contains("/api/media/pley_mk_"))
        #expect(rig.output.loads[0].play)
        var asked = await rig.requests()
        #expect(asked.count == 1)
        #expect(asked[0].itemId == 304 && asked[0].mode == .auto && asked[0].keyedUrls == true)
        #expect(asked[0].clientId == "apple-test-music")
        #expect(rig.music.state.upNext.map(\.track.id) == [303, 305])

        // The track ends: `stopped` at its length, the session goes, and the next one starts.
        rig.output.emit(.ended)
        try await rig.playing(loads: 2)
        #expect(rig.music.state.track?.id == 303)
        #expect(rig.music.sessionId != first)
        asked = await rig.requests()
        #expect(asked.map(\.itemId) == [304, 303])
        let reports = await rig.reports()
        #expect(reports.contains { $0.sessionId == first && $0.state == .stopped && $0.positionMs == 552_000 })
        #expect(!rig.server.openSessions.contains(first))
        #expect(rig.server.openSessions.count == 1)
    }

    @Test func theQueueEndsAndPlayStartsItsLastTrackAgain() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks, start: 2)
        try await rig.playing(loads: 1)
        rig.output.emit(.ended)
        try await until("the end") { rig.music.state.ended }
        #expect(!rig.music.state.playing)
        #expect(rig.music.sessionId == nil)
        rig.music.play()
        try await rig.playing(loads: 2)
        #expect(rig.music.state.track?.id == 305)
        #expect(await rig.requests().map(\.itemId) == [305, 305])
    }

    @Test func aTrackTheDeviceCantDecodeIsConverted() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        rig.output.positionMs = 1_500
        rig.output.emit(.failed(.decode))
        try await until("the conversion") { rig.output.loads.count == 2 }
        let asked = await rig.requests()
        #expect(asked.map(\.mode) == [.auto, .transcode])
        #expect(asked[1].startMs == 1_500)
        // Converted, it still can't: it says so.
        rig.output.emit(.failed(.decode))
        try await until("the error") { rig.music.state.error == .unplayable }
        #expect(MusicError.unplayable.text == "This track can't be played.")
    }

    @Test func anExpiredSessionStartsAgainThreeTimesAtMost() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await until("load 1") { rig.output.loads.count == 1 }
        for n in 2...4 {
            rig.output.emit(.failed(.gone))
            try await until("load \(n)") { rig.output.loads.count == n }
        }
        rig.output.emit(.failed(.gone))
        try await until("the error") { rig.music.state.error == .unplayable }
        #expect(await rig.requests().count == 4)
    }

    @Test func theServerStoppingItStopsTheMusic() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        let id = try #require(rig.music.sessionId)
        rig.music.stoppedByServer()
        #expect(rig.music.state.error == .stopped)
        #expect(!rig.music.state.playing)
        #expect(rig.music.sessionId == nil)
        #expect(rig.output.calls.contains("pause"))
        #expect(await rig.reports().contains { $0.sessionId == id && $0.state == .stopped })
        // Play starts the track again, in a new session.
        rig.music.play()
        try await rig.playing(loads: 2)
        #expect(rig.music.state.error == nil)
        #expect(rig.music.state.track?.id == 304)
    }

    @Test func aPauseIsReported() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        rig.output.positionMs = 12_000
        rig.music.toggle()
        try await until("paused") { !rig.music.state.playing }
        try await until("the report") { await rig.reports().contains { $0.state == .paused && $0.positionMs == 12_000 } }
        rig.music.toggle()
        #expect(rig.output.calls.suffix(1) == ["play"])
    }

    @Test func previousRestartsOrGoesBack() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks, start: 1)
        try await rig.playing(loads: 1)
        rig.output.positionMs = 10_000
        rig.music.previous()
        #expect(rig.output.calls.contains("seek 0"))
        #expect(rig.output.loads.count == 1)
        rig.output.positionMs = 1_000
        rig.music.previous()
        try await until("the track before") { rig.output.loads.count == 2 }
        #expect(rig.music.state.track?.id == 304)
    }

    @Test func nextJumpAndShuffle() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        rig.music.next()
        try await until("the next track") { rig.output.loads.count == 2 }
        #expect(rig.music.state.track?.id == 303)
        rig.music.jump(to: 2)
        try await until("the jump") { rig.output.loads.count == 3 }
        #expect(rig.music.state.track?.id == 305)
        rig.music.setShuffle(true)
        #expect(rig.music.state.shuffle)
        #expect(rig.music.state.track?.id == 305)
        #expect(rig.music.state.index == 0)
    }

    @Test func theSleepTimerAtTheEndOfATrackHoldsTheNextOne() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        rig.music.setSleep(.endOfItem)
        #expect(rig.music.state.sleep.choice == .endOfItem)
        rig.output.emit(.ended)
        try await until("the next track") { rig.output.loads.count == 2 }
        #expect(!rig.output.loads[1].play)
        #expect(rig.music.state.track?.id == 303)
        #expect(rig.music.state.sleep.fired)
        #expect(!rig.music.state.playing)
    }

    @Test func aTimerOfMinutesPauses() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        rig.music.setSleep(.minutes(15))
        #expect(rig.music.state.sleep.minutesLeft == 15)
        rig.clock.advance(ms: 15 * 60_000)
        rig.music.setSleep(.minutes(15))
        #expect(rig.music.state.sleep.minutesLeft == 15)
    }

    @Test func stopEndsTheSessionAndForgetsTheQueue() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        let id = try #require(rig.music.sessionId)
        rig.music.stop()
        #expect(!rig.music.active)
        #expect(rig.music.state == MusicState())
        #expect(rig.output.released)
        _ = await rig.reports()
        #expect(!rig.server.openSessions.contains(id))
    }

    @Test func itObeysTheProfilesRemotesForTheTrackPlayingNow() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks)
        try await rig.playing(loads: 1)
        let asked = try #require(await rig.requests().first)
        #expect(asked.controls == [.play, .pause, .seek, .skip, .next, .previous])
        #expect(asked.deviceName == "Kitchen")
        let id = try #require(rig.music.sessionId)
        let duration = rig.music.state.durationMs
        #expect(duration > 60_000)
        rig.output.positionMs = 20_000

        // Another session's command changes nothing.
        rig.music.obey(.pause, for: "other")
        rig.music.obey(.seek(toMs: 5_000), for: "other")
        #expect(rig.music.state.playing)
        #expect(!rig.output.calls.contains { $0.hasPrefix("seek") })

        // A skip counts from where the track is, and reports at once.
        rig.music.obey(.skip(byMs: 30_000), for: id)
        #expect(rig.output.calls.last == "seek 50000")
        try await until("the skip's report") { await rig.reports().last?.positionMs == 50_000 }
        // A seek stops at the start and the end.
        rig.music.obey(.skip(byMs: -600_000), for: id)
        #expect(rig.output.calls.last == "seek 0")
        rig.music.obey(.seek(toMs: duration + 60_000), for: id)
        #expect(rig.output.calls.last == "seek \(duration)")
        try await until("the seek's report") { await rig.reports().last?.positionMs == duration }
        #expect(await rig.reports().last?.state == .playing)
        #expect(await rig.reports().allSatisfy { $0.sessionId == id })

        // A pause reports itself; play reports at once.
        rig.music.obey(.pause, for: id)
        try await until("paused") { !rig.music.state.playing }
        try await until("the pause's report") { await rig.reports().last?.state == .paused }
        rig.music.obey(.play, for: id)
        try await until("playing") { rig.music.state.playing }
        try await until("the play's report") { await rig.reports().last?.state == .playing }
        let reports = await rig.reports().count
        // Play while playing changes nothing but says so again, so a remote a little behind settles.
        rig.music.obey(.play, for: id)
        try await until("play's report again") { await rig.reports().count == reports + 1 }
        #expect(rig.music.state.playing)
        #expect(await rig.reports().last?.state == .playing)
    }

    @Test func remotesNextAndPreviousMoveAsItsOwnButtonsDo() async throws {
        let rig = try await MusicRig()
        defer { rig.server.stop() }
        rig.music.play(tracks: rig.tracks, start: 1)
        try await rig.playing(loads: 1)
        var id = try #require(rig.music.sessionId)
        // Previous a while into a track starts it again, reported at once.
        rig.output.positionMs = 40_000
        rig.music.obey(.previous, for: id)
        #expect(rig.output.calls.last == "seek 0")
        #expect(rig.music.sessionId == id)
        try await until("the restart's report") { await rig.reports().last?.positionMs == 0 }
        // Early in it, the track before.
        rig.output.positionMs = 1_000
        rig.music.obey(.previous, for: id)
        try await until("the track before") { rig.output.loads.count == 2 }
        #expect(rig.music.state.track?.id == 304)
        // The old session's commands no longer reach anything.
        rig.music.obey(.next, for: id)
        try await rig.playing(loads: 2)
        #expect(rig.music.state.track?.id == 304)
        id = try #require(rig.music.sessionId)
        rig.music.obey(.next, for: id)
        try await rig.playing(loads: 3)
        #expect(rig.music.state.track?.id == 303)
        id = try #require(rig.music.sessionId)
        rig.music.obey(.next, for: id)
        try await rig.playing(loads: 4)
        #expect(rig.music.state.track?.id == 305)
        // Next at the queue's end does nothing.
        id = try #require(rig.music.sessionId)
        rig.music.obey(.next, for: id)
        try await Task.sleep(for: .milliseconds(50))
        #expect(rig.output.loads.count == 4)
        #expect(rig.music.sessionId == id)
    }

    @Test func errorsAreSaidAsAndroidSaysThem() {
        #expect(MusicPlayback.errorOf(PleyError.signedOut) == .signedOut)
        #expect(MusicPlayback.errorOf(PleyError.timedOut) == .offline)
        #expect(MusicError.offline.text == "Can't reach the server.")
        #expect(MusicError.stopped.text == "Playback was stopped by the server.")
    }
}
