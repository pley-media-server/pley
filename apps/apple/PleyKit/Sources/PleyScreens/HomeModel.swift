import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// One row of Home or of a library's Recommended tab: a hub of the
/// server's, its tiles made.
public struct HomeRow: Identifiable, Hashable, Sendable {
    /// The hub's key (`continue_watching`, `recently_added:3`): stable, the row's id.
    public let key: String
    /// "Continue Watching", "Recently Added in Movies", as the server says it.
    public let title: String
    public let shape: RowShape
    public let tiles: [Tile]
    /// Where See all goes: the library, listed as the row is; `nil` for rows without one.
    public let seeAll: Screen?

    public var id: String { key }

    /// Continue Watching: its tiles' actions offer to leave the row.
    public var continueWatching: Bool { key.hasPrefix("continue") }

    /// `hub`'s row; `libraryKind` is its library's, when known (music rows are squares).
    public init(_ hub: Hub, libraryKind: LibraryKind? = nil) {
        key = hub.key
        title = hub.title
        let shape = RowShape.of(hub, libraryKind: libraryKind)
        self.shape = shape
        tiles = hub.items.map { Tile($0, shape: shape) }
        seeAll = ShellModel.seeAll(hub)
    }

    init(key: String, title: String, shape: RowShape, tiles: [Tile], seeAll: Screen?) {
        self.key = key
        self.title = title
        self.shape = shape
        self.tiles = tiles
        self.seeAll = seeAll
    }

    /// The row without `itemId`'s tile.
    func without(_ itemId: Int) -> HomeRow {
        HomeRow(key: key, title: title, shape: shape, tiles: tiles.filter { $0.id != itemId }, seeAll: seeAll)
    }
}

/// What Apple TV's Home says above its rows about the tile with the focus
/// (Android's `HomeInfo`), every word made already.
public struct HomeInfo: Hashable, Sendable {
    public let itemId: Int
    /// The title, or an episode's show.
    public let title: String
    /// Episodes: "S2 · E5 · Glasshouse"; empty otherwise.
    public let context: String
    /// "2016 · PG-13 · 1h 56m · ★ 7.6 · 4K": the parts known.
    public let meta: [String]
    /// Up to the screen to clamp (3 lines); empty when the server has none.
    public let summary: String
    /// "26 min left" while in progress.
    public let timeLeft: String
    /// Whether the details (summary, content rating, quality) are in.
    public let complete: Bool

    /// What the info block says of `item`, with `detail` once it is in.
    public init(_ item: ItemSummary, detail: ItemDetail?) {
        let detail = detail?.id == item.id ? detail : nil
        itemId = item.id
        let episode = item.kind == .episode
        let show = episode ? item.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 } : nil
        title = show ?? item.title
        context = episode ? [Words.episodeLabel(season: item.parentIndex, episode: item.index), item.title]
            .filter { !$0.isEmpty }.joined(separator: " · ") : ""
        let runtime: String = switch item.kind {
        case .show, .season, .artist, .album: Words.cardSubtitle(item)
        default: Words.runtime(ms: item.durationMs ?? detail?.durationMs)
        }
        let year = (item.year ?? detail?.year).map(String.init) ?? ""
        var meta = [year]
        if let rating = detail?.contentRating, !rating.isEmpty { meta.append(rating) }
        if !(item.kind == .show && runtime == year) { meta.append(runtime) }
        if let critics = item.rating ?? detail?.summary.rating, critics > 0 {
            meta.append("★ " + Words.rating(critics))
        }
        if let file = detail?.files.first {
            if let resolution = Words.resolution(width: file.width ?? file.video?.width,
                                                 height: file.height ?? file.video?.height) {
                meta.append(resolution)
            }
            if let range = Words.dynamicRange(file.video) { meta.append(range) }
            if Words.isAtmos(Words.startingAudio(file)) { meta.append("Atmos") }
        }
        self.meta = meta.filter { !$0.isEmpty }
        summary = (detail?.summary.summary ?? item.summary ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        timeLeft = Words.isInProgress(item)
            ? Words.timeLeft(positionMs: item.userState.positionMs, durationMs: item.durationMs) : ""
        complete = detail != nil
    }
}

/// Home: the server's rows for the profile signed in, in the server's
/// order, empty ones left out. It follows the server's events (watch
/// state, My List, a library's items, the libraries, a scan that ended),
/// and after its first load reads the first three Continue Watching
/// titles' details, which starts the server reading their video ahead (as
/// Android does).
@MainActor
public final class HomeModel: ObservableObject {
    public enum State: Equatable {
        case loading
        /// The rows with something in them, in the server's order.
        case loaded([HomeRow])
        /// The profile sees no library.
        case noLibraries
        /// Libraries, but nothing in them yet; `scanning` while a scan runs.
        case empty(scanning: Bool)
        case failed(String)
    }

    /// What Home says when it couldn't load and has nothing to show.
    public static var failedTitle: String { localized("Couldn't load your home screen") }
    /// What it says with no rows.
    public static var emptyTitle: String { localized("Nothing here yet") }
    public static var noLibrariesTitle: String { localized("No libraries yet") }
    public static var scanningTitle: String { localized("Scanning your libraries…") }
    /// How long the focus rests on a tile before its details are read.
    public static let infoDelay: Double = 0.3

    @Published public private(set) var state: State = .loading
    /// Apple TV: what the info block says of the tile with the focus.
    @Published public private(set) var info: HomeInfo?

    public let app: AppModel
    /// The `homeVersion` the rows shown are for.
    private var version: Int?
    private var warmed = false
    private var listener: AnyCancellable?
    private var focusTask: Task<Void, Never>?
    private var details: [Int: ItemDetail] = [:]

    public init(app: AppModel) {
        self.app = app
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, change.home || change.everything else { return }
            Task { await self.reload() }
        }
    }

    /// The rows shown, when there are.
    public var rows: [HomeRow] {
        if case .loaded(let rows) = state { return rows }
        return []
    }

    /// Loads the rows again. The rows shown stay until the new ones come,
    /// unless they belong to another sign-in or profile: those go at once,
    /// so a failed load never leaves one profile's rows on another's Home.
    public func reload() async {
        guard let api = await app.session.api else { return }
        if version != app.homeVersion {
            version = app.homeVersion
            state = .loading
            warmed = false
            details = [:]
            info = nil
        }
        if case .failed = state { state = .loading }
        do {
            // While the server is out of reach, the rows and libraries saved last.
            let owner = app.shelfOwner
            async let hubs = app.cached(SnapshotKeys.home) { try await HomeRepository(api: $0).load() }
            async let libraries = app.cached(SnapshotKeys.libraries) { try await $0.libraries() }
            let (found, libs) = try await (hubs, libraries)
            app.shell.setLibraries(libs)
            let kinds = Dictionary(uniqueKeysWithValues: libs.map { ($0.id, $0.kind) })
            let rows = found.map { HomeRow($0, libraryKind: $0.libraryId.flatMap { kinds[$0] }) }
            if rows.isEmpty {
                state = libs.isEmpty ? .noLibraries
                    : .empty(scanning: libs.contains(where: \.scanning) || !app.scans.isEmpty)
            } else {
                state = .loaded(rows)
            }
            warmUp(rows, api: api)
            // Apple TV's Top Shelf follows Continue Watching.
            if let owner { app.homeRead(found, owner: owner) }
        } catch is CancellationError {
        } catch PleyError.signedOut {
            // The session says so, and sign-in shows.
        } catch {
            if case .loaded = state { return }
            state = .failed(describe(error))
        }
    }

    /// Takes `item` out of Continue Watching (an episode: its show): the
    /// tile goes at once, and comes back with a toast if the server refuses.
    public func removeFromContinueWatching(_ item: ItemSummary) async {
        guard case .loaded(let before) = state else { return }
        let show = item.kind == .episode ? item.grandparentId : nil
        state = .loaded(before.map { row in
            guard row.continueWatching else { return row }
            var out = row.without(item.id)
            if let show { out = HomeRow(key: out.key, title: out.title, shape: out.shape,
                                        tiles: out.tiles.filter { $0.item.grandparentId != show }, seeAll: out.seeAll) }
            return out
        }.filter { !$0.tiles.isEmpty })
        if await !app.actions.removeFromContinueWatching(item) {
            state = .loaded(before)
        }
    }

    /// Apple TV: the focus is on `tile`. The info block says what the
    /// tile knows at once, and its details once the focus rests 0.3 s.
    public func focus(_ tile: Tile) {
        info = HomeInfo(tile.item, detail: details[tile.id])
        focusTask?.cancel()
        guard details[tile.id] == nil else { return }
        let id = tile.id
        focusTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.infoDelay))
            guard !Task.isCancelled, let self, let api = await self.app.session.api,
                  let detail = try? await api.item(id), !Task.isCancelled
            else { return }
            self.details[id] = detail
            if self.info?.itemId == id { self.info = HomeInfo(tile.item, detail: detail) }
        }
    }

    /// The first three Continue Watching titles' details, once per load of
    /// a profile's Home: reading them starts the server reading ahead.
    private func warmUp(_ rows: [HomeRow], api: PleyAPI) {
        guard !warmed, !app.offline, let row = rows.first(where: \.continueWatching) else { return }
        warmed = true
        for tile in row.tiles.prefix(3) {
            let id = tile.id
            Task { [weak self] in
                guard let detail = try? await api.item(id) else { return }
                self?.details[id] = detail
            }
        }
    }

    /// The picture for `item`'s tile: its poster, else its thumbnail (an episode's).
    public static func posterPath(_ item: ItemSummary) -> String? {
        [item.posterUrl, item.thumbUrl].compactMap { $0 }.first { !$0.isEmpty }
    }

    /// Pixels to ask for a tile `points` wide on a screen of `scale`.
    public static func pixels(_ points: Double, scale: Double) -> Int {
        max(1, Int((points * max(scale, 1)).rounded()))
    }
}
