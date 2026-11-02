import CryptoKit
import Foundation

/// What Apple TV's Top Shelf shows of Pley: the Continue Watching row of
/// the server and profile signed in last, as the app last saw it. The app
/// writes it (`ShelfStore`); the Top Shelf extension only reads it, and
/// holds no sign-in and asks the server nothing.
public struct ShelfSnapshot: Codable, Hashable, Sendable {
    /// The most the shelf shows.
    public static let maxEntries = 10

    /// Whose row it is: `ShelfSnapshot.owner(origin:profileId:)`.
    public var owner: String
    /// The row, in the server's order, `maxEntries` at most.
    public var entries: [ShelfEntry]

    public init(owner: String, entries: [ShelfEntry]) {
        self.owner = owner
        self.entries = Array(entries.prefix(Self.maxEntries))
    }

    /// The owner key of `profileId` on the server at `origin`: the first 16
    /// hex digits of the origin's SHA-256 and the profile id. It names the
    /// server without its address.
    public static func owner(origin: String, profileId: Int) -> String {
        let hash = SHA256.hash(data: Data(origin.utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
        return "\(hash)-\(profileId)"
    }
}

/// One title on the shelf.
public struct ShelfEntry: Codable, Hashable, Sendable {
    public enum Kind: String, Codable, Hashable, Sendable {
        /// Part way through.
        case `continue`
        /// Next up: the next episode of a show.
        case next
    }

    /// How its picture is shaped.
    public enum Shape: String, Codable, Hashable, Sendable {
        /// 16:9: an episode's still or a backdrop.
        case wide
        /// 2:3: a poster, when the title has neither.
        case poster
    }

    public var itemId: Int
    /// The title, or an episode's show.
    public var title: String
    /// "S2 · E5" for an episode; empty otherwise.
    public var subtitle: String
    /// How far through, 0 to 1; `nil` when not started.
    public var progress: Double?
    public var kind: Kind
    /// The picture's file name beside the snapshot, `nil` when it has none.
    public var image: String?
    public var shape: Shape

    public init(itemId: Int, title: String, subtitle: String = "", progress: Double? = nil, kind: Kind,
                image: String? = nil, shape: Shape = .wide) {
        self.itemId = itemId
        self.title = title
        self.subtitle = subtitle
        self.progress = progress.map { min(max($0, 0), 1) }
        self.kind = kind
        self.image = image
        self.shape = shape
    }
}
