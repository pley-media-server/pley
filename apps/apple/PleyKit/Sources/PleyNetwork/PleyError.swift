import Foundation
import PleyCommon
import PleyModel

/// Why a request to a server failed. `userMessage` says it in a few words
/// for people, in lower case without a full stop, the way the Android app
/// writes its own; screens make it a sentence.
public enum PleyError: Error, Sendable, Equatable {
    /// This device has no network.
    case offline
    /// No answer in time.
    case timedOut
    /// The server couldn't be reached; why, in words for people.
    case unreachable(String)
    /// The system refused the app the local network (Settings → Privacy &
    /// Security → Local Network).
    case localNetworkBlocked
    /// App Transport Security refused plain http to a name with dots.
    case insecureRefused
    /// Plain http to an address that isn't on the home network: nothing
    /// with a code or token goes there (SECURITY.md).
    case unencrypted
    /// The server answered with a redirect the app doesn't follow.
    case redirected(location: String?)
    /// What answered isn't Pley.
    case notPley
    /// The server is too old for this app.
    case tooOld
    /// An answer the app couldn't read, with its HTTP status.
    case unreadable(status: Int)
    /// An answer larger than the app reads.
    case tooLarge
    /// The server's refusal: `{ code, message }`, the message made plain.
    case api(status: Int, code: String, message: String?)
    /// Live TV's `409`: every connection of the channel's source is in use,
    /// and `connections` says what holds each one.
    case busy(message: String?, connections: [BusyConnection])
    /// The app wasn't allowed this; `scope` is the one that would allow it.
    case insufficientScope(scope: String?)
    /// The playback session was stopped from elsewhere, with what the
    /// person who stopped it wrote.
    case sessionStopped(message: String?)
    /// The server no longer accepts this sign-in.
    case signedOut
    /// An OAuth endpoint's refusal.
    case oauth(status: Int, OAuthErrorBody)

    public var userMessage: String {
        switch self {
        case .offline:
            "this device isn't connected to a network"
        case .timedOut:
            "no answer in time"
        case .unreachable(let why):
            why
        case .localNetworkBlocked:
            "Pley isn't allowed to reach your local network: allow it in Settings → Privacy & Security → Local Network"
        case .insecureRefused:
            "this device opens plain http addresses on your network only: open this server by its https address, "
                + "its IP address or a name without dots"
        case .unencrypted:
            Self.unencryptedMessage
        case .redirected:
            "this server answers at another address: open it by that one"
        case .notPley:
            "that isn't a Pley server"
        case .tooOld:
            "this server is too old for this app"
        case .unreadable(let status):
            "the server's answer couldn't be read (HTTP \(status))"
        case .tooLarge:
            "the server's answer is too large"
        case .api(let status, let code, let message):
            message.flatMap { $0.isEmpty ? nil : $0 } ?? "the server refused (\(code), HTTP \(status))"
        case .busy(let message, _):
            message.flatMap { $0.isEmpty ? nil : $0 } ?? "every connection of this channel's provider is in use"
        case .insufficientScope:
            "this app wasn't allowed to do that: sign in to it again to allow it"
        case .sessionStopped(let message):
            message.flatMap { $0.isEmpty ? nil : "playback was stopped: \($0)" } ?? "playback was stopped"
        case .signedOut:
            "you were signed out: sign in again"
        case .oauth(_, let body):
            plain(body.errorDescription ?? body.error)
        }
    }

    /// The Android app's `UNENCRYPTED`, word for word.
    public static let unencryptedMessage =
        "this server's address isn't on your network and its connection isn't encrypted: open it by its https address to sign in"
}

extension PleyError: LocalizedError {
    public var errorDescription: String? { userMessage }
}

extension PleyError {
    /// `error` from URLSession as a `PleyError`, or `CancellationError`
    /// when the request was cancelled because its task was.
    static func from(_ error: any Error) -> any Error {
        if error is PleyError || error is CancellationError { return error }
        if let resolver = error as? HostResolverError {
            switch resolver {
            case .timedOut: return PleyError.timedOut
            case .failed: return PleyError.unreachable(unknownHost)
            }
        }
        guard let url = error as? URLError else { return PleyError.unreachable(connectionFailed) }
        if blockedLocalNetwork(url as NSError) { return PleyError.localNetworkBlocked }
        switch url.code {
        case .cancelled:
            return CancellationError()
        case .notConnectedToInternet, .dataNotAllowed, .internationalRoamingOff:
            return PleyError.offline
        case .timedOut:
            return PleyError.timedOut
        case .cannotFindHost, .dnsLookupFailed:
            return PleyError.unreachable(unknownHost)
        case .cannotConnectToHost:
            return PleyError.unreachable("nothing answers there")
        case .appTransportSecurityRequiresSecureConnection:
            return PleyError.insecureRefused
        case .secureConnectionFailed, .serverCertificateHasBadDate, .serverCertificateUntrusted,
             .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid, .clientCertificateRejected,
             .clientCertificateRequired:
            return PleyError.unreachable("its secure connection failed")
        default:
            return PleyError.unreachable(connectionFailed)
        }
    }

    static let unknownHost = "that address isn't known on this network"
    static let connectionFailed = "the connection to the server failed"

    /// Whether the system refused the connection because the app may not
    /// use the local network. URLSession says so only in its error's
    /// details: the path it gave up on ("Local network prohibited"), or a
    /// DNS policy refusal (`kDNSServiceErr_PolicyDenied`) underneath.
    private static func blockedLocalNetwork(_ error: NSError) -> Bool {
        var next: NSError? = error
        while let e = next {
            if e.domain == "kNWErrorDomainDNS" && e.code == -65570 { return true }
            if let path = e.userInfo["_NSURLErrorNWPathKey"],
               String(describing: path).localizedCaseInsensitiveContains("local network prohibited") {
                return true
            }
            next = e.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return false
    }
}
