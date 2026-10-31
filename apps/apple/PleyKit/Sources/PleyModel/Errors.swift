import Foundation

/// The body of an API refusal: `{ "code": "not_found", "message": "item 42
/// not found" }`. `message` is written for people; the app shows it only
/// after making it plain (PleyCommon's `plain`). The extra `error` key of a
/// `session_stopped` answer is ignored.
public struct APIErrorBody: Codable, Hashable, Sendable {
    public var code: String
    public var message: String?

    public init(code: String, message: String? = nil) {
        self.code = code
        self.message = message
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        code = try c.value(.code, or: "")
        message = try c.decodeIfPresent(String.self, forKey: .message)
    }
}

/// An OAuth endpoint's refusal (RFC 6749 §5.2): `{ "error": "invalid_grant",
/// "error_description": "…" }`, with Pley's `reason` on a profile switch.
public struct OAuthErrorBody: Codable, Hashable, Sendable {
    public var error: String
    public var errorDescription: String?
    /// `wrong_pin`, `pin_locked`, `throttled`, `browser` or `not_allowed`.
    public var reason: String?

    public init(error: String, errorDescription: String? = nil, reason: String? = nil) {
        self.error = error
        self.errorDescription = errorDescription
        self.reason = reason
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        error = try c.value(.error, or: "")
        errorDescription = try c.decodeIfPresent(String.self, forKey: .errorDescription)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
    }
}
