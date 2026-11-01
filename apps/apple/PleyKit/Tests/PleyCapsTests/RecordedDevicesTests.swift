import Foundation
import PleyCaps
import PleyModel
import Testing

/// Each recorded device's report must be the server's fixture, which
/// `apps/server/tests/playback_apple.rs` decides from: the report and the
/// server's answer to it can't drift apart. `PLEY_UPDATE_CAPS_FIXTURES=1`
/// rewrites the fixtures from the builder (on the Mac).
@Suite("Recorded devices report what the server's fixtures hold")
struct RecordedDevicesTests {
    @Test(arguments: recordedDevices)
    func reportIsTheServersFixture(device: String) throws {
        let snapshot = CapsBuilder.build(try CapsFiles.probe(device))
        let actual = try CapsFiles.json(snapshot.report)
        let file = CapsFiles.serverFixture(device)
        #if os(macOS)
        if ProcessInfo.processInfo.environment["PLEY_UPDATE_CAPS_FIXTURES"] == "1" {
            try actual.write(to: file)
        }
        #endif
        let expected = try Data(contentsOf: file)
        let decodedActual = try JSONSerialization.jsonObject(with: actual) as? NSDictionary
        let decodedExpected = try JSONSerialization.jsonObject(with: expected) as? NSDictionary
        #expect(decodedActual != nil && decodedActual == decodedExpected,
                "\(file.lastPathComponent) differs; PLEY_UPDATE_CAPS_FIXTURES=1 rewrites it:\n\(String(decoding: actual, as: UTF8.self))")
        // The fixture reads back as the report.
        #expect(try PleyJSON.decoder.decode(ClientCaps.self, from: expected) == snapshot.report)
    }

    @Test func eachRecordedDeviceHasItsClass() throws {
        let classes = try recordedDevices.map { CapsBuilder.build(try CapsFiles.probe($0)).deviceClass }
        #expect(classes == ["a14_m2", "a9", "a17_m3", "intel_mac", "a14_m2", "a14_m2", "apple_tv_hd"])
    }
}
