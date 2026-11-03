import Foundation
import PleyModel
import PleyNetwork
@testable import PleyScreens
import PleyTesting
import Testing

/// The search's API on `server`, signed in as the fixture's profile.
@MainActor
private func model(_ server: FixtureServer, language: String? = "eng", admin: Bool = false,
                   added: @escaping @MainActor (SubtitleTrack) -> Void = { _ in }) -> SubtitleSearchModel {
    let tokens = server.issueTokens()
    let http = HTTPTransport(userAgent: "pleyApple/9.9.9 (macOS 15.2; Mac)")
    let api = PleyAPI(client: APIClient(origin: server.origin, tokens: Tokens(token: tokens.accessToken,
                                                                                 issuer: server.origin),
                                        transport: http))
    return SubtitleSearchModel(itemId: 51, fileId: 510, profileLanguage: language, admin: admin, api: { api },
                               onAdded: added)
}

private struct Tokens: TokenProvider {
    let token: String
    let issuer: String
    func accessToken() async throws -> String { token }
    func refresh(after refused: String) async throws {}
    func refusedAfterRefresh(_ token: String) async -> Bool { true }
    func connectionLeaked() async {}
}

@MainActor
private func movie() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    server.addPlayable(FixtureServer.Playable(itemId: 51, fileId: 510, title: "Subtitled", durationMs: 90_000))
    return server
}

@Suite("Find Subtitles…")
@MainActor
struct SubtitleSearchTests {
    @Test func resultsReadTheirLanguageWhatTheyAreAndTheirDownloads() async throws {
        let server = try await movie()
        defer { server.stop() }
        server.setSubtitleResults([
            SubtitleResult(id: "a", language: "en", name: "Subtitled.2020.1080p", hearingImpaired: true, forced: true,
                           downloads: 48211, matched: "hash"),
            SubtitleResult(id: "b", language: "pt-BR", name: "", downloads: 1),
        ])
        let search = model(server)
        search.search()
        #expect(await eventually { if case .found = search.state { true } else { false } })
        guard case .found(let rows) = search.state else { return }
        #expect(rows[0].title == "Subtitled.2020.1080p")
        #expect(rows[0].detail == "English · Hearing impaired · Forced · Made for this file")
        #expect(rows[0].downloads == "48,211 downloads")
        #expect(rows[0].spokenLabel
            == "Subtitled.2020.1080p, English · Hearing impaired · Forced · Made for this file, 48,211 downloads")
        #expect(rows[1].title == "Portuguese (BR)" && rows[1].downloads == "1 download")
        // The profile's language, which the server picks: none is sent.
        #expect(server.subtitleSearches.last == ["file_id": "510"])
    }

    @Test func nothingFoundNamesTheLanguageAndAnotherOneSearchesAgain() async throws {
        let server = try await movie()
        defer { server.stop() }
        let search = model(server, language: nil)
        #expect(search.languageName == "English")
        search.search()
        #expect(await eventually { search.state == .none("No subtitles found in English") })
        #expect(server.subtitleSearches.last == ["file_id": "510"])
        search.choose(language: "fre")
        #expect(search.languageName == "French")
        #expect(await eventually { search.state == .none("No subtitles found in French") })
        #expect(server.subtitleSearches.last == ["file_id": "510", "language": "fre"])
    }

    @Test func aServerWithoutSearchSaysSoAndTellsAnAdministratorWhere() async throws {
        let server = try await movie()
        defer { server.stop() }
        server.setSubtitleSearch(unavailable: false)
        let viewer = model(server)
        viewer.search()
        #expect(await eventually { viewer.state == .notSetUp(SubtitleSearchModel.notSetUp, hint: nil) })
        let admin = model(server, admin: true)
        admin.search()
        #expect(await eventually {
            admin.state == .notSetUp("Subtitle search isn't set up on this server",
                                     hint: "Set it up in Pley on the web, under Settings → Server.")
        })
        // Set up, but failing: the server's words, without Try again.
        server.setSubtitleSearch(unavailable: true)
        viewer.search()
        #expect(await eventually {
            viewer.state == .failed("OpenSubtitles refused the server's key.", canRetry: false)
        })
    }

    @Test func choosingOneAddsItAndHandsItsTrackToThePlayer() async throws {
        let server = try await movie()
        defer { server.stop() }
        server.setSubtitleResults([SubtitleResult(id: "a", language: "en", name: "Release")])
        var tracks: [SubtitleTrack] = []
        let search = model(server) { tracks.append($0) }
        search.search()
        #expect(await eventually { if case .found = search.state { true } else { false } })
        guard case .found(let rows) = search.state else { return }
        search.add(rows[0])
        #expect(search.adding == "a")
        #expect(await eventually { search.added })
        #expect(search.adding == nil)
        #expect(tracks.map(\.index) == [9000])
    }

    @Test func aDayWithoutDownloadsLeftSaysWhenTheyRenew() async throws {
        let server = try await movie()
        defer { server.stop() }
        server.setSubtitleResults([SubtitleResult(id: "a", language: "en", name: "Release")])
        server.refuseSubtitleAdd(status: 429)
        let search = model(server)
        search.search()
        #expect(await eventually { if case .found = search.state { true } else { false } })
        guard case .found(let rows) = search.state else { return }
        search.add(rows[0])
        #expect(await eventually { search.addError != nil })
        #expect(search.addError == "Your downloads for today are used up. They renew in 3 hours.")
        #expect(!search.added)
    }
}
