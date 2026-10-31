import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// A code for signing in (RFC 8628), shown as `userCode` with
/// `verificationURL` and a QR code of `completeURL`, and allowed on a phone
/// or computer at the server's `/link` page.
public struct DeviceCode: Sendable, Hashable, CustomStringConvertible, CustomDebugStringConvertible {
    /// As people read it: `BCDF-GHJK`.
    public let userCode: String
    /// The server's `/link` page, on the address the app used.
    public let verificationURL: URL
    /// The same with the code in it, for the QR code.
    public let completeURL: URL?
    public let expiresAt: Date
    /// What the app polls with. Never shown or printed.
    let deviceCode: String
    /// Seconds between polls, the server's own.
    let interval: Int
    let scope: String

    public var description: String { "DeviceCode(userCode: \(userCode), expiresAt: \(expiresAt))" }
    public var debugDescription: String { description }
}

/// Signing in without a browser, with a code shown on screen: Apple TV's
/// only way in, and the iPhone's and iPad's other one. Only viewer tokens
/// come of it, whoever approves.
///
/// `waitForApproval` asks the token endpoint every interval (5 seconds
/// longer for good after each `slow_down`) until the code is allowed,
/// refused or runs out; cancel its task to stop. When the network fails it
/// waits 2, 4, 8, 16, then 30 seconds and goes on.
public final class DeviceSignIn: Sendable {
    public static let defaultInterval = 5
    public static let slowDown = 5
    static let backoff = [2, 4, 8, 16, 30]

    public let metadata: ServerMetadata
    public let client: AppClient
    private let store: any TokenStore
    private let endpoints: EndpointClient
    private let clock: @Sendable () -> Date
    private let sleep: @Sendable (Duration) async throws -> Void

    /// `clock` and `sleep` are the real ones but in tests.
    public init(metadata: ServerMetadata, client: AppClient = .current, store: any TokenStore,
                endpoints: EndpointClient, clock: @escaping @Sendable () -> Date = { Date() },
                sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.metadata = metadata
        self.client = client
        self.store = store
        self.endpoints = endpoints
        self.clock = clock
        self.sleep = sleep
    }

    /// Asks the server for a code to show.
    public func start() async throws -> DeviceCode {
        guard metadata.signsInWithCodes, let endpoint = metadata.deviceAuthorizationEndpoint else {
            throw AuthError.noCodes
        }
        let scope = metadata.scope
        let data: Data
        do {
            data = try await endpoints.postForm(endpoint, [("client_id", client.clientID), ("scope", scope)])
        } catch PleyError.oauth(_, let body) {
            if body.error == "unauthorized_client" { throw AuthError.noCodes }
            throw AuthError.serverRefused(plain(body.errorDescription ?? body.error))
        }
        guard let answer = try? PleyJSON.decoder.decode(DeviceAuthorization.self, from: data),
              !answer.deviceCode.isEmpty, answer.expiresIn > 0
        else { throw AuthError.unreadable }
        let userCode = Self.shown(answer.userCode)
        guard !userCode.isEmpty, let verification = link(answer.verificationUri) else { throw AuthError.unreadable }
        // The page people open is this server's, so nobody is sent elsewhere to type the code.
        let complete = try answer.verificationUriComplete.flatMap { $0.isEmpty ? nil : $0 }.map { text in
            guard let url = link(text) else { throw AuthError.unreadable }
            return url
        }
        guard Self.onIssuer(verification, metadata.issuer), complete.map({ Self.onIssuer($0, metadata.issuer) }) ?? true
        else { throw AuthError.linkElsewhere }
        let interval = (answer.interval ?? 0) > 0 ? answer.interval! : Self.defaultInterval
        return DeviceCode(userCode: userCode, verificationURL: verification, completeURL: complete,
                          expiresAt: clock().addingTimeInterval(TimeInterval(answer.expiresIn)),
                          deviceCode: answer.deviceCode, interval: interval, scope: scope)
    }

    /// Waits until `code` is allowed, and keeps the tokens (replacing, and
    /// ending, this app's sign-in to that server). Throws `AuthError.refused`
    /// when the person said no, `.expired` when the code ran out, and
    /// `.codeEnded` when the server no longer knows it.
    public func waitForApproval(_ code: DeviceCode) async throws -> StoredTokens {
        var interval = code.interval
        var failures = 0
        while true {
            let wait = failures == 0 ? interval : max(interval, Self.backoff[min(failures, Self.backoff.count) - 1])
            let left = code.expiresAt.timeIntervalSince(clock())
            try await sleep(.milliseconds(Int(max(0, min(Double(wait), left)) * 1000)))
            try Task.checkCancellation()
            if clock() >= code.expiresAt { throw AuthError.expired }
            let data: Data
            do {
                data = try await endpoints.postForm(metadata.tokenEndpoint, [
                    ("grant_type", AppClient.deviceCodeGrant),
                    ("device_code", code.deviceCode),
                    ("client_id", client.clientID),
                ])
            } catch PleyError.oauth(_, let body) {
                failures = 0
                switch body.error {
                case "authorization_pending": continue
                case "slow_down":
                    interval += Self.slowDown
                    continue
                case "access_denied": throw AuthError.refused
                case "expired_token": throw AuthError.expired
                // An unknown code, or one answered already.
                case "invalid_grant": throw AuthError.codeEnded
                default: throw AuthError.serverRefused(plain(body.errorDescription ?? body.error))
                }
            } catch let error as PleyError {
                switch error {
                case .offline, .timedOut, .unreachable, .unreadable:
                    failures += 1
                    continue
                default:
                    throw error
                }
            }
            let answer = try TokenAnswer.parse(data, asked: code.scope)
            let fresh = answer.stored(issuer: metadata.issuer, clientID: client.clientID,
                                      tokenEndpoint: metadata.tokenEndpoint.absoluteString,
                                      revocationEndpoint: metadata.revocationEndpoint.absoluteString, now: clock(),
                                      byRefresh: false)
            return try await keepNewSignIn(answer, asked: code.scope, fresh: fresh, store: store, endpoints: endpoints)
        }
    }

    /// A user code as people read it: 8 letters as `BCDF-GHJK`, anything else as sent.
    static func shown(_ code: String) -> String {
        let letters = code.uppercased().filter { $0.isASCII && ($0.isLetter || $0.isNumber) }
        if letters.count == 8 { return "\(letters.prefix(4))-\(letters.suffix(4))" }
        return plain(code)
    }

    private static func onIssuer(_ url: URL, _ issuer: String) -> Bool {
        Addresses.origin(url) == issuer
    }

    /// `text` when it is a web address to show (http or https), else `nil`.
    private func link(_ text: String) -> URL? {
        guard plain(text) == text, !text.isEmpty, let url = URL(string: text),
              let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = url.host, !host.isEmpty
        else { return nil }
        return url
    }
}
