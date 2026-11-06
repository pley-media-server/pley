import Foundation
import PleyCommon
import PleyNetwork

/// Why signing in didn't work, in a few words for people, worded as the
/// Android app words them. Network failures stay `PleyError`s.
public enum AuthError: Error, Sendable, Equatable {
    /// What was given isn't a server's address.
    case notAnAddress
    /// The server has no app sign-in (or not one this app can use).
    case tooOld
    /// The server answered its metadata with a redirect.
    case otherAddress
    /// The server's issuer isn't the address the app used.
    case otherIssuer(String)
    /// The metadata sends sign-ins to another address.
    case endpointElsewhere
    /// The server can't sign in with a code.
    case noCodes
    /// An answer that couldn't be read.
    case unreadable
    /// Plain http to an address that isn't on the home network.
    case unencrypted
    /// The person closed the sign-in page.
    case cancelled
    /// The browser couldn't be opened.
    case noBrowser
    /// The browser came back with an answer to another sign-in.
    case stateMismatch
    /// The answer came from another server than the one signed in to (RFC 9207).
    case otherServer
    /// The person didn't allow the app, or refused the code.
    case refused
    /// The server refused, in its own words (made plain).
    case serverRefused(String)
    /// The browser came back without a code.
    case noCode
    /// The code was exchanged too late, or twice.
    case tookTooLong
    /// The code shown ran out: get a new one.
    case expired
    /// The code shown doesn't work any more.
    case codeEnded
    /// The device answer sends people to another address.
    case linkElsewhere
    /// The server allowed more than the app asks for; that sign-in was ended.
    case widerScope

    public var message: String {
        switch self {
        case .notAnAddress: localized("that isn't a server address")
        case .tooOld: PleyError.tooOld.userMessage
        case .otherAddress: PleyError.redirected(location: nil).userMessage
        case .otherIssuer(let issuer):
            Self.otherIssuer(String(plain(issuer).prefix(100)))
        case .endpointElsewhere: localized("this server sends apps to another address to sign in: Pley doesn't")
        case .noCodes: localized("this server can't sign in with a code yet: update it, or sign in with your browser")
        case .unreadable: localized("the server's answer can't be read: try again")
        case .unencrypted: PleyError.unencryptedMessage
        case .cancelled: localized("the sign-in was cancelled")
        case .noBrowser: localized("the sign-in page couldn't be opened")
        case .stateMismatch: localized("the answer wasn't for this sign-in: try again")
        case .otherServer: localized("the answer came from another server than the one you signed in to: try again")
        case .refused: localized("you didn't allow the app")
        case .serverRefused(let why): localized("the server refused: \(why)")
        case .noCode: localized("the server sent no code: try again")
        case .tookTooLong: localized("the sign-in took too long: try again")
        case .expired: localized("the code ran out: get a new one")
        case .codeEnded: localized("this code doesn't work anymore: get a new one")
        case .linkElsewhere: localized("this server sends you to another address to allow the app: Pley doesn't")
        case .widerScope: localized("the server allowed more than this app asks for: signed out")
        }
    }
}

extension AuthError {
    private static func otherIssuer(_ name: String) -> String {
        localized("this server calls itself \(name): open it by that address, or check the proxy in front of it passes Host")
    }
}

extension AuthError: LocalizedError {
    public var errorDescription: String? { message }
}
