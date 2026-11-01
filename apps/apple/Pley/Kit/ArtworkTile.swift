import PleyModel
import PleyScreens
import SwiftUI

/// One tile: its picture in its shape, a caption of one or two lines, and
/// over the picture the progress while in progress, a check once watched,
/// or a show's unwatched episodes. One element for VoiceOver ("Arrival,
/// 2016", "watched", "Opens its page"). The pointer lifts it on iPad; on
/// the Mac, hovering grows it a little, or outlines it under Reduce Motion.
struct ArtworkTile: View {
    let app: AppModel
    let tile: Tile
    let width: CGFloat
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false

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
        .scaleEffect(hovering && !reduceMotion ? 1.03 : 1)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hovering)
        .onHover { hovering = $0 }
        #endif
        .accessibilityLabel(tile.spokenLabel)
        .accessibilityValue(tile.spokenValue)
        .accessibilityHint("Opens its page")
        .accessibilityIdentifier("tile.item.\(tile.id)")
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
