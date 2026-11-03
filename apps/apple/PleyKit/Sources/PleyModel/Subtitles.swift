import Foundation

/// A subtitle found online for a title's file (docs/API.md, "Added
/// subtitles"): `GET /api/items/{id}/subtitles/search` lists them, and
/// `POST /api/items/{id}/subtitles` adds one by its `id`.
public struct SubtitleResult: Codable, Hashable, Sendable, Identifiable {
    /// Opaque: what the add takes back ("opensubtitles:1923552:en").
    public var id: String
    public var provider: String
    /// OpenSubtitles' code: "en", "pt-BR".
    public var language: String
    /// The release it was made for, or its file's name.
    public var name: String
    /// What it is added as: "srt", "vtt", "ass", "ssa".
    public var format: String
    public var hearingImpaired: Bool
    /// Foreign parts only.
    public var forced: Bool
    public var downloads: Int
    /// "hash": made for this very file; "title": for the title.
    public var matched: String

    public init(id: String, provider: String = "opensubtitles", language: String, name: String,
                format: String = "srt", hearingImpaired: Bool = false, forced: Bool = false, downloads: Int = 0,
                matched: String = "title") {
        self.id = id
        self.provider = provider
        self.language = language
        self.name = name
        self.format = format
        self.hearingImpaired = hearingImpaired
        self.forced = forced
        self.downloads = downloads
        self.matched = matched
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        provider = try c.value(.provider, or: "")
        language = try c.value(.language, or: "")
        name = try c.value(.name, or: "")
        format = try c.value(.format, or: "srt")
        hearingImpaired = try c.value(.hearingImpaired, or: false)
        forced = try c.value(.forced, or: false)
        downloads = try c.value(.downloads, or: 0)
        matched = try c.value(.matched, or: "title")
    }

    /// Made for this very file (found by its hash).
    public var madeForFile: Bool { matched == "hash" }
}

/// The answer of the search.
public struct SubtitleResults: Codable, Hashable, Sendable {
    public var results: [SubtitleResult]

    public init(results: [SubtitleResult]) {
        self.results = results
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        results = try c.value(.results, or: [])
    }
}

/// The body that adds a search result.
public struct AddSubtitle: Codable, Hashable, Sendable {
    public var result: String
    public var fileId: Int?

    public init(result: String, fileId: Int?) {
        self.result = result
        self.fileId = fileId
    }
}
