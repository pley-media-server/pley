import Foundation
import PleyModel

/// One reading of what this device plays: the report sent with playback
/// requests, the device class it came from, and the same in words.
public struct CapsSnapshot: Codable, Hashable, Sendable {
    public var report: ClientCaps
    /// The class from `DeviceClasses`: "a17_m3", "apple_tv_hd"…
    public var deviceClass: String
    /// What the device plays, a line each, for the player's details.
    public var lines: [String]

    public init(report: ClientCaps, deviceClass: String, lines: [String]) {
        self.report = report
        self.deviceClass = deviceClass
        self.lines = lines
    }
}

/// A report in words, for the player's details and `pley-apple-check
/// --print-caps`: "HEVC: Main, Main 10, up to 3840×2160 at 60 fps, 10-bit,
/// level 156".
public enum CapsLines {
    public static func lines(_ report: ClientCaps, _ input: ProbeInput) -> [String] {
        var lines: [String] = []
        let device = report.device
        lines.append("Device: \(input.model), \(input.platform.rawValue) \(input.osVersion)"
                     + (input.chip.map { ", \($0)" } ?? "")
                     + (input.simulator ? " (simulator)" : "")
                     + ", tier \(device?.tier ?? "unknown")")
        let video = report.video ?? []
        if video.isEmpty { lines.append("No video decoder") }
        for entry in video {
            var parts = [entry.profiles.isEmpty ? "any profile" : entry.profiles.joined(separator: ", ")]
            if let width = entry.maxWidth, let height = entry.maxHeight {
                parts.append("up to \(width)×\(height)" + (entry.maxFps.map { " at \(rate($0)) fps" } ?? ""))
            }
            parts.append("\(entry.maxBitDepth ?? 8)-bit")
            if let level = entry.maxLevel { parts.append("level \(level)") }
            lines.append("\(codecName(entry.codec)): \(parts.joined(separator: ", "))")
        }
        let hardware = input.hardwareDecode.sorted { $0.key < $1.key }
            .map { "\(codecName($0.key)) \($0.value ? "yes" : "no")" }
        lines.append("Hardware decoding: \(hardware.isEmpty ? "none said" : hardware.joined(separator: ", "))")
        let headroom = input.hdr.edrHeadroom.map { ", headroom \(rate($0))×" } ?? ""
        lines.append("HDR: " + (input.hdr.eligible ? "eligible" : "not eligible")
                     + (input.hdr.modes.isEmpty ? "" : ", modes \(input.hdr.modes.joined(separator: ", "))")
                     + headroom)
        let types = (report.hdrTypes ?? []).map(rangeName)
        lines.append("HDR played as it is: \(types.isEmpty ? "none" : types.joined(separator: ", "))")
        let dv = (report.dolbyVisionProfiles ?? []).map { "\($0.profile) (level \($0.maxLevel.map(String.init) ?? "unknown"))" }
        lines.append("Dolby Vision profiles: \(dv.isEmpty ? "none" : dv.joined(separator: ", "))")
        if let display = report.display {
            let rates = display.refreshRates.map(rate).joined(separator: ", ")
            lines.append("Screen: \(display.width)×\(display.height), \(rates) Hz"
                         + (input.display.matchContent ? ", Match Content on" : ""))
        } else {
            lines.append("Screen: unknown")
        }
        lines.append("Audio decoders: \((report.audioDecode ?? []).joined(separator: ", "))")
        lines.append("Audio output: \(input.audio.route), \(report.audioChannels ?? 2) channels"
                     + (input.audio.spatial ? ", spatial audio" : ""))
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

    /// 60 is "60", 23.976 is "23.976".
    static func rate(_ value: Double) -> String {
        value.rounded() == value ? String(Int(value)) : String(format: "%g", value)
    }
}
