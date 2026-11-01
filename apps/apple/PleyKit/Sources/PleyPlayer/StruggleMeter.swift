import Foundation

/// The decoder's frames so far, as AVPlayer's access log counts them: the
/// frames dropped, and the frames the time watched should have shown
/// (seconds watched × the video's frame rate).
public struct FrameCounts: Sendable, Equatable {
    public var expected: Int
    public var dropped: Int

    public init(expected: Int, dropped: Int) {
        self.expected = expected
        self.dropped = dropped
    }

    /// The counts of `sample` for a video of `frameRate` frames a second.
    public init(_ sample: EngineSample, frameRate: Double) {
        expected = Int(sample.watchedSeconds * frameRate)
        dropped = sample.droppedFrames
    }
}

/// Whether the video decoder keeps up (the Android app's `StruggleMeter`,
/// adapted to what AVPlayer tells): it struggles when it drops over 1 % of
/// its frames in 10 s of playback, or 10 frames or more between two
/// samples (2 s apart). AVFoundation counts no frames dropped in a row,
/// so the second rule stands in for Android's "10 in a row". Fed the
/// counts every 2 s while playing.
public struct StruggleMeter: Sendable {
    public static let windowMs: Int64 = 10_000
    public static let maxShare = 0.01
    public static let maxBurst = 10

    private var samples: [(at: Int64, counts: FrameCounts)] = []

    public init() {}

    /// Takes the counts at `atMs`; true when the decoder struggles.
    public mutating func sample(atMs: Int64, _ counts: FrameCounts) -> Bool {
        // The counts start over with each media.
        if let last = samples.last?.counts, counts.expected < last.expected || counts.dropped < last.dropped {
            samples.removeAll()
        }
        let previous = samples.last?.counts
        samples.append((atMs, counts))
        if let previous, counts.dropped - previous.dropped >= Self.maxBurst { return true }
        while samples.count > 2 && samples[1].at <= atMs - Self.windowMs { samples.removeFirst() }
        let (fromMs, from) = samples[0]
        if atMs - fromMs < Self.windowMs { return false }
        let dropped = counts.dropped - from.dropped
        let expected = max(counts.expected - from.expected, dropped)
        return expected > 0 && Double(dropped) / Double(expected) > Self.maxShare
    }

    /// A new media, a seek or a pause: the next window starts afresh.
    public mutating func reset() { samples.removeAll() }
}
