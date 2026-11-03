import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Where a server signs apps in (RFC 8414), read from
/// `<server>/.well-known/oauth-authorization-server`. Kept only when it is
/// this server's own: its issuer is exactly the address the app used and
/// every endpoint is on it, so an answer can't send the app's codes or
/// tokens anywhere else.
public struct ServerMetadata: Sendable, Hashable {
    public static let path = "/.well-known/oauth-authorization-server"

    public let issuer: String
    public let authorizationEndpoint: URL
    public let tokenEndpoint: URL
    public let revocationEndpoint: URL
    /// Where a TV asks for a code (RFC 8628), on this server too; `nil`
    /// when the server has none.
    public let deviceAuthorizationEndpoint: URL?
    /// The scopes it offers; `nil` when it doesn't say.
    public let scopesSupported: [String]?
    public let grantTypesSupported: [String]?

    /// Whether this server signs in with a code: the endpoint, and the grant to collect it.
    public var signsInWithCodes: Bool {
        deviceAuthorizationEndpoint != nil && (grantTypesSupported ?? []).contains(AppClient.deviceCodeGrant)
    }

    /// What to ask this server for: every scope of the app's that it
    /// offers (all of them when it doesn't say), and `requests` and
    /// `approvals` only where it lists them. A server without profiles
    /// doesn't know `profiles` and would refuse the whole sign-in for it.
    public var scope: String {
        let wanted = AppClient.scope.split(separator: " ").map(String.init)
        let offered = wanted.filter { scopesSupported == nil || scopesSupported!.contains($0) }
        let listed = AppClient.listedScope.split(separator: " ").map(String.init)
            .filter { scopesSupported?.contains($0) == true }
        return ((offered.isEmpty ? ["library"] : offered) + listed).joined(separator: " ")
    }

    /// Reads and checks the metadata of the server at `origin`.
    public static func fetch(_ origin: String, endpoints: EndpointClient) async throws -> ServerMetadata {
        guard let home = Addresses.origin(origin), let url = URL(string: home + path) else {
            throw AuthError.notAnAddress
        }
        let data: Data
        do {
            data = try await endpoints.get(url)
        } catch PleyError.redirected {
            // The issuer must be the address the app used, so another one can't be the answer.
            throw AuthError.otherAddress
        } catch PleyError.unreadable(let status) where status == 404 {
            throw AuthError.tooOld
        }
        return try parse(data, origin: home)
    }

    /// The checks of `fetch`, on `data`, the answer of the server at `origin`.
    public static func parse(_ data: Data, origin: String) throws -> ServerMetadata {
        guard let home = Addresses.origin(origin) else { throw AuthError.notAnAddress }
        // Older servers answer their web app's page here.
        guard let raw = try? PleyJSON.decoder.decode(AuthorizationServerMetadata.self, from: data),
              let issuer = raw.issuer, !issuer.isEmpty
        else { throw AuthError.tooOld }
        // RFC 8414 §3.3: the issuer is exactly the address asked at.
        guard issuer == home else { throw AuthError.otherIssuer(issuer) }
        guard (raw.codeChallengeMethodsSupported ?? []).contains("S256") else { throw AuthError.tooOld }

        func endpoint(_ value: String?) throws -> URL {
            guard let value, !value.isEmpty else { throw AuthError.tooOld }
            guard Addresses.origin(value) == issuer, value.hasPrefix(issuer + "/"), let url = URL(string: value) else {
                throw AuthError.endpointElsewhere
            }
            return url
        }
        let device = try raw.deviceAuthorizationEndpoint.flatMap { $0.isEmpty ? nil : $0 }.map { try endpoint($0) }
        return ServerMetadata(
            issuer: issuer,
            authorizationEndpoint: try endpoint(raw.authorizationEndpoint),
            tokenEndpoint: try endpoint(raw.tokenEndpoint),
            revocationEndpoint: try endpoint(raw.revocationEndpoint),
            deviceAuthorizationEndpoint: device,
            scopesSupported: raw.scopesSupported,
            grantTypesSupported: raw.grantTypesSupported)
    }
}
