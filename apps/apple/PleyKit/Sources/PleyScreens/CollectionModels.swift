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
        count = NumberFormatter.localizedString(from: NSNumber(value: collection.itemCount), number: .decimal)
        subtitle = Words.pluralize(collection.itemCount, kind == .shows ? "show" : "movie")
        smart = collection.smart
        automatic = collection.automatic
        pictures = collection.posterUrl.flatMap { $0.isEmpty ? nil : [$0] } ?? ["/api/collections/\(id)/images/poster"]
        spokenLabel = [title, smart ? "smart collection" : "collection", subtitle].joined(separator: ", ")
    }
}

/// One library's collections: its Collections tab, and the row at the end
/// of its Recommended tab. Music has none.
@MainActor
public final class CollectionsModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
    }

    public static let failedTitle = "Couldn't load the collections"
    public static let emptyTitle = "No collections in this library"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var cards: [CollectionCard] = []
    /// Why the last read failed while cards still show.
    @Published public private(set) var notice: String?

    public let app: AppModel
    public let libraryId: Int
    /// The library's kind, for the counts' nouns.
    public var kind: LibraryKind?

    public init(app: AppModel, libraryId: Int, kind: LibraryKind? = nil) {
        self.app = app
        self.libraryId = libraryId
        self.kind = kind
    }

    /// Whether it was read once.
    public var loaded: Bool { state == .loaded }

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            let found = try await api.collections(library: libraryId)
            cards = found.map { CollectionCard($0, kind: kind) }
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch {
            if state == .loaded { notice = describe(error) } else { state = .failed(describe(error)) }
        }
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

    public static let failedTitle = "Couldn't load this collection"
    public static let notFoundTitle = "This collection isn't available"
    public static let nothingToPlay = "Nothing to play"

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
        listener = app.$change.compactMap { $0 }.sink { [weak self] change in
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
        Words.pluralize(detail?.collection.itemCount ?? 0, movies ? "movie" : "show")
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
