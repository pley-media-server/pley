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

    public init(accessToken: String, tokenType: String = "Bearer", expiresIn: Int? = nil, refreshToken: String? = nil,
                scope: String? = nil) {
        self.accessToken = accessToken
        self.tokenType = tokenType
        self.expiresIn = expiresIn
        self.refreshToken = refreshToken
        self.scope = scope
    }

    public var description: String {
        "TokenResponse(tokenType: \(tokenType), expiresIn: \(expiresIn.map(String.init) ?? "nil"), "
            + "refreshToken: \(refreshToken == nil ? "none" : "hidden"), scope: \(scope ?? "nil"))"
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
