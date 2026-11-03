import Foundation
import PleyModel
import PleyNetwork
import PleyPlayer

/// A subtitle found online, as Find Subtitles… lists it.
public struct FoundSubtitleRow: Equatable, Identifiable, Sendable {
    /// What adding it sends back.
    public var id: String
    /// The release it was made for.
    public var title: String
    /// "English · Hearing impaired · Forced · Made for this file".
    public var detail: String
    /// "1,234 downloads"; `nil` with none.
    public var downloads: String?
    /// What VoiceOver reads: all of it.
    public var spokenLabel: String
}

/// Find Subtitles…, the player's search for subtitles online (docs/API.md,
/// "Added subtitles"), the Android app's: the file playing, in the
/// profile's subtitle language (its track defaults, else its audio
/// language, else the server's default) or another one picked. Choosing a
/// result adds it on the server; the new track is then shown as one picked
/// in the Subtitles menu (`PlaybackController.addedSubtitle`).
@MainActor
public final class SubtitleSearchModel: ObservableObject, Identifiable {
    public enum State: Equatable {
        case searching
        case found([FoundSubtitleRow])
        /// Nothing found: "No subtitles found in English".
        case none(String)
        /// The server has no search set up: what it says, and for an
        /// administrator where to set it up.
        case notSetUp(String, hint: String?)
        /// It failed: why, and whether Try again may help.
        case failed(String, canRetry: Bool)
    }

    public static let notSetUp = "Subtitle search isn't set up on this server"
    public static let setUpHint = "Set it up in Pley on the web, under Settings → Server."

    @Published public private(set) var state: State = .searching
    /// The language asked for (a code of `languages`); `nil`: the profile's.
    @Published public private(set) var language: String?
    /// The result being added; the list waits meanwhile.
    @Published public private(set) var adding: String?
    /// Adding failed: why, under the list.
    @Published public private(set) var addError: String?
    /// The track was added: the sheet closes.
    @Published public private(set) var added = false

    /// The languages to pick from, by name (Pley's codes).
    public let languages = MeModel.languages
    private let api: @MainActor () async -> PleyAPI?
    private let itemId: Int
    private let fileId: Int?
    /// The profile's language for subtitles, for the words; `nil`: the server's default, English.
    private let profileLanguage: String?
    private let admin: Bool
    private let onAdded: @MainActor (SubtitleTrack) -> Void
    private var task: Task<Void, Never>?

    public init(itemId: Int, fileId: Int?, profileLanguage: String?, admin: Bool,
                api: @escaping @MainActor () async -> PleyAPI?, onAdded: @escaping @MainActor (SubtitleTrack) -> Void) {
        self.itemId = itemId
        self.fileId = fileId
        self.profileLanguage = profileLanguage
        self.admin = admin
        self.api = api
        self.onAdded = onAdded
    }

    /// The search for what `controller` plays, adding to it.
    public convenience init?(app: AppModel, controller: PlaybackController) {
        guard let item = controller.item else { return nil }
        let user = app.user
        let language = [user?.subtitleLanguage, user?.audioLanguage].compactMap { $0 }.first { !$0.isEmpty }
        self.init(itemId: item.id, fileId: controller.file?.id, profileLanguage: language,
                  admin: user?.isAdmin ?? false, api: { [session = app.session] in await session.api },
                  onAdded: { [weak controller] in controller?.addedSubtitle($0) })
    }

    /// The language's name as the screen shows it: "English".
    public var languageName: String {
        Self.name(language ?? profileLanguage) ?? "English"
    }

    /// Searches, in the language picked.
    public func search() {
        task?.cancel()
        state = .searching
        addError = nil
        let (itemId, fileId, language) = (itemId, fileId, language)
        task = Task { [weak self] in
            guard let self else { return }
            do {
                guard let api = await self.api() else { throw PleyError.signedOut }
                let results = try await api.searchSubtitles(itemId: itemId, fileId: fileId, language: language)
                guard !Task.isCancelled else { return }
                self.state = results.isEmpty ? .none("No subtitles found in \(self.languageName)")
                    : .found(results.map(Self.row))
            } catch is CancellationError {
            } catch {
                guard !Task.isCancelled else { return }
                self.state = self.failure(error)
            }
        }
    }

    /// The language picked, as a code of `languages`: the profile's until another is.
    public var pickedLanguage: String { language ?? profileLanguage ?? "eng" }

    /// Searches again in `code`'s language (the profile's: as at first).
    public func choose(language code: String?) {
        let code = code == (profileLanguage ?? "eng") ? nil : code
        guard code != language else { return }
        language = code
        search()
    }

    /// Adds `row`'s subtitles: once the server has them, the player shows them.
    public func add(_ row: FoundSubtitleRow) {
        guard adding == nil else { return }
        adding = row.id
        addError = nil
        let (itemId, fileId) = (itemId, fileId)
        task = Task { [weak self] in
            guard let self else { return }
            defer { self.adding = nil }
            do {
                guard let api = await self.api() else { throw PleyError.signedOut }
                let track = try await api.addSubtitle(itemId: itemId, result: row.id, fileId: fileId)
                guard !Task.isCancelled else { return }
                self.onAdded(track)
                self.added = true
            } catch is CancellationError {
            } catch {
                guard !Task.isCancelled else { return }
                self.addError = Self.message(error)
            }
        }
    }

    /// The sheet went: nothing more is asked.
    public func close() {
        task?.cancel()
        task = nil
    }

    // MARK: Inside

    private func failure(_ error: any Error) -> State {
        switch error as? PleyError {
        case .notAvailable(_, _, setup: false)?:
            .notSetUp(Self.notSetUp, hint: admin ? Self.setUpHint : nil)
        case .notAvailable(_, _, setup: true)?, .api(429, _, _)?:
            .failed(Self.message(error), canRetry: false)
        case .api(404, _, _)?, .api(405, _, _)?:
            .failed("This server can't search for subtitles: it may need an update.", canRetry: false)
        default:
            .failed(Self.message(error), canRetry: true)
        }
    }

    /// The server's words, or the app's, as a sentence.
    private static func message(_ error: any Error) -> String { asSentence(describe(error)) }

    static func name(_ code: String?) -> String? {
        guard let code, !code.isEmpty else { return nil }
        return MeModel.languages.first { $0.code == code }?.name ?? Words.language(code)
    }

    /// "English · Hearing impaired · Forced · Made for this file", and the downloads.
    static func row(_ result: SubtitleResult) -> FoundSubtitleRow {
        let detail = [Words.language(result.language), result.hearingImpaired ? "Hearing impaired" : nil,
                      result.forced ? "Forced" : nil, result.madeForFile ? "Made for this file" : nil]
            .compactMap { $0 }.joined(separator: " · ")
        let downloads = result.downloads > 0
            ? "\(result.downloads.formatted(.number.locale(Locale(identifier: "en_US")))) download"
                + (result.downloads == 1 ? "" : "s")
            : nil
        let title = result.name.isEmpty ? (Words.language(result.language) ?? result.id) : result.name
        let spoken = [title, detail, downloads].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: ", ")
        return FoundSubtitleRow(id: result.id, title: title, detail: detail, downloads: downloads, spokenLabel: spoken)
    }
}
