import CryptoKit
import Foundation
import os
@testable import PleyAuth
import PleyModel
@testable import PleyNetwork
import PleyTesting
import Testing

/// The server's proofs (docs/API.md, "Telling servers apart"), as the
/// Android app's `ServerProofTest` and `TokensTest` check them.
@Suite struct ServerProofTests {
    // MARK: Reading and computing

    /// The known answers every app checks (packages/tv/test/fixtures/auth.json, "serverProof").
    @Test(arguments: [
        ("http://192.168.1.20:7539", "AAECAwQFBgcICQoLDA0ODw", "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM"),
        ("https://pley.example.com", "AAECAwQFBgcICQoLDA0OEA", "zHpBDBWP9YET2oKU1zMoY50w77pLxVJDXPKyJZVOHuA"),
        ("http://[fd00::5]:7539", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
         "Hy76L1n79Vkst4fC8d1JqvXMZ7rY3MhMKArRSR57AQ4"),
    ])
    func theProofIsTheServersHMAC(address: String, challenge: String, expected: String) throws {
        let proof = try #require(ServerProof(serverId: "0123456789abcdef0123456789abcdef01234567", proofId: "17",
                                             proofKey: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"))
        let key = try #require(proof.key)
        #expect(ServerProofs.proof(key: key, serverId: proof.serverId, address: address, challenge: challenge)
            == expected)
    }

    @Test func aTokenAnswerKeepsOnlyAProofWrittenAsItShouldBe() throws {
        let id = "0123456789abcdef0123456789abcdef01234567"
        let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
        func proof(_ fields: String) throws -> ServerProof? {
            let text = #"{"access_token":"pley_at_a","token_type":"Bearer",\#(fields)}"#
            return ServerProof(try TokenAnswer.parse(Data(text.utf8), asked: "library"))
        }
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17","proof_key":"\#(key)""#)?.proofId == "17")
        // Each of these leaves the answer without a proof, but still an answer.
        #expect(try proof(#""server_id":"\#(id)","proof_id":"017","proof_key":"\#(key)""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":17,"proof_key":"\#(key)""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17\n","proof_key":"\#(key)""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17","proof_key":"AQEBAQEBAQEBAQEBAQEBAQ""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17","proof_key":"\#(key)=""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17","proof_key":"+QEB+QEB+QEB+QEB+QEB+QEB+QEB+QEB+QEB+QEB+QE""#)
            == nil)
        #expect(try proof(#""server_id":"","proof_id":"17","proof_key":"\#(key)""#) == nil)
        #expect(try proof(#""server_id":"\#(id)","proof_id":"17","proof_key":null"#) == nil)
        #expect(try proof(#""scope":"library""#) == nil)
    }

    @Test func neitherTheKeyNorTheTokensArePrinted() throws {
        let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
        let proof = try #require(ServerProof(serverId: "s", proofId: "1", proofKey: key))
        #expect(!String(describing: proof).contains(key))
        let stored = StoredTokens(issuer: "http://192.168.1.20:7539", clientID: AppClient.ios.clientID,
                                  tokenEndpoint: "x", revocationEndpoint: "y", accessToken: "a", accessExpiresAt: Date(),
                                  refreshToken: "r", scope: "library", proof: proof)
        #expect(!String(describing: stored).contains(key))
        #expect(!String(describing: TokenResponse(accessToken: "a", proofKey: key)).contains(key))
    }

    @Test func aSignInKeptBeforeProofsOrWithOneWrittenWrongReadsWithout() throws {
        let old = #"{"issuer":"http://nas:7539","clientID":"c","tokenEndpoint":"t","revocationEndpoint":"r","#
            + #""accessToken":"a","accessExpiresAt":0,"refreshToken":"b","scope":"library"}"#
        #expect(try JSONDecoder().decode(StoredTokens.self, from: Data(old.utf8)).proof == nil)
        let wrong = old.dropLast() + #","proof":{"serverId":"s","proofId":"0","proofKey":"k"}}"#
        let read = try JSONDecoder().decode(StoredTokens.self, from: Data(wrong.utf8))
        #expect(read.proof == nil && read.accessToken == "a")
    }

    // MARK: Before a token goes

    @Test func aRightProofComesBeforeTheToken() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let given = try await provedSignIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        #expect(try await api(server, tokens).me().name == "Alex")
        #expect(try await api(server, tokens).me().name == "Alex")
        // One proof for both, asked with no token, before the first token went.
        let asked = server.journal.filter { $0.path == "/api/server" || $0.path == "/api/auth/me" }
        #expect(asked.map(\.path) == ["/api/server", "/api/auth/me", "/api/auth/me"])
        let proof = try #require(server.proofRequests.first)
        #expect(proof.headers["authorization"] == nil)
        #expect(proof.query["proof_id"] == String(given.grant))
        let challenge = try #require(proof.query["challenge"])
        #expect(challenge.count == 22 && !challenge.contains("="))
    }

    @Test(arguments: [FixtureServer.ProofAnswer.wrongId, .wrongProof, .redirect, .forbidden])
    func anotherDeviceGetsNoToken(_ answer: FixtureServer.ProofAnswer) async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        server.answerProofs(answer)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        await #expect(throws: PleyError.unreachable(PleyError.anotherDevice)) { try await api(server, tokens).me() }
        #expect(PleyError.unreachable(PleyError.anotherDevice).isAnotherDevice)
        #expect(server.journal.allSatisfy { $0.headers["authorization"] == nil })
        #expect(server.requests("/api/auth/me").isEmpty)
        // A redirect isn't followed: the proof counts only from the address asked.
        #expect(server.proofRequests.count == 1)
        // The sign-in is kept: the app shows what it saved and tries again later.
        #expect(try await store.load(issuer: server.origin) != nil)
    }

    @Test func aServerErrorIsOutOfReachNotAnotherDevice() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        server.answerProofs(.unavailable)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        do {
            _ = try await api(server, tokens).me()
            Issue.record("a token went without a proof")
        } catch let error as PleyError {
            guard case .unreachable = error else { throw error }
            #expect(!error.isAnotherDevice)
        }
        #expect(server.requests("/api/auth/me").isEmpty)
        // Back again: the token goes once the server proves itself.
        server.answerProofs(.right)
        #expect(try await api(server, tokens).me().name == "Alex")
    }

    @Test func aProofLastsHalfAnHour() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let time = FakeTime()
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store, expiresIn: 3 * 3600, now: time.now)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), clock: time.clock,
                            networks: NetworkChanges())
        _ = try await tokens.accessToken()
        time.advance(29 * 60)
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 1)
        time.advance(2 * 60)
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 2)
        // A clock set back doesn't stretch one.
        time.advance(-3600)
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 3)
    }

    @Test func anotherNetworkAsksForANewProof() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        let networks = NetworkChanges()
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: networks)
        _ = try await tokens.accessToken()
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 1)
        networks.changed()
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 2)
    }

    @Test func aPathCountsAsAChangeOnlyWhenItDiffers() {
        let networks = NetworkChanges()
        let heard = OSAllocatedUnfairLock(initialState: 0)
        networks.onChange { heard.withLock { $0 += 1 } }
        networks.saw("satisfied en0:wifi 192.168.1.1")
        networks.saw("satisfied en0:wifi 192.168.1.1")
        #expect(networks.current == 0)
        networks.saw("satisfied en0:wifi 10.0.0.1")
        networks.saw("unsatisfied  ")
        #expect(networks.current == 2)
        #expect(heard.withLock { $0 } == 2)
    }

    @Test func aRequestWithNoAnswerDoubtsTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        _ = try await tokens.accessToken()
        await tokens.doubt()
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 2)

        // The API client doubts when nothing answers at the address.
        let gone = try await fixture()
        let origin = gone.origin
        gone.stop()
        let doubts = Doubts(origin)
        let client = PleyAPI(client: APIClient(origin: origin, tokens: doubts, transport: transport()))
        await #expect(throws: PleyError.self) { try await client.me() }
        #expect(await doubts.count == 1)
    }

    @Test func aRefreshWaitsForAProofMadeThen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        _ = try await api(server, tokens).me()
        server.expireAccessTokens()
        _ = try await api(server, tokens).me()
        // The half-hour proof didn't do for the refresh token: a new one came right before it.
        let asked = server.journal.map(\.path).filter { $0 == "/api/server" || $0 == "/api/oauth/token" }
        #expect(asked == ["/api/server", "/api/server", "/api/oauth/token"])
    }

    @Test func aTokenDueForARefreshWaitsForOneProof() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store, expiresIn: 30)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        _ = try await tokens.accessToken()
        #expect(server.proofRequests.count == 1)
        #expect(server.requests("/api/oauth/token").count == 1)
    }

    @Test func aRefreshTokenGoesNowhereThatCantProveItself() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let given = try await provedSignIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        _ = try await api(server, tokens).me()
        server.expireAccessTokens()
        server.answerProofs(.wrongProof)
        await #expect(throws: PleyError.unreachable(PleyError.anotherDevice)) { try await api(server, tokens).me() }
        #expect(server.requests("/api/oauth/token").isEmpty)
        #expect(try await store.load(issuer: server.origin)?.refreshToken == given.refreshToken)
    }

    @Test func aProfileSwitchWaitsForAProofMadeThen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        let switcher = ProfileSwitcher(api: api(server, tokens), tokens: tokens)
        _ = try await switcher.profiles()
        server.answerProofs(.wrongId)
        await #expect(throws: PleyError.unreachable(PleyError.anotherDevice)) {
            try await switcher.switchTo(3, pin: "1234")
        }
        #expect(server.requests("/api/oauth/token").isEmpty)
        server.answerProofs(.right)
        try await switcher.switchTo(3, pin: "1234")
        #expect(try await switcher.api.me().name == "Sam")
        let asked = server.journal.map(\.path).filter { $0 == "/api/server" || $0 == "/api/oauth/token" }
        #expect(asked == ["/api/server", "/api/server", "/api/server", "/api/oauth/token"])
    }

    @Test func signingOutRevokesOnlyWhereTheServerProvesItself() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await provedSignIn(server, store: store)
        server.answerProofs(.forbidden)
        await Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges()).signOut()
        #expect(server.requests("/api/oauth/revoke").isEmpty)
        #expect(try await store.load(issuer: server.origin) == nil)

        _ = try await provedSignIn(server, store: store)
        server.answerProofs(.right)
        await Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges()).signOut()
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(server.proofRequests.count == 2)
    }

    @Test func aSignInKeepsItsFirstKey() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        let signedIn = try await WebSignIn(client: .ios)
            .run(origin: server.origin, metadata: try await metadata(server), authenticator: FakeWebAuthenticator(),
                 store: store, endpoints: endpoints())
        let first = try #require(signedIn.proof)
        #expect(first.serverId == FixtureServer.serverId)
        server.giveOtherKeysLater()
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        server.expireAccessTokens()
        _ = try await api(server, tokens).me()
        let refreshed = try #require(try await store.load(issuer: server.origin))
        #expect(refreshed.refreshToken != signedIn.refreshToken)
        #expect(refreshed.proof == first)
        #expect(server.proofRequests.map { $0.query["proof_id"] } == [first.proofId, first.proofId])
    }

    @Test func aSignInFromBeforeProofsSendsAsBeforeAndTakesTheNextKey() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        _ = try await signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints(), networks: NetworkChanges())
        _ = try await api(server, tokens).me()
        #expect(server.proofRequests.isEmpty)
        server.expireAccessTokens()
        _ = try await api(server, tokens).me()
        // The refresh went as before, and brought this sign-in's key: its
        // access token waited for the server to prove itself.
        let asked = server.journal.map(\.path).filter { $0 == "/api/server" || $0 == "/api/oauth/token" }
        #expect(asked == ["/api/oauth/token", "/api/server"])
        #expect(try await store.load(issuer: server.origin)?.proof != nil)
        _ = try await api(server, tokens).me()
        #expect(server.proofRequests.count == 1)
    }

    @Test func aNewSignInEndsTheOldOneOnlyWhereItsServerProvesIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        func signIn() async throws -> StoredTokens {
            try await WebSignIn(client: .ios)
                .run(origin: server.origin, metadata: try await metadata(server),
                     authenticator: FakeWebAuthenticator(), store: store, endpoints: endpoints())
        }
        _ = try await signIn()
        server.answerProofs(.wrongProof)
        _ = try await signIn()
        #expect(server.requests("/api/oauth/revoke").isEmpty)
        server.answerProofs(.right)
        _ = try await signIn()
        #expect(server.requests("/api/oauth/revoke").count == 1)
    }
}

/// Counts the doubts the API client has of its server.
private actor Doubts: TokenProvider {
    nonisolated let issuer: String
    private(set) var count = 0
    init(_ issuer: String) { self.issuer = issuer }

    func accessToken() async throws -> String { "pley_at_x" }
    func refresh(after refused: String) async throws {}
    func refusedAfterRefresh(_ token: String) async -> Bool { false }
    func connectionLeaked() async {}
    func doubt() async { count += 1 }
}
