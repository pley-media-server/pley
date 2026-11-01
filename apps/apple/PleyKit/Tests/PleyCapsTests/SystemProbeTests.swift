#if os(macOS)
import Foundation
import PleyCaps
import PleyModel
import Testing

/// The real probe, on the Mac running the tests: whatever this Mac is, it
/// must give H.264 and a screen that makes sense.
@Suite("This Mac's own probe")
struct SystemProbeTests {
    @Test func thisMacReportsH264AndASaneScreen() async throws {
        let input = await SystemCapsProbe().read(screen: nil)
        #expect(input.platform == .macOS)
        #expect(!input.model.isEmpty)
        #expect(input.chip?.isEmpty == false)
        #expect(input.memoryBytes > 1 << 30)
        #expect(Set(input.hardwareDecode.keys) == ["hevc", "av1", "vp9"])
        #expect((2...64).contains(input.audio.maxChannels))

        let snapshot = CapsBuilder.build(input)
        let report = snapshot.report
        let h264 = try #require(report.video?.first)
        #expect(h264.codec == "h264" && h264.hardware)
        #expect(report.device?.form == .desktop)
        #expect(report.device?.model == input.model)
        #expect(snapshot.deviceClass != "")
        #expect((2...8).contains(report.audioChannels ?? 0))
        // A screen is attached where the tests run; a headless Mac has none.
        if let display = report.display {
            #expect(display.width >= display.height && display.height >= 480)
            #expect(display.refreshRates.count == 1 && (24...240).contains(display.refreshRates[0]))
        }
        // The report holds no name of this Mac's owner: only its model.
        let json = String(decoding: try PleyJSON.encoder.encode(input), as: UTF8.self)
        #expect(!json.contains(NSUserName()))
    }
}
#endif
