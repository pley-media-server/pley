import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

@Suite struct RemoteAPITests {
    private func api(_ server: FixtureServer) -> PleyAPI {
        let http = transport()
        return PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                         transport: http))
    }

    @Test func theSessionsAreListedAndTakeCommands() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, clientId: "tv", keyedUrls: true,
                                                               controls: [.play, .pause, .seek, .skip],
                                                               deviceName: "Living room"))
        let sessions = try await api.playingSessions()
        let session = try #require(sessions.first)
        #expect(sessions.count == 1)
        #expect(session.sessionId == info.sessionId)
        #expect(session.device == "Living room")
        #expect(session.item?.id == 7)
        #expect(session.controls == [.play, .pause, .seek, .skip])

        try await api.send(.seek(toMs: 90_000), to: info.sessionId)
        try await api.send(.skip(byMs: -10_000), to: info.sessionId)
        let bodies = server.sessionCommands.map { String(decoding: $0.body, as: UTF8.self) }
        #expect(bodies == [#"{"command":"seek","position_ms":90000}"#, #"{"command":"skip","offset_ms":-10000}"#])
        #expect(server.sessionCommands.allSatisfy { $0.sessionId == info.sessionId })
        #expect(server.requests("/api/sessions/\(info.sessionId)/command").allSatisfy { $0.token != nil })
    }

    @Test func refusalsSayWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, controls: [.pause]))
        // A command the player didn't list.
        do {
            try await api.send(.next, to: info.sessionId)
            Issue.record("next was taken")
        } catch let error as PleyError {
            #expect(error.notControllable)
            #expect(error.userMessage == "this player doesn't take next from a remote")
        }
        await #expect(throws: PleyError.api(status: 400, code: "bad_request", message: "position_ms out of range")) {
            try await api.send(.seek(toMs: 700_000), to: info.sessionId)
        }
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "no such session playing")) {
            try await api.send(.pause, to: "gone")
        }
        server.refuseCommands(status: 429, code: "too_many_requests")
        do {
            try await api.send(.pause, to: info.sessionId)
            Issue.record("the command was taken")
        } catch let error as PleyError {
            guard case .api(429, "too_many_requests", _) = error else { throw error }
            #expect(!error.notControllable)
        }
        server.refuseCommands(status: 403, code: "forbidden")
        await #expect(throws: PleyError.api(status: 403, code: "forbidden", message: "refused")) {
            try await api.send(.pause, to: info.sessionId)
        }
    }

    @Test func trackCommandsNameOneOfTheSessionsTracks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        var movie = FixtureServer.Playable(itemId: 7, fileId: 70, durationMs: 600_000)
        movie.audio = [MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true),
                       MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")]
        movie.subtitles = [MediaStream(index: 3, kind: .subtitle, codec: "subrip", language: "eng")]
        server.addPlayable(movie)
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, controls: [.play, .pause, .audio, .subtitles]))
        let listed = try #require(try await api.playingSessions().first)
        #expect(listed.tracks.audio.map(\.index) == [1, 2] && listed.tracks.subtitles.map(\.index) == [3])
        // What the play request started with, until the player says otherwise.
        #expect(listed.audioIndex == 1 && listed.subtitleIndex == SessionTracks.off)
        #expect(listed.deviceId.hasPrefix("app-") && listed.deviceKind == .other)

        try await api.send(.audio(2), to: info.sessionId)
        try await api.send(.subtitles(nil), to: info.sessionId)
        let bodies = server.sessionCommands.map { String(decoding: $0.body, as: UTF8.self) }
        #expect(bodies == [#"{"command":"audio","stream_index":2}"#, #"{"command":"subtitles","stream_index":-1}"#])
        await #expect(throws: PleyError.api(status: 400, code: "bad_request",
                                            message: "stream_index isn't one of its tracks")) {
            try await api.send(.audio(9), to: info.sessionId)
        }
        // The player's report says the tracks it plays: the session follows.
        try await api.reportProgress(itemId: 7, ProgressReport(positionMs: 1_000, sessionId: info.sessionId,
                                                               audioIndex: 2, subtitleIndex: 3))
        let after = try #require(try await api.playingSessions().first)
        #expect(after.audioIndex == 2 && after.subtitleIndex == 3)
    }

    @Test func aQualityCommandNamesOneOfTheSessionsQualities() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 600_000, bitrate: 10_000_000, width: 1920,
                                 height: 1080))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, controls: [.play, .pause, .quality]))
        let listed = try #require(try await api.playingSessions().first)
        // What its Quality menu offers for the file, as every app works it out; nothing picked until it says.
        #expect(listed.qualities.map(\.id) == ["original", "auto", "8m", "4m", "2m"])
        #expect(listed.qualities.map(\.label) == ["Original", "Automatic", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p"])
        #expect(listed.quality == nil)

        try await api.send(.quality("4m"), to: info.sessionId)
        #expect(server.sessionCommands.last.map { String(decoding: $0.body, as: UTF8.self) }
            == #"{"command":"quality","quality":"4m"}"#)
        await #expect(throws: PleyError.api(status: 400, code: "bad_request",
                                            message: "quality isn't one of its qualities")) {
            try await api.send(.quality("30m"), to: info.sessionId)
        }
        // The player's report says its pick: the session follows.
        try await api.reportProgress(itemId: 7, ProgressReport(positionMs: 1_000, sessionId: info.sessionId,
                                                               quality: "4m"))
        #expect(try await api.playingSessions().first?.quality == "4m")
    }

    @Test func devicesThatTakeCastsAreListedAndSentWhatToPlay() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addReceiver("den-tv", deviceId: "app-90", name: "Den TV", casts: ["item"])
        server.addReceiver("kitchen", deviceId: "app-91", name: "Kitchen tablet", kind: "tablet")
        let api = api(server)
        let devices = try await api.remoteDevices()
        #expect(devices == [RemoteDevice(receiverId: "den-tv", deviceId: "app-90", name: "Den TV", kind: .tv,
                                         casts: [.item]),
                            RemoteDevice(receiverId: "kitchen", deviceId: "app-91", name: "Kitchen tablet",
                                         kind: .tablet, casts: [.item, .live])])

        try await api.cast(.item(7, startMs: 61_000), to: "den-tv")
        try await api.cast(.channel(3), to: "kitchen")
        let sent = server.castsReceived
        #expect(sent.map(\.receiverId) == ["den-tv", "kitchen"])
        #expect(sent.map { String(decoding: $0.body, as: UTF8.self) }
            == [#"{"item_id":7,"start_ms":61000}"#, #"{"channel_id":3}"#])
        #expect(server.requests("/api/remote/devices/den-tv/cast").allSatisfy { $0.token != nil })
    }

    @Test func castsAreRefusedAsTheServerRefusesThem() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addReceiver("den-tv", deviceId: "app-90", name: "Den TV", casts: ["item"])
        let api = api(server)
        // A device gone, or never the profile's.
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "no such device")) {
            try await api.cast(.item(7), to: "gone")
        }
        // What it doesn't take.
        do {
            try await api.cast(.channel(3), to: "den-tv")
            Issue.record("the channel was taken")
        } catch let error as PleyError {
            guard case .api(409, "not_castable", _) = error else { throw error }
        }
        // An id no server gives isn't asked for at all.
        let asked = server.castsReceived.count
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "no such device")) {
            try await api.cast(.item(7), to: "../sessions")
        }
        #expect(server.castsReceived.count == asked)
        server.refuseCasts(status: 429)
        do {
            try await api.cast(.item(7), to: "den-tv")
            Issue.record("the cast was taken")
        } catch let error as PleyError {
            guard case .api(429, _, _) = error else { throw error }
        }
    }

    @Test func aSessionIsStoppedFromHereWithAMessage() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7))
        try await api.stopSession(info.sessionId, message: "  Bedtime  ")
        #expect(server.requests("/api/sessions/\(info.sessionId)").map(\.method) == ["DELETE"])
        #expect(try await api.playingSessions().isEmpty)
        // Its player hears it on its next report, with the message trimmed.
        await #expect(throws: PleyError.sessionStopped(message: "Bedtime")) {
            try await api.reportProgress(itemId: 7, ProgressReport(positionMs: 0, sessionId: info.sessionId))
        }
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "no such session")) {
            try await api.stopSession("gone")
        }
    }
}
