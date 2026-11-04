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

/// A signed-in app on `server`, with its event stream.
@MainActor
private func signedIn(_ server: FixtureServer) async throws -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
    await app.start()
    try #require(app.root == .signedIn)
    #expect(await eventually { server.openEventStreams == 1 })
    return app
}

/// Item 7 played on the profile's TV in the bedroom, whose player obeys `controls`: its session id.
@MainActor
private func playOnTheTv(_ app: AppModel, controls: [Control] = [.play, .pause, .seek, .skip],
                         device: String? = "Bedroom TV", client: String = "bedroom-tv") async throws -> String {
    let api = try #require(await app.session.api)
    return try await api.startPlayback(PlaybackRequest(itemId: 7, clientId: client, controls: controls,
                                                       deviceName: device)).sessionId
}

/// The remote on iPhone, iPad and Mac: the profile's sessions on its other
/// devices, and commands to them, said in the web remote's words.
@MainActor
@Suite struct RemoteModelTests {
    @Test func listsTheOtherDevicesAndLeavesThisOneOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        let version = app.sessionsVersion
        let tv = try await playOnTheTv(app)
        // The server tells of the session starting: the remote reads the list again.
        #expect(await eventually { app.sessionsVersion > version })
        await model.reload()
        #expect(model.others.map(\.id) == [tv])
        #expect(model.name(model.others[0]) == "Bedroom TV")

        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(await eventually { controller.session != nil })
        await model.reload()
        #expect(model.entries.count == 2)
        #expect(await eventually { model.here == app.sessionsHere })
        #expect(model.others.map(\.id) == [tv])
        await app.closePlayer(controller)
    }

    @Test func twoDevicesOfOneNameAreNumbered() {
        let sessions = [PlayingSession(sessionId: "a", device: "Living room TV"),
                        PlayingSession(sessionId: "b", device: "Living room TV"),
                        PlayingSession(sessionId: "c")]
        #expect(Words.deviceNames(sessions) == ["a": "Living room TV (1)", "b": "Living room TV (2)",
                                                "c": "Unknown device"])
        #expect(Words.remoteLabel(1) == "Remote, 1 device playing")
        #expect(Words.remoteLabel(2) == "Remote, 2 devices playing")
    }

    @Test func thePlaceMovesOnWhilePlayingByThisDevicesClock() {
        let received = Date(timeIntervalSince1970: 1_000)
        let playing = RemoteModel.Entry(PlayingSession(sessionId: "a", state: .playing, positionMs: 60_000,
                                                       durationMs: 61_000, updatedAt: 5, ageMs: 400),
                                        received: received)
        #expect(RemoteModel.position(playing, at: received) == 60_400)
        #expect(RemoteModel.position(playing, at: received.addingTimeInterval(0.3)) == 60_700)
        // Never past its end.
        #expect(RemoteModel.position(playing, at: received.addingTimeInterval(10)) == 61_000)
        let paused = RemoteModel.Entry(PlayingSession(sessionId: "a", state: .paused, positionMs: 60_000,
                                                      ageMs: 400), received: received)
        #expect(RemoteModel.position(paused, at: received.addingTimeInterval(10)) == 60_000)
    }

    @Test func aCommandIsAnsweredByTheReportThatFollows() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app, answerWait: .milliseconds(400))
        let tv = try await playOnTheTv(app)
        await model.reload()
        let entry = try #require(model.others.first)

        await model.send(.pause, to: entry)
        #expect(model.waiting == [tv])
        let sent = try #require(server.sessionCommands.last)
        #expect(sent.sessionId == tv)
        #expect(try PleyJSON.decoder.decode(PlayerCommand.self, from: sent.body) == .pause)
        // The TV pauses and reports: the list shows it.
        let version = app.sessionsVersion
        server.reportSession(tv, state: "paused", positionMs: 0)
        #expect(await eventually { app.sessionsVersion > version })
        await model.reload()
        #expect(model.waiting.isEmpty)
        #expect(model.others.first?.session.state == .paused)
        try await Task.sleep(for: .milliseconds(600))
        #expect(model.errors.isEmpty)
    }

    @Test func aPlayerThatDoesntActIsSaidNotToAnswer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app, answerWait: .milliseconds(200))
        let tv = try await playOnTheTv(app, controls: [.play, .pause, .seek, .skip, .next])
        await model.reload()
        let entry = try #require(model.others.first)

        await model.send(.skip(byMs: Words.remoteSkipForwardMs), to: entry)
        #expect(await eventually { model.errors[tv] == "Bedroom TV didn't answer." })
        #expect(model.waiting.isEmpty)

        // Nothing after it: a player staying where it is did as it should.
        await model.send(.next, to: entry)
        #expect(model.errors[tv] == nil)
        try await Task.sleep(for: .milliseconds(500))
        #expect(model.waiting.isEmpty)
        #expect(model.errors[tv] == nil)
    }

    @Test func refusalsAreSaidAsTheWebSaysThem() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        let tv = try await playOnTheTv(app, controls: [.play, .pause])
        await model.reload()
        let entry = try #require(model.others.first)

        // Not one it listed: the server refuses, and the list is read again.
        await model.send(.skip(byMs: 30_000), to: entry)
        #expect(model.errors[tv] == "Bedroom TV can't do that.")
        #expect(model.waiting.isEmpty)

        server.refuseCommands(status: 429, code: "too_many")
        await model.send(.pause, to: entry)
        #expect(model.errors[tv] == "Too many at once: try again in a moment.")

        server.refuseCommands(status: 403, code: "forbidden")
        await model.send(.pause, to: entry)
        #expect(model.errors[tv] == "Refused.")

        // Gone: it leaves the list at once.
        server.refuseCommands(status: 404, code: "not_found")
        await model.send(.pause, to: entry)
        #expect(model.errors[tv] == "Bedroom TV isn't playing any more.")
        #expect(model.entries.isEmpty)
    }

    @Test func stopEndsTheSessionThere() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        // A player that obeys nothing is stopped all the same.
        _ = try await playOnTheTv(app, controls: [], device: nil)
        await model.reload()
        let entry = try #require(model.others.first)
        #expect(model.name(entry) == "Pley for Mac")
        await model.stop(entry)
        #expect(model.others.isEmpty)
        #expect(model.errors.isEmpty)
        await model.reload()
        #expect(model.others.isEmpty)
    }

    @Test func aSeekIsAnsweredWhereItWasSent() {
        let received = Date(timeIntervalSince1970: 1_000)
        let before = RemoteModel.Entry(PlayingSession(sessionId: "a", item: ItemSummary(id: 7, libraryId: 1, kind: .movie, title: "Film"),
                                                      state: .playing, positionMs: 10_000, durationMs: 600_000,
                                                      updatedAt: 100), received: received)
        let sent = RemoteModel.Sent(command: .seek, before: before, sentAt: received, target: 300_000)
        let later = received.addingTimeInterval(2)
        // Not reported since: not yet.
        #expect(!RemoteModel.answered(sent, before, at: later))
        let there = RemoteModel.Entry(PlayingSession(sessionId: "a", item: ItemSummary(id: 7, libraryId: 1, kind: .movie, title: "Film"),
                                                     state: .playing, positionMs: 301_000, durationMs: 600_000,
                                                     updatedAt: 101), received: later)
        #expect(RemoteModel.answered(sent, there, at: later))
        // Gone: next and previous end a session to start the next.
        #expect(RemoteModel.answered(sent, nil, at: later))
        // Played on by itself, as it would have anyway: not this command.
        let playedOn = RemoteModel.Entry(PlayingSession(sessionId: "a", item: ItemSummary(id: 7, libraryId: 1, kind: .movie, title: "Film"),
                                                        state: .playing, positionMs: 12_000, durationMs: 600_000,
                                                        updatedAt: 102), received: later)
        #expect(!RemoteModel.answered(sent, playedOn, at: later))
    }
}
