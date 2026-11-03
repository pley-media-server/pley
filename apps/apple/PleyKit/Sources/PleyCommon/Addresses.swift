import Foundation

/// Server addresses as the app compares and keeps them.
public enum Addresses {
    /// The origin of `url` as browsers write it (`https://pley.example.com`,
    /// `http://nas:7539`, `http://[::1]:7539`): scheme and host in lower
    /// case, the port only when it isn't the scheme's own. `nil` when `url`
    /// has no scheme or host, or names an IPv6 address with a zone
    /// (`fe80::1%en0`), which no issuer can carry. A server's issuer is its
    /// origin, so tokens are matched against this.
    public static func origin(_ url: URL) -> String? {
        origin(url.absoluteString)
    }

    public static func origin(_ text: String) -> String? {
        guard let parts = URLParts(text) else { return nil }
        return parts.origin
    }
}

/// A URL's scheme, host and port, checked the way the app needs them:
/// lower case, a host of name characters or a bracketed IPv6 address without
/// a zone, and a port from 0 to 65535.
struct URLParts {
    let scheme: String
    /// Brackets included for IPv6: `[fd00::5]`.
    let host: String
    /// `nil` when the URL names none.
    let port: Int?

    init?(_ text: String) {
        guard let components = URLComponents(string: text),
              let scheme = components.scheme?.lowercased(),
              let host = components.host?.lowercased(),
              !host.isEmpty
        else { return nil }
        if host.hasPrefix("[") {
            guard host.hasSuffix("]"), !host.contains("%") else { return nil }
            let inner = host.dropFirst().dropLast()
            guard !inner.isEmpty, inner.allSatisfy({ $0.isHexDigit || $0 == ":" || $0 == "." }) else { return nil }
        } else {
            guard host.unicodeScalars.allSatisfy({ Self.nameCharacters.contains($0) }) else { return nil }
        }
        if let port = components.port, !(0...65535).contains(port) { return nil }
        self.scheme = scheme
        self.host = host
        self.port = components.port
    }

    private static let nameCharacters = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyz0123456789-._")

    /// The port the scheme uses by itself, if it is one the app knows.
    var defaultPort: Int? {
        switch scheme {
        case "http": 80
        case "https": 443
        default: nil
        }
    }

    var origin: String {
        if let port, port != defaultPort { return "\(scheme)://\(host):\(port)" }
        return "\(scheme)://\(host)"
    }

    /// The host without IPv6 brackets.
    var bareHost: String {
        host.hasPrefix("[") ? String(host.dropFirst().dropLast()) : host
    }
}
