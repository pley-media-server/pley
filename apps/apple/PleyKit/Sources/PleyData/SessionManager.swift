import Foundation
import PleyAuth
import PleyCommon
import PleyModel
import PleyNetwork

/// Where the app stands with the server it opened.
public enum SessionState: Sendable, Equatable {
    /// The server is Pley and can sign the app in; no sign-in is kept.
    case signedOut(ServerMetadata)
    /// Signed in, acting as `User`.
    case signedIn(User)
}

/// Something the screens hear about, wherever it happens.
public enum SessionEvent: Sendable, Equatable {
    /// The app was signed out of `origin` without being asked to; `message` says why.
    case signedOut(origin: String, message: String)
}

/// The server the app has open, and its sign-in: its `Tokens`, an
/// `APIClient` with them, the typed calls and the profile switcher. One
/// server at a time.
public actor SessionManager {
    /// Sign-ins that ended without the person asking, for the screens.
    public nonisolated let events: AsyncStream<SessionEvent>
    public nonisolated let client: AppClient

    private let continuation: AsyncStream<SessionEvent>.Continuation
    private let transport: HTTPTransport
    private let endpoints: EndpointClient
    private let tokenStore: any TokenStore
    private let servers: ServerRepository
    private let clock: @Sendable () -> Date
    private let sleep: @Sendable (Duration) async throws -> Void

    public private(set) var origin: String?
    public private(set) var serverID: String?
    public private(set) var serverInfo: ServerInfo?
    public private(set) var metadata: ServerMetadata?
    public private(set) var tokens: Tokens?
    /// The server's API: with the tokens when signed in, without before.
    public private(set) var api: PleyAPI?
    public private(set) var profiles: ProfileSwitcher?
    private var listener: Task<Void, Never>?

    public init(transport: HTTPTransport, tokenStore: any TokenStore = KeychainTokenStore(),
                servers: ServerRepository, client: AppClient = .current,
                clock: @escaping @Sendable () -> Date = { Date() },
                sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.transport = transport
        endpoints = EndpointClient(transport: transport)
        self.tokenStore = tokenStore
        self.servers = servers
        self.client = client
        self.clock = clock
        self.sleep = sleep
        (events, continuation) = AsyncStream.makeStream(of: SessionEvent.self, bufferingPolicy: .bufferingNewest(16))
    }

    deinit {
        listener?.cancel()
        continuation.finish()
    }

    /// Opens the server at `origin` (found on the network with `id`, or
    /// typed): checks it is Pley, reads where it signs apps in, and uses
    /// the sign-in kept for it, if any and the server still accepts it.
    public func open(origin: String, id: String? = nil) async throws -> SessionState {
        guard let home = Addresses.origin(origin) else { throw AuthError.notAnAddress }
        close()
        let plainAPI = PleyAPI(client: APIClient(origin: home, tokens: nil, transport: transport))
        let info = try await plainAPI.server()
        let metadata = try await ServerMetadata.fetch(home, endpoints: endpoints)
        self.origin = home
        serverID = id
        serverInfo = info
        self.metadata = metadata
        api = plainAPI
        guard try await tokenStore.load(issuer: metadata.issuer) != nil else { return .signedOut(metadata) }
        do {
            return .signedIn(try await adopt())
        } catch PleyError.signedOut {
            detach()
            return .signedOut(metadata)
        }
    }

    /// Signs in through the server's page (iPhone, iPad and Mac).
    public func signInWithBrowser(authenticator: any WebAuthenticator) async throws -> User {
        guard client.signsInWithBrowser else { throw AuthError.noBrowser }
        let (home, metadata) = try opened()
        _ = try await WebSignIn(client: client, clock: clock)
            .run(origin: home, metadata: metadata, authenticator: authenticator, store: tokenStore,
                 endpoints: endpoints)
        return try await adopt()
    }

    /// Signing in with a code for the open server. When
    /// `waitForApproval` returns, call `completeSignIn()`.
    public func deviceSignIn() throws -> DeviceSignIn {
        guard client.signsInWithCode else { throw AuthError.noCodes }
        let (_, metadata) = try opened()
        return DeviceSignIn(metadata: metadata, client: client, store: tokenStore, endpoints: endpoints, clock: clock,
                            sleep: sleep)
    }

    /// Uses the sign-in just kept for the open server (after a code was allowed).
    public func completeSignIn() async throws -> User {
        _ = try opened()
        return try await adopt()
    }

    /// Ends this app's sign-in on the server and forgets its tokens.
    public func signOut() async {
        await tokens?.signOut()
        detach()
    }

    /// Signs out, then removes the server from the list.
    public func forget() async throws {
        let origin = self.origin
        await signOut()
        if let origin { try await servers.forget(origin: origin) }
        close()
    }

    /// Signs out of the saved server at `origin`, open or not, then removes
    /// it from the list. A server that isn't open is signed out with the
    /// endpoints its sign-in was kept with, so nothing is fetched first.
    public func forget(origin: String) async throws {
        if origin == self.origin {
            try await forget()
            return
        }
        // A server's issuer is its origin (ServerMetadata checks it).
        await Tokens(issuer: origin, store: tokenStore, endpoints: endpoints, clock: clock).signOut()
        try await servers.forget(origin: origin)
    }

    // MARK: Inside

    private func opened() throws -> (String, ServerMetadata) {
        guard let origin, let metadata else { throw AuthError.notAnAddress }
        return (origin, metadata)
    }

    /// Uses the tokens kept for the open server: who they act as, and the
    /// server goes to the top of the list.
    private func adopt() async throws -> User {
        let (home, metadata) = try opened()
        if tokens == nil { attach(issuer: metadata.issuer, origin: home) }
        guard let api else { throw PleyError.signedOut }
        let user = try await api.me()
        try? await servers.remember(SavedServer(origin: home, name: serverInfo?.name ?? home, id: serverID,
                                                lastUsed: clock()))
        return user
    }

    private func attach(issuer: String, origin: String) {
        let tokens = Tokens(issuer: issuer, store: tokenStore, endpoints: endpoints, clock: clock)
        let api = PleyAPI(client: APIClient(origin: origin, tokens: tokens, transport: transport))
        self.tokens = tokens
        self.api = api
        profiles = ProfileSwitcher(api: api, tokens: tokens)
        listener?.cancel()
        listener = Task { [weak self] in
            for await event in tokens.events {
                await self?.heard(event, from: tokens)
            }
        }
    }

    private func heard(_ event: AuthEvent, from source: Tokens) {
        guard source === tokens, case .signedOut(_, let reason) = event, let origin else { return }
        detach()
        continuation.yield(.signedOut(origin: origin, message: reason.message))
    }

    /// Back to the open server without a sign-in.
    private func detach() {
        listener?.cancel()
        listener = nil
        tokens = nil
        profiles = nil
        api = origin.map { PleyAPI(client: APIClient(origin: $0, tokens: nil, transport: transport)) }
    }

    private func close() {
        detach()
        origin = nil
        serverID = nil
        serverInfo = nil
        metadata = nil
        api = nil
    }
}
