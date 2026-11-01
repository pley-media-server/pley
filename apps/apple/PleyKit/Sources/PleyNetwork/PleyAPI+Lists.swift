import Foundation
import PleyModel

/// Collections and the profile's playlists (docs/API.md, "Collections",
/// "Playlists"). A collection's pictures come through `image(path:)`, which
/// follows the server's same-origin `307` to the item artwork it falls back to.
extension PleyAPI {
    // MARK: Collections

    /// `GET /api/libraries/{id}/collections`: a library's collections, by title.
    public func collections(library id: Int) async throws -> [CollectionSummary] {
        try await client.get("/api/libraries/\(id)/collections", as: CollectionList.self).collections
    }

    /// `GET /api/collections/{id}`: a collection and its items, in its order.
    public func collection(_ id: Int) async throws -> CollectionDetail {
        try await client.get("/api/collections/\(id)", as: CollectionDetail.self)
    }

    // MARK: Playlists

    /// `GET /api/playlists`: the profile's playlists, by title.
    public func playlists() async throws -> [Playlist] {
        try await client.get("/api/playlists", as: PlaylistList.self).playlists
    }

    /// `GET /api/playlists/{id}`: a playlist and its entries, in order.
    public func playlist(_ id: Int) async throws -> PlaylistDetail {
        try await client.get("/api/playlists/\(id)", as: PlaylistDetail.self)
    }

    /// `POST /api/playlists`: an ordinary playlist, of `kind` or (without
    /// one) of what its first item plays, holding `itemIds` (a show, season,
    /// album or artist adds its episodes or tracks).
    public func createPlaylist(title: String, kind: PlaylistKind? = nil, itemIds: [Int] = []) async throws
        -> PlaylistDetail {
        try await client.send("POST", "/api/playlists", body: NewPlaylist(title: title, kind: kind?.rawValue,
                                                                           itemIds: itemIds),
                              as: PlaylistDetail.self)
    }

    /// `POST /api/playlists`: a smart playlist playing what `filter` finds in library `libraryId`.
    public func createSmartPlaylist(title: String, libraryId: Int, filter: String) async throws -> PlaylistDetail {
        try await client.send("POST", "/api/playlists",
                              body: NewSmartPlaylist(title: title, libraryId: libraryId, smartFilter: filter),
                              as: PlaylistDetail.self)
    }

    /// `PATCH /api/playlists/{id}`: a new title.
    public func renamePlaylist(_ id: Int, title: String) async throws -> PlaylistDetail {
        try await client.send("PATCH", "/api/playlists/\(id)", body: Rename(title: title), as: PlaylistDetail.self)
    }

    /// `DELETE /api/playlists/{id}`.
    public func deletePlaylist(_ id: Int) async throws {
        _ = try await client.send("DELETE", "/api/playlists/\(id)", body: Nothing?.none, as: NoContent.self)
    }

    /// `POST /api/playlists/{id}/items`: how many entries were added; `400`
    /// when an item doesn't fit the playlist's kind.
    @discardableResult
    public func addToPlaylist(_ id: Int, itemIds: [Int]) async throws -> Int {
        try await client.send("POST", "/api/playlists/\(id)/items", body: ItemIds(itemIds: itemIds),
                              as: PlaylistAdded.self).added
    }

    /// `DELETE /api/playlists/{id}/items/{entry_id}`: one entry out.
    public func removeFromPlaylist(_ id: Int, entryId: Int) async throws {
        _ = try await client.send("DELETE", "/api/playlists/\(id)/items/\(entryId)", body: Nothing?.none,
                                  as: NoContent.self)
    }

    /// `PUT /api/playlists/{id}/order`: these entries first, in this order.
    public func reorderPlaylist(_ id: Int, entryIds: [Int]) async throws -> PlaylistDetail {
        try await client.send("PUT", "/api/playlists/\(id)/order", body: EntryIds(entryIds: entryIds),
                              as: PlaylistDetail.self)
    }
}

/// A request without a body.
private struct Nothing: Encodable, Sendable {}

private struct NewPlaylist: Encodable, Sendable {
    let title: String
    let kind: String?
    let itemIds: [Int]

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(title, forKey: .title)
        try c.encodeIfPresent(kind, forKey: .kind)
        if !itemIds.isEmpty { try c.encode(itemIds, forKey: .itemIds) }
    }

    private enum CodingKeys: String, CodingKey { case title, kind, itemIds }
}

private struct NewSmartPlaylist: Encodable, Sendable {
    let title: String
    let libraryId: Int
    let smartFilter: String
}

private struct Rename: Encodable, Sendable {
    let title: String
}

private struct ItemIds: Encodable, Sendable {
    let itemIds: [Int]
}

private struct EntryIds: Encodable, Sendable {
    let entryIds: [Int]
}
