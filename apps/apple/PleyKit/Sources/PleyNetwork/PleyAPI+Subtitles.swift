import Foundation
import PleyModel

/// Added subtitles (docs/API.md, "Added subtitles"): searching online for
/// a title's file, and adding what was found.
extension PleyAPI {
    /// `GET /api/items/{id}/subtitles/search?file_id=&language=`: what
    /// OpenSubtitles has for the file, made for it first. `language` is a
    /// code ("en", "pt-BR"); left out, the server picks the profile's
    /// subtitle language, else its audio language, else English. A server
    /// without search set up answers `.notAvailable(setup: false)`; one
    /// whose search failed `.notAvailable(setup: true)` with why.
    public func searchSubtitles(itemId: Int, fileId: Int?, language: String? = nil) async throws -> [SubtitleResult] {
        var query: [URLQueryItem] = []
        if let fileId { query.append(URLQueryItem(name: "file_id", value: String(fileId))) }
        if let language, !language.isEmpty { query.append(URLQueryItem(name: "language", value: language)) }
        return try await client.get("/api/items/\(itemId)/subtitles/search", query: query, as: SubtitleResults.self)
            .results
    }

    /// `POST /api/items/{id}/subtitles` with a search result: the server
    /// downloads it and answers with the new text track (`429` when the
    /// day's downloads are used up, with when they renew).
    public func addSubtitle(itemId: Int, result: String, fileId: Int?) async throws -> SubtitleTrack {
        try await client.send("POST", "/api/items/\(itemId)/subtitles", body: AddSubtitle(result: result, fileId: fileId),
                              as: SubtitleTrack.self)
    }
}
