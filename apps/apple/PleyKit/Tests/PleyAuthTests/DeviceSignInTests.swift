import Foundation
@testable import PleyAuth
import PleyNetwork
import PleyTesting
import Testing

@Suite struct DeviceSignInTests {
    private func device(_ server: FixtureServer, time: FakeTime, store: any TokenStore = MemoryTokenStore(),
                        client: AppClient = .tvos) async throws -> DeviceSignIn {
        DeviceSignIn(metadata: try await metadata(server), client: client, store: store, endpoints: endpoints(),
                     clock: time.clock, sleep: time.sleep)
    }

    @Test func aCodeIsShownAndAllowed() async throws {
        let server = try await fixture()
        defer { server.stop() }
        // Allowed on the server after the third poll.
        let time = FakeTime { if $0 == 3 { server.approveDevice() } }
        let store = MemoryTokenStore()
        let signIn = try await device(server, time: time, store: store)
        let code = try await signIn.start()
        #expect(code.userCode == "BCDF-GHJK")
        #expect(code.verificationURL.absoluteString == server.origin + "/link")
        #expect(code.completeURL?.absoluteString == server.origin + "/link?code=BCDFGHJK")
        #expect(code.expiresAt == time.now.addingTimeInterval(600))
        #expect(!String(describing: code).contains(code.deviceCode))
        let asked = try #require(server.requests("/api/oauth/device").first)
        #expect(asked.formKeys == ["client_id", "scope"])

        let tokens = try await signIn.waitForApproval(code)
        #expect(tokens.clientID == AppClient.tvos.clientID)
        #expect(tokens.scope == FixtureServer.allScopes)
        #expect(try await store.load(issuer: server.origin) == tokens)
        let polls = server.requests("/api/oauth/token")
        #expect(polls.count == 3)
        #expect(polls.allSatisfy { $0.grantType == AppClient.deviceCodeGrant })
        #expect(time.slept == [.seconds(5), .seconds(5), .seconds(5)])
    }

    @Test func slowDownWaitsFiveSecondsLongerForGood() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.slowDown(1)
        let time = FakeTime { if $0 == 3 { server.approveDevice() } }
        let signIn = try await device(server, time: time)
        _ = try await signIn.waitForApproval(try await signIn.start())
        #expect(time.slept == [.seconds(5), .seconds(10), .seconds(10)])
    }

    @Test func aRefusalEndsTheWait() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let time = FakeTime { if $0 == 2 { server.denyDevice() } }
        let signIn = try await device(server, time: time)
        await #expect(throws: AuthError.refused) { try await signIn.waitForApproval(try await signIn.start()) }
    }

    @Test func anExpiredCodeSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.expireDevice()
        let signIn = try await device(server, time: FakeTime())
        await #expect(throws: AuthError.expired) { try await signIn.waitForApproval(try await signIn.start()) }
    }

    @Test func aCodeThatRanOutIsNotAskedAbout() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let time = FakeTime()
        let signIn = try await device(server, time: time)
        let code = try await signIn.start()
        time.advance(601)
        await #expect(throws: AuthError.expired) { try await signIn.waitForApproval(code) }
        #expect(server.requests("/api/oauth/token").isEmpty)
    }

    @Test func theLinkMustBeOnTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/oauth/device", status: 200, json: """
            {"device_code":"d","user_code":"BCDFGHJK","verification_uri":"http://evil.example.com/link",
             "expires_in":600,"interval":5}
            """)
        let signIn = try await device(server, time: FakeTime())
        await #expect(throws: AuthError.linkElsewhere) { try await signIn.start() }
        server.answer(path: "/api/oauth/device", status: 200, json: """
            {"device_code":"d","user_code":"BCDFGHJK","verification_uri":"\(server.origin)/link",
             "verification_uri_complete":"http://evil.example.com/link?code=BCDFGHJK","expires_in":600}
            """)
        await #expect(throws: AuthError.linkElsewhere) { try await signIn.start() }
    }

    @Test func theMacDoesNotSignInWithACode() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let signIn = try await device(server, time: FakeTime(), client: .macos)
        await #expect(throws: AuthError.noCodes) { try await signIn.start() }
    }

    @Test func aWiderScopeIsRevokedAndRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setWiderScope()
        server.approveDevice()
        let store = MemoryTokenStore()
        let signIn = try await device(server, time: FakeTime(), store: store)
        await #expect(throws: AuthError.widerScope) { try await signIn.waitForApproval(try await signIn.start()) }
        let revoked = try #require(server.requests("/api/oauth/revoke").first)
        #expect(revoked.formKeys == ["client_id", "token"])
        #expect(try await store.load(issuer: server.origin) == nil)
    }

    @Test func userCodesAreShownInTwoHalves() {
        #expect(DeviceSignIn.shown("bcdfghjk") == "BCDF-GHJK")
        #expect(DeviceSignIn.shown("BCDF-GHJK") == "BCDF-GHJK")
        #expect(DeviceSignIn.shown("12345") == "12345")
    }
}
