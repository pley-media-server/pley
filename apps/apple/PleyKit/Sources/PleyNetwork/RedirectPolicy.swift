import Foundation
import PleyCommon

/// Which redirects a request follows. Tokens and codes go only to the
/// server's own origin, so a redirect never takes them anywhere else.
public enum RedirectPolicy {
    public enum Mode: Sendable, Equatable {
        /// Sign-in endpoints and probes: no redirect is followed; the 3xx
        /// is the answer.
        case endpoints
        /// The server's API: a redirect is followed only to `issuer`'s own
        /// origin (scheme, host and port), so never between http and https
        /// and never to another host or port.
        case api(issuer: String)
    }

    /// Whether a redirect from `from` to `to` is followed.
    public static func decide(from: URL, to: URL, mode: Mode) -> Bool {
        switch mode {
        case .endpoints:
            return false
        case .api(let issuer):
            guard let issuer = Addresses.origin(issuer),
                  let source = Addresses.origin(from),
                  let target = Addresses.origin(to)
            else { return false }
            return source == issuer && target == issuer
        }
    }
}
