import Foundation
import PleyModel

/// A request without a body.
private struct NoBody: Encodable, Sendable {}

/// Live TV (docs/API.md, "Live TV"): whether it shows, its groups, a
/// category's channels a page at a time, the guide of the channels in
/// view, favourites, and watching a channel. A provider lists tens of
/// thousands of channels: nothing here asks for them all.
extension PleyAPI {
    /// `GET /api/live`: whether Live TV is open to this profile and has channels.
    public func liveSummary() async throws -> LiveSummary {
        try await client.get("/api/live", maxBytes: BodyLimit.small, as: LiveSummary.self)
    }

    /// `GET /api/live/groups`: the groups in the lineup's order, and the counts.
    public func liveGroups() async throws -> LiveGroups {
        try await client.get("/api/live/groups", as: LiveGroups.self)
    }

    /// `GET /api/live/browse`: `limit` channels of `category` matching
    /// `query`, from `offset`; with `around`, the page that holds that channel.
    public func liveBrowse(_ category: LiveCategory, query: String = "", offset: Int = 0, limit: Int = 100,
                           around: Int? = nil) async throws -> LiveChannelPage {
        var items = [URLQueryItem(name: "category", value: category.wire)]
        if case .group(let name) = category { items.append(URLQueryItem(name: "group", value: name)) }
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if !q.isEmpty { items.append(URLQueryItem(name: "q", value: q)) }
        if let around {
            items.append(URLQueryItem(name: "around", value: String(around)))
        } else {
            items.append(URLQueryItem(name: "offset", value: String(max(offset, 0))))
        }
        items.append(URLQueryItem(name: "limit", value: String(min(max(limit, 1), 500))))
        return try await client.get("/api/live/browse", query: items, as: LiveChannelPage.self)
    }

    /// `GET /api/live/channels/{id}`: one channel, with what's on now and next.
    public func liveChannel(_ id: Int) async throws -> LiveChannel {
        try await client.get("/api/live/channels/\(id)", as: LiveChannel.self)
    }

    /// `GET /api/live/guide`: the programmes of `ids` between `from` and `to` (unix seconds).
    public func liveGuide(from: Int64, to: Int64, ids: [Int]) async throws -> LiveGuide {
        try await client.get("/api/live/guide", query: [
            URLQueryItem(name: "from", value: String(from)),
            URLQueryItem(name: "to", value: String(to)),
            URLQueryItem(name: "ids", value: ids.map(String.init).joined(separator: ",")),
        ], as: LiveGuide.self)
    }

    /// `PUT` or `DELETE /api/live/channels/{id}/favorite`.
    public func setFavoriteChannel(_ id: Int, favorite: Bool) async throws {
        _ = try await client.send(favorite ? "PUT" : "DELETE", "/api/live/channels/\(id)/favorite",
                                  body: NoBody?.none, as: NoContent.self)
    }

    /// `POST /api/live/channels/{id}/watch`: a session watching the
    /// channel, and its keyed live playlist. Every connection of its source
    /// in use, it throws `PleyError.busy`, saying what holds each one.
    public func watchChannel(_ id: Int, _ request: LiveWatchRequest) async throws -> LiveWatch {
        try await client.send("POST", "/api/live/channels/\(id)/watch", body: request, as: LiveWatch.self)
    }

    /// `GET /api/live/sessions/{session}/plan`: what the transcoder makes
    /// of the channel; `nil` when the server doesn't know in time. A
    /// stopped session answers `410` (`.sessionStopped`).
    public func livePlan(_ sessionId: String) async throws -> LivePlan? {
        try await client.get("/api/live/sessions/\(Self.liveSegment(sessionId))/plan", maxBytes: BodyLimit.small,
                             as: LivePlan?.self)
    }

    /// `DELETE /api/live/sessions/{session}`: stops watching.
    public func leaveChannel(_ sessionId: String) async throws {
        _ = try await client.send("DELETE", "/api/live/sessions/\(Self.liveSegment(sessionId))", body: NoBody?.none,
                                  as: NoContent.self)
    }

    /// The address a player opens for a channel watched with keyed URLs:
    /// `path` (`/api/live/media/{key}/index.m3u8`, as `LiveWatch.url` gives
    /// it) on this server's origin, after Pley's plain http rule. The key in
    /// the path is all it carries, so nothing but a keyed live path is
    /// accepted. Never logged.
    public func liveMediaURL(_ path: String) async throws -> URL {
        let bare = path.split(separator: "?", maxSplits: 1).first.map(String.init) ?? path
        let parts = bare.split(separator: "/", omittingEmptySubsequences: false)
        guard path.hasPrefix("/api/live/media/"), parts.count >= 6, !parts[4].isEmpty,
              !parts.contains(where: { $0 == ".." || $0 == "." }), !bare.contains("%"),
              let url = client.url(path)
        else { throw PleyError.unreachable(PleyError.notItsMedia) }
        try await client.checkPlayerURL(url)
        return url
    }

    /// A session id as a path segment: letters, digits, `-` and `_` only.
    private static func liveSegment(_ id: String) -> String {
        String(id.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") })
    }
}
