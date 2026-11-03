import Foundation

/// A token endpoint's answer (docs/API.md, "Tokens"). Its `description`
/// never shows the tokens, so printing or logging one leaks nothing.
public struct TokenResponse: Codable, Hashable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    public var accessToken: String
    public var tokenType: String
    /// Seconds the access token works for.
    public var expiresIn: Int?
    /// Only for apps whose description lists the refresh grant (Pley's own do).
    public var refreshToken: String?
    /// Everything the app was allowed, separated by spaces.
    public var scope: String?
    /// The server's id, and this sign-in's proof key and its id (docs/API.md,
    /// "Telling servers apart"), as written; PleyAuth checks them. Left out
    /// by servers before them, and read as left out when not text.
    public var serverId: String?
    public var proofId: String?
    public var proofKey: String?

    public init(accessToken: String, tokenType: String = "Bearer", expiresIn: Int? = nil, refreshToken: String? = nil,
                scope: String? = nil, serverId: String? = nil, proofId: String? = nil, proofKey: String? = nil) {
        self.accessToken = accessToken
        self.tokenType = tokenType
        self.expiresIn = expiresIn
        self.refreshToken = refreshToken
        self.scope = scope
        self.serverId = serverId
        self.proofId = proofId
        self.proofKey = proofKey
    }

    private enum CodingKeys: String, CodingKey {
        case accessToken, tokenType, expiresIn, refreshToken, scope, serverId, proofId, proofKey
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        accessToken = try c.decode(String.self, forKey: .accessToken)
        tokenType = try c.decode(String.self, forKey: .tokenType)
        expiresIn = try c.decodeIfPresent(Int.self, forKey: .expiresIn)
        refreshToken = try c.decodeIfPresent(String.self, forKey: .refreshToken)
        scope = try c.decodeIfPresent(String.self, forKey: .scope)
        // A proof field written as anything but text (`"proof_id": 17`)
        // leaves the answer without a proof, not unreadable.
        serverId = try? c.decodeIfPresent(String.self, forKey: .serverId)
        proofId = try? c.decodeIfPresent(String.self, forKey: .proofId)
        proofKey = try? c.decodeIfPresent(String.self, forKey: .proofKey)
    }

    public var description: String {
        "TokenResponse(tokenType: \(tokenType), expiresIn: \(expiresIn.map(String.init) ?? "nil"), "
            + "refreshToken: \(refreshToken == nil ? "none" : "hidden"), scope: \(scope ?? "nil"), "
            + "proofKey: \(proofKey == nil ? "none" : "hidden"))"
    }

    public var debugDescription: String { description }
}

/// The server's metadata (`/.well-known/oauth-authorization-server`, RFC
/// 8414) as it came: PleyAuth checks it before using any of it.
public struct AuthorizationServerMetadata: Codable, Hashable, Sendable {
    public var issuer: String?
    public var authorizationEndpoint: String?
    public var tokenEndpoint: String?
    public var revocationEndpoint: String?
    public var deviceAuthorizationEndpoint: String?
    public var responseTypesSupported: [String]?
    public var responseModesSupported: [String]?
    public var grantTypesSupported: [String]?
    public var tokenEndpointAuthMethodsSupported: [String]?
    public var codeChallengeMethodsSupported: [String]?
    public var scopesSupported: [String]?
    public var authorizationResponseIssParameterSupported: Bool?

    public init(issuer: String? = nil) {
        self.issuer = issuer
    }
}

/// `POST /api/oauth/device`'s answer (RFC 8628 §3.2). `deviceCode` stays in
/// the app and is never shown or printed.
public struct DeviceAuthorization: Codable, Hashable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    public var deviceCode: String
    /// `BCDF-GHJK`.
    public var userCode: String
    public var verificationUri: String
    public var verificationUriComplete: String?
    /// Seconds the code works for.
    public var expiresIn: Int
    /// Seconds between polls; 5 when left out.
    public var interval: Int?

    public init(deviceCode: String, userCode: String, verificationUri: String, verificationUriComplete: String? = nil,
                expiresIn: Int, interval: Int? = nil) {
        self.deviceCode = deviceCode
        self.userCode = userCode
        self.verificationUri = verificationUri
        self.verificationUriComplete = verificationUriComplete
        self.expiresIn = expiresIn
        self.interval = interval
    }

    public var description: String {
        "DeviceAuthorization(userCode: \(userCode), verificationUri: \(verificationUri), expiresIn: \(expiresIn))"
    }

    public var debugDescription: String { description }
}
