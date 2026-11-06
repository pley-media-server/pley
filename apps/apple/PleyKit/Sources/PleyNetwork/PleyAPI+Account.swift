import Foundation
import PleyModel

/// Search, the profile's history and its track defaults (docs/API.md,
/// "Home & search", "History", "Your account", "Language").
extension PleyAPI {
    /// `GET /api/search?q=&limit=`: up to `limit` items, and people and
    /// channels besides.
    public func search(_ query: String, limit: Int = 40) async throws -> SearchResults {
        try await client.get("/api/search", query: [URLQueryItem(name: "q", value: query),
                                                    URLQueryItem(name: "limit", value: String(limit))],
                             as: SearchResults.self)
    }

    /// `GET /api/history?offset=&limit=`: the profile's plays, newest first.
    public func history(offset: Int, limit: Int) async throws -> HistoryPage {
        try await client.get("/api/history", query: [URLQueryItem(name: "offset", value: String(offset)),
                                                     URLQueryItem(name: "limit", value: String(limit))],
                             as: HistoryPage.self)
    }

    /// `DELETE /api/history/{id}`: one play out of the history.
    public func deleteHistory(_ id: Int) async throws {
        _ = try await client.send("DELETE", "/api/history/\(id)", body: Optional<NoBody>.none, as: NoContent.self)
    }

    /// `PUT /api/me/track-defaults`: all three at once; the profile as it is now.
    @discardableResult
    public func setTrackDefaults(_ defaults: TrackDefaults) async throws -> User {
        try await client.send("PUT", "/api/me/track-defaults", body: defaults, as: User.self)
    }

    /// `PUT /api/me/language`: the language the profile sees Pley in
    /// ("en", "fr", "es"; `nil`: Automatic, each device's); the profile as it is now.
    @discardableResult
    public func setLanguage(_ language: String?) async throws -> User {
        try await client.send("PUT", "/api/me/language", body: LanguageBody(language: language), as: User.self)
    }
}

/// `{ "language": "fr" }`, or `{ "language": null }` for Automatic: the key is always there.
private struct LanguageBody: Encodable, Sendable {
    var language: String?

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(language, forKey: .language)
    }

    private enum CodingKeys: String, CodingKey { case language }
}

/// A request without a body.
private struct NoBody: Encodable, Sendable {}
