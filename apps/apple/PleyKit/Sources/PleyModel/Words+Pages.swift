import Foundation

/// The words of item, person and music pages, as Android says them
/// (`model/format`: Labels.kt, Media.kt, Durations.kt, Times.kt).
extension Words {
    /// A season's own title, else "Specials" for season 0, else "Season 2".
    public static func seasonTitle(_ title: String?, index: Int?) -> String {
        if let title, !title.isEmpty { return title }
        if index == 0 { return localized("Specials") }
        if let index { return localized("Season \(index)") }
        return localized("Season")
    }

    /// "Season 2 · Episode 5": the parts that are known.
    public static func episodeLongLabel(season: Int?, episode: Int?) -> String {
        [season.map { $0 == 0 ? localized("Specials") : localized("Season \($0)") },
         episode.map { localized("Episode \($0)") }]
            .compactMap { $0 }.joined(separator: " · ")
    }

    /// 6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s"; empty without a duration.
    public static func duration(ms: Int?) -> String {
        guard let ms, ms > 0 else { return "" }
        let minutes = Int((Double(ms) / 60_000).rounded(.toNearestOrAwayFromZero))
        if minutes < 1 { return seconds(max(1, Int((Double(ms) / 1000).rounded()))) }
        if minutes < 60 { return localized("\(minutes)m") }
        return hoursMinutes(minutes)
    }

    /// "2016-11-11" → "Nov 11, 2016" in `locale`'s medium style (the
    /// locale shown by default); what doesn't parse comes back as it is.
    public static func date(_ date: String?, locale: Locale = ShownLanguage.locale) -> String {
        formatted(date, style: .medium, locale: locale)
    }

    /// "2024-03-12" → "March 12, 2024" (en-US) or "12 March 2024" (en-GB): air dates.
    public static func longDate(_ date: String?, locale: Locale = ShownLanguage.locale) -> String {
        formatted(date, style: .long, locale: locale)
    }

    private static func formatted(_ date: String?, style: DateFormatter.Style, locale: Locale) -> String {
        guard let date, !date.isEmpty else { return "" }
        let utc = TimeZone(identifier: "UTC")!
        let parser = DateFormatter()
        parser.locale = Locale(identifier: "en_US_POSIX")
        parser.timeZone = utc
        parser.dateFormat = "yyyy-MM-dd"
        guard let day = parser.date(from: date) else { return date }
        let out = DateFormatter()
        out.locale = locale
        out.timeZone = utc
        out.dateStyle = style
        out.timeStyle = .none
        return out.string(from: day)
    }

    /// The critics' rating, "7.6" ("7,6" in French).
    public static func rating(_ value: Double) -> String {
        fixed(value, digits: 1)
    }

    /// "4K", "1080p", "720p" or "SD", for badges; `nil` without a size.
    public static func resolutionBadge(width: Int?, height: Int?) -> String? {
        let w = width ?? 0, h = height ?? 0
        if w >= 3800 || h >= 2100 { return "4K" }
        if w >= 1800 || h >= 1000 { return "1080p" }
        if w >= 1200 || h >= 700 { return "720p" }
        return h > 0 ? "SD" : nil
    }

    /// A video's dynamic range badges: "DV P8.1", and "HDR10+" too for Dolby Vision with HDR10+.
    public static func dynamicRangeBadges(_ stream: MediaStream?) -> [String] {
        guard var plain = stream else { return [] }
        plain.hdr10plus = false
        guard let badge = dynamicRange(plain)?.replacingOccurrences(of: "Dolby Vision", with: "DV") else { return [] }
        let hybrid = stream?.dynamicRange == .dolbyVision && stream?.hdr10plus == true
        return hybrid ? [badge, "HDR10+"] : [badge]
    }

    /// A file's short quality badges, in the web's order: resolution, dynamic
    /// range, then "Atmos", else "7.1" or "5.1".
    public static func qualityBadges(_ file: MediaFile?) -> [String] {
        guard let file else { return [] }
        let video = file.streams(.video).first
        let audio = file.streams(.audio).first(where: \.default) ?? file.streams(.audio).first
        let channels = audio?.channels ?? 0
        let sound: String? = if isAtmos(audio) {
            "Atmos"
        } else if channels >= 8 {
            "7.1"
        } else if channels >= 6 {
            "5.1"
        } else {
            nil
        }
        return [resolutionBadge(width: file.width ?? video?.width, height: file.height ?? video?.height)]
            .compactMap { $0 } + dynamicRangeBadges(video) + [sound].compactMap { $0 }
    }

    /// A version in an item's Details: "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 29.7 GB".
    public static func versionDescription(_ file: MediaFile) -> String {
        let video = file.streams(.video).first
        return [file.edition ?? "",
                resolution(width: file.width ?? video?.width, height: file.height ?? video?.height) ?? "",
                codec(file.videoCodec ?? video?.codec), dynamicRange(video) ?? "",
                startingAudio(file).map(audioFormat) ?? "", file.size > 0 ? bytes(file.size) : ""]
            .filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The last part of a path, whichever separator it uses: apps get file names only.
    public static func fileName(_ path: String) -> String {
        String(path.split(separator: "/", omittingEmptySubsequences: false).last ?? "")
            .split(separator: "\\", omittingEmptySubsequences: false).last.map(String.init) ?? path
    }

    /// "as Neo", "Director", "Guest star (3 episodes)", as the web says a person's parts.
    public static func roleText(_ roles: [PersonRole]) -> String {
        roles.map { r in
            let episodes = r.episodes ?? 0
            let base: String
            if r.role == .actor {
                if let character = r.character, !character.isEmpty {
                    base = localized("as \(character)")
                } else {
                    base = episodes > 0 ? localized("Guest star") : localized("Actor")
                }
            } else {
                base = roleName(r.role)
            }
            return episodes > 0 ? "\(base) (\(localized("\(episodes) episodes")))" : base
        }.joined(separator: ", ")
    }

    /// "Director", "Writer"…; "Crew" for a role this app doesn't know.
    public static func roleName(_ role: CreditRole) -> String {
        switch role {
        case .actor: localized("Actor")
        case .creator: localized("Creator")
        case .director: localized("Director")
        case .writer: localized("Writer")
        case .producer: localized("Producer")
        default: localized("Crew")
        }
    }

    /// A crew line's label: "Director", "Directors", "Created by"…
    public static func crewLabel(_ role: CreditRole, plural: Bool) -> String {
        switch role {
        case .creator: localized("Created by")
        case .director: plural ? localized("Directors") : localized("Director")
        case .writer: plural ? localized("Writers") : localized("Writer")
        case .producer: plural ? localized("Producers") : localized("Producer")
        default: localized("Crew")
        }
    }

    /// The kind of an extra: "Trailer · 2m", as the web's Extras row says it.
    public static func extraSubtitle(_ item: ItemSummary) -> String {
        let kind = switch item.extraType ?? "" {
        case "trailer": localized("Trailer")
        case "behind_the_scenes": localized("Behind the Scenes")
        case "deleted_scene": localized("Deleted Scene")
        case "featurette": localized("Featurette")
        case "interview": localized("Interview")
        case "scene": localized("Scene")
        case "short": localized("Short")
        default: localized("Extra")
        }
        return [kind, duration(ms: item.durationMs)].filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
