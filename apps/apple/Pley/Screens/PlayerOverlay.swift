import PleyPlayer
import PleyScreens
import SwiftUI

/// What the iPhone, iPad and Mac players lay over the video, bottom
/// trailing: Skip Intro or Skip Credits while the playhead is in one, and
/// the Up Next card (the next episode's picture, its countdown, Play Next
/// and Cancel) while it shows. Apple TV has the same in the system
/// player's contextual actions.
struct PlayerOverlay: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController

    var body: some View {
        VStack {
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
    }
}

/// The next episode: its still, "Up next · S1 E2", its title, and the buttons.
private struct UpNextCardView: View {
    let app: AppModel
    let card: UpNextCard
    let controller: PlaybackController
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ServerImage(app: app, path: card.imagePath,
                        widthPx: min(NextUp.imageWidth, Int(160 * displayScale)))
                .frame(width: 160, height: 90)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 4) {
                Text(card.heading)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(card.title)
                    .font(.headline)
                    .lineLimit(2)
                Spacer(minLength: 4)
                HStack(spacing: 8) {
                    Button(card.playLabel) { controller.playNext() }
                        .buttonStyle(.borderedProminent)
                    Button("Cancel") { controller.cancelUpNext() }
                        .buttonStyle(.bordered)
                }
            }
        }
        .padding(12)
        .frame(maxWidth: 420, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }
}
