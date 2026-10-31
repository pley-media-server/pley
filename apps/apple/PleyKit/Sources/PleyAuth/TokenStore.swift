import Foundation
import os

/// One server's sign-in as the app keeps it. Its `description` never shows
/// the tokens, so printing or logging one leaks nothing.
public struct StoredTokens: Codable, Sendable, Hashable, CustomStringConvertible, CustomDebugStringConvertible {
    /// The server's issuer (its origin); the tokens are sent there only.
    public var issuer: String
    public var clientID: String
    public var tokenEndpoint: String
    public var revocationEndpoint: String
    public var accessToken: String
    public var accessExpiresAt: Date
    public var refreshToken: String?
    /// What the server allowed, separated by spaces.
    public var scope: String
    /// When a refresh or a profile switch gave `accessToken`; `nil` for a sign-in.
    public var issuedByRefreshAt: Date?

    public init(issuer: String, clientID: String, tokenEndpoint: String, revocationEndpoint: String,
                accessToken: String, accessExpiresAt: Date, refreshToken: String?, scope: String,
                issuedByRefreshAt: Date? = nil) {
        self.issuer = issuer
        self.clientID = clientID
        self.tokenEndpoint = tokenEndpoint
        self.revocationEndpoint = revocationEndpoint
        self.accessToken = accessToken
        self.accessExpiresAt = accessExpiresAt
        self.refreshToken = refreshToken
        self.scope = scope
        self.issuedByRefreshAt = issuedByRefreshAt
    }

    /// The scopes, one word each.
    public var scopes: Set<String> { Set(scope.split(separator: " ").map(String.init)) }

    /// Whether the access token still works for a while (a minute) at `now`.
    func accessUsable(at now: Date) -> Bool {
        accessExpiresAt.timeIntervalSince(now) > Tokens.refreshAhead
    }

    public var description: String {
        "StoredTokens(issuer: \(issuer), scope: \(scope), expires: \(accessExpiresAt), "
            + "refreshToken: \(refreshToken == nil ? "none" : "hidden"))"
    }

    public var debugDescription: String { description }
}

/// Where sign-ins are kept, one per server.
public protocol TokenStore: Sendable {
    func load(issuer: String) async throws -> StoredTokens?
    func save(_ tokens: StoredTokens) async throws
    func delete(issuer: String) async throws
}

/// Sign-ins in the Keychain: one item per server (service
/// `app.pley.apple.signin`, account the issuer), on this device only.
public struct KeychainTokenStore: TokenStore {
    public static let service = "app.pley.apple.signin"

    public init() {}

    public func load(issuer: String) async throws -> StoredTokens? {
        guard let data = try item(issuer).read() else { return nil }
        // Unreadable (an older format): as good as signed out.
        return try? JSONDecoder().decode(StoredTokens.self, from: data)
    }

    public func save(_ tokens: StoredTokens) async throws {
        try item(tokens.issuer).write(try JSONEncoder().encode(tokens))
    }

    public func delete(issuer: String) async throws {
        try item(issuer).delete()
    }

    private func item(_ issuer: String) -> KeychainItem {
        KeychainItem(service: Self.service, account: issuer)
    }
}

/// Sign-ins in memory, for tests and the check tool.
public final class MemoryTokenStore: TokenStore {
    private let items = OSAllocatedUnfairLock<[String: StoredTokens]>(initialState: [:])

    public init() {}

    public func load(issuer: String) async throws -> StoredTokens? {
        items.withLock { $0[issuer] }
    }

    public func save(_ tokens: StoredTokens) async throws {
        items.withLock { $0[tokens.issuer] = tokens }
    }

    public func delete(issuer: String) async throws {
        _ = items.withLock { $0.removeValue(forKey: issuer) }
    }
}
