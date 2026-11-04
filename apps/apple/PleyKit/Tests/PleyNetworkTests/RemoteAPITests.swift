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
