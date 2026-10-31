import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

@Suite struct APIClientTests {
    @Test func theUserAgentAndTheTokenGoToTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        let me = try await api.me()
        #expect(me.name == "Alex")
        #expect(me.scopes == ["library", "play", "playlists", "profiles"])
        let asked = try #require(server.requests("/api/auth/me").first)
        #expect(asked.headers["user-agent"] == testAgent)
        #expect(asked.token != nil)
        #expect(asked.headers["cookie"] == nil)
        #expect(asked.query.isEmpty)
    }

    @Test func theTypedCallsReadTheFixtures() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                            transport: http))
        #expect(try await api.server() == ServerInfo(name: "Living Room", version: "0.42.0"))
        let profiles = try await api.householdProfiles()
        #expect(profiles.map(\.name) == ["Alex", "Sam", "Kids", "Robin", "Guest"])
        #expect(profiles.map(\.needs) == [.none, .pin, .none, .password, .locked])
        #expect(profiles.first(where: \.current)?.name == "Alex")
        let home = try await api.home()
        #expect(!home.hubs.isEmpty)
    }

    @Test func noTokenGoesToAnotherOrigin() async throws {
        let home = try await fixture()
        let other = try await fixture()
        defer { home.stop(); other.stop() }
        let http = transport()
        let tokens = FakeTokens(server: home, transport: http)
        // A client for the other server, holding the home server's tokens.
        let client = APIClient(origin: other.origin, tokens: tokens, transport: http)
        _ = try await client.get("/api/server", as: ServerInfo.self)
        let asked = try #require(other.journal.first)
        #expect(asked.headers["authorization"] == nil)
    }

    @Test func aRedirectToAnotherOriginIsNotFollowed() async throws {
        let home = try await fixture()
        let other = try await fixture()
        defer { home.stop(); other.stop() }
        home.redirect(path: "/api/hubs/home", to: other.origin + "/api/hubs/home")
        let http = transport()
        let api = PleyAPI(client: APIClient(origin: home.origin, tokens: FakeTokens(server: home, transport: http),
                                            transport: http))
        await #expect(throws: PleyError.redirected(location: other.origin + "/api/hubs/home")) {
            try await api.home()
        }
        #expect(other.journal.isEmpty)
        #expect(!other.journal.contains { $0.headers["authorization"] != nil })
    }

    @Test func aRedirectFromHttpToHttpsIsNotFollowed() async throws {
        let home = try await fixture()
        defer { home.stop() }
        let secure = home.origin.replacingOccurrences(of: "http://", with: "https://")
        home.redirect(path: "/api/hubs/home", to: secure + "/api/hubs/home")
        let http = transport()
        let api = PleyAPI(client: APIClient(origin: home.origin, tokens: FakeTokens(server: home, transport: http),
                                            transport: http))
        await #expect(throws: PleyError.redirected(location: secure + "/api/hubs/home")) { try await api.home() }
        #expect(home.journal.count == 1)
    }

    @Test func aRedirectOnTheSameOriginIsFollowedWithTheToken() async throws {
        let home = try await fixture()
        defer { home.stop() }
        home.redirect(path: "/api/old-home", to: "/api/hubs/home", status: 307)
        let http = transport()
        let client = APIClient(origin: home.origin, tokens: FakeTokens(server: home, transport: http), transport: http)
        let hubs = try await client.get("/api/old-home", as: HomeHubs.self)
        #expect(!hubs.hubs.isEmpty)
        let followed = try #require(home.requests("/api/hubs/home").first)
        #expect(followed.token != nil)
    }

    @Test func anExpiredTokenIsRefreshedOnceAndTheRequestRetried() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        server.expireAccessTokens()
        #expect(try await api.me().name == "Alex")
        #expect(await tokens.refreshes == 1)
        let asked = server.requests("/api/auth/me")
        #expect(asked.count == 2)
        #expect(asked[0].token != asked[1].token)
    }

    @Test func fiveRequestsWithAnExpiredTokenRefreshOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        server.expireAccessTokens()
        let names = try await withThrowingTaskGroup(of: String.self) { group in
            for _ in 0..<5 { group.addTask { try await api.me().name } }
            return try await group.reduce(into: []) { $0.append($1) }
        }
        #expect(names == Array(repeating: "Alex", count: 5))
        #expect(server.journal.filter { $0.grantType == "refresh_token" }.count == 1)
        #expect(await tokens.refreshes == 1)
    }

    @Test func aRefusedRetryMeansSignedOut() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        server.refuseEveryToken()
        await #expect(throws: PleyError.signedOut) { try await api.me() }
        #expect(await tokens.refreshes == 1)
        #expect(server.requests("/api/auth/me").count == 2)
        // The token the refresh gave was refused too: the sign-in ended.
        #expect(await tokens.ended)
    }

    @Test func aTokenRefusedRightAfterARefreshEndsWithoutAnotherRefresh() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        await tokens.setRefuseAtOnce(true)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        server.expireAccessTokens()
        await #expect(throws: PleyError.signedOut) { try await api.me() }
        #expect(await tokens.refreshes == 0)
        #expect(server.requests("/api/auth/me").count == 1)
    }

    @Test func a401WithoutInvalidTokenIsTheAnswer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/auth/me", status: 401, headers: ["WWW-Authenticate": #"Bearer realm="pley""#],
                      json: #"{"code":"unauthorized","message":"no"}"#)
        let http = transport()
        let tokens = FakeTokens(server: server, transport: http)
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: tokens, transport: http))
        await #expect(throws: PleyError.api(status: 401, code: "unauthorized", message: "no")) { try await api.me() }
        #expect(await tokens.refreshes == 0)
    }

    @Test func refusalsAreRead() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let client = APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http), transport: http)
        server.answer(path: "/api/items/42", status: 404, body: try Fixtures.data("errors/not_found.json"))
        await #expect(throws: PleyError.api(status: 404, code: "not_found", message: "item 42 not found")) {
            try await client.get("/api/items/42", as: ItemSummary.self)
        }
        server.answer(path: "/api/play", status: 403,
                      headers: ["WWW-Authenticate": #"Bearer error="insufficient_scope", scope="play""#],
                      body: try Fixtures.data("errors/insufficient_scope.json"))
        await #expect(throws: PleyError.insufficientScope(scope: "play")) {
            try await client.get("/api/play", as: NoContent.self)
        }
        server.answer(path: "/api/sessions/7", status: 410,
                      json: #"{"code":"session_stopped","error":"session_stopped","message":"Bedtime‮!"}"#)
        await #expect(throws: PleyError.sessionStopped(message: "Bedtime!")) {
            try await client.get("/api/sessions/7", as: NoContent.self)
        }
        server.answer(path: "/api/odd", status: 500, body: Data("<html>oops</html>".utf8))
        await #expect(throws: PleyError.unreadable(status: 500)) {
            try await client.get("/api/odd", as: NoContent.self)
        }
    }

    @Test func aScopeTheTokenLacksIsInsufficientScope() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        actor LibraryOnly: TokenProvider {
            nonisolated let issuer: String
            let token: String
            init(_ server: FixtureServer) {
                issuer = server.origin
                token = server.issueTokens(scope: "library").accessToken
            }
            func accessToken() async throws -> String { token }
            func refresh(after refused: String) async throws {}
            func refusedAfterRefresh(_ token: String) async -> Bool { false }
            func connectionLeaked() async {}
        }
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: LibraryOnly(server), transport: http))
        await #expect(throws: PleyError.insufficientScope(scope: "profiles")) { try await api.householdProfiles() }
    }

    @Test func answersHaveLimits() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                            transport: http))
        // /api/server is read up to 64 KiB.
        server.answer(path: "/api/server", status: 200, headers: ["Content-Type": "application/json"],
                      body: Data(repeating: 0x20, count: 70 << 10))
        await #expect(throws: PleyError.tooLarge) { try await api.server() }
        // The API's JSON up to 8 MiB, with or without a length given.
        let big = Data(repeating: 0x20, count: 9 << 20)
        server.answer(path: "/api/hubs/home", status: 200, headers: ["Content-Type": "application/json"], body: big)
        await #expect(throws: PleyError.tooLarge) { try await api.home() }
        server.answer(path: "/api/hubs/home", status: 200, headers: ["Content-Type": "application/json"], body: big,
                      contentLength: false)
        await #expect(throws: PleyError.tooLarge) { try await api.home() }
        // Pictures up to 10 MiB.
        server.answer(path: "/api/items/1/images/poster", status: 200, headers: ["Content-Type": "image/png"],
                      body: Data(repeating: 0, count: 11 << 20), contentLength: false)
        await #expect(throws: PleyError.tooLarge) { try await api.image(path: "/api/items/1/images/poster", width: nil) }
    }

    @Test func picturesOfTheFourTypesOnly() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let http = transport()
        let api = PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                            transport: http))
        let png = try await api.image(path: "/api/items/12/images/poster?v=1729600000", width: 300)
        #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]))
        let asked = try #require(server.requests("/api/items/12/images/poster").first)
        #expect(asked.query == ["v": "1729600000", "w": "300"])
        let avatar = try await api.image(path: "/api/users/2/avatar?v=1761100000", width: nil)
        #expect(avatar.starts(with: [0xFF, 0xD8]))

        server.answer(path: "/api/items/13/images/poster", status: 200, headers: ["Content-Type": "image/svg+xml"],
                      body: Data("<svg/>".utf8))
        await #expect(throws: PleyError.unreadable(status: 200)) {
            try await api.image(path: "/api/items/13/images/poster", width: nil)
        }
        server.answer(path: "/api/items/14/images/poster", status: 200, headers: ["Content-Type": "text/html"],
                      body: Data("<html/>".utf8))
        await #expect(throws: PleyError.unreadable(status: 200)) {
            try await api.image(path: "/api/items/14/images/poster", width: nil)
        }
        await #expect(throws: PleyError.self) { try await api.image(path: "https://elsewhere.example/a.png", width: nil) }
    }

    @Test func pictureAddressesStayOnTheServer() {
        let api = PleyAPI(client: APIClient(origin: "http://192.168.1.20:7539", tokens: nil, transport: transport()))
        #expect(api.imageURL(path: "/api/items/12/images/poster?v=1729600000", width: 300)?.absoluteString
            == "http://192.168.1.20:7539/api/items/12/images/poster?v=1729600000&w=300")
        #expect(api.imageURL(path: "/api/users/2/avatar", width: 128)?.absoluteString
            == "http://192.168.1.20:7539/api/users/2/avatar?w=128")
        #expect(api.imageURL(path: "/api/users/2/avatar", width: nil)?.absoluteString
            == "http://192.168.1.20:7539/api/users/2/avatar")
        #expect(api.imageURL(path: "https://elsewhere.example/api/x.png", width: nil) == nil)
        #expect(api.imageURL(path: "//elsewhere.example/api/x.png", width: nil) == nil)
        #expect(api.imageURL(path: "/static/logo.svg", width: nil) == nil)
        #expect(api.imageURL(path: "/api/../admin", width: nil) == nil)
    }
}

@Suite struct BearerChallengeTests {
    // The Android app's TokenAuthenticator cases.
    @Test func invalidTokenChallengesAreRecognized() {
        #expect(BearerChallenge.isInvalidToken(#"Bearer error="invalid_token""#))
        #expect(BearerChallenge.isInvalidToken(#"bearer realm="pley", error="invalid_token", error_description="expired""#))
        #expect(!BearerChallenge.isInvalidToken(#"Bearer error="insufficient_scope""#))
        #expect(!BearerChallenge.isInvalidToken(#"Bearer error="invalid_token_x""#))
        #expect(!BearerChallenge.isInvalidToken(#"Basic realm="pley""#))
        #expect(!BearerChallenge.isInvalidToken(#"Bearerx error="invalid_token""#))
    }

    @Test func moreForms() {
        #expect(BearerChallenge.isInvalidToken("Bearer error=invalid_token"))
        #expect(BearerChallenge.isInvalidToken(#"  Bearer   error = "invalid_token" "#))
        #expect(!BearerChallenge.isInvalidToken("Bearer"))
        #expect(!BearerChallenge.isInvalidToken(#"Bearer xerror="invalid_token""#))
    }

    @Test func theScopeOfAnInsufficientScopeChallenge() {
        let header = #"Bearer error="insufficient_scope", scope="play""#
        #expect(BearerChallenge.isInsufficientScope(header))
        #expect(BearerChallenge.scope(header) == "play")
        #expect(BearerChallenge.scope(#"Bearer error="insufficient_scope""#) == nil)
    }
}
