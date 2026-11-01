import Foundation
import PleyAuth
import Security
import Testing

/// The real Keychain, where this test runner has one: `swift test` on the
/// Mac does (the login keychain, since an unsigned runner can't use the
/// data protection keychain); a simulator's test runner may not.
@Suite(.enabled(if: KeychainItem.isAvailable(), "this test runner can't use the Keychain"))
struct KeychainTests {
    @Test func tokensRoundTripThroughTheKeychain() async throws {
        let issuer = "http://127.0.0.1:1/keychain-test-\(UUID().uuidString)"
        let store = KeychainTokenStore()
        defer { Task { try? await store.delete(issuer: issuer) } }
        let tokens = StoredTokens(issuer: issuer, clientID: AppClient.ios.clientID, tokenEndpoint: issuer + "/t",
                                  revocationEndpoint: issuer + "/r", accessToken: "a", accessExpiresAt: Date(timeIntervalSince1970: 1),
                                  refreshToken: "r", scope: "library")
        #expect(try await store.load(issuer: issuer) == nil)
        try await store.save(tokens)
        #expect(try await store.load(issuer: issuer) == tokens)
        var replaced = tokens
        replaced.accessToken = "b"
        try await store.save(replaced)
        #expect(try await store.load(issuer: issuer)?.accessToken == "b")
        try await store.delete(issuer: issuer)
        #expect(try await store.load(issuer: issuer) == nil)
        try await store.delete(issuer: issuer)
    }
}

/// What a Keychain failure says: why, never "try again".
@Suite struct KeychainErrorTests {
    @Test func aKeychainFailureSaysWhatFailed() {
        let unsigned = KeychainError(status: errSecMissingEntitlement)
        #expect(unsigned.localizedDescription.contains("isn't signed"))
        #expect(unsigned.localizedDescription.contains("-34018"))
        let other = KeychainError(status: errSecNotAvailable)
        #expect(other.errorDescription == "this device's Keychain can't be used (error \(errSecNotAvailable))")
    }
}
