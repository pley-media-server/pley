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
/// - No token leaves before the server at the issuer proves it holds this
///   sign-in's key (docs/API.md, "Telling servers apart"): for the access
///   token, a proof made within the half hour on this network, unless a
///   request there got no answer since; for a refresh, a profile switch
///   and a sign-out, one made then. What can't prove itself is out of
///   reach: `PleyError.unreachable(PleyError.anotherDevice)`. A sign-in
///   kept before servers had keys has nothing to check, as on Android, and
///   takes the key its next refresh brings.
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
    private let networks: NetworkChanges
    /// The challenges' random bytes.
    private let random: @Sendable (Int) -> Data
    /// When, on which network (`networks`' count) and with which key the
    /// server last proved itself.
    private var provedAt: (at: Date, network: Int, proof: ServerProof)?
    /// The proof being made; others wait for it, then look again.
    private var proving: Task<Void, any Error>?

    public init(issuer: String, store: any TokenStore, endpoints: EndpointClient,
                clock: @escaping @Sendable () -> Date = { Date() }, networks: NetworkChanges = .shared,
                random: @escaping @Sendable (Int) -> Data = ServerProofs.randomBytes) {
        self.issuer = issuer
        self.store = store
        self.endpoints = endpoints
        self.clock = clock
        self.networks = networks
        self.random = random
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
        if tokens.accessUsable(at: clock()) {
            try await proved(tokens)
            return tokens.accessToken
        }
        // The refresh waits for a proof of its own: one before it would be wasted.
        try await refresh(after: tokens.accessToken)
        guard let fresh = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        // Given by another caller's refresh, maybe on another network since.
        try await proved(fresh)
        return fresh.accessToken
    }

    /// An access token that works for `lasting` seconds more, refreshed
    /// first when it ends sooner, and when it ends: a download's piece
    /// carries one for its whole transfer.
    public func accessToken(lasting: TimeInterval) async throws -> (token: String, expires: Date) {
        guard let tokens = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        if tokens.accessExpiresAt.timeIntervalSince(clock()) > max(lasting, Self.refreshAhead) {
            try await proved(tokens)
            return (tokens.accessToken, tokens.accessExpiresAt)
        }
        try await refresh(after: tokens.accessToken)
        guard let fresh = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        try await proved(fresh)
        return (fresh.accessToken, fresh.accessExpiresAt)
    }

    /// Returns once the server at the issuer has proved itself as the
    /// access token needs (within the half hour, on this network), asking
    /// for a proof when it hasn't: before something sends a token it got
    /// earlier, such as a download's piece going on from where it stopped.
    public func proveServer() async throws {
        guard let tokens = try await store.load(issuer: issuer) else { throw PleyError.signedOut }
        try await proved(tokens)
    }

    /// A request to the server got no answer: the device may be on another
    /// network, so the next token waits for a new proof.
    public func doubt() async {
        provedAt = nil
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
    /// tokens, whatever the server answered. A server that can't prove
    /// itself now gets no token: the person can sign the app out in the web
    /// app's Account → Apps.
    public func signOut() async {
        while let running { _ = try? await running.value }
        if let tokens = try? await store.load(issuer: issuer) {
            if await Revocation.proves(tokens, endpoints: endpoints, random: random) {
                await Revocation.revoke(tokens, endpoints: endpoints)
            } else {
                Log.auth.info("signed out on this device only: the server didn't prove itself")
            }
        }
        try? await store.delete(issuer: issuer)
        lastIssued = nil
        provedAt = nil
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
        // The refresh token goes only to a server that proves itself now.
        try await proved(tokens, now: true)
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
        // The refresh token and the PIN go only to a server that proves itself now.
        try await proved(tokens, now: true)
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

    /// Returns once the server at the issuer has proved it holds `tokens`'
    /// key: within the half hour, on this network and since the last
    /// request that got no answer; with `now`, made now (a refresh token or
    /// a PIN). One proof at a time: a caller that finds one under way waits
    /// for it, then looks again, as the network may have changed meanwhile.
    /// A sign-in without a key has nothing to check.
    private func proved(_ tokens: StoredTokens, now: Bool = false) async throws {
        guard let proof = tokens.proof else { return }
        if now { provedAt = nil }
        while !recentlyProved(proof) {
            if let proving {
                _ = try? await proving.value
                continue
            }
            let network = networks.current
            let (issuer, endpoints, random) = (issuer, endpoints, random)
            // The task notes its end on the actor before its value is
            // ready, so whoever waited for it finds the proof, or none, kept.
            let task = Task {
                do {
                    try await ServerProofs.prove(proof, issuer: issuer, endpoints: endpoints, random: random)
                    self.proofEnded(proof, network: network, proved: true)
                } catch {
                    self.proofEnded(proof, network: network, proved: false)
                    throw error
                }
            }
            proving = task
            try await task.value
        }
    }

    private func proofEnded(_ proof: ServerProof, network: Int, proved: Bool) {
        proving = nil
        if proved { provedAt = (clock(), network, proof) }
    }

    /// A clock set back makes an age below zero: proved again.
    private func recentlyProved(_ proof: ServerProof) -> Bool {
        guard let provedAt, provedAt.proof == proof, provedAt.network == networks.current else { return false }
        let age = clock().timeIntervalSince(provedAt.at)
        return age >= 0 && age < ServerProofs.lasting
    }

    /// Forgets the tokens and says why: the screens show sign-in.
    private func end(_ reason: SignOutReason) async {
        try? await store.delete(issuer: issuer)
        lastIssued = nil
        provedAt = nil
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
