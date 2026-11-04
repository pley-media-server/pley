import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// A signed-in app on `server`, with its event stream, its music playing into `output`.
@MainActor
private func signedIn(_ server: FixtureServer, output: QuietMusicOutput = QuietMusicOutput()) async throws
    -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                  music: { output })
    await app.start()
    try #require(app.root == .signedIn)
    #expect(await eventually { server.openEventStreams == 1 })
    return app
}

/// The profile's remote on another device: the fixture's API, as that device would call it.
@MainActor
private func remote(_ app: AppModel) async throws -> PleyAPI {
    try #require(await app.session.api)
}

@MainActor
private func reports(_ server: FixtureServer) -> [ProgressReport] {
    server.progressReports.compactMap { try? PleyJSON.decoder.decode(ProgressReport.self, from: $0.body) }
}

private struct NothingOpened: Error {}

/// The player a cast opened: at once on iPhone, iPad and Apple TV; on the
/// Mac, as the app's window opens it from `castOpening`.
@MainActor
private func castPlayer(_ app: AppModel) async throws -> PlaybackController {
    #if os(macOS)
    #expect(await eventually { app.castOpening != nil })
    guard case .video(let request)? = app.castOpening else { throw NothingOpened() }
    app.castOpened()
    return try #require(await app.play(request))
    #else
    #expect(await eventually { app.player != nil })
    return try #require(app.player)
    #endif
}

/// The players obey the profile's remotes on other devices: a command goes
/// to the server, which passes it on as `player_command`, and the player of
/// that session acts on it. Apple TV's players are these, from PleyKit.
@MainActor
@Suite struct RemoteScreensTests {
    @Test func aVideoObeysItsOwnSessionsCommands() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(await eventually { controller.session != nil })
        let id = try #require(controller.session?.id)
        #expect(app.sessionsHere == [id])
        let listed = try #require(try await remote(app).playingSessions().first)
        #expect(listed.sessionId == id)
        #expect(listed.device == "Living room")
        #expect(listed.controls == [.play, .pause, .seek, .skip])

        try await remote(app).send(.seek(toMs: 120_000), to: id)
        #expect(await eventually { reports(server).last?.positionMs == 120_000 })
        #expect(reports(server).last?.sessionId == id)
        // A movie has no next: the server refuses it before any player hears of it.
        do {
            try await remote(app).send(.next, to: id)
            Issue.record("next was taken")
        } catch let error as PleyError {
            #expect(error.notControllable)
        }
        await app.closePlayer(controller)
        #expect(app.sessionsHere.isEmpty)
    }

    @Test func musicObeysAndReportsAtOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addMusic()
        let output = QuietMusicOutput()
        let app = try await signedIn(server, output: output)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(await eventually { app.music.sessionId != nil && app.music.state.playing })
        let id = try #require(app.music.sessionId)
        #expect(app.sessionsHere == [id])
        let listed = try #require(try await remote(app).playingSessions().first)
        #expect(listed.controls == [.play, .pause, .seek, .skip, .next, .previous])

        output.positionMs = 30_000
        try await remote(app).send(.skip(byMs: 30_000), to: id)
        #expect(await eventually { output.positionMs == 60_000 })
        #expect(await eventually { reports(server).last?.positionMs == 60_000 })
        try await remote(app).send(.pause, to: id)
        #expect(await eventually { !app.music.state.playing })
        #expect(await eventually { reports(server).last?.state == .paused })
        try await remote(app).send(.next, to: id)
        #expect(await eventually { app.music.state.track?.id == 303 })
    }

    @Test func liveTvChangesChannelInTheOrderItTunesThem() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLiveLineup(count: 5)
        let app = try await signedIn(server)
        let model = try #require(await app.watchLive(LiveRequest(channelId: 1001, category: .all)))
        #expect(await eventually { model.playback.sessionId != nil })
        let first = try #require(model.playback.sessionId)
        let watch = try #require(server.liveWatchRequests.first.flatMap {
            try? PleyJSON.decoder.decode(LiveWatchRequest.self, from: $0)
        })
        #expect(watch.controls == [.next, .previous])
        #expect(watch.deviceName == "Living room")
        #expect(app.sessionsHere == [first])

        try await remote(app).send(.previous, to: first)
        // Round the start of the lineup, as channel down does here.
        #expect(await eventually { model.channelId == 1005 })
        #expect(await eventually { model.playback.sessionId != nil && model.playback.sessionId != first })
        let second = try #require(model.playback.sessionId)
        try await remote(app).send(.next, to: second)
        #expect(await eventually { model.channelId == 1001 })
        // No pause on Live TV: refused at the server.
        #expect(await eventually { model.playback.sessionId != nil && model.playback.sessionId != second })
        let third = try #require(model.playback.sessionId)
        await #expect(throws: PleyError.self) { try await remote(app).send(.pause, to: third) }
        app.closeLivePlayer()
    }

    @Test func aCommandForAnotherSessionChangesNothing() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(await eventually { controller.session != nil })
        let before = reports(server).count
        let event = #"{"type":"player_command","session_id":"other","command":"seek","position_ms":1000,"offset_ms":null}"#
        server.pushEvent(event)
        try await Task.sleep(for: .milliseconds(200))
        #expect(reports(server).count == before)
        #expect(controller.state.positionMs == 0)
    }

    @Test func theEventStreamNamesThisDeviceAsOneThatTakesCasts() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let asked = try #require(server.requests("/api/events").last)
        #expect(asked.query["receiver"] == "apple-test")
        #expect(asked.query["device_name"] == "Living room")
        #expect(asked.query["device_kind"] == DeviceName.kind)
        // The fixture's profile may watch Live TV: channels too, said once the stream opens.
        #expect(app.shell.showsLiveTv)
        #expect(asked.query["casts"] == "item,live")
        #expect(server.eventConnects == 1)
        #expect(app.receiver == EventReceiver(id: "apple-test", deviceName: "Living room",
                                              deviceKind: DeviceKind(rawValue: DeviceName.kind), casts: [.item, .live]))
    }

    @Test func withoutLiveTvItTakesTitlesAloneAndSaysSoAgainWhenItComes() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLiveAllowed(false)
        let app = try await signedIn(server)
        #expect(server.requests("/api/events").last?.query["casts"] == "item")
        // Live TV opened to the profile: the stream says it again.
        server.setLiveAllowed(true)
        app.serverEvents.heard(.liveChannelsChanged)
        #expect(await eventually { server.requests("/api/events").last?.query["casts"] == "item,live" })
        #expect(await eventually { app.serverEvents.connected })
    }

    @Test func aCastForThisDevicePlaysItAndSaysWhereItCameFrom() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000, positionMs: 30_000))
        let app = try await signedIn(server)
        // For another device: nothing plays here.
        server.sendCast(to: "den-tv", ["item_id": 7], from: "Kitchen tablet")
        try await Task.sleep(for: .milliseconds(200))
        #expect(app.player == nil && app.castOpening == nil)

        server.sendCast(to: "apple-test", ["item_id": 7, "start_ms": 61_000], from: "Kitchen tablet")
        let controller = try await castPlayer(app)
        #expect(await eventually { controller.session != nil })
        // From where the remote said, and said over the player.
        let asked = try #require(server.playbackRequests.last.flatMap {
            try? PleyJSON.decoder.decode(PlaybackRequest.self, from: $0)
        })
        #expect(asked.itemId == 7 && asked.startMs == 61_000)
        #expect(controller.state.notice == "Playing from Kitchen tablet")
        await app.closePlayer(controller)
    }

    @Test func aCastShowPlaysItsNextEpisodeAsItsPlayDoes() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        // The show's Play plays its next episode: the server's On Deck (204, for show 201).
        server.sendCast(to: "apple-test", ["item_id": 201], from: nil)
        let controller = try await castPlayer(app)
        #expect(server.requests("/api/items/201/on-deck").count == 1)
        #expect(await eventually { !server.requests("/api/items/204").isEmpty })
        #expect(controller.state.notice == "Playing from another device")
        await app.closePlayer(controller)
    }

    @Test func aCastAlbumOrTrackPlaysAsItsPageAndAChannelTunesIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addMusic()
        server.setLiveLineup(count: 5)
        let app = try await signedIn(server)
        // An album plays its tracks in order; a track, its album from it.
        server.sendCast(to: "apple-test", ["item_id": 302], from: "Kitchen tablet")
        #expect(await eventually { app.music.state.track?.id == 304 })
        #expect(app.shell.toast?.text == "Playing from Kitchen tablet")
        server.sendCast(to: "apple-test", ["item_id": 303], from: "Kitchen tablet")
        #expect(await eventually { app.music.state.track?.id == 303 })

        server.sendCast(to: "apple-test", ["channel_id": 1002], from: "Kitchen tablet")
        #if os(macOS)
        #expect(await eventually { app.castOpening == .live(LiveRequest(channelId: 1002, category: .all)) })
        app.castOpened()
        let model = try #require(await app.watchLive(LiveRequest(channelId: 1002, category: .all)))
        #else
        #expect(await eventually { app.livePlayer != nil })
        let model = try #require(app.livePlayer)
        #endif
        #expect(await eventually { model.channelId == 1002 })
        #expect(model.banner == "Playing from Kitchen tablet")
        app.closeLivePlayer()
    }

    @Test func aCastWhileTheAppIsAwayDoesNothing() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        app.sceneActive(false)
        app.serverEvents.heard(.cast(receiverId: "apple-test", target: .item(7), from: nil))
        try await Task.sleep(for: .milliseconds(300))
        #expect(app.player == nil && app.castOpening == nil)
    }

    @Test func theSessionsChangingIsCounted() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let version = app.sessionsVersion
        server.pushEvent(#"{"type":"sessions_changed"}"#)
        #expect(await eventually { app.sessionsVersion == version + 1 })
    }

    @Test func musicPlayingInTheBackgroundKeepsTheEventStream() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addMusic()
        let output = QuietMusicOutput()
        let app = try await signedIn(server, output: output)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(await eventually { app.music.state.playing })
        let connects = server.eventConnects
        app.sceneActive(false)
        try await Task.sleep(for: .milliseconds(100))
        #expect(app.serverEvents.connected)
        // A remote's pause still reaches it; paused, the stream goes, as the system would suspend the app.
        let id = try #require(app.music.sessionId)
        try await remote(app).send(.pause, to: id)
        #expect(await eventually { !app.music.state.playing })
        #expect(await eventually { !app.serverEvents.connected })
        #expect(server.eventConnects == connects)
        // Played again from the lock screen: the stream comes back.
        app.music.play()
        #expect(await eventually { app.serverEvents.connected })
        #expect(server.eventConnects == connects + 1)
        app.music.pause()
        #expect(await eventually { !app.serverEvents.connected })
        app.sceneActive(true)
        #expect(await eventually { app.serverEvents.connected })
    }
}
