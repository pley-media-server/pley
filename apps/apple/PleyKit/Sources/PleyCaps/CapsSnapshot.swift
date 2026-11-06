import Foundation
import PleyModel

/// One reading of what this device plays: the report sent with playback
/// requests, the device class it came from, and the same in words.
public struct CapsSnapshot: Codable, Hashable, Sendable {
    public var report: ClientCaps
    /// The class from `DeviceClasses`: "a17_m3", "apple_tv_hd"…
    public var deviceClass: String
    /// What was read, so the words follow the language shown when they're read.
    private var input: ProbeInput?
    private var fixedLines: [String]

    /// What the device plays, a line each, for the player's details: in
    /// the language shown now.
    public var lines: [String] { input.map { CapsLines.lines(report, $0) } ?? fixedLines }

    public init(report: ClientCaps, deviceClass: String, lines: [String]) {
        self.report = report
        self.deviceClass = deviceClass
        self.fixedLines = lines
    }

    /// The snapshot of `input`, its lines worded at each read.
    public init(report: ClientCaps, deviceClass: String, input: ProbeInput) {
        self.report = report
        self.deviceClass = deviceClass
        self.input = input
        self.fixedLines = []
    }
}

/// A report in words, for the player's details and `pley-apple-check
/// --print-caps`: "HEVC: Main, Main 10, up to 3840×2160 at 60 fps, 10-bit,
/// level 156".
public enum CapsLines {
    public static func lines(_ report: ClientCaps, _ input: ProbeInput) -> [String] {
        var lines: [String] = []
        let device = report.device
        let tier = device?.tier ?? localized("unknown")
        let os = "\(input.platform.rawValue) \(input.osVersion)"
        let system = input.chip.map { "\(os), \($0)" } ?? os
        lines.append(input.simulator ? localized("Device: \(input.model), \(system) (simulator), tier \(tier)")
                     : localized("Device: \(input.model), \(system), tier \(tier)"))
        let video = report.video ?? []
        if video.isEmpty { lines.append(localized("No video decoder")) }
        for entry in video {
            var parts = [entry.profiles.isEmpty ? localized("any profile") : entry.profiles.joined(separator: ", ")]
            if let width = entry.maxWidth, let height = entry.maxHeight {
                let size = "\(width)×\(height)"
                parts.append(entry.maxFps.map { localized("up to \(size) at \(rate($0)) fps") } ?? localized("up to \(size)"))
            }
            parts.append(localized("\(String(entry.maxBitDepth ?? 8))-bit"))
            if let level = entry.maxLevel { parts.append(localized("level \(String(level))")) }
            lines.append(localized("\(codecName(entry.codec)): \(parts.joined(separator: ", "))"))
        }
        let hardware = input.hardwareDecode.sorted { $0.key < $1.key }
            .map { $0.value ? localized("\(codecName($0.key)) yes") : localized("\(codecName($0.key)) no") }
        let decoding = hardware.isEmpty ? localized("none said") : hardware.joined(separator: ", ")
        lines.append(localized("Hardware decoding: \(decoding)"))
        var hdr = [input.hdr.eligible ? localized("eligible") : localized("not eligible")]
        if !input.hdr.modes.isEmpty { hdr.append(localized("modes \(input.hdr.modes.joined(separator: ", "))")) }
        if let headroom = input.hdr.edrHeadroom { hdr.append(localized("headroom \(rate(headroom))×")) }
        lines.append(localized("HDR: \(hdr.joined(separator: ", "))"))
        let types = (report.hdrTypes ?? []).map(rangeName)
        let played = types.isEmpty ? localized("none") : types.joined(separator: ", ")
        lines.append(localized("HDR played as it is: \(played)"))
        let dv = (report.dolbyVisionProfiles ?? []).map { profile in
            let level = profile.maxLevel.map(String.init) ?? localized("unknown")
            return localized("\(String(profile.profile)) (level \(level))")
        }
        let profiles = dv.isEmpty ? localized("none") : dv.joined(separator: ", ")
        lines.append(localized("Dolby Vision profiles: \(profiles)"))
        if let display = report.display {
            let size = "\(display.width)×\(display.height)"
            let rates = display.refreshRates.map(rate).joined(separator: ", ")
            lines.append(input.display.matchContent ? localized("Screen: \(size), \(rates) Hz, Match Content on")
                         : localized("Screen: \(size), \(rates) Hz"))
        } else {
            lines.append(localized("Screen: unknown"))
        }
        lines.append(localized("Audio decoders: \((report.audioDecode ?? []).joined(separator: ", "))"))
        let channels = report.audioChannels ?? 2
        lines.append(input.audio.spatial ? localized("Audio output: \(input.audio.route), \(channels) channels, spatial audio")
                     : localized("Audio output: \(input.audio.route), \(channels) channels"))
        return lines
    }

    static func codecName(_ codec: String) -> String {
        switch codec {
        case "h264": "H.264"
        case "hevc": "HEVC"
        case "av1": "AV1"
        case "vp9": "VP9"
        default: codec
        }
    }

    static func rangeName(_ type: HDRType) -> String {
        switch type {
        case .hdr10: "HDR10"
        case .hdr10Plus: "HDR10+"
        case .hlg: "HLG"
        case .dolbyVision: "Dolby Vision"
        default: type.rawValue
        }
    }

    /// 60 is "60", 23.976 is "23.976" ("23,976" in French).
    static func rate(_ value: Double) -> String {
        value.rounded() == value ? String(Int(value))
            : value.formatted(.number.precision(.significantDigits(1...6)).grouping(.never).locale(ShownLanguage.locale))
    }
}
