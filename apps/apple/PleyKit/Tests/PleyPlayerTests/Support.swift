import Foundation
import os
import PleyCaps
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting

/// Tokens the fixture server gave, never refreshed.
struct StaticTokens: TokenProvider {
    let issuer: String
    let token: String

    init(server: FixtureServer) {
        issuer = server.origin
        token = server.issueTokens().accessToken
    }

    func accessToken() async throws -> String { token }
    func refresh(after refused: String) async throws {}
    func refusedAfterRefresh(_ token: String) async -> Bool { true }
    func connectionLeaked() async {}
}

func api(_ server: FixtureServer) -> PleyAPI {
    let http = HTTPTransport(userAgent: "pleyApple/9.9.9 (macOS 15.2; Mac)")
    return PleyAPI(client: APIClient(origin: server.origin, tokens: StaticTokens(server: server), transport: http))
}

/// A clock the test moves. Its sleep lasts until cancelled, so the
/// controller's ticker never runs by itself: tests call `tick()`.
final class FakeClock: PlayerClock, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (now: Int64(1_000_000), wall: Date(timeIntervalSince1970: 1_761_000_000)))

    func now() -> Int64 { state.withLock { $0.now } }
    func wall() -> Date { state.withLock { $0.wall } }

    func advance(ms: Int64) {
        state.withLock {
            $0.now += ms
            $0.wall += Double(ms) / 1000
        }
    }

    func sleep(ms: Int64) async throws {
        try await Task.sleep(for: .seconds(3600))
    }
}

/// An engine that does what the test says.
@MainActor
final class FakeEngine: PlayerEngine {
    struct Load: Equatable {
        let url: URL
        let startMs: Int
        let play: Bool
    }

    let events: AsyncStream<EngineEvent>
    private let continuation: AsyncStream<EngineEvent>.Continuation
    private(set) var loads: [Load] = []
    private(set) var calls: [String] = []
    var current = EngineSample()
    private(set) var closed = false

    init() {
        (events, continuation) = AsyncStream.makeStream(of: EngineEvent.self)
    }

    func load(url: URL, startMs: Int, play: Bool) {
        loads.append(Load(url: url, startMs: startMs, play: play))
        current = EngineSample()
        current.positionMs = startMs
    }

    func play() { calls.append("play") }
    func pause() { calls.append("pause") }

    func seek(toMs positionMs: Int) {
        calls.append("seek \(positionMs)")
        current.positionMs = positionMs
    }

    func sample() -> EngineSample { current }

    func close() {
        closed = true
        continuation.finish()
    }

    func emit(_ event: EngineEvent) { continuation.yield(event) }

    /// The media plays at `positionMs`, with frames shown.
    func playing(at positionMs: Int, frames: Int = 10) {
        current.ready = true
        current.playing = true
        current.positionMs = positionMs
        current.videoFrames = frames
        current.presentationWidth = 320
        current.presentationHeight = 180
    }
}

/// A probe that reads what it was given.
struct FixedProbe: CapsProbe {
    let input: ProbeInput
    func read(screen: ScreenFacts?) async -> ProbeInput { input }
}

/// The Mac M3 XDR's recorded probe, from the caps tests.
func recordedProbe(_ name: String = "mac_m3_xdr") throws -> ProbeInput {
    let file = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("PleyCapsTests/Probes/\(name).json")
    return try PleyJSON.decoder.decode(ProbeInput.self, from: Data(contentsOf: file))
}

/// A controller on a fixture server with a fake engine and clock.
@MainActor
struct Rig {
    let server: FixtureServer
    let clock = FakeClock()
    let engine = FakeEngine()
    let reporter = SessionReporter()
    let late: LateReports
    let controller: PlaybackController
    let folder: URL

    init(server: FixtureServer, request: PlayRequest) throws {
        self.server = server
        folder = FileManager.default.temporaryDirectory.appendingPathComponent("pley-late-\(UUID().uuidString)")
        late = LateReports(origin: server.origin, profileId: 2, directory: folder, clock: clock)
        let caps = DeviceCaps(probe: FixedProbe(input: try recordedProbe()))
        let engine = self.engine
        let env = PlaybackEnvironment(api: api(server), caps: caps, clientId: "apple-test", reporter: reporter,
                                      late: late, clock: clock, engines: { engine })
        controller = PlaybackController(request: request, environment: env)
    }

    /// The progress reports' states and positions, after everything queued went out.
    func reports() async -> [ProgressReport] {
        await reporter.flush()
        return server.progressReports.compactMap { try? PleyJSON.decoder.decode(ProgressReport.self, from: $0.body) }
    }

    func requests() async -> [PlaybackRequest] {
        await reporter.flush()
        return server.playbackRequests.compactMap { try? PleyJSON.decoder.decode(PlaybackRequest.self, from: $0) }
    }

    func cleanUp() {
        try? FileManager.default.removeItem(at: folder)
        server.stop()
    }
}

/// Waits until `condition` holds, 5 s at most.
@MainActor
func until(_ what: String = "the condition", _ condition: @MainActor () async -> Bool) async throws {
    for _ in 0..<500 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    struct TimedOut: Error, CustomStringConvertible { let description: String }
    throw TimedOut(description: "timed out waiting for \(what)")
}
