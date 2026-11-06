import Foundation
import PleyModel

/// Search, the history and track defaults: search answers from the
/// fixture's results, keeping those whose title or name holds what was
/// asked; the history is 40 plays made from the fixture's three, newest
/// first, which a test can page through and remove; track defaults are
/// kept per profile and `GET /api/auth/me` says them, as it says each
/// profile's language.
extension FixtureServer {
    struct Account {
        /// `nil` until first asked for: then 40 plays, as (id, JSON).
        var history: [(id: Int, json: Data)]?
        /// `DELETE /api/history/{id}` answers this status from now on (`nil`: normally).
        var historyRefusal: Int?
        /// `PUT /api/me/track-defaults` answers this status from now on.
        var trackDefaultsRefusal: Int?
        /// What each profile set: audio language, subtitle language, mode.
        var trackDefaults: [Int: (audio: String?, subtitles: String?, mode: String)] = [:]
        /// Each profile's language (`PUT /api/me/language`); `nil`: Automatic.
        var languages: [Int: String] = [:]
        /// `PUT /api/me/language` answers this status from now on.
        var languageRefusal: Int?
        /// The server language `GET /api/server` says; `nil`: none chosen.
        var serverLanguage: String?
    }

    /// How many plays the fixture's history holds at first.
    public static let historyCount = 40

    // MARK: Knobs

    /// Answers the search for `query` `ms` later.
    public func delaySearch(_ query: String, ms: Int) {
        delay(path: "/api/search?q=\(query)", ms: ms)
    }

    /// `DELETE /api/history/{id}` answers this status from now on (`nil`: normally).
    public func refuseHistoryRemoval(status: Int?) {
        state.withLock { $0.account.historyRefusal = status }
    }

    /// `PUT /api/me/track-defaults` answers this status from now on (`nil`: normally).
    public func refuseTrackDefaults(status: Int?) {
        state.withLock { $0.account.trackDefaultsRefusal = status }
    }

    /// `PUT /api/me/language` answers this status from now on (`nil`: normally).
    public func refuseLanguage(status: Int?) {
        state.withLock { $0.account.languageRefusal = status }
    }

    /// Sets profile `id`'s language, as another device would (`nil`: Automatic).
    public func setLanguage(_ language: String?, profile id: Int) {
        state.withLock { $0.account.languages[id] = language }
    }

    /// Profile `id`'s language now; `nil`: Automatic.
    public func language(profile id: Int) -> String? {
        state.withLock { $0.account.languages[id] }
    }

    /// The server language `GET /api/server` says from now on; `nil`: none chosen.
    public func setServerLanguage(_ language: String?) {
        state.withLock { $0.account.serverLanguage = language }
    }

    /// The history's entry ids now, newest first.
    public var historyIds: [Int] {
        state.withLock { s in
            Self.seedHistory(&s)
            return (s.account.history ?? []).map(\.id)
        }
    }

    /// What profile `id` last set as its track defaults.
    public func trackDefaults(profile id: Int) -> (audio: String?, subtitles: String?, mode: String)? {
        state.withLock { $0.account.trackDefaults[id] }
    }

    // MARK: Routes

    static func account(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        switch (request.method, parts.count, parts[1]) {
        case ("GET", 2, "search"):
            return withToken(&s, request, scope: "library") { _, _ in search(request.query["q"] ?? "") }
        case ("GET", 2, "history"):
            return withToken(&s, request, scope: "library") { s, _ in
                seedHistory(&s)
                let all = s.account.history ?? []
                let offset = max(Int(request.query["offset"] ?? "") ?? 0, 0)
                let limit = min(max(Int(request.query["limit"] ?? "") ?? 50, 1), 200)
                let page = offset < all.count ? Array(all[offset..<min(offset + limit, all.count)]) : []
                let entries = page.compactMap { try? JSONSerialization.jsonObject(with: $0.json) }
                return json(200, ["total": all.count, "entries": entries])
            }
        case ("DELETE", 3, "history"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                seedHistory(&s)
                if let status = s.account.historyRefusal {
                    return json(status, ["code": "refused", "message": "not now"])
                }
                guard let at = s.account.history?.firstIndex(where: { $0.id == id }) else {
                    return notFound(request.path)
                }
                s.account.history?.remove(at: at)
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("PUT", 3, "me") where parts[2] == "track-defaults":
            return withToken(&s, request, scope: "play") { s, access in
                if let status = s.account.trackDefaultsRefusal {
                    return json(status, ["code": "refused", "message": "not now"])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                let mode = body["subtitle_mode"] as? String ?? "forced"
                guard ["off", "forced", "foreign", "always"].contains(mode) else {
                    return json(400, ["code": "bad_request", "message": "an unknown mode"])
                }
                for key in ["audio_language", "subtitle_language"] {
                    if let code = body[key] as? String, !(2...3).contains(code.count) {
                        return json(400, ["code": "bad_request", "message": "a language of 2 or 3 letters"])
                    }
                }
                s.account.trackDefaults[access.profile] = (body["audio_language"] as? String,
                                                           body["subtitle_language"] as? String, mode)
                return json(200, withTrackDefaults(s, me(profile: access.profile, scope: access.scope),
                                                   profile: access.profile))
            }
        case ("PUT", 3, "me") where parts[2] == "language":
            return withToken(&s, request, scope: "play") { s, access in
                if let status = s.account.languageRefusal {
                    return json(status, ["code": "refused", "message": "not now"])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                guard body.keys.contains("language") else {
                    return json(400, ["code": "bad_request", "message": "language is missing"])
                }
                let language = body["language"] as? String
                if let language, !["en", "fr", "es"].contains(language) {
                    return json(400, ["code": "bad_request", "message": "an unknown language"])
                }
                s.account.languages[access.profile] = language
                return json(200, withTrackDefaults(s, me(profile: access.profile, scope: access.scope),
                                                   profile: access.profile))
            }
        default:
            return nil
        }
    }

    /// `user` with the track defaults and the language its profile set here.
    static func withTrackDefaults(_ s: State, _ user: Any, profile: Int) -> Any {
        guard var user = user as? [String: Any] else { return user }
        user["language"] = s.account.languages[profile] ?? NSNull()
        guard let set = s.account.trackDefaults[profile] else { return user }
        user["audio_language"] = set.audio ?? NSNull()
        user["subtitle_language"] = set.subtitles ?? NSNull()
        user["subtitle_mode"] = set.mode
        return user
    }

    /// The fixture's results whose title or name holds `query`; nothing for a query matching none.
    private static func search(_ query: String) -> Answer {
        let all = object("search/results.json") ?? [:]
        let wanted = query.trimmingCharacters(in: .whitespaces).lowercased()
        func keeps(_ entry: [String: Any], _ key: String) -> Bool {
            !wanted.isEmpty && (entry[key] as? String ?? "").lowercased().contains(wanted)
        }
        let items = (all["items"] as? [[String: Any]] ?? []).filter { keeps($0, "title") }
        let people = (all["people"] as? [[String: Any]] ?? []).filter { keeps($0, "name") }
        let channels = (all["channels"] as? [[String: Any]] ?? []).filter { keeps($0, "name") }
        return json(200, ["items": items, "people": people, "channels": channels])
    }

    /// The history at first: the fixture's three plays over and over, an hour apart.
    private static func seedHistory(_ s: inout State) {
        guard s.account.history == nil else { return }
        let base = object("history/page.json")?["entries"] as? [[String: Any]] ?? []
        guard !base.isEmpty else {
            s.account.history = []
            return
        }
        s.account.history = (0..<historyCount).map { n in
            var entry = base[n % base.count]
            entry["id"] = 5_000 - n
            entry["viewed_at"] = 1_761_250_000 - n * 3_600
            return (5_000 - n, (try? JSONSerialization.data(withJSONObject: entry)) ?? Data())
        }
    }
}
