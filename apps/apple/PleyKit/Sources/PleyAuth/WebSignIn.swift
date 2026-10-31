import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Shows the server's sign-in page and returns the address the browser
/// came back with (`app.pley.apple:/oauth/callback?…`).
public protocol WebAuthenticator: Sendable {
    func authenticate(url: URL, callbackScheme: String) async throws -> URL
}

/// Signing in through the server's own page (OAuth 2.1 with PKCE, RFC
/// 8252): the person signs in there, with their password and two-step code,
/// which the app never sees, and allows the app; the browser comes back
/// with a code the app exchanges for tokens. Never in a web view.
public struct WebSignIn: Sendable {
    public let client: AppClient
    private let clock: @Sendable () -> Date

    public init(client: AppClient = .current, clock: @escaping @Sendable () -> Date = { Date() }) {
        self.client = client
        self.clock = clock
    }

    /// Signs in to the server at `origin`, whose checked metadata is
    /// `metadata`, and keeps the tokens in `store`. A sign-in it replaces
    /// on that server is ended.
    public func run(origin: String, metadata: ServerMetadata, authenticator: any WebAuthenticator,
                    store: any TokenStore, endpoints: EndpointClient) async throws -> StoredTokens {
        // A password typed into a page over plain http beyond the home network: never.
        guard let home = Addresses.origin(origin) else { throw AuthError.notAnAddress }
        if Servers.unencrypted(home) { throw AuthError.unencrypted }

        let verifier = PKCE.verifier()
        let state = PKCE.state()
        let scope = metadata.scope
        let url = try authorizationURL(metadata.authorizationEndpoint, verifier: verifier, state: state, scope: scope)
        let answer = try await authenticator.authenticate(url: url, callbackScheme: AppClient.redirectScheme)
        let code = try Self.code(in: answer, state: state, issuer: metadata.issuer)

        let data: Data
        do {
            data = try await endpoints.postForm(metadata.tokenEndpoint, [
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", client.clientID),
                ("code_verifier", verifier),
                ("redirect_uri", AppClient.redirectURI),
            ])
        } catch PleyError.oauth(_, let body) {
            // A code works once, for a minute.
            if body.error == "invalid_grant" { throw AuthError.tookTooLong }
            throw AuthError.serverRefused(plain(body.errorDescription ?? body.error))
        }
        let tokens = try TokenAnswer.parse(data, asked: scope)
        let fresh = tokens.stored(issuer: metadata.issuer, clientID: client.clientID,
                                  tokenEndpoint: metadata.tokenEndpoint.absoluteString,
                                  revocationEndpoint: metadata.revocationEndpoint.absoluteString, now: clock(),
                                  byRefresh: false)
        return try await keepNewSignIn(tokens, asked: scope, fresh: fresh, store: store, endpoints: endpoints)
    }

    /// The sign-in page's address (OAuth 2.1 §4.1.1), each parameter once.
    func authorizationURL(_ endpoint: URL, verifier: String, state: String, scope: String) throws -> URL {
        let query = EndpointClient.encode([
            ("response_type", "code"),
            ("client_id", client.clientID),
            ("redirect_uri", AppClient.redirectURI),
            ("scope", scope),
            ("state", state),
            ("code_challenge", PKCE.challenge(for: verifier)),
            ("code_challenge_method", "S256"),
        ])
        let base = endpoint.absoluteString
        guard let url = URL(string: base + (base.contains("?") ? "&" : "?") + query) else { throw AuthError.notAnAddress }
        return url
    }

    /// The code in the browser's answer `url`, after checking that it
    /// answers this sign-in (`state`) and comes from `issuer` (RFC 9207).
    static func code(in url: URL, state: String, issuer: String) throws -> String {
        guard url.scheme?.lowercased() == AppClient.redirectScheme, url.host == nil,
              url.path == AppClient.redirectPath,
              let parameters = parameters(of: url)
        else { throw AuthError.stateMismatch }
        guard parameters["state"] == state else { throw AuthError.stateMismatch }
        guard let iss = parameters["iss"], iss == issuer else { throw AuthError.otherServer }
        if let error = parameters["error"] {
            if error == "access_denied" { throw AuthError.refused }
            throw AuthError.serverRefused(plain(parameters["error_description"] ?? error))
        }
        guard let code = parameters["code"] else { throw AuthError.noCode }
        return code
    }

    /// The query's parameters, without empty ones; `nil` when one is given twice.
    private static func parameters(of url: URL) -> [String: String]? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        var out: [String: String] = [:]
        var seen = Set<String>()
        for item in components.queryItems ?? [] {
            guard seen.insert(item.name).inserted else { return nil }
            if let value = item.value, !value.isEmpty { out[item.name] = value }
        }
        return out
    }
}
