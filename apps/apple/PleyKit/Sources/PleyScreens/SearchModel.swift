import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// Search (Android's `SearchViewModel`, the web's SearchPage): what is
/// typed is asked for once typing has paused 250 ms, and only the latest
/// query's answer counts. The results already shown stay while the next
/// ones load. Results come in groups, in order: Movies, Shows, Seasons,
/// Episodes, Artists, Albums, Tracks, each with its count, then Live TV's
/// channels (a dozen at most), which open the live player in All
/// channels, then People: someone searching usually wants a title, as in
/// every app. A query is kept among the profile's last 10 searches when
/// it is submitted or one of its results is opened.
///
/// While asking for movies and shows works, TMDB is searched beside the
/// libraries (never holding them up) for titles the libraries don't have:
/// the Discover row after the results, with See all. It comes and goes as
/// asking starts or stops working, and Apple TV, without Discover, has none.
@MainActor
public final class SearchModel: ObservableObject {
    /// The groups results come in, in the order they show.
    public enum GroupKind: String, CaseIterable, Sendable {
        case movies, shows, seasons, episodes, artists, albums, tracks, channels, people

        public var title: String {
            switch self {
            case .people: "People"
            case .movies: "Movies"
            case .shows: "Shows"
            case .seasons: "Seasons"
            case .episodes: "Episodes"
            case .artists: "Artists"
            case .albums: "Albums"
            case .tracks: "Tracks"
            case .channels: "Live TV"
            }
        }

        /// The kind of item the group holds; `nil` for people and channels.
        var itemKind: ItemKind? {
            switch self {
            case .people, .channels: nil
            case .movies: .movie
            case .shows: .show
            case .seasons: .season
            case .episodes: .episode
            case .artists: .artist
            case .albums: .album
            case .tracks: .track
            }
        }

        /// How its tiles are shaped (people are circles).
        public var shape: RowShape {
            switch self {
            case .episodes, .channels: .landscape
            case .artists, .albums, .tracks: .square
            default: .poster
            }
        }
    }

    /// One group of results: tiles for items, people for People, channels for Live TV.
    public struct Group: Identifiable, Hashable, Sendable {
        public var id: GroupKind { kind }
        public let kind: GroupKind
        public let tiles: [Tile]
        public let people: [PersonSummary]
        public var channels: [LiveChannel] = []

        public var count: Int {
            switch kind {
            case .people: people.count
            case .channels: channels.count
            default: tiles.count
            }
        }
        /// "Movies, 3 results", for VoiceOver.
        public var spoken: String { "\(kind.title), \(Words.pluralize(count, "result"))" }
    }

    public enum State: Equatable, Sendable {
        /// Nothing typed: the recent searches show.
        case idle
        /// The first answer for the query is on its way, with nothing earlier to keep showing.
        case loading(String)
        /// Results for the query; while `loading`, a newer query's are on their way.
        case ready(String, [Group], loading: Bool)
        /// `No results for "north"`.
        case empty(String)
        /// "Search failed", with why.
        case failed(String, String)
    }

    /// How long typing pauses before what is typed is searched.
    public static let debounce: Duration = .milliseconds(250)
    /// How many items a search asks for.
    public static let limit = 40
    /// How many channels show.
    public nonisolated static let channels = 12
    /// Titles the Discover row reads, at most, and the pages it reads for them.
    public nonisolated static let discoverTitles = 12
    public nonisolated static let discoverPages = 3

    /// Search's Discover row.
    public enum DiscoverRow: Hashable, Sendable {
        /// Asking doesn't work here, or nothing is typed.
        case hidden
        case loading
        /// TMDB's titles for the query (some: with none, the row is hidden).
        case ready(query: String, cards: [DiscoverCard])
        /// TMDB couldn't be searched: why, with Retry.
        case failed(ProblemWords)
    }

    /// What is typed now, as the field shows it.
    @Published public var query = "" {
        didSet { if query != oldValue { typed() } }
    }
    @Published public private(set) var state: State = .idle
    /// The profile's recent searches, newest first.
    @Published public private(set) var recent: [String] = []
    /// TMDB's titles for the query, the libraries don't have.
    @Published public private(set) var discover: DiscoverRow = .hidden

    public let app: AppModel
    private let wait: Duration
    private var asking: Task<Void, Never>?
    private var discovering: Task<Void, Never>?
    private var discoverTitles: [DiscoverTitle] = []
    private var listeners: Set<AnyCancellable> = []
    /// The query asked for last, trimmed.
    private var asked = ""
    /// The query whose results show.
    private var shown = ""

    public init(app: AppModel, debounce: Duration = SearchModel.debounce) {
        self.app = app
        wait = debounce
        loadRecent()
        // Badges in the Discover row follow the changes to requests.
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, !change.requests.isEmpty, !self.discoverTitles.isEmpty else { return }
            Task { await self.patchDiscover(change.requests) }
        }.store(in: &listeners)
        // Asking starting or stopping to work (the summary read late, signing
        // in again, access changed) brings the row for what is typed, or hides it.
        app.requests.$summary.dropFirst().map { RequestsGate.of($0).mayAsk }.removeDuplicates().sink { [weak self] _ in
            Task { @MainActor in self?.retryDiscover() }
        }.store(in: &listeners)
    }

    /// TMDB is searched for the Discover row: asking works, and the app has Discover.
    private var discovers: Bool {
        #if os(tvOS)
        false
        #else
        app.requests.gate.mayAsk
        #endif
    }

    /// Discover's card above the recent searches: requests are open to this account.
    public var discoverEntry: Bool { app.shell.requestsEntry.discover }

    /// The libraries found nothing, and the Discover row shows instead: "Nothing in your libraries matches".
    public var nothingInLibraries: Bool {
        guard case .empty = state else { return false }
        return discover != .hidden
    }

    /// See all: every title TMDB finds for the query.
    public var discoverAll: Screen? {
        guard case .ready(let query, let cards) = discover, !cards.isEmpty else { return nil }
        return .discoverResults(query: query)
    }

    /// Searches TMDB again, after it failed or as asking starts or stops working.
    public func retryDiscover() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        discovering?.cancel()
        discovering = Task { [weak self] in await self?.searchDiscover(trimmed) }
    }

    /// `No results for "north"`.
    public static func noResults(_ query: String) -> String { "No results for \u{201C}\(query)\u{201D}" }

    /// The recent searches, read again (another profile, or on showing).
    public func loadRecent() {
        guard let origin = app.server?.origin, let user = app.user else {
            recent = []
            return
        }
        recent = app.settings.recentSearches(origin: origin, profileId: user.id)
    }

    /// Search was pressed: the query is kept among the recent ones, and asked for now.
    public func submit() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        remember(trimmed)
        if trimmed != asked || state.failedQuery != nil { ask(trimmed, after: .zero) }
    }

    /// A recent search was chosen: searched again, at once, and first among the recent ones.
    public func searchAgain(_ text: String) {
        query = text
        submit()
    }

    public func clearRecent() {
        guard let origin = app.server?.origin, let user = app.user else { return }
        app.settings.clearRecentSearches(origin: origin, profileId: user.id)
        recent = []
    }

    /// Asks for the query again, after it failed.
    public func retry() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        ask(trimmed, after: .zero)
    }

    /// Where `tile`'s result goes; the query is kept among the recent
    /// searches, since one of its results was wanted.
    public func open(_ tile: Tile) -> Screen {
        remember(shown)
        return Screen.page(of: tile.item)
    }

    public func open(_ person: PersonSummary) -> Screen {
        remember(shown)
        return .person(person.id)
    }

    /// `results` in their groups and the groups' order, each keeping the
    /// server's order of relevance; empty groups and other kinds (extras)
    /// left out, as on the web.
    public nonisolated static func groups(_ results: SearchResults) -> [Group] {
        GroupKind.allCases.compactMap { kind in
            if kind == .people {
                return results.people.isEmpty ? nil : Group(kind: kind, tiles: [], people: results.people)
            }
            if kind == .channels {
                let channels = Array(results.channels.prefix(channels))
                return channels.isEmpty ? nil : Group(kind: kind, tiles: [], people: [], channels: channels)
            }
            let tiles = results.items.filter { $0.kind == kind.itemKind }.map { Tile($0, shape: kind.shape) }
            return tiles.isEmpty ? nil : Group(kind: kind, tiles: tiles, people: [])
        }
    }

    // MARK: Inside

    private func typed() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != asked else { return }
        if trimmed.isEmpty {
            asking?.cancel()
            asking = nil
            discovering?.cancel()
            discovering = nil
            discoverTitles = []
            discover = .hidden
            asked = ""
            shown = ""
            state = .idle
            loadRecent()
            return
        }
        ask(trimmed, after: wait)
    }

    /// Asks for `text` once `delay` passed with nothing newer typed; an
    /// answer for an older query is dropped.
    private func ask(_ text: String, after delay: Duration) {
        asking?.cancel()
        asked = text
        asking = Task { [weak self] in
            if delay > .zero {
                try? await Task.sleep(for: delay)
                guard !Task.isCancelled else { return }
            }
            await self?.fetch(text)
        }
        discovering?.cancel()
        discovering = Task { [weak self] in
            if delay > .zero {
                try? await Task.sleep(for: delay)
                guard !Task.isCancelled else { return }
            }
            await self?.searchDiscover(text)
        }
    }

    /// TMDB's titles for `text` the libraries don't have, page by page
    /// until a row's worth. A failure with titles already found keeps them.
    private func searchDiscover(_ text: String) async {
        guard discovers else {
            discoverTitles = []
            discover = .hidden
            return
        }
        if discoverTitles.isEmpty { discover = .loading }
        var pages = TitlePages()
        var read = 0
        while pages.more && pages.titles.count < Self.discoverTitles && read < Self.discoverPages {
            do {
                let answer = try await app.requestsAPI().discoverSearch(text, page: pages.page + 1, missing: true)
                guard !Task.isCancelled, text == asked else { return }
                pages = pages.with(answer)
                read += 1
            } catch {
                guard !Task.isCancelled, text == asked, !(error is CancellationError) else { return }
                if pages.titles.isEmpty {
                    discoverTitles = []
                    let problem = RequestProblem.of(error)
                    discover = .failed(problem.words(manage: false, summary: app.requests.summary,
                                                     failure: RequestWords.searchFailed))
                    return
                }
                break
            }
        }
        discoverTitles = Array(pages.titles.prefix(Self.discoverTitles))
        // TMDB found nothing missing: no row, and Search says "No results" as without one.
        discover = discoverTitles.isEmpty ? .hidden : .ready(query: text, cards: discoverTitles.map(DiscoverCard.init))
    }

    private func patchDiscover(_ change: RequestChange) async {
        guard case .ready(let query, _) = discover,
              let patch = await app.patchFor(change, shown: discoverTitles) else { return }
        discoverTitles = discoverTitles.map(patch)
        discover = .ready(query: query, cards: discoverTitles.map(DiscoverCard.init))
    }

    private func fetch(_ text: String) async {
        switch state {
        case .ready(let query, let groups, _): state = .ready(query, groups, loading: true)
        default: state = .loading(text)
        }
        do {
            let limit = Self.limit
            let results = try await app.cached(SnapshotKeys.search(text)) { try await $0.search(text, limit: limit) }
            guard !Task.isCancelled, text == asked else { return }
            let groups = Self.groups(results)
            shown = text
            state = groups.isEmpty ? .empty(text) : .ready(text, groups, loading: false)
        } catch {
            guard !Task.isCancelled, text == asked, !(error is CancellationError) else { return }
            shown = ""
            state = .failed(text, describe(error))
        }
    }

    private func remember(_ text: String) {
        guard !text.isEmpty, let origin = app.server?.origin, let user = app.user else { return }
        app.settings.rememberSearch(text, origin: origin, profileId: user.id)
        recent = app.settings.recentSearches(origin: origin, profileId: user.id)
    }
}

extension SearchModel.State {
    /// The query of a failed search.
    var failedQuery: String? {
        if case .failed(let query, _) = self { return query }
        return nil
    }

    /// The groups showing.
    public var groups: [SearchModel.Group] {
        if case .ready(_, let groups, _) = self { return groups }
        return []
    }
}
