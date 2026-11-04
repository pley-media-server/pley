import Foundation
import PleyModel

/// A request without a body.
private struct NoBody: Encodable, Sendable {}

/// Items and playback (docs/API.md, "Items" and "Playback").
extension PleyAPI {
    /// `GET /api/items/{id}`.
    public func item(_ id: Int) async throws -> ItemDetail {
        try await client.get("/api/items/\(id)", as: ItemDetail.self)
    }

    /// `GET /api/items/{id}/on-deck`: the episode a show's or season's Play starts.
    public func onDeck(_ id: Int) async throws -> ItemSummary? {
        try await client.get("/api/items/\(id)/on-deck", as: MaybeItem.self).item
    }

    /// `GET /api/items/{id}/next`: the episode after this one.
    public func next(_ id: Int) async throws -> ItemSummary? {
        try await client.get("/api/items/\(id)/next", as: MaybeItem.self).item
    }

    /// `POST /api/playback`: a session, and how the item plays in it.
    public func startPlayback(_ request: PlaybackRequest) async throws -> PlaybackInfo {
        try await client.send("POST", "/api/playback", body: request, as: PlaybackInfo.self)
    }

    /// `GET /api/playback/{session}`: how it plays now. `404` once it
    /// expired, `410` (`.sessionStopped`) once it was stopped.
    public func playbackStatus(_ sessionId: String) async throws -> SessionStatus {
        try await client.get("/api/playback/\(Self.segment(sessionId))", as: SessionStatus.self)
    }

    /// `DELETE /api/playback/{session}`: ends it, and any transcode.
    public func stopPlayback(_ sessionId: String) async throws {
        _ = try await client.send("DELETE", "/api/playback/\(Self.segment(sessionId))", body: NoBody?.none,
                                  as: NoContent.self)
    }

    /// `POST /api/items/{id}/progress`. A stopped session answers `410`
    /// (`.sessionStopped`), an expired one `404`.
    public func reportProgress(itemId: Int, _ report: ProgressReport) async throws {
        _ = try await client.send("POST", "/api/items/\(itemId)/progress", body: report, as: NoContent.self)
    }

    /// `PUT /api/items/{id}/streams`: the tracks picked, for the whole show or the movie.
    public func setStreams(itemId: Int, _ choice: StreamsChoice) async throws {
        _ = try await client.send("PUT", "/api/items/\(itemId)/streams", body: choice, as: NoContent.self)
    }

    /// `GET /api/playback/network?client_id=`: what the server remembers of this network.
    public func playbackNetwork(clientId: String) async throws -> NetworkAnswer {
        try await client.get("/api/playback/network", query: [URLQueryItem(name: "client_id", value: clientId)],
                             maxBytes: BodyLimit.small, as: NetworkAnswer.self)
    }

    /// `PUT /api/playback/network`: a speed test's result, or Automatic's ceiling.
    public func reportNetwork(_ report: NetworkReport) async throws -> NetworkAnswer {
        try await client.send("PUT", "/api/playback/network", body: report, as: NetworkAnswer.self)
    }

    /// `GET /api/playback/speedtest?bytes=`, read as it comes: timed from
    /// the first byte, and stopped `readMs` after it or when all arrived.
    /// `timeoutMs` bounds the whole request, the wait for the first byte
    /// included; a test cut short after its first byte still says what it
    /// measured (docs/API.md, "Network memory").
    public func speedTest(bytes: Int, readMs: Int, timeoutMs: Int) async throws -> SpeedSample {
        let reader = SpeedReader(expected: bytes, readMs: readMs)
        let client = client
        let query = [URLQueryItem(name: "bytes", value: String(bytes))]
        do {
            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    // The bytes as they travel: nothing to unpack, nothing counted twice.
                    try await client.bytes("/api/playback/speedtest", query: query,
                                           headers: ["Accept": "application/octet-stream",
                                                     "Accept-Encoding": "identity", "Cache-Control": "no-store"],
                                           read: reader.read)
                }
                group.addTask {
                    try await Task.sleep(for: .milliseconds(max(timeoutMs, 0)))
                    throw PleyError.timedOut
                }
                defer { group.cancelAll() }
                try await group.next()
            }
        } catch {
            guard let sample = reader.sample else { throw error }
            return sample
        }
        return reader.sample ?? SpeedSample(bytes: 0, ms: 0, complete: false)
    }

    /// The address a player opens for a session's media: `path` (a keyed
    /// `/api/media/{key}/…`, as `PlaybackInfo.url` gives it) on this
    /// server's origin, after Pley's plain http rule. The player sends no
    /// token: the key in the path is all it carries, so nothing but a
    /// keyed media path is accepted. Never logged.
    public func mediaURL(_ path: String) async throws -> URL {
        let bare = path.split(separator: "?", maxSplits: 1).first.map(String.init) ?? path
        let parts = bare.split(separator: "/", omittingEmptySubsequences: false)
        guard path.hasPrefix("/api/media/"), parts.count >= 5, !parts[3].isEmpty,
              !parts.contains(where: { $0 == ".." || $0 == "." }), !bare.contains("%"),
              let url = client.url(path)
        else { throw PleyError.unreachable("the server gave an address that isn't its media's") }
        try await client.checkPlayerURL(url)
        return url
    }

    /// A session id as a path segment: letters and digits only.
    static func segment(_ id: String) -> String {
        String(id.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") })
    }
}
