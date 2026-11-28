import PleyPlayer
import PleyScreens
import SwiftUI

/// What the iPhone, iPad and Mac players lay over the video, bottom
/// trailing: Skip Intro or Skip Credits while the playhead is in one, and
/// the Up Next card (the next episode's title, its countdown, Play Next
/// and Cancel) while it shows. Skip shows by itself for its first 8
/// seconds in a segment, then only while `controlsShown`. Apple TV has
/// the same in the system player's contextual actions.
struct PlayerOverlay: View {
    @ObservedObject var controller: PlaybackController
    /// The player's controls show (the screen's best knowledge of the system's).
    var controlsShown: Bool

    private var skipShown: Bool { controller.state.skipShown(withControls: controlsShown) }

    var body: some View {
        VStack {
            Spacer()
            HStack {
                Spacer()
                if let card = controller.state.upNext {
                    UpNextCardView(card: card, controller: controller)
                        .transition(.opacity)
                } else if skipShown, let label = controller.state.skipLabel {
                    Button(label) { controller.skip() }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .tint(.white.opacity(0.9))
                        .foregroundStyle(.black)
                        .transition(.opacity)
                }
            }
        }
        // Above the system's transport controls.
        .padding(.horizontal, 24)
        .padding(.bottom, 96)
        .animation(.easeInOut(duration: 0.2), value: controller.state.upNext?.itemId)
        .animation(.easeInOut(duration: 0.2), value: controller.state.skipLabel)
        .animation(.easeInOut(duration: 0.2), value: skipShown)
    }
}

/// The next episode: "Up next · S1 E2", its title, and the buttons; no
/// still, so it covers little of the credits. As tall as its content, never
/// stretched over the video. At compact width (an iPhone in portrait) the
/// buttons share the card's full width, so neither wraps.
private struct UpNextCardView: View {
    let card: UpNextCard
    let controller: PlaybackController
#if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    private var compact: Bool { sizeClass == .compact }
#else
    private let compact = false
#endif

    var body: some View {
        Group {
            if compact {
                VStack(alignment: .leading, spacing: 10) {
                    words
                    HStack(spacing: 8) {
                        playNext.frame(maxWidth: .infinity)
                        cancel.frame(maxWidth: .infinity)
                    }
                }
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    words
                    HStack(spacing: 8) {
                        playNext
                        cancel
                    }
                }
            }
        }
        .padding(12)
        .frame(maxWidth: 420, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private var words: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(card.heading)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(card.title)
                .font(.headline)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var playNext: some View {
        Button { controller.playNext() } label: {
            Text(card.playLabel).lineLimit(1).frame(maxWidth: compact ? .infinity : nil)
        }
        .buttonStyle(.borderedProminent)
    }

    private var cancel: some View {
        Button { controller.cancelUpNext() } label: {
            Text("Cancel").lineLimit(1).frame(maxWidth: compact ? .infinity : nil)
        }
        .buttonStyle(.bordered)
    }
}
