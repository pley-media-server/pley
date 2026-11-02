import Foundation

// MARK: Search

/// `GET /api/search` (docs/API.md, "Search"): items by relevance, people
/// whose name has words starting with each word asked for, and Live TV
/// channels (up to 12, by name, number or what's on) for those Live TV is
/// open to.
public struct SearchResults: Codable, Hashable, Sendable {
    public var items: [ItemSummary]
    public var people: [PersonSummary]
    public var channels: [LiveChannel]

    public init(items: [ItemSummary] = [], people: [PersonSummary] = [], channels: [LiveChannel] = []) {
        self.items = items
        self.people = people
        self.channels = channels
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        items = try c.value(.items, or: [])
        people = try c.value(.people, or: [])
        // A channel this app can't read takes nothing else down with it.
        channels = (try? c.value(.channels, or: [LiveChannel]())) ?? []
    }
}

// MARK: History

/// Who played an entry of the history.
public struct HistoryUser: Codable, Hashable, Sendable {
    public var id: Int
    /// The display name, or the username.
    public var name: String
    public var avatarUrl: String?
    public var avatarColor: String?

    public init(id: Int, name: String) {
        self.id = id
        self.name = name
    }
}

/// A play: recorded once the item counted as watched (90 %), with the
/// device it played on. Marking something watched by hand is not a play.
public struct HistoryEntry: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    /// Unix seconds.
    public var viewedAt: Int
    /// "Chrome on Windows", a Plex client's name; `nil` when the server doesn't know.
    public var device: String?
    public var user: HistoryUser
    public var item: ItemSummary

    public init(id: Int, viewedAt: Int, device: String? = nil, user: HistoryUser, item: ItemSummary) {
        self.id = id
        self.viewedAt = viewedAt
        self.device = device
        self.user = user
        self.item = item
    }
}

/// A page of `GET /api/history`, newest first; `total` counts every play.
public struct HistoryPage: Codable, Hashable, Sendable {
    public var total: Int
    public var entries: [HistoryEntry]

    public init(total: Int = 0, entries: [HistoryEntry] = []) {
        self.total = total
        self.entries = entries
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        total = try c.value(.total, or: 0)
        entries = try c.value(.entries, or: [])
    }
}

// MARK: Track defaults

/// When subtitles come on by themselves, for titles without a choice
/// (apps/server/src/streams.rs, `SubtitleMode`).
public enum SubtitleMode: String, Codable, Hashable, Sendable, CaseIterable {
    /// Never.
    case off
    /// Forced subtitles (signs, foreign dialogue): the server's default.
    case forced
    /// Full subtitles in the subtitle language when the audio is in another.
    case foreign
    /// Always, in the subtitle language.
    case always

    /// A mode this app doesn't know reads as the server's default.
    public init(wire: String?) {
        self = wire.flatMap(SubtitleMode.init(rawValue:)) ?? .forced
    }

    public init(from decoder: any Decoder) throws {
        self.init(wire: try? decoder.singleValueContainer().decode(String.self))
    }

    /// Whether the mode shows subtitles in a language of their own, so that language is asked for.
    public var readsLanguage: Bool { self == .foreign || self == .always }
}

/// The tracks a profile starts titles with when it hasn't picked any for
/// them: `PUT /api/me/track-defaults`'s body, and what `GET /api/auth/me`
/// says. Languages are ISO 639 codes ("eng", "fr"); a `nil` is left out,
/// which the server reads as none.
public struct TrackDefaults: Codable, Hashable, Sendable {
    /// The main audio track in this language; `nil`: the file's default track.
    public var audioLanguage: String?
    /// `nil`: the audio's language.
    public var subtitleLanguage: String?
    public var subtitleMode: SubtitleMode

    public init(audioLanguage: String? = nil, subtitleLanguage: String? = nil, subtitleMode: SubtitleMode = .forced) {
        self.audioLanguage = audioLanguage
        self.subtitleLanguage = subtitleLanguage
        self.subtitleMode = subtitleMode
    }

    /// The defaults `user` has.
    public init(_ user: User) {
        audioLanguage = user.audioLanguage.flatMap { $0.isEmpty ? nil : $0 }
        subtitleLanguage = user.subtitleLanguage.flatMap { $0.isEmpty ? nil : $0 }
        subtitleMode = SubtitleMode(wire: user.subtitleMode)
    }
}
