import Foundation
import PleyCommon
import PleyData
@testable import PleyDownloads
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private let agent = "pleyApple/9.9.9 (iOS 18.1; iPhone)"

/// Tokens from the fixture, each lasting `lasting` seconds from now; counts what was asked.
private final class FixtureTokens: DownloadTokens, @unchecked Sendable {
    private let server: FixtureServer
    private let lock = NSLock()
    private var asked = 0
    private var proved = 0
    var expiresIn: TimeInterval = 3600

    init(server: FixtureServer) { self.server = server }

    var count: Int { lock.withLock { asked } }
    /// The proofs asked for before a piece went on from its resume data.
    var proofs: Int { lock.withLock { proved } }

    func accessToken(lasting: TimeInterval) async throws -> (token: String, expires: Date) {
        lock.withLock { asked += 1 }
        return (server.issueTokens().accessToken, Date().addingTimeInterval(lock.withLock { expiresIn }))
    }

    func proveServer() async throws {
        lock.withLock { proved += 1 }
    }
}

/// Bytes of a made-up file: what matters is that each arrives in its place.
private func media(_ count: Int) -> Data {
    Data((0..<count).map { UInt8(truncatingIfNeeded: $0 &* 31 &+ $0 / 251) })
}

@MainActor
private final class Harness {
    let server: FixtureServer
    let root: URL
    let settings: AppSettings
    let gate = NetworkGate()
    let networks = NetworkChanges()
    let transfer: PieceTransfer
    let downloads: Downloads
    let tokens: FixtureTokens
    let api: PleyAPI

    init(server: FixtureServer, pieceBytes: Int64 = 1_000, free: Int64 = 1 << 40, transferGate: ConnectionGate? = nil,
         settings defaults: UserDefaults? = nil) {
        self.server = server
        root = FileManager.default.temporaryDirectory.appendingPathComponent("pley-downloads-\(UUID().uuidString)")
        settings = AppSettings(defaults: defaults ?? UserDefaults(suiteName: "pley-downloads-\(UUID().uuidString)")!)
        let http = HTTPTransport(userAgent: agent)
        tokens = FixtureTokens(server: server)
        let fake = FixtureAccess(server: server)
        api = PleyAPI(client: APIClient(origin: server.origin, tokens: fake, transport: http))
        transfer = PieceTransfer(configuration: .ephemeral, folder: root.appendingPathComponent("pieces"),
                                 gate: transferGate ?? ConnectionGate())
        var timing = DownloadRunner.Timing()
        timing.pieceBytes = pieceBytes
        timing.pollSeconds = 0.05
        timing.backoffSeconds = 0.05
        timing.spaceMargin = 0
        downloads = Downloads(root: root, transfer: transfer, gate: gate, settings: settings, timing: timing,
                              freeSpace: { _ in free }, networks: networks)
        downloads.signedIn(DownloadScope(origin: server.origin, profileId: 2, api: api, tokens: tokens,
                                         caps: { ClientCaps(containers: ["mp4", "mov"]) }))
    }

    func entry(_ itemId: Int) -> DownloadEntry? { downloads.entries.first { $0.itemId == itemId } }

    func waitFor(_ itemId: Int, _ state: DownloadState, seconds: Double = 10) async -> Bool {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if entry(itemId)?.state == state { return true }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return false
    }

    func close() {
        transfer.invalidate()
        try? FileManager.default.removeItem(at: root)
        server.stop()
    }
}

/// The API's own token, fixed.
private actor FixtureAccess: TokenProvider {
    nonisolated let issuer: String
    private let access: String
    init(server: FixtureServer) {
        issuer = server.origin
        access = server.issueTokens().accessToken
    }

    func accessToken() async throws -> String { access }
    func refresh(after refused: String) async throws {}
    func refusedAfterRefresh(_ token: String) async -> Bool { false }
    func connectionLeaked() async {}
}

private func started() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

private func movie(_ server: FixtureServer, bytes: Int = 4_500, container: String = "mp4") -> Data {
    let data = media(bytes)
    var playable = FixtureServer.Playable(itemId: 61, fileId: 610, title: "Direct H264", container: container,
                                          durationMs: 90_000, positionMs: 30_000, media: data)
    playable.subtitles = [MediaStream(index: 2, kind: .subtitle, codec: "subrip", language: "eng", default: false)]
    playable.subtitles[0].textBased = true
    server.addPlayable(playable)
    server.serveFile(path: "/api/files/610/subtitles/2.vtt", type: "text/vtt",
                     body: Data("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n".utf8),
                     headers: ["X-Pley-Complete": "0"])
    return data
}

@Suite struct DownloadPlannerTests {
    private func file(_ id: Int, height: Int, size: Int, codec: String = "h264") -> MediaFile {
        var file = MediaFile(id: id, path: "f\(id).mp4", container: "mp4")
        file.height = height
        file.width = height * 16 / 9
        file.size = size
        file.streams = [MediaStream(index: 0, kind: .video, codec: codec, default: true)]
        file.streams[0].height = height
        file.streams[0].width = height * 16 / 9
        return file
    }

    @Test func theSharpestFileUpTo1080pIsPickedElseTheSmallest() {
        let files = [file(1, height: 2160, size: 9_000), file(2, height: 1080, size: 4_000), file(3, height: 720, size: 2_000)]
        #expect(DownloadPlanner.pickFile(files, caps: nil)?.id == 2)
        let tall = [file(1, height: 2160, size: 9_000), file(4, height: 1440, size: 6_000)]
        #expect(DownloadPlanner.pickFile(tall, caps: nil)?.id == 4)
        #expect(DownloadPlanner.pickFile([file(9, height: 2160, size: 1)], caps: nil)?.id == 9)
    }

    @Test func piecesCoverTheFileIn256MiB() {
        let mib: Int64 = 1 << 20
        #expect(DownloadPlanner.piece(have: 0, size: 600 * mib) == 0...(256 * mib - 1))
        #expect(DownloadPlanner.piece(have: 512 * mib, size: 600 * mib) == (512 * mib)...(600 * mib - 1))
        #expect(DownloadPlanner.piece(have: 600 * mib, size: 600 * mib) == nil)
        // A size not known yet: a whole piece is asked for, the answer says the rest.
        #expect(DownloadPlanner.piece(have: 0, size: 0) == 0...(256 * mib - 1))
    }

    @Test func anAnswerIsAppendedOnlyWhenItStartsWhereAsked() {
        #expect(DownloadPlanner.check(status: 206, contentRange: "bytes 100-199/1000", start: 100, size: 1000)
            == .append(total: 1000))
        #expect(DownloadPlanner.check(status: 206, contentRange: "bytes 0-99/1000", start: 100, size: 1000) == .restart)
        #expect(DownloadPlanner.check(status: 206, contentRange: "bytes 100-199/2000", start: 100, size: 1000) == .restart)
        #expect(DownloadPlanner.check(status: 206, contentRange: nil, start: 100, size: 1000) == .restart)
        #expect(DownloadPlanner.check(status: 200, contentRange: nil, start: 0, size: 1000) == .whole)
        #expect(DownloadPlanner.check(status: 200, contentRange: nil, start: 100, size: 1000) == .restart)
        #expect(DownloadPlanner.check(status: 416, contentRange: "bytes */1000", start: 1000, size: 1000) == .complete)
        #expect(DownloadPlanner.check(status: 404, contentRange: nil, start: 0, size: 1000) == .refused(404))
    }

    @Test func aFailedPieceResumesOnlyWhileItsTokenLasts() {
        let now = Date()
        #expect(DownloadPlanner.resumes(tokenExpires: now.addingTimeInterval(50 * 60), now: now))
        #expect(DownloadPlanner.resumes(tokenExpires: now.addingTimeInterval(10 * 60), now: now))
        #expect(!DownloadPlanner.resumes(tokenExpires: now.addingTimeInterval(9 * 60), now: now))
    }

    @Test func playedPast90PercentIsWatchedFromTheStart() {
        #expect(DownloadPlanner.played(positionMs: 50_000, durationMs: 100_000) == (50_000, false))
        #expect(DownloadPlanner.played(positionMs: 95_000, durationMs: 100_000) == (0, true))
    }

    @Test func theNextEpisodeIsTheShowsNextDownloadedWhole() {
        func episode(_ id: Int, _ season: Int, _ number: Int, _ state: DownloadState = .done) -> DownloadRecord {
            DownloadRecord(origin: "o", profileId: 2, itemId: id, fileId: id, kind: .episode, title: "E\(id)", showId: 7,
                           seasonIndex: season, episodeIndex: number, state: state, addedAt: Date())
        }
        let mine = [episode(1, 1, 1), episode(2, 1, 2, .downloading), episode(3, 1, 3), episode(4, 2, 1)]
        #expect(DownloadPlanner.nextEpisode(mine, after: mine[0])?.itemId == 3)
        #expect(DownloadPlanner.nextEpisode(mine, after: mine[2])?.itemId == 4)
        #expect(DownloadPlanner.nextEpisode(mine, after: mine[3]) == nil)
    }
}

@Suite(.serialized) @MainActor struct DownloadsTests {
    @Test func theOriginalComesInRangedPiecesWithItsSubtitlesAndPicture() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        let original = movie(server)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        let local = try #require(h.downloads.local?.file(610))
        #expect(try Data(contentsOf: local.url) == original)
        #expect(local.url.lastPathComponent == "media.mp4")
        #expect(!local.converted)
        let vtt = try #require(local.subtitles[2])
        #expect(try String(contentsOf: vtt, encoding: .utf8).contains("Hello"))
        // Five pieces of 1,000 bytes, the last shorter, each with a token of its own.
        #expect(server.downloadRanges == ["bytes=0-999", "bytes=1000-1999", "bytes=2000-2999", "bytes=3000-3999",
                                          "bytes=4000-4499"])
        #expect(h.tokens.count == 5)
        let entry = try #require(h.entry(61))
        #expect(entry.sizeBytes == 4_500 && entry.downloadedBytes == 4_500)
        // The index is written whole: no temporary file is left.
        let names = try FileManager.default.contentsOfDirectory(atPath: h.root.path)
        #expect(names.contains("index.json") && !names.contains("index.json.tmp"))
        // The details are saved for playing without the server.
        let detail = await h.downloads.local?.detail(61)
        #expect(detail?.title == "Direct H264")
        // The subtitles came partial: they are asked for again.
        let asked = server.journal.filter { $0.path == "/api/files/610/subtitles/2.vtt" }.count
        #expect(asked >= 2)
    }

    @Test func anotherNetworkStopsThePieceUntilTheServerProvesItself() async throws {
        let server = try await started()
        let h = Harness(server: server, pieceBytes: 3_000)
        defer { h.close() }
        let original = movie(server)
        server.delay(path: "/api/files/610/stream", ms: 400)
        try await h.downloads.download([61])
        let end = Date().addingTimeInterval(5)
        while server.downloadRanges.isEmpty && Date() < end { try await Task.sleep(for: .milliseconds(10)) }
        h.networks.changed()
        #expect(await h.waitFor(61, .done))
        #expect(try Data(contentsOf: try #require(h.downloads.local?.file(610)).url) == original)
        // The piece in flight stopped: it went on from its resume data after a
        // proof, or was asked for again with a fresh token (whose proof the
        // tokens make), never on by itself.
        #expect(server.downloadRanges.count == 3)
        #expect(h.tokens.count + h.tokens.proofs == 3)
    }

    @Test func aConvertedDownloadWaitsForTheServerThenFetchesItsFile() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server, container: "mkv")
        let made = media(2_300)
        server.setConversions(convert: true, step: 0.25, made: made)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        let local = try #require(h.downloads.local?.file(610))
        #expect(local.converted && local.url.lastPathComponent == "media.mp4")
        #expect(try Data(contentsOf: local.url) == made)
        let body = try #require(server.conversionRequests.first)
        let sent = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(sent["item_id"] as? Int == 61 && sent["file_id"] as? Int == 610)
        #expect((sent["caps"] as? [String: Any]) != nil)
    }

    @Test func aSmallerQualityAsksTheServerForIt() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server)
        h.settings.downloadQuality = .hd720
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        let first = try #require(server.conversionRequests.first)
        let sent = try #require(try JSONSerialization.jsonObject(with: first) as? [String: Any])
        #expect(sent["max_height"] as? Int == 720 && sent["max_bitrate"] as? Int == 4_000)
    }

    @Test func withoutRoomTheDownloadFailsAndSaysWhy() async throws {
        let server = try await started()
        let h = Harness(server: server, free: 100)
        defer { h.close() }
        _ = movie(server)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .failed))
        #expect(h.entry(61)?.error == "Not enough space on this device")
        // Tried again with room, it goes on.
    }

    @Test func withoutANetworkItWaitsForOneThenGoesOn() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server)
        h.gate.set(.init(satisfied: true, expensive: true))
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .waiting))
        #expect(h.entry(61)?.serverAway == false)
        // Mobile data allowed: it goes.
        h.settings.downloadOnMobileData = true
        h.gate.set(.init(satisfied: true, expensive: false))
        #expect(await h.waitFor(61, .done))
    }

    @Test func aPlainHttpAnswerFromAnAddressThatIsntPrivateIsThrownAway() async throws {
        let server = try await started()
        let h = Harness(server: server, transferGate: ConnectionGate(isPrivate: { _ in false }))
        defer { h.close() }
        _ = movie(server)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .failed))
        #expect(h.entry(61)?.error?.contains("home network") == true)
        #expect(h.downloads.local?.file(610) == nil)
    }

    @Test func aServerErrorFailsTheDownloadAndTryAgainGoesOn() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        let original = movie(server)
        server.setDownloadStatus(404)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .failed))
        #expect(h.entry(61)?.error == "This file is no longer on the server")
        server.setDownloadStatus(nil)
        h.downloads.retry(61)
        #expect(await h.waitFor(61, .done))
        let file = try #require(h.downloads.local?.file(610))
        #expect(try Data(contentsOf: file.url) == original)
    }

    @Test func deletingAConversionStopsItAndDeleteAllEmptiesEverything() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server, container: "mkv")
        server.setConversions(convert: true, step: 0)
        try await h.downloads.download([61])
        let end = Date().addingTimeInterval(5)
        while h.entry(61)?.converting == nil, Date() < end { try await Task.sleep(for: .milliseconds(20)) }
        #expect(h.entry(61)?.converting == 0)
        h.downloads.remove(61)
        #expect(h.entry(61) == nil)
        let deadline = Date().addingTimeInterval(5)
        while server.cancelledConversions.isEmpty, Date() < deadline { try await Task.sleep(for: .milliseconds(20)) }
        #expect(server.cancelledConversions.count == 1)

        server.setConversions(convert: false)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        h.downloads.removeAll()
        #expect(h.downloads.entries.isEmpty)
        let left = try FileManager.default.contentsOfDirectory(atPath: h.root.path).filter { $0 != "index.json" }
        #expect(left.isEmpty)
    }

    @Test func playedHereTheDownloadKeepsWhereItWasLeft() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        #expect(h.entry(61)?.positionMs == 30_000)
        h.downloads.local?.played(61, positionMs: 45_000, durationMs: 90_000)
        #expect(h.entry(61)?.positionMs == 45_000)
        #expect(await h.downloads.local?.detail(61)?.userState.positionMs == 45_000)
        h.downloads.local?.played(61, positionMs: 85_000, durationMs: 90_000)
        #expect(h.entry(61)?.watched == true && h.entry(61)?.positionMs == 0)
    }

    @Test func aPieceThatEndsWithNoOneSignedInGoesOnOnceTheyAre() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        let original = movie(server)
        server.delay(path: PleyAPI.fileStreamPath(610), ms: 300)
        try await h.downloads.download([61])
        for _ in 0..<500 where server.downloadRanges.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }
        // Woken in the background with no window: no one is signed in yet.
        let scope = DownloadScope(origin: server.origin, profileId: 2, api: h.api, tokens: h.tokens,
                                  caps: { ClientCaps(containers: ["mp4", "mov"]) })
        h.downloads.signedIn(nil)
        let key = try #require(h.downloads.store.all.first?.key)
        for _ in 0..<500 where h.downloads.store.get(key)?.state != .queued {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(h.downloads.store.get(key)?.state == .queued)
        #expect(server.downloadRanges == ["bytes=0-999"])
        server.delay(path: PleyAPI.fileStreamPath(610), ms: 0)
        h.downloads.signedIn(scope)
        #expect(await h.waitFor(61, .done))
        #expect(try Data(contentsOf: try #require(h.downloads.local?.file(610)).url) == original)
        #expect(server.downloadRanges.prefix(2) == ["bytes=0-999", "bytes=1000-1999"])
    }

    @Test func theSystemIsToldTheEventsAreHandledWhicheverCameFirst() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        // The session delivered all before the app delegate handed the handler over.
        h.transfer.urlSessionDidFinishEvents(forBackgroundURLSession: session)
        try await Task.sleep(for: .milliseconds(100))
        var told = 0
        h.downloads.handleBackgroundEvents { told += 1 }
        #expect(told == 1)
        // The usual order: the handler first, then the events.
        h.downloads.handleBackgroundEvents { told += 1 }
        #expect(told == 1)
        h.transfer.urlSessionDidFinishEvents(forBackgroundURLSession: session)
        for _ in 0..<500 where told < 2 {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(told == 2)
    }

    @Test func anotherProfilesDownloadsArentListedNorRun() async throws {
        let server = try await started()
        let h = Harness(server: server)
        defer { h.close() }
        _ = movie(server)
        try await h.downloads.download([61])
        #expect(await h.waitFor(61, .done))
        h.downloads.signedIn(DownloadScope(origin: server.origin, profileId: 3, api: h.api, tokens: h.tokens,
                                           caps: { ClientCaps() }))
        #expect(h.downloads.entries.isEmpty)
        #expect(h.downloads.local?.file(610) == nil)
        h.downloads.forgetServer(server.origin)
        h.downloads.signedIn(DownloadScope(origin: server.origin, profileId: 2, api: h.api, tokens: h.tokens,
                                           caps: { ClientCaps() }))
        #expect(h.downloads.entries.isEmpty)
    }
}
