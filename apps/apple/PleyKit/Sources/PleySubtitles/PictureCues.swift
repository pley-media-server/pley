import Foundation

/// One sample of a picture track, decoded: what it shows, and how long it
/// shows when the sample says. Neither depends on the samples after it,
/// so it holds while the track grows.
public struct DecodedPicture: Equatable, Sendable {
    /// `nil` when the sample shows nothing: it clears, or couldn't be decoded.
    public let frame: PictureFrame?
    /// The block's duration, else the sample's own (VobSub's stop command,
    /// DVB's page time-out); `nil`: it shows until the next sample.
    public let ownDurationMs: Int?

    public init(frame: PictureFrame?, ownDurationMs: Int?) {
        self.frame = frame
        self.ownDurationMs = ownDurationMs
    }
}

/// Decodes a picture track's samples away from the main actor. A decode
/// can read back hundreds of samples and colour a 4K screen's pixels; the
/// decoders and their caches stay here, the only place they are used.
public actor PictureRenderer {
    private var subtitles: PictureSubtitles

    /// `nil` for a codec this app doesn't draw.
    public init?(_ track: PictureTrack) {
        guard let subtitles = PictureSubtitles.of(track) else { return nil }
        self.subtitles = subtitles
    }

    /// Sample `index`, decoded; `nil` when the decode was cancelled before it began.
    public func decode(_ index: Int) -> DecodedPicture? {
        guard !Task.isCancelled else { return nil }
        return subtitles.decoded(index)
    }

    /// The same track with more samples, read further out of its file: the
    /// samples before are the same, and stay decoded on the main actor.
    public func grow(_ track: PictureTrack) {
        if let more = PictureSubtitles.of(track) { subtitles = more }
    }
}

/// A picture track as the player draws it, on the main actor: what shows
/// is looked up among the samples decoded already, and the samples it
/// needs (the one at the playhead, the one after it) are listed in
/// `wanted` for a `PictureRenderer` to decode. A sample not decoded yet
/// shows nothing until it is.
public final class PictureCueTrack: CueTrack {
    /// Samples kept decoded before and after the playhead's: one before,
    /// for a step back, and the next two.
    static let keptBefore = 1
    static let keptAfter = 2

    private var times: [Int]
    private var decoded: [Int: DecodedPicture] = [:]
    private var asked: Set<Int> = []
    private var near = 0
    /// Tells this track's pictures apart from another's.
    private let id: String

    public init(_ track: PictureTrack, id: String = UUID().uuidString) {
        times = track.samples.map(\.timeMs)
        self.id = id
    }

    /// Samples in all.
    public var count: Int { times.count }

    /// Samples to decode, nearest first, not decoded or asked for yet:
    /// the one at the playhead last looked at and the one after it.
    public var wanted: [Int] {
        [near, near + 1].filter { times.indices.contains($0) && decoded[$0] == nil && !asked.contains($0) }
    }

    /// Sample `index` is being decoded: it isn't wanted again until it is.
    public func asking(_ index: Int) { asked.insert(index) }

    /// Sample `index`, decoded; `nil` when its decode was cancelled, so it is wanted again.
    public func decoded(_ index: Int, _ picture: DecodedPicture?) {
        asked.remove(index)
        guard let picture, times.indices.contains(index) else { return }
        decoded[index] = picture
        decoded = decoded.filter { $0.key >= near - Self.keptBefore && $0.key <= near + Self.keptAfter }
    }

    /// `track` read further: the same samples and more after them.
    public func grow(_ track: PictureTrack) {
        guard track.samples.count >= times.count else { return }
        times = track.samples.map(\.timeMs)
    }

    /// The last sample starting at or before `ms`, as a count of those.
    private func upTo(_ ms: Int) -> Int {
        var lo = 0
        var hi = times.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if times[mid] <= ms { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// When sample `i` stops showing, once decoded; `Int.max` when it shows on.
    private func endOf(_ i: Int, _ picture: DecodedPicture) -> Int {
        let next = i + 1 < times.count ? times[i + 1] : Int.max
        if let own = picture.ownDurationMs, own > 0 { return min(next, times[i] + own) }
        return next
    }

    public func shown(at ms: Int) -> ShownSubtitles {
        let i = upTo(ms) - 1
        near = max(i, 0)
        guard i >= 0, let picture = decoded[i], ms < endOf(i, picture), let frame = picture.frame,
              !frame.bitmaps.isEmpty
        else { return .none }
        return .picture(ShownPicture(id: "\(id):\(i)", frame: frame))
    }

    /// The sample at `ms` ends, or the next one starts. A sample not decoded
    /// yet changes nothing by itself: its decode says when it is in.
    public func nextChangeAfter(_ ms: Int) -> Int? {
        let after = upTo(ms)
        let next = after < times.count ? times[after] : nil
        let i = after - 1
        guard i >= 0, let picture = decoded[i] else { return next }
        let end = endOf(i, picture)
        return ms < end ? (end == Int.max ? nil : end) : next
    }
}
