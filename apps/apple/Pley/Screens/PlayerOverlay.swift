import PleyPlayer
import PleyScreens
import SwiftUI

/// What the iPhone, iPad and Mac players lay over the video, bottom
/// trailing: Skip Intro or Skip Credits while the playhead is in one, and
/// the Up Next card (the next episode's picture, its countdown, Play Next
/// and Cancel) while it shows; and at the top, in the sleep timer's last
/// 30 s, "Pausing in 30 s" with Keep watching. Apple TV has the same in
/// the system player's contextual actions.
struct PlayerOverlay: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack {
            if let notice = controller.state.sleep.notice {
                sleepNotice(notice)
                    .transition(.opacity)
            }
            Spacer()
            HStack {
                Spacer()
                if let card = controller.state.upNext {
                    UpNextCardView(app: app, card: card, controller: controller)
                        .transition(.opacity)
                } else if let label = controller.state.skipLabel {
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
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: controller.state.sleep.secondsLeft == nil)
    }

    /// The sleep timer's notice: the seconds left and Keep watching.
    private func sleepNotice(_ notice: String) -> some View {
        HStack(spacing: 12) {
            Text(notice)
                .monospacedDigit()
                .accessibilityIdentifier("player.sleepNotice")
            Button("Keep watching") { controller.keepWatching() }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("player.keepWatching")
        }
        .font(.callout)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial, in: Capsule())
        .accessibilityElement(children: .contain)
        .padding(.top, 8)
    }
}

/// The next episode: its still, "Up next · S1 E2", its title, and the
/// buttons. As tall as its content, never stretched over the video. At
/// compact width (an iPhone in portrait) the picture is smaller and the
/// buttons take their own full-width row under it, so neither wraps.
private struct UpNextCardView: View {
    let app: AppModel
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
                    HStack(alignment: .top, spacing: 10) {
                        picture(width: 112, height: 63)
                        words
                    }
                    HStack(spacing: 8) {
                        playNext.frame(maxWidth: .infinity)
                        cancel.frame(maxWidth: .infinity)
                    }
                }
            } else {
                HStack(alignment: .top, spacing: 12) {
                    picture(width: 160, height: 90)
                    VStack(alignment: .leading, spacing: 8) {
                        words
                        HStack(spacing: 8) {
                            playNext
                            cancel
                        }
                    }
                }
            }
        }
        .padding(12)
        .frame(maxWidth: 420, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private func picture(width: CGFloat, height: CGFloat) -> some View {
        ServerImage(app: app, paths: card.imagePath.map { [$0] } ?? [], width: width, limit: NextUp.imageWidth)
            .frame(width: width, height: height)
            .clipShape(RoundedRectangle(cornerRadius: 6))
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
