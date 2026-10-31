import Foundation

/// The JSON fixtures the Android app's tests read, so both apps test
/// against one truth. They are read where they are, in
/// `apps/android/core/testing/src/main/resources/fixtures`, found from this
/// file's own path: a package's resources can't live outside it, and the
/// tests on the simulators read the Mac's files too.
public enum Fixtures {
    /// The fixtures folder.
    public static let root: URL = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // PleyTesting
        .deletingLastPathComponent()  // Sources
        .deletingLastPathComponent()  // PleyKit
        .deletingLastPathComponent()  // apple
        .deletingLastPathComponent()  // apps
        .appendingPathComponent("android/core/testing/src/main/resources/fixtures", isDirectory: true)

    /// The fixture at `path`, such as `hubs/home.json`.
    public static func data(_ path: String) throws -> Data {
        guard !path.split(separator: "/").contains(".."), !path.hasPrefix("/") else {
            throw FixtureError.outsideFolder(path)
        }
        return try Data(contentsOf: root.appendingPathComponent(path))
    }

    public enum FixtureError: Error {
        case outsideFolder(String)
    }
}
