import PleyModel
import PleyScreens
import SwiftUI

/// A collection's tile: its poster as a card that lifts when focused, with
/// how many titles it holds and its TMDB or Smart badge, and its title and
/// count below. One element for VoiceOver.
struct CollectionTile: View {
    let app: AppModel
    let card: CollectionCard
    let width: CGFloat
    let open: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button(action: open) {
                ServerImage(app: app, paths: card.pictures, width: width, symbol: "square.stack")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .topTrailing) {
                        Text(card.count)
                            .font(.caption.bold().monospacedDigit())
                            .foregroundStyle(.white)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 4)
                            .background(.black.opacity(0.75), in: Capsule())
                            .padding(10)
                    }
                    .overlay(alignment: .topLeading) {
                        if card.smart || card.automatic {
                            ListBadge(text: card.smart ? "Smart" : "TMDB").padding(10)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            }
            .buttonStyle(.card)
            .accessibilityLabel(card.spokenLabel)
            .accessibilityIdentifier("tile.collection.\(card.id)")
            VStack(alignment: .leading, spacing: 2) {
                Text(card.title).font(.callout).lineLimit(1)
                Text(card.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }
}

/// A small word over a picture: "Smart", "TMDB". White on a dark fill.
struct ListBadge: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.caption2.bold())
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .background(.black.opacity(0.75), in: RoundedRectangle(cornerRadius: 6))
            .accessibilityHidden(true)
    }
}

/// A playlist's picture: its first four entries' pictures two by two, or
/// the first alone when it has fewer, square.
struct PlaylistMosaic: View {
    let app: AppModel
    let artwork: [String]
    let video: Bool
    let size: CGFloat

    var body: some View {
        Group {
            if artwork.count >= 4 {
                VStack(spacing: 2) {
                    HStack(spacing: 2) { cell(0); cell(1) }
                    HStack(spacing: 2) { cell(2); cell(3) }
                }
            } else {
                ServerImage(app: app, paths: Array(artwork.prefix(1)), width: size, symbol: symbol)
                    .frame(width: size, height: size)
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
        .accessibilityHidden(true)
    }

    private var symbol: String { video ? "list.and.film" : "music.note.list" }

    private func cell(_ index: Int) -> some View {
        ServerImage(app: app, path: artwork[index], width: (size - 2) / 2, symbol: symbol)
            .frame(width: (size - 2) / 2, height: (size - 2) / 2)
            .clipped()
    }
}

/// A playlist's tile: its mosaic as a card, then its title and "12 items · 3h 12m".
struct PlaylistTile: View {
    let app: AppModel
    let card: PlaylistCard
    let width: CGFloat
    let open: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button(action: open) {
                PlaylistMosaic(app: app, artwork: card.artwork, video: card.video, size: width)
                    .overlay(alignment: .topLeading) {
                        if card.smart { ListBadge(text: "Smart").padding(10) }
                    }
            }
            .buttonStyle(.card)
            .accessibilityLabel(card.spokenLabel)
            .accessibilityIdentifier("tile.playlist.\(card.id)")
            VStack(alignment: .leading, spacing: 2) {
                Text(card.title).font(.callout).lineLimit(1)
                Text(card.meta).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }
}

/// Play and Shuffle, for collections and playlists.
struct PlayShuffleButtons: View {
    let prefix: String
    let canPlay: Bool
    let canShuffle: Bool
    let play: () -> Void
    let shuffle: () -> Void

    var body: some View {
        HStack(spacing: 24) {
            Button(action: play) {
                Label("Play", systemImage: "play.fill")
            }
            .disabled(!canPlay)
            .accessibilityIdentifier("\(prefix).play")
            Button(action: shuffle) {
                Label("Shuffle", systemImage: "shuffle")
            }
            .disabled(!canShuffle)
            .accessibilityIdentifier("\(prefix).shuffle")
        }
    }
}
