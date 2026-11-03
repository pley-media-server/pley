import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// Two 75 s episodes with credits from 55 s on, the first playing from `start`.
@MainActor
private func showRig(start: Int = 0) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    for (id, number) in [(31, 1), (32, 2)] {
        var episode = FixtureServer.Playable(itemId: id, fileId: id * 10, title: "Episode \(number)", container: "mp4",
                                             durationMs: 75_000)
        episode.episode = (show: "Pley Test Show", season: 1, number: number)
        episode.next = number == 1 ? 32 : nil
        episode.markers = [Marker(fileId: id * 10, kind: .credits, startMs: 55_000, endMs: 75_000, source: "chapter")]
        server.addPlayable(episode)
    }
    return try Rig(server: server, request: PlayRequest(itemId: 31, startMs: start))
}

@MainActor
private func started(_ rig: Rig) async throws {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
}

/// Plays on at `positionMs`, `ms` later.
@MainActor
private func at(_ rig: Rig, _ positionMs: Int, after ms: Int64 = 500) {
    rig.engine.playing(at: positionMs)
    rig.clock.advance(ms: ms)
    rig.controller.tick()
}

@Suite("Speed and the sleep timer in the controller")
@MainActor
struct SpeedAndSleepTests {
    @Test func theSpeedsAreAndroidsAndTheMenuMarksTheOnePlaying() {
        #expect(PlaybackSpeeds.all == [0.5, 0.75, 1, 1.25, 1.5, 2])
        #expect(PlaybackSpeeds.all.map(PlaybackSpeeds.label) == ["0.5×", "0.75×", "1×", "1.25×", "1.5×", "2×"])
        #expect(PlaybackSpeeds.nearest(1.6) == 1.5 && PlaybackSpeeds.nearest(9) == 2)
        #expect(PlaybackSpeeds.speed(id: "1.25") == 1.25 && PlaybackSpeeds.speed(id: "3") == nil)
        var state = PlayerState()
        #expect(state.speeds.filter(\.selected).map(\.label) == ["1×"])
        state.speed = 1.5
        #expect(state.speeds.filter(\.selected).map(\.id) == ["1.5"])
        #expect(state.speedLabel == "1.5×")
    }

    @Test func aSpeedHoldsForThisTitleAndTheNextPlaysAtNormal() async throws {
        let rig = try await showRig(start: 50_000)
        defer { rig.cleanUp() }
        try await started(rig)
        rig.controller.setSpeed(1.5)
        #expect(rig.engine.rates == [1.5])
        #expect(rig.controller.state.speed == 1.5)
        // The same again does nothing.
        rig.controller.setSpeed(1.5)
        #expect(rig.engine.rates == [1.5])
        // A new session of this title keeps it: the engine's rate holds for the media loaded after.
        rig.controller.retry()
        try await until("the second load") { rig.engine.loads.count == 2 }
        #expect(rig.engine.rates == [1.5] && rig.controller.state.speed == 1.5)
        rig.engine.emit(.ready)
        try await until("playing again") { rig.controller.state.phase == .playing }
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        rig.controller.playNext()
        #expect(rig.engine.rates == [1.5, 1])
        #expect(rig.controller.state.speed == 1)
        await rig.controller.close()
    }

    @Test func afterSomeMinutesItPausesWithANoticeAndKeepWatchingAddsTheTimeAgain() async throws {
        let rig = try await showRig()
        defer { rig.cleanUp() }
        try await started(rig)
        rig.controller.setSleep(.minutes(15))
        #expect(rig.controller.state.sleep.choice == .minutes(15))
        #expect(rig.controller.state.sleepSummary == "15 min left")
        #expect(rig.controller.state.sleepChoices.filter(\.selected).map(\.label) == ["15 minutes"])
        rig.clock.advance(ms: 14 * 60_000 + 35_000)
        rig.controller.tickSleepTimer()
        #expect(rig.controller.state.sleep.secondsLeft == 25)
        #expect(rig.controller.state.sleepSummary == "Pausing in 25 s")
        rig.controller.keepWatching()
        #expect(rig.controller.state.sleep.secondsLeft == nil)
        #expect(rig.controller.state.sleep.minutesLeft == 16)
        #expect(!rig.engine.calls.contains("pause"))
        rig.clock.advance(ms: 15 * 60_000 + 25_000)
        rig.controller.tickSleepTimer()
        #expect(rig.engine.calls.last == "pause")
        #expect(rig.controller.state.phase == .paused)
        #expect(rig.controller.state.sleep.fired && rig.controller.state.sleep.choice == .off)
        rig.controller.play()
        #expect(!rig.controller.state.sleep.fired)
        await rig.controller.close()
    }

    @Test func theEndOfThisEpisodeCountsInTimeWatchedAndUpNextWaits() async throws {
        let rig = try await showRig(start: 15_000)
        defer { rig.cleanUp() }
        try await started(rig)
        #expect(rig.controller.state.sleepItem == "episode")
        rig.controller.setSleep(.endOfItem)
        #expect(rig.controller.state.sleepSummary == "End of this episode")
        // At 2×, the minute left of the episode is 30 s watched.
        rig.controller.setSpeed(2)
        at(rig, 15_000)
        rig.controller.tickSleepTimer()
        #expect(rig.controller.state.sleep.secondsLeft == 30)
        // Its credits bring up the next one, with no countdown.
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        #expect(rig.controller.state.upNext?.secondsLeft == nil)
        rig.engine.emit(.ended)
        try await until("the end") { rig.controller.state.phase == .ended }
        #expect(rig.controller.state.sleep.fired)
        #expect(rig.controller.state.upNext?.itemId == 32 && rig.controller.state.upNext?.secondsLeft == nil)
        // Nothing plays by itself.
        rig.clock.advance(ms: 15_000)
        rig.controller.tick()
        #expect(rig.engine.loads.count == 1)
        await rig.controller.close()
    }

    @Test func keepWatchingAtTheEndOfAnEpisodeWaitsForTheNextOnesEnd() async throws {
        let rig = try await showRig(start: 50_000)
        defer { rig.cleanUp() }
        try await started(rig)
        rig.controller.setSleep(.endOfItem)
        at(rig, 50_000)
        rig.controller.tickSleepTimer()
        #expect(rig.controller.state.sleep.secondsLeft == 25)
        rig.controller.keepWatching()
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        // Spared: this one's Up Next counts down as usual.
        #expect(rig.controller.state.upNext?.secondsLeft == 10)
        rig.engine.emit(.ended)
        try await until("the end") { rig.controller.state.phase == .ended }
        #expect(!rig.controller.state.sleep.fired && rig.controller.state.sleep.choice == .endOfItem)
        await rig.controller.close()
    }
}

/// Find Subtitles…: a track added on the server joins the menu and shows.
@Suite("Subtitles found online in the controller")
@MainActor
struct AddedSubtitleTests {
    @Test func anAddedTrackIsSelectedSavedAndDrawn() async throws {
        let server = FixtureServer()
        try await server.start()
        var movie = FixtureServer.Playable(itemId: 51, fileId: 510, title: "Subtitled", durationMs: 90_000)
        var srt = MediaStream(index: 2, kind: .subtitle, codec: "subrip", language: "eng")
        srt.textBased = true
        movie.subtitles = [srt]
        server.addPlayable(movie)
        server.setSubtitleResults([SubtitleResult(id: "opensubtitles:1:en", language: "en", name: "Release")])
        let rig = try Rig(server: server, request: PlayRequest(itemId: 51), warmer: NoWarmer())
        defer { rig.cleanUp() }
        try await started(rig)
        #expect(rig.controller.state.canFindSubtitles)
        #expect(rig.controller.state.subtitles.map(\.id) == ["off", "2"])
        let track = try await api(server).addSubtitle(itemId: 51, result: "opensubtitles:1:en", fileId: 510)
        rig.controller.addedSubtitle(track)
        #expect(rig.controller.state.subtitles.map(\.id) == ["off", "2", "9000"])
        #expect(rig.controller.state.subtitles.first(where: \.selected)?.id == "9000")
        #expect(rig.controller.state.canSyncSubtitles)
        // Saved for the movie, as a pick is.
        try await until("the choice saved") { !server.streamsChoices.isEmpty }
        let body = try #require(server.streamsChoices.last)
        let saved = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(saved["subtitle_index"] as? Int == 9000)
        // Drawn from its WebVTT.
        at(rig, 2_000)
        try await until("the cue") {
            rig.controller.tick()
            if case .text(let cues) = rig.controller.state.cues { return cues.map(\.plain) == ["Found online."] }
            return false
        }
        await rig.controller.close()
    }
}
