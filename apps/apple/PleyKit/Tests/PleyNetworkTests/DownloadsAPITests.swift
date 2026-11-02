import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func api(_ server: FixtureServer) -> PleyAPI {
    let http = transport()
    return PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                     transport: http))
}

@Suite struct DownloadsAPITests {
    @Test func theServerSaysWhetherTheOriginalOrAConversionIsDownloaded() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(FixtureServer.Playable(itemId: 61, fileId: 610, media: Data(count: 300)))
        let api = api(server)
        let direct = try await api.convert(PlaybackRequest(itemId: 61, fileId: 610, caps: ClientCaps(containers: ["mp4"])))
        #expect(direct.state == .direct && direct.id == nil && direct.size == 300)
        #expect(direct.url == PleyAPI.fileStreamPath(610))

        server.setConversions(convert: true, step: 0.5)
        let started = try await api.convert(PlaybackRequest(itemId: 61, fileId: 610, maxBitrate: 4_000, maxHeight: 720))
        let id = try #require(started.id)
        #expect(started.state == .queued)
        let sent = try JSONSerialization.jsonObject(with: try #require(server.conversionRequests.last)) as? [String: Any]
        #expect(sent?["max_height"] as? Int == 720 && sent?["max_bitrate"] as? Int == 4_000)
        #expect(try await api.conversion(id).state == .running)
        let done = try await api.conversion(id)
        #expect(done.state == .done && done.size == 300 && PleyAPI.isDownloadPath(done.url))
        try await api.cancelConversion(id)
        #expect(server.cancelledConversions == [id])
        await #expect(throws: PleyError.self) { try await api.conversion(id) }
    }

    @Test func onlyAFileStreamOrAConversionsFileIsADownloadPath() {
        #expect(PleyAPI.isDownloadPath("/api/files/12/stream"))
        #expect(PleyAPI.isDownloadPath("/api/conversions/0123abc/file"))
        #expect(!PleyAPI.isDownloadPath("/api/files/x/stream"))
        #expect(!PleyAPI.isDownloadPath("/api/files/12/stream?token=1"))
        #expect(!PleyAPI.isDownloadPath("/api/conversions/../file"))
        #expect(!PleyAPI.isDownloadPath("https://elsewhere.example/api/files/12/stream"))
        #expect(!PleyAPI.isDownloadPath("/api/media/pley_mk_1/file.mp4"))
    }

    @Test func aSubtitleTrackSaysWhetherItIsWhole() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.serveFile(path: "/api/files/610/subtitles/2.vtt", type: "text/vtt", body: Data("WEBVTT\n".utf8),
                         headers: ["X-Pley-Complete": "0"])
        server.serveFile(path: "/api/files/610/subtitles/3.vtt", type: "text/vtt", body: Data("WEBVTT\n".utf8))
        let api = api(server)
        #expect(try await api.subtitleText(fileId: 610, index: 2).complete == false)
        #expect(try await api.subtitleText(fileId: 610, index: 3).complete == true)
    }
}
