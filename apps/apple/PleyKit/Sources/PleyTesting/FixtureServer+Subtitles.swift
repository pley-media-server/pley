import Foundation
import PleyModel

/// Added subtitles (docs/API.md, "Added subtitles"): a search that answers
/// the results a test set (or `503`, as a server without OpenSubtitles set
/// up, or `429`), and an add that puts a text track on the playable, with
/// its WebVTT served as the server serves its tracks.
extension FixtureServer {
    struct SubtitleSearch {
        var results: [SubtitleResult] = []
        /// `nil`: search answers; `false`: `503` not set up; `true`: `503` set up but failing.
        var unavailable: Bool?
        /// The add answers this status (`429`, …) with `refusalMessage`.
        var addRefusal: Int?
        var refusalMessage = "Your downloads for today are used up. They renew in 3 hours."
        /// What each search asked for, its query.
        var searches: [[String: String]] = []
        var adds: [Data] = []
        var nextIndex = 9000
    }

    // MARK: Knobs

    /// What the search finds.
    public func setSubtitleResults(_ results: [SubtitleResult]) {
        state.withLock { $0.subtitleSearch.results = results }
    }

    /// Search answers `503`: `setup` false, as a server without an
    /// OpenSubtitles key; true, as one whose key OpenSubtitles refuses.
    /// `nil` answers again.
    public func setSubtitleSearch(unavailable setup: Bool?) {
        state.withLock { $0.subtitleSearch.unavailable = setup }
    }

    /// Adding answers `status` (`429`: the day's downloads used up), or normally with `nil`.
    public func refuseSubtitleAdd(status: Int?) {
        state.withLock { $0.subtitleSearch.addRefusal = status }
    }

    /// The queries of the searches made, oldest first.
    public var subtitleSearches: [[String: String]] { state.withLock { $0.subtitleSearch.searches } }

    /// The bodies of the adds sent, oldest first.
    public var subtitleAdds: [Data] { state.withLock { $0.subtitleSearch.adds } }

    // MARK: Routes

    static func subtitles(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 4, parts[0] == "api", parts[1] == "items", let itemId = Int(parts[2]),
              parts[3] == "subtitles"
        else { return nil }
        switch (request.method, parts.count) {
        case ("GET", 5) where parts[4] == "search":
            return withToken(&s, request, scope: "play") { s, _ in
                s.subtitleSearch.searches.append(request.query)
                if let setup = s.subtitleSearch.unavailable {
                    return json(503, ["code": "unavailable", "setup": setup,
                                      "message": setup ? "OpenSubtitles refused the server's key" :
                                          "Subtitle search isn't set up"])
                }
                guard s.playables[itemId] != nil else { return notFound(request.path) }
                let encoder = JSONEncoder()
                encoder.keyEncodingStrategy = .convertToSnakeCase
                let body = (try? encoder.encode(SubtitleResults(results: s.subtitleSearch.results))) ?? Data()
                return Answer(status: 200, headers: ["Content-Type": "application/json"], body: body)
            }
        case ("POST", 4):
            return withToken(&s, request, scope: "play") { s, _ in
                s.subtitleSearch.adds.append(request.body)
                if let status = s.subtitleSearch.addRefusal {
                    return json(status, ["code": "too_many_requests", "message": s.subtitleSearch.refusalMessage])
                }
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                guard var playable = s.playables[itemId], let id = body["result"] as? String,
                      let found = s.subtitleSearch.results.first(where: { $0.id == id })
                else { return json(400, ["code": "invalid", "message": "not valid subtitles"]) }
                let index = s.subtitleSearch.nextIndex
                s.subtitleSearch.nextIndex += 1
                let language = found.language == "en" ? "eng" : found.language
                var stream = MediaStream(index: index, kind: .subtitle, codec: "subrip", language: language)
                stream.title = found.name
                stream.textBased = true
                stream.external = true
                stream.added = true
                playable.subtitles.append(stream)
                s.playables[itemId] = playable
                let path = "/api/files/\(playable.fileId)/subtitles/\(index).vtt"
                s.files[path] = (type: "text/vtt", body: Data("""
                WEBVTT

                00:00:01.000 --> 00:00:30.000
                Found online.
                """.utf8), headers: [:])
                return json(201, ["index": index, "codec": "subrip", "language": language, "title": found.name,
                                  "default": false, "forced": found.forced, "external": true, "text_based": true,
                                  "url": path, "added": true, "deletable": true])
            }
        default:
            return nil
        }
    }
}
