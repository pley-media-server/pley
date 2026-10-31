import Foundation

/// `GET /api/server`, which answers without signing in: the server's name
/// and version, and whether it still has to be set up in the browser.
public struct ServerInfo: Codable, Hashable, Sendable {
    public var name: String
    public var version: String
    /// The build's short hash, `"unknown"` when built without the repository.
    public var commit: String?
    public var setupRequired: Bool
    /// Setting up takes the code from the server's log.
    public var setupCodeRequired: Bool

    public init(name: String, version: String, commit: String? = nil, setupRequired: Bool = false,
                setupCodeRequired: Bool = false) {
        self.name = name
        self.version = version
        self.commit = commit
        self.setupRequired = setupRequired
        self.setupCodeRequired = setupCodeRequired
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        version = try c.decode(String.self, forKey: .version)
        commit = try c.decodeIfPresent(String.self, forKey: .commit)
        setupRequired = try c.value(.setupRequired, or: false)
        setupCodeRequired = try c.value(.setupCodeRequired, or: false)
    }
}
