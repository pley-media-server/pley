import Foundation
@testable import PleyAuth
import PleyNetwork
import PleyTesting
import Testing

@Suite struct TokensTests {
    @Test func aRefusedTokenIsRefreshedAndTheNewOneKeptBeforeUse() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let first = try await signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        server.expireAccessTokens()
        #expect(try await api(server, tokens).me().name == "Alex")

        let kept = try #require(try await store.load(issuer: server.origin))
        #expect(kept.refreshToken != first.refreshToken)
        #expect(kept.accessToken != first.accessToken)
        #expect(kept.issuedByRefreshAt != nil)
        let refresh = try #require(server.requests("/api/oauth/token").first)
        #expect(refresh.grantType == "refresh_token")
        #expect(refresh.formKeys == ["client_id", "grant_type", "refresh_token"])
        // Refused, refreshed, then asked again with the new token.
        #expect(server.requests("/api/auth/me").map(\.token) == [1, 2])
    }

    @Test func aTokenAboutToEndIsRefreshedFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await signedIn(server, store: store, expiresIn: 30)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        _ = try await api(server, tokens).me()
        #expect(server.requests("/api/oauth/token").count == 1)
        #expect(server.requests("/api/auth/me").map(\.token) == [2])
    }

    @Test func callersAtOnceShareOneRefresh() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await signedIn(server, store: store, expiresIn: 30)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        let got = try await withThrowingTaskGroup(of: String.self) { group in
            for _ in 0..<5 { group.addTask { try await tokens.accessToken() } }
            return try await group.reduce(into: Set<String>()) { $0.insert($1) }
        }
        #expect(got.count == 1)
        #expect(server.requests("/api/oauth/token").count == 1)
    }

    @Test func aRefreshTokenUsedTwiceEndsTheSignIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let first = try await signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        server.expireAccessTokens()
        _ = try await api(server, tokens).me()

        // Someone else uses the spent refresh token: the server ends the whole grant.
        await #expect(throws: PleyError.self) {
            try await endpoints().postForm(URL(string: server.origin + "/api/oauth/token")!, [
                ("grant_type", "refresh_token"), ("refresh_token", first.refreshToken),
                ("client_id", AppClient.tvos.clientID),
            ])
        }
        // The fresh token is refused moments after its refresh: signed out.
        await #expect(throws: PleyError.signedOut) { try await api(server, tokens).me() }
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await firstEvent(tokens) == .signedOut(issuer: server.origin, reason: .ended))
    }

    @Test func invalidGrantOnARefreshSignsOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let given = try await signedIn(server, store: store, expiresIn: 30)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        // The grant was ended elsewhere (Account → Apps).
        _ = try await endpoints().postForm(URL(string: server.origin + "/api/oauth/revoke")!,
                                           [("token", given.refreshToken), ("client_id", AppClient.tvos.clientID)])
        await #expect(throws: PleyError.signedOut) { try await tokens.accessToken() }
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await firstEvent(tokens) == .signedOut(issuer: server.origin, reason: .ended))
    }

    @Test func aWiderScopeOnARefreshSignsOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await signedIn(server, store: store, expiresIn: 30)
        server.setWiderScope()
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        await #expect(throws: AuthError.widerScope) { try await tokens.accessToken() }
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await firstEvent(tokens) == .signedOut(issuer: server.origin, reason: .widerScope))
    }

    @Test func aLeakedConnectionSignsOutOnTheDevice() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        await tokens.connectionLeaked()
        #expect(try await store.load(issuer: server.origin) == nil)
        #expect(await firstEvent(tokens) == .signedOut(issuer: server.origin, reason: .unencrypted))
        await #expect(throws: PleyError.signedOut) { try await tokens.accessToken() }
    }

    @Test func signingOutRevokes() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let given = try await signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        await tokens.signOut()
        let revoked = try #require(server.requests("/api/oauth/revoke").first)
        #expect(revoked.formKeys == ["client_id", "token"])
        #expect(try await store.load(issuer: server.origin) == nil)
        // The server no longer takes the old access token.
        let plain = PleyAPI(client: APIClient(origin: server.origin, tokens: Fixed(server.origin, given.accessToken),
                                              transport: transport()))
        await #expect(throws: PleyError.signedOut) { try await plain.me() }
    }

    @Test func storedTokensNeverPrintTheirTokens() {
        let t = StoredTokens(issuer: "http://192.168.1.20:7539", clientID: AppClient.ios.clientID,
                             tokenEndpoint: "x", revocationEndpoint: "y", accessToken: "pley_at_secret",
                             accessExpiresAt: Date(), refreshToken: "pley_rt_secret", scope: "library")
        #expect(!String(describing: t).contains("secret"))
        #expect(!String(reflecting: t).contains("secret"))
    }
}

/// The first event of `tokens`: they are kept until read.
func firstEvent(_ tokens: Tokens) async -> AuthEvent? {
    var events = tokens.events.makeAsyncIterator()
    return await events.next()
}

/// One token, never refreshed: for checking what the server takes.
struct Fixed: TokenProvider {
    let issuer: String
    let token: String
    init(_ issuer: String, _ token: String) {
        self.issuer = issuer
        self.token = token
    }

    func accessToken() async throws -> String { token }
    func refresh(after refused: String) async throws { throw PleyError.signedOut }
    func refusedAfterRefresh(_ token: String) async -> Bool { true }
    func connectionLeaked() async {}
}
