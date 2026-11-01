import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// A library's page: its tabs (Recommended, its rows then its collections;
/// Library, every title sorted and filtered; Collections, not for music),
/// opened on the tab and view it was left in on this device, or on the view
/// See all asks for. A tab reads only once shown. Save as… keeps the
/// Library tab's filters and order as a smart playlist.
/// The Library tab's grid reads 100 titles at a time (`LibraryGrid`); sorted
/// by title with more than 30 and at most 2,000 titles it reads them all
/// for the A to Z (`alpha`).
@MainActor
public final class LibraryModel: ObservableObject {
    public enum State: Equatable {
        case loading
        case loaded
        case failed(String)
        /// The library isn't there, or this profile may not open it.
        case notFound
    }

    /// What a failed load says.
    public static let failedTitle = "Couldn't load this library"
    public static let notFoundTitle = "This library isn't available"
    public static let emptyTitle = "Nothing here yet"
    public static let noMatchTitle = "Nothing matches these filters"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var library: Library?
    @Published public private(set) var params: LibraryParams
    /// The genres and years to filter by, once read.
    @Published public private(set) var filters = LibraryFilters()
    /// The Recommended tab's rows; `nil` until read.
    @Published public private(set) var rows: [HomeRow]?
    /// The Library tab's titles.
    @Published public private(set) var grid: LibraryGrid?
    /// The A to Z, when the listing has one.
    @Published public private(set) var alpha: AlphaIndex?
    /// Why the last read failed while something still shows.
    @Published public private(set) var notice: String?
    /// Its collections: the Collections tab, and the Recommended tab's last row.
    public let collections: CollectionsModel

    public let app: AppModel
    public let libraryId: Int
    /// A view asked for (See all): it wins over the one remembered, once.
    private let asked: String?
    private var alphaTask: Task<Void, Never>?
    /// The tab being shown after a change of sort or filter.
    private var changing: Task<Void, Never>?
    private var listener: AnyCancellable?
    /// The collections' changes show on the page too (Recommended's last row).
    private var forwarding: AnyCancellable?

    public init(app: AppModel, libraryId: Int, view: String? = nil) {
        self.app = app
        self.libraryId = libraryId
        asked = view
        params = LibraryParams.parse(view ?? "")
        collections = CollectionsModel(app: app, libraryId: libraryId)
        forwarding = collections.objectWillChange.sink { [weak self] _ in self?.objectWillChange.send() }
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, self.follows(change) else { return }
            Task { await self.followed(change) }
        }
    }

    /// The library's name, for the title.
    public var title: String { library?.name ?? "" }

    /// The tabs this library has: music has no collections.
    public var tabs: [LibraryTab] { LibraryParams.tabs(for: library?.kind) }

    /// The sorts this library offers.
    public var sorts: [LibrarySort] { LibraryParams.sorts(for: library?.kind ?? .movies) }

    /// Whether filters are offered: not for music.
    public var filterable: Bool { library?.kind != .music }

    /// The shape of its tiles: squares for music.
    public var shape: RowShape { library?.kind == .music ? .square : .poster }

    /// The library, then the tab it opens on.
    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            let library = try await api.library(libraryId)
            self.library = library
            var params = LibraryParams.parse(asked ?? remembered() ?? "")
            params = params.valid(for: library.kind)
            self.params = params
            collections.kind = library.kind
            state = .loaded
            notice = nil
            // The genres and years to filter by; without them the menus offer watched only.
            if library.kind != .music, let filters = try? await api.libraryFilters(libraryId) {
                self.filters = filters
            }
            await showTab()
        } catch is CancellationError {
        } catch PleyError.api(status: 404, _, _) {
            state = .notFound
        } catch PleyError.api(status: 403, _, _) {
            state = .notFound
        } catch {
            if library == nil { state = .failed(describe(error)) } else { notice = describe(error) }
        }
    }

    /// Reads the tab showing again (pull to refresh, Reload).
    public func reload() async {
        if library == nil { return await load() }
        switch params.tab {
        case .recommended:
            await loadRows()
            if library?.kind != .music { await collections.load() }
        case .collections:
            await collections.load()
        case .library:
            if let grid { await grid.reload() } else { await showTab() }
            refreshAlpha()
        }
    }

    /// Save as…: the smart playlist sheet, with the Library tab's filters
    /// and order in Plex's syntax.
    public func saveAs() {
        app.shell.sheet = .saveSmartPlaylist(libraryId: libraryId, query: SmartFilter.of(params))
    }

    public func select(_ tab: LibraryTab) {
        guard tab != params.tab, tabs.contains(tab) else { return }
        params.tab = tab
        remember()
        Task { await showTab() }
    }

    public func sort(by sort: LibrarySort) {
        change(params.sorted(by: sort))
    }

    public func flipOrder() {
        change(params.flipped())
    }

    /// `true` watched only, `false` unwatched only, `nil` both.
    public func filter(watched: Bool?) {
        var next = params
        next.watched = watched
        change(next)
    }

    public func filter(genre: String?) {
        var next = params
        next.genre = genre
        change(next)
    }

    public func filter(year: Int?) {
        var next = params
        next.year = year
        change(next)
    }

    public func clearFilters() {
        change(params.cleared())
    }

    /// Where the grid goes for `letter` of the A to Z.
    public func jumpTarget(_ letter: Int) -> Int? {
        alpha?.jumpTarget(letter)
    }

    /// The filters in words, for the button that shows them: "Unwatched · Comedy · 2021".
    public var filterSummary: String {
        [params.watched.map { $0 ? "Watched" : "Unwatched" }, params.genre, params.year.map(String.init)]
            .compactMap { $0 }.joined(separator: " · ")
    }

    // MARK: Inside

    private func change(_ next: LibraryParams) {
        guard next != params else { return }
        params = next
        remember()
        grid = nil
        alpha = nil
        // The last change wins: what an earlier one started reads no more.
        alphaTask?.cancel()
        changing?.cancel()
        changing = Task { await showTab() }
    }

    private func showTab() async {
        switch params.tab {
        case .recommended:
            if rows == nil { await loadRows() }
            if library?.kind != .music, !collections.loaded { await collections.load() }
        case .collections:
            if !collections.loaded { await collections.load() }
        case .library:
            if grid == nil { await startGrid() }
        }
    }

    private func loadRows() async {
        guard let api = await app.session.api else { return }
        do {
            let kinds = Dictionary(uniqueKeysWithValues: app.shell.libraries.map { ($0.id, $0.kind) })
            rows = try await api.libraryHubs(libraryId).filter { !$0.items.isEmpty }
                .map { HomeRow($0, libraryKind: kinds[$0.libraryId ?? libraryId] ?? library?.kind) }
            notice = nil
        } catch is CancellationError {
        } catch {
            if rows == nil { rows = [] }
            notice = describe(error)
        }
    }

    private func startGrid() async {
        guard let api = await app.session.api else { return }
        let id = libraryId
        let query = params.query
        let bitrate = params.sort == .bitrate
        let shape = shape
        guard !Task.isCancelled, self.grid == nil else { return }
        let grid = LibraryGrid(shape: shape, makeTile: { item in
            Tile(item, shape: shape, line: bitrate ? Words.fileLine(item.media) : nil)
        }, fetch: { offset, limit, tag in
            try await api.libraryItems(id, query, offset: offset, limit: limit, tag: tag)
        })
        self.grid = grid
        await grid.start()
        guard self.grid === grid else { return }
        refreshAlpha()
    }

    /// The A to Z: sorted by title, more than 30 titles and at most 2,000;
    /// every page read, then the index made off the main actor.
    private func refreshAlpha() {
        alphaTask?.cancel()
        guard let grid, params.sort == .title, let total = grid.total, total > AlphaIndex.minItems,
              total <= LibraryGrid.pageSize * LibraryGrid.maxPages
        else {
            alpha = nil
            return
        }
        alphaTask = Task { [weak self] in
            await grid.loadAll()
            guard !Task.isCancelled, let items = grid.everything else { return }
            let titles = items.map { $0.sortTitle.trimmingCharacters(in: .whitespaces).isEmpty ? $0.title : $0.sortTitle }
            let index = await Task.detached { AlphaIndex.of(titles: titles) }.value
            guard !Task.isCancelled, let self, self.grid === grid else { return }
            self.alpha = index
        }
    }

    /// Whether `change` touches what the page shows: its library's items,
    /// the libraries, or a title it shows.
    private func follows(_ change: ServerChange) -> Bool {
        if change.touches(library: libraryId) || change.libraries { return true }
        guard !change.itemIds.isEmpty else { return false }
        let shown = (rows ?? []).flatMap { $0.tiles.map(\.id) } + (grid?.pages.values.flatMap { $0.map(\.id) } ?? [])
        return shown.contains(where: change.itemIds.contains)
    }

    private func followed(_ change: ServerChange) async {
        guard library != nil else { return }
        if change.libraries || change.touches(library: libraryId) {
            if let api = await app.session.api, let fresh = try? await api.library(libraryId) { library = fresh }
        }
        await reload()
    }

    private func remembered() -> String? {
        guard let origin = app.server?.origin, let user = app.user else { return nil }
        return app.settings.libraryView(origin: origin, profileId: user.id, libraryId: libraryId)
    }

    private func remember() {
        guard let origin = app.server?.origin, let user = app.user else { return }
        app.settings.setLibraryView(params.queryString, origin: origin, profileId: user.id, libraryId: libraryId)
    }
}
