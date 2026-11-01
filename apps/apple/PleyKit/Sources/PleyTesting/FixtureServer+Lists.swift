import Foundation
import Network
import PleyModel

/// Collections and playlists: the fixtures' collections (Dune, a smart one,
/// Villeneuve Picks) and playlists (Weekend, a smart one, a music one),
/// kept as the server keeps them, so a test can make, rename, fill, reorder,
/// empty and delete playlists and read back what the server has.
extension FixtureServer {
    struct FixtureCollection: Sendable {
        var id: Int
        var libraryId: Int
        var title: String
        var summary: String?
        var automatic = false
        var sort = "release"
        var smartFilter: String?
        var itemIds: [Int]
    }

    struct FixturePlaylist: Sendable {
        var id: Int
        var title: String
        var kind: String
        var smartFilter: String?
        var libraryId: Int?
        /// Entry ids and the items they hold, in order.
        var entries: [(entry: Int, item: Int)]
    }

    struct Lists {
        /// `nil` until first asked for: then the fixtures'.
        var collections: [FixtureCollection]?
        var playlists: [FixturePlaylist]?
        var nextPlaylist = 950
        var nextEntry = 9_500
        /// `PUT …/order` answers this status from now on (`nil`: normally).
        var orderRefusal: Int?
        /// `DELETE …/items/{entry}` answers this status from now on.
        var removeRefusal: Int?
        /// Every order sent, as entry ids.
        var orders: [[Int]] = []
        /// Every entry removal, in order.
        var removals: [Int] = []
    }

    // MARK: Knobs

    /// Adds a collection of `itemIds` to library `libraryId`.
    public func addCollection(id: Int, libraryId: Int, title: String, itemIds: [Int]) {
        state.withLock { s in
            Self.seed(&s)
            s.lists.collections?.append(FixtureCollection(id: id, libraryId: libraryId, title: title, itemIds: itemIds))
        }
    }

    /// Adds an ordinary video playlist holding `itemIds`; its entries are numbered from `firstEntry`.
    public func addPlaylist(id: Int, title: String, itemIds: [Int], firstEntry: Int) {
        state.withLock { s in
            Self.seed(&s)
            s.lists.playlists?.append(FixturePlaylist(
                id: id, title: title, kind: "video", entries: itemIds.enumerated().map { (firstEntry + $0, $1) }))
        }
    }

    /// `PUT /api/playlists/{id}/order` answers this status from now on (`nil`: normally).
    public func refusePlaylistOrder(status: Int?) {
        state.withLock { $0.lists.orderRefusal = status }
    }

    /// `DELETE /api/playlists/{id}/items/{entry}` answers this status from now on (`nil`: normally).
    public func refusePlaylistRemoval(status: Int?) {
        state.withLock { $0.lists.removeRefusal = status }
    }

    /// The orders sent, as entry ids, oldest first.
    public var playlistOrders: [[Int]] { state.withLock { $0.lists.orders } }

    /// The entries removed, oldest first.
    public var playlistRemovals: [Int] { state.withLock { $0.lists.removals } }

    /// Playlist `id`'s entries now, as (entry, item); `nil` when there is no such playlist.
    public func playlistEntries(_ id: Int) -> [(entry: Int, item: Int)]? {
        state.withLock { s in
            Self.seed(&s)
            return s.lists.playlists?.first { $0.id == id }?.entries
        }
    }

    /// Playlist `id`'s title now.
    public func playlistTitle(_ id: Int) -> String? {
        state.withLock { s in
            Self.seed(&s)
            return s.lists.playlists?.first { $0.id == id }?.title
        }
    }

    /// The playlists' ids now, by title.
    public var playlistIds: [String: Int] {
        state.withLock { s in
            Self.seed(&s)
            return Dictionary((s.lists.playlists ?? []).map { ($0.title, $0.id) }, uniquingKeysWith: { a, _ in a })
        }
    }

    /// Playlist `id`'s smart filter and library, as made.
    public func playlistFilter(_ id: Int) -> (filter: String?, library: Int?)? {
        state.withLock { s in
            Self.seed(&s)
            return s.lists.playlists?.first { $0.id == id }.map { ($0.smartFilter, $0.libraryId) }
        }
    }

    // MARK: Routes

    /// The collections' and playlists' routes; `nil` for the others.
    static func lists(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api", parts[1] == "collections" || parts[1] == "playlists" else {
            return nil
        }
        seed(&s)
        let method = request.method
        if parts[1] == "collections" {
            guard parts.count >= 3, let id = Int(parts[2]) else { return nil }
            switch (method, parts.count) {
            case ("GET", 3):
                return withToken(&s, request, scope: "library") { s, _ in
                    guard let collection = s.lists.collections?.first(where: { $0.id == id }) else {
                        return notFound(request.path)
                    }
                    var out = collectionJSON(s, collection)
                    out["items"] = collection.itemIds.compactMap { itemJSON(s, $0) }
                    return json(200, out)
                }
            case ("GET", 5) where parts[3] == "images" && ["poster", "backdrop"].contains(parts[4]):
                // No artwork of its own: the first item's, as the server's 307 does.
                return withToken(&s, request, scope: "library") { s, _ in
                    guard let collection = s.lists.collections?.first(where: { $0.id == id }),
                          let first = collection.itemIds.first
                    else { return notFound(request.path) }
                    let query = request.query.isEmpty ? "" : "?" + request.query.sorted { $0.key < $1.key }
                        .map { "\($0.key)=\($0.value)" }.joined(separator: "&")
                    return Answer(status: 307, headers: ["Location": "/api/items/\(first)/images/\(parts[4])\(query)"],
                                  body: Data())
                }
            default:
                return nil
            }
        }
        switch (method, parts.count) {
        case ("GET", 2):
            return withToken(&s, request, scope: "library") { s, _ in
                let sorted = (s.lists.playlists ?? []).sorted {
                    $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                }
                return json(200, ["playlists": sorted.map { playlistJSON(s, $0) }])
            }
        case ("POST", 2):
            return withToken(&s, request, scope: "playlists") { s, _ in
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                let title = (body["title"] as? String ?? "").trimmingCharacters(in: .whitespaces)
                guard !title.isEmpty, title.count <= 200 else {
                    return json(400, ["code": "bad_request", "message": "a title of 1 to 200 characters"])
                }
                s.lists.nextPlaylist += 1
                var playlist = FixturePlaylist(id: s.lists.nextPlaylist, title: title, kind: "video", entries: [])
                if let filter = body["smart_filter"] as? String {
                    playlist.smartFilter = filter
                    playlist.libraryId = body["library_id"] as? Int
                    let found = object("libraries/items.json")?["items"] as? [[String: Any]] ?? []
                    playlist.entries = found.prefix(2).compactMap { $0["id"] as? Int }.map { ($0, $0) }
                } else {
                    let ids = body["item_ids"] as? [Int] ?? []
                    let leaves = ids.flatMap { leavesOf(s, $0) }
                    playlist.kind = body["kind"] as? String
                        ?? leaves.first.flatMap { kindOf(s, $0) } ?? "video"
                    for item in leaves where kindOf(s, item) == playlist.kind {
                        s.lists.nextEntry += 1
                        playlist.entries.append((s.lists.nextEntry, item))
                    }
                }
                s.lists.playlists?.append(playlist)
                return json(201, detailJSON(s, playlist))
            }
        default:
            break
        }
        guard parts.count >= 3, let id = Int(parts[2]) else { return nil }
        return withToken(&s, request, scope: method == "GET" ? "library" : "playlists") { s, _ in
            guard let at = s.lists.playlists?.firstIndex(where: { $0.id == id }) else { return notFound(request.path) }
            let smart = s.lists.playlists![at].smartFilter != nil
            let readOnly = json(400, ["code": "bad_request", "message": "a smart playlist's entries come from its filter"])
            switch (method, parts.count) {
            case ("GET", 3):
                return json(200, detailJSON(s, s.lists.playlists![at]))
            case ("PATCH", 3):
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                if let title = (body["title"] as? String)?.trimmingCharacters(in: .whitespaces) {
                    guard !title.isEmpty, title.count <= 200 else {
                        return json(400, ["code": "bad_request", "message": "a title of 1 to 200 characters"])
                    }
                    s.lists.playlists![at].title = title
                }
                return json(200, detailJSON(s, s.lists.playlists![at]))
            case ("DELETE", 3):
                s.lists.playlists!.remove(at: at)
                return Answer(status: 204, headers: [:], body: Data())
            case ("POST", 4) where parts[3] == "items":
                if smart { return readOnly }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                let leaves = (body["item_ids"] as? [Int] ?? []).flatMap { leavesOf(s, $0) }
                let kind = s.lists.playlists![at].kind
                guard leaves.allSatisfy({ kindOf(s, $0) == kind }) else {
                    return json(400, ["code": "bad_request", "message": "that doesn't fit this playlist"])
                }
                for item in leaves {
                    s.lists.nextEntry += 1
                    s.lists.playlists![at].entries.append((s.lists.nextEntry, item))
                }
                return json(200, ["added": leaves.count])
            case ("DELETE", 5) where parts[3] == "items":
                if smart { return readOnly }
                if let status = s.lists.removeRefusal { return json(status, ["code": "refused", "message": "not now"]) }
                guard let entry = Int(parts[4]),
                      s.lists.playlists![at].entries.contains(where: { $0.entry == entry })
                else { return notFound(request.path) }
                s.lists.playlists![at].entries.removeAll { $0.entry == entry }
                s.lists.removals.append(entry)
                return Answer(status: 204, headers: [:], body: Data())
            case ("PUT", 4) where parts[3] == "order":
                if smart { return readOnly }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                let ids = body["entry_ids"] as? [Int] ?? []
                s.lists.orders.append(ids)
                if let status = s.lists.orderRefusal { return json(status, ["code": "refused", "message": "not now"]) }
                let entries = s.lists.playlists![at].entries
                let first = ids.compactMap { id in entries.first { $0.entry == id } }
                s.lists.playlists![at].entries = first + entries.filter { !ids.contains($0.entry) }
                return json(200, detailJSON(s, s.lists.playlists![at]))
            default:
                return notFound(request.path)
            }
        }
    }

    /// A library's collections, by title.
    static func libraryCollections(_ s: State, _ libraryId: Int) -> [[String: Any]] {
        var s = s
        seed(&s)
        return (s.lists.collections ?? []).filter { $0.libraryId == libraryId }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
            .map { collectionJSON(s, $0) }
    }

    // MARK: Inside

    /// The fixtures' collections and playlists, the first time they are asked for.
    private static func seed(_ s: inout State) {
        if s.lists.collections == nil {
            func collection(_ path: String) -> FixtureCollection? {
                guard let o = object(path), let id = o["id"] as? Int else { return nil }
                return FixtureCollection(
                    id: id, libraryId: o["library_id"] as? Int ?? 1, title: o["title"] as? String ?? "",
                    summary: o["summary"] as? String, automatic: o["automatic"] as? Bool ?? false,
                    sort: o["sort"] as? String ?? "release", smartFilter: o["smart_filter"] as? String,
                    itemIds: (o["items"] as? [[String: Any]] ?? []).compactMap { $0["id"] as? Int })
            }
            s.lists.collections = [collection("collections/detail.json"), collection("collections/smart_detail.json"),
                                   FixtureCollection(id: 802, libraryId: 1, title: "Villeneuve Picks",
                                                     summary: "Hand-picked.", sort: "custom",
                                                     itemIds: [101, 105, 104])].compactMap { $0 }
        }
        if s.lists.playlists == nil {
            func playlist(_ path: String) -> FixturePlaylist? {
                guard let o = object(path), let id = o["id"] as? Int else { return nil }
                return FixturePlaylist(
                    id: id, title: o["title"] as? String ?? "", kind: o["kind"] as? String ?? "video",
                    smartFilter: o["smart_filter"] as? String, libraryId: o["library_id"] as? Int,
                    entries: (o["entries"] as? [[String: Any]] ?? []).compactMap { entry in
                        guard let e = entry["entry_id"] as? Int, let item = (entry["item"] as? [String: Any])?["id"]
                            as? Int else { return nil }
                        return (e, item)
                    })
            }
            s.lists.playlists = [playlist("playlists/detail.json"), playlist("playlists/smart_detail.json"),
                                 FixturePlaylist(id: 903, title: "Focus", kind: "audio", entries: [(9301, 303)])]
                .compactMap { $0 }
        }
    }

    /// Item `id` as a summary: the browsing fixtures', else the lists' own fixtures'.
    private static func itemJSON(_ s: State, _ id: Int) -> [String: Any]? {
        if let found = detailJSON(s, id) { return found }
        for path in ["collections/detail.json", "collections/smart_detail.json"] {
            if let item = (object(path)?["items"] as? [[String: Any]])?.first(where: { $0["id"] as? Int == id }) {
                return patched(s, item)
            }
        }
        for path in ["playlists/detail.json", "playlists/smart_detail.json"] {
            let entries = object(path)?["entries"] as? [[String: Any]] ?? []
            if let item = entries.compactMap({ $0["item"] as? [String: Any] }).first(where: { $0["id"] as? Int == id }) {
                return patched(s, item)
            }
        }
        if let item = (object("items/related.json")?["items"] as? [[String: Any]])?
            .first(where: { $0["id"] as? Int == id }) {
            return patched(s, item)
        }
        if let playable = s.playables[id] {
            return ["id": id, "library_id": 1, "kind": playable.episode == nil ? "movie" : "episode",
                    "title": playable.title, "duration_ms": playable.durationMs,
                    "user_state": ["position_ms": playable.positionMs, "watched": playable.watched, "play_count": 0]]
        }
        return nil
    }

    /// What adding `id` adds: a show's, season's, album's or artist's episodes or tracks, else itself.
    private static func leavesOf(_ s: State, _ id: Int) -> [Int] {
        switch itemJSON(s, id)?["kind"] as? String {
        case "show": return leavesOf(s, 202)
        case "season", "album":
            let children = id == 302 ? tracks : object("items/children_episodes.json")?["items"] as? [[String: Any]] ?? []
            return children.compactMap { $0["id"] as? Int }
        case "artist": return tracks.compactMap { $0["id"] as? Int }
        case nil: return []
        default: return [id]
        }
    }

    private static func kindOf(_ s: State, _ id: Int) -> String? {
        guard let kind = itemJSON(s, id)?["kind"] as? String else { return nil }
        return ["artist", "album", "track"].contains(kind) ? "audio" : "video"
    }

    private static func collectionJSON(_ s: State, _ c: FixtureCollection) -> [String: Any] {
        ["id": c.id, "library_id": c.libraryId, "title": c.title, "summary": c.summary ?? NSNull(),
         "automatic": c.automatic, "sort": c.sort, "smart_filter": c.smartFilter ?? NSNull(),
         "item_count": c.itemIds.count, "poster_url": "/api/collections/\(c.id)/images/poster?v=1",
         "backdrop_url": "/api/collections/\(c.id)/images/backdrop?v=1", "created_at": 1_761_300_000,
         "updated_at": 1_761_300_000]
    }

    private static func playlistJSON(_ s: State, _ p: FixturePlaylist) -> [String: Any] {
        let items = p.entries.compactMap { itemJSON(s, $0.item) }
        return ["id": p.id, "title": p.title, "kind": p.kind, "item_count": items.count,
                "duration_ms": items.reduce(0) { $0 + ($1["duration_ms"] as? Int ?? 0) },
                "artwork": items.prefix(4).compactMap { ($0["poster_url"] ?? $0["thumb_url"]) as? String },
                "smart_filter": p.smartFilter ?? NSNull(), "library_id": p.libraryId ?? NSNull(),
                "created_at": 1_761_320_000, "updated_at": 1_761_390_500]
    }

    private static func detailJSON(_ s: State, _ p: FixturePlaylist) -> [String: Any] {
        var out = playlistJSON(s, p)
        out["entries"] = p.entries.compactMap { entry -> [String: Any]? in
            itemJSON(s, entry.item).map { ["entry_id": entry.entry, "item": $0] }
        }
        return out
    }
}
