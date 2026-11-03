import Foundation

/// What the probe read from the system: the facts the report is built from
/// (`CapsBuilder`), and nothing else. It is recorded per device class in
/// the tests (`Tests/PleyCapsTests/Probes`), so it holds no personal data:
/// no device name, account or address.
public struct ProbeInput: Codable, Hashable, Sendable {
    public enum Platform: String, Codable, Sendable {
        case iOS, iPadOS, tvOS, macOS
    }

    public var platform: Platform
    /// "26.1", "16.7.10".
    public var osVersion: String
    /// The model identifier: "iPhone15,2", "iPad6,11", "AppleTV14,1",
    /// "Mac15,6"; in a simulator, the simulated device's.
    public var model: String
    /// The Mac's processor (`machdep.cpu.brand_string`): "Apple M3 Pro",
    /// "Intel(R) Core(TM) i9…". `nil` elsewhere.
    public var chip: String?
    public var memoryBytes: UInt64
    public var simulator: Bool
    /// What VideoToolbox decodes in hardware, by ffprobe's codec name:
    /// "hevc", "av1", "vp9".
    public var hardwareDecode: [String: Bool]
    public var hdr: HDRFacts
    public var display: DisplayFacts
    public var audio: AudioFacts

    public init(platform: Platform, osVersion: String, model: String, chip: String? = nil, memoryBytes: UInt64,
                simulator: Bool = false, hardwareDecode: [String: Bool], hdr: HDRFacts, display: DisplayFacts,
                audio: AudioFacts) {
        self.platform = platform
        self.osVersion = osVersion
        self.model = model
        self.chip = chip
        self.memoryBytes = memoryBytes
        self.simulator = simulator
        self.hardwareDecode = hardwareDecode
        self.hdr = hdr
        self.display = display
        self.audio = audio
    }
}

/// What the system says of HDR.
public struct HDRFacts: Codable, Hashable, Sendable {
    /// `AVPlayer.eligibleForHDRPlayback`: an HDR screen is there and the
    /// device plays HDR to it.
    public var eligible: Bool
    /// `AVPlayer.availableHDRModes` on iOS and tvOS, as "hdr10", "hlg" and
    /// "dolby_vision"; empty on the Mac, which doesn't have it.
    public var modes: [String]
    /// The screen's `maximumPotentialExtendedDynamicRangeColorComponentValue`
    /// on the Mac (`potentialEDRHeadroom` on iPhone and iPad): above 1, it
    /// shows brighter than white.
    public var edrHeadroom: Double?

    public init(eligible: Bool, modes: [String] = [], edrHeadroom: Double? = nil) {
        self.eligible = eligible
        self.modes = modes
        self.edrHeadroom = edrHeadroom
    }
}

/// The screen the video is shown on.
public struct DisplayFacts: Codable, Hashable, Sendable {
    /// Pixels, the long side first; 0 when no screen is known.
    public var width: Int
    public var height: Int
    public var maxFps: Int
    /// Apple TV: Match Content's frame rate is on
    /// (`AVDisplayManager.isDisplayCriteriaMatchingEnabled`).
    public var matchContent: Bool

    public init(width: Int, height: Int, maxFps: Int, matchContent: Bool = false) {
        self.width = max(width, height)
        self.height = min(width, height)
        self.maxFps = maxFps
        self.matchContent = matchContent
    }
}

/// The audio output.
public struct AudioFacts: Codable, Hashable, Sendable {
    /// The channels the output takes.
    public var maxChannels: Int
    /// The route's output plays spatial audio (AirPods and the like).
    public var spatial: Bool
    /// "speaker", "headphones", "bluetooth", "hdmi", "airplay", "usb" or "other".
    public var route: String

    public init(maxChannels: Int, spatial: Bool = false, route: String) {
        self.maxChannels = maxChannels
        self.spatial = spatial
        self.route = route
    }

    public static let speaker = "speaker"
    public static let headphones = "headphones"
    public static let bluetooth = "bluetooth"
    public static let hdmi = "hdmi"
    public static let airplay = "airplay"
    public static let usb = "usb"
    public static let other = "other"
}

/// The screen a player window is on, as the app knows it: on the Mac the
/// window's `NSScreen`, on iPhone and iPad its scene's screen.
public struct ScreenFacts: Hashable, Sendable {
    /// Pixels.
    public var width: Int
    public var height: Int
    public var maxFps: Int
    public var edrHeadroom: Double?

    public init(width: Int, height: Int, maxFps: Int, edrHeadroom: Double? = nil) {
        self.width = width
        self.height = height
        self.maxFps = maxFps
        self.edrHeadroom = edrHeadroom
    }
}
