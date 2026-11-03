import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

/// An item's page (movie, show, season, episode), a port of Android's
/// `ItemViewModel` and `itemPage`: the item, and beside it, read together,
/// a show's or season's next episode and seasons, the selected season's
/// episodes, More Like This and the collections it is part of. Every word
/// is in `page`, worked out here.
///
/// Play plays the movie or episode (Resume where the profile left it,
/// with From start beside it), or a show's or season's on-deck episode.
/// An item with several files has a version picker; the audio and subtitle
/// pickers save the choice on the server (`PUT /api/items/{id}/streams`),
/// for the movie or the whole show, and Automatic forgets the audio picked.
/// Watched marks, My List and the rating show at once and go back, with a
/// toast saying why, when the server refuses. Artists, albums, tracks and
/// extras have other pages: `elsewhere` names it. The page reads itself
/// again when the server says the item (or one of its episodes) changed.
@MainActor
public final class ItemModel: ObservableObject {
    public enum State: Equatable {
        case loading
        case loaded
        /// No such item, or one this profile may not see (404).
        case notFound
        case failed(String)
    }

    /// A choice in a picker: a version (by file id) or a track (by stream index).
    public struct Choice: Hashable, Identifiable, Sendable {
        public var id: Int
        public var label: String
    }

    /// What the page says when the item isn't there.
    public static let notFoundTitle = "This title isn't available"

    @Published public private(set) var state: State = .loading
    /// The item the page is of.
    @Published public private(set) var item: ItemDetail?
    /// What Play plays: the movie or episode itself, a show's or season's
    /// on-deck episode; `nil` when there's nothing to play.
    @Published public private(set) var playable: ItemDetail?
    /// The version picked; `nil` until someone picks one (the player then
    /// takes the one this device plays best).
    @Published public private(set) var pickedFileId: Int?
    /// A choice that couldn't be saved, in words (a toast says it too).
    @Published public var notice: String?
    /// The page, every word worked out; `nil` until the item is read.
    @Published public private(set) var page: ItemPage?
    /// The page that shows this item instead: an artist's, an album's
    /// (a track's), or an extra's movie or show.
    @Published public private(set) var elsewhere: Screen?

    public let app: AppModel
    public let itemId: Int
    /// The locale dates are written in.
    public var locale: Locale = .current

    private var onDeck: ItemSummary?
    private var onDeckKnown = false
    private var seasons: [ItemSummary]?
    private var selectedSeason: Int?
    private var chosenSeason: Int?
    private var episodes: [ItemSummary]?
    private var related: [ItemSummary] = []
    private var partOf: [CollectionRef] = []
    /// A write of the page's own on its way: the server's events about it aren't its last word yet.
    private var writing = 0
    private var listeners: Set<AnyCancellable> = []
    private var seasonTask: Task<Void, Never>?

    public init(app: AppModel, itemId: Int) {
        self.app = app
        self.itemId = itemId
        // Marks and My List made anywhere show here at once.
        app.actions.$watched.dropFirst().sink { [weak self] _ in
            Task { @MainActor in self?.rebuild() }
        }.store(in: &listeners)
        app.actions.$listed.dropFirst().sink { [weak self] _ in
            Task { @MainActor in self?.rebuild() }
        }.store(in: &listeners)
        // A download's progress shows on its button.
        app.downloads.changed.sink { [weak self] in
            Task { @MainActor in self?.objectWillChange.send() }
        }.store(in: &listeners)
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, self.writing == 0, self.follows(change) else { return }
            Task { await self.load() }
        }.store(in: &listeners)
    }

    /// Reads the item and what the page shows beside it. What shows stays
    /// while it reads again, and when that fails.
    public func load() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            let found = try await app.cached(SnapshotKeys.item(itemId)) { [itemId] in try await $0.item(itemId) }
            if let screen = Self.elsewhere(found) {
                item = found
                elsewhere = screen
                state = .loaded
                return
            }
            guard ItemKind.known.contains(found.kind) else {
                state = .notFound
                return
            }
            let kind = found.kind
            let container = kind == .show || kind == .season
            let listed = kind == .movie || kind == .show
            let show = kind == .show ? itemId : kind == .season ? found.summary.parentId : nil
            let id = itemId
            // Offline, what was saved: the item and its seasons and episodes; nothing else is asked.
            let online = !app.offline
            let app = app
            async let deck: ItemSummary? = container && online ? (try? await api.onDeck(id)) ?? nil : nil
            async let seasonList: [ItemSummary]? = show == nil ? nil
                : try? await app.cached(SnapshotKeys.children(show!)) { try await $0.children(show!) }
            async let more: [ItemSummary] = listed && online ? (try? await api.related(id)) ?? [] : []
            async let collections: [CollectionRef] = listed && online ? (try? await api.itemCollections(id)) ?? [] : []
            let next = await deck
            var play: ItemDetail? = found
            if container {
                play = next == nil ? nil
                    : try await app.cached(SnapshotKeys.item(next!.id)) { try await $0.item(next!.id) }
            } else if found.files.isEmpty {
                play = nil
            }
            let list = await seasonList
            let selected: Int? = switch kind {
            case .show: chosenSeason ?? list.flatMap { ItemPage.defaultSeason($0, onDeck: next) }
            case .season: chosenSeason ?? itemId
            case .episode: found.summary.parentId
            default: nil
            }
            let shown: [ItemSummary]? = if let selected {
                try? await app.cached(SnapshotKeys.children(selected)) { try await $0.children(selected) }
            } else {
                nil
            }
            related = await more
            partOf = await collections
            item = found
            playable = play
            onDeck = next
            onDeckKnown = container
            seasons = list
            selectedSeason = selected
            episodes = shown
            elsewhere = nil
            if let picked = pickedFileId, play?.files.contains(where: { $0.id == picked }) != true {
                pickedFileId = nil
            }
            state = .loaded
            rebuild()
        } catch is CancellationError {
        } catch PleyError.signedOut {
            // The session says so, and sign-in shows.
        } catch PleyError.api(status: 404, _, _) {
            item = nil
            page = nil
            state = .notFound
        } catch {
            if item == nil { state = .failed(describe(error)) }
        }
    }

    // MARK: Downloads

    /// The item page's Download button, as its download is.
    public enum DownloadButton: Hashable, Sendable {
        /// Not downloaded: Download.
        case download
        /// On its way: where it is in words, and how far (0 to 1) when known.
        case progress(String, Double?)
        case downloaded
        /// Stopped: why; Try again.
        case failed(String)
    }

    /// A movie's or an episode's Download button; `nil` where there is
    /// none (Apple TV, other kinds, nothing to play, or offline and not downloaded).
    public var downloadButton: DownloadButton? {
        guard app.downloads.available, let item, item.kind == .movie || item.kind == .episode else { return nil }
        guard let entry = app.downloads.entries.first(where: { $0.itemId == item.id }) else {
            return app.offline || item.files.isEmpty ? nil : .download
        }
        switch entry.state {
        case .done: return .downloaded
        case .failed: return .failed(DownloadWords.status(entry))
        default:
            let row = DownloadRow(entry)
            return .progress(row.status, row.progress)
        }
    }

    /// Download season, on a show's or season's page: the season's episodes listed, while online.
    public var canDownloadSeason: Bool {
        guard app.downloads.available, !app.offline, let kind = item?.kind, kind == .show || kind == .season
        else { return false }
        return episodes?.contains { $0.kind == .episode } == true
    }

    /// Downloads this movie or episode; the toast says where it goes.
    public func download() async {
        guard let item else { return }
        await download([item.id])
    }

    /// Downloads the episodes of the season shown, in order.
    public func downloadSeason() async {
        let ids = (episodes ?? []).filter { $0.kind == .episode }.map(\.id)
        await download(ids)
    }

    public func removeDownload() {
        guard let item else { return }
        app.downloads.remove(item.id)
    }

    public func retryDownload() {
        guard let item else { return }
        app.downloads.retry(item.id)
    }

    private func download(_ ids: [Int]) async {
        guard !ids.isEmpty else { return }
        do {
            try await app.downloads.download(ids)
            app.shell.show(DownloadWords.started)
        } catch is CancellationError {
        } catch {
            app.shell.show(asSentence(describe(error)))
        }
    }

    /// The page an item of another kind has: music's own, an extra's parent's.
    nonisolated static func elsewhere(_ item: ItemDetail) -> Screen? {
        let s = item.summary
        switch item.kind {
        case .artist: return .artist(s.id)
        case .album: return .album(s.id)
        case .track: return s.parentId.map { .album($0, track: s.id) }
        case .extra: return s.parentId.map { .item($0) }
        default: return nil
        }
    }

    // MARK: What the page shows

    /// "Arrival", an episode's own title, a season's "Season 2".
    public var title: String { page?.title ?? item?.title ?? "" }

    /// "2016 · 1h 56m": the year and the runtime of what plays.
    public var facts: String {
        guard let item else { return "" }
        let runtime = Words.runtime(ms: file?.durationMs ?? playable?.durationMs ?? item.durationMs)
        return [item.year.map(String.init) ?? "", runtime].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    public var summary: String? { page?.summary ?? item?.summary.summary.flatMap { $0.isEmpty ? nil : $0 } }

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
        rebuild()
    }

    /// Picks an audio track and saves it, with the subtitles as they are.
    /// `nil` is Automatic: the pick is forgotten, and each device plays its
    /// best track of the language.
    public func chooseAudio(_ index: Int?) async {
        guard let file else { return }
        guard let index else {
            await save(StreamsChoice(fileId: file.id, subtitleIndex: file.selectedSubtitle, automaticAudio: true)) {
                $0.audioPicked = false
            }
            // Without a pick, the track the details name is the server's to work out again.
            await load()
            return
        }
        guard file.streams(.audio).contains(where: { $0.index == index }) else { return }
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

    /// What playing `item` (an episode of the list, an extra, the trailer) asks for.
    public func request(for item: ItemSummary, fromStart: Bool = false) -> PlayRequest? {
        if item.id == playable?.id { return request(fromStart: fromStart) }
        switch item.kind {
        case .movie, .episode, .extra: return PlayRequest(itemId: item.id, fromStart: fromStart)
        default: return nil
        }
    }

    // MARK: Seasons

    /// Shows `seasonId`'s episodes.
    public func selectSeason(_ seasonId: Int) {
        guard seasonId != selectedSeason, seasons?.contains(where: { $0.id == seasonId }) == true else { return }
        chosenSeason = seasonId
        selectedSeason = seasonId
        episodes = nil
        rebuild()
        seasonTask?.cancel()
        seasonTask = Task { [weak self] in
            guard let api = await self?.app.session.api else { return }
            let found = try? await api.children(seasonId)
            guard let self, !Task.isCancelled, self.selectedSeason == seasonId else { return }
            self.episodes = found ?? []
            self.rebuild()
        }
    }

    // MARK: Marks

    /// Marks the item watched, or unwatched when it is.
    public func toggleWatched() async {
        guard let page, page.markable, let item else { return }
        await setWatched(item.summary, !page.watched)
    }

    /// Marks `target` (this item, or one of its episodes) watched or not:
    /// shown at once, undone with a toast when the server refuses.
    public func setWatched(_ target: ItemSummary, _ mark: Bool) async {
        writing += 1
        defer { writing -= 1 }
        await app.actions.setWatched(target, mark)
        // Kept here too, so the page doesn't go back while it reads the server again.
        if app.actions.watched[target.id] == mark {
            let marked = Tile(target, shape: .poster).marked(watched: mark).item
            if item?.id == target.id { item?.summary = marked }
            if playable?.id == target.id { playable?.summary = marked }
            if let at = episodes?.firstIndex(where: { $0.id == target.id }) { episodes?[at] = marked }
        }
        rebuild()
    }

    /// Adds the item to My List, or takes it out.
    public func toggleMyList() async {
        guard let page, let inList = page.myList, let item else { return }
        writing += 1
        defer { writing -= 1 }
        await app.actions.setListed(item.summary, !inList)
    }

    /// The profile's rating, 1 to 10 (a half star each); `nil` or 0 takes it away.
    public func rate(_ rating: Int?) async {
        guard let api = await app.session.api, item != nil else { return }
        let value = rating.flatMap { $0 > 0 ? min($0, 10) : nil }
        let before = item?.summary.userState.rating
        item?.summary.userState.rating = value.map(Double.init)
        rebuild()
        writing += 1
        defer { writing -= 1 }
        do {
            try await api.setRating(itemId, value.map(Double.init))
        } catch {
            item?.summary.userState.rating = before
            rebuild()
            app.shell.show("Couldn't save your rating: \(describe(error))")
        }
    }

    /// Play is about to be pressed (it has the focus or the pointer): the
    /// server reads ahead what it plays first. At most once in 2 minutes.
    public func prefetch() {
        guard page?.primary.kind != .unavailable else { return }
        app.actions.prefetch(itemId)
    }

    // MARK: Inside

    /// Whether `change` names what the page shows: the item, its show or
    /// season, or one of the episodes listed.
    private func follows(_ change: ServerChange) -> Bool {
        if change.names(itemId, parent: item?.summary.parentId) { return true }
        if let show = item?.summary.grandparentId, change.parentIds.contains(show) { return true }
        if let deck = onDeck?.id, change.itemIds.contains(deck) { return true }
        return episodes?.contains { change.itemIds.contains($0.id) } == true
    }

    private func rebuild() {
        guard let item, elsewhere == nil else {
            page = nil
            return
        }
        var detail = item
        detail.summary = app.actions.shown(detail.summary)
        let video = item.kind == .movie || item.kind == .episode || item.kind == .extra
        let listed = item.kind == .movie || item.kind == .show
        let input = PageInput(
            detail: detail,
            file: video && playable?.id == item.id ? file : nil,
            onDeck: onDeck,
            onDeckKnown: onDeckKnown,
            seasons: seasons,
            selectedSeason: selectedSeason,
            episodes: episodes?.map(app.actions.shown),
            related: related,
            partOf: partOf,
            inWatchlist: listed ? app.actions.listed.map { $0.contains(item.id) } : nil,
            locale: locale)
        page = ItemPage(input)
    }

    private func save(_ choice: StreamsChoice, _ change: (inout MediaFile) -> Void) async {
        guard let api = await app.session.api, var playable, let fileId = choice.fileId,
              let at = playable.files.firstIndex(where: { $0.id == fileId })
        else { return }
        let before = playable
        change(&playable.files[at])
        self.playable = playable
        if item?.id == playable.id { item = playable }
        rebuild()
        writing += 1
        defer { writing -= 1 }
        do {
            try await api.setStreams(itemId: playable.id, choice)
            notice = nil
        } catch {
            self.playable = before
            if item?.id == before.id { item = before }
            rebuild()
            notice = asSentence(describe(error))
            app.shell.show("Couldn't save that choice: \(describe(error))")
        }
    }
}
