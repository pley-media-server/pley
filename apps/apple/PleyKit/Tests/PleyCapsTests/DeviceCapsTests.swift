import Foundation
import PleyCaps
import PleyModel
import Testing

/// A probe that answers what it is given, and counts its readings.
final class FakeProbe: CapsProbe, @unchecked Sendable {
    private let lock = NSLock()
    private var input: ProbeInput
    private var screens: [ScreenFacts?] = []

    init(_ input: ProbeInput) { self.input = input }

    var reads: Int { lock.withLock { screens.count } }
    var lastScreen: ScreenFacts? { lock.withLock { screens.last ?? nil } }

    func set(_ change: (inout ProbeInput) -> Void) {
        lock.withLock { change(&input) }
    }

    func read(screen: ScreenFacts?) async -> ProbeInput {
        lock.withLock {
            screens.append(screen)
            var input = input
            if let screen {
                input.display = DisplayFacts(width: screen.width, height: screen.height, maxFps: screen.maxFps)
                input.hdr.edrHeadroom = screen.edrHeadroom
            }
            return input
        }
    }
}

@Suite("Device caps are read once, and again after a change")
@MainActor
struct DeviceCapsTests {
    @Test func readsOnceUntilSomethingChanges() async throws {
        let fake = FakeProbe(try probe("tv_4k_hdr10"))
        let caps = DeviceCaps(probe: fake)
        #expect(caps.snapshot == nil)
        let first = await caps.warmUp()
        #expect(first.report.hdrTypes == [.hdr10, .hlg])
        #expect(caps.snapshot == first)
        _ = await caps.warmUp()
        #expect(fake.reads == 1)

        // The TV now shows Dolby Vision, and the route changed with it.
        fake.set { $0.hdr.modes = ["hdr10", "hlg", "dolby_vision"] }
        caps.audioRouteChanged()
        let second = await caps.warmUp()
        #expect(second.report.hdrTypes == [.hdr10, .hlg, .dolbyVision])
        #expect(fake.reads == 2)
    }

    @Test func theMacsWindowMovingToAnotherScreenReadsItsHDR() async throws {
        let fake = FakeProbe(try probe("mac_m3_xdr"))
        let caps = DeviceCaps(probe: fake)
        let xdr = ScreenFacts(width: 3024, height: 1964, maxFps: 120, edrHeadroom: 16)
        caps.screenChanged(xdr)
        #expect(await caps.warmUp().report.hdrTypes == [.hdr10, .hlg, .dolbyVision])
        #expect(fake.lastScreen == xdr)

        let sdr = ScreenFacts(width: 2560, height: 1440, maxFps: 60, edrHeadroom: 1)
        caps.screenChanged(sdr)
        let snapshot = await caps.warmUp()
        #expect(snapshot.report.hdrTypes == [])
        #expect(snapshot.report.display == DisplayCaps(width: 2560, height: 1440, refreshRates: [60]))
        let reads = fake.reads
        // The same screen again changes nothing.
        caps.screenChanged(sdr)
        _ = await caps.warmUp()
        #expect(fake.reads == reads)
    }
}
