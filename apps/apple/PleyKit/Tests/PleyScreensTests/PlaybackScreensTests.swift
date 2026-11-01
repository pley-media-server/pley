import Foundation
import PleyAuth
import PleyCaps
import PleyCommon
import PleyData
import PleyModel
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// An engine that opens anything and never plays: the app's ownership of
/// playbacks is tested, not AVPlayer.
@MainActor
final class StillEngine: PlayerEngine {
    let events: AsyncStream<EngineEvent>
    private let continuation: AsyncStream<EngineEvent>.Continuation
    private(set) var loads: [URL] = []
    private(set) var closed = false

    init() {
        (events, continuation) = AsyncStream.makeStream(of: EngineEvent.self)
    }

    func load(url: URL, startMs: Int, play: Bool) { loads.append(url) }
    func play() {}
    func pause() {}
    func seek(toMs positionMs: Int) {}
    func sample() -> EngineSample { EngineSample() }
    func setMaximumResolution(width: Int, height: Int) {}

    func close() {
        closed = true
        continuation.finish()
    }
}

/// The recorded probe of an M3 Mac with an XDR screen, from the caps tests.
struct RecordedProbe: CapsProbe {
    func read(screen: ScreenFacts?) async -> ProbeInput {
        let file = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("PleyCapsTests/Probes/mac_m3_xdr.json")
        // swiftlint:disable:next force_try
        return try! PleyJSON.decoder.decode(ProbeInput.self, from: Data(contentsOf: file))
    }
}

/// A signed-in app on `server`, at Home.
@MainActor
private func signedIn(_ server: FixtureServer,
                      engines: @escaping @MainActor () -> any PlayerEngine = { StillEngine() }) async throws
    -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                  engines: engines)
    await app.start()
    try #require(app.route == .home)
    return app
}

private func item(_ id: Int, kind: String, title: String, positionMs: Int = 0, extra: String = "") -> String {
    """
    {"id": \(id), "library_id": 2, "kind": "\(kind)", "title": "\(title)", "sort_title": "\(title)",
     "added_at": 1761300000, "user_state": {"position_ms": \(positionMs), "watched": false, "play_count": 0}\(extra)}
    """
}

/// A show (50) whose on-deck episode (51) has two versions, the second with
/// two audio tracks and a subtitle, and a resume point at 12:34.
private func addShow(to server: FixtureServer) {
    server.answer(path: "/api/items/50", status: 200,
                  json: item(50, kind: "show", title: "Pley Test Show", extra: #", "year": 2024, "files": []"#))
    server.answer(path: "/api/items/50/on-deck", status: 200,
                  json: #"{"item": "# + item(51, kind: "episode", title: "Pilot") + "}")
    let episode = """
    , "index": 2, "parent_index": 1, "grandparent_title": "Pley Test Show", "duration_ms": 4500000,
    "summary": "It begins.",
    "files": [
      {"id": 510, "path": "Pilot.mp4", "size": 1000, "container": "mp4", "duration_ms": 4500000,
       "width": 1280, "height": 720, "video_codec": "h264",
       "streams": [{"index": 0, "kind": "video", "codec": "h264", "default": true, "width": 1280, "height": 720},
                   {"index": 1, "kind": "audio", "codec": "aac", "language": "eng", "channels": 2, "default": true}]},
      {"id": 511, "path": "Pilot.mkv", "size": 2000, "container": "mkv", "duration_ms": 4500000,
       "width": 1920, "height": 1080, "video_codec": "hevc", "selected_audio": 2, "audio_picked": true,
       "selected_subtitle": 3,
       "streams": [{"index": 0, "kind": "video", "codec": "hevc", "default": true, "width": 1920, "height": 1080,
                    "dynamic_range": "hdr10", "bit_depth": 10},
                   {"index": 1, "kind": "audio", "codec": "eac3", "language": "eng", "channels": 6, "default": true},
                   {"index": 2, "kind": "audio", "codec": "ac3", "language": "fre", "channels": 6},
                   {"index": 3, "kind": "subtitle", "codec": "subrip", "language": "eng", "text_based": true}]}
    ]
    """
    server.answer(path: "/api/items/51", status: 200,
                  json: item(51, kind: "episode", title: "Pilot", positionMs: 754_000, extra: episode))
}

@MainActor
@Suite struct ItemModelTests {
    @Test func aMoviePlaysItselfFromTheStart() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, title: "Arrival", durationMs: 6_960_000))
        let app = try await signedIn(server)
        app.showItem(7)
        #expect(app.route == .item(7))
        let model = ItemModel(app: app, itemId: 7)
        await model.load()
        guard case .loaded = model.state else {
            Issue.record("not loaded: \(model.state)")
            return
        }
        #expect(model.title == "Arrival")
        #expect(model.facts == "2020 · 1h 56m")
        #expect(model.episodeLine == nil)
        #expect(model.videoLine == "SD · H.264")
        #expect(model.resumeLabel == nil)
        #expect(model.versions.isEmpty)
        #expect(model.request() == PlayRequest(itemId: 7))
    }

    @Test func aShowPlaysItsOnDeckEpisodeWhereItWasLeft() async throws {
        let server = try await fixture()
        defer { server.stop() }
        addShow(to: server)
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 50)
        await model.load()
        guard case .loaded = model.state else {
            Issue.record("not loaded: \(model.state)")
            return
        }
        #expect(model.title == "Pley Test Show")
        #expect(model.playable?.id == 51)
        #expect(model.episodeLine == "S1 E2 · Pilot")
        #expect(model.resumeMs == 754_000)
        #expect(model.resumeLabel == "Resume from 12:34")
        #expect(model.request() == PlayRequest(itemId: 51))
        #expect(model.request(fromStart: true) == PlayRequest(itemId: 51, fromStart: true))
        #expect(model.versions.map(\.id) == [510, 511])
        #expect(model.versions.first?.label == "720p · H.264 · MP4 · 1000 B")
    }

    @Test func aVersionPickedIsAskedForWithItsTracks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        addShow(to: server)
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 51)
        await model.load()
        model.chooseVersion(511)
        #expect(model.file?.id == 511)
        #expect(model.request() == PlayRequest(itemId: 51, fileId: 511))
        #expect(model.audioTracks.map(\.label) == ["English (EAC3 5.1)", "French (AC3 5.1)"])
        #expect(model.audioIndex == 2)
        #expect(model.subtitleTracks.map(\.label) == ["English (SRT)"])
        #expect(model.subtitleIndex == 3)
        #expect(model.videoLine == "1080p · HEVC · HDR10 · AC3 5.1")
        model.chooseVersion(999)
        #expect(model.file?.id == 511)
    }

    @Test func audioAndSubtitleChoicesAreSavedForTheShow() async throws {
        let server = try await fixture()
        defer { server.stop() }
        addShow(to: server)
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 51)
        await model.load()
        model.chooseVersion(511)
        await model.chooseAudio(1)
        #expect(model.audioIndex == 1)
        await model.chooseSubtitle(nil)
        #expect(model.subtitleIndex == nil)
        await model.chooseAudio(42)
        let bodies = server.streamsChoices.map { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        #expect(bodies.count == 2)
        // Audio: with the subtitles as they were.
        #expect(bodies[0]?["file_id"] as? Int == 511)
        #expect(bodies[0]?["audio_index"] as? Int == 1)
        #expect(bodies[0]?["subtitle_index"] as? Int == 3)
        // Subtitles off: the audio is left out, so the one chosen stays.
        #expect(bodies[1]?["file_id"] as? Int == 511)
        #expect(bodies[1]?.keys.contains("audio_index") == false)
        #expect(bodies[1]?.keys.contains("subtitle_index") == false)
        #expect(server.requests("/api/items/51/streams").count == 2)
    }

    @Test func aChoiceThatCannotBeSavedIsUndoneAndSaysWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        addShow(to: server)
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 51)
        await model.load()
        model.chooseVersion(511)
        server.answer(path: "/api/items/51/streams", status: 500, json: #"{"code":"internal","message":"no"}"#)
        await model.chooseAudio(1)
        #expect(model.audioIndex == 2)
        #expect(model.notice != nil)
    }

    @Test func aShowWithNothingOnDeckHasNothingToPlay() async throws {
        let server = try await fixture()
        defer { server.stop() }
        addShow(to: server)
        server.answer(path: "/api/items/50/on-deck", status: 200, json: #"{"item": null}"#)
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 50)
        await model.load()
        #expect(model.playable == nil)
        #expect(model.request() == nil)
    }

    @Test func anItemThatIsGoneSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 404)
        await model.load()
        guard case .failed = model.state else {
            Issue.record("not failed: \(model.state)")
            return
        }
    }
}

@MainActor
@Suite struct AppPlaybackTests {
    private func playable(_ server: FixtureServer) {
        server.addPlayable(.init(itemId: 7, fileId: 70, title: "First"))
        server.addPlayable(.init(itemId: 8, fileId: 80, title: "Second"))
    }

    @Test func oneAtATimeTheOneBeforeEndsFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        playable(server)
        let app = try await signedIn(server)
        let first = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(app.player === first)
        try #require(await eventually { first.session != nil })
        let firstId = try #require(first.session?.id)

        let second = try #require(await app.play(PlayRequest(itemId: 8)))
        #expect(app.player === second)
        // The first session ended (stopped, then DELETE) before the second was asked for.
        #expect(!server.openSessions.contains(firstId))
        #expect(server.journal.contains { $0.method == "DELETE" && $0.path == "/api/playback/\(firstId)" })
        try #require(await eventually { second.session != nil })
        let bodies = server.playbackRequests.compactMap { try? PleyJSON.decoder.decode(PlaybackRequest.self, from: $0) }
        #expect(bodies.map(\.itemId) == [7, 8])
        #expect(bodies.allSatisfy { $0.clientId == "apple-test" && $0.keyedUrls == true && $0.caps != nil })
        let firstStopped = server.progressReports.compactMap {
            try? PleyJSON.decoder.decode(ProgressReport.self, from: $0.body)
        }.filter { $0.sessionId == firstId }
        #expect(firstStopped.last?.state == .stopped)

        await app.closePlayer()
        #expect(app.player == nil)
        #expect(server.openSessions.isEmpty)
    }

    @Test func closingAnotherPlaybackLeavesTheOneOpen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        playable(server)
        let app = try await signedIn(server)
        let first = try #require(await app.play(PlayRequest(itemId: 7)))
        let second = try #require(await app.play(PlayRequest(itemId: 8)))
        await app.closePlayer(first)
        #expect(app.player === second)
        await app.closePlayer(second)
        #expect(app.player == nil)
    }

    @Test func signingOutEndsThePlayback() async throws {
        let server = try await fixture()
        defer { server.stop() }
        playable(server)
        let app = try await signedIn(server)
        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        try #require(await eventually { controller.session != nil })
        await app.signOut()
        #expect(app.player == nil)
        #expect(server.openSessions.isEmpty)
    }

    @Test func signedOutNothingPlays() async throws {
        let app = app()
        await app.start()
        #expect(await app.play(PlayRequest(itemId: 7)) == nil)
        #expect(app.player == nil)
    }

    @Test func theCapsAreReadOnceSignedIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(await eventually { app.caps.snapshot != nil })
        #expect(app.caps.snapshot?.report.device?.form == .desktop)
    }
}
