import Foundation
@testable import PleyAuth
import PleyNetwork
import PleyTesting
import Testing

@Suite struct PKCETests {
    @Test func theChallengeIsRFC7636sAppendixB() {
        // The 32 octets of RFC 7636 Appendix B, their verifier, and its S256 challenge.
        let octets: [UInt8] = [116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186,
                               22, 212, 37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121]
        #expect(PKCE.base64url(Data(octets)) == "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
        #expect(PKCE.challenge(for: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
            == "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
    }

    @Test func verifiersAndStatesAreRandomBase64url() {
        let verifiers = (1...50).map { _ in PKCE.verifier() }
        let states = (1...50).map { _ in PKCE.state() }
        let alphabet = Set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")
        for v in verifiers {
            #expect(v.count == 64)
            #expect(v.allSatisfy(alphabet.contains))
        }
        for s in states {
            #expect(s.count == 43)
            #expect(s.allSatisfy(alphabet.contains))
        }
        #expect(Set(verifiers).count == 50)
        #expect(Set(states).count == 50)
        #expect(PKCE.challenge(for: verifiers[0]).count == 43)
    }
}

@Suite struct ServerMetadataTests {
    private func document(_ issuer: String, _ change: (inout [String: Any]) -> Void = { _ in }) -> Data {
        var o: [String: Any] = [
            "issuer": issuer,
            "authorization_endpoint": "\(issuer)/oauth/authorize",
            "token_endpoint": "\(issuer)/api/oauth/token",
            "revocation_endpoint": "\(issuer)/api/oauth/revoke",
            "response_types_supported": ["code"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["library", "play", "playlists"],
            "authorization_response_iss_parameter_supported": true,
        ]
        change(&o)
        return try! JSONSerialization.data(withJSONObject: o)
    }

    private func refused(_ data: Data, at origin: String = "http://192.168.1.20:7539") -> AuthError? {
        do {
            _ = try ServerMetadata.parse(data, origin: origin)
            return nil
        } catch {
            return error as? AuthError
        }
    }

    @Test func theServersOwnMetadataIsKept() throws {
        let m = try ServerMetadata.parse(document("http://192.168.1.20:7539"), origin: "http://192.168.1.20:7539/")
        #expect(m.issuer == "http://192.168.1.20:7539")
        #expect(m.authorizationEndpoint.absoluteString == "http://192.168.1.20:7539/oauth/authorize")
        #expect(m.tokenEndpoint.absoluteString == "http://192.168.1.20:7539/api/oauth/token")
        #expect(m.revocationEndpoint.absoluteString == "http://192.168.1.20:7539/api/oauth/revoke")
        #expect(m.scopesSupported == ["library", "play", "playlists"])
        // A server without profiles isn't asked for them.
        #expect(m.scope == "library play playlists")
        #expect(!m.signsInWithCodes)
        // Default ports and upper case, as browsers and the server write addresses.
        #expect(try ServerMetadata.parse(document("https://pley.example.com"),
                                         origin: "https://PLEY.example.com:443").issuer == "https://pley.example.com")
    }

    @Test func theIssuerMustBeTheAddressUsed() {
        #expect(refused(document("http://192.168.1.21:7539")) == .otherIssuer("http://192.168.1.21:7539"))
        #expect(refused(document("http://192.168.1.21:7539"))?.message.hasPrefix("this server calls itself") == true)
        for issuer in ["https://192.168.1.20:7539", "http://192.168.1.20:7539/", "http://192.168.1.20:7539/pley"] {
            #expect(refused(document(issuer)) != nil)
        }
    }

    @Test func s256IsRequired() {
        #expect(refused(document("http://192.168.1.20:7539") { $0["code_challenge_methods_supported"] = ["plain"] })
            == .tooOld)
        #expect(refused(document("http://192.168.1.20:7539") { $0["code_challenge_methods_supported"] = nil })
            == .tooOld)
    }

    @Test func everyEndpointIsOnTheIssuersAddress() {
        let issuer = "http://192.168.1.20:7539"
        for name in ["authorization_endpoint", "token_endpoint", "revocation_endpoint", "device_authorization_endpoint"] {
            for elsewhere in ["https://evil.example.com/token", "http://192.168.1.20:8080/x", "https://192.168.1.20:7539/x",
                              "http://192.168.1.20:7539@evil.example.com/x"] {
                #expect(refused(document(issuer) { $0[name] = elsewhere }) == .endpointElsewhere, "\(name) \(elsewhere)")
            }
            if name != "device_authorization_endpoint" {
                #expect(refused(document(issuer) { $0[name] = nil }) == .tooOld)
            }
        }
    }

    @Test func codesNeedTheEndpointAndTheGrant() throws {
        let issuer = "http://192.168.1.20:7539"
        let both = try ServerMetadata.parse(document(issuer) {
            $0["device_authorization_endpoint"] = "\(issuer)/api/oauth/device"
            $0["grant_types_supported"] = ["authorization_code", AppClient.deviceCodeGrant]
        }, origin: issuer)
        #expect(both.signsInWithCodes)
        let noGrant = try ServerMetadata.parse(document(issuer) {
            $0["device_authorization_endpoint"] = "\(issuer)/api/oauth/device"
        }, origin: issuer)
        #expect(!noGrant.signsInWithCodes)
    }

    @Test func requestsAndApprovalsAreAskedForOnlyWhereTheServerListsThem() throws {
        let issuer = "http://192.168.1.20:7539"
        // A server that doesn't say what it offers is asked for the four it has always had.
        let silent = try ServerMetadata.parse(document(issuer) { $0["scopes_supported"] = nil }, origin: issuer)
        #expect(silent.scope == "library play playlists profiles")
        let both = try ServerMetadata.parse(document(issuer) {
            $0["scopes_supported"] = ["library", "play", "playlists", "profiles", "requests", "approvals"]
        }, origin: issuer)
        #expect(both.scope == "library play playlists profiles requests approvals")
        let one = try ServerMetadata.parse(document(issuer) {
            $0["scopes_supported"] = ["library", "play", "playlists", "profiles", "requests"]
        }, origin: issuer)
        #expect(one.scope == "library play playlists profiles requests")
    }

    @Test func theFixturesMetadataIsFetched() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let m = try await metadata(server)
        #expect(m.issuer == server.origin)
        #expect(m.signsInWithCodes)
        #expect(m.scope == FixtureServer.allScopes)
        let asked = try #require(server.journal.first)
        #expect(asked.path == ServerMetadata.path)
        #expect(asked.headers["user-agent"] == testAgent)
        #expect(asked.headers["authorization"] == nil && asked.headers["cookie"] == nil)
    }

    @Test func aWrongIssuerIsRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setIssuerOverride("http://192.168.1.99:7539")
        await #expect(throws: AuthError.otherIssuer("http://192.168.1.99:7539")) { try await metadata(server) }
    }

    @Test func olderServersAreTooOld() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: ServerMetadata.path, status: 404, json: #"{"code":"not_found"}"#)
        await #expect(throws: AuthError.tooOld) { try await metadata(server) }
        server.answer(path: ServerMetadata.path, status: 200, headers: ["Content-Type": "text/html"],
                      body: Data("<!doctype html><title>Pley</title>".utf8))
        await #expect(throws: AuthError.tooOld) { try await metadata(server) }
    }

    @Test func metadataIsReadWithoutRedirects() async throws {
        let server = try await fixture()
        let other = try await fixture()
        defer { server.stop(); other.stop() }
        server.redirect(path: ServerMetadata.path, to: other.origin + ServerMetadata.path)
        await #expect(throws: AuthError.otherAddress) { try await metadata(server) }
        #expect(other.journal.isEmpty)
    }
}
