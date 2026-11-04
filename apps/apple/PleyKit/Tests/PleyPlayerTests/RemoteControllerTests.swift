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
        let paused = await rig.reports().count
        rig.controller.obey(.pause, for: session.id)
        #expect(await rig.reports().count == paused)
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
