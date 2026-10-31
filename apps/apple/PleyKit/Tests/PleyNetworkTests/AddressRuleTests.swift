import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

@Suite struct RedirectPolicyTests {
    private func url(_ text: String) -> URL { URL(string: text)! }

    @Test func theAPIFollowsOnlyWithinItsOrigin() {
        let api = RedirectPolicy.Mode.api(issuer: "http://nas:7539")
        #expect(RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("http://nas:7539/b"), mode: api))
        #expect(RedirectPolicy.decide(from: url("http://NAS:7539/a"), to: url("http://nas:7539/b?x=1"), mode: api))
        // http to https, another port, another host.
        #expect(!RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("https://nas:7539/b"), mode: api))
        #expect(!RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("https://nas/b"), mode: api))
        #expect(!RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("http://nas:8080/b"), mode: api))
        #expect(!RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("http://evil:7539/b"), mode: api))
    }

    @Test func httpsNeverGoesDownToHttp() {
        let api = RedirectPolicy.Mode.api(issuer: "https://pley.example.com")
        #expect(!RedirectPolicy.decide(from: url("https://pley.example.com/a"), to: url("http://pley.example.com/a"),
                                       mode: api))
        #expect(RedirectPolicy.decide(from: url("https://pley.example.com:443/a"), to: url("https://pley.example.com/b"),
                                      mode: api))
    }

    @Test func theEndpointsNeverFollow() {
        #expect(!RedirectPolicy.decide(from: url("http://nas:7539/a"), to: url("http://nas:7539/b"), mode: .endpoints))
    }
}

@Suite struct ConnectionGateTests {
    @Test func aNameOnTheInternetIsRefusedBeforeAnythingIsSent() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let gate = ConnectionGate(resolver: FakeResolver(answer: ["8.8.8.8"]))
        // localhost reaches the fixture if anything goes out.
        let port = try #require(URLComponents(string: server.origin)?.port)
        let api = PleyAPI(client: APIClient(origin: "http://localhost:\(port)", tokens: nil, transport: transport(gate)))
        await #expect(throws: PleyError.unencrypted) { try await api.server() }
        #expect(server.journal.isEmpty)
    }

    @Test func everyAddressOfANameMustBePrivate() async throws {
        let gate = ConnectionGate(resolver: FakeResolver(answer: ["192.168.1.2", "93.184.216.34"]))
        await #expect(throws: PleyError.unencrypted) { try await gate.check(host: "nas") }
        try await ConnectionGate(resolver: FakeResolver(answer: ["192.168.1.2", "fd12::1"])).check(host: "nas")
        await #expect(throws: PleyError.unencrypted) {
            try await ConnectionGate(resolver: FakeResolver(answer: [])).check(host: "nas")
        }
    }

    @Test func addressesAreCheckedWithoutALookup() async throws {
        let gate = ConnectionGate(resolver: NoResolver())
        try await gate.check(host: "192.168.1.20")
        try await gate.check(host: "[fd12::1]")
        try await gate.check(host: "127.0.0.1")
        await #expect(throws: PleyError.unencrypted) { try await gate.check(host: "8.8.8.8") }
        await #expect(throws: PleyError.unencrypted) { try await gate.check(host: "[2001:db8::1]") }
    }

    @Test func httpsIsNotGated() async throws {
        // The resolver would fail: https to a name on the internet never asks it.
        let http = transport(ConnectionGate(resolver: NoResolver()))
        let client = APIClient(origin: "https://127.0.0.1:1", tokens: nil, transport: http)
        await #expect(throws: PleyError.self) {
            do {
                _ = try await client.get("/api/server", as: ServerInfo.self)
            } catch let error as PleyError {
                #expect(error != .unencrypted)
                throw error
            }
        }
    }

    @Test func theConnectionsAddressIsChecked() throws {
        let gate = ConnectionGate()
        #expect(throws: PleyError.unencrypted) { try gate.verify(remoteAddress: "93.184.216.34") }
        #expect(throws: PleyError.unencrypted) { try gate.verify(remoteAddress: nil) }
        #expect(throws: PleyError.unencrypted) { try gate.verify(remoteAddress: "::ffff:8.8.8.8") }
        try gate.verify(remoteAddress: "127.0.0.1")
        try gate.verify(remoteAddress: "::1")
        try gate.verify(remoteAddress: "fe80::1%en0")
        try gate.verify(remoteAddress: "192.168.1.20")
    }

    @Test func anAnswerFromAnAddressThatIsntPrivateIsThrownAwayAndTheSignInEnds() async throws {
        let server = try await fixture()
        defer { server.stop() }
        // The name looks private before the request; the connection then
        // reaches loopback, which this gate calls public.
        let gate = ConnectionGate(resolver: FakeResolver(answer: ["10.0.0.5"]), isPrivate: { $0 == "10.0.0.5" })
        let http = transport(gate)
        let port = try #require(URLComponents(string: server.origin)?.port)
        actor Leaky: TokenProvider {
            nonisolated let issuer: String
            let token: String
            var leaked = false
            init(_ issuer: String, _ token: String) {
                self.issuer = issuer
                self.token = token
            }
            func accessToken() async throws -> String { token }
            func refresh(after refused: String) async throws {}
            func refusedAfterRefresh(_ token: String) async -> Bool { false }
            func connectionLeaked() async { leaked = true }
        }
        let leaky = Leaky("http://localhost:\(port)", server.issueTokens().accessToken)
        let api = PleyAPI(client: APIClient(origin: "http://localhost:\(port)", tokens: leaky, transport: http))
        await #expect(throws: PleyError.unencrypted) { try await api.me() }
        #expect(await leaky.leaked)
        // It went out (that's what is checked after), and its answer was not used.
        #expect(server.requests("/api/auth/me").count == 1)
    }

    @Test func loopbackPassesBothChecks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let port = try #require(URLComponents(string: server.origin)?.port)
        let api = PleyAPI(client: APIClient(origin: "http://localhost:\(port)", tokens: nil, transport: transport()))
        #expect(try await api.server().name == "Living Room")
    }
}

@Suite struct EndpointClientTests {
    @Test func formsAreEncodedStrictly() {
        #expect(EndpointClient.encode([("grant_type", "refresh_token"), ("client_id", "https://a.example/b.json?x=1&y")])
            == "grant_type=refresh_token&client_id=https%3A%2F%2Fa.example%2Fb.json%3Fx%3D1%26y")
        #expect(EndpointClient.encode([("scope", "library play"), ("pin", "~1.2_3-")]) == "scope=library%20play&pin=~1.2_3-")
    }

    @Test func oauthRefusalsAreRead() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let endpoints = EndpointClient(transport: transport())
        let url = try #require(URL(string: server.origin + "/api/oauth/token"))
        await #expect(throws: PleyError.oauth(status: 400, OAuthErrorBody(error: "invalid_grant",
                                                                         errorDescription: "the sign-in has ended"))) {
            _ = try await endpoints.postForm(url, [("grant_type", "refresh_token"), ("refresh_token", "nope")])
        }
        let asked = try #require(server.requests("/api/oauth/token").first)
        #expect(asked.headers["content-type"] == "application/x-www-form-urlencoded")
        #expect(asked.formKeys == ["grant_type", "refresh_token"])
    }

    @Test func noRedirectIsFollowed() async throws {
        let server = try await fixture()
        let other = try await fixture()
        defer { server.stop(); other.stop() }
        server.redirect(path: "/api/oauth/token", to: other.origin + "/api/oauth/token", status: 307)
        let endpoints = EndpointClient(transport: transport())
        let url = try #require(URL(string: server.origin + "/api/oauth/token"))
        await #expect(throws: PleyError.redirected(location: other.origin + "/api/oauth/token")) {
            _ = try await endpoints.postForm(url, [("grant_type", "refresh_token"), ("refresh_token", "x")])
        }
        #expect(other.journal.isEmpty)
        let metadata = try #require(URL(string: server.origin + "/.well-known/oauth-authorization-server"))
        let data = try await endpoints.get(metadata)
        let document = try PleyJSON.decoder.decode(AuthorizationServerMetadata.self, from: data)
        #expect(document.issuer == server.origin)
    }

    @Test func aRefreshRotatesAndAReusedRefreshTokenEndsTheGrant() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let endpoints = EndpointClient(transport: transport())
        let url = try #require(URL(string: server.origin + "/api/oauth/token"))
        let tokens = server.issueTokens()
        let form = [("grant_type", "refresh_token"), ("refresh_token", tokens.refreshToken)]
        let answer = try PleyJSON.decoder.decode(TokenResponse.self, from: try await endpoints.postForm(url, form))
        #expect(answer.refreshToken != tokens.refreshToken)
        await #expect(throws: PleyError.self) { _ = try await endpoints.postForm(url, form) }
        // The grant ended: the rotated refresh token is refused too.
        await #expect(throws: PleyError.self) {
            _ = try await endpoints.postForm(url, [("grant_type", "refresh_token"),
                                                   ("refresh_token", answer.refreshToken ?? "")])
        }
    }
}

@Suite struct ServerProbeTests {
    @Test func pleyAnswers() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let port = try #require(URLComponents(string: server.origin)?.port)
        let result = await ServerProbe(transport: transport()).probe("127.0.0.1:\(port)")
        #expect(result == .pley(origin: server.origin, ServerInfo(name: "Living Room", version: "0.42.0")))
        let asked = try #require(server.journal.first)
        #expect(asked.headers["authorization"] == nil)
        #expect(asked.headers["user-agent"] == testAgent)
    }

    @Test func httpsIsTriedFirstThenHttp() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let secure = server.origin.replacingOccurrences(of: "http://", with: "https://")
        let result = await ServerProbe(transport: transport()).probe(candidates: [secure, server.origin])
        #expect(result == .pley(origin: server.origin, ServerInfo(name: "Living Room", version: "0.42.0")))
    }

    @Test func aRedirectToHttpsOnTheSameHostIsTriedNext() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let secure = server.origin.replacingOccurrences(of: "http://", with: "https://")
        server.redirect(path: "/api/server", to: secure + "/api/server", status: 308)
        // The fixture has no https: the upgrade is tried, and fails as a connection.
        let result = await ServerProbe(transport: transport()).probe(candidates: [server.origin])
        guard case .unreachable = result else {
            Issue.record("expected the https address to be tried, got \(result)")
            return
        }
        #expect(server.journal.count == 1)
    }

    @Test func aRedirectElsewhereIsNotPley() async throws {
        let server = try await fixture()
        let other = try await fixture()
        defer { server.stop(); other.stop() }
        server.redirect(path: "/api/server", to: other.origin + "/api/server")
        let result = await ServerProbe(transport: transport()).probe(candidates: [server.origin])
        #expect(result == .notPley(origin: server.origin))
        #expect(other.journal.isEmpty)
    }

    @Test func otherAnswersAreNotPley() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let probe = ServerProbe(transport: transport())
        server.answer(path: "/api/server", status: 200, json: #"{"hello":"router"}"#)
        #expect(await probe.probe(candidates: [server.origin]) == .notPley(origin: server.origin))
        server.answer(path: "/api/server", status: 200, json: #"{"name":"x","version":" "}"#)
        #expect(await probe.probe(candidates: [server.origin]) == .notPley(origin: server.origin))
        server.answer(path: "/api/server", status: 200, headers: ["Content-Type": "text/html"],
                      body: Data("<html>router</html>".utf8))
        #expect(await probe.probe(candidates: [server.origin]) == .notPley(origin: server.origin))
    }

    @Test func nothingAnswering() async throws {
        let server = try await fixture()
        let origin = server.origin
        server.stop()
        let probe = ServerProbe(transport: transport())
        guard case .unreachable = await probe.probe(candidates: [origin]) else {
            Issue.record("a closed port answered")
            return
        }
        #expect(await probe.probe("not an address") == .unreachable("that isn't a server address"))
    }
}
