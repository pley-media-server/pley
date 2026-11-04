import Foundation
import PleyModel
import PleyNetwork

/// Requests that changed: the screens showing them read them again.
public struct RequestChange: Hashable, Sendable {
    /// Many changed, or what changed is unknown: whatever shows requests is read again.
    public var all = false
    /// Requests that changed, by id.
    public var ids: Set<Int> = []
    /// Titles whose requests changed, as `DiscoverTitle.key` gives them (`movie:933260`).
    public var titles: Set<String> = []

    public init(all: Bool = false, ids: Set<Int> = [], titles: Set<String> = []) {
        self.all = all
        self.ids = ids
        self.titles = titles
    }

    /// One request, with its title when known.
    public init(id: Int?, kind: TitleKind?, tmdbId: Int?) {
        if let id { ids = [id] }
        if let kind, let tmdbId { titles = ["\(kind.rawValue):\(tmdbId)"] }
        all = id == nil && tmdbId == nil
    }

    public static let everything = RequestChange(all: true)

    public var isEmpty: Bool { !all && ids.isEmpty && titles.isEmpty }

    /// Whether it may be about the title `kind` `tmdbId`.
    public func about(_ kind: TitleKind, tmdbId: Int) -> Bool {
        all || titles.contains("\(kind.rawValue):\(tmdbId)")
    }

    /// Whether it may be about request `id`.
    public func about(request id: Int) -> Bool { all || ids.contains(id) }

    public mutating func merge(_ other: RequestChange) {
        all = all || other.all
        ids.formUnion(other.ids)
        titles.formUnion(other.titles)
    }
}

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
    /// Live TV's channels (a source added, refreshed or removed, or its access changed).
    public var live = false
    /// Requests, and the titles they ask for.
    public var requests = RequestChange()
    /// Everything: the stream connected again, and what came meanwhile is unknown.
    public var everything = false

    public init(serial: Int = 0, home: Bool = false, libraries: Bool = false, libraryIds: Set<Int> = [],
                itemIds: Set<Int> = [], parentIds: Set<Int> = [], watchlist: Bool = false, playlists: Bool = false,
                live: Bool = false, requests: RequestChange = RequestChange(), everything: Bool = false) {
        self.serial = serial
        self.home = home
        self.libraries = libraries
        self.libraryIds = libraryIds
        self.itemIds = itemIds
        self.parentIds = parentIds
        self.watchlist = watchlist
        self.playlists = playlists
        self.live = live
        self.requests = requests
        self.everything = everything
    }

    public var isEmpty: Bool {
        !home && !libraries && libraryIds.isEmpty && itemIds.isEmpty && !watchlist && !playlists && !live
            && requests.isEmpty && !everything
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
        live = live || other.live
        requests.merge(other.requests)
        everything = everything || other.everything
    }
}

/// Turns the server's events into changes for the screens (Android's
/// `ServerEvents`): the changes of 0.3 seconds go out together, so a burst
/// of events (a play reaching 90 % sends two) fetches once. The scans
/// running are told as they move; a playback stopped from elsewhere, a
/// remote's command, the sessions changing and a request's news are told
/// at once.
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
    /// The profile's remote, on another device, sent a player a command:
    /// the player of `sessionId`, if it plays here, acts on it.
    public var onPlayerCommand: (_ sessionId: String, _ command: PlayerCommand) -> Void = { _, _ in }
    /// The profile's sessions started, ended or changed, or may have while
    /// the stream was down (it connected again): remotes read them again.
    public var onSessionsChanged: () -> Void = {}
    /// A request's news, as its event comes.
    public var onRequestNews: (RequestNews) -> Void = { _ in }
    /// Requests' changes, as their events come (before they are gathered).
    public var onRequestChange: (RequestChange) -> Void = { _ in }
    /// The server's settings for requests changed.
    public var onRequestsSettings: () -> Void = {}
    /// The stream connected (`true`) or went down.
    public var onConnection: (Bool) -> Void = { _ in }

    /// Whether the event stream is connected: while it isn't, what it
    /// would tell is polled for.
    public private(set) var connected = false
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
        if connected {
            connected = false
            onConnection(false)
        }
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
            connected = true
            onConnection(true)
            if connections > 1 {
                gather(ServerChange(home: true, libraries: true, watchlist: true, requests: .everything,
                                    everything: true))
                onSessionsChanged()
            }
        case .disconnected:
            guard connected else { return }
            connected = false
            onConnection(false)
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
        case .playerCommand(let id, let command):
            onPlayerCommand(id, command)
        case .sessionsChanged:
            onSessionsChanged()
        case .liveChannelsChanged:
            gather(ServerChange(live: true))
        case .requestsChanged(let id, let kind, let tmdbId):
            requests(RequestChange(id: id, kind: kind, tmdbId: tmdbId))
        case .requestAvailable(let id, let title, let kind, let tmdbId, let itemId, _, let seasons, let complete):
            onRequestNews(.ready(requestId: id, title: title, kind: kind, itemId: itemId, seasons: seasons,
                                 complete: complete))
            requests(RequestChange(id: id, kind: kind, tmdbId: tmdbId))
        case .requestDecided(let id, let title, let kind, let tmdbId, let status, let reason):
            if status == .declined {
                onRequestNews(.declined(requestId: id, title: title, reason: reason))
            } else if status == .approved {
                onRequestNews(.approved(requestId: id, title: title))
            }
            requests(RequestChange(id: id, kind: kind, tmdbId: tmdbId))
        case .requestCreated(let id, let title, let kind, let tmdbId, let seasons, let by, let status):
            // Managers hear of every request; only those waiting for them are told.
            if status == .pending { onRequestNews(.asked(requestId: id, title: title, by: by?.name, seasons: seasons)) }
            requests(RequestChange(id: id, kind: kind, tmdbId: tmdbId))
        case .requestFailed(let id, let title, let kind, let reason):
            onRequestNews(.failed(requestId: id, title: title, kind: kind, reason: reason))
            // The event doesn't say which title: only the request is read again.
            requests(RequestChange(ids: [id]))
        case .requestsSettingsChanged:
            onRequestsSettings()
            requests(.everything)
        case .liveTunersChanged, .unknown:
            break
        }
    }

    private func requests(_ change: RequestChange) {
        onRequestChange(change)
        gather(ServerChange(requests: change))
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
