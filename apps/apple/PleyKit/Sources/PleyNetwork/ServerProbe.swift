import Foundation
import PleyCommon
import PleyModel

/// What answered at an address someone typed.
public enum ProbeResult: Sendable, Equatable {
    /// Pley, at `origin`.
    case pley(origin: String, ServerInfo)
    /// Something answered at `origin`, but not Pley.
    case notPley(origin: String)
    /// Nothing answered; why, in words for people.
    case unreachable(String)
    /// The system refused the app the local network.
    case blocked
}

/// Checks that what someone typed is a Pley server: `GET /api/server`,
/// without a token, at each of `Servers.candidates` in turn (a name on the
/// internet with https first). No redirect is followed; one to https on the
/// same host is tried next instead, and any other means it isn't Pley.
public struct ServerProbe: Sendable {
    /// How long a home address has to answer.
    public static let localTimeout: TimeInterval = 4
    /// How long a name on the internet has.
    public static let timeout: TimeInterval = 10

    private let transport: HTTPTransport

    public init(transport: HTTPTransport) {
        self.transport = transport
    }

    public func probe(_ input: String) async -> ProbeResult {
        let candidates = Servers.candidates(input)
        if candidates.isEmpty { return .unreachable(PleyError.notAnAddress) }
        return await probe(candidates: candidates)
    }

    /// `probe` with the origins to try given.
    public func probe(candidates: [String]) async -> ProbeResult {
        var queue = candidates
        var tried = Set<String>()
        var notPley: ProbeResult?
        var failure: PleyError?
        while !queue.isEmpty {
            let origin = queue.removeFirst()
            guard tried.insert(origin).inserted, let url = URL(string: origin + "/api/server"),
                  let host = url.host(percentEncoded: false)
            else { continue }
            var request = URLRequest(url: url)
            request.timeoutInterval = Servers.isLocalHost(host) ? Self.localTimeout : Self.timeout
            request.setValue("application/json", forHTTPHeaderField: "Accept")
            let response: HTTPResponse
            do {
                // Ungated: nothing secret goes, and a server on the internet
                // over plain http is shown with its warning, not hidden.
                response = try await transport.perform(request, redirects: .endpoints, maxBytes: BodyLimit.small,
                                                       gated: false)
            } catch PleyError.localNetworkBlocked {
                return .blocked
            } catch let error as PleyError {
                failure = failure ?? error
                continue
            } catch {
                return .unreachable(PleyError.connectionFailed)
            }
            if (300..<400).contains(response.status) {
                if let upgrade = Self.upgrade(of: url, location: response.header("Location")) {
                    queue.insert(upgrade, at: 0)
                } else {
                    notPley = notPley ?? .notPley(origin: origin)
                }
                continue
            }
            guard (200..<300).contains(response.status),
                  let info = try? PleyJSON.decoder.decode(ServerInfo.self, from: response.body),
                  !info.version.trimmingCharacters(in: .whitespaces).isEmpty
            else {
                notPley = notPley ?? .notPley(origin: origin)
                continue
            }
            return .pley(origin: Addresses.origin(origin) ?? origin, info)
        }
        if let notPley { return notPley }
        // Cancelled, or every address failed.
        return .unreachable(failure?.userMessage ?? PleyError.nothingAnswers)
    }

    /// A redirect from http to https on the same host (a proxy that wants
    /// https): that origin, else `nil`.
    static func upgrade(of url: URL, location: String?) -> String? {
        guard let location, let target = URL(string: location, relativeTo: url)?.absoluteURL,
              target.scheme?.lowercased() == "https", url.scheme?.lowercased() == "http",
              target.host(percentEncoded: false)?.lowercased() == url.host(percentEncoded: false)?.lowercased()
        else { return nil }
        return Addresses.origin(target)
    }
}
