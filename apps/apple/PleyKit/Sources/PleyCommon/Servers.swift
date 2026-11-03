import Foundation

/// A server opened before.
public struct SavedServer: Codable, Hashable, Sendable {
    /// Its origin: `http://nas:7539`.
    public var origin: String
    public var name: String
    /// The id it announces on the network, which stays across renames and new addresses.
    public var id: String?
    public var lastUsed: Date

    public init(origin: String, name: String, id: String? = nil, lastUsed: Date) {
        self.origin = origin
        self.name = name
        self.id = id
        self.lastUsed = lastUsed
    }
}

/// A server that answered on the local network.
public struct FoundServer: Codable, Hashable, Sendable {
    public var origin: String
    public var name: String
    public var id: String?
    public var version: String

    public init(origin: String, name: String, id: String? = nil, version: String) {
        self.origin = origin
        self.name = name
        self.id = id
        self.version = version
    }
}

/// One row of the server list: saved, found on the network, or both.
/// `lookalike`: found, not saved, and named like a saved server it isn't.
public struct ServerEntry: Hashable, Sendable, Identifiable {
    public var origin: String
    public var name: String
    public var id: String?
    public var saved: Bool
    public var found: Bool
    public var lookalike: Bool

    public init(origin: String, name: String, id: String? = nil, saved: Bool, found: Bool, lookalike: Bool = false) {
        self.origin = origin
        self.name = name
        self.id = id
        self.saved = saved
        self.found = found
        self.lookalike = lookalike
    }
}

/// The rules for server addresses and the server list, as the Android app
/// (`Servers.kt`) and the desktop app's picker follow them.
public enum Servers {
    /// Pley's web port, for addresses typed without one.
    public static let defaultPort = 7539

    /// `nas`, `nas:8080`, `192.168.1.20` or `https://pley.example.com/` as a
    /// server's address (its origin); `nil` when it can't be one. Without a
    /// scheme it's http, and without a port Pley's.
    public static func normalize(_ input: String) -> String? {
        let text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty || text.contains(where: \.isWhitespace) { return nil }
        let hasScheme = typedScheme(text)
        guard let parts = URLParts(hasScheme ? text : "http://\(text)"),
              parts.scheme == "http" || parts.scheme == "https"
        else { return nil }
        let port = parts.port ?? (hasScheme ? nil : defaultPort)
        if let port, port != parts.defaultPort { return "\(parts.scheme)://\(parts.host):\(port)" }
        return "\(parts.scheme)://\(parts.host)"
    }

    /// Whether `host` is this device, on its local network (a private
    /// address, a name without dots, `.local`…) or a Tailscale name: where
    /// plain http is how Pley is reached, and the network itself is trusted.
    public static func isLocalHost(_ host: String) -> Bool {
        var h = host.lowercased()
        if h.hasPrefix("[") { h.removeFirst() }
        if h.hasSuffix("]") { h.removeLast() }
        if h == "localhost" || (!h.contains(".") && !h.contains(":")) { return true }
        if matches(h, #"\.(local|lan|home|internal|localdomain|home\.arpa|ts\.net)$"#) { return true }
        if matches(h, #"^\d+\.\d+\.\d+\.\d+$"#) {
            let octets = h.split(separator: ".").map { Int($0) ?? -1 }
            let (a, b) = (octets[0], octets[1])
            return a == 10 || a == 127 || (a == 172 && (16...31).contains(b)) || (a == 192 && b == 168)
                || (a == 169 && b == 254) || (a == 100 && (64...127).contains(b))
        }
        return h == "::1" || matches(h, "^f[cd][0-9a-f]{0,2}:") || matches(h, "^fe[89ab][0-9a-f]?:")
    }

    /// Whether a connection to `origin` travels unencrypted beyond the local
    /// network: passwords and all.
    public static func unencrypted(_ origin: String) -> Bool {
        guard let parts = URLParts(origin) else { return false }
        return parts.scheme == "http" && !isLocalHost(parts.host)
    }

    /// The addresses to try, in turn, for what someone typed: a name on the
    /// internet typed without `http://` is tried with https first (Pley
    /// behind a proxy with a certificate), then with plain http on Pley's
    /// port.
    public static func candidates(_ input: String) -> [String] {
        guard let origin = normalize(input), let parts = URLParts(origin) else { return [] }
        let text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        let beforePath = text.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        let typedPort = matches(String(beforePath), #"^[^/]*:\d+$"#)
        if typedScheme(text) || typedPort || isLocalHost(parts.host) { return [origin] }
        return ["https://\(parts.host)", origin]
    }

    /// The address to show for a server: its host and, unless it's Pley's,
    /// its port.
    public static func displayAddress(_ origin: String) -> String {
        guard let parts = URLParts(origin) else { return origin }
        guard let port = parts.port, !(parts.scheme == "http" && port == defaultPort) else { return parts.host }
        return "\(parts.host):\(port)"
    }

    /// Saved servers, most recently opened first, then the ones found on
    /// the network that aren't saved.
    public static func merge(saved: [SavedServer], found: [FoundServer]) -> [ServerEntry] {
        let newestFirst = saved.enumerated()
            .sorted { $0.element.lastUsed != $1.element.lastUsed ? $0.element.lastUsed > $1.element.lastUsed : $0.offset < $1.offset }
            .map(\.element)
        var entries = newestFirst.map { s in
            ServerEntry(origin: s.origin, name: s.name, id: s.id, saved: true,
                        found: found.contains { same(s.origin, s.id, $0.origin, $0.id) })
        }
        for f in found where !entries.contains(where: { same($0.origin, $0.id, f.origin, f.id) }) {
            let name = f.name.trimmingCharacters(in: .whitespaces).lowercased()
            let lookalike = saved.contains { $0.name.trimmingCharacters(in: .whitespaces).lowercased() == name }
            entries.append(ServerEntry(origin: f.origin, name: f.name, id: f.id, saved: false, found: true,
                                       lookalike: lookalike))
        }
        return entries
    }

    /// The server to open by itself at start: the one opened last.
    public static func lastUsed(_ saved: [SavedServer]) -> SavedServer? {
        saved.max { $0.lastUsed < $1.lastUsed }
    }

    /// `servers` with `server` in it, replacing one with the same address
    /// or id (a server found at a new address).
    public static func withServer(_ servers: [SavedServer], _ server: SavedServer) -> [SavedServer] {
        servers.filter { $0.origin != server.origin && (server.id == nil || $0.id != server.id) } + [server]
    }

    private static func same(_ aOrigin: String, _ aID: String?, _ bOrigin: String, _ bID: String?) -> Bool {
        (aID != nil && aID == bID) || aOrigin == bOrigin
    }

    private static func typedScheme(_ text: String) -> Bool {
        matches(text, "^[a-zA-Z][a-zA-Z0-9+.-]*://")
    }

    private static func matches(_ text: String, _ pattern: String) -> Bool {
        text.range(of: pattern, options: .regularExpression) != nil
    }
}
