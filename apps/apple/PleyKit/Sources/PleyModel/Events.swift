import Foundation

/// How far a library's scan has got (docs/API.md, `ScanStatus`), as
/// `GET /api/activity` gives it, or as the scan events tell it.
public struct ScanStatus: Codable, Hashable, Sendable {
    public var libraryId: Int
    public var libraryName: String
    /// `queued`, `walking`, `probing`, `metadata` or `done`.
    public var phase: String
    public var processed: Int
    public var total: Int

    public init(libraryId: Int, libraryName: String = "", phase: String = "queued", processed: Int = 0,
                total: Int = 0) {
        self.libraryId = libraryId
        self.libraryName = libraryName
        self.phase = phase
        self.processed = processed
        self.total = total
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        libraryId = try c.decode(Int.self, forKey: .libraryId)
        libraryName = try c.value(.libraryName, or: "")
        phase = try c.value(.phase, or: "queued")
        processed = try c.value(.processed, or: 0)
        total = try c.value(.total, or: 0)
    }
}

/// One event of `GET /api/events` (docs/API.md, `ServerEvent`): each
/// `data:` line is one, by its `type`. A type this app doesn't know reads
/// as `unknown`, so a newer server can add one.
public enum ServerEvent: Hashable, Sendable {
    case scanStarted(libraryId: Int)
    case scanProgress(libraryId: Int, phase: String, processed: Int, total: Int)
    case scanFinished(libraryId: Int)
    /// Items or their metadata changed: fetch the library again.
    case libraryChanged(libraryId: Int)
    /// A library was created, changed or deleted, or the order changed.
    case librariesChanged
    case sessionsChanged
    /// The profile's My List changed.
    case watchlistChanged
    /// The profile's watch state of these items changed: the item, then its
    /// parent and grandparent; with `children`, everything under the first too.
    case userStateChanged(itemIds: [Int], children: Bool)
    /// One of the profile's playbacks was stopped from elsewhere.
    case sessionStopped(sessionId: String, message: String?, byAdmin: Bool)
    case liveChannelsChanged
    case liveTunersChanged
    case unknown(String)

    private struct Wire: Decodable {
        var type: String
        var libraryId: Int?
        var phase: String?
        var processed: Int?
        var total: Int?
        var itemIds: [Int]?
        var children: Bool?
        var sessionId: String?
        var message: String?
        var byAdmin: Bool?
    }

    /// The event in `data` (one `data:` line's JSON); `nil` when it isn't
    /// an event at all, or one missing what its type needs.
    public static func parse(_ data: Data) -> ServerEvent? {
        guard let wire = try? PleyJSON.decoder.decode(Wire.self, from: data) else { return nil }
        switch wire.type {
        case "scan_started":
            return wire.libraryId.map { .scanStarted(libraryId: $0) }
        case "scan_progress":
            return wire.libraryId.map {
                .scanProgress(libraryId: $0, phase: wire.phase ?? "", processed: wire.processed ?? 0,
                              total: wire.total ?? 0)
            }
        case "scan_finished":
            return wire.libraryId.map { .scanFinished(libraryId: $0) }
        case "library_changed":
            return wire.libraryId.map { .libraryChanged(libraryId: $0) }
        case "libraries_changed": return .librariesChanged
        case "sessions_changed": return .sessionsChanged
        case "watchlist_changed": return .watchlistChanged
        case "user_state_changed":
            return .userStateChanged(itemIds: wire.itemIds ?? [], children: wire.children ?? false)
        case "session_stopped":
            return wire.sessionId.map {
                .sessionStopped(sessionId: $0, message: wire.message, byAdmin: wire.byAdmin ?? false)
            }
        case "live_channels_changed": return .liveChannelsChanged
        case "live_tuners_changed": return .liveTunersChanged
        default: return .unknown(wire.type)
        }
    }
}
