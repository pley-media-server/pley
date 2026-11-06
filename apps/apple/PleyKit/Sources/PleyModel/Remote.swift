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
    /// Another of the file's audio tracks (`stream_index`).
    public static let audio = Control(rawValue: "audio")
    /// Other subtitles of the file (`stream_index`), or none.
    public static let subtitles = Control(rawValue: "subtitles")
    /// Another entry of the player's Quality menu (`quality`, its id).
    public static let quality = Control(rawValue: "quality")
}

/// One command: the body of `POST /api/sessions/{id}/command`, and what
/// the player hears of it in `player_command`. `seek` carries
/// `positionMs`, `skip` `offsetMs` (back when negative), `audio` and
/// `subtitles` `streamIndex` (`SessionTracks.off`: no subtitles),
/// `quality` the id of an entry of the Quality menu, the others none.
public struct PlayerCommand: Codable, Hashable, Sendable {
    public var command: Control
    public var positionMs: Int?
    public var offsetMs: Int?
    public var streamIndex: Int?
    public var quality: String?

    public init(_ command: Control, positionMs: Int? = nil, offsetMs: Int? = nil, streamIndex: Int? = nil,
                quality: String? = nil) {
        self.command = command
        self.positionMs = positionMs
        self.offsetMs = offsetMs
        self.streamIndex = streamIndex
        self.quality = quality
    }

    public static let play = PlayerCommand(.play)
    public static let pause = PlayerCommand(.pause)
    public static let next = PlayerCommand(.next)
    public static let previous = PlayerCommand(.previous)

    public static func seek(toMs positionMs: Int) -> PlayerCommand { PlayerCommand(.seek, positionMs: positionMs) }
    public static func skip(byMs offsetMs: Int) -> PlayerCommand { PlayerCommand(.skip, offsetMs: offsetMs) }
    /// The audio track of stream index `index`.
    public static func audio(_ index: Int) -> PlayerCommand { PlayerCommand(.audio, streamIndex: index) }
    /// The subtitles of stream index `index`; `nil`: none.
    public static func subtitles(_ index: Int?) -> PlayerCommand {
        PlayerCommand(.subtitles, streamIndex: index ?? SessionTracks.off)
    }
    /// The Quality menu's entry of id `id` ("auto", "8m").
    public static func quality(_ id: String) -> PlayerCommand { PlayerCommand(.quality, quality: id) }
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

/// What kind of device plays a session, or takes casts: as its player said,
/// else as the server tells from the app.
public struct DeviceKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let tv = DeviceKind(rawValue: "tv")
    public static let phone = DeviceKind(rawValue: "phone")
    public static let tablet = DeviceKind(rawValue: "tablet")
    public static let computer = DeviceKind(rawValue: "computer")
    public static let browser = DeviceKind(rawValue: "browser")
    /// An app that doesn't say, and apps of other publishers.
    public static let other = DeviceKind(rawValue: "other")
}

/// The channel a Live TV session watches; `now`: the programme on, from
/// `nowStart` to `nowStop` (unix seconds); `next`: the one after it.
public struct PlayingChannel: Codable, Hashable, Sendable {
    public var id: Int
    public var name: String
    public var number: String?
    /// Its logo's address on the server.
    public var logo: String?
    public var now: String?
    public var nowStart: Int64?
    public var nowStop: Int64?
    public var next: String?

    public init(id: Int, name: String, number: String? = nil, logo: String? = nil, now: String? = nil,
                nowStart: Int64? = nil, nowStop: Int64? = nil, next: String? = nil) {
        self.id = id
        self.name = name
        self.number = number
        self.logo = logo
        self.now = now
        self.nowStart = nowStart
        self.nowStop = nowStop
        self.next = next
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        number = try c.decodeIfPresent(String.self, forKey: .number)
        logo = try c.decodeIfPresent(String.self, forKey: .logo)
        now = try c.decodeIfPresent(String.self, forKey: .now)
        nowStart = try c.decodeIfPresent(Int64.self, forKey: .nowStart)
        nowStop = try c.decodeIfPresent(Int64.self, forKey: .nowStop)
        next = try c.decodeIfPresent(String.self, forKey: .next)
    }

    /// How far along the programme on is at `time` (unix seconds), from 0
    /// to 1; `nil` when its times aren't known.
    public func progress(at time: Int64) -> Double? {
        guard let start = nowStart, let stop = nowStop, stop > start else { return nil }
        return min(max(Double(time - start) / Double(stop - start), 0), 1)
    }
}

/// One entry of a player's Quality menu, as its session lists it: the id
/// `quality` sends, and the menu's label ("8 Mbps 1080p").
public struct QualityOption: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var label: String

    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }
}

/// One of a title's tracks a remote picks from, as a session lists them.
public struct TrackOption: Codable, Hashable, Sendable, Identifiable {
    /// The stream index commands and reports name.
    public var index: Int
    /// "eng".
    public var language: String?
    public var title: String?
    /// "eac3", "subrip".
    public var codec: String?
    /// Audio only.
    public var channels: Int?
    public var `default`: Bool
    public var forced: Bool

    public var id: Int { index }

    public init(index: Int, language: String? = nil, title: String? = nil, codec: String? = nil,
                channels: Int? = nil, default: Bool = false, forced: Bool = false) {
        self.index = index
        self.language = language
        self.title = title
        self.codec = codec
        self.channels = channels
        self.default = `default`
        self.forced = forced
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        index = try c.decode(Int.self, forKey: .index)
        language = try c.decodeIfPresent(String.self, forKey: .language)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        codec = try c.decodeIfPresent(String.self, forKey: .codec)
        channels = try c.decodeIfPresent(Int.self, forKey: .channels)
        `default` = try c.value(.default, or: false)
        forced = try c.value(.forced, or: false)
    }

    /// The track as one of the file's streams of `kind`, so it is labelled
    /// as the player's own menus label tracks (`Words.stream`). Subtitles
    /// numbered from 1000 are files beside the video or added ones.
    public func stream(_ kind: StreamKind) -> MediaStream {
        var stream = MediaStream(index: index, kind: kind, codec: codec ?? "", language: language, default: `default`)
        stream.title = title
        stream.forced = forced
        stream.channels = channels
        stream.external = kind == .subtitle && index >= 1000
        return stream
    }
}

/// A title's tracks, as its session lists them: what `audio` and
/// `subtitles` pick from. Live TV sessions have none.
public struct SessionTracks: Codable, Hashable, Sendable {
    /// The subtitle index that says none show.
    public static let off = -1

    public var audio: [TrackOption]
    public var subtitles: [TrackOption]

    public init(audio: [TrackOption] = [], subtitles: [TrackOption] = []) {
        self.audio = audio
        self.subtitles = subtitles
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        audio = try c.value(.audio, or: [])
        subtitles = try c.value(.subtitles, or: [])
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
    /// Which device plays it ("app-12", "web-9f2c…"): the same for
    /// everything one sign-in plays, and for the device's receiver.
    public var deviceId: String
    public var deviceKind: DeviceKind
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
    /// The audio track it plays, by stream index; `nil` when not known, and on Live TV.
    public var audioIndex: Int?
    /// The subtitles it shows (`SessionTracks.off`: none); `nil` when not known, and on Live TV.
    public var subtitleIndex: Int?
    /// Its file's tracks, which `audio` and `subtitles` pick from.
    public var tracks: SessionTracks
    /// Its player's Quality menu's pick, by id ("auto", "8m"); `nil` until its player says.
    public var quality: String?
    /// What that menu offers for the file, which `quality` picks from; none for music and on Live TV.
    public var qualities: [QualityOption]
    /// The intro and credits of the file it plays, for the remote's Skip Intro and Skip Credits; none on
    /// Live TV, and from an older server.
    public var markers: [Marker]

    public var id: String { sessionId }

    public init(sessionId: String, kind: SessionKind = .item, device: String? = nil, deviceId: String = "",
                deviceKind: DeviceKind = .other, item: ItemSummary? = nil, channel: PlayingChannel? = nil,
                state: PlayingState = .playing, positionMs: Int = 0, durationMs: Int? = nil, updatedAt: Int = 0,
                ageMs: Int = 0, controls: [Control] = [], audioIndex: Int? = nil, subtitleIndex: Int? = nil,
                tracks: SessionTracks = SessionTracks(), quality: String? = nil, qualities: [QualityOption] = [],
                markers: [Marker] = []) {
        self.sessionId = sessionId
        self.kind = kind
        self.device = device
        self.deviceId = deviceId
        self.deviceKind = deviceKind
        self.item = item
        self.channel = channel
        self.state = state
        self.positionMs = positionMs
        self.durationMs = durationMs
        self.updatedAt = updatedAt
        self.ageMs = ageMs
        self.controls = controls
        self.audioIndex = audioIndex
        self.subtitleIndex = subtitleIndex
        self.tracks = tracks
        self.quality = quality
        self.qualities = qualities
        self.markers = markers
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        kind = try c.value(.kind, or: .item)
        device = try c.decodeIfPresent(String.self, forKey: .device)
        deviceId = try c.value(.deviceId, or: "")
        deviceKind = try c.value(.deviceKind, or: .other)
        // A title or channel that can't be read leaves the session listed all the same.
        item = try? c.decodeIfPresent(ItemSummary.self, forKey: .item)
        channel = try? c.decodeIfPresent(PlayingChannel.self, forKey: .channel)
        state = try c.value(.state, or: .playing)
        positionMs = try c.value(.positionMs, or: 0)
        durationMs = try c.decodeIfPresent(Int.self, forKey: .durationMs)
        updatedAt = try c.value(.updatedAt, or: 0)
        ageMs = try c.value(.ageMs, or: 0)
        controls = try c.value(.controls, or: [])
        audioIndex = try c.decodeIfPresent(Int.self, forKey: .audioIndex)
        subtitleIndex = try c.decodeIfPresent(Int.self, forKey: .subtitleIndex)
        // Tracks that can't be read leave it listed, with nothing to pick.
        tracks = (try? c.decodeIfPresent(SessionTracks.self, forKey: .tracks)) ?? SessionTracks()
        quality = try c.decodeIfPresent(String.self, forKey: .quality)
        // Qualities that can't be read leave it listed, with nothing to pick.
        qualities = (try? c.decodeIfPresent([QualityOption].self, forKey: .qualities)) ?? []
        // Markers that can't be read leave it listed, with nothing to skip.
        markers = (try? c.decodeIfPresent([Marker].self, forKey: .markers)) ?? []
    }

    /// Whether its player obeys `control`.
    public func obeys(_ control: Control) -> Bool { controls.contains(control) }
}

/// What a device plays when a remote casts to it: `item` for titles,
/// music and playlists, `live` for Live TV's channels.
public struct CastKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let item = CastKind(rawValue: "item")
    public static let live = CastKind(rawValue: "live")
}

/// One of the profile's devices that takes casts (`GET /api/remote/devices`):
/// an app whose event stream names it as a receiver, while that is open.
public struct RemoteDevice: Codable, Hashable, Sendable, Identifiable {
    /// The app's own id for the device: where casts go.
    public var receiverId: String
    /// As sessions say it: what the device plays has the same.
    public var deviceId: String
    /// What remotes call it, as a session's `device`; `nil` when the server can't say.
    public var name: String?
    public var kind: DeviceKind
    /// What it plays when cast to.
    public var casts: [CastKind]

    public var id: String { receiverId }

    public init(receiverId: String, deviceId: String = "", name: String? = nil, kind: DeviceKind = .other,
                casts: [CastKind] = []) {
        self.receiverId = receiverId
        self.deviceId = deviceId
        self.name = name
        self.kind = kind
        self.casts = casts
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        receiverId = try c.decode(String.self, forKey: .receiverId)
        deviceId = try c.value(.deviceId, or: "")
        name = try c.decodeIfPresent(String.self, forKey: .name)
        kind = try c.value(.kind, or: .other)
        casts = try c.value(.casts, or: [])
    }

    /// Whether it plays `kind` when cast to.
    public func takes(_ kind: CastKind) -> Bool { casts.contains(kind) }
}

/// What a remote casts to one of the profile's devices (the body of
/// `POST /api/remote/devices/{id}/cast`, and what the device hears in
/// `cast`): a title (a movie, an episode, a show, a season, a track, an
/// album or an artist), a playlist or a Live TV channel; where a title
/// starts, and whether an album, an artist or a playlist plays shuffled.
/// A movie or an episode may carry the tracks picked on its page, which
/// the device plays it with.
public struct CastTarget: Encodable, Hashable, Sendable {
    public enum What: Hashable, Sendable {
        case item(Int)
        case playlist(Int)
        case channel(Int)
    }

    public var what: What
    /// Where a title starts, in ms; `nil`: where its Play starts.
    public var startMs: Int?
    public var shuffle: Bool
    /// A movie's or an episode's audio track, by stream index; `nil`: the device's own choice.
    public var audioIndex: Int?
    /// Its subtitles, by stream index (`SessionTracks.off`: none); `nil`: the device's own choice.
    public var subtitleIndex: Int?

    public init(_ what: What, startMs: Int? = nil, shuffle: Bool = false, audioIndex: Int? = nil,
                subtitleIndex: Int? = nil) {
        self.what = what
        self.startMs = startMs
        self.shuffle = shuffle
        self.audioIndex = audioIndex
        self.subtitleIndex = subtitleIndex
    }

    public static func item(_ id: Int, startMs: Int? = nil, shuffle: Bool = false, audioIndex: Int? = nil,
                            subtitleIndex: Int? = nil) -> CastTarget {
        CastTarget(.item(id), startMs: startMs, shuffle: shuffle, audioIndex: audioIndex, subtitleIndex: subtitleIndex)
    }

    public static func playlist(_ id: Int, shuffle: Bool = false) -> CastTarget {
        CastTarget(.playlist(id), shuffle: shuffle)
    }

    public static func channel(_ id: Int) -> CastTarget { CastTarget(.channel(id)) }

    /// What a device must take to play it: `live` for a channel, `item` for the rest.
    public var kind: CastKind {
        if case .channel = what { return .live }
        return .item
    }

    /// The same, shuffled ("Shuffle on…").
    public var shuffled: CastTarget {
        var target = self
        target.shuffle = true
        return target
    }

    private enum CodingKeys: String, CodingKey {
        case itemId, playlistId, channelId, startMs, shuffle, audioIndex, subtitleIndex
    }

    /// Exactly one of `item_id`, `playlist_id` and `channel_id`; `start_ms`,
    /// `shuffle` and the tracks only when they say something.
    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch what {
        case .item(let id): try c.encode(id, forKey: .itemId)
        case .playlist(let id): try c.encode(id, forKey: .playlistId)
        case .channel(let id): try c.encode(id, forKey: .channelId)
        }
        try c.encodeIfPresent(startMs, forKey: .startMs)
        if shuffle { try c.encode(true, forKey: .shuffle) }
        try c.encodeIfPresent(audioIndex, forKey: .audioIndex)
        try c.encodeIfPresent(subtitleIndex, forKey: .subtitleIndex)
    }
}
