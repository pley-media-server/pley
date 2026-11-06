import Foundation
import PleyData
import PleyModel

/// How a Live TV page shows the channels: the guide (channels by time) or cards.
public enum LiveViewKind: String, CaseIterable, Sendable {
    case guide, channels

    public var title: String {
        switch self {
        case .guide: localized("Guide")
        case .channels: localized("Channels")
        }
    }
}

/// A region ("UK") gathering its groups ("UK| Sport", "UK| News"…), with their channels' count.
public struct GroupRegion: Hashable, Sendable, Identifiable {
    public let region: String
    public let groups: [LiveGroup]
    public let count: Int

    public var id: String { region }
}

/// The groups as the categories list them: `regions` in alphabetical
/// order, each opening to its groups, then `others` (no region, or a
/// region of one group) in the provider's order. With fewer than two
/// regions, every group is in `others`, in the provider's order.
public struct GroupSections: Hashable, Sendable {
    public let regions: [GroupRegion]
    public let others: [LiveGroup]
}

/// Live TV's rules, shared by its pages and its player: a port of
/// Android's `LiveTv` (the web's apps/web/src/pages/livetv).
public enum LiveTvRules {
    /// The guide's columns: half an hour each.
    public static let slot: Int64 = 30 * 60
    /// How far ahead the guide goes from where it starts.
    public static let guideSpan: Int64 = 48 * 3600
    /// At most this many groups show for a search.
    public static let foundGroups = 12
    /// The progress bars and the now line move this often on the page.
    public static let pageTick: Duration = .seconds(15)
    /// What's on moves this often in the player.
    public static let playerTick: Duration = .seconds(5)
    /// What's on is asked for again at most this often.
    public static let refreshFloor: Duration = .seconds(60)

    /// The regions of two or more groups, alphabetically, then the rest in
    /// the provider's order; one list with fewer than two regions.
    public static func groupSections(_ groups: [LiveGroup]) -> GroupSections {
        var byRegion: [String: [LiveGroup]] = [:]
        for group in groups {
            if let region = group.region { byRegion[region, default: []].append(group) }
        }
        let regions = byRegion.filter { $0.value.count > 1 }
            .map { GroupRegion(region: $0.key, groups: $0.value, count: $0.value.reduce(0) { $0 + $1.count }) }
            .sorted { $0.region.localizedCaseInsensitiveCompare($1.region) == .orderedAscending }
        guard regions.count >= 2 else { return GroupSections(regions: [], others: groups) }
        let gathered = Set(regions.map(\.region))
        return GroupSections(regions: regions, others: groups.filter { $0.region.map(gathered.contains) != true })
    }

    /// The groups a search names, as quick links beside its channels (a dozen at most).
    public static func foundGroups(_ groups: [LiveGroup], query: String) -> [LiveGroup] {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !q.isEmpty else { return [] }
        return Array(groups.filter { $0.name.lowercased().contains(q) }.prefix(foundGroups))
    }

    /// A name's region and the rest, when it begins with one the way
    /// providers write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as
    /// the server splits group names.
    public static func splitRegion(_ name: String) -> (region: String, rest: String)? {
        var n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        while n.hasPrefix("|") { n = String(n.dropFirst()).trimmingCharacters(in: .whitespaces) }
        let patterns = [#"^\[([^\]]{1,16})\]\s*(.+)$"#, #"^([^|]{1,16})\|\s*(.+)$"#, #"^([A-Z]{1,4}):\s*(.+)$"#]
        for pattern in patterns {
            guard let regex = try? NSRegularExpression(pattern: pattern),
                  let match = regex.firstMatch(in: n, range: NSRange(n.startIndex..., in: n)),
                  let regionRange = Range(match.range(at: 1), in: n), let restRange = Range(match.range(at: 2), in: n)
            else { continue }
            let region = n[regionRange].trimmingCharacters(in: .whitespaces)
            var rest = String(n[restRange])
            while let first = rest.first, "|:- ".contains(first) { rest.removeFirst() }
            rest = rest.trimmingCharacters(in: .whitespaces)
            return region.isEmpty || rest.isEmpty ? nil : (region, rest)
        }
        return nil
    }

    /// A channel's name as a list shows it: in a group, without the region
    /// the group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky Sports 1").
    public static func channelName(_ channel: LiveChannel, in category: LiveCategory?) -> String {
        guard case .group? = category, let name = splitRegion(channel.name), let group = splitRegion(channel.group),
              name.region.caseInsensitiveCompare(group.region) == .orderedSame
        else { return channel.name }
        return name.rest
    }

    /// "UK · Sport" for a group of a region; its label otherwise, or `name`.
    public static func groupName(_ group: LiveGroup?, name: String) -> String {
        guard let group else { return name.isEmpty ? localized("No group") : name }
        if let region = group.region { return "\(region) · \(group.label)" }
        return group.label.isEmpty ? localized("No group") : group.label
    }

    /// A category's name: "All channels", "Favourites", "Recently watched", or its group's.
    public static func categoryName(_ category: LiveCategory, groups: [LiveGroup]) -> String {
        switch category {
        case .all: localized("All channels")
        case .favorites: localized("Favourites")
        case .recent: localized("Recently watched")
        case .group(let name): groupName(groups.first { $0.name == name }, name: splitRegion(name).map {
            "\($0.region) · \($0.rest)"
        } ?? name)
        }
    }

    /// Where the guide starts for `now` (unix seconds): the start of the
    /// half hour before the one it is in.
    public static func guideStart(_ now: Int64) -> Int64 {
        floorDiv(now, slot) * slot - slot
    }

    /// The guide's chunks that cover `from` to `to`, and one either side,
    /// so what comes into view next is there already; none before `start`
    /// or past its span.
    public static func chunks(start: Int64, from: Int64, to: Int64) -> [Int64] {
        let size = LiveTvRepository.guideChunk
        let first = floorDiv(max(from, start), size) * size - size
        let last = floorDiv(min(to, start + guideSpan), size) * size + size
        let floor = floorDiv(start, size) * size
        return stride(from: first, through: last, by: Int(size)).filter { $0 >= floor && $0 < start + guideSpan }
    }

    /// The programmes of `guides` by channel, each list by start, one programme per start.
    public static func merge(_ guides: [LiveGuide]) -> [Int: [LiveProgram]] {
        var out: [Int: [Int64: LiveProgram]] = [:]
        for guide in guides {
            for (channel, programs) in guide.programs {
                for program in programs where out[channel]?[program.start] == nil {
                    out[channel, default: [:]][program.start] = program
                }
            }
        }
        return out.mapValues { $0.values.sorted { $0.start < $1.start } }
    }

    /// The programmes that show between `from` and `to` (unix seconds).
    public static func between(_ programs: [LiveProgram]?, from: Int64, to: Int64) -> [LiveProgram] {
        (programs ?? []).filter { $0.stop > from && $0.start < to }
    }

    /// A programme's rest at `now`, as hours and minutes.
    public static func left(_ program: LiveProgram, now: Int64) -> (hours: Int, minutes: Int) {
        let minutes = max(1, Int((Double(program.stop - now) / 60).rounded()))
        return (minutes / 60, minutes % 60)
    }

    /// "1 h 5 min left", "2 h left", "25 min left".
    public static func leftText(_ program: LiveProgram, now: Int64) -> String {
        let (hours, minutes) = left(program, now: now)
        if hours == 0 { return localized("\(minutes) min left") }
        return minutes == 0 ? localized("\(hours) h left") : localized("\(hours) h \(minutes) min left")
    }

    /// How many days `seconds`'s date is from `now`'s (unix seconds) in
    /// `timeZone`: 1 tomorrow, -1 yesterday. Dates, not 24-hour steps: a
    /// day is 23 or 25 hours when daylight saving starts or ends.
    public static func daysApart(_ seconds: Int64, now: Int64, timeZone: TimeZone = .current) -> Int {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let a = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(now)))
        let b = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(seconds)))
        return calendar.dateComponents([.day], from: a, to: b).day ?? 0
    }

    /// "6:00 PM" in the style of the language shown.
    public static func time(_ seconds: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(seconds))
            .formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: ShownLanguage.locale))
    }

    /// "6:00 – 6:30 PM", with "Tomorrow" or "Yesterday" before a time on another day.
    public static func span(_ program: LiveProgram, now: Int64) -> String {
        let text = "\(time(program.start)) – \(time(program.stop))"
        switch daysApart(program.start, now: now) {
        case 0: return text
        case 1: return localized("Tomorrow, \(text)")
        case -1: return localized("Yesterday, \(text)")
        default:
            let day = Date(timeIntervalSince1970: TimeInterval(program.start))
                .formatted(.dateTime.weekday(.wide).locale(ShownLanguage.locale))
            return localized("\(day), \(text)")
        }
    }

    /// What VoiceOver reads for a programme of the guide: "BBC One, 101,
    /// News at Six, 6:00 to 6:30 PM, on now".
    public static func spoken(_ program: LiveProgram, channel: LiveChannel, now: Int64) -> String {
        var parts = [channel.name]
        if let number = channel.number { parts.append(number) }
        parts.append(program.title.isEmpty ? localized("No information for this time") : program.title)
        let start = time(program.start), stop = time(program.stop)
        parts.append(localized("\(start) to \(stop)"))
        if program.airing(at: now) { parts.append(localized("on now")) }
        return parts.joined(separator: ", ")
    }

    private static func floorDiv(_ a: Int64, _ b: Int64) -> Int64 {
        let q = a / b
        return (a % b != 0 && (a < 0) != (b < 0)) ? q - 1 : q
    }
}

/// Where each programme of the guide goes (pure): from its start and stop,
/// at `pointsPerMinute` from the guide's `start`. The scale grows with the
/// text size, so a half hour's title still fits.
public struct GuideLayout: Equatable, Sendable {
    /// The guide's left edge, unix seconds.
    public let start: Int64
    public let pointsPerMinute: Double

    public init(start: Int64, pointsPerMinute: Double) {
        self.start = start
        self.pointsPerMinute = pointsPerMinute
    }

    /// The scale for a text size: `scale` is how much larger the body text is than at the default size.
    /// Apple TV's two hours fit beside the categories and the channel column on a 1920-point screen.
    public static func pointsPerMinute(textScale scale: Double, tv: Bool = false) -> Double {
        (tv ? 8 : 6) * min(max(scale, 1), 2.5)
    }

    /// The guide's whole width.
    public var width: Double { Double(LiveTvRules.guideSpan) / 60 * pointsPerMinute }

    /// Where `time` is, in points from the left edge.
    public func x(_ time: Int64) -> Double { Double(time - start) / 60 * pointsPerMinute }

    /// The time at `x` points from the left edge.
    public func time(atX x: Double) -> Int64 { start + Int64(x / pointsPerMinute * 60) }

    /// Where `program` goes: its x and width, clipped to the guide's span, at least a sliver wide.
    public func frame(_ program: LiveProgram) -> (x: Double, width: Double) {
        let from = max(program.start, start)
        let to = min(program.stop, start + LiveTvRules.guideSpan)
        return (x(from), max(x(to) - x(from), 2))
    }

    /// The half hours the time header marks.
    public var slots: [Int64] {
        Array(stride(from: start, to: start + LiveTvRules.guideSpan, by: Int(LiveTvRules.slot)))
    }
}
