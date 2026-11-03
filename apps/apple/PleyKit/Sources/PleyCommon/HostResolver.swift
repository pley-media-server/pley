import Foundation

/// Looks up a host name's addresses, as text (`192.168.1.20`, `fd12::1`).
/// The plain-http rule checks every address a name has before a request
/// goes out; tests give a fake one.
public protocol HostResolver: Sendable {
    func addresses(of host: String) async throws -> [String]
}

/// Why a lookup gave no addresses.
public enum HostResolverError: Error, Sendable, Equatable {
    /// No answer within the time allowed.
    case timedOut
    /// The resolver's error (`EAI_NONAME` and the like).
    case failed(Int32)
}

/// The system's resolver (`getaddrinfo`, IPv4 and IPv6), given at most
/// `timeout` to answer.
public struct SystemHostResolver: HostResolver {
    public var timeout: Duration

    public init(timeout: Duration = .seconds(3)) {
        self.timeout = timeout
    }

    public func addresses(of host: String) async throws -> [String] {
        // getaddrinfo blocks and can't be cancelled: it runs on a detached
        // task, and a lookup that outlives the timeout finishes unheard.
        let lookup = Task.detached(priority: .userInitiated) { try Self.lookUp(host) }
        let timeout = self.timeout
        return try await withThrowingTaskGroup(of: [String].self) { group in
            group.addTask { try await lookup.value }
            group.addTask {
                try await Task.sleep(for: timeout)
                throw HostResolverError.timedOut
            }
            defer { group.cancelAll() }
            guard let first = try await group.next() else { throw HostResolverError.timedOut }
            return first
        }
    }

    private static func lookUp(_ host: String) throws -> [String] {
        var hints = addrinfo()
        hints.ai_family = AF_UNSPEC
        hints.ai_socktype = SOCK_STREAM
        var list: UnsafeMutablePointer<addrinfo>?
        let status = getaddrinfo(host, nil, &hints, &list)
        guard status == 0 else { throw HostResolverError.failed(status) }
        defer { freeaddrinfo(list) }
        var addresses: [String] = []
        var next = list
        while let info = next {
            var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            if getnameinfo(info.pointee.ai_addr, info.pointee.ai_addrlen, &buffer, socklen_t(buffer.count), nil, 0,
                           NI_NUMERICHOST) == 0 {
                let address = String(decoding: buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
                if !addresses.contains(address) { addresses.append(address) }
            }
            next = info.pointee.ai_next
        }
        return addresses
    }
}
