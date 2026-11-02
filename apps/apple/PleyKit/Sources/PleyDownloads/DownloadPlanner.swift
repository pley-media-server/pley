import Foundation
import PleyCaps
import PleyData
import PleyModel

/// The rules of downloads, pure: which file, which piece, what an answer
/// means, and what plays next (Android's `AppDownloads` and `DownloadRunner` rules).
enum DownloadPlanner {
    /// A file taller than this isn't picked when another fits: a phone, a
    /// tablet or a laptop shows 1080p, and a 4K file is several times larger.
    static let maxHeight = 1080
    /// Each piece is its own request, with a token of its own.
    static let pieceBytes: Int64 = 256 << 20
    /// A piece's token lasts at least this long when it goes.
    static let tokenLasting: TimeInterval = 10 * 60
    /// Room kept free beside a download, so the device doesn't fill up.
    static let spaceMargin: Int64 = 500 << 20
    /// How often a conversion the server is making is looked at.
    static let pollSeconds: Double = 5
    /// A conversion that doesn't move on for this long is given up.
    static let conversionPatience: TimeInterval = 15 * 60
    /// The width of the picture saved with a download.
    static let pictureWidth = 640
    /// A share of the duration past which a download played here counts as watched, as on the server.
    static let watchedShare = 0.9

    static let noSpace = "Not enough space on this device"
    static let conversionFailed = "The server couldn't make it for this device"
    static let conversionStalled = "The server stopped making it for this device"
    static let noConversions = "The server can't make smaller downloads: choose Original in Settings → Downloads"
    static let notPrivate = "The server's address isn't https or on your home network, so the download was stopped"
    static let nothingToDownload = "This title has no file to download"

    /// The file of `files` a download fetches: of those this device plays
    /// as they are (all when none does), the sharpest up to 1080p, else
    /// the smallest there is.
    static func pickFile(_ files: [MediaFile], caps: ClientCaps?) -> MediaFile? {
        guard files.count > 1 else { return files.first }
        let playable = caps.map { caps in files.filter { CapsPolicy.decide($0, caps) == .direct } } ?? files
        let candidates = playable.isEmpty ? files : playable
        let fitting = candidates.filter { (1...maxHeight).contains(height($0)) }
        if let best = fitting.max(by: { height($0) < height($1) }) { return best }
        return candidates.min { size($0) < size($1) }
    }

    private static func height(_ file: MediaFile) -> Int { file.height ?? file.video?.height ?? 0 }
    private static func size(_ file: MediaFile) -> Int64 { file.size > 0 ? Int64(file.size) : .max }

    /// What is kept of `detail` to download it: a movie or an episode
    /// with a file; `nil` for anything else.
    static func record(_ detail: ItemDetail, caps: ClientCaps?, origin: String, profileId: Int,
                       quality: DownloadQuality, now: Date) -> DownloadRecord? {
        guard detail.kind == .movie || detail.kind == .episode,
              let file = pickFile(detail.files, caps: caps) else { return nil }
        let s = detail.summary
        let episode = detail.kind == .episode
        let target = quality.target
        return DownloadRecord(
            origin: origin, profileId: profileId, itemId: detail.id, fileId: file.id, kind: detail.kind,
            title: s.title, showId: episode ? s.grandparentId : nil, showTitle: episode ? s.grandparentTitle : nil,
            seasonIndex: episode ? s.parentIndex : nil, episodeIndex: episode ? s.index : nil, year: s.year,
            durationMs: file.durationMs ?? detail.durationMs, container: file.container,
            sizeBytes: Int64(max(file.size, 0)), addedAt: now,
            positionMs: detail.userState.watched ? 0 : detail.userState.positionMs,
            watched: detail.userState.watched, maxHeight: target?.maxHeight, maxBitrateKbps: target?.maxBitrateKbps)
    }

    /// The next piece after the `have` bytes down: 256 MiB at most, to the
    /// end of a file of `size` (`0`: not known yet). `nil` when it is all down.
    static func piece(have: Int64, size: Int64, pieceBytes: Int64 = pieceBytes) -> ClosedRange<Int64>? {
        if size > 0 && have >= size { return nil }
        let end = size > 0 ? min(have + pieceBytes, size) - 1 : have + pieceBytes - 1
        return have...end
    }

    /// What a piece's answer means.
    enum Answer: Equatable {
        /// The bytes asked for, from `start`: appended; `total` is the whole file's size.
        case append(total: Int64?)
        /// The whole file (a server that ignored the range, asked from the start).
        case whole
        /// Nothing past what is down: it is whole already.
        case complete
        /// Not what was asked for: the file changed on the server, and the download starts again.
        case restart
        /// The server refused it.
        case refused(Int)
    }

    /// The answer `status` with `Content-Range` to a piece asked from `start`.
    static func check(status: Int, contentRange: String?, start: Int64, size: Int64) -> Answer {
        switch status {
        case 206:
            guard let range = contentRange.flatMap(parseRange), range.start == start else { return .restart }
            if size > 0, let total = range.total, total != size { return .restart }
            return .append(total: range.total)
        case 200:
            return start == 0 ? .whole : .restart
        case 416:
            return size > 0 && start >= size ? .complete : .restart
        default:
            return .refused(status)
        }
    }

    /// `bytes a-b/total` (`total` may be `*`).
    static func parseRange(_ header: String) -> (start: Int64, end: Int64, total: Int64?)? {
        let text = header.trimmingCharacters(in: .whitespaces)
        guard text.lowercased().hasPrefix("bytes ") else { return nil }
        let rest = text.dropFirst(6)
        let halves = rest.split(separator: "/", maxSplits: 1)
        guard halves.count == 2 else { return nil }
        let bounds = halves[0].split(separator: "-", maxSplits: 1)
        guard bounds.count == 2, let a = Int64(bounds[0]), let b = Int64(bounds[1]), a <= b else { return nil }
        return (a, b, Int64(halves[1]))
    }

    /// Whether a piece that failed partway goes on from its resume data:
    /// its request carries the token it started with, which must still
    /// have 10 minutes left. Otherwise the piece is asked for again from
    /// what the partial file holds, with a fresh token.
    static func resumes(tokenExpires: Date, now: Date) -> Bool {
        tokenExpires.timeIntervalSince(now) >= tokenLasting
    }

    /// What a failed download says, from the server's answer.
    static func message(status: Int) -> String {
        switch status {
        case 401, 403: "Sign in to the server again to download this"
        case 404, 410: "This file is no longer on the server"
        default: "The server couldn't send this file (\(status))"
        }
    }

    /// The episode of `now`'s show downloaded whole after it, by season and number, among `mine`.
    static func nextEpisode(_ mine: [DownloadRecord], after now: DownloadRecord) -> DownloadRecord? {
        guard now.kind == .episode, let show = now.showId else { return nil }
        func order(_ r: DownloadRecord) -> (Int, Int) { (r.seasonIndex ?? .max, r.episodeIndex ?? .max) }
        return mine.filter { $0.kind == .episode && $0.showId == show && $0.state == .done && order($0) > order(now) }
            .min { order($0) < order($1) }
    }

    /// Where `positionMs` of `durationMs` leaves an item played here: the
    /// position kept, or watched from the start at 90 % or more.
    static func played(positionMs: Int, durationMs: Int?) -> (positionMs: Int, watched: Bool) {
        if let durationMs, durationMs > 0, Double(positionMs) >= Double(durationMs) * watchedShare {
            return (0, true)
        }
        return (max(positionMs, 0), false)
    }
}
