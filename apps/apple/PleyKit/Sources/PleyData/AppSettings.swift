import CoreText
import Foundation

/// This device's own settings, kept in `UserDefaults` under `pley.settings.*`
/// (the quality under the `pley.quality.*` keys the player has always
/// read): nothing here goes to the server, and nothing is a secret. Each
/// change is kept at once. A profile's own choices (its track defaults)
/// live on the server instead.
public final class AppSettings: @unchecked Sendable {
    // Unchecked Sendable: UserDefaults is thread-safe.

    /// The key prefix every setting shares.
    public static let prefix = "pley.settings."

    /// How many recent searches a profile keeps.
    public static let recentSearches = 10

    private let defaults: UserDefaults

    /// Settings in `defaults` (the app's standard defaults by default).
    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// The section shown last (the tab or the sidebar's row), as the
    /// screens name it; `nil` before one was chosen.
    public var lastSection: String? {
        get { defaults.string(forKey: Self.prefix + "section") }
        set { set(newValue, for: "section") }
    }

    // MARK: Playback

    /// Where the quality for a kind of network is kept: the player reads it
    /// (`DefaultsQuality`) and its menu writes it, as Settings does.
    public static func qualityKey(cellular: Bool) -> String {
        cellular ? "pley.quality.cellular" : "pley.quality.other"
    }

    /// The quality chosen on mobile data (`cellular`) or on every other
    /// network: a preset's id ("8m", "original"); `nil` is Automatic.
    public func quality(cellular: Bool) -> String? {
        let value = defaults.string(forKey: Self.qualityKey(cellular: cellular))
        return value == "auto" || value?.isEmpty == true ? nil : value
    }

    /// Sets the quality; `nil` (Automatic) removes it, as nothing remembered means Automatic.
    public func setQuality(_ id: String?, cellular: Bool) {
        let key = Self.qualityKey(cellular: cellular)
        if let id, id != "auto" {
            defaults.set(id, forKey: key)
        } else {
            defaults.removeObject(forKey: key)
        }
    }

    /// Whether the next episode plays by itself after the Up Next
    /// countdown; off, the card shows with no countdown. On by default.
    public var autoplayNext: Bool {
        get { bool("autoplayNext", or: true) }
        set { defaults.set(newValue, forKey: Self.prefix + "autoplayNext") }
    }

    // MARK: Subtitles

    /// The subtitle look chosen over the system's caption style.
    public var subtitles: SubtitleOverrides {
        get {
            SubtitleOverrides(
                size: SubtitleSize(rawValue: defaults.string(forKey: Self.prefix + "subtitleSize") ?? "") ?? .system,
                background: SubtitleBackground(rawValue: defaults.string(forKey: Self.prefix + "subtitleBackground")
                    ?? "") ?? .system,
                font: SubtitleFont(rawValue: defaults.string(forKey: Self.prefix + "subtitleFont") ?? "") ?? .system,
                raise: SubtitleRaise(rawValue: defaults.integer(forKey: Self.prefix + "subtitleRaise")) ?? .bottom)
        }
        set {
            set(newValue.size == .system ? nil : newValue.size.rawValue, for: "subtitleSize")
            set(newValue.background == .system ? nil : newValue.background.rawValue, for: "subtitleBackground")
            set(newValue.font == .system ? nil : newValue.font.rawValue, for: "subtitleFont")
            if newValue.raise == .bottom {
                defaults.removeObject(forKey: Self.prefix + "subtitleRaise")
            } else {
                defaults.set(newValue.raise.rawValue, forKey: Self.prefix + "subtitleRaise")
            }
        }
    }

    // MARK: Browsing

    /// How big a grid's tiles are.
    public var gridDensity: GridDensity {
        get { GridDensity(rawValue: defaults.string(forKey: Self.gridDensityKey) ?? "") ?? .medium }
        set { defaults.set(newValue.rawValue, forKey: Self.gridDensityKey) }
    }

    /// Where the grid size is kept, for views that follow it.
    public static let gridDensityKey = prefix + "gridDensity"

    // MARK: Signing in

    /// Sign in from a private browser session, which keeps no cookie behind
    /// (iPhone, iPad and Mac). On by default.
    public var privateSignIn: Bool {
        get { bool("privateSignIn", or: true) }
        set { defaults.set(newValue, forKey: Self.prefix + "privateSignIn") }
    }

    // MARK: Profiles

    /// Apple TV's "Always start as": the profile the app starts as on the
    /// server at `origin`, without asking "Who's watching?"; `nil`: ask.
    public func alwaysStartAs(origin: String) -> Int? {
        (defaults.dictionary(forKey: Self.prefix + "alwaysStartAs") as? [String: Int])?[origin]
    }

    public func setAlwaysStartAs(_ profileId: Int?, origin: String) {
        var all = defaults.dictionary(forKey: Self.prefix + "alwaysStartAs") as? [String: Int] ?? [:]
        all[origin] = profileId
        defaults.set(all, forKey: Self.prefix + "alwaysStartAs")
    }

    /// The profile used last on the server at `origin`: the app starts as
    /// it, from what it saved, when the server doesn't answer.
    public func lastProfile(origin: String) -> Int? {
        (defaults.dictionary(forKey: Self.prefix + "lastProfile") as? [String: Int])?[origin]
    }

    public func setLastProfile(_ profileId: Int?, origin: String) {
        var all = defaults.dictionary(forKey: Self.prefix + "lastProfile") as? [String: Int] ?? [:]
        all[origin] = profileId
        defaults.set(all, forKey: Self.prefix + "lastProfile")
    }

    /// The household of the server at `origin` as last seen: its profiles'
    /// ids and names, so the start needn't wait for the network.
    public func household(origin: String) -> [(id: Int, name: String)] {
        let all = defaults.dictionary(forKey: Self.prefix + "households") as? [String: [[String: Any]]]
        return (all?[origin] ?? []).compactMap { entry in
            guard let id = entry["id"] as? Int, let name = entry["name"] as? String else { return nil }
            return (id, name)
        }
    }

    public func setHousehold(_ profiles: [(id: Int, name: String)], origin: String) {
        var all = defaults.dictionary(forKey: Self.prefix + "households") as? [String: [[String: Any]]] ?? [:]
        all[origin] = profiles.map { ["id": $0.id, "name": $0.name] }
        defaults.set(all, forKey: Self.prefix + "households")
    }

    // MARK: Search

    /// The profile's last searches on the server at `origin`, newest first.
    public func recentSearches(origin: String, profileId: Int) -> [String] {
        let all = defaults.dictionary(forKey: Self.prefix + "recentSearches") as? [String: [String]]
        return all?[Self.viewKey(origin, profileId)] ?? []
    }

    /// Puts `query` first among the profile's recent searches, once, keeping the last 10.
    public func rememberSearch(_ query: String, origin: String, profileId: Int) {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return }
        var recent = recentSearches(origin: origin, profileId: profileId)
        recent.removeAll { $0.caseInsensitiveCompare(query) == .orderedSame }
        recent.insert(String(query.prefix(200)), at: 0)
        setRecent(Array(recent.prefix(Self.recentSearches)), origin: origin, profileId: profileId)
    }

    public func clearRecentSearches(origin: String, profileId: Int) {
        setRecent([], origin: origin, profileId: profileId)
    }

    private func setRecent(_ recent: [String], origin: String, profileId: Int) {
        var all = defaults.dictionary(forKey: Self.prefix + "recentSearches") as? [String: [String]] ?? [:]
        all[Self.viewKey(origin, profileId)] = recent.isEmpty ? nil : recent
        defaults.set(all, forKey: Self.prefix + "recentSearches")
    }

    // MARK: Libraries

    /// How a library was left on this device (its tab, sort and filters,
    /// as `LibraryParams` writes them), by server, profile and library.
    public func libraryView(origin: String, profileId: Int, libraryId: Int) -> String? {
        let views = defaults.dictionary(forKey: Self.prefix + "libraryViews") as? [String: String]
        return views?[Self.viewKey(origin, profileId, libraryId)]
    }

    public func setLibraryView(_ view: String, origin: String, profileId: Int, libraryId: Int) {
        var views = defaults.dictionary(forKey: Self.prefix + "libraryViews") as? [String: String] ?? [:]
        views[Self.viewKey(origin, profileId, libraryId)] = view
        // A bound: the oldest kept views don't matter much.
        if views.count > 500 { views = Dictionary(uniqueKeysWithValues: views.suffix(400).map { ($0.key, $0.value) }) }
        defaults.set(views, forKey: Self.prefix + "libraryViews")
    }

    // MARK: Downloads

    /// Settings → Downloads → Quality: the original, or a smaller file the server makes.
    public var downloadQuality: DownloadQuality {
        get { DownloadQuality(rawValue: defaults.string(forKey: Self.prefix + "downloads.quality") ?? "") ?? .original }
        set { set(newValue == .original ? nil : newValue.rawValue, for: "downloads.quality") }
    }

    /// Settings → Downloads → "Download on mobile data"; off, downloads wait for Wi-Fi.
    public var downloadOnMobileData: Bool {
        get { bool("downloads.mobileData", or: false) }
        set { defaults.set(newValue, forKey: Self.prefix + "downloads.mobileData") }
    }

    // MARK: Live TV

    /// How Live TV was last left on this device by a profile, as Android's
    /// LiveTvViews keeps it: the category (`all`, `favorites`, `recent`,
    /// `group:UK| Sport`), the view (`guide` or `channels`) and the channel
    /// last watched, under `pley.settings.live.<origin>.<profile>.*`.
    public struct LiveView: Equatable, Sendable {
        public var category: String?
        public var view: String?
        public var channel: Int?

        public init(category: String? = nil, view: String? = nil, channel: Int? = nil) {
            self.category = category
            self.view = view
            self.channel = channel
        }
    }

    public func liveView(origin: String, profileId: Int) -> LiveView {
        let base = Self.liveKey(origin, profileId)
        return LiveView(category: defaults.string(forKey: base + "category"), view: defaults.string(forKey: base + "view"),
                        channel: defaults.object(forKey: base + "channel") as? Int)
    }

    /// Keeps what isn't `nil` of `category`, `view` and `channel`, the rest as it was.
    public func keepLiveView(origin: String, profileId: Int, category: String? = nil, view: String? = nil,
                             channel: Int? = nil) {
        let base = Self.liveKey(origin, profileId)
        if let category { defaults.set(category, forKey: base + "category") }
        if let view { defaults.set(view, forKey: base + "view") }
        if let channel { defaults.set(channel, forKey: base + "channel") }
    }

    private static func liveKey(_ origin: String, _ profileId: Int) -> String {
        prefix + "live.\(origin).\(profileId)."
    }

    private static func viewKey(_ origin: String, _ profileId: Int, _ libraryId: Int) -> String {
        "\(origin) \(profileId) \(libraryId)"
    }

    private static func viewKey(_ origin: String, _ profileId: Int) -> String {
        "\(origin) \(profileId)"
    }

    private func bool(_ key: String, or fallback: Bool) -> Bool {
        defaults.object(forKey: Self.prefix + key) as? Bool ?? fallback
    }

    private func set(_ value: String?, for key: String) {
        if let value {
            defaults.set(value, forKey: Self.prefix + key)
        } else {
            defaults.removeObject(forKey: Self.prefix + key)
        }
    }
}

/// How big a grid's tiles are: more small ones, or fewer large ones.
public enum GridDensity: String, CaseIterable, Sendable {
    case small, medium, large

    public var label: String {
        switch self {
        case .small: "Small"
        case .medium: "Medium"
        case .large: "Large"
        }
    }

    /// One size larger (`by: 1`, ⌘+) or smaller (`by: -1`, ⌘−); `nil` past the largest or smallest.
    public func stepped(by step: Int) -> GridDensity? {
        let all = Self.allCases
        guard let index = all.firstIndex(of: self), all.indices.contains(index + step) else { return nil }
        return all[index + step]
    }

    /// The step a key pressed asks for: ⌘= and ⌘+ (⇧⌘= on most
    /// keyboards, whose `+` the menu's ⌘= doesn't take) one size larger,
    /// ⌘− one smaller; `nil` for any other key, or with ⌃ or ⌥.
    /// `characters` are the key's without modifiers but Shift.
    public static func keyStep(_ characters: String?, command: Bool, option: Bool = false,
                               control: Bool = false) -> Int? {
        guard command, !option, !control else { return nil }
        switch characters {
        case "=", "+": return 1
        case "-", "\u{2212}": return -1
        default: return nil
        }
    }

    /// A tile's width against the medium one's.
    public var scale: Double {
        switch self {
        case .small: 0.8
        case .medium: 1
        case .large: 1.25
        }
    }
}

// MARK: Subtitle style

/// What the person chose over the system's caption style; `system` for
/// each leaves it to that style (Settings → Accessibility → Subtitles &
/// Captioning), which leaves it to Pley's own look.
public struct SubtitleOverrides: Hashable, Sendable {
    public var size: SubtitleSize
    public var background: SubtitleBackground
    public var font: SubtitleFont
    /// How far text subtitles sit above where they would.
    public var raise: SubtitleRaise

    public init(size: SubtitleSize = .system, background: SubtitleBackground = .system, font: SubtitleFont = .system,
                raise: SubtitleRaise = .bottom) {
        self.size = size
        self.background = background
        self.font = font
        self.raise = raise
    }
}

/// A text size against Pley's own (4.8 % of the video's height).
public enum SubtitleSize: String, CaseIterable, Sendable {
    case system, p75, p100, p125, p150, p200

    /// The size against Pley's; `nil` for the system's.
    public var scale: Double? {
        switch self {
        case .system: nil
        case .p75: 0.75
        case .p100: 1
        case .p125: 1.25
        case .p150: 1.5
        case .p200: 2
        }
    }

    public var label: String {
        guard let scale else { return "System" }
        return "\(Int((scale * 100).rounded())) %"
    }
}

public enum SubtitleBackground: String, CaseIterable, Sendable {
    case system, none, darkBox

    public var label: String {
        switch self {
        case .system: "System"
        case .none: "None"
        case .darkBox: "Dark box"
        }
    }
}

public enum SubtitleFont: String, CaseIterable, Sendable {
    case system, sansSerif, serif, monospace, casual

    public var label: String {
        switch self {
        case .system: "System"
        case .sansSerif: "Sans serif"
        case .serif: "Serif"
        case .monospace: "Monospace"
        case .casual: "Casual"
        }
    }

    /// The casual hand, where the platform has it (Noteworthy).
    public static let casualName = "Noteworthy-Bold"

    /// Whether this device has the casual hand: Core Text gives another
    /// font when it hasn't.
    public static let casualAvailable: Bool = {
        let font = CTFontCreateWithName(casualName as CFString, 12, nil)
        return (CTFontCopyPostScriptName(font) as String) == casualName
    }()

    /// The fonts this device offers, in order.
    public static var offered: [SubtitleFont] {
        allCases.filter { $0 != .casual || casualAvailable }
    }
}

/// How far text subtitles are raised, as a share of the video's height.
public enum SubtitleRaise: Int, CaseIterable, Sendable {
    case bottom = 0, five = 5, ten = 10, fifteen = 15

    public var share: Double { Double(rawValue) / 100 }

    public var label: String {
        self == .bottom ? "Bottom" : "Raised \(rawValue) %"
    }
}
