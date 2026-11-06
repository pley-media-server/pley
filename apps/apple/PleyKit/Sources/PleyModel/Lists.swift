import Foundation

/// How a collection orders its items (docs/API.md, `Collection.sort`).
public struct CollectionSort: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let release = CollectionSort(rawValue: "release")
    public static let title = CollectionSort(rawValue: "title")
    public static let custom = CollectionSort(rawValue: "custom")
}

/// A collection (docs/API.md, `Collection`): a TMDB one (`automatic`), a
/// hand-made one, or a smart one filled by `smartFilter`.
public struct CollectionSummary: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var libraryId: Int
    public var title: String
    public var summary: String?
    /// Mirrors a TMDB collection.
    public var automatic: Bool
    public var sort: CollectionSort
    /// A smart collection's library filter, in the compatibility API's syntax.
    public var smartFilter: String?
    /// The items this profile can see.
    public var itemCount: Int
    public var posterUrl: String?
    public var backdropUrl: String?

    public init(id: Int, libraryId: Int, title: String, summary: String? = nil, automatic: Bool = false,
                sort: CollectionSort = .release, smartFilter: String? = nil, itemCount: Int = 0,
                posterUrl: String? = nil, backdropUrl: String? = nil) {
        self.id = id
        self.libraryId = libraryId
        self.title = title
        self.summary = summary
        self.automatic = automatic
        self.sort = sort
        self.smartFilter = smartFilter
        self.itemCount = itemCount
        self.posterUrl = posterUrl
        self.backdropUrl = backdropUrl
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        libraryId = try c.value(.libraryId, or: 0)
        title = try c.value(.title, or: "")
        summary = try c.decodeIfPresent(String.self, forKey: .summary)
        automatic = try c.value(.automatic, or: false)
        sort = try c.value(.sort, or: CollectionSort(rawValue: ""))
        smartFilter = try c.decodeIfPresent(String.self, forKey: .smartFilter)
        itemCount = try c.value(.itemCount, or: 0)
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        backdropUrl = try c.decodeIfPresent(String.self, forKey: .backdropUrl)
    }

    /// Filled by a filter: the Smart badge.
    public var smart: Bool { !(smartFilter ?? "").isEmpty }
}

/// `{ collections }`: `GET /api/libraries/{id}/collections`, by title.
public struct CollectionList: Codable, Hashable, Sendable {
    public var collections: [CollectionSummary]

    public init(collections: [CollectionSummary]) { self.collections = collections }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        collections = try c.value(.collections, or: [])
    }
}

/// A collection with its items, in its order (`GET /api/collections/{id}`).
public struct CollectionDetail: Codable, Hashable, Sendable, Identifiable {
    public var collection: CollectionSummary
    public var items: [ItemSummary]
    public var id: Int { collection.id }

    public init(collection: CollectionSummary, items: [ItemSummary]) {
        self.collection = collection
        self.items = items
    }

    public init(from decoder: any Decoder) throws {
        collection = try CollectionSummary(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        items = try c.value(.items, or: [])
    }

    public func encode(to encoder: any Encoder) throws {
        try collection.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(items, forKey: .items)
    }

    private enum CodingKeys: String, CodingKey { case items }
}

/// What a playlist plays: movies and episodes, or tracks.
public struct PlaylistKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let video = PlaylistKind(rawValue: "video")
    public static let audio = PlaylistKind(rawValue: "audio")
}

/// One of the profile's playlists (docs/API.md, `Playlist`).
public struct Playlist: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var title: String
    public var kind: PlaylistKind
    /// Visible entries.
    public var itemCount: Int
    public var durationMs: Int
    /// The pictures of its first entries (at most 4), for a mosaic.
    public var artwork: [String]
    /// A smart playlist's filter of `libraryId`.
    public var smartFilter: String?
    public var libraryId: Int?

    public init(id: Int, title: String, kind: PlaylistKind = .video, itemCount: Int = 0, durationMs: Int = 0,
                artwork: [String] = [], smartFilter: String? = nil, libraryId: Int? = nil) {
        self.id = id
        self.title = title
        self.kind = kind
        self.itemCount = itemCount
        self.durationMs = durationMs
        self.artwork = artwork
        self.smartFilter = smartFilter
        self.libraryId = libraryId
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        title = try c.value(.title, or: "")
        kind = try c.value(.kind, or: .video)
        itemCount = try c.value(.itemCount, or: 0)
        durationMs = try c.value(.durationMs, or: 0)
        artwork = try c.value(.artwork, or: [])
        smartFilter = try c.decodeIfPresent(String.self, forKey: .smartFilter)
        libraryId = try c.decodeIfPresent(Int.self, forKey: .libraryId)
    }

    /// Filled by its filter: read-only.
    public var smart: Bool { !(smartFilter ?? "").isEmpty }
}

/// `{ playlists }`: `GET /api/playlists`, by title.
public struct PlaylistList: Codable, Hashable, Sendable {
    public var playlists: [Playlist]

    public init(playlists: [Playlist]) { self.playlists = playlists }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        playlists = try c.value(.playlists, or: [])
    }
}

/// An entry of a playlist: an item may be in it more than once, each its own entry.
public struct PlaylistEntry: Codable, Hashable, Sendable, Identifiable {
    public var entryId: Int
    public var item: ItemSummary
    public var id: Int { entryId }

    public init(entryId: Int, item: ItemSummary) {
        self.entryId = entryId
        self.item = item
    }
}

/// A playlist with its entries, in its order (`GET /api/playlists/{id}`).
public struct PlaylistDetail: Codable, Hashable, Sendable, Identifiable {
    public var playlist: Playlist
    public var entries: [PlaylistEntry]
    public var id: Int { playlist.id }

    public init(playlist: Playlist, entries: [PlaylistEntry]) {
        self.playlist = playlist
        self.entries = entries
    }

    public init(from decoder: any Decoder) throws {
        playlist = try Playlist(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        entries = try c.value(.entries, or: [])
    }

    public func encode(to encoder: any Encoder) throws {
        try playlist.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(entries, forKey: .entries)
    }

    private enum CodingKeys: String, CodingKey { case entries }
}

/// `{ added }`: how many entries `POST /api/playlists/{id}/items` added.
public struct PlaylistAdded: Codable, Hashable, Sendable {
    public var added: Int

    public init(added: Int) { self.added = added }
}

extension Words {
    /// "12 items · 3h 12m", "1 item"; the length left out without one.
    public static func playlistMeta(count: Int, durationMs: Int) -> String {
        [localized("\(count) items"), duration(ms: durationMs)].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// A title in full, as Add to playlist names it: "The Office — S2 · E3 —
    /// Office Olympics", "The Office — Season 2", "One More Time — Daft
    /// Punk", "Heat (1995)".
    public static func fullTitle(_ item: ItemSummary) -> String {
        switch item.kind {
        case .episode:
            return [item.grandparentTitle ?? "", episodeLabel(season: item.parentIndex, episode: item.index), item.title]
                .filter { !$0.isEmpty }.joined(separator: " — ")
        case .season:
            return [item.parentTitle ?? "", seasonTitle(item.title, index: item.index)]
                .filter { !$0.isEmpty }.joined(separator: " — ")
        case .track:
            return [item.title, item.grandparentTitle ?? ""].filter { !$0.isEmpty }.joined(separator: " — ")
        default:
            return item.year.map { "\(item.title) (\($0))" } ?? item.title
        }
    }
}
