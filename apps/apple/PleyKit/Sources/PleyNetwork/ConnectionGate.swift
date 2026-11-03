import Foundation
import os
import PleyCommon

/// Pley's rule for plain http (SECURITY.md): codes and tokens travel over
/// it only to private addresses, the home network's. https always passes.
///
/// URLSession has no hook between connecting and sending, so the rule is
/// checked twice: before a request, on the addresses the host stands for
/// (an IP address as it is, a name by looking it up; every address must be
/// private), and after it, on the address the connection actually reached
/// (from the task's metrics). An answer from anywhere else is thrown away.
/// App Transport Security already lets plain http reach only IP addresses,
/// names without dots and `.local` names, which narrows the gap between
/// the two checks to a name whose lookup changes in between. Plain http
/// uses no proxy (`HTTPTransport`), so the address looked up is the one
/// connected to, not a proxy's.
public final class ConnectionGate: Sendable {
    private let resolver: any HostResolver
    private let isPrivate: @Sendable (String) -> Bool
    private let cacheFor: Duration
    private let verdicts = OSAllocatedUnfairLock(initialState: [String: (Bool, ContinuousClock.Instant)]())

    /// `isPrivate` judges one address as text; tests give their own.
    public init(resolver: any HostResolver = SystemHostResolver(),
                isPrivate: @escaping @Sendable (String) -> Bool = { PrivateAddress.isPrivate(literal: $0) == true },
                cacheFor: Duration = .seconds(30)) {
        self.resolver = resolver
        self.isPrivate = isPrivate
        self.cacheFor = cacheFor
    }

    /// Before a plain http request to `host`: throws `.unencrypted` unless
    /// every address it stands for is private. A name's verdict is kept for
    /// 30 seconds.
    public func check(host: String) async throws {
        let host = host.lowercased()
        let bare = host.hasPrefix("[") && host.hasSuffix("]") ? String(host.dropFirst().dropLast()) : host
        if PrivateAddress.isPrivate(literal: bare) != nil {
            guard isPrivate(bare) else { throw PleyError.unencrypted }
            return
        }
        let now = ContinuousClock.now
        if let (verdict, until) = verdicts.withLock({ $0[bare] }), until > now {
            guard verdict else { throw PleyError.unencrypted }
            return
        }
        let addresses: [String]
        do {
            addresses = try await resolver.addresses(of: bare)
        } catch {
            throw PleyError.from(error)
        }
        let verdict = !addresses.isEmpty && addresses.allSatisfy(isPrivate)
        let until = now.advanced(by: cacheFor)
        verdicts.withLock { $0[bare] = (verdict, until) }
        guard verdict else {
            Log.network.notice("plain http refused: the name's addresses aren't private")
            throw PleyError.unencrypted
        }
    }

    /// After a plain http request: throws `.unencrypted` unless the
    /// connection reached a private address. No address known is refused.
    public func verify(remoteAddress: String?) throws {
        guard let remoteAddress, !remoteAddress.isEmpty, isPrivate(remoteAddress) else {
            Log.network.error("plain http answer refused: it came from an address that isn't private")
            throw PleyError.unencrypted
        }
    }

    /// `verify(remoteAddress:)` for each plain http connection a task made.
    public func verify(metrics: URLSessionTaskMetrics?) throws {
        let hops = (metrics?.transactionMetrics ?? []).filter {
            $0.resourceFetchType == .networkLoad && $0.request.url?.scheme?.lowercased() == "http"
        }
        guard !hops.isEmpty else { throw PleyError.unencrypted }
        for hop in hops { try verify(remoteAddress: hop.remoteAddress) }
    }
}
