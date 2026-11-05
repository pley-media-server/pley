import Foundation
import PleyData
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// A live engine that does what the test says.
@MainActor
final class FakeLiveEngine: LiveEngine {
    let events: AsyncStream<LiveEngineEvent>
    private let continuation: AsyncStream<LiveEngineEvent>.Continuation
    private(set) var loads: [URL] = []
    private(set) var plays = 0
    private(set) var closed = false
    var video = true

    init() {
        (events, continuation) = AsyncStream.makeStream(of: LiveEngineEvent.self)
    }

    func load(url: URL) { loads.append(url) }
    func play() { plays += 1 }
    func hasVideo() -> Bool { video }

    func close() {
        closed = true
        continuation.finish()
    }

    func send(_ event: LiveEngineEvent) { continuation.yield(event) }
}

/// A clock whose sleeps end at once: the stall and sound-only waits pass straight away.
struct InstantClock: PlayerClock {
    func now() -> Int64 { 0 }
    func wall() -> Date { Date() }
    func sleep(ms: Int64) async throws { try await Task.sleep(for: .milliseconds(5)) }
}

@MainActor
private func eventually(_ condition: @MainActor () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return condition()
}

@MainActor
@Suite struct LivePlaybackTests {
    private func playback(_ server: FixtureServer, engines: [FakeLiveEngine]) -> LivePlayback {
        var made = engines
        return LivePlayback(live: LiveTvRepository(api: api(server)), clientId: "apple-test", clock: InstantClock(),
                            engines: { made.removeFirst() })
    }

    @Test func tuningPlaysTheKeyedPlaylistAndZappingLetsTheSessionBeforeGo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let engine = FakeLiveEngine()
        let live = playback(server, engines: [engine])
        live.tune(1)
        #expect(live.state.phase == .tuning)
        #expect(await eventually { engine.loads.count == 1 })
        #expect(engine.loads[0].path.hasPrefix("/api/live/media/pley_mk_"))
        engine.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        let first = try #require(live.sessionId)

        live.tune(2)
        #expect(await eventually { engine.loads.count == 2 })
        // The same engine plays on; the first session is left.
        #expect(!engine.closed)
        #expect(await eventually { server.liveSessions[first] == nil })
        #expect(server.liveSessions.values.sorted() == [2])
        let body = try #require(server.liveWatchRequests.last)
        let sent = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect(sent["client_id"] as? String == "apple-test" && sent["keyed_urls"] as? Bool == true)
        #expect(sent["caps"] == nil)
    }

    @Test func aPauseFromAnywherePlaysOn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let engine = FakeLiveEngine()
        let live = playback(server, engines: [engine])
        live.tune(1)
        #expect(await eventually { engine.loads.count == 1 })
        engine.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        engine.send(.paused)
        #expect(await eventually { engine.plays == 1 })
    }

    @Test func aStallShowsAfterAWhileAndSoundOnlyIsSaid() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let engine = FakeLiveEngine()
        engine.video = false
        let live = playback(server, engines: [engine])
        live.tune(1)
        #expect(await eventually { engine.loads.count == 1 })
        engine.send(.playing)
        #expect(await eventually { live.state.audioOnly })
        engine.send(.waiting)
        #expect(await eventually { live.state.buffering })
        engine.send(.playing)
        #expect(await eventually { !live.state.buffering })
        engine.send(.video(true))
        #expect(await eventually { !live.state.audioOnly })
    }

    @Test func everyConnectionInUseCanBeTakenOver() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.holdLiveConnection(2)
        let engine = FakeLiveEngine()
        let live = playback(server, engines: [engine])
        live.tune(1)
        #expect(await eventually {
            if case .failed(let trouble) = live.state.phase { return trouble.kind == .busy }
            return false
        })
        guard case .failed(let trouble) = live.state.phase else { return }
        #expect(trouble.connections.first?.canTakeOver == true)
        #expect(trouble.title(channel: "News") == "All connections are in use")
        live.retry(takeOver: trouble.connections[0].channelId)
        #expect(await eventually { engine.loads.count == 1 })
        #expect(server.liveSessions.values.sorted() == [1])
    }

    @Test func troubleHasAndroidsKinds() {
        #expect(LiveTrouble.kind(of: 409) == .busy)
        #expect(LiveTrouble.kind(of: 410) == .stopped)
        #expect(LiveTrouble.kind(of: 404) == .missing)
        #expect(LiveTrouble.kind(of: 403) == .notAllowed)
        for status in [502, 503, 504] { #expect(LiveTrouble.kind(of: status) == .unavailable) }
        #expect(LiveTrouble.of(PleyError.offline).kind == .offline)
        #expect(LiveTrouble.of(PleyError.unreachable("x")).kind == .offline)
        #expect(!LiveTrouble(kind: .missing, message: "").canRetry)
        #expect(LiveTrouble(kind: .unavailable, message: "").title(channel: "BBC One") == "BBC One isn't playing")
    }

    @Test func aMissingChannelIsSaidAndNotRetried() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let live = playback(server, engines: [FakeLiveEngine()])
        live.tune(999)
        #expect(await eventually { live.state.phase != .tuning })
        guard case .failed(let trouble) = live.state.phase else {
            Issue.record("it played")
            return
        }
        #expect(trouble.kind == .missing && !trouble.canRetry)
    }

    @Test func aStreamStoppedAtTheServerSaysWhyAndOneThatBrokeOffTunesAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let engine = FakeLiveEngine()
        let live = playback(server, engines: [engine])
        live.tune(1)
        #expect(await eventually { engine.loads.count == 1 })
        engine.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        // Broke off: its plan answers, so it tunes again.
        engine.send(.failed(.network(detail: "gone")))
        #expect(await eventually { engine.loads.count == 2 })
        engine.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        // Stopped at the server: its plan says 410.
        let session = try #require(live.sessionId)
        server.stopLiveSession(session, message: "Someone took this connection")
        engine.send(.failed(.http(status: 410, detail: "gone")))
        #expect(await eventually {
            if case .failed(let trouble) = live.state.phase { return trouble.kind == .stopped }
            return false
        })
        guard case .failed(let trouble) = live.state.phase else { return }
        #expect(trouble.message == "Someone took this connection.")
        #expect(engine.closed)
    }

    @Test func stoppedFromElsewhereItStopsAtOnceAndStaysStopped() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let first = FakeLiveEngine()
        let second = FakeLiveEngine()
        let live = playback(server, engines: [first, second])
        live.tune(1)
        #expect(await eventually { first.loads.count == 1 })
        first.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        // Another session's stop isn't this one's.
        live.stoppedElsewhere("someone-else", message: nil, byAdmin: false)
        #expect(live.state.phase == .playing)
        let session = try #require(live.sessionId)
        live.stoppedElsewhere(session, message: nil, byAdmin: false)
        #expect(live.state.phase == .failed(LiveTrouble(kind: .stopped, message: "Live TV was stopped from another device.")))
        #expect(first.closed && live.sessionId == nil)
        // Back from the background it stays stopped; Watch again tunes it.
        live.stop()
        live.resume()
        if case .failed(let trouble) = live.state.phase { #expect(trouble.kind == .stopped) } else { Issue.record("not stopped") }
        #expect(second.loads.isEmpty)
        live.retry()
        #expect(await eventually { second.loads.count == 1 })
        live.close()
    }

    @Test func anAdministratorsStopOrATakenConnectionSaysWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let first = FakeLiveEngine()
        let second = FakeLiveEngine()
        let live = playback(server, engines: [first, second])
        live.tune(1)
        #expect(await eventually { first.loads.count == 1 })
        live.stoppedElsewhere(try #require(live.sessionId), message: " Bedtime ", byAdmin: true)
        #expect(live.state.phase == .failed(LiveTrouble(kind: .stopped,
                                                        message: "Live TV was stopped by the server's administrator.\nBedtime.")))
        live.retry()
        #expect(await eventually { second.loads.count == 1 })
        live.takenOver(try #require(live.sessionId), message: "someone else needed the connection")
        #expect(live.state.phase == .failed(LiveTrouble(kind: .stopped, message: "Someone else needed the connection.")))
        #expect(second.closed)
        live.close()
    }

    @Test func aStopHeardWhileTuningEndsItWhenTheAnswerComes() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let first = FakeLiveEngine()
        let second = FakeLiveEngine()
        let live = playback(server, engines: [first, second])
        // The fixture numbers its sessions: the first is 1.
        live.tune(1)
        live.takenOver(String(format: "%032x", 1), message: "")
        #expect(await eventually {
            if case .failed = live.state.phase { return true }
            return false
        })
        #expect(live.state.phase == .failed(LiveTrouble(kind: .stopped, message: "Watching was stopped.")))
        #expect(first.loads.isEmpty && live.sessionId == nil)
        // Another session's stop heard while tuning doesn't touch this one.
        live.retry()
        live.stoppedElsewhere("someone-else", message: nil, byAdmin: false)
        #expect(await eventually { first.loads.count == 1 })
        #expect(live.sessionId == String(format: "%032x", 2))
        live.close()
    }

    @Test func anyFailureStaysAfterTheBackground() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.refuseLiveWatch(status: 404)
        let live = playback(server, engines: [])
        live.tune(1)
        #expect(await eventually {
            if case .failed = live.state.phase { return true }
            return false
        })
        live.stop()
        live.resume()
        guard case .failed(let trouble) = live.state.phase else {
            Issue.record("tuned again")
            return
        }
        #expect(trouble.kind == .missing)
        live.close()
    }

    @Test func goingToTheBackgroundLetsGoAndComingBackTunesAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let first = FakeLiveEngine()
        let second = FakeLiveEngine()
        let live = playback(server, engines: [first, second])
        live.tune(1)
        #expect(await eventually { first.loads.count == 1 })
        first.send(.playing)
        #expect(await eventually { live.state.phase == .playing })
        live.stop()
        #expect(first.closed && live.engine == nil)
        #expect(live.state == LiveState(channelId: 1))
        #expect(await eventually { server.liveSessions.isEmpty })
        live.resume()
        #expect(await eventually { second.loads.count == 1 })
        #expect(server.liveSessions.values.sorted() == [1])
        live.close()
        #expect(second.closed)
        #expect(await eventually { server.liveSessions.isEmpty })
        live.tune(2)
        #expect(live.state.phase == .idle)
    }
}

private func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}
