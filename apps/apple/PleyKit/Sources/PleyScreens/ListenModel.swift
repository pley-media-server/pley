import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// The iPhone's Listen tab (and the iPad's in a narrow window), which
/// takes the place of Playlists: Music, the profile's listening. Recently
/// Played is the albums of its latest tracks in History (`GET /api/history`),
/// newest first; Playlists are all of its playlists, music and video alike
/// (a video one marked so), with New playlist; Artists are the first music
/// library's, the most recently played first; then each music library's
/// own page. A part with nothing in it is left out. Audiobooks join it
/// once the server lets Pley's apps open audiobook libraries; until then
/// Listen is Music alone.
@MainActor
public final class ListenModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
    }

    /// How many recent albums and artists show.
    public static let rowLimit = 12
    /// How many plays are read for Recently Played.
    public static let historyLimit = 60

    @Published public private(set) var state: State = .loading
    /// Recently Played, as album tiles.
    @Published public private(set) var recent: [Tile] = []
    @Published public private(set) var artists: [Tile] = []
    /// The profile's music libraries, in its order.
    @Published public private(set) var libraries: [Library] = []
    public let playlists: PlaylistsModel

    public static var failedTitle: String { localized("Couldn't load your music") }
    public static var emptyTitle: String { localized("No music yet") }

    private unowned let app: AppModel
    private var listener: AnyCancellable?

    public init(app: AppModel) {
        self.app = app
        playlists = PlaylistsModel(app: app)
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, change.home || change.everything else { return }
            Task { await self.load() }
        }
    }

    /// Whether there is anything to show.
    public var isEmpty: Bool {
        recent.isEmpty && artists.isEmpty && libraries.isEmpty && playlists.cards.isEmpty
    }

    /// Reads the history, the first music library's artists and the playlists.
    public func load() async {
        guard let api = await app.session.api else { return }
        libraries = app.shell.libraries.filter { $0.kind == .music }
        async let lists: Void = playlists.load()
        async let history = try? api.history(offset: 0, limit: Self.historyLimit)
        var found: [ItemSummary] = []
        if let first = libraries.first {
            let query = LibraryQuery(sort: LibrarySort.lastPlayed.rawValue, order: "desc")
            found = (try? await api.libraryItems(first.id, query, offset: 0, limit: Self.rowLimit))??.page.items ?? []
        }
        let plays = await history
        _ = await lists
        recent = Self.recentAlbums(plays?.entries ?? []).map { Tile($0, shape: .square) }
        artists = found.filter { $0.kind == .artist }.map { Tile($0, shape: .square) }
        if plays == nil && libraries.isEmpty && playlists.cards.isEmpty, case .failed(let why) = playlists.state {
            state = .failed(why)
        } else {
            state = .loaded
        }
    }

    /// The albums of the tracks in `entries`, newest play first, each once,
    /// `rowLimit` at most: an album as its tile shows it, its artist under it.
    public static func recentAlbums(_ entries: [HistoryEntry]) -> [ItemSummary] {
        var seen = Set<Int>()
        var out: [ItemSummary] = []
        for entry in entries.sorted(by: { $0.viewedAt > $1.viewedAt }) {
            let track = entry.item
            guard track.kind == .track, let album = track.parentId, seen.insert(album).inserted else { continue }
            var item = ItemSummary(id: album, libraryId: track.libraryId, kind: .album,
                                   title: track.parentTitle ?? track.title)
            item.posterUrl = "/api/items/\(album)/images/poster"
            item.parentId = track.grandparentId
            item.parentTitle = track.grandparentTitle
            out.append(item)
            if out.count == rowLimit { break }
        }
        return out
    }
}
