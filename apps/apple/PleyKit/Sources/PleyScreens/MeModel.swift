import Combine
import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Me (Apple TV: the Settings tab): who is signed in, on which server,
/// Switch profile, the profile's audio and subtitle defaults (kept on the
/// server, so every app starts titles the same way), History, Settings,
/// the servers and Sign out.
@MainActor
public final class MeModel: ObservableObject {
    /// Says where a profile's name, picture and PIN are changed.
    public static let webNote = "A profile's name, picture and PIN are changed in Pley on the web."
    /// Under track defaults that can't be changed here.
    public static let readOnlyNote = "This sign-in can't change them: change them in Pley on the web."

    /// Whether Switch profile shows: the app may switch (`profiles`) and
    /// the household has another profile.
    @Published public private(set) var canSwitch = false
    /// The profile's track defaults, as shown: a change shows at once.
    @Published public private(set) var trackDefaults = TrackDefaults()
    /// Whether this sign-in may change them (`play`).
    @Published public private(set) var editable = false
    /// A change on its way to the server.
    @Published public private(set) var saving = false

    public let app: AppModel
    private var watching: AnyCancellable?

    public init(app: AppModel) {
        self.app = app
        follow(app.user)
        watching = app.$user.dropFirst().sink { [weak self] user in self?.follow(user) }
    }

    /// The name shown: the profile's display name, else its username.
    public var name: String {
        guard let user = app.user else { return "" }
        return user.displayName.flatMap { $0.isEmpty ? nil : $0 } ?? user.name
    }

    /// "Managed profile" for a profile without a password of its own; `nil` otherwise.
    public var kind: String? {
        app.user?.managed == true ? "Managed profile" : nil
    }

    /// The server's name, and its address as people type it.
    public var serverName: String { app.server?.name ?? "" }
    public var serverAddress: String { app.server.map { Servers.displayAddress($0.origin) } ?? "" }

    /// Asks the household, for Switch profile.
    public func load() async {
        guard app.user?.scopes.contains("profiles") == true, let switcher = await app.session.profiles else {
            canSwitch = false
            return
        }
        do {
            let household = try await switcher.profiles()
            app.noteHousehold(household)
            canSwitch = household.count > 1
        } catch is CancellationError {
        } catch {
            // Without an answer the switch stays offered: "Who's watching?" says why it can't.
            canSwitch = true
        }
    }

    public func switchProfile() {
        app.showProfiles()
    }

    public func showServers() {
        app.showServers()
    }

    public func signOut() async {
        await app.signOut()
    }

    // MARK: Track defaults

    /// The languages offered, as the web offers them: ISO 639-2/B codes
    /// (the form ffprobe reports), by their English names.
    public static let languages: [(code: String, name: String)] = [
        "ara", "bul", "cat", "chi", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hrv", "hun",
        "ind", "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "slo", "spa", "srp", "swe", "tha", "tur",
        "ukr", "vie",
    ].map { ($0, Words.language($0) ?? $0) }.sorted { $0.name < $1.name }

    /// The modes, in the web's order.
    public static let modes: [SubtitleMode] = [.off, .forced, .foreign, .always]

    /// What the audio language row says: "English", or "The file's default".
    public var audioLabel: String { Self.languageName(trackDefaults.audioLanguage) ?? "The file's default" }

    /// What the subtitle language row says: "English", or "Same as the audio language".
    public var subtitleLanguageLabel: String {
        Self.languageName(trackDefaults.subtitleLanguage) ?? "Same as the audio language"
    }

    /// Whether the subtitle language is asked for: only modes that show subtitles of their own language.
    public var showsSubtitleLanguage: Bool { trackDefaults.subtitleMode.readsLanguage }

    /// The web's name of a mode.
    public static func modeLabel(_ mode: SubtitleMode) -> String {
        switch mode {
        case .off: "Off"
        case .forced: "Only for foreign dialogue"
        case .foreign: "When the audio is in another language"
        case .always: "Always"
        }
    }

    /// What a mode does, under the row.
    public static func modeHint(_ mode: SubtitleMode) -> String {
        switch mode {
        case .off: "Subtitles stay off until you turn them on."
        case .forced: "Only forced subtitles, for signs and foreign dialogue."
        case .foreign: "Full subtitles come on when the audio isn't in your subtitle language."
        case .always: "Subtitles in your language come on whenever there are some."
        }
    }

    /// `code`'s name; `nil` for none.
    public static func languageName(_ code: String?) -> String? {
        guard let code, !code.isEmpty else { return nil }
        return languages.first { $0.code == code }?.name ?? Words.language(code)
    }

    public func setAudioLanguage(_ code: String?) async {
        var next = trackDefaults
        next.audioLanguage = code
        await save(next, saved: "Audio language saved")
    }

    public func setSubtitleMode(_ mode: SubtitleMode) async {
        var next = trackDefaults
        next.subtitleMode = mode
        await save(next, saved: "Subtitles saved")
    }

    public func setSubtitleLanguage(_ code: String?) async {
        var next = trackDefaults
        next.subtitleLanguage = code
        await save(next, saved: "Subtitle language saved")
    }

    /// Sends `next`, shown at once; the one before comes back, with a toast
    /// saying why, when the server refuses. One change at a time.
    private func save(_ next: TrackDefaults, saved: String) async {
        guard editable, !saving, next != trackDefaults else { return }
        let before = trackDefaults
        trackDefaults = next
        saving = true
        defer { saving = false }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            let user = try await api.setTrackDefaults(next)
            app.updated(user)
            app.shell.show(saved)
        } catch is CancellationError {
            trackDefaults = before
        } catch {
            trackDefaults = before
            if case PleyError.insufficientScope = error { editable = false }
            app.shell.show(asSentence("Couldn't save: \(describe(error))"))
        }
    }

    /// The profile's defaults, as `GET /api/auth/me` said them; a change
    /// on its way keeps what shows.
    private func follow(_ user: User?) {
        editable = user?.scopes.contains("play") == true
        guard !saving, let user else { return }
        trackDefaults = TrackDefaults(user)
    }
}
