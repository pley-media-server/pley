import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

/// An item's page: what it is, and Play. A show or a season plays its
/// on-deck episode (`GET /api/items/{id}/on-deck`), shown with its "S1 E2"
/// label. Play resumes where the profile left off ("Resume from 12:34",
/// with "Play from start" beside it) unless the item was watched. An item
/// with several files has a version picker; the audio and subtitle pickers
/// save the choice on the server (`PUT /api/items/{id}/streams`), for the
/// movie or the whole show.
@MainActor
public final class ItemModel: ObservableObject {
    public enum State {
        case loading
        case loaded
        case failed(String)
    }

    /// A choice in a picker: a version (by file id) or a track (by stream index).
    public struct Choice: Hashable, Identifiable, Sendable {
        public var id: Int
        public var label: String
    }

    @Published public private(set) var state: State = .loading
    /// The item the page is of.
    @Published public private(set) var item: ItemDetail?
    /// What Play plays: the movie or episode itself, a show's or season's
    /// on-deck episode; `nil` when there's nothing to play.
    @Published public private(set) var playable: ItemDetail?
    /// The version picked; `nil` until someone picks one (the player then
    /// takes the one this device plays best).
    @Published public private(set) var pickedFileId: Int?
    /// A choice that couldn't be saved, in words.
    @Published public var notice: String?

    public let app: AppModel
    public let itemId: Int

    public init(app: AppModel, itemId: Int) {
        self.app = app
        self.itemId = itemId
    }

    /// Reads the item, and a show's or season's on-deck episode.
    public func load() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            let found = try await api.item(itemId)
            var play: ItemDetail? = found
            if found.kind == .show || found.kind == .season {
                if let next = try await api.onDeck(itemId) {
                    play = try await api.item(next.id)
                } else {
                    play = nil
                }
            } else if found.files.isEmpty {
                play = nil
            }
            item = found
            playable = play
            if let picked = pickedFileId, play?.files.contains(where: { $0.id == picked }) != true {
                pickedFileId = nil
            }
            state = .loaded
        } catch is CancellationError {
        } catch PleyError.signedOut {
            // The session says so, and sign-in shows.
        } catch {
            if item == nil { state = .failed(describe(error)) }
        }
    }

    // MARK: What the page shows

    /// "Arrival", or an episode's own title.
    public var title: String { item?.title ?? "" }

    /// "2016 · 1h 56m": the year and the runtime of what plays.
    public var facts: String {
        guard let item else { return "" }
        let runtime = Words.runtime(ms: file?.durationMs ?? playable?.durationMs ?? item.durationMs)
        return [item.year.map(String.init) ?? "", runtime].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    public var summary: String? { item?.summary.summary.flatMap { $0.isEmpty ? nil : $0 } }

    /// The file's video and sound: "1080p · H.264 · HDR10 · AAC Stereo".
    public var videoLine: String? { file.map(Words.videoLine).flatMap { $0.isEmpty ? nil : $0 } }

    /// The episode Play plays: "S1 E2 · Pilot" (a show's or season's
    /// on-deck episode, or the episode the page is of); `nil` for a movie.
    public var episodeLine: String? {
        guard let playable, playable.kind == .episode else { return nil }
        let label = Words.episode(season: playable.summary.parentIndex, episode: playable.summary.index)
        let title = playable.id == item?.id ? "" : playable.title
        let line = [label, title].filter { !$0.isEmpty }.joined(separator: " · ")
        return line.isEmpty ? nil : line
    }

    /// Where Resume starts, when the profile left it part-way and it isn't
    /// watched; `nil` shows Play alone.
    public var resumeMs: Int? {
        guard let state = playable?.userState, state.positionMs > 0, !state.watched else { return nil }
        return state.positionMs
    }

    /// "Resume from 12:34".
    public var resumeLabel: String? { resumeMs.map { "Resume from \(Words.clock(ms: $0))" } }

    /// The file shown and played: the one picked, else the one this device plays best.
    public var file: MediaFile? {
        guard let playable else { return nil }
        if let picked = pickedFileId, let file = playable.files.first(where: { $0.id == picked }) { return file }
        return PlaybackPlan.file(playable, requested: nil, caps: app.caps.snapshot?.report)
    }

    /// The versions, when there are several.
    public var versions: [Choice] {
        guard let files = playable?.files, files.count > 1 else { return [] }
        return files.enumerated().map { position, file in
            Choice(id: file.id, label: Words.version(file, position: position))
        }
    }

    public var audioTracks: [Choice] {
        file?.streams(.audio).map { Choice(id: $0.index, label: Words.stream($0)) } ?? []
    }

    public var subtitleTracks: [Choice] {
        file?.streams(.subtitle).map { Choice(id: $0.index, label: Words.stream($0)) } ?? []
    }

    /// The audio track the file starts with.
    public var audioIndex: Int? { file.flatMap(Words.startingAudio)?.index }

    /// The subtitle track it starts with; `nil`: off.
    public var subtitleIndex: Int? { file?.selectedSubtitle }

    // MARK: Choices

    public func chooseVersion(_ fileId: Int) {
        guard playable?.files.contains(where: { $0.id == fileId }) == true else { return }
        pickedFileId = fileId
    }

    /// Picks an audio track and saves it, with the subtitles as they are.
    public func chooseAudio(_ index: Int) async {
        guard let file, file.streams(.audio).contains(where: { $0.index == index }) else { return }
        await save(StreamsChoice(fileId: file.id, audioIndex: index, subtitleIndex: file.selectedSubtitle)) {
            $0.selectedAudio = index
            $0.audioPicked = true
        }
    }

    /// Picks a subtitle track (`nil`: off) and saves it; the audio stays as chosen.
    public func chooseSubtitle(_ index: Int?) async {
        guard let file else { return }
        if let index, !file.streams(.subtitle).contains(where: { $0.index == index }) { return }
        await save(StreamsChoice(fileId: file.id, subtitleIndex: index)) { $0.selectedSubtitle = index }
    }

    /// What Play (or Resume) asks for; `fromStart` for "Play from start".
    public func request(fromStart: Bool = false) -> PlayRequest? {
        guard let playable, !playable.files.isEmpty else { return nil }
        return PlayRequest(itemId: playable.id, fileId: pickedFileId, fromStart: fromStart)
    }

    private func save(_ choice: StreamsChoice, _ change: (inout MediaFile) -> Void) async {
        guard let api = await app.session.api, var playable, let fileId = choice.fileId,
              let at = playable.files.firstIndex(where: { $0.id == fileId })
        else { return }
        let before = playable
        change(&playable.files[at])
        self.playable = playable
        if item?.id == playable.id { item = playable }
        do {
            try await api.setStreams(itemId: playable.id, choice)
            notice = nil
        } catch {
            self.playable = before
            if item?.id == before.id { item = before }
            notice = asSentence(describe(error))
        }
    }
}
