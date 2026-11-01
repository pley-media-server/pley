import Foundation
import PleyModel

/// What this device does with a file's video, by the caps report.
public enum CapsDecision: Hashable, Sendable {
    /// It plays as it is.
    case direct
    /// Dolby Vision this device can't decode, on an HDR10 screen: the server sends the HDR10 base layer.
    case remuxBaseLayer
    /// The screen can't show its HDR: the server tone-maps it.
    case serverToneMaps
    /// Not as it is, and why (for logs and details, not shown as it is).
    case unsupported(String)
}

/// The client's side of the HDR and Dolby Vision rules, a port of the
/// Android app's `DefaultPlaybackCapsPolicy`: what this device does with
/// each version of an item, and which version plays best here. The server
/// decides the session; this only picks the file to ask for.
///
/// - HDR10 plays as it is with an HDR10 screen and a 10-bit hardware
///   decoder; HDR10+ with an HDR10+ or HDR10 screen; HLG needs HLG.
/// - Dolby Vision plays as it is when the screen shows it and the report
///   lists the profile. Otherwise profiles 8.1 and 8.4 play their HDR10 or
///   HLG base layer, profile 7 needs the server's HDR10 base layer, and
///   profile 5 is tone-mapped by the server.
/// - A codec, size, rate, depth or level no decoder takes is unsupported.
///
/// Pure: it reads the item and the report, nothing else.
public enum CapsPolicy {
    /// What happens to `file`'s video here.
    public static func decide(_ file: MediaFile, _ caps: ClientCaps) -> CapsDecision {
        guard let video = file.video else { return .direct }
        let types = caps.hdrTypes ?? []
        let range = rangeOf(video)
        switch range {
        case .dolbyVision:
            return dolbyVision(video, caps)
        case .hdr10, .hdr10Plus, .hlg:
            let shown = switch range {
            case .hlg: types.contains(.hlg)
            case .hdr10Plus: types.contains(.hdr10Plus) || types.contains(.hdr10)
            default: types.contains(.hdr10)
            }
            return shown ? fits(video, codec: video.codec, caps, hdr: true) : .serverToneMaps
        default:
            return fits(video, codec: video.codec, caps, hdr: false)
        }
    }

    /// The file of `item` that plays best here: as it is before
    /// repackaged, tone-mapped or unsupported; then one no taller than the
    /// screen, the tallest, the best dynamic range among those that play
    /// as they are, and the highest bitrate. `nil` without files.
    public static func bestFile(_ item: ItemDetail, _ caps: ClientCaps) -> Int? {
        let screen = caps.display.map { min($0.width, $0.height) }
        let ranked = item.files.map { ($0, decide($0, caps)) }.sorted { a, b in
            let (fa, da) = a
            let (fb, db) = b
            if rank(da) != rank(db) { return rank(da) < rank(db) }
            let fitsA = screen.map { height(fa) <= $0 } ?? true
            let fitsB = screen.map { height(fb) <= $0 } ?? true
            if fitsA != fitsB { return fitsA }
            if height(fa) != height(fb) { return height(fa) > height(fb) }
            let rangeA = da == .direct ? rangeRank(rangeOf(fa.video)) : 0
            let rangeB = db == .direct ? rangeRank(rangeOf(fb.video)) : 0
            if rangeA != rangeB { return rangeA > rangeB }
            return (fa.bitrate ?? 0) > (fb.bitrate ?? 0)
        }
        return ranked.first?.0.id
    }

    /// The video's dynamic range, from what ffprobe found.
    public static func rangeOf(_ video: MediaStream?) -> DynamicRange {
        guard let video else { return .sdr }
        if let range = video.dynamicRange, range.rawValue != "unknown" { return range }
        if video.dvProfile != nil { return .dolbyVision }
        if video.colorTransfer == "smpte2084" { return .hdr10 }
        if video.colorTransfer == "arib-std-b67" { return .hlg }
        return .sdr
    }

    private static func dolbyVision(_ video: MediaStream, _ caps: ClientCaps) -> CapsDecision {
        let profile = video.dvProfile
        let types = caps.hdrTypes ?? []
        if types.contains(.dolbyVision), let decoded = (caps.dolbyVisionProfiles ?? []).first(where: { $0.profile == profile }) {
            if let level = video.dvLevel, let max = decoded.maxLevel, level > max {
                return .unsupported("Dolby Vision level \(level) is above this decoder's \(max)")
            }
            return .direct
        }
        let base = baseCodec(profile, video.codec)
        switch profile {
        case 5:
            // No base layer to fall back to.
            return .serverToneMaps
        case 7:
            return types.contains(.hdr10) ? .remuxBaseLayer : .serverToneMaps
        default:
            // A Blu-ray's HDR10 id in profile 8 (a profile 7 converted elsewhere) is HDR10.
            let compat = video.dvBlCompat.flatMap { profile == 8 && $0 == blBluRay ? nil : $0 }
                ?? (profile == 8 ? blHDR10 : nil)
            switch compat {
            case blHDR10: return types.contains(.hdr10) ? fits(video, codec: base, caps, hdr: true) : .serverToneMaps
            case blHLG: return types.contains(.hlg) ? fits(video, codec: base, caps, hdr: true) : .serverToneMaps
            case blSDR: return fits(video, codec: base, caps, hdr: false)
            default: return .serverToneMaps
            }
        }
    }

    /// Whether a decoder entry of `codec` takes the stream's size, rate, depth, profile and level.
    private static func fits(_ video: MediaStream, codec: String, _ caps: ClientCaps, hdr: Bool) -> CapsDecision {
        let entries = (caps.video ?? []).filter { $0.codec.caseInsensitiveCompare(codec) == .orderedSame }
        guard !entries.isEmpty else { return .unsupported("No decoder for \(codec) here") }
        let depth = bitDepth(video)
        if hdr && !entries.contains(where: { $0.hardware && ($0.maxBitDepth ?? 8) >= 10 }) {
            return .unsupported("No 10-bit hardware decoder for \(codec) here")
        }
        if entries.contains(where: { takes($0, video, depth: depth) }) { return .direct }
        let size = [video.width, video.height].compactMap { $0 }.map(String.init).joined(separator: "×")
        return .unsupported("\(codec) \(size), \(depth)-bit, is beyond this device's decoders")
    }

    private static func takes(_ entry: VideoDecoderCaps, _ video: MediaStream, depth: Int) -> Bool {
        if let w = video.width, let h = video.height, let maxW = entry.maxWidth, let maxH = entry.maxHeight {
            let asIs = w <= maxW && h <= maxH
            let turned = h <= maxW && w <= maxH
            if !asIs && !turned { return false }
        }
        if let fps = video.frameRate, let maxFps = entry.maxFps, fps > maxFps + 0.01 { return false }
        if depth > (entry.maxBitDepth ?? 8) { return false }
        if let level = video.level, level > 0, let maxLevel = entry.maxLevel, level > maxLevel { return false }
        if let profile = video.profile, !entry.profiles.isEmpty,
           !entry.profiles.contains(where: { $0.caseInsensitiveCompare(profile) == .orderedSame }),
           knownProfiles.contains(profile.lowercased()) {
            return false
        }
        return true
    }

    // Dolby Vision base-layer compatibility ids (ffprobe's dv_bl_signal_compatibility_id).
    private static let blHDR10 = 1
    private static let blSDR = 2
    private static let blHLG = 4
    private static let blBluRay = 6

    /// Profiles ffprobe names the same way the report does; others aren't refused for their name.
    private static let knownProfiles: Set<String> = [
        "main", "main 10", "main still picture", "rext", "baseline", "constrained baseline", "high", "high 10",
        "high 4:2:2", "high 4:4:4 predictive", "profile 0", "profile 1", "profile 2", "profile 3", "professional",
    ]

    /// The codec under a Dolby Vision profile: AVC for 9, AV1 for 10, HEVC for the rest.
    private static func baseCodec(_ profile: Int?, _ codec: String) -> String {
        switch profile {
        case 9: "h264"
        case 10: "av1"
        default: ["dvhe", "dvh1"].contains(codec.lowercased()) ? "hevc" : codec
        }
    }

    /// The stream's bit depth, else from its pixel format ("yuv420p10le") or profile ("Main 10").
    private static func bitDepth(_ video: MediaStream) -> Int {
        if let depth = video.bitDepth { return depth }
        if let format = video.pixFmt, let match = format.firstMatch(of: /(1[02])(?:le|be)$/), let depth = Int(match.1) {
            return depth
        }
        return video.profile?.contains("10") == true ? 10 : 8
    }

    private static func height(_ file: MediaFile) -> Int { file.video?.height ?? file.height ?? 0 }

    private static func rank(_ decision: CapsDecision) -> Int {
        switch decision {
        case .direct: 0
        case .remuxBaseLayer: 1
        case .serverToneMaps: 2
        case .unsupported: 3
        }
    }

    private static func rangeRank(_ range: DynamicRange) -> Int {
        switch range {
        case .dolbyVision: 4
        case .hdr10Plus: 3
        case .hdr10: 2
        case .hlg: 1
        default: 0
        }
    }
}
