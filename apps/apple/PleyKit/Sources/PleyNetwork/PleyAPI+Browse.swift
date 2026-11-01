import Foundation
import PleyModel

/// A request without a body.
private struct Empty: Encodable, Sendable {}

/// What `libraryItems` asks for: the listing's sort, order and filters, and
/// the page (docs/API.md, `GET /api/libraries/{id}/items`).
public struct LibraryQuery: Hashable, Sendable {
    /// `title`, `year`, `added`, `rating`, `user_rating`, `release`, `duration`, `last_played`, `bitrate`.
    public var sort: String
    /// `asc` or `desc`.
    public var order: String
    /// `true` watched only, `false` unwatched only, `nil` both.
    public var watched: Bool?
    public var genre: String?
    public var year: Int?

    public init(sort: String = "title", order: String = "asc", watched: Bool? = nil, genre: String? = nil,
                year: Int? = nil) {
        self.sort = sort
        self.order = order
        self.watched = watched
        self.genre = genre
        self.year = year
    }

    /// The query of the page from `offset`, `limit` items long.
    public func items(offset: Int, limit: Int) -> [URLQueryItem] {
        var out = [URLQueryItem(name: "sort", value: sort), URLQueryItem(name: "order", value: order)]
        if let watched { out.append(URLQueryItem(name: "watched", value: watched ? "true" : "false")) }
        if let genre { out.append(URLQueryItem(name: "genre", value: genre)) }
        if let year { out.append(URLQueryItem(name: "year", value: String(year))) }
        out.append(URLQueryItem(name: "offset", value: String(offset)))
        out.append(URLQueryItem(name: "limit", value: String(limit)))
        return out
    }
}

/// Libraries, Home's and the libraries' rows, and the profile's own marks
/// (docs/API.md, "Libraries", "Home & search", "Items").
extension PleyAPI {
    /// `GET /api/libraries`: the libraries this profile may open, in its order.
    public func libraries() async throws -> [Library] {
        try await client.get("/api/libraries", as: [Library].self)
    }

    /// `GET /api/libraries/{id}`.
    public func library(_ id: Int) async throws -> Library {
        try await client.get("/api/libraries/\(id)", as: Library.self)
    }

    /// `GET /api/libraries/{id}/hubs`: its Recommended rows.
    public func libraryHubs(_ id: Int) async throws -> [Hub] {
        try await client.get("/api/libraries/\(id)/hubs", as: HomeHubs.self).hubs
    }

    /// `GET /api/libraries/{id}/items`: a page of the listing. With `tag`
    /// (the `ETag` of the page shown), `nil` means it is unchanged.
    public func libraryItems(_ id: Int, _ query: LibraryQuery, offset: Int, limit: Int,
                             tag: String? = nil) async throws -> (page: LibraryItems, tag: String?)? {
        guard let answer = try await client.get("/api/libraries/\(id)/items", query: query.items(offset: offset,
                                                                                                  limit: limit),
                                                ifNoneMatch: tag, as: LibraryItems.self)
        else { return nil }
        return (answer.value, answer.tag)
    }

    /// `GET /api/libraries/{id}/filters`: the genres and years to filter by.
    public func libraryFilters(_ id: Int) async throws -> LibraryFilters {
        try await client.get("/api/libraries/\(id)/filters", as: LibraryFilters.self)
    }

    /// `PUT /api/me/libraries/order`: the profile's own order, every
    /// library it may open once; `409` when they aren't the ones there are.
    public func setLibraryOrder(_ ids: [Int]) async throws -> [Library] {
        try await client.send("PUT", "/api/me/libraries/order", body: LibraryOrder(ids: ids), as: [Library].self)
    }

    /// `DELETE /api/me/libraries/order`: back to the server's order.
    public func resetLibraryOrder() async throws -> [Library] {
        try await client.send("DELETE", "/api/me/libraries/order", body: Empty?.none, as: [Library].self)
    }

    /// `POST /api/items/{id}/watched` or `…/unwatched`: a show or season, every episode.
    public func setWatched(_ id: Int, _ watched: Bool) async throws {
        _ = try await client.send("POST", "/api/items/\(id)/\(watched ? "watched" : "unwatched")", body: Empty?.none,
                                  as: NoContent.self)
    }

    /// `GET /api/watchlist`: My List, newest added first.
    public func watchlist() async throws -> [ItemSummary] {
        try await client.get("/api/watchlist", as: ItemList.self).items
    }

    /// `PUT /api/watchlist/{id}` or `DELETE`: a movie or show in My List, or out of it.
    public func setInWatchlist(_ id: Int, _ listed: Bool) async throws {
        _ = try await client.send(listed ? "PUT" : "DELETE", "/api/watchlist/\(id)", body: Empty?.none,
                                  as: NoContent.self)
    }

    /// `DELETE /api/continue-watching/{id}`: a movie, or an episode's whole
    /// show, out of Continue Watching until it plays again.
    public func removeFromContinueWatching(_ id: Int) async throws {
        _ = try await client.send("DELETE", "/api/continue-watching/\(id)", body: Empty?.none, as: NoContent.self)
    }

    // MARK: Item pages

    /// `GET /api/items/{id}/children`: a show's seasons, a season's
    /// episodes, an artist's albums, an album's tracks, by index.
    public func children(_ id: Int) async throws -> [ItemSummary] {
        try await client.get("/api/items/\(id)/children", as: ItemList.self).items
    }

    /// `GET /api/items/{id}/related`: More Like This.
    public func related(_ id: Int, limit: Int = 20) async throws -> [ItemSummary] {
        try await client.get("/api/items/\(id)/related", query: [URLQueryItem(name: "limit", value: String(limit))],
                             as: ItemList.self).items
    }

    /// `GET /api/items/{id}/collections`: the collections a movie or show is in.
    public func itemCollections(_ id: Int) async throws -> [CollectionRef] {
        try await client.get("/api/items/\(id)/collections", as: ItemCollections.self).collections
    }

    /// `PUT /api/items/{id}/rating`: the profile's own rating, 0 to 10 (two
    /// points a star); `nil` takes it away.
    public func setRating(_ id: Int, _ rating: Double?) async throws {
        _ = try await client.send("PUT", "/api/items/\(id)/rating", body: Rating(rating: rating), as: NoContent.self)
    }

    /// `POST /api/items/{id}/prefetch`: Play is about to be pressed, so the
    /// server reads ahead what it plays first.
    public func prefetch(_ id: Int) async throws {
        _ = try await client.send("POST", "/api/items/\(id)/prefetch", body: Empty?.none, as: NoContent.self)
    }

    /// `GET /api/people/{id}`: a person and their titles this profile may see.
    public func person(_ id: Int) async throws -> PersonDetail {
        try await client.get("/api/people/\(id)", as: PersonDetail.self)
    }
}

/// `{ rating }`, with `null` written out: no rating.
private struct Rating: Encodable, Sendable {
    let rating: Double?

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let rating { try c.encode(rating, forKey: .rating) } else { try c.encodeNil(forKey: .rating) }
    }

    private enum CodingKeys: String, CodingKey { case rating }
}
