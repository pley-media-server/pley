import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

/// A collection as its tile shows it: its poster, title, how many titles
/// (the badge on the poster) and its TMDB or Smart badge, every word made.
public struct CollectionCard: Identifiable, Hashable, Sendable {
    public let id: Int
    public let title: String
    /// The badge: "12".
    public let count: String
    /// "12 movies", under the title.
    public let subtitle: String
    public let smart: Bool
    public let automatic: Bool
    /// Server paths of its poster: its own (which falls back to an item's), else none.
    public let pictures: [String]
    /// What VoiceOver says: "Dune Collection, collection, 2 movies".
    public let spokenLabel: String

    public init(_ collection: CollectionSummary, kind: LibraryKind?) {
        id = collection.id
        title = collection.title
        count = collection.itemCount.formatted(.number.locale(ShownLanguage.locale))
        let items = collection.itemCount
        subtitle = kind == .shows ? localized("\(items) shows") : localized("\(items) movies")
        smart = collection.smart
        automatic = collection.automatic
        pictures = collection.posterUrl.flatMap { $0.isEmpty ? nil : [$0] } ?? ["/api/collections/\(id)/images/poster"]
        spokenLabel = [title, smart ? localized("smart collection") : localized("collection"), subtitle]
            .joined(separator: ", ")
    }
}

/// What the Category filter keeps: every category, or one.
public enum CategoryChoice: Hashable, Sendable {
    case all
    case category(Int)
}

/// Collections under one category. The server sends Other as the
/// category of those without one; `category` is `nil` only from a server
/// older than categories, whose collections are all in one group, with no
/// heading and no choice in the Category filter.
public struct CollectionGroup: Identifiable, Hashable, Sendable {
    public let category: CollectionCategory?
    public let cards: [CollectionCard]

    /// The category's id, or "none".
    public var id: String { category.map { String($0.id) } ?? "none" }
    /// The Category filter's choice that picks it; `nil` for the group without a category.
    public var choice: CategoryChoice? { category.map { .category($0.id) } }
    public var title: String { category?.title ?? "" }

    /// `collections` by category, in the categories' order; empty groups
    /// left out. A category not listed yet (made since), or none, comes
    /// after the rest.
    public static func of(_ collections: [CollectionSummary], categories: [CollectionCategory],
                          kind: LibraryKind?) -> [CollectionGroup] {
        var order: [CollectionCategory?] = categories
        for category in collections.map(\.category) where !order.contains(where: { $0?.id == category?.id }) {
            order.append(category)
        }
        return order.compactMap { category in
            let cards = collections.filter { $0.category?.id == category?.id }.map { CollectionCard($0, kind: kind) }
            return cards.isEmpty ? nil : CollectionGroup(category: category, cards: cards)
        }
    }
}

/// One library's collections: its Collections tab, grouped by category in
/// the categories' order, with a Category filter. Categories
/// are arranged on the web, and collections made there. Music has none.
@MainActor
public final class CollectionsModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
    }

    public static var failedTitle: String { localized("Couldn't load the collections") }
    public static var emptyTitle: String { localized("No collections in this library") }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var collections: [CollectionSummary] = []
    @Published public private(set) var categories: [CollectionCategory] = []
    /// Every group, those with a collection in them. Made after each read
    /// (and a change of kind), not each time a view asks: every card's
    /// words are made with them, and the tab asks for them several times a
    /// drawing.
    @Published private var allGroups: [CollectionGroup] = []
    /// Why the last read failed while cards still show.
    @Published public private(set) var notice: String?
    @Published public var category: CategoryChoice = .all

    public let app: AppModel
    public let libraryId: Int
    /// The library's kind, for the counts' nouns.
    public var kind: LibraryKind? {
        didSet { if kind != oldValue { regroup() } }
    }

    public init(app: AppModel, libraryId: Int, kind: LibraryKind? = nil) {
        self.app = app
        self.libraryId = libraryId
        self.kind = kind
    }

    /// Whether it was read once.
    public var loaded: Bool { state == .loaded }

    /// Every collection's card, by title.
    public var cards: [CollectionCard] { collections.map { CollectionCard($0, kind: kind) } }

    /// The groups the Category filter offers: those with a category.
    public var categoryChoices: [CollectionGroup] { allGroups.filter { $0.choice != nil } }

    /// The Category filter is offered when there is more than one group to
    /// choose between, or one is chosen.
    public var offersCategories: Bool { categoryChoices.count > 1 || chosenCategory != .all }

    /// The category chosen; one emptied or deleted since filters nothing.
    public var chosenCategory: CategoryChoice {
        guard category != .all else { return .all }
        return categoryChoices.contains { $0.choice == category } ? category : .all
    }

    /// What shows: the collections grouped, those of the category chosen.
    public var groups: [CollectionGroup] {
        let chosen = chosenCategory
        return allGroups.filter { chosen == .all || $0.choice == chosen }
    }

    /// Headings show when more than one group does.
    public var showsHeadings: Bool { groups.count > 1 }

    public var filtered: Bool { chosenCategory != .all }

    /// The Category filter's name for `choice`.
    public func title(of choice: CategoryChoice) -> String {
        switch choice {
        case .all: localized("All categories")
        case .category: categoryChoices.first { $0.choice == choice }?.title ?? ""
        }
    }

    /// "12 collections": how many show.
    public var countWords: String {
        let count = groups.reduce(0) { $0 + $1.cards.count }
        return localized("\(count) collections")
    }

    public func clearFilters() {
        category = .all
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            async let found = api.collections(library: libraryId)
            // Without the categories, the groups follow the collections' own.
            async let order = try? api.collectionCategories()
            let (collections, categories) = try await (found, order)
            self.collections = collections
            self.categories = categories ?? self.categories
            regroup()
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch {
            if state == .loaded { notice = describe(error) } else { state = .failed(describe(error)) }
        }
    }

    private func regroup() {
        allGroups = CollectionGroup.of(collections, categories: categories, kind: kind)
    }
}

/// A collection's page (Android's `CollectionViewModel`): its backdrop and
/// poster, the TMDB and Smart badges, what it holds in words, Play and
/// Shuffle, then its titles.
///
/// Movies play as a queue in its order (`PlayQueue.collection`): Play from
/// the first, Shuffle from a random one. A collection of shows plays the
/// first show's next episode; Shuffle a random show's, as the web does.
@MainActor
public final class CollectionModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
        /// Gone, or nothing in it this profile may see.
        case notFound
    }

    public static var failedTitle: String { localized("Couldn't load this collection") }
    public static var notFoundTitle: String { localized("This collection isn't available") }
    public static var nothingToPlay: String { localized("Nothing to play") }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var detail: CollectionDetail?
    @Published public private(set) var tiles: [Tile] = []
    @Published public private(set) var notice: String?

    public let app: AppModel
    public let collectionId: Int
    /// Picks a random index below its argument (the tests set their own).
    var random: (Int) -> Int = { Int.random(in: 0..<$0) }
    private var listener: AnyCancellable?

    public init(app: AppModel, collectionId: Int) {
        self.app = app
        self.collectionId = collectionId
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, self.detail != nil else { return }
            let shown = Set(self.tiles.map(\.id))
            guard change.everything || change.touches(library: self.detail?.collection.libraryId ?? -1)
                || !shown.isDisjoint(with: change.itemIds) else { return }
            Task { await self.load() }
        }
    }

    public var title: String { detail?.collection.title ?? "" }
    public var summary: String? { detail?.collection.summary.flatMap { $0.isEmpty ? nil : $0 } }
    public var smart: Bool { detail?.collection.smart ?? false }
    public var automatic: Bool { detail?.collection.automatic ?? false }
    /// Its category's title (the server's Other among them).
    public var category: String? { detail?.collection.category.map(\.title).flatMap { $0.isEmpty ? nil : $0 } }

    /// What it holds and in which order, in words: its filter's ("Genre:
    /// Comedy", "Newest first"), else its order ("By release date").
    public var notes: [String] {
        guard let collection = detail?.collection else { return [] }
        if let filter = collection.smartFilter, !filter.isEmpty { return SmartFilter.describe(filter) }
        return SmartFilter.sortNote(collection.sort).map { [$0] } ?? []
    }

    /// Its titles are movies, played in its order; else shows, which play their next episode.
    public var movies: Bool {
        guard let detail else { return true }
        if detail.items.contains(where: { $0.kind == .movie }) { return true }
        return detail.items.isEmpty && libraryKind != .shows
    }

    /// "2 movies", "3 shows".
    public var count: String {
        let count = detail?.collection.itemCount ?? 0
        return movies ? localized("\(count) movies") : localized("\(count) shows")
    }

    public var canPlay: Bool { !(detail?.items.isEmpty ?? true) }
    public var canShuffle: Bool { (detail?.items.count ?? 0) >= 2 }

    /// The server paths of its poster and backdrop.
    public var poster: [String] {
        detail.map { [$0.collection.posterUrl ?? "/api/collections/\($0.id)/images/poster"] } ?? []
    }

    public var backdrop: [String] {
        detail.map { [$0.collection.backdropUrl ?? "/api/collections/\($0.id)/images/backdrop"] } ?? []
    }

    private var libraryKind: LibraryKind? {
        guard let id = detail?.collection.libraryId else { return nil }
        return app.shell.libraries.first { $0.id == id }?.kind
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            let found = try await api.collection(collectionId)
            detail = found
            tiles = found.items.map { Tile($0, shape: .poster) }
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch PleyError.api(status: 404, _, _) {
            state = .notFound
        } catch {
            if detail == nil { state = .failed(describe(error)) } else { notice = describe(error) }
        }
    }

    /// What Play (`shuffle` false) or Shuffle starts; `nil` with a toast when nothing can play.
    public func play(shuffle: Bool) async -> PlayRequest? {
        guard let detail, !detail.items.isEmpty else { return nil }
        if movies {
            let videos = detail.items.filter { [.movie, .episode, .extra].contains($0.kind) }
            guard !videos.isEmpty else {
                app.shell.show(Self.nothingToPlay)
                return nil
            }
            let at = shuffle ? random(videos.count) : 0
            return PlayRequest(itemId: videos[at].id, queue: PlayQueue(kind: .collection, id: collectionId,
                                                                       itemIds: videos.map(\.id), position: at))
        }
        let pick = detail.items[shuffle ? random(detail.items.count) : 0]
        guard let request = await app.actions.playRequest(pick) else {
            app.shell.show(Self.nothingToPlay)
            return nil
        }
        return request
    }
}

/// The page of a Home row made of several libraries' collections of one
/// title, as the web's: the title, the libraries they come from (each
/// opening its own collection), Play and Shuffle, then every title in the
/// row's order. Movies play as a queue; shows play the first show's next
/// episode (Shuffle, a random show's).
@MainActor
public final class MergedCollectionModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
        case notFound
    }

    /// A library one of its collections is in, to open that collection.
    public struct Source: Identifiable, Hashable, Sendable {
        public let id: Int
        public let library: String
    }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var merged: MergedCollection?
    @Published public private(set) var tiles: [Tile] = []
    @Published public private(set) var notice: String?

    public let app: AppModel
    public let ids: [Int]
    var random: (Int) -> Int = { Int.random(in: 0..<$0) }
    private var listener: AnyCancellable?

    public init(app: AppModel, ids: [Int]) {
        self.app = app
        self.ids = ids
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, let merged = self.merged else { return }
            let shown = Set(self.tiles.map(\.id))
            guard change.everything || merged.collections.contains(where: { change.touches(library: $0.libraryId) })
                || !shown.isDisjoint(with: change.itemIds) else { return }
            Task { await self.load() }
        }
    }

    public var title: String { merged?.title ?? "" }

    /// Its collections' libraries, in the row's order; those this profile
    /// can't name are left out.
    public var sources: [Source] {
        (merged?.collections ?? []).compactMap { part in
            app.shell.libraries.first { $0.id == part.libraryId }.map { Source(id: part.id, library: $0.name) }
        }
    }

    private var movieItems: [ItemSummary] { (merged?.items ?? []).filter { $0.kind == .movie } }

    /// "3 movies", "2 shows", "3 movies · 2 shows".
    public var count: String {
        let movies = movieItems.count
        let shows = (merged?.items ?? []).filter { $0.kind == .show }.count
        return [movies > 0 ? localized("\(movies) movies") : nil, shows > 0 ? localized("\(shows) shows") : nil]
            .compactMap { $0 }.joined(separator: " · ")
    }

    public var canPlay: Bool { !(merged?.items.isEmpty ?? true) }
    public var canShuffle: Bool { (merged?.items.count ?? 0) >= 2 }

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            let found = try await api.mergedCollection(ids)
            merged = found
            tiles = found.items.map { Tile($0, shape: .poster) }
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch PleyError.api(status: 404, _, _) {
            state = .notFound
        } catch {
            if merged == nil { state = .failed(describe(error)) } else { notice = describe(error) }
        }
    }

    /// What Play (`shuffle` false) or Shuffle starts; `nil` with a toast when nothing can play.
    public func play(shuffle: Bool) async -> PlayRequest? {
        guard let merged, !merged.items.isEmpty else { return nil }
        let movies = movieItems
        if !movies.isEmpty {
            let at = shuffle ? random(movies.count) : 0
            return PlayRequest(itemId: movies[at].id, queue: PlayQueue(kind: .collection, id: ids.first ?? 0,
                                                                       itemIds: movies.map(\.id), position: at))
        }
        let pick = merged.items[shuffle ? random(merged.items.count) : 0]
        guard let request = await app.actions.playRequest(pick) else {
            app.shell.show(CollectionModel.nothingToPlay)
            return nil
        }
        return request
    }
}
