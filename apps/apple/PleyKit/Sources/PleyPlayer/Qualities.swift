import Foundation
import PleyModel

/// A quality the player offers (docs/QUALITY.md): the original, Automatic,
/// or a transcode capped at `maxBitrateKbps` and `maxHeight`. `id` is what
/// the server's `auto_quality` and the remembered choice use ("8m"). A
/// port of the Android app's `QualityPreset`.
///
/// Automatic is a choice, never what plays: it plays the original or one
/// of the transcodes (`AutoQuality` decides which), so `original` is true
/// only for the file as it is.
public struct QualityPreset: Hashable, Sendable {
    public static let originalId = "original"
    public static let automaticId = "auto"

    public let id: String
    public let maxBitrateKbps: Int?
    public let maxHeight: Int?

    public init(_ id: String, maxBitrateKbps: Int?, maxHeight: Int?) {
        self.id = id
        self.maxBitrateKbps = maxBitrateKbps
        self.maxHeight = maxHeight
    }

    public var original: Bool { id == Self.originalId }

    /// Automatic: the original, lower only when it doesn't play well.
    public var automatic: Bool { id == Self.automaticId }

    /// How the quality is asked for: the original lets the server choose
    /// (direct play when the device can), every other quality is a transcode.
    public var mode: PlaybackMode { original || automatic ? .auto : .transcode }
}

/// The menu's qualities, in its order, and which of them a file is offered
/// (a port of the Android app's `Qualities`, the same table on every app).
public enum Qualities {
    public static let original = QualityPreset(QualityPreset.originalId, maxBitrateKbps: nil, maxHeight: nil)
    public static let automatic = QualityPreset(QualityPreset.automaticId, maxBitrateKbps: nil, maxHeight: nil)

    /// The transcodes, in the menu's order: the two 4K ones, then 1080p and below.
    public static let transcodes: [QualityPreset] = [
        QualityPreset("30m", maxBitrateKbps: 30_000, maxHeight: 2160),
        QualityPreset("15m", maxBitrateKbps: 15_000, maxHeight: 2160),
        QualityPreset("20m", maxBitrateKbps: 20_000, maxHeight: 1080),
        QualityPreset("12m", maxBitrateKbps: 12_000, maxHeight: 1080),
        QualityPreset("8m", maxBitrateKbps: 8_000, maxHeight: 1080),
        QualityPreset("4m", maxBitrateKbps: 4_000, maxHeight: 720),
        QualityPreset("2m", maxBitrateKbps: 2_000, maxHeight: 480),
    ]

    /// The menu, as on every app: Original, Automatic, then the transcodes.
    public static let all: [QualityPreset] = [original, automatic] + transcodes

    /// A quality is offered for a file's bitrate when it is at most this share of it.
    public static let lighter = 0.8

    /// The lowest quality Automatic plays.
    public static var lowest: QualityPreset { transcodes[transcodes.count - 1] }

    /// The preset with `id`, else the original.
    public static func byId(_ id: String?) -> QualityPreset {
        all.first { $0.id == id } ?? original
    }

    /// A remembered value: a preset's id ("8m", "auto"), or "Original" as people say it.
    public static func byIdOrLabel(_ value: String?) -> QualityPreset {
        guard let value else { return original }
        return all.first { $0.id.caseInsensitiveCompare(value) == .orderedSame } ?? original
    }

    /// The qualities worth offering for `file` (docs/QUALITY.md): the
    /// original and Automatic, then those that make it smaller, with fewer
    /// lines or at most 80 % of its bitrate (a copy a little lighter plays
    /// no better). Two that come to the same bitrate and lines for it show
    /// once, and they go by the lines they give the file, then bitrate.
    /// Every one when its bitrate and size are unknown.
    public static func forFile(_ file: MediaFile?) -> [QualityPreset] {
        let kbps = file?.bitrate.flatMap { $0 > 0 ? Double($0) / 1000 : nil }
        let lines = linesClassOf(file)
        if kbps == nil && lines == nil { return all }
        var seen = Set<[Int]>()
        let smaller = transcodes.filter { q in
            let lighter = kbps.map { Double(q.maxBitrateKbps ?? 0) <= $0 * Self.lighter } ?? false
            let fewer = lines.map { (q.maxHeight ?? 0) < $0 } ?? false
            guard lighter || fewer else { return false }
            return seen.insert([q.maxBitrateKbps ?? 0, linesFor(q, fileLines: lines) ?? -1]).inserted
        }
        // By the lines they give the file, then bitrate: a 4K file keeps the table's order.
        let sorted = smaller.enumerated().sorted { a, b in
            let (la, lb) = (linesFor(a.element, fileLines: lines) ?? 0, linesFor(b.element, fileLines: lines) ?? 0)
            if la != lb { return la > lb }
            let (ka, kb) = (a.element.maxBitrateKbps ?? 0, b.element.maxBitrateKbps ?? 0)
            if ka != kb { return ka > kb }
            return a.offset < b.offset
        }.map(\.element)
        return [original, automatic] + sorted
    }

    /// The lines `preset` gives a file of `fileLines`: never more than the file's.
    public static func linesFor(_ preset: QualityPreset, fileLines: Int?) -> Int? {
        guard let most = preset.maxHeight else { return fileLines }
        if let fileLines, fileLines < most { return fileLines }
        return most
    }

    /// The quality chosen for `file`: `remembered` when the file is offered
    /// it, Automatic when nothing was ever chosen, else the original (an
    /// 8 Mbps cap on a 5 Mbps file plays the original, as on the web).
    public static func choose(_ file: MediaFile?, remembered: String?) -> QualityPreset {
        guard let remembered, !remembered.trimmingCharacters(in: .whitespaces).isEmpty else { return automatic }
        let wanted = byIdOrLabel(remembered)
        return forFile(file).first { $0.id == wanted.id } ?? original
    }

    /// The file's bitrate in kbps; `nil` when unknown.
    public static func fileKbps(_ file: MediaFile?) -> Int? {
        file?.bitrate.flatMap { $0 > 0 ? $0 / 1000 : nil }
    }

    /// The file's lines; `nil` when unknown.
    public static func linesOf(_ file: MediaFile?) -> Int? {
        if let height = file?.height, height > 0 { return height }
        if let height = file?.video?.height, height > 0 { return height }
        return nil
    }

    /// The file's lines as qualities name them (2160, 1080, 720 or 480), by
    /// its width or height as the server tells them apart: a 1920×800 film
    /// is 1080p. `nil` when neither is known.
    public static func linesClassOf(_ file: MediaFile?) -> Int? {
        let w = file?.width ?? file?.video?.width ?? 0
        let h = linesOf(file) ?? 0
        if w <= 0 && h <= 0 { return nil }
        switch true {
        case w >= 3800 || h >= 2100: return 2160
        case w >= 1800 || h >= 1000: return 1080
        case w >= 1200 || h >= 700: return 720
        default: return 480
        }
    }

    /// What the server converts `file` at when it converts the original
    /// (apps/server/src/playback.rs, closest_quality): its own size's
    /// quality, with the highest bitrate up to 1.5 times the file's, or
    /// that size's lowest. The server says so itself (`auto_quality`) only
    /// when it converts; Automatic needs it before.
    public static func closest(_ file: MediaFile?) -> QualityPreset {
        let lines = linesClassOf(file) ?? 1080
        let cap = file?.bitrate.flatMap { $0 > 0 ? Double($0) * 1.5 / 1000 : nil }
        let tier = transcodes.filter { $0.maxHeight == lines }.sorted { ($0.maxBitrateKbps ?? 0) > ($1.maxBitrateKbps ?? 0) }
        return tier.first { cap == nil || Double($0.maxBitrateKbps ?? 0) <= cap! } ?? tier.last ?? byId("8m")
    }

    // MARK: Words

    /// A quality by the menu's name: "Original", "Automatic", "8 Mbps 1080p".
    public static func label(_ preset: QualityPreset) -> String {
        if preset.original { return localized("Original") }
        if preset.automatic { return localized("Automatic") }
        guard let kbps = preset.maxBitrateKbps, let lines = preset.maxHeight else { return localized("Original") }
        return "\(kbps / 1000) Mbps \(linesLabel(lines))"
    }

    /// A quality as offered for a file of `fileLines`: one taller than the
    /// file is named by the file's lines ("8 Mbps 720p" for a 720p file).
    public static func label(_ preset: QualityPreset, fileLines: Int?) -> String {
        guard let most = preset.maxHeight, let kbps = preset.maxBitrateKbps, let fileLines, fileLines < most else {
            return label(preset)
        }
        return "\(kbps / 1000) Mbps \(linesLabel(fileLines))"
    }

    /// "4K" for 2160 lines, "1080p" for 1080.
    static func linesLabel(_ lines: Int) -> String { lines >= 2160 ? "4K" : "\(lines)p" }
}
