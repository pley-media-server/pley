import PleyModel
import PleyScreens
import SwiftUI

/// One tile: its picture in its shape as a card that lifts when focused,
/// and below it a caption of one or two lines. Over the picture, the
/// progress while in progress, a check once watched, or a show's unwatched
/// episodes. One element for VoiceOver ("Arrival, 2016", "watched").
/// A long press on Select opens its title's actions, and Play/Pause plays
/// it; a mark made there shows at once.
struct ArtworkTile: View {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    private let given: Tile
    let width: CGFloat
    /// From a Continue Watching row: its actions offer to leave it.
    let continueWatching: Bool
    let remove: (() async -> Void)?
    /// The focus came to it (Home's info block).
    let focused: (() -> Void)?
    let open: () -> Void
    @FocusState private var hasFocus: Bool

    init(app: AppModel, tile: Tile, width: CGFloat, continueWatching: Bool = false,
         remove: (() async -> Void)? = nil, focused: (() -> Void)? = nil, open: @escaping () -> Void) {
        self.app = app
        _actions = ObservedObject(wrappedValue: app.actions)
        given = tile
        self.width = width
        self.continueWatching = continueWatching
        self.remove = remove
        self.focused = focused
        self.open = open
    }

    /// The tile with the marks made here.
    private var tile: Tile { actions.shown(given) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button(action: open) {
                picture
            }
            .buttonStyle(.card)
            .focused($hasFocus)
            .onChange(of: hasFocus) { _, now in if now { focused?() } }
            .accessibilityLabel(tile.spokenLabel)
            .accessibilityValue(tile.spokenValue)
            .itemActions(app, tile.item, continueWatching: continueWatching, remove: remove)
            .accessibilityIdentifier("tile.item.\(tile.id)")
            caption
                .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }

    private var picture: some View {
        ServerImage(app: app, paths: tile.pictures, width: width, symbol: Metrics.symbol(tile.item.kind))
            .frame(width: width, height: Metrics.pictureHeight(tile.shape, width: width))
            .overlay(alignment: .bottom) {
                if tile.progress > 0 {
                    ProgressBar(fraction: tile.progress)
                }
            }
            .overlay(alignment: .topTrailing) { badge }
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
    }

    /// White on a dark fill: the accent colour is white on Apple TV.
    @ViewBuilder private var badge: some View {
        if tile.unwatched > 0 {
            Text("\(tile.unwatched)")
                .font(.caption.bold().monospacedDigit())
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(.black.opacity(0.75), in: Capsule())
                .padding(10)
        } else if tile.watched {
            Image(systemName: "checkmark.circle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, .black.opacity(0.75))
                .font(.title3)
                .padding(10)
        }
    }

    private var caption: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(tile.title)
                .font(.callout)
                .lineLimit(1)
            if !tile.subtitle.isEmpty {
                Text(tile.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

/// A tile not loaded yet: its shape, empty.
struct PlaceholderTile: View {
    let shape: RowShape
    let width: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: Metrics.corner)
            .fill(.quaternary)
            .frame(width: width, height: Metrics.pictureHeight(shape, width: width))
            .accessibilityHidden(true)
    }
}

/// How much has been played, along a picture's bottom edge.
struct ProgressBar: View {
    let fraction: Double

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Rectangle().fill(.black.opacity(0.45))
                Rectangle().fill(Color.accentColor)
                    .frame(width: geometry.size.width * min(1, max(0, fraction)))
            }
        }
        .frame(height: 8)
        .accessibilityHidden(true)
    }
}
