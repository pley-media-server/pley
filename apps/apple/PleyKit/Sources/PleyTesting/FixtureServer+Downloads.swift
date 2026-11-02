import Foundation
import PleyModel

/// Downloads (docs/API.md, "Downloads"): the original of a playable from
/// `/api/files/{id}/stream` with `Range`, or a conversion the test steps
/// through (`queued`, `running`, `done`) whose file is served with `Range`.
extension FixtureServer {
    struct Conversions {
        /// `true`: every request is converted, as for a file this device doesn't play.
        var convert = false
        /// How far each poll moves a conversion on.
        var step = 0.5
        /// What the conversions make; the playable's media when `nil`.
        var made: Data?
        var running: [String: Running] = [:]
        var requests: [Data] = []
        var cancelled: [String] = []
        /// `/api/conversions` answers this status (`501`: the server can't convert).
        var refuse: Int?
        /// The media routes answer this status from now on (`nil`: normally).
        var mediaStatus: Int?
    }

    struct Running {
        var itemId: Int
        var progress: Double
        var failed = false
    }

    // MARK: Knobs

    /// Whether every download is converted (`true`), and how far each poll moves one on.
    public func setConversions(convert: Bool, step: Double = 0.5, made: Data? = nil) {
        state.withLock {
            $0.conversions.convert = convert
            $0.conversions.step = step
            $0.conversions.made = made
        }
    }

    /// `POST /api/conversions` answers `status` (`501`, `503`), or normally with `nil`.
    public func refuseConversions(status: Int?) {
        state.withLock { $0.conversions.refuse = status }
    }

    /// The download routes (the original's stream, a conversion's file) answer `status`, or normally with `nil`.
    public func setDownloadStatus(_ status: Int?) {
        state.withLock { $0.conversions.mediaStatus = status }
    }

    /// The conversion of `id` fails.
    public func failConversion(_ id: String) {
        state.withLock { $0.conversions.running[id]?.failed = true }
    }

    /// The bodies of `POST /api/conversions`, in order.
    public var conversionRequests: [Data] { state.withLock { $0.conversions.requests } }

    /// The conversions the app stopped (`DELETE`).
    public var cancelledConversions: [String] { state.withLock { $0.conversions.cancelled } }

    /// The `Range` headers asked of the download routes, in order.
    public var downloadRanges: [String] {
        journal.filter { $0.path.hasSuffix("/stream") || $0.path.hasSuffix("/file") }.compactMap { $0.headers["range"] }
    }

    // MARK: Routes

    static func downloads(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        switch (request.method, parts.count, parts[1]) {
        case ("POST", 2, "conversions"):
            return withToken(&s, request, scope: "play") { s, _ in
                s.conversions.requests.append(request.body)
                if let status = s.conversions.refuse {
                    return json(status, ["code": "unavailable", "message": "the transcoder can't convert"])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                guard let itemId = body["item_id"] as? Int, let playable = s.playables[itemId] else {
                    return json(404, ["code": "not_found", "message": "item not found"])
                }
                if !s.conversions.convert, body["max_height"] == nil {
                    return json(200, ["id": NSNull(), "state": "direct", "progress": 1, "size": playable.media.count,
                                      "error": NSNull(), "url": "/api/files/\(playable.fileId)/stream"])
                }
                let id = String(randomToken().prefix(32))
                s.conversions.running[id] = Running(itemId: itemId, progress: 0)
                return json(200, conversion(s, id))
            }
        case ("GET", 3, "conversions"):
            return withToken(&s, request, scope: "play") { s, _ in
                let id = parts[2]
                guard var running = s.conversions.running[id] else { return notFound(request.path) }
                if !running.failed { running.progress = min(running.progress + s.conversions.step, 1) }
                s.conversions.running[id] = running
                return json(200, conversion(s, id))
            }
        case ("DELETE", 3, "conversions"):
            return withToken(&s, request, scope: "play") { s, _ in
                s.conversions.cancelled.append(parts[2])
                s.conversions.running[parts[2]] = nil
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("GET", 4, "conversions") where parts[3] == "file":
            return withToken(&s, request, scope: "play") { s, _ in
                if let status = s.conversions.mediaStatus { return json(status, ["code": "x", "message": "refused"]) }
                guard let running = s.conversions.running[parts[2]], running.progress >= 1,
                      let playable = s.playables[running.itemId]
                else { return notFound(request.path) }
                return ranged(s.conversions.made ?? playable.media, range: request.headers["range"], type: "video/mp4")
            }
        case ("GET", 4, "files") where parts[3] == "stream":
            return withToken(&s, request, scope: "play") { s, _ in
                if let status = s.conversions.mediaStatus { return json(status, ["code": "x", "message": "refused"]) }
                guard let fileId = Int(parts[2]),
                      let playable = s.playables.values.first(where: { $0.fileId == fileId })
                else { return notFound(request.path) }
                return ranged(playable.media, range: request.headers["range"],
                              type: playable.container == "mkv" ? "video/x-matroska" : "video/mp4")
            }
        default:
            return nil
        }
    }

    private static func conversion(_ s: State, _ id: String) -> [String: Any] {
        let running = s.conversions.running[id]
        let done = (running?.progress ?? 0) >= 1
        let made = s.conversions.made ?? running.flatMap { s.playables[$0.itemId] }?.media ?? Data()
        let state = running?.failed == true ? "failed" : done ? "done" : (running?.progress ?? 0) > 0 ? "running" : "queued"
        return ["id": id, "state": state, "progress": running?.progress ?? 0,
                "size": done ? made.count as Any : NSNull(), "error": NSNull(), "url": "/api/conversions/\(id)/file"]
    }
}
