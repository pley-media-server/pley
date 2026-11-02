import Foundation
import PleyModel

/// Live TV: a lineup (the Android fixtures' three channels, or one made up
/// as large as a test needs), favourites and Recently watched kept as the
/// server keeps them, a guide of half-hour programmes made around the time
/// asked for, and watching: sessions with keyed playlists, a source with
/// one connection that answers `409` (Android's `live/busy.json`) until a
/// take over, plans, and leaving.
extension FixtureServer {
    /// A channel of the fixture's lineup.
    struct FixtureChannel: Sendable {
        var id: Int
        var name: String
        var number: String?
        var group: String
        var sourceId = 1
    }

    /// A session watching a channel.
    struct LiveSession: Sendable {
        let channelId: Int
        let key: String
        let clientId: String
        var deleted = false
        /// Why it was stopped (`410`); `nil` while it plays.
        var stopped: String?
    }

    struct Live {
        var allowed = true
        var channels: [FixtureChannel] = FixtureServer.androidChannels()
        var favorites: Set<Int> = [2]
        var recent: [Int] = [2, 1]
        /// The source has one connection: a channel watched by another
        /// holds it, and `watch` answers `409` unless it takes over.
        var heldBy: Int?
        var sessions: [String: LiveSession] = [:]
        var watchBodies: [Data] = []
        var favoriteRefusal: Int?
        var watchRefusal: Int?
        /// The plan is the fixture's own, or `null` (the server not knowing in time).
        var planKnown = true
        var nextSession = 1
    }

    /// The Android fixtures' channels (`live/channels.json`), by number.
    static func androidChannels() -> [FixtureChannel] {
        guard let data = try? Fixtures.data("live/channels.json"),
              let list = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]]
        else { return [] }
        return list.compactMap { c in
            guard let id = c["id"] as? Int else { return nil }
            return FixtureChannel(id: id, name: c["name"] as? String ?? "", number: c["number"] as? String,
                                  group: c["group"] as? String ?? "", sourceId: c["source_id"] as? Int ?? 1)
        }
    }

    // MARK: Knobs

    /// A lineup of `count` made-up channels in place of the fixtures':
    /// ids from 1,001, numbers from 1, names "Channel 0001"…, in the groups
    /// `groups` in turn ("UK| News", "UK| Sport", "FR| Info" by default).
    public func setLiveLineup(count: Int, groups: [String] = ["UK| News", "UK| Sport", "FR| Info"]) {
        state.withLock { s in
            s.live.channels = (0..<count).map { n in
                FixtureChannel(id: 1001 + n, name: String(format: "Channel %04d", n + 1), number: String(n + 1),
                               group: groups.isEmpty ? "" : groups[n % groups.count])
            }
            s.live.favorites = []
            s.live.recent = []
        }
    }

    /// Whether Live TV is open to the profile (`403` otherwise).
    public func setLiveAllowed(_ allowed: Bool) {
        state.withLock { $0.live.allowed = allowed }
    }

    /// Recently watched, newest first.
    public func setLiveRecent(_ ids: [Int]) {
        state.withLock { $0.live.recent = ids }
    }

    /// The source's one connection is held by someone else watching `channelId` (`nil`: free).
    public func holdLiveConnection(_ channelId: Int?) {
        state.withLock { $0.live.heldBy = channelId }
    }

    /// Setting a favourite answers this status from now on (`nil`: normally).
    public func refuseLiveFavorite(status: Int?) {
        state.withLock { $0.live.favoriteRefusal = status }
    }

    /// Watching answers this status from now on (`nil`: normally).
    public func refuseLiveWatch(status: Int?) {
        state.withLock { $0.live.watchRefusal = status }
    }

    /// The plan answers `null` (the server not knowing in time) when `known` is false.
    public func setLivePlanKnown(_ known: Bool) {
        state.withLock { $0.live.planKnown = known }
    }

    /// Stops session `id` from elsewhere: its plan and media answer `410` with `message`.
    public func stopLiveSession(_ id: String, message: String = "An administrator stopped this channel.") {
        state.withLock { $0.live.sessions[id]?.stopped = message }
    }

    /// The favourite channels now.
    public var liveFavorites: Set<Int> { state.withLock { $0.live.favorites } }

    /// The bodies of `POST /api/live/channels/{id}/watch`, in order.
    public var liveWatchRequests: [Data] { state.withLock { $0.live.watchBodies } }

    /// The sessions watching now (not left, not stopped), by id, with their channel.
    public var liveSessions: [String: Int] {
        state.withLock { s in
            s.live.sessions.filter { !$0.value.deleted && $0.value.stopped == nil }.mapValues(\.channelId)
        }
    }

    // MARK: Routes

    static func live(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api", parts[1] == "live" else { return nil }
        let method = request.method
        // Keyed media carry their own key, and no token.
        if method == "GET", parts.count == 5, parts[2] == "media" {
            guard let session = s.live.sessions.values.first(where: { $0.key == parts[3] }), !session.deleted else {
                return notFound(request.path)
            }
            if let message = session.stopped { return json(410, ["code": "session_stopped", "message": message]) }
            return Answer(status: 200, headers: ["Content-Type": "application/vnd.apple.mpegurl"],
                          body: Data(liveIndex.utf8))
        }
        return withToken(&s, request, scope: method == "GET" ? "library" : "play") { s, _ in
            if parts.count == 2 && method == "GET" { return summary(s) }
            guard s.live.allowed else {
                return json(403, ["code": "forbidden", "message": "Live TV isn't open to this account"])
            }
            switch (method, parts.count, parts[2]) {
            case ("GET", 3, "groups"):
                return json(200, groups(s))
            case ("GET", 3, "browse"):
                return browse(s, request.query)
            case ("GET", 3, "guide"):
                return guide(s, request.query)
            case ("GET", 4, "channels"):
                guard let id = Int(parts[3]), let channel = s.live.channels.first(where: { $0.id == id }) else {
                    return notFound(request.path)
                }
                return json(200, channelJSON(s, channel))
            case ("GET", 5, "channels") where parts[4] == "logo",
                 ("GET", 7, "channels") where parts[4] == "programs" && parts[6] == "image":
                let width = min(max(Int(request.query["w"] ?? "") ?? 200, 1), 800)
                return image(width: width, height: max(width * 9 / 16, 1), type: "public.png",
                             contentType: "image/png")
            case ("PUT", 5, "channels") where parts[4] == "favorite",
                 ("DELETE", 5, "channels") where parts[4] == "favorite":
                guard let id = Int(parts[3]), s.live.channels.contains(where: { $0.id == id }) else {
                    return notFound(request.path)
                }
                if let status = s.live.favoriteRefusal { return json(status, ["code": "refused", "message": "not now"]) }
                if method == "PUT" { s.live.favorites.insert(id) } else { s.live.favorites.remove(id) }
                return Answer(status: 204, headers: [:], body: Data())
            case ("POST", 5, "channels") where parts[4] == "watch":
                guard let id = Int(parts[3]) else { return notFound(request.path) }
                return watch(&s, id, request.body)
            case ("GET", 5, "sessions") where parts[4] == "plan":
                guard let session = s.live.sessions[parts[3]], !session.deleted else { return notFound(request.path) }
                if let message = session.stopped { return json(410, ["code": "session_stopped", "message": message]) }
                return s.live.planKnown ? fixture("live/plan.json")
                    : Answer(status: 200, headers: ["Content-Type": "application/json"], body: Data("null".utf8))
            case ("DELETE", 4, "sessions"):
                guard s.live.sessions[parts[3]]?.deleted == false else { return notFound(request.path) }
                s.live.sessions[parts[3]]?.deleted = true
                return Answer(status: 204, headers: [:], body: Data())
            default:
                return notFound(request.path)
            }
        }
    }

    /// A live playlist that names no segment: enough for a request's answer.
    private static let liveIndex = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:0\n"

    private static func summary(_ s: State) -> Answer {
        json(200, ["available": s.live.allowed && !s.live.channels.isEmpty, "allowed": s.live.allowed,
                   "channels": s.live.allowed ? s.live.channels.count : 0, "sources": 0,
                   "recent": Array(s.live.recent.prefix(12))])
    }

    private static func groups(_ s: State) -> [String: Any] {
        var order: [String] = []
        var counts: [String: Int] = [:]
        for channel in s.live.channels {
            if counts[channel.group] == nil { order.append(channel.group) }
            counts[channel.group, default: 0] += 1
        }
        let groups = order.map { name -> [String: Any] in
            let split = splitRegion(name)
            return ["name": name, "region": split?.0 ?? NSNull(), "label": split?.1 ?? name, "count": counts[name] ?? 0]
        }
        return ["total": s.live.channels.count, "favorites": s.live.favorites.count,
                "recent": s.live.recent.count, "groups": groups]
    }

    /// "UK| Sport" as ("UK", "Sport"), as the server splits group names.
    private static func splitRegion(_ name: String) -> (String, String)? {
        guard let bar = name.firstIndex(of: "|") else { return nil }
        let region = name[..<bar].trimmingCharacters(in: .whitespaces)
        let rest = name[name.index(after: bar)...].trimmingCharacters(in: .whitespaces)
        return region.isEmpty || rest.isEmpty ? nil : (region, rest)
    }

    /// The channels of a category, in its order.
    private static func listed(_ s: State, _ query: [String: String]) -> [FixtureChannel] {
        let all = s.live.channels
        var list: [FixtureChannel]
        switch query["category"] ?? "all" {
        case "favorites": list = all.filter { s.live.favorites.contains($0.id) }
        case "recent": list = s.live.recent.compactMap { id in all.first { $0.id == id } }
        case "group": list = all.filter { $0.group == query["group"] ?? "" }
        default: list = all
        }
        let q = (query["q"] ?? "").trimmingCharacters(in: .whitespaces).lowercased()
        if !q.isEmpty {
            let now = Int64(Date().timeIntervalSince1970)
            list = list.filter { c in
                c.name.lowercased().contains(q) || (c.number ?? "").hasPrefix(q)
                    || programTitle(c.id, slot(now)).lowercased().contains(q)
            }
        }
        return list
    }

    private static func browse(_ s: State, _ query: [String: String]) -> Answer {
        let list = listed(s, query)
        let limit = min(max(Int(query["limit"] ?? "") ?? 100, 1), 500)
        var offset = max(Int(query["offset"] ?? "") ?? 0, 0)
        if let around = query["around"].flatMap(Int.init) {
            offset = (list.firstIndex { $0.id == around } ?? 0) / limit * limit
        }
        let page = offset < list.count ? Array(list[offset..<min(offset + limit, list.count)]) : []
        return json(200, ["total": list.count, "offset": offset, "channels": page.map { channelJSON(s, $0) }])
    }

    /// Programmes are half an hour each, from the hour and the half hour.
    private static func slot(_ time: Int64) -> Int64 { time / 1800 * 1800 }

    private static func programTitle(_ channelId: Int, _ start: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(start))
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let parts = calendar.dateComponents([.hour, .minute], from: date)
        return String(format: "Programme %d at %02d:%02d", channelId, parts.hour ?? 0, parts.minute ?? 0)
    }

    private static func programJSON(_ channelId: Int, _ start: Int64) -> [String: Any] {
        ["channel_id": channelId, "start": start, "stop": start + 1800, "title": programTitle(channelId, start),
         "categories": [String](), "is_new": start % 3600 == 0, "is_live": false,
         "description": "Half an hour on channel \(channelId).",
         "image": "/api/live/channels/\(channelId)/programs/\(start)/image"]
    }

    private static func channelJSON(_ s: State, _ c: FixtureChannel) -> [String: Any] {
        let now = slot(Int64(Date().timeIntervalSince1970))
        return ["id": c.id, "source_id": c.sourceId, "name": c.name, "number": c.number ?? NSNull(), "group": c.group,
                "logo": "/api/live/channels/\(c.id)/logo", "favorite": s.live.favorites.contains(c.id),
                "has_guide": true, "now": programJSON(c.id, now), "next": programJSON(c.id, now + 1800)]
    }

    private static func guide(_ s: State, _ query: [String: String]) -> Answer {
        let now = Int64(Date().timeIntervalSince1970)
        let from = Int64(query["from"] ?? "") ?? (slot(now) - 1800)
        let to = min(Int64(query["to"] ?? "") ?? (from + 6 * 3600), from + 48 * 3600)
        let ids = (query["ids"] ?? "").split(separator: ",").compactMap { Int($0) }
        let wanted = ids.isEmpty ? s.live.channels.map(\.id) : ids.filter { id in
            s.live.channels.contains { $0.id == id }
        }
        var programs: [String: Any] = [:]
        for id in wanted {
            programs[String(id)] = stride(from: slot(from), to: to, by: 1800).map { programJSON(id, $0) }
        }
        return json(200, ["from": from, "to": to, "programs": programs])
    }

    private static func watch(_ s: inout State, _ channelId: Int, _ body: Data) -> Answer {
        s.live.watchBodies.append(body)
        guard let channel = s.live.channels.first(where: { $0.id == channelId }) else {
            return json(404, ["code": "not_found", "message": "channel \(channelId) not found"])
        }
        if let status = s.live.watchRefusal {
            return json(status, ["code": "unavailable", "message": "the provider sends nothing for it"])
        }
        let request = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        let client = request["client_id"] as? String ?? ""
        // What this device watched before ends.
        if !client.isEmpty {
            for (id, session) in s.live.sessions where session.clientId == client && !session.deleted {
                s.live.sessions[id]?.deleted = true
            }
        }
        if let held = s.live.heldBy, held != channelId {
            guard let takeOver = request["take_over"] as? Int, takeOver == held else {
                let busy = (try? Fixtures.data("live/busy.json"))
                    .flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
                return json(409, busy)
            }
            s.live.heldBy = nil
        }
        let id = String(format: "%032x", s.live.nextSession)
        s.live.nextSession += 1
        let key = "pley_mk_live\(UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased())"
        s.live.sessions[id] = LiveSession(channelId: channelId, key: key, clientId: client)
        s.live.recent.removeAll { $0 == channelId }
        s.live.recent.insert(channelId, at: 0)
        let url = request["keyed_urls"] as? Bool == true
            ? "/api/live/media/\(key)/index.m3u8" : "/api/live/sessions/\(id)/index.m3u8"
        return json(200, ["session_id": id, "url": url, "kind": "hls", "channel": channelJSON(s, channel)])
    }
}
