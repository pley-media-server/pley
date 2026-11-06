import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Why a profile switch didn't happen, by the server's reason.
public enum ProfileSwitchError: Error, Sendable, Equatable {
    /// A PIN isn't 4 to 8 digits; nothing was sent.
    case pinFormat
    case wrongPin
    /// The profile's PIN stopped working after too many wrong ones.
    case pinLocked
    /// Too many wrong PINs lately.
    case throttled
    /// The profile needs its password, which only the server's page asks for.
    case needsBrowser
    case notAllowed
    /// The server can't check the PIN of the profile the app uses already.
    case cannotCheckPins
    /// The server no longer accepts this sign-in.
    case signedOut
    /// Another refusal, in the server's words.
    case refused(String)

    public var message: String {
        switch self {
        case .pinFormat: localized("a PIN is 4 to 8 digits")
        case .wrongPin: localized("wrong PIN")
        case .pinLocked:
            localized("this profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it")
        case .throttled: localized("too many wrong PINs: try again in a few minutes")
        case .needsBrowser:
            AppClient.current.signsInWithBrowser
                ? localized("this profile needs its password: sign in again to use it")
                : localized("this profile needs its password: sign in again with a code and choose it there")
        case .notAllowed: localized("you can't switch to this profile")
        case .cannotCheckPins: localized("this server can't check PINs")
        case .signedOut: PleyError.signedOut.userMessage
        case .refused(let why): why
        }
    }
}

extension ProfileSwitchError: LocalizedError {
    public var errorDescription: String? { message }
}

/// The household's profiles, and moving the sign-in between them with
/// Pley's switch-profile grant: with a PIN when the profile has one, never
/// with a password or a two-step code, which only the server's page asks for.
public struct ProfileSwitcher: Sendable {
    public let api: PleyAPI
    public let tokens: Tokens

    public init(api: PleyAPI, tokens: Tokens) {
        self.api = api
        self.tokens = tokens
    }

    /// The household (`GET /api/oauth/profiles`); empty for someone who isn't a home member.
    public func profiles() async throws -> [HouseholdProfile] {
        try await api.householdProfiles()
    }

    /// Whether `pin` can be a PIN: 4 to 8 digits.
    public static func isPin(_ pin: String) -> Bool {
        (4...8).contains(pin.count) && pin.allSatisfy { $0.isASCII && $0.isNumber }
    }

    /// Switches to profile `id`, with `pin` when it has one. On success the
    /// tokens are replaced (the old ones stop at once); a refusal keeps them.
    public func switchTo(_ id: Int, pin: String?) async throws {
        if let pin, !Self.isPin(pin) { throw ProfileSwitchError.pinFormat }
        do {
            try await tokens.switchProfile(id, pin: pin)
        } catch PleyError.signedOut {
            throw ProfileSwitchError.signedOut
        } catch PleyError.oauth(_, let body) {
            throw Self.error(body)
        }
    }

    static func error(_ body: OAuthErrorBody) -> ProfileSwitchError {
        switch body.reason {
        case "wrong_pin": return .wrongPin
        case "pin_locked": return .pinLocked
        case "throttled": return .throttled
        case "browser": return .needsBrowser
        case "not_allowed": return .notAllowed
        default: break
        }
        switch body.error {
        case "invalid_request" where body.reason == nil: return .cannotCheckPins
        case "unauthorized_client": return .refused(localized("this sign-in can't switch profiles: sign in again to allow it"))
        default: return .refused(plain(body.errorDescription ?? localized("the server refused: try again")))
        }
    }
}
