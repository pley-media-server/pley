import Foundation

/// A video's dynamic range (packages/transcoder-api, `DynamicRange`).
public struct DynamicRange: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let sdr = DynamicRange(rawValue: "sdr")
    public static let hdr10 = DynamicRange(rawValue: "hdr10")
    public static let hdr10Plus = DynamicRange(rawValue: "hdr10plus")
    public static let hlg = DynamicRange(rawValue: "hlg")
    public static let dolbyVision = DynamicRange(rawValue: "dolby_vision")
}

/// A stream's kind (packages/transcoder-api, `StreamKind`).
public struct StreamKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let video = StreamKind(rawValue: "video")
    public static let audio = StreamKind(rawValue: "audio")
    public static let subtitle = StreamKind(rawValue: "subtitle")
}

/// `GET /api/items/{id}`: an item with everything the server knows about
/// it (docs/API.md, `ItemDetail`): an `ItemSummary`'s fields and more
/// beside them in the same object. Cast and crew are left unread here.
public struct ItemDetail: Codable, Hashable, Sendable, Identifiable {
    public var summary: ItemSummary
    public var originalTitle: String?
    public var tagline: String?
    public var genres: [String]
    public var studio: String?
    /// "PG-13", "15", "FSK 12"…
    public var contentRating: String?
    /// Empty for shows, seasons, artists and albums. Versions of a movie are several files.
    public var files: [MediaFile]
    /// Intro and credits of the files, for Skip Intro and Skip Credits.
    public var markers: [Marker]
    /// Movies and shows: trailers and other bonus material, trailers first.
    public var extras: [ItemSummary]
    /// In the profile's My List (movies and shows).
    public var inWatchlist: Bool

    public var id: Int { summary.id }
    public var kind: ItemKind { summary.kind }
    public var title: String { summary.title }
    public var year: Int? { summary.year }
    public var durationMs: Int? { summary.durationMs }
    public var userState: UserState { summary.userState }

    public init(summary: ItemSummary, files: [MediaFile] = [], markers: [Marker] = []) {
        self.summary = summary
        genres = []
        self.files = files
        self.markers = markers
        extras = []
        inWatchlist = false
    }

    private enum CodingKeys: String, CodingKey {
        case originalTitle, tagline, genres, studio, contentRating, files, markers, extras, inWatchlist
    }

    public init(from decoder: any Decoder) throws {
        summary = try ItemSummary(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        originalTitle = try c.decodeIfPresent(String.self, forKey: .originalTitle)
        tagline = try c.decodeIfPresent(String.self, forKey: .tagline)
        genres = try c.value(.genres, or: [])
        studio = try c.decodeIfPresent(String.self, forKey: .studio)
        contentRating = try c.decodeIfPresent(String.self, forKey: .contentRating)
        files = try c.value(.files, or: [])
        markers = try c.value(.markers, or: [])
        extras = try c.value(.extras, or: [])
        inWatchlist = try c.value(.inWatchlist, or: false)
    }

    public func encode(to encoder: any Encoder) throws {
        try summary.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(originalTitle, forKey: .originalTitle)
        try c.encodeIfPresent(tagline, forKey: .tagline)
        try c.encode(genres, forKey: .genres)
        try c.encodeIfPresent(studio, forKey: .studio)
        try c.encodeIfPresent(contentRating, forKey: .contentRating)
        try c.encode(files, forKey: .files)
        try c.encode(markers, forKey: .markers)
        try c.encode(extras, forKey: .extras)
        try c.encode(inWatchlist, forKey: .inWatchlist)
    }
}

/// `{ item }`: `GET /api/items/{id}/on-deck` (the episode a show's Play
/// starts) and `GET /api/items/{id}/next` (the episode after this one);
/// `nil` when there's none.
public struct MaybeItem: Codable, Hashable, Sendable {
    public var item: ItemSummary?

    public init(item: ItemSummary? = nil) { self.item = item }
}

/// A media file of an item (docs/API.md, `MediaFile`). Sizes are bytes,
/// bitrates bits per second, durations ms.
public struct MediaFile: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    /// An app's token gets the file name only, never where the server keeps it.
    public var path: String
    public var size: Int
    /// "mkv", "mp4", "webm", "avi", "mp3", "flac"…
    public var container: String?
    public var durationMs: Int?
    public var bitrate: Int?
    public var width: Int?
    public var height: Int?
    /// ffprobe's name: "h264", "hevc", "av1"…
    public var videoCodec: String?
    /// The default audio stream's codec.
    public var audioCodec: String?
    public var streams: [MediaStream]
    /// From `{edition-Final Cut}` in the file's or its folder's name.
    public var edition: String?
    /// The audio track to start with for this profile: its choice, else its defaults.
    public var selectedAudio: Int?
    /// The profile picked `selectedAudio` for the movie or show; `false`:
    /// each device plays its best track of that language.
    public var audioPicked: Bool
    /// The subtitle track to start with; `nil`: none.
    public var selectedSubtitle: Int?
    /// From the container, by start time.
    public var chapters: [Chapter]

    public init(id: Int, path: String, container: String? = nil, durationMs: Int? = nil,
                streams: [MediaStream] = []) {
        self.id = id
        self.path = path
        size = 0
        self.container = container
        self.durationMs = durationMs
        self.streams = streams
        audioPicked = false
        chapters = []
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        path = try c.value(.path, or: "")
        size = try c.value(.size, or: 0)
        container = try c.decodeIfPresent(String.self, forKey: .container)
        durationMs = try c.decodeIfPresent(Int.self, forKey: .durationMs)
        bitrate = try c.decodeIfPresent(Int.self, forKey: .bitrate)
        width = try c.decodeIfPresent(Int.self, forKey: .width)
        height = try c.decodeIfPresent(Int.self, forKey: .height)
        videoCodec = try c.decodeIfPresent(String.self, forKey: .videoCodec)
        audioCodec = try c.decodeIfPresent(String.self, forKey: .audioCodec)
        streams = try c.value(.streams, or: [])
        edition = try c.decodeIfPresent(String.self, forKey: .edition)
        selectedAudio = try c.decodeIfPresent(Int.self, forKey: .selectedAudio)
        audioPicked = try c.value(.audioPicked, or: false)
        selectedSubtitle = try c.decodeIfPresent(Int.self, forKey: .selectedSubtitle)
        chapters = try c.value(.chapters, or: [])
    }

    /// The file's streams of `kind`, in the file's order.
    public func streams(_ kind: StreamKind) -> [MediaStream] { streams.filter { $0.kind == kind } }

    /// The video stream: the default one, else the first.
    public var video: MediaStream? {
        let videos = streams(.video)
        return videos.first(where: \.default) ?? videos.first
    }
}

/// One stream of a file, as ffprobe found it (docs/API.md, `MediaStream`).
/// Fields that don't apply to its kind, or weren't found, are `nil`.
public struct MediaStream: Codable, Hashable, Sendable {
    /// ffprobe's stream index; subtitle files beside the media count from 1000.
    public var index: Int
    public var kind: StreamKind
    public var codec: String
    /// ISO 639-2 as found in the file ("eng").
    public var language: String?
    public var title: String?
    public var `default`: Bool
    public var forced: Bool
    public var bitrate: Int?
    /// "Main 10", "High", "LC"…
    public var profile: String?
    // Video.
    public var width: Int?
    public var height: Int?
    public var frameRate: Double?
    public var bitDepth: Int?
    /// "yuv420p10le"…
    public var pixFmt: String?
    /// The level as ffprobe numbers it: 153 for HEVC 5.1, 41 for H.264 4.1.
    public var level: Int?
    public var interlaced: Bool?
    public var hdr: Bool?
    public var dynamicRange: DynamicRange?
    /// Dolby Vision profile: 5, 7, 8…
    public var dvProfile: Int?
    public var dvLevel: Int?
    /// Dolby Vision base-layer compatibility: 0 none, 1 HDR10, 2 SDR, 4 HLG, 6 Blu-ray HDR10.
    public var dvBlCompat: Int?
    /// Profile 7's enhancement layer: "fel" or "mel".
    public var dvEl: String?
    public var hdr10plus: Bool?
    /// "smpte2084", "arib-std-b67", "bt709"…
    public var colorTransfer: String?
    public var colorPrimaries: String?
    public var colorSpace: String?
    // Audio.
    public var channels: Int?
    /// "5.1(side)", "stereo"…
    public var channelLayout: String?
    public var sampleRate: Int?
    // Subtitles.
    /// srt, ass, WebVTT, mov_text (true); PGS, VobSub, DVB (false).
    public var textBased: Bool?
    /// A subtitle file beside the media, or one added on the server.
    public var external: Bool
    /// Added on the server (found online, or a file uploaded).
    public var added: Bool

    public init(index: Int, kind: StreamKind, codec: String, language: String? = nil, default: Bool = false) {
        self.index = index
        self.kind = kind
        self.codec = codec
        self.language = language
        self.default = `default`
        forced = false
        external = false
        added = false
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        index = try c.decode(Int.self, forKey: .index)
        kind = try c.decode(StreamKind.self, forKey: .kind)
        codec = try c.value(.codec, or: "")
        language = try c.decodeIfPresent(String.self, forKey: .language)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        `default` = try c.value(.default, or: false)
        forced = try c.value(.forced, or: false)
        bitrate = try c.decodeIfPresent(Int.self, forKey: .bitrate)
        profile = try c.decodeIfPresent(String.self, forKey: .profile)
        width = try c.decodeIfPresent(Int.self, forKey: .width)
        height = try c.decodeIfPresent(Int.self, forKey: .height)
        frameRate = try c.decodeIfPresent(Double.self, forKey: .frameRate)
        bitDepth = try c.decodeIfPresent(Int.self, forKey: .bitDepth)
        pixFmt = try c.decodeIfPresent(String.self, forKey: .pixFmt)
        level = try c.decodeIfPresent(Int.self, forKey: .level)
        interlaced = try c.decodeIfPresent(Bool.self, forKey: .interlaced)
        hdr = try c.decodeIfPresent(Bool.self, forKey: .hdr)
        dynamicRange = try c.decodeIfPresent(DynamicRange.self, forKey: .dynamicRange)
        dvProfile = try c.decodeIfPresent(Int.self, forKey: .dvProfile)
        dvLevel = try c.decodeIfPresent(Int.self, forKey: .dvLevel)
        dvBlCompat = try c.decodeIfPresent(Int.self, forKey: .dvBlCompat)
        dvEl = try c.decodeIfPresent(String.self, forKey: .dvEl)
        hdr10plus = try c.decodeIfPresent(Bool.self, forKey: .hdr10plus)
        colorTransfer = try c.decodeIfPresent(String.self, forKey: .colorTransfer)
        colorPrimaries = try c.decodeIfPresent(String.self, forKey: .colorPrimaries)
        colorSpace = try c.decodeIfPresent(String.self, forKey: .colorSpace)
        channels = try c.decodeIfPresent(Int.self, forKey: .channels)
        channelLayout = try c.decodeIfPresent(String.self, forKey: .channelLayout)
        sampleRate = try c.decodeIfPresent(Int.self, forKey: .sampleRate)
        textBased = try c.decodeIfPresent(Bool.self, forKey: .textBased)
        external = try c.value(.external, or: false)
        added = try c.value(.added, or: false)
    }
}

/// A chapter of a file, from its container.
public struct Chapter: Codable, Hashable, Sendable {
    public var startMs: Int
    public var endMs: Int
    public var title: String?
    /// "/api/files/{id}/chapters/{n}.jpg?v=…", once the file has seek previews.
    public var thumbUrl: String?

    public init(startMs: Int, endMs: Int, title: String? = nil, thumbUrl: String? = nil) {
        self.startMs = startMs
        self.endMs = endMs
        self.title = title
        self.thumbUrl = thumbUrl
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        startMs = try c.value(.startMs, or: 0)
        endMs = try c.value(.endMs, or: 0)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        thumbUrl = try c.decodeIfPresent(String.self, forKey: .thumbUrl)
    }
}

/// What a marker marks.
public struct MarkerKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let intro = MarkerKind(rawValue: "intro")
    public static let credits = MarkerKind(rawValue: "credits")
}

/// An intro or the credits of a file (docs/API.md, `Marker`). Credits that
/// run to the end of the file end at its duration.
public struct Marker: Codable, Hashable, Sendable {
    public var fileId: Int
    public var kind: MarkerKind
    public var startMs: Int
    public var endMs: Int
    /// "chapter", "fingerprint" or "frames".
    public var source: String?

    public init(fileId: Int, kind: MarkerKind, startMs: Int, endMs: Int, source: String? = nil) {
        self.fileId = fileId
        self.kind = kind
        self.startMs = startMs
        self.endMs = endMs
        self.source = source
    }
}
