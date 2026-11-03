import Foundation
import PleyCaps
import PleyModel

/// The device classes recorded in `Probes/`, each with its report in
/// `apps/server/tests/fixtures/caps/apple_<class>_v2.json`.
let recordedDevices = ["iphone_a16_hdr", "ipad_5th_gen", "mac_m3_xdr", "mac_intel_sdr", "tv_4k_hdr10", "tv_4k_dv",
                       "tv_hd"]

/// Files are read where they are, found from this file's own path, as
/// `Fixtures` does: the tests on the simulators read the Mac's files too.
enum CapsFiles {
    static let tests = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

    static let serverFixtures: URL = tests
        .deletingLastPathComponent()  // Tests
        .deletingLastPathComponent()  // PleyKit
        .deletingLastPathComponent()  // apple
        .deletingLastPathComponent()  // apps
        .appendingPathComponent("server/tests/fixtures/caps", isDirectory: true)

    static func probe(_ name: String) throws -> ProbeInput {
        let data = try Data(contentsOf: tests.appendingPathComponent("Probes/\(name).json"))
        return try PleyJSON.decoder.decode(ProbeInput.self, from: data)
    }

    static func serverFixture(_ name: String) -> URL {
        serverFixtures.appendingPathComponent("apple_\(name)_v2.json")
    }

    /// A report as the fixtures hold it: snake_case, sorted, indented.
    static func json(_ report: ClientCaps) throws -> Data {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        // Foundation writes `"key" : value` and empty lists over three lines.
        let text = String(decoding: try encoder.encode(report), as: UTF8.self)
            .replacingOccurrences(of: "\" : ", with: "\": ")
            .replacingOccurrences(of: #"\[\s*\]"#, with: "[]", options: .regularExpression)
        return Data((text + "\n").utf8)
    }
}

/// A probe input to change one fact of in a test.
func probe(_ name: String, _ change: (inout ProbeInput) -> Void = { _ in }) throws -> ProbeInput {
    var input = try CapsFiles.probe(name)
    change(&input)
    return input
}

func report(_ input: ProbeInput) -> ClientCaps {
    CapsBuilder.build(input).report
}
