import Foundation

/// Relative times, as Android says them (`model/format/Times.kt`, the
/// web's `Intl.RelativeTimeFormat` with numeric "auto", in English).
extension Words {
    /// Seconds per unit, and the age below which that unit is used.
    private static let relativeUnits: [(below: Int, unit: String, seconds: Int)] = [
        (60, "second", 1), (3_600, "minute", 60), (86_400, "hour", 3_600), (86_400 * 7, "day", 86_400),
        (86_400 * 30, "week", 86_400 * 7), (86_400 * 365, "month", 86_400 * 30), (Int.max, "year", 86_400 * 365),
    ]

    /// Unix seconds → "3 minutes ago", "in 5 hours", "yesterday", "last
    /// week", "now": the largest unit that fits. "never" without a time.
    public static func relative(_ ts: Int?, now: Int = Int(Date().timeIntervalSince1970)) -> String {
        guard let ts, ts != 0 else { return "never" }
        let diff = ts - now
        let unit = relativeUnits.first { abs(diff) < $0.below } ?? relativeUnits[relativeUnits.count - 1]
        // Halves round up, as Kotlin's Math.round does.
        let value = Int((Double(diff) / Double(unit.seconds) + 0.5).rounded(.down))
        switch (value, unit.unit) {
        case (0, "second"): return "now"
        case (0, "day"): return "today"
        case (0, _): return "this \(unit.unit)"
        case (-1, "day"): return "yesterday"
        case (-1, "week"), (-1, "month"), (-1, "year"): return "last \(unit.unit)"
        case (1, "day"): return "tomorrow"
        case (1, "week"), (1, "month"), (1, "year"): return "next \(unit.unit)"
        default:
            let n = abs(value)
            let words = n == 1 ? "1 \(unit.unit)" : "\(n) \(unit.unit)s"
            return value < 0 ? "\(words) ago" : "in \(words)"
        }
    }
}
