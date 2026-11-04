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
    /// One of the profile's sessions started, ended or changed (a pause, a
    /// seek, another channel): remotes read the list again.
    case sessionsChanged
    /// The profile's My List changed.
    case watchlistChanged
    /// The profile's watch state of these items changed: the item, then its
    /// parent and grandparent; with `children`, everything under the first too.
    case userStateChanged(itemIds: [Int], children: Bool)
    /// One of the profile's playbacks was stopped from elsewhere.
    case sessionStopped(sessionId: String, message: String?, byAdmin: Bool)
    /// The profile's remote, on another device, sent the player of
    /// `sessionId` a command.
    case playerCommand(sessionId: String, command: PlayerCommand)
    /// The profile's remote, on another device, sent the device of
    /// `receiverId` something to play; `from` is the remote's device, as
    /// the server names it.
    case cast(receiverId: String, target: CastTarget, from: String?)
    case liveChannelsChanged
    case liveTunersChanged
    /// A request changed (`requestId` with its title), or many did (all
    /// `nil`): what shows it is read again.
    case requestsChanged(requestId: Int?, kind: TitleKind?, tmdbId: Int?)
    /// One of the profile's requests is ready to watch: the whole of it
    /// (`complete`), or `seasons` of a show.
    case requestAvailable(requestId: Int, title: String, kind: TitleKind, tmdbId: Int, itemId: Int?,
                          posterUrl: String?, seasons: [Int]?, complete: Bool)
    /// A manager approved or declined one of the profile's requests.
    case requestDecided(requestId: Int, title: String, kind: TitleKind, tmdbId: Int, status: RequestStatus,
                        reason: String?)
    /// Managers: someone asked for something (`pending` until decided).
    case requestCreated(requestId: Int, title: String, kind: TitleKind, tmdbId: Int, seasons: [Int]?,
                        by: RequestUser?, status: RequestStatus)
    /// Managers: a request couldn't be sent to Radarr or Sonarr.
    case requestFailed(requestId: Int, title: String, kind: TitleKind, reason: String?)
    /// Requests' settings changed: the summary may have.
    case requestsSettingsChanged
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
        var requestId: Int?
        var kind: TitleKind?
        var tmdbId: Int?
        var title: String?
        var itemId: Int?
        var posterUrl: String?
        var seasons: [Int]?
        var complete: Bool?
        var status: RequestStatus?
        var reason: String?
        var requestedBy: RequestUser?
        var command: Control?
        var positionMs: Int?
        var offsetMs: Int?
        var streamIndex: Int?
        var receiverId: String?
        var playlistId: Int?
        var channelId: Int?
        var startMs: Int?
        var shuffle: Bool?
        var from: String?
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
        case "player_command":
            guard let id = wire.sessionId, let command = wire.command else { return .unknown(wire.type) }
            return .playerCommand(sessionId: id, command: PlayerCommand(command, positionMs: wire.positionMs,
                                                                         offsetMs: wire.offsetMs,
                                                                         streamIndex: wire.streamIndex))
        case "cast":
            // One of a title, a playlist and a channel; without one, or a receiver, there is nothing to play.
            let what: CastTarget.What?
            if let id = wire.itemId {
                what = .item(id)
            } else if let id = wire.playlistId {
                what = .playlist(id)
            } else {
                what = wire.channelId.map { CastTarget.What.channel($0) }
            }
            guard let receiver = wire.receiverId, let what else { return .unknown(wire.type) }
            let target = CastTarget(what, startMs: wire.startMs, shuffle: wire.shuffle ?? false)
            return .cast(receiverId: receiver, target: target, from: wire.from)
        case "live_channels_changed": return .liveChannelsChanged
        case "live_tuners_changed": return .liveTunersChanged
        case "requests_changed":
            return .requestsChanged(requestId: wire.requestId, kind: wire.kind, tmdbId: wire.tmdbId)
        case "request_available":
            guard let id = wire.requestId, let title = wire.title, let kind = wire.kind, let tmdbId = wire.tmdbId
            else { return .unknown(wire.type) }
            return .requestAvailable(requestId: id, title: title, kind: kind, tmdbId: tmdbId, itemId: wire.itemId,
                                     posterUrl: wire.posterUrl, seasons: wire.seasons, complete: wire.complete ?? true)
        case "request_decided":
            guard let id = wire.requestId, let title = wire.title, let kind = wire.kind, let tmdbId = wire.tmdbId,
                  let status = wire.status
            else { return .unknown(wire.type) }
            return .requestDecided(requestId: id, title: title, kind: kind, tmdbId: tmdbId, status: status,
                                   reason: wire.reason)
        case "request_created":
            guard let id = wire.requestId, let title = wire.title, let kind = wire.kind, let tmdbId = wire.tmdbId
            else { return .unknown(wire.type) }
            return .requestCreated(requestId: id, title: title, kind: kind, tmdbId: tmdbId, seasons: wire.seasons,
                                   by: wire.requestedBy, status: wire.status ?? .pending)
        case "request_failed":
            guard let id = wire.requestId, let title = wire.title, let kind = wire.kind else { return .unknown(wire.type) }
            return .requestFailed(requestId: id, title: title, kind: kind, reason: wire.reason)
        case "requests_settings_changed": return .requestsSettingsChanged
        default: return .unknown(wire.type)
        }
    }
}
