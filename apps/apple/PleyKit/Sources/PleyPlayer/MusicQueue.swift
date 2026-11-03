import Foundation
import PleyModel

/// The tracks music plays and which one plays, a port of the Android
/// app's `MusicQueue` (the web's AudioPlayer.tsx).
///
/// The queue keeps the order it was given. Shuffled, it plays a
/// Fisher–Yates order of it that starts with the current track;
/// unshuffled, it goes back to the order given, at the same track. The
/// same track may be in it twice: positions, not ids, say which one plays.
/// There is no repeat, as on Android and the web.
public struct MusicQueue: Sendable {
    /// What Previous did.
    public enum Previous: Equatable, Sendable {
        /// The track starts again.
        case restart
        /// The track before plays.
        case moved
    }

    /// Previous restarts a track that has played longer than this.
    public static let restartAfterMs = 3_000

    private var given: [ItemSummary] = []
    /// Play position → position in `given`.
    private var order: [Int] = []
    /// Where in `order` the current track is.
    public private(set) var index = 0
    public private(set) var shuffle = false
    /// A random number below its argument (the tests give their own).
    private let random: @Sendable (Int) -> Int

    public init(random: @escaping @Sendable (Int) -> Int = { Int.random(in: 0..<$0) }) {
        self.random = random
    }

    public var count: Int { order.count }
    public var isEmpty: Bool { order.isEmpty }
    /// The tracks in the order they play.
    public var tracks: [ItemSummary] { order.map { given[$0] } }
    public var current: ItemSummary? { order.indices.contains(index) ? given[order[index]] : nil }
    public var hasNext: Bool { index + 1 < order.count }
    /// "Up next": what plays after the current track, in order.
    public var upNext: [ItemSummary] { hasNext ? order[(index + 1)...].map { given[$0] } : [] }

    /// Starts over with `tracks` at `start`; with `shuffle`, in a random
    /// order that begins with that track.
    public mutating func start(_ tracks: [ItemSummary], at start: Int = 0, shuffle: Bool = false) {
        given = tracks
        order = Array(tracks.indices)
        index = tracks.isEmpty ? 0 : min(max(start, 0), tracks.count - 1)
        self.shuffle = false
        if shuffle { setShuffle(true) }
    }

    /// Forgets every track.
    public mutating func clear() {
        start([])
    }

    /// Shuffles: the current track first, the rest after it in a random
    /// order. Unshuffles: the order given, at the same track.
    public mutating func setShuffle(_ on: Bool) {
        guard on != shuffle, !order.isEmpty else {
            shuffle = on && !order.isEmpty
            return
        }
        let playing = order[index]
        if on {
            var rest = order.enumerated().filter { $0.offset != index }.map(\.element)
            // Fisher–Yates.
            if rest.count > 1 {
                for i in stride(from: rest.count - 1, to: 0, by: -1) {
                    rest.swapAt(i, random(i + 1))
                }
            }
            order = [playing] + rest
            index = 0
        } else {
            order = Array(given.indices)
            index = playing
        }
        shuffle = on
    }

    /// Moves to the next track; `false` at the end.
    public mutating func next() -> Bool {
        guard hasNext else { return false }
        index += 1
        return true
    }

    /// What Previous does (the web's rule): after 3 s of the track, or on
    /// the first track, it starts again; otherwise the track before plays.
    public mutating func previous(positionMs: Int) -> Previous {
        if positionMs > Self.restartAfterMs || index == 0 { return .restart }
        index -= 1
        return .moved
    }

    /// Moves to the track at `position` of the play order; `false` when there is none.
    public mutating func jump(to position: Int) -> Bool {
        guard order.indices.contains(position) else { return false }
        index = position
        return true
    }
}
