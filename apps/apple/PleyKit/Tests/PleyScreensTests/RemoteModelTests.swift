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
        // In the order of their ids, whatever the list's: a device keeps its number as it plays and stops.
        let devices: [(id: String, name: String?)] = [("app-2", "Living room TV"), ("app-1", "Living room TV"),
                                                      ("app-3", nil)]
        #expect(Words.deviceNames(devices) == ["app-1": "Living room TV (1)", "app-2": "Living room TV (2)",
                                               "app-3": "Unknown device"])
        #expect(Words.remoteLabel(1) == "Remote, 1 device")
        #expect(Words.remoteLabel(2) == "Remote, 2 devices")
    }

    @Test func devicesAreTheReceiversWithWhatTheyPlayThisOneLeftOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        server.addReceiver("den-tv", deviceId: "app-90", name: "Den TV")
        server.addReceiver("bedroom-tv", deviceId: "app-91", name: "Bedroom TV", casts: ["item"])
        let bedroom = server.playElsewhere(7, deviceId: "app-91", device: "Bedroom TV")
        // An app that takes no casts (a Jellyfin app's): listed all the same.
        let other = server.playElsewhere(7, deviceId: "token-5", device: "Other app", kind: "other", controls: [])
        // A session of this device's own sign-in that doesn't play here (its end not heard yet): this device's.
        let api = try #require(await app.session.api)
        _ = try await api.startPlayback(PlaybackRequest(itemId: 7, clientId: "apple-test-gone"))
        await model.reload()
        // This device's own stream named it: it is a receiver, but not one of the remote's devices.
        #expect(model.receivers.contains { $0.receiverId == app.receiverId })
        #expect(model.entries.count == 3)
        let devices = model.devices
        // Those playing first, then those ready to play, each by name.
        #expect(devices.map(\.id) == ["app-91", "token-5", "app-90"])
        #expect(devices[0].receiver?.receiverId == "bedroom-tv" && devices[0].entry?.id == bedroom)
        #expect(devices[0].kind == .tv && model.name(devices[0]) == "Bedroom TV")
        #expect(devices[1].receiver == nil && devices[1].entry?.id == other && devices[1].kind == .other)
        #expect(devices[2].entry == nil && model.name(devices[2]) == "Den TV")
        // "Play on…" offers those that take it.
        #expect(model.takers(.item).map(\.id) == ["app-91", "app-90"])
        #expect(model.takers(.live).map(\.id) == ["app-90"])
        // Several devices: none chosen until one is; closed, the choice goes.
        #expect(model.selected == nil)
        model.show("app-90")
        #expect(model.presented && model.selected?.id == "app-90")
        model.presented = false
        #expect(model.chosen == nil)

        // One device alone is chosen at once.
        server.removeReceiver("den-tv")
        server.removeReceiver("bedroom-tv")
        _ = try await app.session.api?.stopSession(bedroom)
        await model.reload()
        #expect(model.devices.map(\.id) == ["token-5"])
        #expect(model.selected?.id == "token-5")
    }

    @Test func aTrackIsAnsweredByTheTrackItPlays() async throws {
        let server = try await fixture()
        defer { server.stop() }
        var movie = FixtureServer.Playable(itemId: 7, fileId: 70, durationMs: 600_000)
        movie.audio = [MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true),
                       MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")]
        movie.subtitles = [MediaStream(index: 3, kind: .subtitle, codec: "subrip", language: "eng")]
        server.addPlayable(movie)
        let app = try await signedIn(server)
        let model = RemoteModel(app: app, answerWait: .milliseconds(300))
        let tv = server.playElsewhere(7, deviceId: "app-90", device: "Bedroom TV",
                                      controls: ["play", "pause", "audio", "subtitles"])
        await model.reload()
        let entry = try #require(model.others.first)
        #expect(entry.session.tracks.audio.count == 2 && entry.session.audioIndex == 1)
        #expect(entry.session.subtitleIndex == SessionTracks.off)

        await model.send(.audio(2), to: entry)
        #expect(model.waiting == [tv])
        // The TV plays it and reports: the list shows the track, which answers the command.
        let version = app.sessionsVersion
        server.reportSession(tv, state: "playing", positionMs: 0, audioIndex: 2)
        #expect(await eventually { app.sessionsVersion > version })
        await model.reload()
        #expect(model.waiting.isEmpty)
        #expect(model.others.first?.session.audioIndex == 2)

        // Subtitles the TV doesn't change: it didn't answer.
        let again = try #require(model.others.first)
        await model.send(.subtitles(3), to: again)
        #expect(await eventually { model.errors[tv] == "Bedroom TV didn't answer." })
        #expect(model.waiting.isEmpty)
    }

    @Test func playOnSendsItThereAndOpensTheRemoteOnThatDevice() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        server.addReceiver("den-tv", deviceId: "app-90", name: "Den TV", casts: ["item"])
        await model.reload()
        let den = try #require(model.devices.first)
        #expect(model.takers(.item).map(\.id) == ["app-90"] && model.takers(.live).isEmpty)

        await model.cast(.item(7, startMs: 5_000), to: den)
        let sent = try #require(server.castsReceived.last)
        #expect(sent.receiverId == "den-tv")
        #expect(String(decoding: sent.body, as: UTF8.self) == #"{"item_id":7,"start_ms":5000}"#)
        #expect(app.shell.toast?.text == "Playing on Den TV")
        #expect(model.presented && model.selected?.id == "app-90")

        // Refusals say why, in every app's words.
        server.refuseCasts(status: 429)
        await model.cast(.item(7), to: den)
        #expect(app.shell.toast?.text == "Too many at once: try again in a moment")
        #expect(app.shell.toast?.style == .error)
        server.refuseCasts(status: 409, code: "not_castable")
        await model.cast(.item(7), to: den)
        #expect(app.shell.toast?.text == "Den TV can't play that")
        server.refuseCasts(status: nil)
        server.removeReceiver("den-tv")
        await model.cast(.item(7), to: den)
        #expect(app.shell.toast?.text == "Den TV isn't on any more")
        // It goes from the list.
        #expect(await eventually { model.devices.isEmpty })
    }

    @Test func playOnSendsTheTracksThePagesPickersShow() async throws {
        let server = try await fixture()
        defer { server.stop() }
        var movie = FixtureServer.Playable(itemId: 7, fileId: 70, durationMs: 600_000, positionMs: 30_000)
        movie.audio = [MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true),
                       MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")]
        movie.subtitles = [MediaStream(index: 3, kind: .subtitle, codec: "subrip", language: "eng")]
        server.addPlayable(movie)
        let app = try await signedIn(server)
        let page = ItemModel(app: app, itemId: 7)
        await page.load()
        // Automatic audio leaves the device its own; subtitles off are sent off.
        #expect(page.castTarget == .item(7, startMs: 30_000, subtitleIndex: SessionTracks.off))
        await page.chooseAudio(2)
        await page.chooseSubtitle(3)
        #expect(page.castTarget == .item(7, startMs: 30_000, audioIndex: 2, subtitleIndex: 3))
        // A file with one audio track and no subtitles has no pickers: nothing to send.
        server.addPlayable(.init(itemId: 8, fileId: 80, durationMs: 600_000))
        let plain = ItemModel(app: app, itemId: 8)
        await plain.load()
        #expect(plain.castTarget == .item(8))
    }

    @Test func aQualityCommandIsAnsweredByTheReportThatSaysIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000, bitrate: 10_000_000, width: 1920,
                                 height: 1080))
        let app = try await signedIn(server)
        let model = RemoteModel(app: app, answerWait: .milliseconds(300))
        let tv = server.playElsewhere(7, deviceId: "app-90", device: "Bedroom TV",
                                      controls: ["play", "pause", "quality"])
        await model.reload()
        let entry = try #require(model.others.first)
        #expect(entry.session.qualities.map(\.id) == ["original", "auto", "8m", "4m", "2m"])
        #expect(entry.session.quality == nil)

        await model.send(.quality("4m"), to: entry)
        #expect(model.waiting == [tv])
        let version = app.sessionsVersion
        server.reportSession(tv, state: "playing", positionMs: 0, quality: "4m")
        #expect(await eventually { app.sessionsVersion > version })
        await model.reload()
        #expect(model.waiting.isEmpty)
        #expect(model.others.first?.session.quality == "4m")
        try await Task.sleep(for: .milliseconds(500))
        #expect(model.errors.isEmpty)
    }

    @Test func aQualityIsAnsweredByThePickItSays() {
        let received = Date(timeIntervalSince1970: 1_000)
        let film = ItemSummary(id: 7, libraryId: 1, kind: .movie, title: "Film")
        let before = RemoteModel.Entry(PlayingSession(sessionId: "a", item: film, quality: "auto"), received: received)
        let sent = RemoteModel.Sent(command: .quality, before: before, sentAt: received, target: nil, quality: "4m")
        #expect(!RemoteModel.answered(sent, before, at: received))
        let picked = RemoteModel.Entry(PlayingSession(sessionId: "a", item: film, quality: "4m"), received: received)
        #expect(RemoteModel.answered(sent, picked, at: received))
        // Another quality plays in a session beside it: the one sent to goes.
        #expect(RemoteModel.answered(sent, nil, at: received))
    }

    @Test func theRemoteSaysItsControlsInEveryAppsWords() {
        #expect(Words.castHint == "Send something here with Play on…")
        #expect(Words.remoteQuality("Living room TV") == "Quality on Living room TV")
        #expect(Words.remoteStopQuestion("Living room TV") == "Stop playing on Living room TV?")
        #expect(Words.remoteNextProgramme("Weather") == "Next: Weather")
        #expect(Words.remotePrevious("Living room TV", live: true) == "Previous channel on Living room TV")
        #expect(Words.remoteNext("Living room TV", live: true) == "Next channel on Living room TV")
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

    @Test func skipIntroAndSkipCreditsWhileThePlaceShownIsInOne() {
        let received = Date(timeIntervalSince1970: 1_000)
        let intro = Marker(fileId: 70, kind: .intro, startMs: 10_000, endMs: 70_000, source: "fingerprint")
        let credits = Marker(fileId: 70, kind: .credits, startMs: 1_250_000, endMs: 1_300_000, source: "chapter")
        func offer(at positionMs: Int, after seconds: TimeInterval = 0, state: PlayingState = .paused,
                   kind: SessionKind = .item, controls: [Control] = [.play, .pause, .seek, .skip],
                   markers: [Marker]? = nil) -> RemoteModel.SkipOffer? {
            let session = PlayingSession(sessionId: "a", kind: kind, state: state, positionMs: positionMs,
                                         durationMs: 1_300_000, controls: controls,
                                         markers: markers ?? [intro, credits])
            return RemoteModel.skipOffer(RemoteModel.Entry(session, received: received),
                                         at: received.addingTimeInterval(seconds))
        }
        // From the intro's start until a second before its end.
        #expect(offer(at: 9_999) == nil)
        #expect(offer(at: 10_000)?.marker == intro)
        #expect(offer(at: 68_999)?.marker == intro)
        #expect(offer(at: 69_000) == nil)
        // The credits the same way; nothing between them.
        #expect(offer(at: 600_000) == nil)
        #expect(offer(at: 1_250_000)?.marker == credits)
        #expect(offer(at: 1_298_999)?.marker == credits)
        #expect(offer(at: 1_299_000) == nil)
        // Where the progress shows it: the place moves on while it plays, not while it is paused.
        #expect(offer(at: 9_500, after: 0.4, state: .playing) == nil)
        #expect(offer(at: 9_500, after: 0.6, state: .playing)?.marker == intro)
        #expect(offer(at: 68_500, after: 0.6, state: .playing) == nil)
        #expect(offer(at: 9_500, after: 10) == nil)
        // Never on Live TV, nor where its player doesn't seek, nor for a kind this app doesn't know.
        #expect(offer(at: 20_000, kind: .live) == nil)
        #expect(offer(at: 20_000, controls: [.play, .pause, .skip]) == nil)
        #expect(offer(at: 20_000, markers: [Marker(fileId: 70, kind: MarkerKind(rawValue: "recap"), startMs: 0,
                                                   endMs: 60_000)]) == nil)
        #expect(offer(at: 20_000, markers: []) == nil)
    }

    @Test func aSkipSeeksWhereThePlayersOwnSkipLands() {
        let intro = RemoteModel.SkipOffer(Marker(fileId: 70, kind: .intro, startMs: 10_000, endMs: 70_000))
        #expect(intro.kind == .intro && intro.label == "Skip Intro")
        // Just before an intro ends, so its last moment plays.
        #expect(intro.targetMs == 69_250)
        #expect(intro.command == .seek(toMs: 69_250))
        // Never more than half a short intro before its end.
        #expect(RemoteModel.SkipOffer(Marker(fileId: 70, kind: .intro, startMs: 0, endMs: 1_000)).targetMs == 500)
        // The credits, to their end.
        let credits = RemoteModel.SkipOffer(Marker(fileId: 70, kind: .credits, startMs: 1_250_000,
                                                   endMs: 1_300_000))
        #expect(credits.kind == .credits && credits.label == "Skip Credits")
        #expect(credits.command == .seek(toMs: 1_300_000))
        // Credits said to run past the title's end go to its end: the server refuses a seek past it.
        let past = RemoteModel.SkipOffer(Marker(fileId: 70, kind: .credits, startMs: 1_250_000, endMs: 1_300_000),
                                         durationMs: 1_290_000)
        #expect(past.command == .seek(toMs: 1_290_000))
        // VoiceOver reads it with its device.
        #expect(Words.remoteSkip(.intro, device: "Living room TV") == "Skip intro on Living room TV")
        #expect(Words.remoteSkip(.credits, device: "Living room TV") == "Skip credits on Living room TV")
    }

    @Test func backAndForwardGoThirtySeconds() {
        #expect(RemoteModel.back == .skip(byMs: -30_000))
        #expect(RemoteModel.forward == .skip(byMs: 30_000))
        #expect(Words.remoteBack("Living room TV") == "Back 30 seconds on Living room TV")
        #expect(Words.remoteForward("Living room TV") == "Forward 30 seconds on Living room TV")
    }

    @Test func skipIntroSendsTheSeekToTheDeviceThatPlaysIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        var episode = FixtureServer.Playable(itemId: 7, fileId: 70, durationMs: 1_300_000)
        let intro = Marker(fileId: 70, kind: .intro, startMs: 10_000, endMs: 70_000, source: "fingerprint")
        // Another file's, another cut's, isn't the session's.
        episode.markers = [intro, Marker(fileId: 71, kind: .credits, startMs: 1_000_000, endMs: 1_100_000,
                                         source: "chapter")]
        server.addPlayable(episode)
        let app = try await signedIn(server)
        let model = RemoteModel(app: app)
        let tv = server.playElsewhere(7, deviceId: "app-90", device: "Bedroom TV", positionMs: 20_000)
        await model.reload()
        let entry = try #require(model.others.first)
        #expect(entry.session.markers == [intro])
        let offer = try #require(RemoteModel.skipOffer(entry, at: Date()))
        #expect(offer.marker == intro)

        await model.send(offer.command, to: entry)
        let sent = try #require(server.sessionCommands.last)
        #expect(sent.sessionId == tv)
        #expect(try PleyJSON.decoder.decode(PlayerCommand.self, from: sent.body) == .seek(toMs: 69_250))
        #expect(model.errors[tv] == nil)
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
