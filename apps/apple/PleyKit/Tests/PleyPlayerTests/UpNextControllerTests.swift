import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// Two 75 s episodes, as make-test-media.sh makes them: an intro from 10 to
/// 30 s and credits from 55 s on, from their chapters.
@MainActor
private func showRig(start: Int? = nil, secondResumesAt: Int = 20_000, method: String = "direct",
                     autoplayNext: Bool = true) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    let chapters = [Chapter(startMs: 0, endMs: 10_000, title: "Prologue"),
                    Chapter(startMs: 10_000, endMs: 30_000, title: "Intro"),
                    Chapter(startMs: 30_000, endMs: 55_000, title: "Story"),
                    Chapter(startMs: 55_000, endMs: 75_000, title: "")]
    for (id, number) in [(31, 1), (32, 2)] {
        var episode = FixtureServer.Playable(itemId: id, fileId: id * 10, title: "Episode \(number)", container: "mp4",
                                             durationMs: 75_000, positionMs: number == 2 ? secondResumesAt : 0)
        episode.episode = (show: "Pley Test Show", season: 1, number: number)
        episode.next = number == 1 ? 32 : nil
        episode.chapters = chapters
        episode.markers = [Marker(fileId: id * 10, kind: .intro, startMs: 10_000, endMs: 30_000, source: "chapter"),
                           Marker(fileId: id * 10, kind: .credits, startMs: 55_000, endMs: 75_000, source: "chapter")]
        server.addPlayable(episode)
    }
    server.setPlaybackMethod(method)
    return try Rig(server: server, request: PlayRequest(itemId: 31, startMs: start), autoplayNext: autoplayNext)
}

/// A movie whose MP4 has English AAC (1) and French AC-3 5.1 (2).
@MainActor
private func twoAudioRig(method: String = "direct") async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    var movie = FixtureServer.Playable(itemId: 41, fileId: 410, title: "Two Audio", durationMs: 90_000)
    var english = MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true)
    english.channels = 2
    var french = MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")
    french.channels = 6
    movie.audio = [english, french]
    server.addPlayable(movie)
    server.setPlaybackMethod(method)
    let rig = try Rig(server: server, request: PlayRequest(itemId: 41))
    rig.engine.audio = method == "direct"
        ? [EngineAudioOption(position: 0, language: "en"), EngineAudioOption(position: 1, language: "fr")] : []
    return rig
}

@MainActor
private func started(_ rig: Rig) async throws -> ServerPlaybackSession {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
    return try #require(rig.controller.session)
}

/// Plays on at `positionMs`, `ms` later.
@MainActor
private func at(_ rig: Rig, _ positionMs: Int, after ms: Int64 = 500) {
    rig.engine.playing(at: positionMs)
    rig.clock.advance(ms: ms)
    rig.controller.tick()
}

@Suite("Skips, Up Next, chapters and audio in the controller")
@MainActor
struct UpNextControllerTests {
    @Test func skipIntroShowsInsideTheIntroAndLands750msBeforeItsEnd() async throws {
        let rig = try await showRig(start: 0)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        at(rig, 9_000)
        #expect(rig.controller.state.skipLabel == nil)
        at(rig, 12_000)
        #expect(rig.controller.state.skipLabel == "Skip Intro")
        rig.controller.skip()
        #expect(rig.engine.calls.filter { $0.hasPrefix("seek") } == ["seek 29250"])
        at(rig, 29_250)
        // A second before the intro ends, the button goes; nothing skips by itself.
        #expect(rig.controller.state.skipLabel == nil)
        at(rig, 56_000)
        #expect(rig.controller.state.skipLabel == "Skip Credits")
        #expect(rig.engine.calls.filter { $0.hasPrefix("seek") }.count == 1)
        await rig.controller.close()
    }

    @Test func enteringTheCreditsOffersUpNextOncePerSegment() async throws {
        let rig = try await showRig(start: 50_000)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        let card = try #require(rig.controller.state.upNext)
        #expect(card.itemId == 32)
        #expect(card.title == "Episode 2")
        #expect(card.heading == "Up next · S1 E2")
        #expect(card.imagePath == "/api/items/32/images/thumb")
        #expect(card.secondsLeft == 10)
        #expect(card.playLabel == "Play Next (10)")
        #expect(rig.server.requests("/api/items/31/next").count == 1)

        // Seeking back hides it; entering the same credits again doesn't bring it back.
        rig.controller.seek(toMs: 40_000)
        #expect(rig.controller.state.upNext == nil)
        at(rig, 40_500)
        rig.controller.seek(toMs: 60_000)
        at(rig, 60_500)
        #expect(rig.controller.state.skipLabel == "Skip Credits")
        #expect(rig.controller.state.upNext == nil)
        #expect(rig.server.requests("/api/items/31/next").count == 1)
        await rig.controller.close()
    }

    @Test func theCountdownPlaysTheNextEpisodeAfterTheOneBeforeEnded() async throws {
        let rig = try await showRig(start: 50_000)
        defer { rig.cleanUp() }
        let first = try await started(rig)
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        for _ in 0..<9 { at(rig, 56_000, after: 1_000) }
        #expect(rig.controller.state.upNext?.secondsLeft == 1)
        #expect(rig.engine.loads.count == 1)
        at(rig, 56_000, after: 1_000)
        try await until("the next episode loaded") { rig.engine.loads.count == 2 }
        let requests = await rig.requests()
        #expect(requests.map(\.itemId) == [31, 32])
        // It starts where it resumes.
        #expect(requests.last?.startMs == 20_000)
        #expect(rig.engine.loads.last?.startMs == 20_000)
        // The one before ended on the server first.
        let journal = rig.server.journal
        let deleted = try #require(journal.firstIndex { $0.method == "DELETE" && $0.path == "/api/playback/\(first.id)" })
        let second = try #require(journal.lastIndex { $0.method == "POST" && $0.path == "/api/playback" })
        #expect(deleted < second)
        #expect(rig.server.progressReports.contains { $0.itemId == 31 && String(decoding: $0.body, as: UTF8.self).contains("stopped") })
        rig.engine.emit(.ready)
        try await until("the second one plays") { rig.controller.state.phase == .playing }
        let state = rig.controller.state
        #expect(state.itemId == 32)
        #expect(state.title == "Pley Test Show")
        #expect(state.subtitle == "S1 E2 · Episode 2")
        #expect(state.upNext == nil)
        #expect(state.chapters.count == 4)
        await rig.controller.close()
    }

    @Test func withAutoplayOffTheCardShowsWithNoCountdownAndNothingPlaysByItself() async throws {
        let rig = try await showRig(start: 50_000, autoplayNext: false)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        let card = try #require(rig.controller.state.upNext)
        #expect(card.itemId == 32)
        #expect(card.secondsLeft == nil)
        #expect(card.playLabel == "Play Next")
        // Playing on through the credits: nothing counts down.
        for n in 1...15 { at(rig, 56_000 + n * 1_000, after: 1_000) }
        #expect(rig.controller.state.upNext?.secondsLeft == nil)
        #expect(rig.engine.loads.count == 1)
        // At the end too, the card waits for Play Next.
        rig.engine.emit(.ended)
        try await until("the end") { rig.controller.state.phase == .ended }
        for _ in 0..<15 {
            rig.clock.advance(ms: 1_000)
            rig.controller.tick()
        }
        #expect(rig.controller.state.upNext?.secondsLeft == nil)
        #expect(rig.engine.loads.count == 1)
        rig.controller.playNext()
        try await until("the next episode loaded") { rig.engine.loads.count == 2 }
        await rig.controller.close()
    }

    @Test func playNextAtTheEndMarksItWatchedAndSaysWhenNothingFollows() async throws {
        let rig = try await showRig(start: 60_000, secondResumesAt: 0)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        at(rig, 70_000)
        rig.controller.playNext()
        try await until("the next episode loaded") { rig.engine.loads.count == 2 }
        #expect(rig.server.playable(31)?.watched == true)
        rig.engine.emit(.ready)
        try await until("the second one plays") { rig.controller.state.itemId == 32 && rig.controller.state.phase == .playing }
        rig.controller.playNext()
        try await until("the notice") { rig.controller.state.notice != nil }
        #expect(rig.controller.state.notice == "This is the last episode")
        #expect(rig.engine.loads.count == 2)
        await rig.controller.close()
    }

    @Test func aCancelledCardComesBackWithItsCountdownAtTheEnd() async throws {
        let rig = try await showRig(start: 50_000)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        at(rig, 56_000)
        try await until("the card") { rig.controller.state.upNext != nil }
        rig.controller.cancelUpNext()
        #expect(rig.controller.state.upNext == nil)
        at(rig, 74_000, after: 1_000)
        #expect(rig.controller.state.upNext == nil)
        rig.engine.emit(.ended)
        try await until("the card again") { rig.controller.state.upNext != nil }
        #expect(rig.controller.state.upNext?.secondsLeft == 10)
        #expect(rig.controller.state.phase == .ended)
        rig.clock.advance(ms: 10_000)
        rig.controller.tick()
        try await until("the next episode loaded") { rig.engine.loads.count == 2 }
        #expect(await rig.requests().last?.itemId == 32)
        await rig.controller.close()
    }

    @Test func chaptersAreListedMarkedAndJumpedTo() async throws {
        let rig = try await showRig(start: 12_000)
        defer { rig.cleanUp() }
        _ = try await started(rig)
        let chapters = rig.controller.state.chapters
        #expect(chapters.map(\.title) == ["Prologue", "Intro", "Story", "Chapter 4"])
        #expect(chapters.map(\.current) == [false, true, false, false])
        #expect(chapters[2].label == "Story · 0:30")
        rig.controller.jumpToChapter(2)
        #expect(rig.engine.calls.last == "seek 30000")
        at(rig, 31_000)
        #expect(rig.controller.state.chapters.map(\.current) == [false, false, true, false])
        let picture = try #require(await rig.controller.chapterPicture(1))
        #expect(picture.starts(with: [0xFF, 0xD8]))
        #expect(rig.server.requests("/api/files/310/chapters/1.jpg").first?.headers["authorization"] != nil)
        #expect(await rig.controller.chapterPicture(9) == nil)
        await rig.controller.close()
    }

    @Test func directPlaySwitchesAudioInThePlayerAndSavesTheChoice() async throws {
        let rig = try await twoAudioRig()
        defer { rig.cleanUp() }
        let session = try await started(rig)
        // No track asked for: the server names one, which AVPlayer plays.
        #expect(await rig.requests().single?.audioIndex == nil)
        #expect(rig.engine.audioChoices == [EngineAudioOption(position: 0, language: "en")])
        #expect(rig.controller.state.audio.map(\.selected) == [true, false])
        #expect(rig.controller.state.audio.map(\.label) == ["English (AAC Stereo)", "French (AC3 5.1)"])

        rig.controller.selectAudio(2)
        #expect(rig.engine.audioChoices.last == EngineAudioOption(position: 1, language: "fr"))
        #expect(rig.controller.state.audio.map(\.selected) == [false, true])
        try await until("the choice saved") { rig.server.streamsChoices.count == 1 }
        let saved = try #require(JSONSerialization.jsonObject(with: rig.server.streamsChoices[0]) as? [String: Any])
        #expect(saved["file_id"] as? Int == 410)
        #expect(saved["audio_index"] as? Int == 2)
        #expect(saved["subtitle_index"] == nil)
        #expect(rig.controller.session === session)
        #expect(await rig.requests().count == 1)
        #expect(rig.server.playable(41)?.audioPicked == true)
        await rig.controller.close()
    }

    @Test func aChoiceInTheSystemsAudioMenuIsSaved() async throws {
        let rig = try await twoAudioRig()
        defer { rig.cleanUp() }
        let session = try await started(rig)
        rig.engine.emit(.audioChanged(EngineAudioOption(position: 1, language: "fr")))
        try await until("the choice saved") { rig.server.streamsChoices.count == 1 }
        let saved = try #require(JSONSerialization.jsonObject(with: rig.server.streamsChoices[0]) as? [String: Any])
        #expect(saved["audio_index"] as? Int == 2)
        #expect(rig.controller.state.audio.map(\.selected) == [false, true])
        #expect(rig.controller.session === session)
        await rig.controller.close()
    }

    @Test func aStreamsAudioSwitchStartsASessionBesideItWithAudioIndex() async throws {
        let rig = try await twoAudioRig(method: "remux")
        defer { rig.cleanUp() }
        let first = try await started(rig)
        #expect(await rig.requests().single?.audioIndex == nil)
        rig.engine.playing(at: 4_000)
        rig.controller.selectAudio(2)
        try await until("a second session") { rig.engine.loads.count == 2 }
        let again = try #require(await rig.requests().last)
        #expect(again.audioIndex == 2)
        #expect(again.keepPrevious == true)
        #expect(again.startMs == 4_000)
        #expect(rig.controller.session !== first)
        #expect(rig.controller.state.audio.map(\.selected) == [false, true])
        try await until("the first one deleted") {
            rig.server.requests("/api/playback/\(first.id)").map(\.method) == ["DELETE"]
        }
        await rig.controller.close()
    }
}

private extension Array {
    var single: Element? { count == 1 ? first : nil }
}
