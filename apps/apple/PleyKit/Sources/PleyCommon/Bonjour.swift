import Foundation
import Network
import os

/// What a server's Bonjour announcement says about it, bounded and cleaned
/// (docs/API.md, "Discovery"). Anyone on the network can announce, so this
/// is only a lead: a server is listed once its own `GET /api/server` has
/// answered (`LocalServers`).
public struct BonjourAnnouncement: Sendable, Hashable {
    /// The DNS-SD instance name, which can change when two servers share one.
    public let instance: String
    /// The server's id, which stays the same across restarts and renames.
    public let id: String
    public let name: String
    public let version: String?

    public init(instance: String, id: String, name: String, version: String?) {
        self.instance = instance
        self.id = id
        self.name = name
        self.version = version
    }
}

/// Reads a `_pley._tcp` TXT record.
public enum BonjourTXT {
    /// The most keys a record may have.
    public static let maxKeys = 16
    /// The most bytes a record may have.
    public static let maxBytes = 1300

    /// The announcement in `record`, or `nil` when it isn't one: `pley`
    /// must be `1` and `id` 1 to 64 letters, digits and hyphens. The name is
    /// cleaned and cut to 64 characters, and falls back to `instance`; a
    /// version over 32 characters or with anything but printable ASCII is
    /// dropped. Other keys are ignored, but a record with more than 16 keys
    /// or 1,300 bytes is refused. `size` is the record's size in bytes as it
    /// came; without it, the size is counted from `record`.
    public static func parse(_ record: [String: String], instance: String, size: Int? = nil) -> BonjourAnnouncement? {
        guard record.count <= maxKeys else { return nil }
        let bytes = size ?? record.reduce(0) { $0 + 2 + $1.key.utf8.count + $1.value.utf8.count }
        guard bytes <= maxBytes else { return nil }
        // DNS-SD's keys are case-insensitive.
        var fields: [String: String] = [:]
        for (key, value) in record { fields[key.lowercased()] = value }

        guard fields["pley"] == "1", let id = fields["id"], (1...64).contains(id.count),
              id.unicodeScalars.allSatisfy({ $0.isASCII && (CharacterSet.alphanumerics.contains($0) || $0 == "-") })
        else { return nil }
        let instanceName = cleanName(instance)
        let name = fields["name"].map(cleanName).flatMap { $0.isEmpty ? nil : $0 } ?? instanceName
        guard !name.isEmpty else { return nil }
        var version = fields["version"]
        if let text = version,
           text.isEmpty || text.count > 32 || !text.unicodeScalars.allSatisfy({ (0x21...0x7e).contains($0.value) }) {
            version = nil
        }
        return BonjourAnnouncement(instance: instanceName.isEmpty ? name : instanceName, id: id, name: name,
                                   version: version)
    }

    /// A server name to show: one line without control or format
    /// characters, trimmed, at most 64 characters.
    public static func cleanName(_ text: String) -> String {
        String(plain(text).prefix(64)).trimmingCharacters(in: .whitespaces)
    }
}

/// What browsing for `_pley._tcp` tells.
public enum BonjourEvent: Sendable, Hashable {
    /// A server announced itself (or changed its announcement); `endpoint`
    /// is the service, to resolve.
    case found(BonjourAnnouncement, NWEndpoint)
    /// The instance stopped announcing itself.
    case lost(instance: String)
    /// The system refused the app the local network.
    case blocked
    /// Browsing stopped; why, for the log.
    case failed(String)
}

/// Something that browses for servers: `BonjourBrowser`, or a test's own.
public protocol ServiceBrowsing: Sendable {
    /// The events of one browse, which ends when the stream's consumer
    /// stops reading.
    func results() -> AsyncStream<BonjourEvent>
}

/// Where a service answered: an IP address (IPv6 without brackets or zone)
/// and a port.
public struct ResolvedService: Sendable, Hashable {
    public let host: String
    public let port: Int

    public init(host: String, port: Int) {
        self.host = host
        self.port = port
    }

    /// `http://host:port`, with an IPv6 address in brackets.
    public var httpOrigin: String {
        host.contains(":") ? "http://[\(host)]:\(port)" : "http://\(host):\(port)"
    }
}

/// Browses for Pley servers by Bonjour (`_pley._tcp` in `local.`, with
/// their TXT records), with Network.framework's `NWBrowser`. No multicast
/// entitlement is needed: the system browses for the app.
public final class BonjourBrowser: ServiceBrowsing {
    public static let serviceType = "_pley._tcp"
    /// The most servers followed at once; more are ignored.
    public static let maxInstances = 32

    public init() {}

    public func results() -> AsyncStream<BonjourEvent> {
        AsyncStream { continuation in
            let parameters = NWParameters()
            parameters.includePeerToPeer = false
            let browser = Box(NWBrowser(for: .bonjourWithTXTRecord(type: Self.serviceType, domain: "local."),
                                        using: parameters))
            let known = OSAllocatedUnfairLock(initialState: [String: BonjourAnnouncement]())
            browser.value.stateUpdateHandler = { state in
                switch state {
                case .waiting(let error), .failed(let error):
                    if Self.isPolicyDenied(error) {
                        Log.discovery.info("local network access refused")
                        continuation.yield(.blocked)
                    } else if case .failed = state {
                        Log.discovery.error("browsing failed: \(error.debugDescription, privacy: .public)")
                        continuation.yield(.failed(error.debugDescription))
                        continuation.finish()
                    }
                default:
                    break
                }
            }
            browser.value.browseResultsChangedHandler = { results, _ in
                let events = known.withLock { Self.changes(from: &$0, to: results) }
                for event in events { continuation.yield(event) }
            }
            continuation.onTermination = { _ in browser.value.cancel() }
            browser.value.start(queue: DispatchQueue(label: "app.pley.apple.bonjour"))
        }
    }

    /// The events that take `known` (by instance name) to `results`, which
    /// the browser gives in full each time. A service seen on several
    /// interfaces is one instance.
    static func changes(from known: inout [String: BonjourAnnouncement],
                        to results: Set<NWBrowser.Result>) -> [BonjourEvent] {
        var announced: [String: (BonjourAnnouncement, NWEndpoint)] = [:]
        // Sorted, so the same endpoint is kept for an instance each time.
        for result in results.sorted(by: { $0.endpoint.debugDescription < $1.endpoint.debugDescription }) {
            guard case .service(let instance, _, _, _) = result.endpoint,
                  case .bonjour(let txt) = result.metadata, announced[instance] == nil,
                  let announcement = BonjourTXT.parse(txt.dictionary, instance: instance, size: txt.data.count)
            else { continue }
            announced[instance] = (announcement, result.endpoint)
        }
        // The instances already followed keep their place; new ones fill
        // what is left of the 32, by name.
        let order = announced.keys.sorted { (known[$0] == nil ? 1 : 0, $0) < (known[$1] == nil ? 1 : 0, $1) }
        var current: [String: (BonjourAnnouncement, NWEndpoint)] = [:]
        for instance in order.prefix(maxInstances) { current[instance] = announced[instance] }
        var events: [BonjourEvent] = []
        for instance in known.keys.sorted() where current[instance] == nil {
            known[instance] = nil
            events.append(.lost(instance: instance))
        }
        for (instance, (announcement, endpoint)) in current.sorted(by: { $0.key < $1.key })
        where known[instance] != announcement {
            known[instance] = announcement
            events.append(.found(announcement, endpoint))
        }
        return events
    }

    /// Whether the system refused the app the local network.
    static func isPolicyDenied(_ error: NWError) -> Bool {
        if case .dns(let code) = error { return code == DNSServiceErrorType(kDNSServiceErr_PolicyDenied) }
        return false
    }

    /// Where `endpoint`, a Bonjour service, answers: a TCP connection is
    /// opened to it and closed again once ready. An IPv4 address is
    /// preferred; an IPv6 one is used only when it isn't link-local (its
    /// zone can't be part of an address to open). The address must be
    /// private; otherwise, or after `timeout`, `nil`.
    public static func resolve(_ endpoint: NWEndpoint, timeout: Duration = .seconds(3)) async -> ResolvedService? {
        enum Attempt { case v4, v6, timeout }
        return await withTaskGroup(of: (Attempt, ResolvedService?).self) { group in
            group.addTask { (.v4, await connect(endpoint, version: .v4)) }
            group.addTask { (.v6, await connect(endpoint, version: .v6)) }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return (.timeout, nil)
            }
            var v6: ResolvedService?
            var tried = 0
            for await (attempt, found) in group {
                switch attempt {
                case .v4:
                    if let found {
                        group.cancelAll()
                        return found
                    }
                    tried += 1
                case .v6:
                    v6 = found
                    tried += 1
                case .timeout:
                    tried = 2
                }
                if tried >= 2 { break }
            }
            group.cancelAll()
            return v6
        }
    }

    /// A connection to `endpoint` over IP `version`: where it went, once
    /// ready; `nil` when it fails or the task is cancelled.
    private static func connect(_ endpoint: NWEndpoint, version: NWProtocolIP.Options.Version) async
        -> ResolvedService? {
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = false
        if let ip = parameters.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
            ip.version = version
        }
        let connection = Box(NWConnection(to: endpoint, using: parameters))
        let once = OSAllocatedUnfairLock(initialState: false)
        return await withTaskCancellationHandler {
            await withCheckedContinuation { (continuation: CheckedContinuation<ResolvedService?, Never>) in
                let finish: @Sendable (ResolvedService?) -> Void = { result in
                    guard once.withLock({ done in defer { done = true }; return !done }) else { return }
                    connection.value.cancel()
                    continuation.resume(returning: result)
                }
                connection.value.stateUpdateHandler = { state in
                    switch state {
                    case .ready:
                        finish(connection.value.currentPath?.remoteEndpoint.flatMap(address(of:)))
                    case .failed, .cancelled:
                        finish(nil)
                    case .waiting:
                        // No route, nothing listening or no permission:
                        // waiting would only retry.
                        finish(nil)
                    default:
                        break
                    }
                }
                connection.value.start(queue: DispatchQueue(label: "app.pley.apple.resolve"))
            }
        } onCancel: {
            connection.value.cancel()
        }
    }

    /// The private, openable address of a connected endpoint.
    public static func address(of endpoint: NWEndpoint) -> ResolvedService? {
        guard case .hostPort(let host, let port) = endpoint else { return nil }
        let text: String
        switch host {
        case .ipv4(let address):
            guard PrivateAddress.isPrivate(address) else { return nil }
            text = ipText(address.rawValue, family: AF_INET)
        case .ipv6(let address):
            let bytes = [UInt8](address.rawValue)
            // An IPv4-mapped address is opened as IPv4.
            if bytes.count == 16, bytes[0..<10].allSatisfy({ $0 == 0 }), bytes[10] == 0xff, bytes[11] == 0xff {
                guard PrivateAddress.isPrivate(address) else { return nil }
                text = ipText(Data(bytes[12..<16]), family: AF_INET)
            } else {
                guard !address.isLinkLocal, PrivateAddress.isPrivate(address) else { return nil }
                text = ipText(address.rawValue, family: AF_INET6)
            }
        default:
            return nil
        }
        guard !text.isEmpty else { return nil }
        return ResolvedService(host: text, port: Int(port.rawValue))
    }

    /// An address as text, without a zone.
    private static func ipText(_ raw: Data, family: Int32) -> String {
        var buffer = [CChar](repeating: 0, count: Int(INET6_ADDRSTRLEN))
        let ok = raw.withUnsafeBytes { bytes in
            inet_ntop(family, bytes.baseAddress, &buffer, socklen_t(buffer.count)) != nil
        }
        return ok ? String(decoding: buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self) : ""
    }
}

/// Network's browsers and connections are Sendable from iOS 17 only; each
/// is used through its own handlers and cancelled once.
private final class Box<Value>: @unchecked Sendable {
    let value: Value
    init(_ value: Value) { self.value = value }
}
