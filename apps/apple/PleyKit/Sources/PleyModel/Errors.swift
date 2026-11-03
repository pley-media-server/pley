import Foundation

/// The body of an API refusal: `{ "code": "not_found", "message": "item 42
/// not found" }`. `message` is written for people; the app shows it only
/// after making it plain (PleyCommon's `plain`). The extra `error` key of a
/// `session_stopped` answer is ignored.
public struct APIErrorBody: Codable, Hashable, Sendable {
    public var code: String
    public var message: String?
    /// Live TV's `409` (every connection of a source in use): what holds each one.
    public var connections: [BusyConnection]
    /// Subtitle search's `503`: `false` when the server has no search set
    /// up, `true` when what it has failed; `nil` for any other refusal.
    public var setup: Bool?
    /// Requests' `503 tmdb_key`: `false`, trying again won't help.
    public var retry: Bool?
    /// Requests' `409 declined`, `409 conflict` and `429`: when asking (or
    /// the action) may work, unix seconds; `nil` when it won't by waiting.
    public var retryAt: Int?
    /// Requests' `409 conflict`: the request as it is now.
    public var request: MediaRequest?

    public init(code: String, message: String? = nil, connections: [BusyConnection] = [], setup: Bool? = nil,
                retry: Bool? = nil, retryAt: Int? = nil, request: MediaRequest? = nil) {
        self.code = code
        self.message = message
        self.connections = connections
        self.setup = setup
        self.retry = retry
        self.retryAt = retryAt
        self.request = request
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        code = try c.value(.code, or: "")
        message = try c.decodeIfPresent(String.self, forKey: .message)
        connections = (try? c.value(.connections, or: [BusyConnection]())) ?? []
        setup = try? c.decodeIfPresent(Bool.self, forKey: .setup)
        retry = try? c.decodeIfPresent(Bool.self, forKey: .retry)
        retryAt = try? c.decodeIfPresent(Int.self, forKey: .retryAt)
        // A request that can't be read leaves the refusal readable all the same.
        request = try? c.decodeIfPresent(MediaRequest.self, forKey: .request)
    }

    /// Whether the body carries what a requests' refusal says beside its message.
    public var refusal: Bool { request != nil || retryAt != nil }
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
