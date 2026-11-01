import Foundation
import PleyModel

/// How a row's or a grid's tiles are shaped (Android's `RowShape`, the
/// web's `hubAspect`).
public enum RowShape: String, Sendable, Hashable {
    /// 16:9 stills: Continue Watching, and rows of episodes.
    case landscape
    /// 2:3 posters: movies, shows and seasons.
    case poster
    /// Music.
    case square

    /// The shape of `hub`'s tiles: Continue Watching as stills, music as
    /// squares, the rest by their first item.
    public static func of(_ hub: Hub) -> RowShape {
        if hub.key.hasPrefix("continue") { return .landscape }
        switch hub.items.first?.kind {
        case .episode?: return .landscape
        case .artist?, .album?, .track?: return .square
        default: return .poster
        }
    }
}

/// One tile as the screens draw it, every word made already, so a view's
/// body only lays it out (and SwiftUI compares tiles cheaply).
public struct Tile: Identifiable, Hashable, Sendable {
    public var id: Int { item.id }
    /// What it opens and plays.
    public let item: ItemSummary
    public let shape: RowShape
    /// The caption's first line: the title, an episode's show in a still.
    public let title: String
    /// Its second line: "2016", "3 seasons", "S2 · E5 · 26 min left"; empty for none.
    public let subtitle: String
    /// Server paths of its picture, the best first: the next is tried when one is missing.
    public let pictures: [String]
    /// Played so far, 0 to 1; 0 when not in progress.
    public let progress: Double
    /// Watched, all of it.
    public let watched: Bool
    /// A show's or season's episodes not watched yet.
    public let unwatched: Int
    /// What VoiceOver says: "Arrival, 2016"; its state ("watched") is `spokenValue`.
    public let spokenLabel: String
    public let spokenValue: String

    /// `item` as a tile of `shape` (Android's `ArtworkTileModel.of`).
    public init(_ item: ItemSummary, shape: RowShape) {
        self.item = item
        self.shape = shape
        let inProgress = Words.isInProgress(item)
        let left = inProgress ? Words.timeLeft(positionMs: item.userState.positionMs, durationMs: item.durationMs) : ""
        if shape == .landscape, item.kind == .episode {
            let show = item.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 } ?? item.title
            title = show
            subtitle = [Words.episodeLabel(season: item.parentIndex, episode: item.index),
                        left.isEmpty ? (show != item.title ? item.title : "") : left]
                .filter { !$0.isEmpty }.joined(separator: " · ")
        } else {
            title = item.title
            subtitle = left.isEmpty ? Words.cardSubtitle(item) : left
        }
        pictures = shape == .landscape ? Self.stills(item) : Self.posters(item)
        progress = inProgress ? Words.progress(item) : 0
        unwatched = item.kind == .show || item.kind == .season ? Words.unwatchedCount(item) : 0
        watched = item.userState.watched && unwatched == 0
        let spoken = Words.tileSpoken(item)
        spokenLabel = spoken.label
        spokenValue = spoken.value
    }

    /// A poster: its own, else its parent's (an episode's season), else its grandparent's (the show).
    public static func posters(_ item: ItemSummary) -> [String] {
        var out: [String] = []
        if let own = item.posterUrl, !own.isEmpty { out.append(own) }
        if let parent = item.parentId { out.append("/api/items/\(parent)/images/poster") }
        if let grandparent = item.grandparentId { out.append("/api/items/\(grandparent)/images/poster") }
        return out
    }

    /// A 16:9 still: a movie's backdrop first (its thumbnail is a frame
    /// grab), else its own thumbnail, then an episode's show backdrop, or
    /// another kind's own backdrop.
    public static func stills(_ item: ItemSummary) -> [String] {
        let backdrop = item.backdropUrl.flatMap { $0.isEmpty ? nil : $0 }
        var out: [String] = []
        if item.kind == .movie, let backdrop { out.append(backdrop) }
        if let thumb = item.thumbUrl, !thumb.isEmpty { out.append(thumb) }
        if item.kind == .episode, let show = item.grandparentId { out.append("/api/items/\(show)/images/backdrop") }
        if item.kind != .episode, let backdrop, !out.contains(backdrop) { out.append(backdrop) }
        return out
    }
}
