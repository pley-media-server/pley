import Foundation

/// How the app asks for an item to be delivered (docs/API.md, "Play request").
public struct PlaybackMode: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// The cheapest the caps allow: direct play, then remux, then transcode.
    public static let auto = PlaybackMode(rawValue: "auto")
    /// The original file, even when the caps don't claim it plays.
    public static let direct = PlaybackMode(rawValue: "direct")
    /// HLS, the video copied when it can be.
    public static let remux = PlaybackMode(rawValue: "remux")
    /// HLS, the video always converted.
    public static let transcode = PlaybackMode(rawValue: "transcode")
}

/// How the server delivers it.
public struct PlaybackMethod: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// The file as it is.
    public static let direct = PlaybackMethod(rawValue: "direct")
    /// HLS with the video copied.
    public static let remux = PlaybackMethod(rawValue: "remux")
    /// HLS with the video converted.
    public static let transcode = PlaybackMethod(rawValue: "transcode")
}

/// The body of `POST /api/playback`. Fields left `nil` are left out, and
/// the server picks: the item's first file, the audio this player plays
/// best in the profile's language, no subtitles burned in, the original
/// quality, the start.
public struct PlaybackRequest: Codable, Hashable, Sendable {
    public var itemId: Int
    public var fileId: Int?
    /// The audio's stream index, only when the person picked it.
    public var audioIndex: Int?
    /// Only for picture subtitles to burn in (forces a transcode).
    public var subtitleIndex: Int?
    /// kbps; `nil`: the original quality.
    public var maxBitrate: Int?
    /// Lines: 1080, 720, 480.
    public var maxHeight: Int?
    /// Where to start, in ms; a transcode starts producing there.
    public var startMs: Int?
    public var mode: PlaybackMode
    /// The player: starting a session ends this profile's earlier ones on the same client.
    public var clientId: String?
    /// The client's last session plays on while this one gets ready; the player ends it.
    public var keepPrevious: Bool?
    /// The player can't send the Authorization header: its addresses carry a key.
    public var keyedUrls: Bool?
    /// The remote's commands this player obeys (Remote control); `nil`: none.
    public var controls: [Control]?
    /// What remotes call this device; `nil`: the server names it from the User-Agent.
    public var deviceName: String?
    /// What this device plays.
    public var caps: ClientCaps

    public init(itemId: Int, fileId: Int? = nil, audioIndex: Int? = nil, subtitleIndex: Int? = nil,
                maxBitrate: Int? = nil, maxHeight: Int? = nil, startMs: Int? = nil, mode: PlaybackMode = .auto,
                clientId: String? = nil, keepPrevious: Bool? = nil, keyedUrls: Bool? = nil,
                controls: [Control]? = nil, deviceName: String? = nil, caps: ClientCaps = ClientCaps()) {
        self.itemId = itemId
        self.fileId = fileId
        self.audioIndex = audioIndex
        self.subtitleIndex = subtitleIndex
        self.maxBitrate = maxBitrate
        self.maxHeight = maxHeight
        self.startMs = startMs
        self.mode = mode
        self.clientId = clientId
        self.keepPrevious = keepPrevious
        self.keyedUrls = keyedUrls
        self.controls = controls
        self.deviceName = deviceName
        self.caps = caps
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        itemId = try c.decode(Int.self, forKey: .itemId)
        fileId = try c.decodeIfPresent(Int.self, forKey: .fileId)
        audioIndex = try c.decodeIfPresent(Int.self, forKey: .audioIndex)
        subtitleIndex = try c.decodeIfPresent(Int.self, forKey: .subtitleIndex)
        maxBitrate = try c.decodeIfPresent(Int.self, forKey: .maxBitrate)
        maxHeight = try c.decodeIfPresent(Int.self, forKey: .maxHeight)
        startMs = try c.decodeIfPresent(Int.self, forKey: .startMs)
        mode = try c.value(.mode, or: .auto)
        clientId = try c.decodeIfPresent(String.self, forKey: .clientId)
        keepPrevious = try c.decodeIfPresent(Bool.self, forKey: .keepPrevious)
        keyedUrls = try c.decodeIfPresent(Bool.self, forKey: .keyedUrls)
        controls = try c.decodeIfPresent([Control].self, forKey: .controls)
        deviceName = try c.decodeIfPresent(String.self, forKey: .deviceName)
        caps = try c.value(.caps, or: ClientCaps())
    }
}

/// A subtitle track of the file being played.
public struct SubtitleTrack: Codable, Hashable, Sendable {
    public var index: Int
    public var codec: String
    public var language: String?
    public var title: String?
    public var `default`: Bool
    public var forced: Bool
    public var external: Bool
    public var textBased: Bool
    /// WebVTT: "/api/files/7/subtitles/3.vtt"; `nil` for subtitles shown as pictures.
    public var url: String?
    /// Added on the server (found online, or a file uploaded).
    public var added: Bool

    public init(index: Int, codec: String, textBased: Bool, url: String? = nil) {
        self.index = index
        self.codec = codec
        `default` = false
        forced = false
        external = false
        self.textBased = textBased
        self.url = url
        added = false
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        index = try c.decode(Int.self, forKey: .index)
        codec = try c.value(.codec, or: "")
        language = try c.decodeIfPresent(String.self, forKey: .language)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        `default` = try c.value(.default, or: false)
        forced = try c.value(.forced, or: false)
        external = try c.value(.external, or: false)
        textBased = try c.value(.textBased, or: false)
        url = try c.decodeIfPresent(String.self, forKey: .url)
        added = try c.value(.added, or: false)
    }
}

/// The answer of `POST /api/playback`: how the item plays and from where.
public struct PlaybackInfo: Codable, Hashable, Sendable {
    /// For progress reports, the session's status and its end.
    public var sessionId: String
    public var method: PlaybackMethod
    /// Direct: the file; otherwise the HLS playlist. With keyed URLs,
    /// `/api/media/{key}/…`, which carries the key: never logged.
    public var url: String
    public var hls: Bool
    public var fileId: Int
    public var durationMs: Int
    /// The audio track that plays: the one asked for, else the server's choice for this player.
    public var audioIndex: Int?
    /// The subtitle burned in, only when `burnSubtitles`.
    public var subtitleIndex: Int?
    public var burnSubtitles: Bool
    /// The audio is converted, also in a remux, where the video is copied.
    public var audioConverted: Bool
    /// Every subtitle track of the file.
    public var subtitles: [SubtitleTrack]
    /// Why it isn't played as it is, one short sentence each.
    public var reasons: [String]
    /// The same in one line.
    public var reason: String?
    /// When the original is converted: the closest quality players offer ("15m"…).
    public var autoQuality: String?
    /// What the screen gets of HDR video, by name: "Dolby Vision P8.1",
    /// "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)", "SDR (tone-mapped)"…
    public var videoRange: String?
    /// The stream comes in several sizes the player moves between by itself.
    public var adaptive: Bool
    /// The video's bitrate can change as it is sent (MPEG-TS transcodes only).
    public var liveBitrate: Bool
    /// How much later than the file's times the stream's timestamps are;
    /// `nil` when the file plays as it is.
    public var ptsOffsetMs: Int?
    /// The server converts no video: the Quality menu offers the original
    /// alone, and Automatic stays there.
    public var originalOnly: Bool

    public init(sessionId: String, method: PlaybackMethod, url: String, hls: Bool, fileId: Int, durationMs: Int = 0) {
        self.sessionId = sessionId
        self.method = method
        self.url = url
        self.hls = hls
        self.fileId = fileId
        self.durationMs = durationMs
        burnSubtitles = false
        audioConverted = false
        subtitles = []
        reasons = []
        adaptive = false
        liveBitrate = false
        originalOnly = false
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        method = try c.decode(PlaybackMethod.self, forKey: .method)
        url = try c.decode(String.self, forKey: .url)
        hls = try c.value(.hls, or: false)
        fileId = try c.decode(Int.self, forKey: .fileId)
        durationMs = try c.value(.durationMs, or: 0)
        audioIndex = try c.decodeIfPresent(Int.self, forKey: .audioIndex)
        subtitleIndex = try c.decodeIfPresent(Int.self, forKey: .subtitleIndex)
        burnSubtitles = try c.value(.burnSubtitles, or: false)
        audioConverted = try c.value(.audioConverted, or: false)
        subtitles = try c.value(.subtitles, or: [])
        reasons = try c.value(.reasons, or: [])
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        autoQuality = try c.decodeIfPresent(String.self, forKey: .autoQuality)
        videoRange = try c.decodeIfPresent(String.self, forKey: .videoRange)
        adaptive = try c.value(.adaptive, or: false)
        liveBitrate = try c.value(.liveBitrate, or: false)
        ptsOffsetMs = try c.decodeIfPresent(Int.self, forKey: .ptsOffsetMs)
        originalOnly = try c.value(.originalOnly, or: false)
    }
}

/// The live numbers of a session's transcode, for the player's details.
public struct TranscodeInfo: Codable, Hashable, Sendable {
    public var jobId: String
    /// "starting", "running", "idle", "finished" or "failed".
    public var state: String?
    /// "copy", "transcode" or "none".
    public var video: String?
    public var audio: String?
    /// "HEVC 3840×2160 Dolby Vision P8.1".
    public var videoIn: String?
    /// "H.264 1920×1080".
    public var videoOut: String?
    public var audioIn: String?
    public var audioOut: String?
    public var encoder: String?
    public var decoder: String?
    public var pipeline: String?
    public var tonemap: String?
    /// Remuxes: "kept" or "converted to 8.1".
    public var dolbyVision: String?
    public var subtitleBurn: Bool
    /// ffmpeg's speed: 2.5 is two and a half times real time.
    public var speed: Double?
    public var fps: Double?
    public var lastError: String?

    public init(jobId: String) {
        self.jobId = jobId
        subtitleBurn = false
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        jobId = try c.value(.jobId, or: "")
        state = try c.decodeIfPresent(String.self, forKey: .state)
        video = try c.decodeIfPresent(String.self, forKey: .video)
        audio = try c.decodeIfPresent(String.self, forKey: .audio)
        videoIn = try c.decodeIfPresent(String.self, forKey: .videoIn)
        videoOut = try c.decodeIfPresent(String.self, forKey: .videoOut)
        audioIn = try c.decodeIfPresent(String.self, forKey: .audioIn)
        audioOut = try c.decodeIfPresent(String.self, forKey: .audioOut)
        encoder = try c.decodeIfPresent(String.self, forKey: .encoder)
        self.decoder = try c.decodeIfPresent(String.self, forKey: .decoder)
        pipeline = try c.decodeIfPresent(String.self, forKey: .pipeline)
        tonemap = try c.decodeIfPresent(String.self, forKey: .tonemap)
        dolbyVision = try c.decodeIfPresent(String.self, forKey: .dolbyVision)
        subtitleBurn = try c.value(.subtitleBurn, or: false)
        speed = try c.decodeIfPresent(Double.self, forKey: .speed)
        fps = try c.decodeIfPresent(Double.self, forKey: .fps)
        lastError = try c.decodeIfPresent(String.self, forKey: .lastError)
    }
}

/// `GET /api/playback/{session}`: how the session plays now.
public struct SessionStatus: Codable, Hashable, Sendable {
    public var sessionId: String
    public var method: PlaybackMethod
    /// `nil` for direct play, or when the transcoder can't say.
    public var transcode: TranscodeInfo?

    public init(sessionId: String, method: PlaybackMethod, transcode: TranscodeInfo? = nil) {
        self.sessionId = sessionId
        self.method = method
        self.transcode = transcode
    }
}

/// The state a progress report gives.
public struct ProgressState: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let playing = ProgressState(rawValue: "playing")
    public static let paused = ProgressState(rawValue: "paused")
    /// The player closed.
    public static let stopped = ProgressState(rawValue: "stopped")
}

/// The body of `POST /api/items/{id}/progress`: every 10 s while playing,
/// on pause, every 30 s while paused, and `stopped` on closing. At 90 %
/// the item is watched and its resume point cleared.
public struct ProgressReport: Codable, Hashable, Sendable {
    public var positionMs: Int
    /// The player's own duration; `nil`: the item's.
    public var durationMs: Int?
    public var state: ProgressState
    /// Keeps the session, and its transcode, alive.
    public var sessionId: String?
    /// How long ago the report was made, for one sent late: the server
    /// keeps whatever happened to the item since.
    public var ageMs: Int?
    /// The tracks the player plays now, by stream index (`subtitleIndex`
    /// `SessionTracks.off`: none), kept on the session for the profile's
    /// remotes; `nil` leaves out what it doesn't say (music, a download).
    public var audioIndex: Int?
    public var subtitleIndex: Int?
    /// The Quality menu's pick, by id ("auto", "8m"), kept on the session
    /// for the profile's remotes; `nil` for music and a download.
    public var quality: String?

    public init(positionMs: Int, durationMs: Int? = nil, state: ProgressState = .playing, sessionId: String? = nil,
                ageMs: Int? = nil, audioIndex: Int? = nil, subtitleIndex: Int? = nil, quality: String? = nil) {
        self.positionMs = positionMs
        self.durationMs = durationMs
        self.state = state
        self.sessionId = sessionId
        self.ageMs = ageMs
        self.audioIndex = audioIndex
        self.subtitleIndex = subtitleIndex
        self.quality = quality
    }
}

/// The body of `PUT /api/items/{id}/streams`: the tracks picked,
/// remembered for the whole show or the movie.
public struct StreamsChoice: Encodable, Hashable, Sendable {
    /// The file the indexes are of; `nil`: the item's first.
    public var fileId: Int?
    /// The audio track picked; `nil` is left out, which keeps the audio chosen before.
    public var audioIndex: Int?
    /// `nil`: subtitles off.
    public var subtitleIndex: Int?
    /// Automatic picked: the audio picked before is forgotten (`audio_index: null`).
    public var automaticAudio: Bool

    public init(fileId: Int?, audioIndex: Int? = nil, subtitleIndex: Int? = nil, automaticAudio: Bool = false) {
        self.fileId = fileId
        self.audioIndex = audioIndex
        self.subtitleIndex = subtitleIndex
        self.automaticAudio = automaticAudio
    }

    private enum CodingKeys: String, CodingKey { case fileId, audioIndex, subtitleIndex }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(fileId, forKey: .fileId)
        if automaticAudio {
            try c.encodeNil(forKey: .audioIndex)
        } else {
            try c.encodeIfPresent(audioIndex, forKey: .audioIndex)
        }
        try c.encodeIfPresent(subtitleIndex, forKey: .subtitleIndex)
    }
}

/// What the server remembers of this client's network, for Automatic's
/// start (`GET` and `PUT /api/playback/network`).
public struct NetworkAnswer: Codable, Hashable, Sendable {
    /// The server's own network: never a speed test.
    public var local: Bool
    /// The most Automatic starts at here, kbps; `nil`: no limit known.
    public var ceilingKbps: Int?
    /// "playback" or "probe"; `nil` with no ceiling.
    public var source: String?
    /// Unix ms of what set it.
    public var since: Int?
    /// Run a speed test before Automatic starts.
    public var probe: Bool

    public init(local: Bool = false, ceilingKbps: Int? = nil, source: String? = nil, since: Int? = nil,
                probe: Bool = false) {
        self.local = local
        self.ceilingKbps = ceilingKbps
        self.source = source
        self.since = since
        self.probe = probe
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        local = try c.value(.local, or: false)
        ceilingKbps = try c.decodeIfPresent(Int.self, forKey: .ceilingKbps)
        source = try c.decodeIfPresent(String.self, forKey: .source)
        since = try c.decodeIfPresent(Int.self, forKey: .since)
        probe = try c.value(.probe, or: false)
    }
}

/// A body of `PUT /api/playback/network`: a speed test's result, or
/// Automatic's ceiling (`nil` forgets it, and is sent as `null`).
public enum NetworkReport: Encodable, Hashable, Sendable {
    case probe(clientId: String, kbps: Int, complete: Bool)
    case ceiling(clientId: String, kbps: Int?)

    private enum CodingKeys: String, CodingKey { case clientId, probeKbps, probeComplete, ceilingKbps }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .probe(let clientId, let kbps, let complete):
            try c.encode(clientId, forKey: .clientId)
            try c.encode(kbps, forKey: .probeKbps)
            try c.encode(complete, forKey: .probeComplete)
        case .ceiling(let clientId, let kbps):
            try c.encode(clientId, forKey: .clientId)
            if let kbps { try c.encode(kbps, forKey: .ceilingKbps) } else { try c.encodeNil(forKey: .ceilingKbps) }
        }
    }
}

/// What a speed test measured (`GET /api/playback/speedtest`): the bytes
/// read from the first one on, over `ms`, and whether all of them came
/// within the time.
public struct SpeedSample: Hashable, Sendable {
    public var bytes: Int
    public var ms: Int
    public var complete: Bool

    public init(bytes: Int, ms: Int, complete: Bool) {
        self.bytes = bytes
        self.ms = ms
        self.complete = complete
    }

    /// kbps: bytes × 8 / seconds / 1000.
    public var kbps: Int { ms <= 0 ? 0 : Int(Double(bytes) * 8 / Double(ms)) }
}
