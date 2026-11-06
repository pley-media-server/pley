import Foundation

/// Relative times, as Android says them (`model/format/Times.kt`, the
/// web's `Intl.RelativeTimeFormat` with numeric "auto"), in the language
/// shown.
extension Words {
    /// The units of relative times, as Intl.RelativeTimeFormat has them.
    private enum RelativeUnit: Int, Comparable {
        case second, minute, hour, day, week, month, year

        static func < (a: Self, b: Self) -> Bool { a.rawValue < b.rawValue }
    }

    /// Seconds per unit, and the age below which that unit is used.
    private static let relativeUnits: [(below: Int, unit: RelativeUnit, seconds: Int)] = [
        (60, .second, 1), (3_600, .minute, 60), (86_400, .hour, 3_600), (86_400 * 7, .day, 86_400),
        (86_400 * 30, .week, 86_400 * 7), (86_400 * 365, .month, 86_400 * 30), (Int.max, .year, 86_400 * 365),
    ]

    /// Unix seconds → "3 minutes ago", "in 5 hours", "yesterday", "last
    /// week", "now": the largest unit that fits, in the language shown.
    /// "never" without a time.
    public static func relative(_ ts: Int?, now: Int = Int(Date().timeIntervalSince1970)) -> String {
        guard let ts, ts != 0 else { return localized("never") }
        let diff = ts - now
        let unit = relativeUnits.first { abs(diff) < $0.below } ?? relativeUnits[relativeUnits.count - 1]
        // Halves round up, as Kotlin's Math.round does.
        let value = Int((Double(diff) / Double(unit.seconds) + 0.5).rounded(.down))
        switch value {
        case 0: return thisTime(unit.unit)
        case -1 where unit.unit >= .day: return lastTime(unit.unit)
        case 1 where unit.unit >= .day: return nextTime(unit.unit)
        default: return value < 0 ? ago(-value, unit.unit) : later(value, unit.unit)
        }
    }

    /// `unit`'s now: "now" (seconds), "this minute", "today", "this week".
    private static func thisTime(_ unit: RelativeUnit) -> String {
        switch unit {
        case .second: localized("now")
        case .minute: localized("this minute")
        case .hour: localized("this hour")
        case .day: localized("today")
        case .week: localized("this week")
        case .month: localized("this month")
        case .year: localized("this year")
        }
    }

    /// The day, week, month or year before: "yesterday", "last week".
    private static func lastTime(_ unit: RelativeUnit) -> String {
        switch unit {
        case .day: localized("yesterday")
        case .week: localized("last week")
        case .month: localized("last month")
        default: localized("last year")
        }
    }

    /// The day, week, month or year after: "tomorrow", "next week".
    private static func nextTime(_ unit: RelativeUnit) -> String {
        switch unit {
        case .day: localized("tomorrow")
        case .week: localized("next week")
        case .month: localized("next month")
        default: localized("next year")
        }
    }

    /// "3 minutes ago".
    private static func ago(_ n: Int, _ unit: RelativeUnit) -> String {
        switch unit {
        case .second: localized("\(n) seconds ago")
        case .minute: localized("\(n) minutes ago")
        case .hour: localized("\(n) hours ago")
        case .day: localized("\(n) days ago")
        case .week: localized("\(n) weeks ago")
        case .month: localized("\(n) months ago")
        case .year: localized("\(n) years ago")
        }
    }

    /// "in 5 hours".
    private static func later(_ n: Int, _ unit: RelativeUnit) -> String {
        switch unit {
        case .second: localized("in \(n) seconds")
        case .minute: localized("in \(n) minutes")
        case .hour: localized("in \(n) hours")
        case .day: localized("in \(n) days")
        case .week: localized("in \(n) weeks")
        case .month: localized("in \(n) months")
        case .year: localized("in \(n) years")
        }
    }
}
