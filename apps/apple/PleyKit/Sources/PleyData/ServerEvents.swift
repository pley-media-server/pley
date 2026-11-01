import Foundation
import PleyModel
import PleyNetwork

/// What changed on the server, gathered from its events: what the screens
/// showing it fetch again. Each change has a number of its own, so two
/// alike still differ.
public struct ServerChange: Hashable, Sendable {
    public var serial = 0
    /// Home's rows (watch state, My List, a library's items, the libraries).
    public var home = false
    /// The list of libraries, or their order.
    public var libraries = false
    /// Libraries whose items changed.
    public var libraryIds: Set<Int> = []
    /// Items whose watch state changed, with their parents.
    public var itemIds: Set<Int> = []
    /// Items everything under which changed too (a show marked as a whole).
    public var parentIds: Set<Int> = []
    /// My List.
    public var watchlist = false
    /// The profile's playlists (this app's own changes: the server sends no event for them).
    public var playlists = false
    /// Everything: the stream connected again, and what came meanwhile is unknown.
    public var everything = false

    public init(serial: Int = 0, home: Bool = false, libraries: Bool = false, libraryIds: Set<Int> = [],
                itemIds: Set<Int> = [], parentIds: Set<Int> = [], watchlist: Bool = false, playlists: Bool = false,
                everything: Bool = false) {
        self.serial = serial
        self.home = home
        self.libraries = libraries
        self.libraryIds = libraryIds
        self.itemIds = itemIds
        self.parentIds = parentIds
        self.watchlist = watchlist
        self.playlists = playlists
        self.everything = everything
    }

    public var isEmpty: Bool {
        !home && !libraries && libraryIds.isEmpty && itemIds.isEmpty && !watchlist && !playlists && !everything
    }

    /// Whether a screen showing `itemId` (whose parent is `parentId`, if any) fetches it again.
    public func names(_ itemId: Int, parent parentId: Int? = nil) -> Bool {
        everything || itemIds.contains(itemId) || parentId.map(parentIds.contains) == true
    }

    /// Whether a screen of library `id` fetches it again.
    public func touches(library id: Int) -> Bool { everything || libraryIds.contains(id) }

    /// `other` added to this one.
    public mutating func merge(_ other: ServerChange) {
        home = home || other.home
        libraries = libraries || other.libraries
        libraryIds.formUnion(other.libraryIds)
        itemIds.formUnion(other.itemIds)
        parentIds.formUnion(other.parentIds)
        watchlist = watchlist || other.watchlist
        playlists = playlists || other.playlists
        everything = everything || other.everything
    }
}

/// Turns the server's events into changes for the screens (Android's
/// `ServerEvents`): the changes of 0.3 seconds go out together, so a burst
/// of events (a play reaching 90 % sends two) fetches once. The scans
/// running are told as they move; a playback stopped from elsewhere is
/// told at once.
@MainActor
public final class ServerEvents {
    /// How long changes are gathered before they go out.
    public static let window: Double = 0.3

    /// The changes, gathered.
    public var onChange: (ServerChange) -> Void = { _ in }
    /// The scans running, by library.
    public var onScans: ([Int: ScanStatus]) -> Void = { _ in }
    /// A playback of this profile's was stopped from elsewhere.
    public var onSessionStopped: (_ sessionId: String, _ message: String?, _ byAdmin: Bool) -> Void = { _, _, _ in }

    public private(set) var scans: [Int: ScanStatus] = [:]
    private var pending = ServerChange()
    private var serial = 0
    private var flush: Task<Void, Never>?
    private var reading: Task<Void, Never>?
    private var connections = 0
    private let window: Double

    public init(window: Double = ServerEvents.window) {
        self.window = window
    }

    /// Reads `signals` until `stop`, or until they end.
    public func start(_ signals: AsyncStream<EventSignal>) {
        stop()
        connections = 0
        reading = Task { [weak self] in
            for await signal in signals {
                guard let self, !Task.isCancelled else { return }
                self.heard(signal)
            }
        }
    }

    /// Stops reading, and forgets the scans and the changes not sent yet.
    public func stop() {
        reading?.cancel()
        reading = nil
        flush?.cancel()
        flush = nil
        pending = ServerChange()
        if !scans.isEmpty {
            scans = [:]
            onScans(scans)
        }
    }

    /// Takes one signal in. A connection after the first means events may
    /// have been missed: everything is fetched again.
    public func heard(_ signal: EventSignal) {
        switch signal {
        case .connected:
            connections += 1
            if connections > 1 { gather(ServerChange(home: true, libraries: true, watchlist: true, everything: true)) }
        case .event(let event):
            heard(event)
        }
    }

    public func heard(_ event: ServerEvent) {
        switch event {
        case .userStateChanged(let ids, let children):
            var change = ServerChange(home: true, itemIds: Set(ids))
            if children, let first = ids.first { change.parentIds = [first] }
            gather(change)
        case .watchlistChanged:
            gather(ServerChange(home: true, watchlist: true))
        case .libraryChanged(let id):
            gather(ServerChange(home: true, libraryIds: [id]))
        case .librariesChanged:
            gather(ServerChange(home: true, libraries: true))
        case .scanStarted(let id):
            scans[id] = ScanStatus(libraryId: id, libraryName: scans[id]?.libraryName ?? "")
            onScans(scans)
        case .scanProgress(let id, let phase, let processed, let total):
            scans[id] = ScanStatus(libraryId: id, libraryName: scans[id]?.libraryName ?? "", phase: phase,
                                   processed: processed, total: total)
            onScans(scans)
        case .scanFinished(let id):
            scans[id] = nil
            onScans(scans)
            // Its items and its count changed.
            gather(ServerChange(home: true, libraries: true, libraryIds: [id]))
        case .sessionStopped(let id, let message, let byAdmin):
            onSessionStopped(id, message, byAdmin)
        case .sessionsChanged, .liveChannelsChanged, .liveTunersChanged, .unknown:
            break
        }
    }

    /// A change this app made itself, gathered with the server's: the
    /// other screens follow it even when the stream is slow to tell.
    public func note(_ change: ServerChange) {
        gather(change)
    }

    /// Scans the screens learnt of otherwise (a library listed as scanning).
    public func setScans(_ scans: [Int: ScanStatus]) {
        self.scans = scans
        onScans(scans)
    }

    private func gather(_ change: ServerChange) {
        pending.merge(change)
        guard flush == nil else { return }
        let window = window
        flush = Task { [weak self] in
            try? await Task.sleep(for: .seconds(window))
            guard !Task.isCancelled, let self else { return }
            self.flush = nil
            var out = self.pending
            self.pending = ServerChange()
            guard !out.isEmpty else { return }
            self.serial += 1
            out.serial = self.serial
            self.onChange(out)
        }
    }
}
