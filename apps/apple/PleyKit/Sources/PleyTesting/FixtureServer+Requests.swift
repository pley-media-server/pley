import Foundation
import PleyModel

/// Requests (docs/API.md, "Requests"), answered from the Android app's
/// fixtures: the summary, Discover's lists, search and title pages, a
/// library title as Discover sees it, the requests and the managers'
/// decisions, each behind its scope as the server checks it.
extension FixtureServer {
    struct Requests {
        /// The summary the server answers, its JSON: the fixture's, with
        /// nothing unseen, so a sign-in tells no news unless a test asks
        /// (`setRequestsUnseen`).
        var summary = FixtureServer.unseen(0, in: (try? Fixtures.data("requests/summary.json")) ?? Data())
        /// Requests are off: the summary says so, and every other route answers `503 unavailable`.
        var off = false
        /// An older server: the summary answers `404`.
        var missing = false
        /// Routes answering an error, by path prefix: its status and JSON.
        var failing: [String: (status: Int, body: Data)] = [:]
        /// What asking answers: its status and the request's JSON (`nil`: `201`, request 44 as pending).
        var askAnswer: (status: Int, body: Data)?
        /// The status request 44 comes back with when `askAnswer` is `nil`:
        /// `201` a new request, `200` one followed now.
        var askStatus = 201
        var asked: [Data] = []
        /// The titles asked for (`movie:1184918`), with the request that came back: their pages say so.
        var askedTitles: [String: Data] = [:]
        /// The decisions and removals sent: method, path, body.
        var acted: [(method: String, path: String, body: Data)] = []
        var seen = 0
        /// Requests changed by an action, by id: their JSON now.
        var changed: [Int: Data] = [:]
        /// Requests gone (withdrawn, deleted, no longer followed).
        var gone: Set<Int> = []
        /// The library items Discover sees, by id: their answer's fixture.
        var items: [Int: String] = [201: "requests/item_request.json"]
    }

    // MARK: Knobs

    /// The summary answers `json`.
    public func setRequestSummary(_ json: String) {
        state.withLock { $0.requests.summary = Data(json.utf8) }
    }

    /// The summary says `count` of the person's requests have news (the
    /// fixture's list has one: Arrival, ready).
    public func setRequestsUnseen(_ count: Int) {
        state.withLock { $0.requests.summary = Self.unseen(count, in: $0.requests.summary) }
    }

    static func unseen(_ count: Int, in summary: Data) -> Data {
        guard var object = dict(summary) else { return summary }
        object["unseen"] = count
        return (try? JSONSerialization.data(withJSONObject: object)) ?? summary
    }

    /// The summary of an administrator, who approves and declines.
    public func useManagerSummary() {
        state.withLock { $0.requests.summary = (try? Fixtures.data("requests/summary_manager.json")) ?? Data() }
    }

    /// Requests off on the server (or on again).
    public func setRequestsOff(_ off: Bool) {
        state.withLock { $0.requests.off = off }
    }

    /// The server is older than requests: the summary answers `404`.
    public func setRequestsMissing(_ missing: Bool) {
        state.withLock { $0.requests.missing = missing }
    }

    /// Paths starting with `prefix` answer `status` with `fixture` (an
    /// error's file, as `errors/tmdb_down.json`); `nil` answers again.
    public func failRequests(prefix: String, status: Int? = nil, fixture: String = "errors/tmdb_down.json") {
        state.withLock {
            if let status, let body = try? Fixtures.data(fixture) {
                $0.requests.failing[prefix] = (status, body)
            } else {
                $0.requests.failing[prefix] = nil
            }
        }
    }

    /// Asking answers `status` with `fixture`'s JSON (a request, or an error).
    public func answerAsk(status: Int, fixture: String) {
        state.withLock { $0.requests.askAnswer = (try? Fixtures.data(fixture)).map { (status, $0) } }
    }

    /// Asking answers request 44 as pending with `status`: `201` (new), or
    /// `200` (one this account follows now).
    public func answerAsk(status: Int) {
        state.withLock {
            $0.requests.askAnswer = nil
            $0.requests.askStatus = status
        }
    }

    /// The bodies of the asks sent, oldest first.
    public var askedBodies: [Data] { state.withLock { $0.requests.asked } }

    /// The decisions and removals sent, oldest first: "POST /api/requests/44/approve".
    public var requestActions: [String] { state.withLock { $0.requests.acted.map { "\($0.method) \($0.path)" } } }

    /// The bodies of the decisions sent, oldest first.
    public var requestActionBodies: [Data] { state.withLock { $0.requests.acted.map(\.body) } }

    /// How many times the person's news was marked seen.
    public var requestsSeenCalls: Int { state.withLock { $0.requests.seen } }

    /// Sends one of the Android app's event fixtures on the event stream (`events/request_available.json`).
    public func pushFixtureEvent(_ path: String) {
        guard let data = try? Fixtures.data(path), let object = try? JSONSerialization.jsonObject(with: data),
              let line = try? JSONSerialization.data(withJSONObject: object) else { return }
        pushEvent(String(decoding: line, as: UTF8.self))
    }

    // MARK: Routes

    static func requests(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        let path = request.path
        let get = request.method == "GET"
        // A library title as Discover sees it.
        if get, parts.count == 4, parts[1] == "items", let id = Int(parts[2]), parts[3] == "request" {
            return withToken(&s, request, scope: "requests") { s, _ in
                if let refused = refusal(s, path) { return refused }
                guard let file = s.requests.items[id] else { return notFound(path) }
                return fixture(file)
            }
        }
        guard parts[1] == "discover" || parts[1] == "requests" else { return nil }
        if get, path == "/api/requests/summary" {
            return withToken(&s, request) { s, access in
                if s.requests.missing { return notFound(path) }
                if s.requests.off { return json(200, ["enabled": false, "access": "off"]) }
                guard access.scope.split(separator: " ").contains("requests"),
                      var summary = dict(s.requests.summary) else {
                    // A sign-in from before requests: only what tells it to sign in again.
                    return json(200, ["enabled": true, "access": "ask", "sign_in_again": "requests", "pending": 0,
                                      "failed": 0, "unseen": 0, "own": 0, "manage": false])
                }
                if summary["manage"] as? Bool == true, !access.scope.split(separator: " ").contains("approvals") {
                    summary["sign_in_again"] = "approvals"
                }
                return json(200, summary)
            }
        }
        if parts[1] == "discover" {
            if get, parts.count == 5, parts[2] == "images" {
                return withToken(&s, request, scope: "requests") { _, _ in
                    let poster = parts[3] == "poster" || parts[3] == "profile"
                    return image(width: poster ? 200 : 400, height: poster ? 300 : 225, type: "public.jpeg",
                                 contentType: "image/jpeg")
                }
            }
            return withToken(&s, request, scope: "requests") { s, _ in
                if let refused = refusal(s, path) { return refused }
                return discover(s, request, parts)
            }
        }
        let scope = ["approve", "decline", "handled"].contains(parts.last ?? "") || path == "/api/requests/options"
            ? "approvals" : "requests"
        return withToken(&s, request, scope: scope) { s, _ in
            if let refused = refusal(s, path) { return refused }
            return requestRoute(&s, request, parts)
        }
    }

    /// Requests off, or a failure a test asked for.
    private static func refusal(_ s: State, _ path: String) -> Answer? {
        if s.requests.off { return json(503, ["code": "unavailable", "setup": false, "message": "Requests are off"]) }
        guard let failing = s.requests.failing.first(where: { path.hasPrefix($0.key) })?.value else { return nil }
        return Answer(status: failing.status, headers: ["Content-Type": "application/json"], body: failing.body)
    }

    private static func discover(_ s: State, _ request: Request, _ parts: [String]) -> Answer {
        guard request.method == "GET" else { return notFound(request.path) }
        if parts.count == 4, let id = Int(parts[3]), parts[2] == "movie" || parts[2] == "show" {
            guard id != 0 else { return fixture("errors/not_found.json").with(status: 404) }
            if let asked = dict(s.requests.askedTitles["\(parts[2]):\(id)"]),
               var page = dict(try? Fixtures.data("discover/\(parts[2]).json")) {
                page["status"] = "requested"
                page["request_id"] = asked["id"]
                page["requests"] = [asked]
                return json(200, page)
            }
            return fixture("discover/\(parts[2]).json")
        }
        guard parts.count == 3, var trending = dict(try? Fixtures.data("discover/trending.json")) else {
            return notFound(request.path)
        }
        let page = Int(request.query["page"] ?? "") ?? 1
        if parts[2] == "search" {
            let q = request.query["q"] ?? ""
            if q.isEmpty { return json(400, ["code": "invalid", "message": "q is empty"]) }
            if q.contains("nothing") { return json(200, ["page": 1, "total_pages": 1, "results": [Any]()]) }
            if request.query["missing"] == "1" {
                let results = trending["results"] as? [[String: Any]] ?? []
                trending["results"] = results.filter { $0["status"] as? String != "library" }
            }
        } else if DiscoverList(rawValue: parts[2]) == nil {
            return notFound(request.path)
        }
        // Page 2 gives the first page's titles again (de-duplicated by the
        // app), page 3 none: a short page isn't the end.
        if page >= 3 { return json(200, ["page": page, "total_pages": 3, "results": [Any]()]) }
        trending["page"] = page
        return json(200, trending)
    }

    private static func requestRoute(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer {
        let path = request.path
        switch (request.method, parts.count) {
        case ("GET", 2):
            let file = request.query["attention"] == "1" ? "requests/attention.json" : "requests/list.json"
            guard var list = dict(try? Fixtures.data(file)) else { return notFound(path) }
            var all = (list["requests"] as? [[String: Any]] ?? []).compactMap { current(s, $0) }
            if request.query["unseen"] == "1" { all = all.filter { $0["unseen"] as? Bool == true } }
            if request.query["mine"] == "1" { all = all.filter { $0["mine"] as? Bool == true || $0["watching"] as? Bool == true } }
            if let kind = request.query["kind"] { all = all.filter { $0["kind"] as? String == kind } }
            if let status = request.query["status"] {
                let statuses = status.split(separator: ",").map(String.init)
                all = all.filter { statuses.contains($0["status"] as? String ?? "") }
            }
            if let q = request.query["q"]?.lowercased() {
                all = all.filter { ($0["title"] as? String ?? "").lowercased().contains(q) }
            }
            let offset = Int(request.query["offset"] ?? "") ?? 0
            let limit = Int(request.query["limit"] ?? "") ?? 30
            list["total"] = all.count
            list["requests"] = Array(all.dropFirst(offset).prefix(limit))
            return json(200, list)
        case ("POST", 2):
            s.requests.asked.append(request.body)
            if let answer = s.requests.askAnswer {
                return Answer(status: answer.status, headers: ["Content-Type": "application/json"], body: answer.body)
            }
            guard var asked = find(s, 44) else { return notFound(path) }
            asked["status"] = "pending"
            asked["stage"] = "pending"
            asked["mine"] = true
            asked["actions"] = ["withdraw"]
            // The request is for the title asked for, and its page says it's requested from now on.
            let body = dict(request.body)
            if let kind = body?["kind"] as? String, let tmdbId = body?["tmdb_id"] as? Int {
                asked["kind"] = kind
                asked["tmdb_id"] = tmdbId
                s.requests.askedTitles["\(kind):\(tmdbId)"] = try? JSONSerialization.data(withJSONObject: asked)
            }
            return json(s.requests.askStatus, asked)
        case ("POST", 3) where parts[2] == "seen":
            s.requests.seen += 1
            // The summary reads nothing new from now on.
            s.requests.summary = unseen(0, in: s.requests.summary)
            return Answer(status: 204, headers: [:], body: Data())
        case ("GET", 3) where parts[2] == "options":
            return fixture("requests/options.json")
        case ("GET", 3):
            guard let id = Int(parts[2]), let found = find(s, id) else { return notFound(path) }
            return json(200, found)
        case ("DELETE", 3), ("DELETE", 4):
            guard let id = Int(parts[2]), find(s, id) != nil else { return notFound(path) }
            s.requests.acted.append((request.method, path, request.body))
            s.requests.gone.insert(id)
            return Answer(status: 204, headers: [:], body: Data())
        case ("POST", 4):
            guard let id = Int(parts[2]), var found = find(s, id) else { return notFound(path) }
            s.requests.acted.append((request.method, path, request.body))
            switch parts[3] {
            case "approve", "handled":
                found["status"] = "approved"
                found["stage"] = parts[3] == "handled" ? "approved" : "searching"
                found["actions"] = ["decline", "delete"]
            case "decline":
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any]
                found["status"] = "declined"
                found["stage"] = "declined"
                found["reason"] = body?["reason"] ?? NSNull()
                found["actions"] = ["delete"]
            default:
                return notFound(path)
            }
            s.requests.changed[id] = try? JSONSerialization.data(withJSONObject: found)
            return json(200, found)
        default:
            return notFound(path)
        }
    }

    /// Request `id` as it is now: from the lists, changed by the actions, or gone.
    private static func find(_ s: State, _ id: Int) -> [String: Any]? {
        guard !s.requests.gone.contains(id) else { return nil }
        if let changed = dict(s.requests.changed[id]) { return changed }
        for file in ["requests/list.json", "requests/attention.json"] {
            let all = dict(try? Fixtures.data(file))?["requests"] as? [[String: Any]] ?? []
            if let found = all.first(where: { $0["id"] as? Int == id }) { return found }
        }
        return nil
    }

    private static func current(_ s: State, _ request: [String: Any]) -> [String: Any]? {
        guard let id = request["id"] as? Int else { return request }
        if s.requests.gone.contains(id) { return nil }
        return dict(s.requests.changed[id]) ?? request
    }

    private static func dict(_ data: Data?) -> [String: Any]? {
        data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
    }
}

extension FixtureServer.Answer {
    func with(status: Int) -> FixtureServer.Answer {
        var answer = self
        answer.status = status
        return answer
    }
}
