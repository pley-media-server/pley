import Foundation
import PleyCaps
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// The Mac's probe, the screen's EDR headroom as the window's screen has it:
/// 16 on an XDR display (HDR), 1 on an SDR one.
private struct ScreenProbe: CapsProbe {
    let input: ProbeInput
    func read(screen: ScreenFacts?) async -> ProbeInput {
        var input = input
        if let screen { input.hdr.edrHeadroom = screen.edrHeadroom }
        return input
    }
}

private let xdr = ScreenFacts(width: 3024, height: 1964, maxFps: 120, edrHeadroom: 16)
private let sdrScreen = ScreenFacts(width: 2560, height: 1440, maxFps: 60, edrHeadroom: 1)

/// A 90 s movie of `range`, on a Mac whose window is on the XDR display.
@MainActor
private func rig(range: String, followsScreen: Bool = true) async throws -> (Rig, DeviceCaps) {
    let server = FixtureServer()
    try await server.start()
    var movie = FixtureServer.Playable(itemId: 61, fileId: 610, title: "Bright", durationMs: 90_000, width: 3840,
                                       height: 2160)
    movie.dynamicRange = range
    server.addPlayable(movie)
    let caps = DeviceCaps(probe: ScreenProbe(input: try recordedProbe()))
    caps.screenChanged(xdr)
    return (try Rig(server: server, request: PlayRequest(itemId: 61), warmer: NoWarmer(), caps: caps,
                    followsScreen: followsScreen), caps)
}

@MainActor
private func started(_ rig: Rig) async throws {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
}

@Suite("The screen's HDR in the controller")
@MainActor
struct CapsChangeTests {
    @Test func anHDRVideoMovedToAnSDRScreenStartsANewSessionWhereItIs() async throws {
        let (rig, caps) = try await rig(range: "hdr10")
        defer { rig.cleanUp() }
        try await started(rig)
        let first = try #require(await rig.requests().first)
        #expect(first.caps.hdrTypes?.isEmpty == false)
        rig.engine.playing(at: 30_000)
        caps.screenChanged(sdrScreen)
        try await until("the new session") { rig.engine.loads.count == 2 }
        let asked = await rig.requests()
        #expect(asked.count == 2)
        #expect(asked[1].caps.hdrTypes ?? [] == [])
        // Beside the one playing, from where it was, as a quality change.
        #expect(asked[1].keepPrevious == true)
        #expect(rig.engine.loads.last?.startMs == 30_000)
        // Back on the XDR display: HDR again.
        rig.engine.emit(.ready)
        try await until("playing again") { rig.controller.state.phase == .playing }
        caps.screenChanged(xdr)
        try await until("the third session") { rig.engine.loads.count == 3 }
        #expect(await rig.requests().last?.caps.hdrTypes?.isEmpty == false)
        await rig.controller.close()
    }

    @Test func anSDRVideoPlaysOnAndTheNextSessionAsksWithTheNewScreen() async throws {
        let (rig, caps) = try await rig(range: "sdr")
        defer { rig.cleanUp() }
        try await started(rig)
        caps.screenChanged(sdrScreen)
        try await until("the screen read") { caps.snapshot?.report.hdrTypes ?? [] == [] }
        try await Task.sleep(for: .milliseconds(100))
        #expect(rig.engine.loads.count == 1)
        // A new session for another reason asks with what the screen shows now.
        rig.controller.retry()
        try await until("the second load") { rig.engine.loads.count == 2 }
        #expect(await rig.requests().last?.caps.hdrTypes ?? [] == [])
        await rig.controller.close()
    }

    @Test func iPhoneIPadAndAppleTVDontFollowTheScreen() async throws {
        let (rig, caps) = try await rig(range: "hdr10", followsScreen: false)
        defer { rig.cleanUp() }
        try await started(rig)
        caps.screenChanged(sdrScreen)
        try await until("the screen read") { caps.snapshot?.report.hdrTypes ?? [] == [] }
        try await Task.sleep(for: .milliseconds(100))
        #expect(rig.engine.loads.count == 1)
        await rig.controller.close()
    }
}
