import Foundation
import PleyModel

/// Subtitles moved in time (the Subtitle sync control), as the Android app
/// does it: in 0.1 s steps from −10 s to +10 s; positive shows them later.
public enum SubtitleSync {
    public static let stepMs = 100
    public static let maxMs = 10_000

    /// `offsetMs` rounded to a step and kept within ±10 s.
    public static func clamp(_ offsetMs: Int) -> Int {
        let steps = (Double(offsetMs) / Double(stepMs)).rounded()
        return min(max(Int(steps) * stepMs, -maxMs), maxMs)
    }

    /// The track's own time when the playhead is at `positionMs`: a cue made for t shows at t + `offsetMs`.
    public static func trackTime(_ positionMs: Int, offsetMs: Int) -> Int { positionMs - offsetMs }

    /// How long until what `track` shows changes, from the playhead at
    /// `positionMs`; `nil` when nothing changes any more.
    public static func untilChange(_ track: any CueTrack, positionMs: Int, offsetMs: Int) -> Int? {
        let at = trackTime(positionMs, offsetMs: offsetMs)
        return track.nextChangeAfter(at).map { $0 - at }
    }

    /// "+0.5 s", "−1.2 s", "0.0 s" ("+0,5 s" in French).
    public static func label(_ offsetMs: Int) -> String {
        let sign = offsetMs > 0 ? "+" : offsetMs < 0 ? "\u{2212}" : ""
        let tenths = (abs(offsetMs) + 50) / 100
        let point = ShownLanguage.locale.decimalSeparator ?? "."
        let seconds = "\(sign)\(tenths / 10)\(point)\(tenths % 10)"
        return localized("\(seconds) s")
    }

    /// Where a file's offset is kept: the file's own address on its
    /// server, so files of different servers don't mix, and the profile
    /// watching, so one profile's timing never moves another's subtitles.
    public static func key(origin: String, fileId: Int, profileId: Int?) -> String {
        let file = "\(origin)/api/files/\(fileId)"
        return profileId.map { "\(file)#profile=\($0)" } ?? file
    }
}

/// Subtitle offsets, remembered per file and profile on this device (never on the server).
public protocol SubtitleOffsets: Sendable {
    /// The offset of the file under `key`; 0 when none was set.
    func offset(_ key: String) -> Int
    /// Remembers `offsetMs` for `key`; 0 forgets it.
    func remember(_ key: String, offsetMs: Int)
}

/// Kept while the app runs: for tests and previews.
public final class MemorySubtitleOffsets: SubtitleOffsets, @unchecked Sendable {
    // Unchecked Sendable: `kept` is only touched under the lock.
    private let lock = NSLock()
    private var kept: [String: Int] = [:]

    public init() {}

    public func offset(_ key: String) -> Int { lock.withLock { kept[key] ?? 0 } }

    public func remember(_ key: String, offsetMs: Int) {
        lock.withLock { kept[key] = offsetMs == 0 ? nil : offsetMs }
    }
}

/// In `UserDefaults`, an entry per file and profile whose offset isn't 0,
/// under one dictionary (`pley.subtitleOffsets`).
public final class DefaultsSubtitleOffsets: SubtitleOffsets, @unchecked Sendable {
    // Unchecked Sendable: UserDefaults is thread-safe, and every change goes through the lock.
    private let defaults: UserDefaults
    private let lock = NSLock()
    static let key = "pley.subtitleOffsets"
    /// The most files remembered; the oldest go first.
    static let most = 500

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public func offset(_ key: String) -> Int {
        (defaults.dictionary(forKey: Self.key)?[key] as? [Any])?.first as? Int ?? 0
    }

    public func remember(_ key: String, offsetMs: Int) {
        lock.withLock {
            var all = defaults.dictionary(forKey: Self.key) ?? [:]
            if offsetMs == 0 {
                all[key] = nil
            } else {
                // With when it was set, so the oldest go when there are too many.
                all[key] = [offsetMs, Date().timeIntervalSince1970]
                if all.count > Self.most {
                    let oldest = all.min { a, b in
                        ((a.value as? [Any])?.last as? Double ?? 0) < ((b.value as? [Any])?.last as? Double ?? 0)
                    }
                    if let oldest { all[oldest.key] = nil }
                }
            }
            defaults.set(all, forKey: Self.key)
        }
    }
}
