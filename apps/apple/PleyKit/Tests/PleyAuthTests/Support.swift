import Foundation
import os
@testable import PleyAuth
import PleyCommon
@testable import PleyNetwork
import PleyTesting

let testAgent = "pleyApple/9.9.9 (tvOS 18.2; Apple TV)"

func transport() -> HTTPTransport {
    HTTPTransport(userAgent: testAgent)
}

func endpoints() -> EndpointClient {
    EndpointClient(transport: transport())
}

/// A started fixture server; `stop()` it at the end.
func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

/// The fixture's metadata, checked.
func metadata(_ server: FixtureServer) async throws -> ServerMetadata {
    try await ServerMetadata.fetch(server.origin, endpoints: endpoints())
}

/// Tokens the fixture gave, kept as a sign-in would keep them.
func signedIn(_ server: FixtureServer, store: any TokenStore, client: AppClient = .tvos,
              expiresIn: TimeInterval = 3600, now: Date = Date()) async throws -> FixtureServer.Tokens {
    let tokens = server.issueTokens()
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: client.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: now.addingTimeInterval(expiresIn), refreshToken: tokens.refreshToken,
        scope: FixtureServer.allScopes))
    return tokens
}

/// An API client for the fixture with `tokens`.
func api(_ server: FixtureServer, _ tokens: Tokens) -> PleyAPI {
    PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: transport()))
}

/// The sign-in page without a browser: asks the fixture's
/// `/oauth/authorize` and follows its `303` back to the app by hand.
/// `change` alters the answer before the app reads it.
struct FakeWebAuthenticator: WebAuthenticator {
    var change: @Sendable (URL) -> URL = { $0 }

    func authenticate(url: URL, callbackScheme: String) async throws -> URL {
        let response = try await transport().perform(URLRequest(url: url), redirects: .endpoints,
                                                     maxBytes: BodyLimit.small)
        guard response.status == 303, let location = response.header("Location"), let back = URL(string: location),
              back.scheme == callbackScheme
        else { throw AuthError.noBrowser }
        return change(back)
    }
}

/// `url` with its query parameter `name` set to `value`, or removed when `nil`.
func replacing(_ name: String, with value: String?, in url: URL) -> URL {
    var components = URLComponents(url: url, resolvingAgainstBaseURL: false)!
    var items = (components.queryItems ?? []).filter { $0.name != name }
    if let value { items.append(URLQueryItem(name: name, value: value)) }
    components.queryItems = items
    return components.url!
}

/// A clock the tests move, and a sleep that moves it instead of waiting.
final class FakeTime: Sendable {
    private let state: OSAllocatedUnfairLock<(now: Date, slept: [Duration])>
    /// Called after each sleep with how many sleeps there were.
    let afterSleep: @Sendable (Int) -> Void

    init(afterSleep: @escaping @Sendable (Int) -> Void = { _ in }) {
        state = OSAllocatedUnfairLock(initialState: (Date(timeIntervalSince1970: 1_800_000_000), []))
        self.afterSleep = afterSleep
    }

    var now: Date { state.withLock { $0.now } }
    var slept: [Duration] { state.withLock { $0.slept } }

    func advance(_ seconds: TimeInterval) {
        state.withLock { $0.now = $0.now.addingTimeInterval(seconds) }
    }

    var clock: @Sendable () -> Date { { self.now } }

    var sleep: @Sendable (Duration) async throws -> Void {
        { duration in
            let count = self.state.withLock { s -> Int in
                s.slept.append(duration)
                let (seconds, attoseconds) = duration.components
                s.now = s.now.addingTimeInterval(TimeInterval(seconds) + TimeInterval(attoseconds) / 1e18)
                return s.slept.count
            }
            self.afterSleep(count)
        }
    }
}
