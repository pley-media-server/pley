import Foundation
import PleyNetwork

/// Gets a new session's stream ready while the last one plays on (a
/// quality change, Automatic's step): it is asked for as AVPlayer will ask
/// for it, so the transcoder makes the segment the player starts with
/// before the player moves there (the Android app's `StreamWarmer`, and
/// apps/web/src/player/prepare.ts).
public protocol StreamWarmer: Sendable {
    /// Asks for the HLS stream at `url` as a player starting at `startMs`
    /// would; returns once its segment has come.
    func warm(_ url: URL, startMs: Int) async throws
}

/// Asks for nothing: the player moves over at once.
public struct NoWarmer: StreamWarmer {
    public init() {}
    public func warm(_ url: URL, startMs: Int) async throws {}
}

/// `StreamWarmer` through the server's API client, as the player asks:
/// keyed addresses, no token, Pley's plain http rule. It reads the
/// playlist, the first size listed of a stream in several, the init
/// segment of an fMP4 one, and the segment that holds the start.
public struct APIStreamWarmer: StreamWarmer {
    /// The most a playlist may be.
    static let maxPlaylist = 4 << 20

    let client: APIClient

    public init(client: APIClient) {
        self.client = client
    }

    public func warm(_ url: URL, startMs: Int) async throws {
        var playlistURL = url
        var playlist = try await text(url)
        if let variant = HLSWarmup.variants(playlist, base: url).first {
            playlistURL = variant
            playlist = try await text(variant)
        }
        if let map = HLSWarmup.initSegment(playlist, base: playlistURL) {
            try await client.playerBytes(map) { _ in true }
        }
        guard let segment = HLSWarmup.segmentAt(playlist, base: playlistURL, positionMs: startMs) else { return }
        // Read to its end: the segment is whole once the transcoder made it.
        try await client.playerBytes(segment) { _ in true }
    }

    private func text(_ url: URL) async throws -> String {
        String(decoding: try await client.playerData(url, maxBytes: Self.maxPlaylist), as: UTF8.self)
    }
}

/// What a warm-up reads of HLS playlists.
public enum HLSWarmup {
    /// The sizes a multivariant playlist lists, in its order; none for a
    /// media playlist. AVPlayer starts with the first one listed, and the
    /// server makes that one first.
    public static func variants(_ playlist: String, base: URL) -> [URL] {
        let lines = playlist.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
        return lines.indices.filter { lines[$0].hasPrefix("#EXT-X-STREAM-INF") }.compactMap { i in
            lines[(i + 1)...].first { !$0.isEmpty && !$0.hasPrefix("#") }.flatMap { resolve($0, base) }
        }
    }

    /// An fMP4 playlist's init segment (`#EXT-X-MAP:URI="init.mp4"`).
    public static func initSegment(_ playlist: String, base: URL) -> URL? {
        guard let line = playlist.split(whereSeparator: \.isNewline).first(where: { $0.hasPrefix("#EXT-X-MAP:") }),
              let match = line.firstMatch(of: /URI="([^"]+)"/)
        else { return nil }
        return resolve(String(match.1), base)
    }

    /// The segment of a media playlist that holds `positionMs`: the last one for a time past its end.
    public static func segmentAt(_ playlist: String, base: URL, positionMs: Int) -> URL? {
        let seconds = Double(positionMs) / 1000
        var at = 0.0
        var duration: Double?
        var last: URL?
        for raw in playlist.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("#EXTINF:") {
                duration = Double(line.dropFirst(8).split(separator: ",", maxSplits: 1).first?
                    .trimmingCharacters(in: .whitespaces) ?? "")
            } else if !line.isEmpty, !line.hasPrefix("#"), let length = duration {
                guard let url = resolve(line, base) else { continue }
                last = url
                if seconds < at + length { return url }
                at += length
                duration = nil
            }
        }
        return last
    }

    private static func resolve(_ reference: String, _ base: URL) -> URL? {
        URL(string: reference, relativeTo: base)?.absoluteURL
    }
}
