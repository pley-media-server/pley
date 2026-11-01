#if os(macOS)
import Foundation
import PleyCaps
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// AVPlayer itself, playing an MP4 made for the test from the fixture
/// server's keyed media address. The Mac only: the simulators' AVPlayer
/// decodes too, but slowly and unevenly on shared machines.
@Suite("AVPlayer plays a keyed file", .serialized)
@MainActor
struct AVPlayerEngineTests {
    @Test(.timeLimit(.minutes(1)))
    func itPlaysFromTheStartPositionWithFrames() async throws {
        let server = FixtureServer()
        try await server.start()
        defer { server.stop() }
        let media = try await MediaMaker.mp4(seconds: 6)
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 6_000, media: media))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, keyedUrls: true))
        let url = try await api.mediaURL(info.url)

        let engine = AVPlayerEngine()
        defer { engine.close() }
        engine.load(url: url, startMs: 2_000, play: true)
        var events = engine.events.makeAsyncIterator()
        #expect(await events.next() == .ready)
        let start = engine.sample()
        #expect(start.ready)
        #expect(abs(start.positionMs - 2_000) < 100)
        try await until("frames shown") {
            let sample = engine.sample()
            return sample.positionMs > 2_500 && (sample.videoFrames ?? 0) > 0
        }
        let sample = engine.sample()
        #expect(sample.durationMs == 6_000)
        #expect(sample.presentationWidth == 320)
        engine.pause()
        // The media went without a token: the key is the session's.
        let gets = server.journal.filter { $0.path.hasPrefix("/api/media/") }
        #expect(!gets.isEmpty)
        #expect(gets.allSatisfy { $0.headers["authorization"] == nil })
    }

    /// AVKit's Play pressed while an item loaded paused (after a quality
    /// change while paused) is told once the item is ready.
    @Test(.timeLimit(.minutes(1)))
    func aPlayPressedWhileLoadingIsToldWhenReady() async throws {
        let server = FixtureServer()
        try await server.start()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 6_000, media: try await MediaMaker.mp4(seconds: 6)))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, keyedUrls: true))
        let url = try await api.mediaURL(info.url)

        let engine = AVPlayerEngine()
        defer { engine.close() }
        // The last item played: the player's rate is still set.
        engine.player.play()
        engine.load(url: url, startMs: 1_000, play: false)
        #expect(engine.player.rate == 0)
        engine.player.play()
        var events = engine.events.makeAsyncIterator()
        #expect(await events.next() == .ready)
        #expect(await events.next() == .playRequested)
        try await until("playing") { engine.sample().positionMs > 1_200 }
    }

    @Test(.timeLimit(.minutes(1)))
    func aStoppedSessionsMediaFails() async throws {
        let server = FixtureServer()
        try await server.start()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, durationMs: 4_000, media: try await MediaMaker.mp4()))
        let api = api(server)
        let info = try await api.startPlayback(PlaybackRequest(itemId: 7, keyedUrls: true))
        let url = try await api.mediaURL(info.url)
        server.stopSession(info.sessionId)
        let engine = AVPlayerEngine()
        defer { engine.close() }
        engine.load(url: url, startMs: 0, play: true)
        var events = engine.events.makeAsyncIterator()
        guard case .failed(let error) = await events.next() else {
            Issue.record("expected a failure")
            return
        }
        // AVPlayer reads a file's failure as its own error or the HTTP
        // status; either way, nothing that a new session can't mend.
        switch error {
        case .http, .unsupported, .other, .network: break
        case .playlist: Issue.record("a file isn't a playlist: \(error)")
        }
    }
}
#endif
