import Foundation

/// This install's player id, `apple-<uuid>`, kept in `UserDefaults`: the
/// server ends a client's earlier session when it starts another, and
/// remembers a network per client (docs/API.md, "Play request"). It names
/// the install, never the person or the device.
public enum ClientID {
    static let key = "pley.player.clientId"

    public static func current(_ defaults: UserDefaults = .standard) -> String {
        if let kept = defaults.string(forKey: key), kept.hasPrefix("apple-"), kept.count <= 128 { return kept }
        let made = "apple-" + UUID().uuidString.lowercased()
        defaults.set(made, forKey: key)
        return made
    }
}
