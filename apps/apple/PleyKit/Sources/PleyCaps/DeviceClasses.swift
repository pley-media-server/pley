import Foundation
import PleyModel

/// A family of Apple devices that decode video alike: one chip generation
/// (or a few). VideoToolbox only says whether a codec is decoded in
/// hardware, so the sizes, frame rates, levels and bit depths come from
/// here, from Apple's published specifications, as the Android app's come
/// from its quirks.
public struct DeviceClass: Sendable, Hashable {
    /// How the class's tier follows from the device's memory.
    public enum TierRule: Sendable, Hashable {
        /// Always low (the A8 and A9).
        case low
        /// Low with about 2 GB, mid otherwise.
        case capped
        /// Low with about 2 GB, high with about 6 GB or more, mid otherwise
        /// (the A14, the M1 and later, and Intel Macs).
        case byMemory
    }

    public var id: String
    /// Model identifiers, whole ("iPad7,5") or a whole generation
    /// ("iPhone9,", ending with the comma).
    public var models: [String]
    /// Apple silicon by generation: "Apple M1" takes "Apple M1 Pro" too.
    public var chips: [String]
    public var video: [VideoDecoderCaps]
    /// The highest Dolby Vision level decoded; `nil`: no Dolby Vision.
    public var dolbyVisionMaxLevel: Int?
    public var tier: TierRule

    public init(id: String, models: [String] = [], chips: [String] = [], video: [VideoDecoderCaps],
                dolbyVisionMaxLevel: Int?, tier: TierRule) {
        self.id = id
        self.models = models
        self.chips = chips
        self.video = video
        self.dolbyVisionMaxLevel = dolbyVisionMaxLevel
        self.tier = tier
    }
}

/// Apple's devices, by class.
public enum DeviceClasses {
    static let h264Profiles = ["Constrained Baseline", "Baseline", "Main", "High"]

    static func h264(_ width: Int, _ height: Int, _ fps: Double, level: Int) -> VideoDecoderCaps {
        VideoDecoderCaps(codec: "h264", hardware: true, profiles: h264Profiles, maxWidth: width, maxHeight: height,
                         maxFps: fps, maxBitDepth: 8, maxLevel: level)
    }

    static func hevc(_ width: Int, _ height: Int, _ fps: Double, level: Int, main10: Bool = true) -> VideoDecoderCaps {
        VideoDecoderCaps(codec: "hevc", hardware: true, profiles: main10 ? ["Main", "Main 10"] : ["Main"],
                         maxWidth: width, maxHeight: height, maxFps: fps, maxBitDepth: main10 ? 10 : 8,
                         maxLevel: level)
    }

    /// AV1 Main, 10-bit, level 5.1 (ffprobe's `seq_level_idx` 13).
    static let av1 = VideoDecoderCaps(codec: "av1", hardware: true, profiles: ["Main"], maxWidth: 3840,
                                      maxHeight: 2160, maxFps: 60, maxBitDepth: 10, maxLevel: 13)

    /// Apple TV HD (A8): 1080p H.264 only.
    public static let appleTVHD = DeviceClass(
        id: "apple_tv_hd", models: ["AppleTV5,"],
        video: [h264(1920, 1080, 60, level: 42)], dolbyVisionMaxLevel: nil, tier: .low)

    /// A9 and A9X: iPhone 6s and SE, iPad 5th generation, the first iPad
    /// Pros. 8-bit HEVC only, until the A9 is seen decoding Main 10.
    public static let a9 = DeviceClass(
        id: "a9", models: ["iPhone8,", "iPad6,"],
        video: [h264(3840, 2160, 30, level: 51), hevc(3840, 2160, 30, level: 150, main10: false)],
        dolbyVisionMaxLevel: nil, tier: .low)

    /// A10: iPhone 7, iPad 6th and 7th generation. 4K at 30 fps.
    public static let a10 = DeviceClass(
        id: "a10", models: ["iPhone9,", "iPad7,5", "iPad7,6", "iPad7,11", "iPad7,12"],
        video: [h264(3840, 2160, 30, level: 51), hevc(3840, 2160, 30, level: 153)],
        dolbyVisionMaxLevel: 7, tier: .capped)

    /// A10X: iPad Pro 2nd generation, Apple TV 4K 1st generation. 4K at 60 fps.
    public static let a10X = DeviceClass(
        id: "a10x", models: ["iPad7,1", "iPad7,2", "iPad7,3", "iPad7,4", "AppleTV6,"],
        video: [h264(3840, 2160, 60, level: 52), hevc(3840, 2160, 60, level: 153)],
        dolbyVisionMaxLevel: 9, tier: .capped)

    /// A11 to A13: iPhone 8 to 11 and SE 2, iPads of those years, Apple TV
    /// 4K 2nd generation.
    public static let a11 = DeviceClass(
        id: "a11_a13", models: ["iPhone10,", "iPhone11,", "iPhone12,", "iPad8,", "iPad11,", "iPad12,", "AppleTV11,"],
        video: [h264(3840, 2160, 60, level: 52), hevc(3840, 2160, 60, level: 156)],
        dolbyVisionMaxLevel: 9, tier: .capped)

    /// A14 to A16, M1 and M2: iPhone 12 to 15, iPads of those years, Apple
    /// TV 4K 3rd generation, the first Apple silicon Macs.
    public static let a14 = DeviceClass(
        id: "a14_m2",
        models: ["iPhone13,", "iPhone14,", "iPhone15,", "iPad13,", "iPad14,", "iPad15,7", "iPad15,8", "AppleTV14,"],
        chips: ["Apple M1", "Apple M2"],
        video: [h264(3840, 2160, 60, level: 52), hevc(3840, 2160, 60, level: 156)],
        dolbyVisionMaxLevel: 9, tier: .byMemory)

    /// A17 Pro, A18, A19, M3 and later: AV1 too.
    public static let a17 = DeviceClass(
        id: "a17_m3", models: ["iPhone16,", "iPhone17,", "iPhone18,", "iPad15,", "iPad16,"],
        chips: ["Apple M3", "Apple M4", "Apple M5"],
        video: [h264(3840, 2160, 60, level: 52), hevc(3840, 2160, 60, level: 156), av1],
        dolbyVisionMaxLevel: 9, tier: .byMemory)

    /// Intel Macs: HEVC only where VideoToolbox decodes it, no Dolby Vision.
    public static let intelMac = DeviceClass(
        id: "intel_mac", chips: ["Intel"],
        video: [h264(3840, 2160, 60, level: 52), hevc(3840, 2160, 60, level: 153)],
        dolbyVisionMaxLevel: nil, tier: .byMemory)

    public static let table: [DeviceClass] = [appleTVHD, a9, a10, a10X, a11, a14, a17, intelMac]

    /// A device the table doesn't know newer than its newest of the same
    /// kind gets this class.
    public static let newest = a17
    /// Any other device the table doesn't know.
    public static let fallback = a11

    /// The class of the device the probe read: a Mac by its chip, anything
    /// else by its model identifier.
    public static func match(_ input: ProbeInput) -> DeviceClass {
        if input.platform == .macOS {
            return matchChip(input.chip ?? "")
        }
        return matchModel(input.model)
    }

    static func matchChip(_ chip: String) -> DeviceClass {
        if chip.contains("Intel") { return intelMac }
        guard let generation = appleSilicon(chip) else { return fallback }
        if let known = table.first(where: { $0.chips.compactMap(appleSilicon).contains(generation) }) { return known }
        let newestKnown = table.flatMap(\.chips).compactMap(appleSilicon).max() ?? 0
        return generation > newestKnown ? newest : fallback
    }

    /// "Apple M3 Pro" is 3.
    static func appleSilicon(_ chip: String) -> Int? {
        guard chip.hasPrefix("Apple M") else { return nil }
        return Int(chip.dropFirst("Apple M".count).prefix { $0.isNumber })
    }

    static func matchModel(_ model: String) -> DeviceClass {
        if let exact = table.first(where: { $0.models.contains(model) }) { return exact }
        if let generation = table.first(where: { $0.models.contains { $0.hasSuffix(",") && model.hasPrefix($0) } }) {
            return generation
        }
        // Unknown: newer than the table's newest of its family, or not.
        guard let (family, major) = familyAndMajor(model) else { return fallback }
        let known = table.flatMap(\.models).compactMap(familyAndMajor).filter { $0.0 == family }.map(\.1)
        if let newestKnown = known.max(), major > newestKnown { return newest }
        return fallback
    }

    /// "iPhone15,2" is ("iPhone", 15).
    static func familyAndMajor(_ model: String) -> (String, Int)? {
        let family = model.prefix { $0.isLetter }
        let digits = model.dropFirst(family.count).prefix { $0.isNumber }
        guard !family.isEmpty, let major = Int(digits) else { return nil }
        return (String(family), major)
    }

    /// The tier in the report: "low", "mid" or "high".
    public static func tier(_ rule: DeviceClass.TierRule, memoryBytes: UInt64) -> String {
        let gib = Double(memoryBytes) / 1_073_741_824
        // Devices sold with 2 GB report a little under or over it; 6 GB
        // devices a little under 6.
        let small = gib < 2.5
        let large = gib >= 5.5
        switch rule {
        case .low: return "low"
        case .capped: return small ? "low" : "mid"
        case .byMemory: return small ? "low" : large ? "high" : "mid"
        }
    }
}
