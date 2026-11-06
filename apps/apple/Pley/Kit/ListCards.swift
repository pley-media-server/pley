import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// Opens the player: full screen on iPhone and iPad, its own window on the Mac.
struct Launcher: DynamicProperty {
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    @MainActor func play(_ app: AppModel, _ request: PlayRequest?) {
        guard let request else { return }
        #if os(macOS)
        openWindow(id: PlayerWindow.id, value: request)
        #else
        Task { await app.play(request) }
        #endif
    }
}

/// A collection's tile: its poster with how many titles it holds, its
/// TMDB or Smart badge, and its title and count below. One element for
/// VoiceOver ("Dune Collection, collection, 2 movies").
struct CollectionTile: View {
    let app: AppModel
    let card: CollectionCard
    let width: CGFloat
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 6) {
                ServerImage(app: app, paths: card.pictures, width: width, symbol: "square.stack")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .topTrailing) {
                        Text(card.count)
                            .font(.caption2.bold().monospacedDigit())
                            .foregroundStyle(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(.black.opacity(0.7), in: Capsule())
                            .padding(6)
                    }
                    .overlay(alignment: .topLeading) {
                        if card.smart || card.automatic {
                            ListBadge(text: card.smart ? localized("Smart") : "TMDB").padding(6)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                    .overlay {
                        if hovering && reduceMotion {
                            RoundedRectangle(cornerRadius: Metrics.corner).strokeBorder(Color.accentColor, lineWidth: 2)
                        }
                    }
                VStack(alignment: .leading, spacing: 1) {
                    Text(card.title)
                        .font(.footnote.weight(.medium))
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                    Text(card.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .fixedSize(horizontal: false, vertical: true)
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
        .onKeyPress(.return) {
            open()
            return .handled
        }
        #endif
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(card.spokenLabel)
        .accessibilityHint("Opens its page")
        .accessibilityAddTraits(.isButton)
        .contextMenu {
            Button(action: open) { Label("Open", systemImage: "square.stack") }
        }
        .accessibilityIdentifier("tile.collection.\(card.id)")
    }
}

/// A small word over a picture: "Smart", "TMDB".
struct ListBadge: View {
    /// Shown as it is: callers pass words through `localized`.
    let text: String

    var body: some View {
        Text(text)
            .font(.caption2.bold())
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(.black.opacity(0.7), in: RoundedRectangle(cornerRadius: 4))
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
                VStack(spacing: 1) {
                    HStack(spacing: 1) { cell(0); cell(1) }
                    HStack(spacing: 1) { cell(2); cell(3) }
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
        ServerImage(app: app, path: artwork[index], width: (size - 1) / 2, symbol: symbol)
            .frame(width: (size - 1) / 2, height: (size - 1) / 2)
            .clipped()
    }
}

/// A playlist's tile: its mosaic, its title and "12 items · 3h 12m".
struct PlaylistTile: View {
    let app: AppModel
    let card: PlaylistCard
    let width: CGFloat
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 6) {
                PlaylistMosaic(app: app, artwork: card.artwork, video: card.video, size: width)
                    .overlay(alignment: .topLeading) {
                        if card.smart { ListBadge(text: localized("Smart")).padding(6) }
                    }
                    .overlay {
                        if hovering && reduceMotion {
                            RoundedRectangle(cornerRadius: Metrics.corner).strokeBorder(Color.accentColor, lineWidth: 2)
                        }
                    }
                VStack(alignment: .leading, spacing: 1) {
                    Text(card.title)
                        .font(.footnote.weight(.medium))
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 2)
                    Text(card.meta)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .fixedSize(horizontal: false, vertical: true)
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
        .onKeyPress(.return) {
            open()
            return .handled
        }
        #endif
        .contextMenu {
            Button(action: open) { Label("Open", systemImage: "music.note.list") }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(card.spokenLabel)
        .accessibilityHint("Opens it")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("tile.playlist.\(card.id)")
    }
}

/// Play and Shuffle, for collections, playlists and music; with `remote`
/// and `cast`, "Play on…" beside them, with "Shuffle on…" in it.
struct PlayShuffleButtons: View {
    let prefix: String
    let canPlay: Bool
    let canShuffle: Bool
    let play: () -> Void
    let shuffle: () -> Void
    /// The profile's other devices, and what "Play on…" sends them; `nil`: none.
    var remote: RemoteModel?
    var cast: CastTarget?
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        // One under the other at the accessibility text sizes, so neither word breaks;
        // otherwise in a line, "Play on…" going under them where the line is too narrow.
        let layout = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
            : AnyLayout(FlowLayout(spacing: 12, lineSpacing: 12))
        layout {
            Button(action: play) {
                Label("Play", systemImage: "play.fill")
                    // A list's row would make the label icon-only and give it
                    // any height: the words stay, and only the width is fixed.
                    .labelStyle(.titleAndIcon)
                    .fixedSize(horizontal: true, vertical: false)
                    .frame(minWidth: 90)
                    // In a list's row the symbol takes the accent colour, the
                    // button's own, and vanishes; it is drawn as the words are.
                    .foregroundStyle(canPlay ? AnyShapeStyle(.white) : AnyShapeStyle(.secondary))
            }
            .buttonStyle(.borderedProminent)
            .disabled(!canPlay)
            .accessibilityIdentifier("\(prefix).play")
            Button(action: shuffle) {
                Label("Shuffle", systemImage: "shuffle")
                    // A list's row would make the label icon-only and give it
                    // any height: the words stay, and only the width is fixed.
                    .labelStyle(.titleAndIcon)
                    .fixedSize(horizontal: true, vertical: false)
                    .frame(minWidth: 90)
            }
            .buttonStyle(.bordered)
            .disabled(!canShuffle)
            .accessibilityIdentifier("\(prefix).shuffle")
            if let remote, let cast {
                PlayOnMenu(remote: remote, target: cast, shuffle: canShuffle, prefix: prefix)
                    .labelStyle(.titleAndIcon)
                    .fixedSize(horizontal: true, vertical: false)
                    .menuStyle(.button)
                    .buttonStyle(.bordered)
            }
        }
        // A list's row offers any height: the buttons keep their own. Being
        // bordered, each keeps its own hit area there, not the whole row's.
        .fixedSize(horizontal: false, vertical: true)
        #if os(iOS)
        .controlSize(.large)
        #endif
    }
}
