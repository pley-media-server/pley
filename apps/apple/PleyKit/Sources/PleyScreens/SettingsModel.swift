import Combine
import Foundation
import PleyCaps
import PleyData
import PleyModel
import PleyPlayer

/// One choice of a setting: its value and what it says.
public struct SettingChoice<Value: Hashable & Sendable>: Hashable, Sendable, Identifiable {
    public var id: Value { value }
    public let value: Value
    public let label: String

    public init(_ value: Value, _ label: String) {
        self.value = value
        self.label = label
    }
}

/// Settings: this device's own choices (`AppSettings`), each kept at once.
/// Playback: the quality (on Wi-Fi and on mobile data where the device has
/// it), and whether the next episode plays by itself. Subtitles: size,
/// background, font and position over the system's caption style.
/// Browsing: the grid size and the libraries' order. Signing in privately
/// (iPhone, iPad and Mac) and, on Apple TV, who to always start as.
@MainActor
public final class SettingsModel: ObservableObject {
    /// Under the subtitle settings: what System means.
    public static let systemStyleNote =
        "System uses your caption style from Settings → Accessibility → Subtitles & Captioning."
    /// The preview line.
    public static let previewText = "This is how subtitles look."
    /// Under Sign in privately.
    public static let privateSignInNote =
        "Signing in in a private window keeps no Pley cookie on this device, so you type your password each time."

    @Published public private(set) var qualityOther: String?
    @Published public private(set) var qualityCellular: String?
    @Published public private(set) var autoplayNext: Bool
    @Published public private(set) var subtitles: SubtitleOverrides
    @Published public private(set) var gridDensity: GridDensity
    @Published public private(set) var privateSignIn: Bool
    /// "Always start as": a profile's id, `nil` to ask.
    @Published public private(set) var alwaysStartAs: Int?

    public let app: AppModel
    /// Whether the quality on mobile data is offered: iPhone, and an iPad with cellular.
    public let offersCellular: Bool
    private var settings: AppSettings { app.settings }

    public init(app: AppModel, offersCellular: Bool) {
        self.app = app
        self.offersCellular = offersCellular
        let settings = app.settings
        qualityOther = settings.quality(cellular: false)
        qualityCellular = settings.quality(cellular: true)
        autoplayNext = settings.autoplayNext
        subtitles = settings.subtitles
        gridDensity = settings.gridDensity
        privateSignIn = settings.privateSignIn
        alwaysStartAs = app.server.flatMap { settings.alwaysStartAs(origin: $0.origin) }
    }

    // MARK: Playback

    /// The qualities offered: Automatic (`nil`), Original, then the transcodes.
    public static let qualities: [SettingChoice<String?>] =
        [SettingChoice<String?>(nil, "Automatic"), SettingChoice<String?>(QualityPreset.originalId, "Original")]
        + Qualities.transcodes.map { SettingChoice<String?>($0.id, Qualities.label($0)) }

    /// A quality's name: "Automatic", "Original", "4 Mbps 720p".
    public static func qualityLabel(_ id: String?) -> String {
        qualities.first { $0.value == id }?.label ?? "Automatic"
    }

    /// The quality title of a row: "Quality" alone, or "on Wi-Fi" and "on mobile data".
    public func qualityTitle(cellular: Bool) -> String {
        guard offersCellular else { return "Quality" }
        return cellular ? "Quality on mobile data" : "Quality on Wi-Fi"
    }

    public func setQuality(_ id: String?, cellular: Bool) {
        settings.setQuality(id, cellular: cellular)
        qualityOther = settings.quality(cellular: false)
        qualityCellular = settings.quality(cellular: true)
    }

    public func setAutoplayNext(_ on: Bool) {
        settings.autoplayNext = on
        autoplayNext = on
    }

    // MARK: Subtitles

    public static let sizes = SubtitleSize.allCases.map { SettingChoice($0, $0.label) }
    public static let backgrounds = SubtitleBackground.allCases.map { SettingChoice($0, $0.label) }
    public static var fonts: [SettingChoice<SubtitleFont>] { SubtitleFont.offered.map { SettingChoice($0, $0.label) } }
    public static let raises = SubtitleRaise.allCases.map { SettingChoice($0, $0.label) }

    public func setSubtitleSize(_ size: SubtitleSize) { changeSubtitles { $0.size = size } }
    public func setSubtitleBackground(_ background: SubtitleBackground) { changeSubtitles { $0.background = background } }
    public func setSubtitleFont(_ font: SubtitleFont) { changeSubtitles { $0.font = font } }
    public func setSubtitleRaise(_ raise: SubtitleRaise) { changeSubtitles { $0.raise = raise } }

    private func changeSubtitles(_ change: (inout SubtitleOverrides) -> Void) {
        var next = subtitles
        change(&next)
        settings.subtitles = next
        subtitles = next
    }

    // MARK: Browsing

    public static let densities = GridDensity.allCases.map { SettingChoice($0, $0.label) }

    public func setGridDensity(_ density: GridDensity) {
        settings.gridDensity = density
        gridDensity = density
    }

    /// The libraries back in the server's order, for this profile.
    public func useServerOrder() async {
        guard let api = await app.session.api else { return }
        do {
            let libraries = try await api.resetLibraryOrder()
            app.shell.setLibraries(libraries)
            app.note(ServerChange(home: true, libraries: true))
            app.shell.show("Libraries are in the server's order")
        } catch {
            app.shell.show(asSentence("Couldn't change the order: \(describe(error))"))
        }
    }

    // MARK: Signing in

    public func setPrivateSignIn(_ on: Bool) {
        settings.privateSignIn = on
        privateSignIn = on
    }

    // MARK: Apple TV's start

    /// "Always start as"'s choices: Ask, then the household as last seen.
    public var startChoices: [SettingChoice<Int?>] {
        let household = app.server.map { settings.household(origin: $0.origin) } ?? []
        return [SettingChoice(nil, "Ask who's watching")] + household.map { SettingChoice($0.id, $0.name) }
    }

    /// Whether "Always start as" is offered: a household of several profiles.
    public var offersStartAs: Bool { startChoices.count > 2 }

    public var startAsLabel: String {
        startChoices.first { $0.value == alwaysStartAs }?.label ?? "Ask who's watching"
    }

    public func setAlwaysStartAs(_ profileId: Int?) {
        guard let origin = app.server?.origin else { return }
        settings.setAlwaysStartAs(profileId, origin: origin)
        alwaysStartAs = profileId
    }

    /// Reads the household again, for "Always start as".
    public func load() async {
        guard app.user?.scopes.contains("profiles") == true, let switcher = await app.session.profiles,
              let household = try? await switcher.profiles()
        else { return }
        app.noteHousehold(household)
        objectWillChange.send()
    }
}

/// Diagnostics: the device's facts and the caps report the app sends with
/// each playback request, as indented JSON.
@MainActor
public final class DiagnosticsModel: ObservableObject {
    @Published public private(set) var snapshot: CapsSnapshot?
    /// The report, as sent, indented.
    @Published public private(set) var json = ""

    public let app: AppModel

    public init(app: AppModel) {
        self.app = app
        if let snapshot = app.caps.snapshot { show(snapshot) }
    }

    /// The device class this device was matched to: "a17_m3".
    public var deviceClass: String { snapshot?.deviceClass ?? "" }
    /// What the device plays, a line each.
    public var lines: [String] { snapshot?.lines ?? [] }

    /// Reads the device once more when nothing was read yet.
    public func load() async {
        show(await app.caps.warmUp())
    }

    private func show(_ snapshot: CapsSnapshot) {
        self.snapshot = snapshot
        json = Self.indented(snapshot.report)
    }

    /// `report` as the server reads it, indented, keys sorted.
    public nonisolated static func indented(_ report: ClientCaps) -> String {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(report) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }
}

/// About: the version, that the app holds no third-party code, and Pley's licence.
public struct AboutModel: Sendable {
    public let version: String
    public let build: String
    public let device: String

    public init(info: [String: Any] = Bundle.main.infoDictionary ?? [:], device: String = AboutModel.thisDevice) {
        version = info["CFBundleShortVersionString"] as? String ?? "0.0.0"
        build = info["CFBundleVersion"] as? String ?? "0"
        self.device = device
    }

    /// "for iPhone and iPad", "for Mac", "for Apple TV".
    public static var thisDevice: String {
        #if os(tvOS)
        "Apple TV"
        #elseif os(macOS)
        "Mac"
        #else
        "iPhone and iPad"
        #endif
    }

    /// "Pley 0.1.0 (build 12) for iPhone and iPad".
    public var title: String { "Pley \(version) (build \(build)) for \(device)" }

    public static let licences =
        "Open-source licences: none. Pley for Apple is built only with Apple's frameworks and contains no third-party code."
    public static let licence = "Pley is licensed under the terms in its LICENSE file."
}
