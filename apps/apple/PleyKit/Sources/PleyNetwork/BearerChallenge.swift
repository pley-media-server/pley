import Foundation

/// Reading `WWW-Authenticate` Bearer challenges (RFC 6750 §3), as the
/// Android app's `TokenAuthenticator` does.
public enum BearerChallenge {
    /// Whether `header` is a Bearer challenge with `error="invalid_token"`:
    /// the token expired or was ended, and a refresh may help.
    public static func isInvalidToken(_ header: String) -> Bool {
        error(in: header) == "invalid_token"
    }

    /// Whether `header` is a Bearer challenge with `error="insufficient_scope"`.
    public static func isInsufficientScope(_ header: String) -> Bool {
        error(in: header) == "insufficient_scope"
    }

    /// The `scope` an insufficient_scope challenge names, if any.
    public static func scope(_ header: String) -> String? {
        guard let params = parameters(header) else { return nil }
        return param("scope", in: params)
    }

    private static func error(in header: String) -> String? {
        guard let params = parameters(header) else { return nil }
        return param("error", in: params)?.lowercased()
    }

    /// What follows `Bearer`, or `nil` when it isn't a Bearer challenge.
    private static func parameters(_ header: String) -> Substring? {
        let h = header.trimmingCharacters(in: .whitespaces)
        guard h.count >= 6, h.prefix(6).lowercased() == "bearer" else { return nil }
        let rest = h.dropFirst(6)
        if let first = rest.first, !first.isWhitespace { return nil }
        return rest
    }

    /// `name`'s value, quoted or not, as a whole parameter: `error=invalid_token_x` isn't `invalid_token`.
    private static func param(_ name: String, in params: Substring) -> String? {
        let pattern = #"(?:^|[\s,])"# + name + #"\s*=\s*(?:"([^"]*)"|([^\s,"]+))\s*(?:,|$)"#
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return nil }
        let text = String(params)
        let range = NSRange(text.startIndex..., in: text)
        guard let match = regex.firstMatch(in: text, range: range) else { return nil }
        for group in 1...2 {
            if let r = Range(match.range(at: group), in: text) { return String(text[r]) }
        }
        return nil
    }
}
