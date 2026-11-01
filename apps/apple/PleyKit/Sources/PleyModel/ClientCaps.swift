import Foundation

/// What this device plays, sent with every playback request as `caps`
/// (docs/API.md, `POST /api/playback`, and "Version 2"). The server decides
/// from it whether a file plays as it is, is repackaged or is converted.
///
/// A version 2 report still carries the version 1 fields (`containers`,
/// `video_codecs`, `audio_codecs`, `hdr`, `dolby_vision`), as the Android
/// app's does. Version 2 fields left `nil` are left out of the JSON.
public struct ClientCaps: Codable, Hashable, Sendable {
    public static let version2 = 2

    /// Containers played as they are, by the server's names (file extensions).
    public var containers: [String]
    public var videoCodecs: [String]
    public var audioCodecs: [String]
    /// Shows HDR10 and HLG as they are, without tone mapping.
    public var hdr: Bool
    /// Decodes Dolby Vision, profile 5 included (it has no base layer).
    public var dolbyVision: Bool

    // Version 2.
    public var capsVersion: Int?
    /// Each video decoder, with its limits.
    public var video: [VideoDecoderCaps]?
    /// The dynamic ranges the screen shows as they are.
    public var hdrTypes: [HDRType]?
    /// The Dolby Vision profiles decoded, each with its highest level; only
    /// with a Dolby Vision screen.
    public var dolbyVisionProfiles: [DolbyVisionCaps]?
    public var audioDecode: [String]?
    public var audioPassthrough: [String]?
    /// The most channels the audio output takes: 2, 6 or 8.
    public var audioChannels: Int?
    /// Subtitles the player draws itself, by ffprobe's names.
    public var subtitleCodecs: [String]?
    public var audioTrackSwitching: Bool?
    public var pictureSubtitlesSideload: Bool?
    public var hls: HlsCaps?
    public var display: DisplayCaps?
    public var device: DeviceCapsInfo?

    public init(containers: [String] = [], videoCodecs: [String] = [], audioCodecs: [String] = [],
                hdr: Bool = false, dolbyVision: Bool = false, capsVersion: Int? = nil,
                video: [VideoDecoderCaps]? = nil, hdrTypes: [HDRType]? = nil,
                dolbyVisionProfiles: [DolbyVisionCaps]? = nil, audioDecode: [String]? = nil,
                audioPassthrough: [String]? = nil, audioChannels: Int? = nil, subtitleCodecs: [String]? = nil,
                audioTrackSwitching: Bool? = nil, pictureSubtitlesSideload: Bool? = nil, hls: HlsCaps? = nil,
                display: DisplayCaps? = nil, device: DeviceCapsInfo? = nil) {
        self.containers = containers
        self.videoCodecs = videoCodecs
        self.audioCodecs = audioCodecs
        self.hdr = hdr
        self.dolbyVision = dolbyVision
        self.capsVersion = capsVersion
        self.video = video
        self.hdrTypes = hdrTypes
        self.dolbyVisionProfiles = dolbyVisionProfiles
        self.audioDecode = audioDecode
        self.audioPassthrough = audioPassthrough
        self.audioChannels = audioChannels
        self.subtitleCodecs = subtitleCodecs
        self.audioTrackSwitching = audioTrackSwitching
        self.pictureSubtitlesSideload = pictureSubtitlesSideload
        self.hls = hls
        self.display = display
        self.device = device
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        containers = try c.value(.containers, or: [])
        videoCodecs = try c.value(.videoCodecs, or: [])
        audioCodecs = try c.value(.audioCodecs, or: [])
        hdr = try c.value(.hdr, or: false)
        dolbyVision = try c.value(.dolbyVision, or: false)
        capsVersion = try c.decodeIfPresent(Int.self, forKey: .capsVersion)
        video = try c.decodeIfPresent([VideoDecoderCaps].self, forKey: .video)
        hdrTypes = try c.decodeIfPresent([HDRType].self, forKey: .hdrTypes)
        dolbyVisionProfiles = try c.decodeIfPresent([DolbyVisionCaps].self, forKey: .dolbyVisionProfiles)
        audioDecode = try c.decodeIfPresent([String].self, forKey: .audioDecode)
        audioPassthrough = try c.decodeIfPresent([String].self, forKey: .audioPassthrough)
        audioChannels = try c.decodeIfPresent(Int.self, forKey: .audioChannels)
        subtitleCodecs = try c.decodeIfPresent([String].self, forKey: .subtitleCodecs)
        audioTrackSwitching = try c.decodeIfPresent(Bool.self, forKey: .audioTrackSwitching)
        pictureSubtitlesSideload = try c.decodeIfPresent(Bool.self, forKey: .pictureSubtitlesSideload)
        hls = try c.decodeIfPresent(HlsCaps.self, forKey: .hls)
        display = try c.decodeIfPresent(DisplayCaps.self, forKey: .display)
        device = try c.decodeIfPresent(DeviceCapsInfo.self, forKey: .device)
    }
}

/// One video decoder and its limits. Limits left out aren't known, and the
/// server doesn't check them.
public struct VideoDecoderCaps: Codable, Hashable, Sendable {
    /// ffprobe's name: "h264", "hevc", "av1", "vp9".
    public var codec: String
    public var hardware: Bool
    /// As ffprobe names them: "High", "Main", "Main 10", "Profile 2".
    public var profiles: [String]
    public var maxWidth: Int?
    public var maxHeight: Int?
    public var maxFps: Double?
    public var maxBitDepth: Int?
    /// Numbered as ffprobe numbers a stream's `level` (H.264 5.1 is 51,
    /// HEVC 5.1 is 153, AV1 5.1 is 13).
    public var maxLevel: Int?

    public init(codec: String, hardware: Bool, profiles: [String] = [], maxWidth: Int? = nil,
                maxHeight: Int? = nil, maxFps: Double? = nil, maxBitDepth: Int? = nil, maxLevel: Int? = nil) {
        self.codec = codec
        self.hardware = hardware
        self.profiles = profiles
        self.maxWidth = maxWidth
        self.maxHeight = maxHeight
        self.maxFps = maxFps
        self.maxBitDepth = maxBitDepth
        self.maxLevel = maxLevel
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        codec = try c.decode(String.self, forKey: .codec)
        hardware = try c.value(.hardware, or: false)
        profiles = try c.value(.profiles, or: [])
        maxWidth = try c.decodeIfPresent(Int.self, forKey: .maxWidth)
        maxHeight = try c.decodeIfPresent(Int.self, forKey: .maxHeight)
        maxFps = try c.decodeIfPresent(Double.self, forKey: .maxFps)
        maxBitDepth = try c.decodeIfPresent(Int.self, forKey: .maxBitDepth)
        maxLevel = try c.decodeIfPresent(Int.self, forKey: .maxLevel)
    }
}

/// A Dolby Vision profile decoded, with its highest level.
public struct DolbyVisionCaps: Codable, Hashable, Sendable {
    public var profile: Int
    public var maxLevel: Int?

    public init(profile: Int, maxLevel: Int? = nil) {
        self.profile = profile
        self.maxLevel = maxLevel
    }
}

/// What the player takes of HLS.
public struct HlsCaps: Codable, Hashable, Sendable {
    /// Reads multivariant playlists (`master.m3u8`).
    public var masterPlaylist: Bool
    /// "fmp4" and "ts", the one the player would rather have first.
    public var segments: [String]
    /// Moves between a stream's sizes by itself.
    public var variants: Bool

    public init(masterPlaylist: Bool, segments: [String], variants: Bool) {
        self.masterPlaylist = masterPlaylist
        self.segments = segments
        self.variants = variants
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        masterPlaylist = try c.value(.masterPlaylist, or: false)
        segments = try c.value(.segments, or: [])
        variants = try c.value(.variants, or: false)
    }
}

/// The screen, in pixels with the long side first, and the refresh rates
/// the player may switch it to.
public struct DisplayCaps: Codable, Hashable, Sendable {
    public var width: Int
    public var height: Int
    public var refreshRates: [Double]

    public init(width: Int, height: Int, refreshRates: [Double]) {
        self.width = width
        self.height = height
        self.refreshRates = refreshRates
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        width = try c.value(.width, or: 0)
        height = try c.value(.height, or: 0)
        refreshRates = try c.value(.refreshRates, or: [])
    }
}

/// The device: its form, how capable it is overall, and its model.
public struct DeviceCapsInfo: Codable, Hashable, Sendable {
    public var form: DeviceForm
    /// "low", "mid" or "high".
    public var tier: String?
    public var fireTv: Bool
    /// The model identifier ("iPhone15,2", "AppleTV14,1", "Mac15,6").
    public var model: String?

    public init(form: DeviceForm, tier: String? = nil, fireTv: Bool = false, model: String? = nil) {
        self.form = form
        self.tier = tier
        self.fireTv = fireTv
        self.model = model
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        form = try c.value(.form, or: .unknown)
        tier = try c.decodeIfPresent(String.self, forKey: .tier)
        fireTv = try c.value(.fireTv, or: false)
        model = try c.decodeIfPresent(String.self, forKey: .model)
    }
}

/// A dynamic range a screen shows, by the server's name.
public struct HDRType: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let hdr10 = HDRType(rawValue: "hdr10")
    public static let hdr10Plus = HDRType(rawValue: "hdr10plus")
    public static let hlg = HDRType(rawValue: "hlg")
    public static let dolbyVision = HDRType(rawValue: "dolby_vision")
}

/// The kind of device in a caps report.
public struct DeviceForm: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let phone = DeviceForm(rawValue: "phone")
    public static let tablet = DeviceForm(rawValue: "tablet")
    public static let tv = DeviceForm(rawValue: "tv")
    public static let desktop = DeviceForm(rawValue: "desktop")
    public static let unknown = DeviceForm(rawValue: "unknown")
}
