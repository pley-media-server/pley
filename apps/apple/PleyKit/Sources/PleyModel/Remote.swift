import Foundation

/// A command a remote sends a player (docs/API.md, "Remote control"). A
/// play request lists the ones its player obeys; Stop needs none, since
/// every player obeys it.
public struct Control: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let play = Control(rawValue: "play")
    public static let pause = Control(rawValue: "pause")
    /// To a place (`position_ms`).
    public static let seek = Control(rawValue: "seek")
    /// Forward or back from where the player is (`offset_ms`).
    public static let skip = Control(rawValue: "skip")
    /// The next track or episode; on Live TV, the next channel.
    public static let next = Control(rawValue: "next")
    /// The track before; on Live TV, the channel before. Videos have none.
    public static let previous = Control(rawValue: "previous")
}

/// One command: the body of `POST /api/sessions/{id}/command`, and what
/// the player hears of it in `player_command`. `seek` carries
/// `positionMs`, `skip` `offsetMs` (back when negative), the others neither.
public struct PlayerCommand: Codable, Hashable, Sendable {
    public var command: Control
    public var positionMs: Int?
    public var offsetMs: Int?

    public init(_ command: Control, positionMs: Int? = nil, offsetMs: Int? = nil) {
        self.command = command
        self.positionMs = positionMs
        self.offsetMs = offsetMs
    }

    public static let play = PlayerCommand(.play)
    public static let pause = PlayerCommand(.pause)
    public static let next = PlayerCommand(.next)
    public static let previous = PlayerCommand(.previous)

    public static func seek(toMs positionMs: Int) -> PlayerCommand { PlayerCommand(.seek, positionMs: positionMs) }
    public static func skip(byMs offsetMs: Int) -> PlayerCommand { PlayerCommand(.skip, offsetMs: offsetMs) }
}

/// What a session plays: a title of the library, or a Live TV channel.
public struct SessionKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let item = SessionKind(rawValue: "item")
    public static let live = SessionKind(rawValue: "live")
}

/// Where a session's player is, as it last reported.
public struct PlayingState: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let playing = PlayingState(rawValue: "playing")
    public static let paused = PlayingState(rawValue: "paused")
    public static let buffering = PlayingState(rawValue: "buffering")
}

/// The channel a Live TV session watches; `now`: the programme on.
public struct PlayingChannel: Codable, Hashable, Sendable {
    public var id: Int
    public var name: String
    public var number: String?
    /// Its logo's address on the server.
    public var logo: String?
    public var now: String?

    public init(id: Int, name: String, number: String? = nil, logo: String? = nil, now: String? = nil) {
        self.id = id
        self.name = name
        self.number = number
        self.logo = logo
        self.now = now
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        number = try c.decodeIfPresent(String.self, forKey: .number)
        logo = try c.decodeIfPresent(String.self, forKey: .logo)
        now = try c.decodeIfPresent(String.self, forKey: .now)
    }
}

/// One of the profile's sessions whose player is there (`GET /api/sessions`):
/// what plays where, and what its player obeys. A live one is always
/// `playing`, at 0, with no length.
public struct PlayingSession: Codable, Hashable, Sendable, Identifiable {
    public var sessionId: String
    public var kind: SessionKind
    /// What remotes call the device ("Living room TV", "Pley for iPhone").
    public var device: String?
    /// What it plays, for `item`.
    public var item: ItemSummary?
    /// What it watches, for `live`.
    public var channel: PlayingChannel?
    public var state: PlayingState
    public var positionMs: Int
    /// `nil` when the length isn't known, and on Live TV.
    public var durationMs: Int?
    /// Its player's last report, unix seconds by the server's clock.
    public var updatedAt: Int
    /// How long before the answer that report was, by the server's clock:
    /// a remote moves the place on from it by its own clock, never
    /// comparing the two.
    public var ageMs: Int
    /// What its player obeys; Stop always works.
    public var controls: [Control]

    public var id: String { sessionId }

    public init(sessionId: String, kind: SessionKind = .item, device: String? = nil, item: ItemSummary? = nil,
                channel: PlayingChannel? = nil, state: PlayingState = .playing, positionMs: Int = 0,
                durationMs: Int? = nil, updatedAt: Int = 0, ageMs: Int = 0, controls: [Control] = []) {
        self.sessionId = sessionId
        self.kind = kind
        self.device = device
        self.item = item
        self.channel = channel
        self.state = state
        self.positionMs = positionMs
        self.durationMs = durationMs
        self.updatedAt = updatedAt
        self.ageMs = ageMs
        self.controls = controls
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        kind = try c.value(.kind, or: .item)
        device = try c.decodeIfPresent(String.self, forKey: .device)
        // A title or channel that can't be read leaves the session listed all the same.
        item = try? c.decodeIfPresent(ItemSummary.self, forKey: .item)
        channel = try? c.decodeIfPresent(PlayingChannel.self, forKey: .channel)
        state = try c.value(.state, or: .playing)
        positionMs = try c.value(.positionMs, or: 0)
        durationMs = try c.decodeIfPresent(Int.self, forKey: .durationMs)
        updatedAt = try c.value(.updatedAt, or: 0)
        ageMs = try c.value(.ageMs, or: 0)
        controls = try c.value(.controls, or: [])
    }

    /// Whether its player obeys `control`.
    public func obeys(_ control: Control) -> Bool { controls.contains(control) }
}
