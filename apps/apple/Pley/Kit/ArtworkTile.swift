import PleyModel
import PleyScreens
import SwiftUI

/// One tile: its picture in its shape, a caption of one or two lines, and
/// over the picture the progress while in progress, a check once watched,
/// or a show's unwatched episodes. One element for VoiceOver ("Arrival,
/// 2016", "watched", "Opens its page"). The pointer lifts it on iPad; on
/// the Mac, hovering grows it a little, or outlines it under Reduce Motion,
/// and shows a play button over a video's picture. On the Mac, Return opens
/// the tile with the focus and ⌘↩ plays it. Its title's actions are its
/// context menu and VoiceOver's actions; a mark made there shows at once.
struct ArtworkTile: View {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    private let given: Tile
    let width: CGFloat
    /// From a Continue Watching row: its actions offer to leave it.
    let continueWatching: Bool
    let remove: (() async -> Void)?
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    init(app: AppModel, tile: Tile, width: CGFloat, continueWatching: Bool = false,
         remove: (() async -> Void)? = nil, open: @escaping () -> Void) {
        self.app = app
        _actions = ObservedObject(wrappedValue: app.actions)
        given = tile
        self.width = width
        self.continueWatching = continueWatching
        self.remove = remove
        self.open = open
    }

    /// The tile with the marks made here.
    private var tile: Tile { actions.shown(given) }

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 6) {
                picture
                caption
            }
            .frame(width: width, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        #if os(iOS)
        .hoverEffect(.lift)
        #else
        .overlay(alignment: .top) {
            if hovering, let target = playTarget {
                playBadge(target)
            }
        }
        .scaleEffect(hovering && !reduceMotion ? 1.03 : 1)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hovering)
        .onHover { hovering = $0 }
        .onKeyPress(.return) {
            open()
            return .handled
        }
        .focusedValue(\.playTarget, playTarget)
        #endif
        .accessibilityLabel(tile.spokenLabel)
        .accessibilityValue(tile.spokenValue)
        .accessibilityHint("Opens its page")
        .itemActions(app, tile.item, continueWatching: continueWatching, remove: remove)
        .accessibilityIdentifier("tile.item.\(tile.id)")
    }

    #if os(macOS)
    /// What ⌘↩ and the play button play: videos only (music plays in a later version).
    private var playTarget: PlayTarget? { PlayTarget.item(tile.item, actions: actions) }

    /// The play button over a video's picture while the pointer is on the tile.
    private func playBadge(_ target: PlayTarget) -> some View {
        Button {
            Task {
                if let request = await target.request() { openWindow(id: PlayerWindow.id, value: request) }
            }
        } label: {
            Image(systemName: "play.circle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, .black.opacity(0.55))
                .font(.system(size: min(44, width / 3)))
        }
        .buttonStyle(.plain)
        .frame(width: width, height: Metrics.pictureHeight(tile.shape, width: width))
        .help(target.menuTitle)
        // VoiceOver plays it from the tile's actions.
        .accessibilityHidden(true)
    }
    #endif

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
            .overlay {
                if hovering && reduceMotion {
                    RoundedRectangle(cornerRadius: Metrics.corner).strokeBorder(Color.accentColor, lineWidth: 2)
                }
            }
    }

    @ViewBuilder private var badge: some View {
        if tile.unwatched > 0 {
            Text("\(tile.unwatched)")
                .font(.caption2.bold().monospacedDigit())
                .foregroundStyle(.white)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(Color.accentColor, in: Capsule())
                .padding(6)
        } else if tile.watched {
            Image(systemName: "checkmark.circle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, Color.accentColor)
                .font(.body)
                .padding(6)
        }
    }

    private var caption: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(tile.title)
                .font(.footnote.weight(.medium))
                .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
            if !tile.subtitle.isEmpty {
                Text(tile.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(typeSize.isAccessibilitySize ? 2 : 1)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// A tile not loaded yet: its shape, empty.
struct PlaceholderTile: View {
    let shape: RowShape
    let width: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            RoundedRectangle(cornerRadius: Metrics.corner)
                .fill(.quaternary)
                .frame(width: width, height: Metrics.pictureHeight(shape, width: width))
            RoundedRectangle(cornerRadius: 3)
                .fill(.quaternary)
                .frame(width: width * 0.7, height: 10)
        }
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
        .frame(height: 4)
        .accessibilityHidden(true)
    }
}
