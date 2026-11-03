import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

@Suite struct PlaybackAPITests {
    private func api(_ server: FixtureServer, gate: ConnectionGate = ConnectionGate()) -> PleyAPI {
        let http = transport(gate)
        return PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                         transport: http))
    }

    @Test func aSessionStartsReportsAndEnds() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, positionMs: 1_500))
        let api = api(server)
        let item = try await api.item(7)
        #expect(item.userState.positionMs == 1_500)
        #expect(item.files.first?.id == 70)

        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, fileId: 70, startMs: 1_500,
                                                               clientId: "apple-test", keyedUrls: true))
        #expect(info.method == .direct)
        #expect(info.url.hasPrefix("/api/media/pley_mk_"))
        let sent = try JSONSerialization.jsonObject(with: try #require(server.playbackRequests.first)) as? [String: Any]
        #expect(sent?["keyed_urls"] as? Bool == true)
        #expect(sent?["client_id"] as? String == "apple-test")

        try await api.reportProgress(itemId: 7, ProgressReport(positionMs: 2_000, state: .playing,
                                                               sessionId: info.sessionId))
        #expect(try await api.playbackStatus(info.sessionId).method == .direct)
        try await api.stopPlayback(info.sessionId)
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "playback session expired")) {
            try await api.playbackStatus(info.sessionId)
        }
        #expect(server.requests("/api/playback/\(info.sessionId)").map(\.method) == ["GET", "DELETE", "GET"])
        #expect(server.requests("/api/playback").allSatisfy { $0.token != nil })
    }

    @Test func aStoppedSessionAnswers410() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7))
        server.stopSession(info.sessionId, message: "Server maintenance")
        await #expect(throws: PleyError.sessionStopped(message: "Server maintenance")) {
            try await api.reportProgress(itemId: 7, ProgressReport(positionMs: 0, sessionId: info.sessionId))
        }
    }

    @Test func theNetworkAndStreamsCalls() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        #expect(try await api.playbackNetwork(clientId: "apple-test").local)
        #expect(server.requests("/api/playback/network").first?.query["client_id"] == "apple-test")
        try await api.setStreams(itemId: 7, StreamsChoice(fileId: 70, subtitleIndex: 3))
        let body = try JSONSerialization.jsonObject(with: try #require(server.streamsChoices.first)) as? [String: Int]
        #expect(body == ["file_id": 70, "subtitle_index": 3])
        #expect(try await api.next(7) == nil)
    }

    @Test func onlyAKeyedMediaPathOnTheServerIsGivenToThePlayer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server, gate: ConnectionGate(resolver: NoResolver()))
        let url = try await api.mediaURL("/api/media/pley_mk_abc/master.m3u8")
        #expect(url.absoluteString == server.origin + "/api/media/pley_mk_abc/master.m3u8")
        for bad in ["/api/files/7/stream", "/api/media/", "/api/media//index.m3u8", "/api/media/k/../../items/1",
                    "//elsewhere.example/api/media/k/file.mp4", "http://elsewhere.example/api/media/k/file.mp4",
                    "/api/media/k/%2e%2e/x", "api/media/k/file.mp4"] {
            await #expect(throws: PleyError.self, "\(bad)") { try await api.mediaURL(bad) }
        }
    }

    @Test func plainHTTPMediaFollowsTheRule() async throws {
        // A server named by a name that resolves to the internet: refused before AVPlayer gets it.
        let http = transport(ConnectionGate(resolver: FakeResolver(answer: ["93.184.216.34"])))
        let api = PleyAPI(client: APIClient(origin: "http://media.example:7539", tokens: nil, transport: http))
        await #expect(throws: PleyError.unencrypted) { try await api.mediaURL("/api/media/pley_mk_a/file.mp4") }
        let home = PleyAPI(client: APIClient(origin: "http://nas:7539", tokens: nil,
                                             transport: transport(ConnectionGate(resolver: FakeResolver(answer: ["192.168.1.20"])))))
        #expect(try await home.mediaURL("/api/media/pley_mk_a/file.mp4").host() == "nas")
        // https goes as it is.
        let secure = PleyAPI(client: APIClient(origin: "https://media.example", tokens: nil,
                                               transport: transport(ConnectionGate(resolver: NoResolver()))))
        #expect(try await secure.mediaURL("/api/media/pley_mk_a/file.mp4").scheme == "https")
    }
}
