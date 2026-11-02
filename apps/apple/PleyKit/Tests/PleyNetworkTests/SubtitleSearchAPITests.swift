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

/// A movie the fixture plays, file 510.
private func movie(_ server: FixtureServer) {
    server.addPlayable(FixtureServer.Playable(itemId: 51, fileId: 510, title: "Subtitled", durationMs: 90_000))
}

@Suite struct SubtitleSearchAPITests {
    @Test func searchAsksForTheFileAndTheLanguageAndReadsTheResults() async throws {
        let server = try await fixture()
        defer { server.stop() }
        movie(server)
        server.setSubtitleResults([
            SubtitleResult(id: "opensubtitles:1:en", language: "en", name: "Subtitled.2020.1080p", hearingImpaired: true,
                           downloads: 1234, matched: "hash"),
            SubtitleResult(id: "opensubtitles:2:en", language: "en", name: "Subtitled.2020.720p"),
        ])
        let api = api(server)
        let results = try await api.searchSubtitles(itemId: 51, fileId: 510, language: "fr")
        #expect(results.map(\.id) == ["opensubtitles:1:en", "opensubtitles:2:en"])
        #expect(results[0].madeForFile && results[0].hearingImpaired && results[0].downloads == 1234)
        #expect(!results[1].madeForFile)
        #expect(server.subtitleSearches.last == ["file_id": "510", "language": "fr"])
        // Without a language, the server picks the profile's.
        _ = try await api.searchSubtitles(itemId: 51, fileId: 510)
        #expect(server.subtitleSearches.last == ["file_id": "510"])
    }

    @Test func aServerWithoutSearchSaysSoAndOneThatFailedSaysWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        movie(server)
        let api = api(server)
        server.setSubtitleSearch(unavailable: false)
        await #expect(throws: PleyError.notAvailable(message: "Subtitle search isn't set up", setup: false)) {
            try await api.searchSubtitles(itemId: 51, fileId: 510)
        }
        server.setSubtitleSearch(unavailable: true)
        await #expect(throws: PleyError.notAvailable(message: "OpenSubtitles refused the server's key", setup: true)) {
            try await api.searchSubtitles(itemId: 51, fileId: 510)
        }
    }

    @Test func addingAResultGivesItsTextTrack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        movie(server)
        server.setSubtitleResults([SubtitleResult(id: "opensubtitles:1:en", language: "en", name: "Release")])
        let api = api(server)
        let track = try await api.addSubtitle(itemId: 51, result: "opensubtitles:1:en", fileId: 510)
        #expect(track.index == 9000 && track.added && track.textBased && track.external)
        #expect(track.url == "/api/files/510/subtitles/9000.vtt")
        let body = try #require(server.subtitleAdds.last)
        let sent = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(sent["result"] as? String == "opensubtitles:1:en" && sent["file_id"] as? Int == 510)
        // The item's file lists it from now on.
        let detail = try await api.item(51)
        #expect(detail.files.first?.streams(.subtitle).first { $0.index == 9000 }?.added == true)
    }

    @Test func aDayWithoutDownloadsLeftSaysWhenTheyRenew() async throws {
        let server = try await fixture()
        defer { server.stop() }
        movie(server)
        server.setSubtitleResults([SubtitleResult(id: "opensubtitles:1:en", language: "en", name: "Release")])
        server.refuseSubtitleAdd(status: 429)
        let api = api(server)
        do {
            _ = try await api.addSubtitle(itemId: 51, result: "opensubtitles:1:en", fileId: 510)
            Issue.record("added")
        } catch let error as PleyError {
            guard case .api(429, _, let message) = error else {
                Issue.record("\(error)")
                return
            }
            #expect(message?.contains("renew") == true)
        }
    }
}
