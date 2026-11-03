import Foundation
@testable import PleyAuth
import PleyModel
@testable import PleyNetwork
import PleyTesting
import Testing

@Suite struct WebSignInTests {
    private func signIn(_ server: FixtureServer, store: any TokenStore = MemoryTokenStore(),
                        change: @escaping @Sendable (URL) -> URL = { $0 }) async throws -> StoredTokens {
        try await WebSignIn(client: .ios).run(origin: server.origin, metadata: try await metadata(server),
                                              authenticator: FakeWebAuthenticator(change: change), store: store,
                                              endpoints: endpoints())
    }

    @Test func signingInKeepsTheTokens() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let tokens = try await signIn(server, store: store)
        #expect(tokens.issuer == server.origin)
        #expect(tokens.clientID == AppClient.ios.clientID)
        #expect(tokens.scope == FixtureServer.allScopes)
        #expect(tokens.refreshToken != nil)
        #expect(try await store.load(issuer: server.origin) == tokens)
        #expect(!String(describing: tokens).contains(tokens.accessToken))

        let page = try #require(server.requests("/oauth/authorize").first)
        #expect(page.query["response_type"] == "code")
        #expect(page.query["client_id"] == AppClient.ios.clientID)
        #expect(page.query["redirect_uri"] == "app.pley.apple:/oauth/callback")
        #expect(page.query["code_challenge_method"] == "S256")
        #expect(page.query["code_challenge"]?.count == 43)
        #expect(page.query["scope"] == FixtureServer.allScopes)
        #expect(page.query["state"]?.count == 43)
        let exchange = try #require(server.requests("/api/oauth/token").first)
        #expect(exchange.grantType == "authorization_code")
        #expect(exchange.formKeys == ["client_id", "code", "code_verifier", "grant_type", "redirect_uri"])

        let me = try await api(server, Tokens(issuer: server.origin, store: store, endpoints: endpoints())).me()
        #expect(me.name == "Alex")
    }

    @Test func theAuthorizationAddressHasEachParameterOnce() throws {
        let endpoint = URL(string: "http://192.168.1.20:7539/oauth/authorize")!
        let url = try WebSignIn(client: .macos).authorizationURL(endpoint, verifier: "v", state: "s",
                                                                 scope: "library play")
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)!.queryItems!
        #expect(Set(items.map(\.name)).count == items.count)
        #expect(items.count == 7)
        #expect(url.absoluteString.contains("redirect_uri=app.pley.apple%3A%2Foauth%2Fcallback"))
        #expect(url.absoluteString.contains("scope=library%20play"))
    }

    @Test func anAnswerToAnotherSignInIsRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        await #expect(throws: AuthError.stateMismatch) {
            try await signIn(server) { replacing("state", with: "someone-elses", in: $0) }
        }
        #expect(server.requests("/api/oauth/token").isEmpty)
    }

    @Test func theAnswerMustNameTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        await #expect(throws: AuthError.otherServer) {
            try await signIn(server) { replacing("iss", with: nil, in: $0) }
        }
        await #expect(throws: AuthError.otherServer) {
            try await signIn(server) { replacing("iss", with: "http://192.168.1.99:7539", in: $0) }
        }
        #expect(server.requests("/api/oauth/token").isEmpty)
    }

    @Test func aRefusalSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setAuthorizeError("access_denied")
        await #expect(throws: AuthError.refused) { try await signIn(server) }
        server.setAuthorizeError("server_error")
        await #expect(throws: AuthError.serverRefused("server_error")) { try await signIn(server) }
    }

    @Test func aCodeTheServerRefusesFailsTheSignIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        await #expect(throws: AuthError.tookTooLong) {
            try await signIn(server) { replacing("code", with: "not-a-code", in: $0) }
        }
    }

    @Test func theCodeNeedsItsVerifier() async throws {
        let server = try await fixture()
        defer { server.stop() }
        // The fixture checks S256 as the server does: the code works only with its verifier.
        let verifier = PKCE.verifier()
        let m = try await metadata(server)
        let page = try WebSignIn(client: .ios).authorizationURL(m.authorizationEndpoint, verifier: verifier,
                                                                state: "s", scope: "library")
        let back = try await FakeWebAuthenticator().authenticate(url: page, callbackScheme: "app.pley.apple")
        let code = try WebSignIn.code(in: back, state: "s", issuer: m.issuer)
        let form: (String) -> [(String, String)] = { v in
            [("grant_type", "authorization_code"), ("code", code), ("client_id", AppClient.ios.clientID),
             ("code_verifier", v), ("redirect_uri", AppClient.redirectURI)]
        }
        await #expect(throws: PleyError.self) { try await endpoints().postForm(m.tokenEndpoint, form(PKCE.verifier())) }
        let answer = try await endpoints().postForm(m.tokenEndpoint, form(verifier))
        #expect(try TokenAnswer.parse(answer, asked: "library").scope == "library")
    }

    @Test func plainHttpBeyondTheHomeNetworkIsRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let m = try await metadata(server)
        await #expect(throws: AuthError.unencrypted) {
            try await WebSignIn(client: .ios).run(origin: "http://pley.example.com", metadata: m,
                                                  authenticator: FakeWebAuthenticator(), store: MemoryTokenStore(),
                                                  endpoints: endpoints())
        }
        #expect(server.requests("/oauth/authorize").isEmpty)
    }

    @Test func signingInAgainEndsTheSignInItReplaces() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let first = try await signIn(server, store: store)
        let second = try await signIn(server, store: store)
        #expect(first.refreshToken != second.refreshToken)
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == second)
    }

    @Test func aWiderScopeIsRevokedAndRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setWiderScope()
        let store = MemoryTokenStore()
        await #expect(throws: AuthError.widerScope) { try await signIn(server, store: store) }
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == nil)
    }
}
