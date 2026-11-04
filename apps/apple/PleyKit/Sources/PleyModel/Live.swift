import Foundation

// MARK: Live TV (docs/API.md, "Live TV")

/// `GET /api/live`: whether to show Live TV. `allowed`: it is open to this
/// profile; `available`: and it has channels. `sources` is for
/// administrators (0 for the others); `recent` the channels watched last,
/// newest first.
public struct LiveSummary: Codable, Hashable, Sendable {
    public var available: Bool
    public var allowed: Bool
    public var channels: Int
    public var sources: Int
    public var recent: [Int]

    public init(available: Bool = false, allowed: Bool = false, channels: Int = 0, sources: Int = 0,
                recent: [Int] = []) {
        self.available = available
        self.allowed = allowed
        self.channels = channels
        self.sources = sources
        self.recent = recent
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = try c.value(.available, or: false)
        allowed = try c.value(.allowed, or: false)
        channels = try c.value(.channels, or: 0)
        sources = try c.value(.sources, or: 0)
        recent = try c.value(.recent, or: [])
    }
}

/// A programme of the guide (`LiveProgram`). Times are unix seconds;
/// `image` is an address on the server, when the guide has a picture.
public struct LiveProgram: Codable, Hashable, Sendable {
    public var channelId: Int
    public var start: Int64
    public var stop: Int64
    public var title: String
    public var subtitle: String?
    public var description: String?
    public var categories: [String]
    public var image: String?
    public var season: Int?
    public var episode: Int?
    public var isNew: Bool
    /// Shown as it happens (a match, the news).
    public var isLive: Bool
    public var rating: String?

    public init(channelId: Int, start: Int64, stop: Int64, title: String = "", subtitle: String? = nil,
                description: String? = nil, categories: [String] = [], image: String? = nil, season: Int? = nil,
                episode: Int? = nil, isNew: Bool = false, isLive: Bool = false, rating: String? = nil) {
        self.channelId = channelId
        self.start = start
        self.stop = stop
        self.title = title
        self.subtitle = subtitle
        self.description = description
        self.categories = categories
        self.image = image
        self.season = season
        self.episode = episode
        self.isNew = isNew
        self.isLive = isLive
        self.rating = rating
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        channelId = try c.value(.channelId, or: 0)
        start = try c.decode(Int64.self, forKey: .start)
        stop = try c.decode(Int64.self, forKey: .stop)
        title = try c.value(.title, or: "")
        subtitle = try c.decodeIfPresent(String.self, forKey: .subtitle)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        categories = try c.value(.categories, or: [])
        image = try c.decodeIfPresent(String.self, forKey: .image)
        season = try c.decodeIfPresent(Int.self, forKey: .season)
        episode = try c.decodeIfPresent(Int.self, forKey: .episode)
        isNew = try c.value(.isNew, or: false)
        isLive = try c.value(.isLive, or: false)
        rating = try c.decodeIfPresent(String.self, forKey: .rating)
    }

    /// On at `now` (unix seconds).
    public func airing(at now: Int64) -> Bool { start <= now && stop > now }

    /// How far it is at `now`, from 0 to 1.
    public func progress(at now: Int64) -> Double {
        min(max(Double(now - start) / Double(max(stop - start, 1)), 0), 1)
    }

    /// "S2 E14", or "E14", when the guide numbers the episode.
    public var episodeLabel: String? {
        switch (season, episode) {
        case (let s?, let e?): "S\(s) E\(e)"
        case (nil, let e?): "E\(e)"
        default: nil
        }
    }
}

/// A channel of the lineup, with what's on `now` and `next` (`LiveChannel`).
/// `logo` is its logo's address on the server.
public struct LiveChannel: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var sourceId: Int
    public var name: String
    public var number: String?
    public var group: String
    public var logo: String?
    public var favorite: Bool
    /// The guide has programmes for it.
    public var hasGuide: Bool
    public var now: LiveProgram?
    public var next: LiveProgram?

    public init(id: Int, sourceId: Int = 0, name: String, number: String? = nil, group: String = "",
                logo: String? = nil, favorite: Bool = false, hasGuide: Bool = false, now: LiveProgram? = nil,
                next: LiveProgram? = nil) {
        self.id = id
        self.sourceId = sourceId
        self.name = name
        self.number = number
        self.group = group
        self.logo = logo
        self.favorite = favorite
        self.hasGuide = hasGuide
        self.now = now
        self.next = next
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        sourceId = try c.value(.sourceId, or: 0)
        name = try c.value(.name, or: "")
        number = try c.decodeIfPresent(String.self, forKey: .number)
        group = try c.value(.group, or: "")
        logo = try c.decodeIfPresent(String.self, forKey: .logo)
        favorite = try c.value(.favorite, or: false)
        hasGuide = try c.value(.hasGuide, or: false)
        now = try? c.decodeIfPresent(LiveProgram.self, forKey: .now)
        next = try? c.decodeIfPresent(LiveProgram.self, forKey: .next)
    }

    /// What is on at `time` (unix seconds); `nil` once `now` ended, before the list is asked again.
    public func on(at time: Int64) -> LiveProgram? {
        if let now, now.airing(at: time) { return now }
        if let next, next.airing(at: time) { return next }
        return nil
    }

    /// What comes after `on(at:)`: `next`, or what was listed as now when it hasn't begun.
    public func after(at time: Int64) -> LiveProgram? {
        let listed = [now, next].compactMap { $0 }
        guard let on = on(at: time) else { return listed.first { $0.start > time } }
        return listed.first { $0.start >= on.stop }
    }

    /// "101 BBC One", or the name alone.
    public var numberedName: String { number.map { "\($0) \(name)" } ?? name }
}

/// A group of channels as the provider names it (`LiveGroup`): `region` is
/// the "UK" of "UK| Sport", `label` the rest.
public struct LiveGroup: Codable, Hashable, Sendable, Identifiable {
    public var name: String
    public var region: String?
    public var label: String
    public var count: Int

    public var id: String { name }

    public init(name: String, region: String? = nil, label: String? = nil, count: Int = 0) {
        self.name = name
        self.region = region
        self.label = label ?? name
        self.count = count
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.value(.name, or: "")
        region = try c.decodeIfPresent(String.self, forKey: .region)
        label = try c.decodeIfPresent(String.self, forKey: .label) ?? name
        count = try c.value(.count, or: 0)
    }
}

/// `GET /api/live/groups`: the groups in the lineup's order, and how many
/// channels there are in all, marked and watched lately.
public struct LiveGroups: Codable, Hashable, Sendable {
    public var total: Int
    public var favorites: Int
    public var recent: Int
    public var groups: [LiveGroup]

    public init(total: Int = 0, favorites: Int = 0, recent: Int = 0, groups: [LiveGroup] = []) {
        self.total = total
        self.favorites = favorites
        self.recent = recent
        self.groups = groups
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        total = try c.value(.total, or: 0)
        favorites = try c.value(.favorites, or: 0)
        recent = try c.value(.recent, or: 0)
        groups = try c.value(.groups, or: [])
    }
}

/// `GET /api/live/browse`: `channels` from `offset` of a list of `total`.
public struct LiveChannelPage: Codable, Hashable, Sendable {
    public var total: Int
    public var offset: Int
    public var channels: [LiveChannel]

    public init(total: Int = 0, offset: Int = 0, channels: [LiveChannel] = []) {
        self.total = total
        self.offset = offset
        self.channels = channels
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        total = try c.value(.total, or: 0)
        offset = try c.value(.offset, or: 0)
        channels = try c.value(.channels, or: [])
    }
}

/// Which channels a list has (`category` of `GET /api/live/browse`):
/// every one, the favourites, those watched lately, or one group's.
public enum LiveCategory: Hashable, Sendable, Codable {
    case all, favorites, recent
    case group(String)

    /// How it is kept and passed on: `all`, `favorites`, `recent` or `group:UK| Sport`.
    public var key: String {
        switch self {
        case .all: "all"
        case .favorites: "favorites"
        case .recent: "recent"
        case .group(let name): "group:\(name)"
        }
    }

    /// `category` on the wire.
    public var wire: String {
        switch self {
        case .all: "all"
        case .favorites: "favorites"
        case .recent: "recent"
        case .group: "group"
        }
    }

    /// The category `key` names; All for anything else.
    public static func parse(_ key: String?) -> LiveCategory {
        switch key {
        case "favorites": return .favorites
        case "recent": return .recent
        case let key? where key.hasPrefix("group:"): return .group(String(key.dropFirst("group:".count)))
        default: return .all
        }
    }
}

/// `GET /api/live/guide`: the programmes between `from` and `to` (unix seconds), by channel.
public struct LiveGuide: Codable, Hashable, Sendable {
    public var from: Int64
    public var to: Int64
    public var programs: [Int: [LiveProgram]]

    public init(from: Int64, to: Int64, programs: [Int: [LiveProgram]] = [:]) {
        self.from = from
        self.to = to
        self.programs = programs
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        from = try c.value(.from, or: 0)
        to = try c.value(.to, or: 0)
        // The channels' ids are the object's keys.
        let byKey = try c.value(.programs, or: [String: [LiveProgram]]())
        var programs: [Int: [LiveProgram]] = [:]
        for (key, list) in byKey {
            if let id = Int(key) { programs[id] = list }
        }
        self.programs = programs
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(from, forKey: .from)
        try c.encode(to, forKey: .to)
        try c.encode(Dictionary(uniqueKeysWithValues: programs.map { (String($0.key), $0.value) }), forKey: .programs)
    }

    private enum CodingKeys: String, CodingKey {
        case from, to, programs
    }
}

/// The body of `POST /api/live/channels/{id}/watch`. `clientId` ends what
/// this device watched before, so changing channels never holds two of
/// the provider's connections; `keyedUrls` asks for an address the player
/// opens without a token; `takeOver`, every connection being in use, is the
/// channel whose connection to take. No caps: every app gets H.264 and AAC.
public struct LiveWatchRequest: Codable, Hashable, Sendable {
    public var clientId: String
    public var keyedUrls: Bool
    public var takeOver: Int?
    /// The remote's commands the live player obeys, as a play request's.
    public var controls: [Control]?
    /// What remotes call this device; `nil`: the server names it from the User-Agent.
    public var deviceName: String?

    public init(clientId: String, keyedUrls: Bool = true, takeOver: Int? = nil, controls: [Control]? = nil,
                deviceName: String? = nil) {
        self.clientId = clientId
        self.keyedUrls = keyedUrls
        self.takeOver = takeOver
        self.controls = controls
        self.deviceName = deviceName
    }
}

/// The answer to `POST /api/live/channels/{id}/watch`: `url` is the
/// channel's live HLS playlist, read as `sessionId` until
/// `DELETE /api/live/sessions/{session}` (or 45 s without a request).
/// Never logged: a keyed `url` is all a player needs.
public struct LiveWatch: Codable, Hashable, Sendable {
    public var sessionId: String
    public var url: String
    public var kind: String
    public var channel: LiveChannel?

    public init(sessionId: String, url: String, kind: String = "hls", channel: LiveChannel? = nil) {
        self.sessionId = sessionId
        self.url = url
        self.kind = kind
        self.channel = channel
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        url = try c.decode(String.self, forKey: .url)
        kind = try c.value(.kind, or: "hls")
        channel = try? c.decodeIfPresent(LiveChannel.self, forKey: .channel)
    }
}

/// `GET /api/live/sessions/{session}/plan`: what the channel a session
/// watches carries, and what the transcoder makes of it (`video`: `copy`
/// or the encoder; `audio`: `copy`, `aac` or `none`).
public struct LivePlan: Codable, Hashable, Sendable {
    public var videoCodec: String?
    public var width: Int?
    public var height: Int?
    public var frameRate: Double?
    public var interlaced: Bool
    public var audioCodec: String?
    public var audioChannels: Int?
    public var video: String
    public var audio: String

    public init(videoCodec: String? = nil, width: Int? = nil, height: Int? = nil, frameRate: Double? = nil,
                interlaced: Bool = false, audioCodec: String? = nil, audioChannels: Int? = nil, video: String = "",
                audio: String = "") {
        self.videoCodec = videoCodec
        self.width = width
        self.height = height
        self.frameRate = frameRate
        self.interlaced = interlaced
        self.audioCodec = audioCodec
        self.audioChannels = audioChannels
        self.video = video
        self.audio = audio
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        videoCodec = try c.decodeIfPresent(String.self, forKey: .videoCodec)
        width = try c.decodeIfPresent(Int.self, forKey: .width)
        height = try c.decodeIfPresent(Int.self, forKey: .height)
        frameRate = try c.decodeIfPresent(Double.self, forKey: .frameRate)
        interlaced = try c.value(.interlaced, or: false)
        audioCodec = try c.decodeIfPresent(String.self, forKey: .audioCodec)
        audioChannels = try c.decodeIfPresent(Int.self, forKey: .audioChannels)
        video = try c.value(.video, or: "")
        audio = try c.value(.audio, or: "")
    }
}

/// A connection of a `409` (every connection of the channel's source in
/// use): the channel it pulls, who watches it (`watchers`, named for
/// administrators only), how many people, whether only the asker does
/// (`yours`), and whether the asker may take it over.
public struct BusyConnection: Codable, Hashable, Sendable, Identifiable {
    public var channelId: Int
    public var channelName: String
    public var watchers: [String]
    public var people: Int
    public var yours: Bool
    public var canTakeOver: Bool

    public var id: Int { channelId }

    public init(channelId: Int, channelName: String, watchers: [String] = [], people: Int = 0, yours: Bool = false,
                canTakeOver: Bool = false) {
        self.channelId = channelId
        self.channelName = channelName
        self.watchers = watchers
        self.people = people
        self.yours = yours
        self.canTakeOver = canTakeOver
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        channelId = try c.decode(Int.self, forKey: .channelId)
        channelName = try c.value(.channelName, or: "")
        watchers = try c.value(.watchers, or: [])
        people = try c.value(.people, or: 0)
        yours = try c.value(.yours, or: false)
        canTakeOver = try c.value(.canTakeOver, or: false)
    }
}
