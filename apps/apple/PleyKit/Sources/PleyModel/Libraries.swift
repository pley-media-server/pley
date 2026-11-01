import Foundation

/// What a library holds (docs/API.md, `LibraryKind`).
public struct LibraryKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let movies = LibraryKind(rawValue: "movies")
    public static let shows = LibraryKind(rawValue: "shows")
    public static let music = LibraryKind(rawValue: "music")
}

/// A library (docs/API.md, `Library`), as `GET /api/libraries` lists them
/// in the profile's order.
public struct Library: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var kind: LibraryKind
    /// Top-level items (movies, shows, artists) this profile can see.
    public var itemCount: Int
    /// Unix seconds; `nil` before its first scan.
    public var scannedAt: Int?
    /// A scan is queued or running.
    public var scanning: Bool

    public init(id: Int, name: String, kind: LibraryKind, itemCount: Int = 0, scannedAt: Int? = nil,
                scanning: Bool = false) {
        self.id = id
        self.name = name
        self.kind = kind
        self.itemCount = itemCount
        self.scannedAt = scannedAt
        self.scanning = scanning
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        kind = try c.value(.kind, or: LibraryKind(rawValue: ""))
        itemCount = try c.value(.itemCount, or: 0)
        scannedAt = try c.decodeIfPresent(Int.self, forKey: .scannedAt)
        scanning = try c.value(.scanning, or: false)
    }
}

/// `{ total, items }`: a page of `GET /api/libraries/{id}/items`.
public struct LibraryItems: Codable, Hashable, Sendable {
    /// Every item the listing has, not just this page's.
    public var total: Int
    public var items: [ItemSummary]

    public init(total: Int, items: [ItemSummary]) {
        self.total = total
        self.items = items
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        items = try c.value(.items, or: [])
        total = try c.value(.total, or: items.count)
    }
}

/// `{ genres, years }`: what a library's items can be filtered by
/// (`GET /api/libraries/{id}/filters`).
public struct LibraryFilters: Codable, Hashable, Sendable {
    public var genres: [String]
    /// Newest first.
    public var years: [Int]

    public init(genres: [String] = [], years: [Int] = []) {
        self.genres = genres
        self.years = years
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        genres = try c.value(.genres, or: [])
        years = try c.value(.years, or: [])
    }
}

/// `{ ids }`: a library order (`PUT /api/me/libraries/order`).
public struct LibraryOrder: Codable, Hashable, Sendable {
    public var ids: [Int]

    public init(ids: [Int]) { self.ids = ids }
}

/// `{ items }`: a list of items (My List, an item's children, related titles).
public struct ItemList: Codable, Hashable, Sendable {
    public var items: [ItemSummary]

    public init(items: [ItemSummary]) { self.items = items }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        items = try c.value(.items, or: [])
    }
}
