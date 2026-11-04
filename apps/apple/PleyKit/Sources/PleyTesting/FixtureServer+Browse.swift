import Foundation
import Network
import PleyModel

/// Browsing: the libraries and their listings, the profile's marks (watched,
/// My List, Continue Watching, the library order) kept as the server keeps
/// them, a generated library as large as asked for, and an event stream
/// the tests push events into.
extension FixtureServer {
    /// A library the fixture lists.
    struct FixtureLibrary: Sendable {
        var id: Int
        var name: String
        var kind: String
        var count: Int
        var scanning = false
        /// Its items are made up (`Title 0001`…), as many as `count`.
        var generated = false
    }

    /// An open event stream.
    final class EventClient: @unchecked Sendable {
        // Unchecked Sendable: the connection is only used on the fixture's queue and is safe to share.
        let connection: NWConnection
        init(_ connection: NWConnection) { self.connection = connection }
    }

    struct Browse {
        var libraries: [FixtureLibrary] = [
            FixtureLibrary(id: 1, name: "Movies", kind: "movies", count: 412),
            FixtureLibrary(id: 2, name: "TV Shows", kind: "shows", count: 37),
            FixtureLibrary(id: 3, name: "Music", kind: "music", count: 128),
        ]
        /// The profile's own order; `nil`: the shared one.
        var order: [Int]?
        var orderRefusal: Int?
        var watched: [Int: Bool] = [:]
        var watchlist: Set<Int>?
        var hidden: Set<Int> = []
        /// Goes up with every mark: the listings' `ETag`.
        var version = 1
        var events: [EventClient] = []
        var eventConnects = 0
        var eventsRefusal: Int?
        /// The profile's ratings, by item; a `nil` value: taken away.
        var ratings: [Int: Double?] = [:]
        var ratingRefusal: Int?
        /// The items read ahead, in order.
        var prefetches: [Int] = []
    }

    // MARK: Knobs

    /// Adds a library of `count` made-up titles, sorted by title from
    /// "A …" to "Z …", as large as a test needs (a 5,000-title one).
    public func addLibrary(id: Int, name: String, kind: String = "movies", count: Int) {
        state.withLock {
            $0.browse.libraries.append(FixtureLibrary(id: id, name: name, kind: kind, count: count, generated: true))
        }
    }

    /// Whether library `id` is listed as scanning.
    public func setScanning(_ id: Int, _ scanning: Bool) {
        state.withLock { s in
            if let at = s.browse.libraries.firstIndex(where: { $0.id == id }) {
                s.browse.libraries[at].scanning = scanning
            }
        }
    }

    /// `PUT /api/me/libraries/order` answers this status from now on (`nil`: normally).
    public func refuseLibraryOrder(status: Int?) {
        state.withLock { $0.browse.orderRefusal = status }
    }

    /// The library order the profile has now, by id.
    public var libraryOrder: [Int] {
        state.withLock { s in s.browse.order ?? s.browse.libraries.map(\.id) }
    }

    /// Item `id`'s watched mark, as the fixture keeps it; `nil` when never marked.
    public func watched(_ id: Int) -> Bool? {
        state.withLock { $0.browse.watched[id] }
    }

    /// The ids in My List now.
    public var watchlistIds: Set<Int> {
        state.withLock { Self.watchlist($0) }
    }

    /// The ids taken out of Continue Watching.
    public var hiddenFromContinueWatching: Set<Int> {
        state.withLock { $0.browse.hidden }
    }

    /// `PUT /api/items/{id}/rating` answers this status from now on (`nil`: normally).
    public func refuseRating(status: Int?) {
        state.withLock { $0.browse.ratingRefusal = status }
    }

    /// The rating item `id` has now; `nil` when never rated, `.some(nil)` when taken away.
    public func rating(_ id: Int) -> Double?? {
        state.withLock { $0.browse.ratings[id] }
    }

    /// The items `POST /api/items/{id}/prefetch` was asked for, in order.
    public var prefetches: [Int] { state.withLock { $0.browse.prefetches } }

    /// `GET /api/events` answers this status from now on (`nil`: a stream).
    public func refuseEvents(status: Int?) {
        state.withLock { $0.browse.eventsRefusal = status }
    }

    /// How many event streams were opened.
    public var eventConnects: Int { state.withLock { $0.browse.eventConnects } }

    /// How many event streams are open.
    public var openEventStreams: Int { state.withLock { $0.browse.events.count } }

    /// Sends `json` as one event to every open stream.
    public func pushEvent(_ json: String) {
        pushEventText("data: \(json)\n\n")
    }

    /// Sends `text` as it is to every open stream.
    public func pushEventText(_ text: String) {
        let clients = state.withLock { $0.browse.events }
        for client in clients {
            client.connection.send(content: Data(text.utf8), completion: .contentProcessed { _ in })
        }
    }

    /// Ends every open stream, as the server does when a token expires;
    /// the receivers they named go with them.
    public func endEvents() {
        let clients = state.withLock { s -> [EventClient] in
            defer {
                s.browse.events = []
                s.remote.receivers = s.remote.receivers.filter { !$0.value.streamed }
            }
            return s.browse.events
        }
        for client in clients { client.connection.cancel() }
    }

    // MARK: The event stream

    /// `GET /api/events`: the stream's head, and the connection kept for `pushEvent`.
    func openEvents(_ request: Request, on connection: NWConnection) {
        let answer: Answer? = state.withLock { s in
            var headers = request.headers
            if let auth = headers["authorization"] {
                let token = auth.hasPrefix("Bearer ") ? String(auth.dropFirst(7)) : ""
                headers["authorization"] = s.accessTokens[token].map { "Bearer #\($0.id)" } ?? "Bearer (unknown)"
            }
            s.journal.append(Recorded(method: request.method, path: request.path, query: request.query,
                                      headers: headers, grantType: nil, formKeys: []))
            let checked = Self.withToken(&s, request, scope: "library") { s, access in
                if let status = s.browse.eventsRefusal {
                    return Self.json(status, ["code": "too_many_streams", "message": "too many event streams"])
                }
                s.browse.eventConnects += 1
                s.browse.events.append(EventClient(connection))
                Self.openedReceiver(&s, request.query, deviceId: Self.deviceId(access))
                return Answer(status: 0, headers: [:], body: Data())
            }
            return checked.status == 0 ? nil : checked
        }
        if let answer {
            send(answer, on: connection)
            return
        }
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n"
            + "Connection: close\r\n\r\n:\n\n"
        connection.send(content: Data(head.utf8), completion: .contentProcessed { _ in })
    }

    // MARK: Routes

    /// The browsing routes; `nil` for the others.
    static func browse(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        let method = request.method
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        switch (method, parts.count, parts[1]) {
        case ("GET", 2, "libraries"):
            return withToken(&s, request, scope: "library") { s, _ in json(200, ordered(s).map(libraryJSON)) }
        case ("PUT", 4, "me") where parts[2] == "libraries" && parts[3] == "order",
             ("DELETE", 4, "me") where parts[2] == "libraries" && parts[3] == "order":
            return withToken(&s, request, scope: "play") { s, _ in
                if method == "DELETE" {
                    s.browse.order = nil
                    return json(200, ordered(s).map(libraryJSON))
                }
                if let status = s.browse.orderRefusal {
                    return json(status, ["code": status == 409 ? "conflict" : "refused", "message": "not that order"])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                let ids = body["ids"] as? [Int] ?? []
                guard Set(ids).count == ids.count else { return json(400, ["code": "bad_request", "message": "twice"]) }
                guard Set(ids) == Set(s.browse.libraries.map(\.id)) else {
                    return json(409, ["code": "conflict", "message": "those aren't the libraries"])
                }
                s.browse.order = ids
                return json(200, ordered(s).map(libraryJSON))
            }
        case ("GET", 3, "libraries"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard let library = s.browse.libraries.first(where: { $0.id == id }) else {
                    return notFound(request.path)
                }
                return json(200, libraryJSON(library))
            }
        case ("GET", 4, "libraries"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard let library = s.browse.libraries.first(where: { $0.id == id }) else {
                    return notFound(request.path)
                }
                switch parts[3] {
                case "hubs": return library.generated ? json(200, ["hubs": []]) : fixture("libraries/hubs.json")
                case "filters": return fixture("libraries/filters.json")
                case "collections": return json(200, ["collections": libraryCollections(s, library.id)])
                case "items": return items(&s, library, request)
                default: return notFound(request.path)
                }
            }
        case ("GET", 2, "watchlist"):
            return withToken(&s, request, scope: "library") { s, _ in
                let ids = watchlist(s)
                let known = summaries(s)
                return json(200, ["items": ids.sorted().compactMap { known[$0] }])
            }
        case ("PUT", 3, "watchlist"), ("DELETE", 3, "watchlist"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                var ids = watchlist(s)
                if method == "PUT" { ids.insert(id) } else { ids.remove(id) }
                s.browse.watchlist = ids
                s.browse.version += 1
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("DELETE", 3, "continue-watching"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                s.browse.hidden.insert(id)
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("POST", 4, "items") where parts[3] == "watched" || parts[3] == "unwatched":
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                let mark = parts[3] == "watched"
                s.browse.watched[id] = mark
                s.browse.version += 1
                s.playables[id]?.watched = mark
                if mark { s.playables[id]?.positionMs = 0 }
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("GET", 3, "items"):
            guard let id = Int(parts[2]), !playsVideo(s, id) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard var detail = detailJSON(s, id) else { return notFound(request.path) }
                detail["in_watchlist"] = watchlist(s).contains(id)
                return json(200, patched(s, detail))
            }
        case ("GET", 4, "items") where parts[3] == "on-deck":
            guard let id = Int(parts[2]), !playsVideo(s, id), id == 201 || id == 202 else { return nil }
            return withToken(&s, request, scope: "library") { _, _ in fixture("items/on_deck.json") }
        case ("GET", 4, "items") where parts[3] == "children":
            guard let id = Int(parts[2]), !playsVideo(s, id) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard let items = children(id) else { return notFound(request.path) }
                return json(200, ["items": items.map { patched(s, $0) }])
            }
        case ("GET", 4, "items") where parts[3] == "related":
            guard let id = Int(parts[2]), !playsVideo(s, id) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard detailJSON(s, id) != nil else { return notFound(request.path) }
                let items = id == 101 || id == 201 ? object("items/related.json")?["items"] as? [[String: Any]] ?? [] : []
                return json(200, ["items": items.map { patched(s, $0) }])
            }
        case ("GET", 4, "items") where parts[3] == "collections":
            guard let id = Int(parts[2]), !playsVideo(s, id) else { return nil }
            return withToken(&s, request, scope: "library") { _, _ in
                id == 101 ? fixture("items/collections.json") : json(200, ["collections": [Any]()])
            }
        case ("PUT", 4, "items") where parts[3] == "rating":
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                if let status = s.browse.ratingRefusal {
                    return json(status, ["code": "refused", "message": "not now"])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                guard body.keys.contains("rating") else { return json(400, ["code": "bad_request", "message": "rating"]) }
                let rating = body["rating"] as? Double
                if let rating, !(0...10).contains(rating) {
                    return json(400, ["code": "bad_request", "message": "a rating is 0 to 10"])
                }
                s.browse.ratings[id] = .some(rating)
                s.browse.version += 1
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("POST", 4, "items") where parts[3] == "prefetch":
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                s.browse.prefetches.append(id)
                return Answer(status: 202, headers: [:], body: Data())
            }
        case ("GET", 3, "people"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "library") { _, _ in
                id == 706 ? fixture("people/detail.json") : notFound(request.path)
            }
        case ("GET", 2, "hubs") where parts.count == 3:
            return nil
        default:
            return nil
        }
    }

    /// `GET /api/hubs/home`, without the titles taken out of Continue Watching.
    static func home(_ s: State) -> Answer {
        guard var hubs = object("hubs/home.json")?["hubs"] as? [[String: Any]] else { return fixture("hubs/home.json") }
        for (n, hub) in hubs.enumerated() where (hub["key"] as? String)?.hasPrefix("continue") == true {
            let items = (hub["items"] as? [[String: Any]] ?? []).filter { item in
                let id = item["id"] as? Int ?? 0
                let show = item["grandparent_id"] as? Int
                return !s.browse.hidden.contains(id) && !(show.map(s.browse.hidden.contains) ?? false)
            }
            hubs[n]["items"] = items
        }
        hubs = hubs.map { hub in
            var hub = hub
            hub["items"] = (hub["items"] as? [[String: Any]] ?? []).map { patched(s, $0) }
            return hub
        }.filter { !($0["items"] as? [Any] ?? []).isEmpty }
        return json(200, ["hubs": hubs])
    }

    // MARK: Inside

    private static func ordered(_ s: State) -> [FixtureLibrary] {
        guard let order = s.browse.order else { return s.browse.libraries }
        return order.compactMap { id in s.browse.libraries.first { $0.id == id } }
    }

    private static func libraryJSON(_ library: FixtureLibrary) -> [String: Any] {
        ["id": library.id, "name": library.name, "kind": library.kind, "paths": [String](),
         "item_count": library.count, "created_at": 1_758_000_000, "scanned_at": 1_761_300_000,
         "scanning": library.scanning]
    }

    /// A page of a library's listing, with its `ETag`; `304` for the tag it has.
    private static func items(_ s: inout State, _ library: FixtureLibrary, _ request: Request) -> Answer {
        let offset = max(Int(request.query["offset"] ?? "") ?? 0, 0)
        let limit = min(max(Int(request.query["limit"] ?? "") ?? 50, 1), 500)
        let tag = "\"v\(s.browse.version)-\(library.id)-\(offset)-\(request.query["sort"] ?? "title")\""
        if request.headers["if-none-match"] == tag { return Answer(status: 304, headers: ["ETag": tag], body: Data()) }
        var items: [[String: Any]]
        let total: Int
        if library.generated {
            total = library.count
            items = (offset..<min(offset + limit, library.count)).map { generated(library, $0) }
        } else if offset == 0 {
            let file = request.query["sort"] == "bitrate" || library.id == 2
                ? "libraries/items_by_bitrate.json" : "libraries/items.json"
            let page = object(file) ?? [:]
            total = page["total"] as? Int ?? 0
            items = page["items"] as? [[String: Any]] ?? []
        } else {
            total = object("libraries/items.json")?["total"] as? Int ?? 0
            items = []
        }
        items = items.map { patched(s, $0) }
        if let watched = request.query["watched"] {
            items = items.filter { (($0["user_state"] as? [String: Any])?["watched"] as? Bool ?? false) == (watched == "true") }
        }
        var answer = json(200, ["total": total, "items": items])
        answer.headers["ETag"] = tag
        return answer
    }

    /// The made-up title at `index` of a generated library: "A 0001" to
    /// "Z 5000", in title order.
    private static func generated(_ library: FixtureLibrary, _ index: Int) -> [String: Any] {
        let letters = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ")
        let letter = letters[min(index * letters.count / max(library.count, 1), letters.count - 1)]
        let title = "\(letter) \(String(format: "%05d", index + 1))"
        return ["id": library.id * 100_000 + index, "library_id": library.id,
                "kind": library.kind == "music" ? "artist" : "movie", "title": title, "sort_title": title,
                "added_at": 1_761_300_000, "user_state": ["position_ms": 0, "watched": false, "play_count": 0]]
    }

    /// An item's watch state and rating as the fixture keeps them.
    static func patched(_ s: State, _ item: [String: Any]) -> [String: Any] {
        guard let id = item["id"] as? Int else { return item }
        var item = item
        var state = item["user_state"] as? [String: Any] ?? [:]
        if let rating = s.browse.ratings[id] { state["rating"] = rating ?? NSNull() }
        if let mark = s.browse.watched[id] {
            state["watched"] = mark
            if mark { state["position_ms"] = 0 }
            if let leaves = item["leaf_count"] as? Int { item["viewed_leaf_count"] = mark ? leaves : 0 }
        }
        item["user_state"] = state
        return item
    }

    /// An item's children: Severance's seasons and season 1's episodes (from
    /// the fixtures), and the music fixtures' artist and album; `nil` for
    /// an item the fixture doesn't have.
    private static func children(_ id: Int) -> [[String: Any]]? {
        switch id {
        case 201: object("items/children_seasons.json")?["items"] as? [[String: Any]]
        case 202: object("items/children_episodes.json")?["items"] as? [[String: Any]]
        case 206: []
        case 301: [album]
        case 302: tracks
        default: nil
        }
    }

    /// The music fixtures beside `items/track_detail.json`: its artist, Nils
    /// Frahm (301), and its album, All Melody (302), with three tracks
    /// (the second is the fixture's track 303) on two discs.
    static var artist: [String: Any] {
        [
            "id": 301, "library_id": 3, "kind": "artist", "title": "Nils Frahm", "sort_title": "Frahm, Nils",
            "added_at": 1_761_100_000, "poster_url": "/api/items/301/images/poster?v=1761100300",
            "user_state": ["position_ms": 0, "watched": false, "play_count": 0], "child_count": 1, "leaf_count": 3,
            "summary": "A composer from Hamburg.", "genres": ["Ambient"], "files": [Any](),
        ]
    }

    static var album: [String: Any] {
        [
            "id": 302, "library_id": 3, "kind": "album", "title": "All Melody", "sort_title": "All Melody", "year": 2018,
            "parent_id": 301, "parent_title": "Nils Frahm", "added_at": 1_761_100_000,
            "poster_url": "/api/items/302/images/poster?v=1761100300",
            "user_state": ["position_ms": 0, "watched": false, "play_count": 0], "leaf_count": 3,
        ]
    }

    static var tracks: [[String: Any]] {
        [
            track(304, index: 1, disc: 1, title: "Sunson", ms: 552_000),
            track(303, index: 2, disc: 1, title: "The Whole Universe Wants to Be Touched", ms: 245_000),
            track(305, index: 1, disc: 2, title: "Forever Changeless", ms: 190_000, artist: "Guest Player"),
        ]
    }

    private static func track(_ id: Int, index: Int, disc: Int, title: String, ms: Int,
                              artist: String = "Nils Frahm") -> [String: Any] {
        ["id": id, "library_id": 3, "kind": "track", "title": title, "sort_title": title, "year": 2018,
         "index": index, "parent_id": 302, "parent_title": "All Melody", "parent_index": disc, "grandparent_id": 301,
         "grandparent_title": artist, "duration_ms": ms, "added_at": 1_761_100_000,
         "user_state": ["position_ms": 0, "watched": false, "play_count": 0]]
    }

    private static func watchlist(_ s: State) -> Set<Int> {
        if let ids = s.browse.watchlist { return ids }
        let items = object("watchlist/list.json")?["items"] as? [[String: Any]] ?? []
        return Set(items.compactMap { $0["id"] as? Int })
    }

    /// Every item the fixtures list, by id, as summaries.
    static func summaries(_ s: State) -> [Int: [String: Any]] {
        var out: [Int: [String: Any]] = [:]
        let lists: [[[String: Any]]] = [
            object("libraries/items.json")?["items"] as? [[String: Any]] ?? [],
            object("libraries/items_by_bitrate.json")?["items"] as? [[String: Any]] ?? [],
            object("watchlist/list.json")?["items"] as? [[String: Any]] ?? [],
            object("items/children_seasons.json")?["items"] as? [[String: Any]] ?? [],
            object("items/children_episodes.json")?["items"] as? [[String: Any]] ?? [],
            tracks,
        ] + ((object("hubs/home.json")?["hubs"] as? [[String: Any]] ?? []).map { $0["items"] as? [[String: Any]] ?? [] })
        for list in lists {
            for item in list { if let id = item["id"] as? Int, out[id] == nil { out[id] = patched(s, item) } }
        }
        return out
    }

    /// Item `id`'s details: its fixture's when there is one, else its summary.
    static func detailJSON(_ s: State, _ id: Int) -> [String: Any]? {
        for file in ["items/movie_detail.json", "items/show_detail.json", "items/episode_detail.json",
                     "items/track_detail.json", "items/extra.json"] {
            if let detail = object(file), detail["id"] as? Int == id { return patched(s, detail) }
        }
        if id == 301 { return patched(s, artist) }
        if id == 302 { return patched(s, album.merging(["genres": ["Ambient"], "files": [Any]()]) { a, _ in a }) }
        if let library = s.browse.libraries.first(where: { $0.generated && id / 100_000 == $0.id }) {
            return generated(library, id % 100_000)
        }
        return summaries(s)[id]
    }

    static func object(_ path: String) -> [String: Any]? {
        guard let data = try? Fixtures.data(path) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }
}
