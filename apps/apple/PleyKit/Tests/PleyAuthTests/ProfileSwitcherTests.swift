import Foundation
@testable import PleyAuth
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

@Suite struct ProfileSwitcherTests {
    private struct Signed {
        let server: FixtureServer
        let store: MemoryTokenStore
        let tokens: Tokens
        let switcher: ProfileSwitcher
    }

    private func signedIn() async throws -> Signed {
        let server = try await fixture()
        let store = MemoryTokenStore()
        _ = try await PleyAuthTests.signedIn(server, store: store)
        let tokens = Tokens(issuer: server.origin, store: store, endpoints: endpoints())
        return Signed(server: server, store: store, tokens: tokens,
                      switcher: ProfileSwitcher(api: api(server, tokens), tokens: tokens))
    }

    @Test func theHouseholdIsListed() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        let profiles = try await s.switcher.profiles()
        #expect(profiles.map(\.name) == ["Alex", "Sam", "Kids", "Robin", "Guest"])
    }

    @Test func aProfileWithoutAPinSwitchesAtOnce() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        let before = try #require(try await s.store.load(issuer: s.server.origin))
        try await s.switcher.switchTo(4, pin: nil)
        #expect(try await s.switcher.api.me().name == "Kids")
        let after = try #require(try await s.store.load(issuer: s.server.origin))
        #expect(after.refreshToken != before.refreshToken)
        let asked = try #require(s.server.requests("/api/oauth/token").first)
        #expect(asked.grantType == AppClient.switchProfileGrant)
        #expect(asked.formKeys == ["client_id", "grant_type", "profile", "refresh_token"])
    }

    @Test func aProfileWithAPinTakesTheRightOne() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        await #expect(throws: ProfileSwitchError.wrongPin) { try await s.switcher.switchTo(3, pin: "9999") }
        // A refusal spends nothing: the old tokens still work.
        #expect(try await s.switcher.api.me().name == "Alex")
        try await s.switcher.switchTo(3, pin: "1234")
        #expect(try await s.switcher.api.me().name == "Sam")
        #expect(s.server.requests("/api/oauth/token").last?.formKeys.contains("pin") == true)
    }

    @Test func tooManyWrongPinsAreThrottled() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        for _ in 0..<5 {
            await #expect(throws: ProfileSwitchError.wrongPin) { try await s.switcher.switchTo(3, pin: "0000") }
        }
        await #expect(throws: ProfileSwitchError.throttled) { try await s.switcher.switchTo(3, pin: "1234") }
    }

    @Test func aLockedPinSaysSo() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        s.server.lockPin(of: 3)
        await #expect(throws: ProfileSwitchError.pinLocked) { try await s.switcher.switchTo(3, pin: "1234") }
    }

    @Test func theReasonsAreKept() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        await #expect(throws: ProfileSwitchError.needsBrowser) { try await s.switcher.switchTo(1, pin: nil) }
        await #expect(throws: ProfileSwitchError.notAllowed) { try await s.switcher.switchTo(5, pin: nil) }
        await #expect(throws: ProfileSwitchError.cannotCheckPins) { try await s.switcher.switchTo(2, pin: nil) }
        #expect(try await s.switcher.api.me().name == "Alex")
        #expect(try await s.store.load(issuer: s.server.origin) != nil)
    }

    @Test func aPinIsFourToEightDigits() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        for pin in ["", "123", "123456789", "12a4", "１２３４"] {
            await #expect(throws: ProfileSwitchError.pinFormat) { try await s.switcher.switchTo(3, pin: pin) }
        }
        #expect(s.server.requests("/api/oauth/token").isEmpty)
        #expect(ProfileSwitcher.isPin("1234") && ProfileSwitcher.isPin("12345678"))
    }

    @Test func aSpentSignInEndsOnASwitch() async throws {
        let s = try await signedIn()
        defer { s.server.stop() }
        // The sign-in was ended elsewhere (Account → Apps).
        let current = try #require(try await s.store.load(issuer: s.server.origin))
        _ = try await endpoints().postForm(URL(string: s.server.origin + "/api/oauth/revoke")!,
                                           [("token", current.refreshToken!), ("client_id", current.clientID)])
        await #expect(throws: ProfileSwitchError.signedOut) { try await s.switcher.switchTo(4, pin: nil) }
        #expect(try await s.store.load(issuer: s.server.origin) == nil)
    }

    @Test func theMessagesAreWords() {
        #expect(ProfileSwitchError.wrongPin.message == "wrong PIN")
        #expect(ProfileSwitcher.error(OAuthErrorBody(error: "invalid_grant", reason: "throttled")) == .throttled)
        #expect(ProfileSwitcher.error(OAuthErrorBody(error: "unauthorized_client")).message
            == "this sign-in can't switch profiles: sign in again to allow it")
    }
}
