import Foundation
import PleyModel

/// Downloads (docs/API.md, "Downloads"): the server decides, as for a
/// playback, whether the device downloads the original or a file it makes.
extension PleyAPI {
    /// The largest WebVTT a download keeps.
    public static let downloadSubtitleMaxBytes = 16 << 20

    /// `POST /api/conversions` with the play request's body: the original
    /// (`direct`) or the server's conversion for this device and quality.
    public func convert(_ request: PlaybackRequest) async throws -> DownloadInfo {
        try await client.send("POST", "/api/conversions", body: request, as: DownloadInfo.self)
    }

    /// `GET /api/conversions/{id}`: how far the conversion is. `404` once
    /// the server forgot it (it restarted): asked for again.
    public func conversion(_ id: String) async throws -> DownloadInfo {
        try await client.get("/api/conversions/\(Self.conversionSegment(id))", as: DownloadInfo.self)
    }

    /// `DELETE /api/conversions/{id}`: the download went, the server stops making its file.
    public func cancelConversion(_ id: String) async throws {
        _ = try await client.send("DELETE", "/api/conversions/\(Self.conversionSegment(id))", body: NoBody?.none,
                                  as: NoContent.self)
    }

    /// `/api/files/{id}/stream`: the original, with `Range`.
    public static func fileStreamPath(_ fileId: Int) -> String { "/api/files/\(fileId)/stream" }

    /// Whether `path` (a `DownloadInfo.url`) is a download's file on the
    /// server: the original's stream or a conversion's file, nothing else.
    public static func isDownloadPath(_ path: String) -> Bool {
        guard !path.contains("?"), !path.contains("%"), !path.contains("\\") else { return false }
        let parts = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 5, parts[0].isEmpty, parts[1] == "api", !parts.contains(".."), !parts.contains(".")
        else { return false }
        switch (parts[2], parts[4]) {
        case ("files", "stream"): return Int(parts[3]) != nil
        case ("conversions", "file"): return !parts[3].isEmpty && conversionSegment(parts[3]) == parts[3]
        default: return false
        }
    }

    /// A text subtitle track as the server's WebVTT, and whether it was
    /// whole (`X-Pley-Complete: 0` while the server still reads it).
    public func subtitleText(fileId: Int, index: Int) async throws -> (data: Data, complete: Bool) {
        let response = try await client.fetch("/api/files/\(fileId)/subtitles/\(index).vtt",
                                              maxBytes: Self.downloadSubtitleMaxBytes, accept: ["text/vtt"])
        return (response.body, response.header("X-Pley-Complete")?.trimmingCharacters(in: .whitespaces) != "0")
    }

    private static func conversionSegment(_ id: String) -> String {
        String(id.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") })
    }
}

private struct NoBody: Encodable, Sendable {}
