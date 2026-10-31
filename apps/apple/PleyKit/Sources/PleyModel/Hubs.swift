import Foundation

/// `{ hubs }`: `GET /api/hubs/home` (Continue Watching, My List, then
/// Recently Added of each library in the libraries' order).
public struct HomeHubs: Codable, Hashable, Sendable {
    public var hubs: [Hub]

    public init(hubs: [Hub]) { self.hubs = hubs }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        hubs = try c.value(.hubs, or: [])
    }
}

/// A row of the home page or of a library's front page. Empty rows are
/// left out by the server.
public struct Hub: Codable, Hashable, Sendable, Identifiable {
    /// `continue_watching`, `my_list`, `recently_added:<library id>`…
    public var key: String
    public var title: String
    /// `nil` for the home page's Continue Watching and My List.
    public var libraryId: Int?
    public var items: [ItemSummary]

    public var id: String { key }

    public init(key: String, title: String, libraryId: Int? = nil, items: [ItemSummary] = []) {
        self.key = key
        self.title = title
        self.libraryId = libraryId
        self.items = items
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        key = try c.decode(String.self, forKey: .key)
        title = try c.decode(String.self, forKey: .title)
        libraryId = try c.decodeIfPresent(Int.self, forKey: .libraryId)
        items = try c.value(.items, or: [])
    }
}

/// The profile's own state for an item.
public struct UserState: Codable, Hashable, Sendable {
    /// Where to resume, in ms; 0 when there's nothing to resume.
    public var positionMs: Int
    /// Shows and seasons: every episode is watched.
    public var watched: Bool
    public var playCount: Int
    /// Unix seconds.
    public var lastPlayedAt: Int?
    /// The profile's own rating, 0 to 10 (two points per star).
    public var rating: Double?

    public init(positionMs: Int = 0, watched: Bool = false, playCount: Int = 0, lastPlayedAt: Int? = nil,
                rating: Double? = nil) {
        self.positionMs = positionMs
        self.watched = watched
        self.playCount = playCount
        self.lastPlayedAt = lastPlayedAt
        self.rating = rating
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        positionMs = try c.value(.positionMs, or: 0)
        watched = try c.value(.watched, or: false)
        playCount = try c.value(.playCount, or: 0)
        lastPlayedAt = try c.decodeIfPresent(Int.self, forKey: .lastPlayedAt)
        rating = try c.decodeIfPresent(Double.self, forKey: .rating)
    }
}

/// A movie's best file, or a show's episodes together; only in library
/// listings sorted by bitrate.
public struct MediaSummary: Codable, Hashable, Sendable {
    public var bitrate: Int?
    public var size: Int?
    public var width: Int?
    public var height: Int?
    public var videoCodec: String?
    /// `sdr`, `hdr10`, `hdr10plus`, `hlg` or `dolby_vision`; `nil` for shows.
    public var dynamicRange: String?

    public init(bitrate: Int? = nil, size: Int? = nil, width: Int? = nil, height: Int? = nil, videoCodec: String? = nil,
                dynamicRange: String? = nil) {
        self.bitrate = bitrate
        self.size = size
        self.width = width
        self.height = height
        self.videoCodec = videoCodec
        self.dynamicRange = dynamicRange
    }
}

/// An item in a list (docs/API.md, `ItemSummary`). Picture addresses are
/// server-relative (`/api/items/12/images/poster?v=…`).
public struct ItemSummary: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var libraryId: Int
    public var kind: ItemKind
    public var title: String
    public var sortTitle: String
    public var year: Int?
    /// Season number, episode number or track number.
    public var index: Int?
    /// The season of an episode, the show of a season, the album of a track, the artist of an album.
    public var parentId: Int?
    public var parentTitle: String?
    /// An episode's season number; a track's disc number.
    public var parentIndex: Int?
    /// The show of an episode, the artist of a track.
    public var grandparentId: Int?
    public var grandparentTitle: String?
    public var durationMs: Int?
    /// The critics' rating, 0 to 10.
    public var rating: Double?
    /// Unix seconds.
    public var addedAt: Int
    public var posterUrl: String?
    public var backdropUrl: String?
    /// Episode stills and video frame grabs (16:9).
    public var thumbUrl: String?
    public var userState: UserState
    /// Shows and seasons: episodes; artists and albums: tracks.
    public var leafCount: Int?
    /// Shows and seasons: watched episodes.
    public var viewedLeafCount: Int?
    /// Shows: seasons; artists: albums.
    public var childCount: Int?
    /// In listings only for episodes.
    public var summary: String?
    /// `YYYY-MM-DD`.
    public var releaseDate: String?
    /// Kind `extra` only: `trailer`, `featurette`…
    public var extraType: String?
    public var media: MediaSummary?

    public init(id: Int, libraryId: Int, kind: ItemKind, title: String) {
        self.id = id
        self.libraryId = libraryId
        self.kind = kind
        self.title = title
        sortTitle = title
        addedAt = 0
        userState = UserState()
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        libraryId = try c.decode(Int.self, forKey: .libraryId)
        kind = try c.decode(ItemKind.self, forKey: .kind)
        title = try c.decode(String.self, forKey: .title)
        sortTitle = try c.value(.sortTitle, or: "")
        year = try c.decodeIfPresent(Int.self, forKey: .year)
        index = try c.decodeIfPresent(Int.self, forKey: .index)
        parentId = try c.decodeIfPresent(Int.self, forKey: .parentId)
        parentTitle = try c.decodeIfPresent(String.self, forKey: .parentTitle)
        parentIndex = try c.decodeIfPresent(Int.self, forKey: .parentIndex)
        grandparentId = try c.decodeIfPresent(Int.self, forKey: .grandparentId)
        grandparentTitle = try c.decodeIfPresent(String.self, forKey: .grandparentTitle)
        durationMs = try c.decodeIfPresent(Int.self, forKey: .durationMs)
        rating = try c.decodeIfPresent(Double.self, forKey: .rating)
        addedAt = try c.value(.addedAt, or: 0)
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        backdropUrl = try c.decodeIfPresent(String.self, forKey: .backdropUrl)
        thumbUrl = try c.decodeIfPresent(String.self, forKey: .thumbUrl)
        userState = try c.value(.userState, or: UserState())
        leafCount = try c.decodeIfPresent(Int.self, forKey: .leafCount)
        viewedLeafCount = try c.decodeIfPresent(Int.self, forKey: .viewedLeafCount)
        childCount = try c.decodeIfPresent(Int.self, forKey: .childCount)
        summary = try c.decodeIfPresent(String.self, forKey: .summary)
        releaseDate = try c.decodeIfPresent(String.self, forKey: .releaseDate)
        extraType = try c.decodeIfPresent(String.self, forKey: .extraType)
        media = try c.decodeIfPresent(MediaSummary.self, forKey: .media)
    }
}
