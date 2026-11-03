import Foundation

/// What someone asked to play: an item, maybe a version and a place to
/// start, and the collection or playlist it plays through. The Mac's player
/// window is opened with one.
public struct PlayRequest: Codable, Hashable, Sendable {
    public var itemId: Int
    /// The version (file) to play; `nil` lets the app pick the best one for this device.
    public var fileId: Int?
    /// Where to start; `nil` resumes where the profile left off, unless it was watched.
    public var startMs: Int?
    /// Play from the start, whatever the resume point.
    public var fromStart: Bool
    /// The collection or playlist to keep playing, in order; `nil` for a title by itself.
    public var queue: PlayQueue?

    public init(itemId: Int, fileId: Int? = nil, startMs: Int? = nil, fromStart: Bool = false,
                queue: PlayQueue? = nil) {
        self.itemId = itemId
        self.fileId = fileId
        self.startMs = startMs
        self.fromStart = fromStart
        self.queue = queue
    }
}

/// A collection's or a playlist's videos, played one after another (the
/// web's `videoQueue`, Android's `PlayQueue`): which it is, the items in
/// its order, and which of them plays now. The items travel with the
/// request (at most 5,000, a playlist's most), so what comes next needs no
/// fresh read of the list, and an item in a playlist twice keeps its place.
public struct PlayQueue: Codable, Hashable, Sendable {
    public enum Kind: String, Codable, Hashable, Sendable {
        case collection, playlist
    }

    /// A playlist holds at most this many entries.
    public static let maxItems = 5_000

    public var kind: Kind
    /// The collection's or playlist's id.
    public var id: Int
    /// Its videos, in order.
    public private(set) var itemIds: [Int]
    /// Which of `itemIds` plays now.
    public private(set) var position: Int

    /// `itemIds` cut to `maxItems`; `position` kept inside them.
    public init(kind: Kind, id: Int, itemIds: [Int], position: Int = 0) {
        self.kind = kind
        self.id = id
        self.itemIds = Array(itemIds.prefix(Self.maxItems))
        self.position = min(max(position, 0), max(self.itemIds.count - 1, 0))
    }

    /// The item playing now.
    public var current: Int? { itemIds.indices.contains(position) ? itemIds[position] : nil }

    /// The item after this one; `nil` at the end.
    public var nextId: Int? { itemIds.indices.contains(position + 1) ? itemIds[position + 1] : nil }

    /// The queue moved on to the next item; `nil` at the end.
    public func advanced() -> PlayQueue? {
        guard nextId != nil else { return nil }
        var next = self
        next.position = position + 1
        return next
    }
}
