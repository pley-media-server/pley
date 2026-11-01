import Foundation
import PleyModel
import PleyNetwork

/// What plays after an item (a port of the Android app's `NextUp`, without
/// queues and downloads, which come later): the next episode, fetched
/// fresh so its watch state is today's; movies and extras have none.
public enum NextUp {
    /// How long the Up Next card counts down before the next one plays.
    public static let countdownSeconds = 10
    /// Up Next is looked up from this share of the media on, when nothing marks the credits.
    public static let lookAheadShare = 0.9
    /// The width the card's picture is asked for at.
    public static let imageWidth = 480

    /// Whether `kind` can have something next, before asking.
    public static func hasNext(_ kind: ItemKind) -> Bool { kind == .episode }

    /// What follows `itemId` (of `kind`); `nil` at the end, or for anything but an episode.
    public static func after(_ itemId: Int, kind: ItemKind, api: PleyAPI) async throws -> ItemSummary? {
        guard hasNext(kind) else { return nil }
        return try await api.next(itemId)
    }

    /// Where `next` starts: from the start when watched, else its resume point.
    public static func startOf(_ next: ItemSummary) -> Int {
        next.userState.watched ? 0 : max(next.userState.positionMs, 0)
    }

    /// The card's heading: "Up next · S1 E2" for an episode, else "Up next".
    public static func heading(_ next: ItemSummary) -> String {
        let label = next.kind == .episode ? Words.episode(season: next.parentIndex, episode: next.index) : ""
        return label.isEmpty ? "Up next" : "Up next · \(label)"
    }
}
