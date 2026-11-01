import Combine
import Foundation
import PleyModel
import PleyNetwork

/// An artist's page (Android's `artistPage`): their picture, "4 albums ·
/// 40 tracks", what the server says of them, and their albums newest
/// first. Music plays from a later version, so it has no Play.
public struct ArtistPage: Hashable, Sendable {
    public var id: Int
    public var title: String
    public var posters: [String]
    public var backdrops: [String]
    public var summary: String?
    public var genres: [String]
    /// "4 albums · 40 tracks"; empty when the server doesn't say.
    public var counts: String
    /// Square tiles, newest first, then by title.
    public var albums: [Tile]
    /// While the albums load.
    public var albumsLoading: Bool

    public init(_ detail: ItemDetail, albums: [ItemSummary]?) {
        let sorted = (albums ?? []).sorted { a, b in
            if (a.year ?? 0) != (b.year ?? 0) { return (a.year ?? 0) > (b.year ?? 0) }
            let x = a.sortTitle.isEmpty ? a.title : a.sortTitle, y = b.sortTitle.isEmpty ? b.title : b.sortTitle
            return x.localizedCaseInsensitiveCompare(y) == .orderedAscending
        }
        let count = detail.summary.childCount ?? albums?.count
        id = detail.id
        title = detail.title
        posters = Tile.posters(detail.summary)
        backdrops = ItemPage.backdrops(detail.summary)
        summary = detail.summary.summary.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        genres = detail.genres
        counts = [count.map { Words.pluralize($0, "album") }, detail.summary.leafCount.map { Words.pluralize($0, "track") }]
            .compactMap { $0 }.joined(separator: " · ")
        self.albums = sorted.map { Tile($0, shape: .square, line: $0.year.map(String.init) ?? "") }
        albumsLoading = albums == nil
    }
}

/// One line of an album's tracks: a disc's heading, or a track.
public enum AlbumRow: Hashable, Sendable, Identifiable {
    /// "Disc 2", above its tracks, on albums of several discs.
    case disc(Int)
    case track(TrackRow)

    public var id: String {
        switch self {
        case .disc(let n): "disc-\(n)"
        case .track(let track): "track-\(track.id)"
        }
    }
}

/// A track of an album, its words worked out.
public struct TrackRow: Hashable, Sendable, Identifiable {
    public var id: Int
    /// Its place in the album's tracks.
    public var position: Int
    /// Its number on the disc, "2"; empty without one.
    public var number: String
    public var title: String
    /// The track's own artist, when it isn't the album's.
    public var artist: String?
    /// "4:05".
    public var duration: String
    /// "Track 2, The Whole Universe Wants to Be Touched, 4 minutes 5 seconds".
    public var spoken: String
}

/// An album's page (Android's `albumPage`): its cover, its artist (a link
/// to their page), the year, "10 tracks · 42m", the profile's rating and
/// the tracks, with disc headings when there are several discs. The track
/// it was opened from is `highlight`. No Play: music plays from a later
/// version.
public struct AlbumPage: Hashable, Sendable {
    public var id: Int
    public var title: String
    public var posters: [String]
    public var artist: String?
    public var artistId: Int?
    public var year: Int?
    /// "10 tracks · 42m".
    public var meta: String
    /// 1 to 10; `nil`: unrated.
    public var userRating: Int?
    public var summary: String?
    public var genres: [String]
    public var rows: [AlbumRow]
    public var tracksLoading: Bool
    /// The track the album was opened from.
    public var highlight: Int?

    public init(_ detail: ItemDetail, tracks: [ItemSummary]?, highlight: Int? = nil) {
        let list = tracks ?? []
        let total = list.reduce(0) { $0 + ($1.durationMs ?? 0) }
        id = detail.id
        title = detail.title
        posters = Tile.posters(detail.summary)
        artist = detail.summary.parentTitle.flatMap { $0.isEmpty ? nil : $0 }
        artistId = detail.summary.parentId
        year = detail.year
        meta = [list.isEmpty ? nil : Words.pluralize(list.count, "track"),
                total > 0 ? Words.duration(ms: total) : nil].compactMap { $0 }.joined(separator: " · ")
        userRating = detail.userState.rating.map { Int($0.rounded()) }.flatMap { $0 > 0 ? min($0, 10) : nil }
        summary = detail.summary.summary.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        genres = detail.genres
        rows = Self.rows(list, albumArtist: artist)
        tracksLoading = tracks == nil
        self.highlight = highlight
    }

    /// Disc headings (with several discs) and the tracks, in order.
    public static func rows(_ tracks: [ItemSummary], albumArtist: String?) -> [AlbumRow] {
        let several = Set(tracks.map { $0.parentIndex ?? 1 }).count > 1
        var out: [AlbumRow] = []
        var disc: Int?
        for (n, t) in tracks.enumerated() {
            let d = t.parentIndex ?? 1
            if several && d != disc { out.append(.disc(d)) }
            disc = d
            let artist = t.grandparentTitle.flatMap { $0.isEmpty || $0 == albumArtist ? nil : $0 }
            let duration = (t.durationMs ?? 0) > 0 ? Words.clock(ms: t.durationMs!) : ""
            let spoken = [t.index.map { "Track \($0)" }, t.title, artist, Words.spokenDuration(ms: t.durationMs)]
                .compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: ", ")
            out.append(.track(TrackRow(id: t.id, position: n, number: t.index.map(String.init) ?? "", title: t.title,
                                       artist: artist, duration: duration, spoken: spoken)))
        }
        return out
    }
}

/// An artist's page: the artist (`GET /api/items/{id}`) and their albums (`/children`).
@MainActor
public final class ArtistModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded, notFound
        case failed(String)
    }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var page: ArtistPage?

    public let app: AppModel
    public let itemId: Int

    public init(app: AppModel, itemId: Int) {
        self.app = app
        self.itemId = itemId
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            let id = itemId
            async let detail = api.item(id)
            async let albums = try? await api.children(id)
            let found = try await detail
            page = ArtistPage(found, albums: await albums)
            state = .loaded
        } catch is CancellationError {
        } catch PleyError.signedOut {
        } catch PleyError.api(status: 404, _, _) {
            page = nil
            state = .notFound
        } catch {
            if page == nil { state = .failed(describe(error)) }
        }
    }
}

/// An album's page: the album (`GET /api/items/{id}`), its tracks
/// (`/children`), and the profile's rating of it, shown at once and
/// undone when the server refuses.
@MainActor
public final class AlbumModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded, notFound
        case failed(String)
    }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var page: AlbumPage?

    public let app: AppModel
    public let itemId: Int
    /// The track the album was opened from.
    public let track: Int?
    private var detail: ItemDetail?
    private var tracks: [ItemSummary]?

    public init(app: AppModel, itemId: Int, track: Int? = nil) {
        self.app = app
        self.itemId = itemId
        self.track = track
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            let id = itemId
            async let found = api.item(id)
            async let list = try? await api.children(id)
            detail = try await found
            tracks = await list
            rebuild()
            state = .loaded
        } catch is CancellationError {
        } catch PleyError.signedOut {
        } catch PleyError.api(status: 404, _, _) {
            page = nil
            state = .notFound
        } catch {
            if page == nil { state = .failed(describe(error)) }
        }
    }

    /// The profile's rating, 1 to 10; `nil` or 0 takes it away.
    public func rate(_ rating: Int?) async {
        guard let api = await app.session.api, detail != nil else { return }
        let value = rating.flatMap { $0 > 0 ? min($0, 10) : nil }
        let before = detail?.summary.userState.rating
        detail?.summary.userState.rating = value.map(Double.init)
        rebuild()
        do {
            try await api.setRating(itemId, value.map(Double.init))
        } catch {
            detail?.summary.userState.rating = before
            rebuild()
            app.shell.show("Couldn't save your rating: \(describe(error))")
        }
    }

    private func rebuild() {
        guard let detail else { return }
        page = AlbumPage(detail, tracks: tracks, highlight: track)
    }
}
