import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// A ten-minute movie, or two episodes of a show, played on "Living room".
@MainActor
private func rig(show: Bool = false) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    if show {
        for (id, number) in [(31, 1), (32, 2)] {
            var episode = FixtureServer.Playable(itemId: id, fileId: id * 10, title: "Episode \(number)",
                                                 container: "mp4", durationMs: 75_000, positionMs: 0)
            episode.episode = (show: "Pley Test Show", season: 1, number: number)
            episode.next = number == 1 ? 32 : nil
            server.addPlayable(episode)
        }
    } else {
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
    }
    return try Rig(server: server, request: PlayRequest(itemId: show ? 31 : 7), deviceName: "Living room")
}

/// A 90 s movie played as it is, its MP4 with English AAC (1) and French
/// AC-3 5.1 (2), an English SRT (3) in it, no subtitles shown.
@MainActor
private func tracksRig() async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    var movie = FixtureServer.Playable(itemId: 41, fileId: 410, title: "Two Audio", durationMs: 90_000)
    var english = MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true)
    english.channels = 2
    var french = MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")
    french.channels = 6
    movie.audio = [english, french]
    var srt = MediaStream(index: 3, kind: .subtitle, codec: "subrip", language: "eng")
    srt.textBased = true
    movie.subtitles = [srt]
    server.addPlayable(movie)
    server.serveFile(path: "/api/files/410/subtitles/3.vtt", type: "text/vtt", body: Data("""
    WEBVTT

    00:00:02.000 --> 00:00:05.000
    A line.
    """.utf8))
    let rig = try Rig(server: server, request: PlayRequest(itemId: 41), deviceName: "Living room")
    rig.engine.audio = [EngineAudioOption(position: 0, language: "en"), EngineAudioOption(position: 1, language: "fr")]
    return rig
}

@MainActor
private func playing(_ rig: Rig) async throws -> ServerPlaybackSession {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
    rig.engine.playing(at: 0)
    return try #require(rig.controller.session)
}

@Suite("The playback controller and the profile's remotes")
@MainActor
struct RemoteControllerTests {
    @Test func aMovieSaysWhatItObeysAndWhereItPlays() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        _ = try await playing(rig)
        let request = try #require(await rig.requests().first)
        #expect(request.controls == [.play, .pause, .seek, .skip])
        #expect(request.deviceName == "Living room")
        #expect(PlaybackController.controls(.episode) == [.play, .pause, .seek, .skip, .next])
        // A collection's or playlist's videos have a next until their last.
        let queue = PlayQueue(kind: .playlist, id: 3, itemIds: [7, 8])
        #expect(PlaybackController.controls(.movie, queue: queue) == [.play, .pause, .seek, .skip, .next])
        #expect(PlaybackController.controls(.movie, queue: queue.advanced()) == [.play, .pause, .seek, .skip])
    }

    @Test func itObeysOnlyItsOwnSessionAndReportsAtOnce() async throws {
        let rig = try await rig()
        defer { rig.cleanUp() }
        let session = try await playing(rig)
        let before = await rig.reports().count

        // Another session's command changes nothing.
        rig.controller.obey(.pause, for: "other")
        rig.controller.obey(.seek(toMs: 30_000), for: "other")
        rig.controller.obey(.pause, for: "")
        #expect(!rig.engine.calls.contains("pause"))
        #expect(!rig.engine.calls.contains { $0.hasPrefix("seek") })
        #expect(await rig.reports().count == before)

        // A skip counts from where it is; both report at once, not a second later.
        rig.engine.playing(at: 100_000)
        rig.controller.obey(.skip(byMs: -30_000), for: session.id)
        #expect(rig.engine.calls.last == "seek 70000")
        #expect(await rig.reports().last.map { [$0.positionMs] } == [70_000])
        // A seek stops at the start and the end.
        rig.controller.obey(.skip(byMs: -600_000), for: session.id)
        #expect(rig.engine.calls.last == "seek 0")
        rig.controller.obey(.seek(toMs: 700_000), for: session.id)
        #expect(rig.engine.calls.last == "seek \(600_000 - PlaybackController.seekEndMarginMs)")
        let sought = try #require(await rig.reports().last)
        #expect(sought.positionMs == 600_000 - PlaybackController.seekEndMarginMs)
        #expect(sought.state == .playing && sought.sessionId == session.id)

        // A pause reports itself, play at once.
        rig.engine.playing(at: 200_000)
        rig.controller.obey(.pause, for: session.id)
        #expect(rig.engine.calls.last == "pause")
        #expect(rig.controller.state.phase == .paused)
        #expect(await rig.reports().last.map { [$0.state] } == [.paused])
        // Paused already: it says so again, so a remote a little behind settles.
        let paused = await rig.reports().count
        rig.controller.obey(.pause, for: session.id)
        #expect(rig.engine.calls.last == "pause")
        #expect(await rig.reports().count == paused + 1)
        #expect(await rig.reports().last?.state == .paused)
        rig.controller.obey(.play, for: session.id)
        #expect(rig.engine.calls.last == "play")
        #expect(await rig.reports().last.map { [$0.state, $0.positionMs] as [AnyHashable] }
            == [ProgressState.playing, 200_000])

        // A movie has no next, and no video a previous.
        let calls = rig.engine.calls.count
        rig.controller.obey(.next, for: session.id)
        rig.controller.obey(.previous, for: session.id)
        try await Task.sleep(for: .milliseconds(50))
        #expect(rig.engine.calls.count == calls)
        #expect(rig.engine.loads.count == 1)
        #expect(rig.controller.state.notice == nil)
    }

    @Test func aFileWithTracksSaysItChangesThemAndReportsThem() async throws {
        let rig = try await tracksRig()
        defer { rig.cleanUp() }
        let session = try await playing(rig)
        #expect(await rig.requests().first?.controls == [.play, .pause, .seek, .skip, .audio, .subtitles])
        // The first frame's report says the tracks it plays: the server's audio, no subtitles.
        let first = try #require(await rig.reports().first)
        #expect(first.audioIndex == 1 && first.subtitleIndex == SessionTracks.off)
        // Music says neither.
        #expect(!MusicPlayback.controls.contains(.audio) && !MusicPlayback.controls.contains(.subtitles))

        // Another audio track, as the Audio menu picks it: AVPlayer switches it, it is saved, and said at once.
        rig.controller.obey(.audio(2), for: session.id)
        #expect(rig.engine.audioChoices.last == EngineAudioOption(position: 1, language: "fr"))
        #expect(rig.controller.state.audio.map(\.selected) == [false, true])
        let switched = try #require(await rig.reports().last)
        #expect(switched.audioIndex == 2 && switched.subtitleIndex == SessionTracks.off)
        #expect(switched.sessionId == session.id)
        try await until("the choice saved") { rig.server.streamsChoices.count == 1 }
        #expect(rig.controller.session === session)
        // The one playing already: nothing changes, and it is said again, so a remote a little behind settles.
        let before = await rig.reports().count
        rig.controller.obey(.audio(2), for: session.id)
        #expect(await rig.reports().count == before + 1)
        #expect(rig.server.streamsChoices.count == 1)

        // Subtitles, then none: as the Subtitles menu shows them.
        rig.controller.obey(.subtitles(3), for: session.id)
        #expect(rig.controller.state.subtitles.first(where: \.selected)?.id == "3")
        #expect(await rig.reports().last?.subtitleIndex == 3)
        rig.controller.obey(.subtitles(nil), for: session.id)
        #expect(rig.controller.state.subtitleLabel == "Off")
        #expect(await rig.reports().last?.subtitleIndex == SessionTracks.off)
        // Another session's tracks change nothing.
        rig.controller.obey(.audio(1), for: "other")
        #expect(rig.controller.state.audio.map(\.selected) == [false, true])

        // The reports due say them too.
        rig.engine.playing(at: 20_000)
        rig.clock.advance(ms: 10_500)
        rig.controller.tick()
        let due = try #require(await rig.reports().last)
        #expect(due.positionMs == 20_000 && due.audioIndex == 2 && due.subtitleIndex == SessionTracks.off)
        await rig.controller.close()
    }

    @Test func nextPlaysTheNextEpisodeAndNothingAfterTheLast() async throws {
        let rig = try await rig(show: true)
        defer { rig.cleanUp() }
        let first = try await playing(rig)
        #expect(await rig.requests().first?.controls == [.play, .pause, .seek, .skip, .next])
        rig.controller.obey(.next, for: first.id)
        try await until("the next episode") { rig.engine.loads.count == 2 }
        #expect(rig.controller.state.itemId == 32)
        #expect(await rig.requests().last?.itemId == 32)
        // The first session ended as it moved on, and its commands no longer reach anything.
        #expect(!rig.server.openSessions.contains(first.id))
        rig.engine.emit(.ready)
        try await until("the second episode plays") { rig.controller.session != nil && rig.controller.state.phase == .playing }
        let second = try #require(rig.controller.session)
        rig.controller.obey(.pause, for: first.id)
        #expect(rig.controller.state.phase == .playing)
        // After the last one, next does nothing, and says nothing here.
        rig.controller.obey(.next, for: second.id)
        try await until("the look-up") { rig.server.requests("/api/items/32/next").count == 1 }
        try await Task.sleep(for: .milliseconds(50))
        #expect(rig.engine.loads.count == 2)
        #expect(rig.controller.session === second)
        #expect(rig.controller.state.notice == nil)
    }
}
