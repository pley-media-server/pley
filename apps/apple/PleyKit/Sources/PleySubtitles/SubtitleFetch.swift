import Foundation
import PleyNetwork

/// Reads the server's subtitle tracks (docs/API.md, "Files"), with the
/// token, as every request: text as WebVTT (`….vtt`), pictures as a
/// Matroska file of their own (`….mks`). An answer may be partial while
/// the server still reads the track out of its file (`X-Pley-Complete: 0`):
/// it is used as it is, and asked for again for the rest.
public enum SubtitleFetch {
    /// The largest WebVTT read.
    public static let textMaxBytes = 16 << 20
    /// The largest `.mks` read (a film's PGS is tens of MB).
    public static let picturesMaxBytes = 512 << 20
    public static let completeHeader = "X-Pley-Complete"

    /// `/api/files/{fileId}/subtitles/{index}.vtt`.
    public static func textPath(fileId: Int, index: Int) -> String { "/api/files/\(fileId)/subtitles/\(index).vtt" }

    /// `/api/files/{fileId}/subtitles/{index}.mks`.
    public static func picturesPath(fileId: Int, index: Int) -> String { "/api/files/\(fileId)/subtitles/\(index).mks" }

    /// Whether `path` is a subtitle track's address on the server (the
    /// session's answer names them): under `/api/files/`, nothing climbing out.
    public static func isTrackPath(_ path: String) -> Bool {
        let bare = path.split(separator: "?", maxSplits: 1).first.map(String.init) ?? path
        return bare.hasPrefix("/api/files/") && !bare.split(separator: "/").contains { $0 == ".." || $0 == "." }
            && !bare.contains("%")
    }

    /// A text track's cues, and whether the server had all of them.
    public static func text(_ client: APIClient, path: String) async throws -> (track: TextCueTrack, complete: Bool) {
        let response = try await client.fetch(path, maxBytes: textMaxBytes, accept: ["text/vtt"])
        let cues = WebVTT.parse(String(decoding: response.body, as: UTF8.self))
        return (TextCueTrack(cues), complete(response.header(completeHeader)))
    }

    /// A picture track, downloaded into `file`: what an earlier, partial
    /// answer left there is kept and only the rest is asked for (`Range`).
    /// When the file then can't be read, it is asked for again whole.
    public static func pictures(_ client: APIClient, path: String,
                                file: URL) async throws -> (track: PictureTrack, complete: Bool) {
        let had = (try? FileManager.default.attributesOfItem(atPath: file.path)[.size] as? Int) ?? 0
        let answer = try await client.download(path, to: file, maxBytes: picturesMaxBytes, from: had,
                                               accept: ["video/x-matroska", "video/matroska"])
        // Nothing new past what was read: the server has it all.
        let complete = answer.status == 416 ? true : complete(answer.header(completeHeader))
        do {
            return (try MatroskaPictureTrack.read(data: try Data(contentsOf: file, options: .alwaysMapped)), complete)
        } catch where had > 0 {
            try? FileManager.default.removeItem(at: file)
            return try await pictures(client, path: path, file: file)
        }
    }

    /// `X-Pley-Complete: 0` says the answer is partial; anything else (or none) that it is whole.
    static func complete(_ header: String?) -> Bool {
        header?.trimmingCharacters(in: .whitespaces) != "0"
    }
}
