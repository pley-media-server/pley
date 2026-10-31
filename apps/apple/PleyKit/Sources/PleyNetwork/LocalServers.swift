import Foundation
import Network
import PleyCommon
import PleyModel

/// What a search of the local network shows: the servers confirmed so far,
/// sorted by name; `blocked` when the system refused the app the local
/// network (so finding nothing says nothing); `searching` while
/// announcements are still being checked, or browsing has only just begun.
public struct DiscoveryState: Sendable, Equatable {
    public var servers: [FoundServer]
    public var blocked: Bool
    public var searching: Bool

    public init(servers: [FoundServer] = [], blocked: Bool = false, searching: Bool = true) {
        self.servers = servers
        self.blocked = blocked
        self.searching = searching
    }
}

/// Finds Pley servers on the local network by Bonjour. Each announcement
/// is resolved to an address, which must be private, and then confirmed by
/// the server itself (`GET /api/server` at `http://<address>:<port>`):
/// anyone on the network can announce, so a server is listed only once it
/// answers as Pley. Its name and version come from that answer (the TXT
/// record's name if the answer has none), its id from the TXT record.
public final class LocalServers: Sendable {
    public typealias Resolve = @Sendable (NWEndpoint) async -> ResolvedService?

    /// The most announcements resolved and confirmed at once.
    public static let maxConcurrent = 4

    private let browser: any ServiceBrowsing
    private let resolve: Resolve
    private let probe: ServerProbe
    private let confirmTimeout: Duration
    private let settle: Duration

    /// `confirmTimeout` bounds resolving and confirming one announcement;
    /// one that takes longer is left out. `settle` is how long `searching`
    /// stays on at the start, while the first announcements come in.
    public init(transport: HTTPTransport, browser: any ServiceBrowsing = BonjourBrowser(),
                resolve: @escaping Resolve = { await BonjourBrowser.resolve($0) },
                confirmTimeout: Duration = .seconds(5), settle: Duration = .seconds(2)) {
        self.browser = browser
        self.resolve = resolve
        probe = ServerProbe(transport: transport)
        self.confirmTimeout = confirmTimeout
        self.settle = settle
    }

    /// The search's state each time it changes, starting with an empty,
    /// searching one. Browsing stops when the consumer stops reading.
    public func browse() -> AsyncStream<DiscoveryState> {
        let (stream, continuation) = AsyncStream.makeStream(of: DiscoveryState.self,
                                                            bufferingPolicy: .bufferingNewest(8))
        let search = Search(resolve: resolve, probe: probe, timeout: confirmTimeout, output: continuation)
        let events = browser.results()
        let settle = settle
        let task = Task {
            await search.publish()
            let settling = Task {
                try? await Task.sleep(for: settle)
                await search.settled()
            }
            for await event in events {
                await search.handle(event)
            }
            settling.cancel()
            await search.stop()
        }
        continuation.onTermination = { _ in task.cancel() }
        return stream
    }
}

/// One browse's state, one announcement at a time.
private actor Search {
    private struct Lead {
        let announcement: BonjourAnnouncement
        let endpoint: NWEndpoint
        /// Which announcement of the instance this is; a check of an older one is ignored.
        let generation: Int
    }

    private let resolve: LocalServers.Resolve
    private let probe: ServerProbe
    private let timeout: Duration
    private let output: AsyncStream<DiscoveryState>.Continuation

    private var leads: [String: Lead] = [:]
    private var queue: [String] = []
    private var running: [String: Task<Void, Never>] = [:]
    private var confirmed: [String: FoundServer] = [:]
    private var generation = 0
    private var blocked = false
    private var settling = true
    private var last: DiscoveryState?

    init(resolve: @escaping LocalServers.Resolve, probe: ServerProbe, timeout: Duration,
         output: AsyncStream<DiscoveryState>.Continuation) {
        self.resolve = resolve
        self.probe = probe
        self.timeout = timeout
        self.output = output
    }

    func handle(_ event: BonjourEvent) {
        switch event {
        case .found(let announcement, let endpoint):
            blocked = false
            generation += 1
            let instance = announcement.instance
            leads[instance] = Lead(announcement: announcement, endpoint: endpoint, generation: generation)
            running[instance]?.cancel()
            running[instance] = nil
            if !queue.contains(instance) { queue.append(instance) }
            startChecks()
        case .lost(let instance):
            leads[instance] = nil
            queue.removeAll { $0 == instance }
            running.removeValue(forKey: instance)?.cancel()
            confirmed[instance] = nil
            startChecks()
        case .blocked:
            blocked = true
        case .failed(let reason):
            Log.discovery.error("discovery stopped: \(reason, privacy: .public)")
            settling = false
        }
        publish()
    }

    func settled() {
        settling = false
        publish()
    }

    func stop() {
        for task in running.values { task.cancel() }
        running = [:]
        queue = []
        settling = false
        publish()
        output.finish()
    }

    /// Sends the state when it changed.
    func publish() {
        // One entry per server: two instances with one id (a rename in
        // progress) show once.
        var seen = Set<String>()
        let servers = confirmed.values
            .sorted { ($0.name.lowercased(), $0.origin) < ($1.name.lowercased(), $1.origin) }
            .filter { seen.insert($0.id ?? $0.origin).inserted && seen.insert($0.origin).inserted }
        let state = DiscoveryState(servers: servers, blocked: blocked && servers.isEmpty,
                                   searching: settling || !queue.isEmpty || !running.isEmpty)
        guard state != last else { return }
        last = state
        output.yield(state)
    }

    private func startChecks() {
        while running.count < LocalServers.maxConcurrent, !queue.isEmpty {
            let instance = queue.removeFirst()
            guard let lead = leads[instance] else { continue }
            let resolve = resolve
            let probe = probe
            let timeout = timeout
            running[instance] = Task {
                let server = await Self.check(lead.announcement, lead.endpoint, resolve: resolve, probe: probe,
                                              timeout: timeout)
                self.checked(instance, generation: lead.generation, server: server)
            }
        }
    }

    private func checked(_ instance: String, generation: Int, server: FoundServer?) {
        guard leads[instance]?.generation == generation else { return }
        running[instance] = nil
        if let server {
            confirmed[instance] = server
        } else {
            confirmed[instance] = nil
            Log.discovery.info("an announcement wasn't confirmed")
        }
        startChecks()
        publish()
    }

    /// The server behind an announcement, if it answers as Pley at a
    /// private address within `timeout`.
    private static func check(_ announcement: BonjourAnnouncement, _ endpoint: NWEndpoint,
                              resolve: @escaping LocalServers.Resolve, probe: ServerProbe,
                              timeout: Duration) async -> FoundServer? {
        await withTaskGroup(of: FoundServer?.self) { group in
            group.addTask {
                guard let service = await resolve(endpoint),
                      PrivateAddress.isPrivate(literal: service.host) == true,
                      (1...65535).contains(service.port)
                else { return nil }
                guard case .pley(let origin, let info) = await probe.probe(candidates: [service.httpOrigin]),
                      !Task.isCancelled
                else { return nil }
                let name = BonjourTXT.cleanName(info.name)
                return FoundServer(origin: origin, name: name.isEmpty ? announcement.name : name,
                                   id: announcement.id, version: info.version)
            }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return nil
            }
            let first = await group.next() ?? nil
            group.cancelAll()
            return first
        }
    }
}
