import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Why the app was signed out of a server without being asked to.
public enum SignOutReason: Sendable, Equatable {
    /// The server ended the sign-in: a refresh or a profile switch was
    /// refused (`invalid_grant`), or a fresh token was.
    case ended
    /// A request with a token reached an address that isn't private over
    /// plain http, so the app ended that sign-in itself.
    case unencrypted
    /// The server allowed more than the app asks for, so the app ended that sign-in itself.
    case widerScope

    public var message: String {
        switch self {
        case .ended: PleyError.signedOut.userMessage
        case .unencrypted:
            "this server answered from an address that isn't on your network, without encryption: "
                + "you were signed out to keep your sign-in safe"
        case .widerScope: AuthError.widerScope.message
        }
    }
}

/// Something about signing in that the screens hear about, wherever it happens.
public enum AuthEvent: Sendable, Equatable {
    /// The app holds no tokens for `issuer` any more: show sign-in.
    case signedOut(issuer: String, reason: SignOutReason)
}

/// The tokens of one server: the access token while it works, a new pair
/// from the refresh token when it doesn't. A port of the Android app's
/// `Tokens`.
///
/// - Refreshes run one at a time: callers that come while one runs share
///   it (the server ends the sign-in when a refresh token is used twice).
/// - The new refresh token is saved before the new access token is used.
/// - `invalid_grant` on a refresh, or a token refused moments after the
///   refresh that gave it, ends the sign-in (`events` says so).
public actor Tokens: TokenProvider {
    /// Access tokens are refreshed when they end within this.
    public static let refreshAhead: TimeInterval = 60
    /// A token refused within this of the refresh that gave it means the
    /// server no longer accepts this sign-in.
    public static let justIssued: TimeInterval = 10

    public nonisolated let issuer: String
    /// Sign-ins that ended without the person asking. One listener (the
    /// session); the newest 16 are kept for it.
    public nonisolated let events: AsyncStream<AuthEvent>

    private let continuation: AsyncStream<AuthEvent>.Continuation
    private let store: any TokenStore
    private let endpoints: EndpointClient
    private let clock: @Sendable () -> Date
    /// The refresh or profile switch running; others wait for it.
    private var running: Task<Void, any Error>?
    /// The access token the last refresh or switch gave, and when.
    private var lastIssued: (token: String, at: Date)?

    public init(issuer: String, store: any TokenStore, endpoints: EndpointClient,
                clock: @escaping @Sendable () -> Date = { Date() }) {
        self.issuer = issuer
        self.store = store
        self.endpoints = endpoints
        self.clock = clock
        (events, continuation) = AsyncStream.makeStream(of: AuthEvent.self, bufferingPolicy: .bufferingNewest(16))
    }

    deinit {
        continuation.finish()
    }

    /// The sign-in kept, or `nil` when signed out.
    public func current() async -> StoredTokens? {
        try? await store.load(issuer: issuer)
    }

    public func accessToken() async throws -> String {
        guard let tokens = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        if tokens.accessUsable(at: clock()) { return tokens.accessToken }
        try await refresh(after: tokens.accessToken)
        guard let fresh = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        return fresh.accessToken
    }

    public func refresh(after refused: String) async throws {
        if let running {
            // Someone is refreshing (or switching profile): its token is the new one.
            try await running.value
            return
        }
        try await exclusively { try await $0.performRefresh(refused) }
    }

    public func refusedAfterRefresh(_ token: String) async -> Bool {
        guard let last = lastIssued, last.token == token, clock().timeIntervalSince(last.at) <= Self.justIssued else {
            return false
        }
        // Another refresh since, or signed out already: nothing to end here.
        guard (try? await store.load(issuer: issuer))?.accessToken == token else { return false }
        await end(.ended)
        return true
    }

    public func connectionLeaked() async {
        Log.auth.error("a request with a token reached an address that isn't private: signed out")
        await end(.unencrypted)
    }

    /// Moves the sign-in to another profile of the household with Pley's
    /// switch-profile grant, with `pin` when the profile has one. A refusal
    /// spends nothing and keeps the tokens; it throws `PleyError.oauth`
    /// with the server's reason. A refresh token the server no longer
    /// knows ends the sign-in (`PleyError.signedOut`).
    public func switchProfile(_ profile: Int, pin: String?) async throws {
        while let running { _ = try? await running.value }
        try await exclusively { try await $0.performSwitch(profile, pin: pin) }
    }

    /// Ends this app's sign-in on the server (RFC 7009), then forgets the
    /// tokens, whatever the server answered.
    public func signOut() async {
        while let running { _ = try? await running.value }
        if let tokens = try? await store.load(issuer: issuer) {
            await Revocation.revoke(tokens, endpoints: endpoints)
        }
        try? await store.delete(issuer: issuer)
        lastIssued = nil
    }

    // MARK: Inside

    /// Runs `body` as the one refresh or switch; callers that come meanwhile wait for it.
    private func exclusively(_ body: @escaping @Sendable (isolated Tokens) async throws -> Void) async throws {
        let task = Task { try await body(self) }
        running = task
        defer { running = nil }
        try await task.value
    }

    private func performRefresh(_ refused: String) async throws {
        guard let tokens = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        // Refreshed meanwhile: nothing to send.
        if tokens.accessToken != refused && tokens.accessUsable(at: clock()) { return }
        guard let refresh = tokens.refreshToken, let url = URL(string: tokens.tokenEndpoint) else {
            await end(.ended)
            throw PleyError.signedOut
        }
        let data: Data
        do {
            data = try await endpoints.postForm(url, [("grant_type", "refresh_token"), ("refresh_token", refresh),
                                                      ("client_id", tokens.clientID)],
                                                onLeak: { [weak self] in await self?.connectionLeaked() })
        } catch PleyError.oauth(_, let body) where body.error == "invalid_grant" {
            await end(.ended)
            throw PleyError.signedOut
        }
        let answer = try TokenAnswer.parse(data, asked: tokens.scope)
        let fresh = answer.stored(like: tokens, now: clock(), byRefresh: true)
        if !ScopeGuard.extra(answer.scope, asked: tokens.scope).isEmpty { try await widened(fresh) }
        try await keep(fresh, replacing: refresh)
    }

    private func performSwitch(_ profile: Int, pin: String?) async throws {
        guard let tokens = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        guard let refresh = tokens.refreshToken, let url = URL(string: tokens.tokenEndpoint) else {
            await end(.ended)
            throw PleyError.signedOut
        }
        var form = [("grant_type", AppClient.switchProfileGrant), ("client_id", tokens.clientID),
                    ("refresh_token", refresh), ("profile", String(profile))]
        if let pin { form.append(("pin", pin)) }
        let data: Data
        do {
            data = try await endpoints.postForm(url, form, onLeak: { [weak self] in await self?.connectionLeaked() })
        } catch PleyError.oauth(_, let body) where body.error == "invalid_grant" && body.reason == nil {
            // Without a reason, the refresh token itself is spent or unknown.
            await end(.ended)
            throw PleyError.signedOut
        }
        let answer = try TokenAnswer.parse(data, asked: tokens.scope)
        let fresh = answer.stored(like: tokens, now: clock(), byRefresh: true)
        if !ScopeGuard.extra(answer.scope, asked: tokens.scope).isEmpty { try await widened(fresh) }
        try await keep(fresh, replacing: refresh)
    }

    /// Saves `fresh` before anything uses it: a refresh token works once.
    private func keep(_ fresh: StoredTokens, replacing refresh: String) async throws {
        // A sign-in that replaced these meanwhile wins; this grant was ended
        // with it, and what it just gave is ended too.
        if let now = try await store.load(issuer: issuer), now.refreshToken != refresh {
            await Revocation.revoke(fresh, endpoints: endpoints)
            return
        }
        try await store.save(fresh)
        lastIssued = (fresh.accessToken, clock())
    }

    /// Forgets the tokens and says why: the screens show sign-in.
    private func end(_ reason: SignOutReason) async {
        try? await store.delete(issuer: issuer)
        lastIssued = nil
        continuation.yield(.signedOut(issuer: issuer, reason: reason))
    }

    /// A refresh or switch gave more than this sign-in holds: the grant is
    /// ended with the new refresh token (the old one is spent), and the app
    /// is signed out.
    private func widened(_ fresh: StoredTokens) async throws -> Never {
        await Revocation.revoke(fresh, endpoints: endpoints)
        await end(.widerScope)
        throw AuthError.widerScope
    }
}
