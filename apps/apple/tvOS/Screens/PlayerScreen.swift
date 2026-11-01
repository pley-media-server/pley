import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

/// The player on Apple TV: the system's player, full screen, with its
/// transport bar's Quality menu (each choice with its second line) and
/// Audio menu, a Details tab in its info panel, Skip Intro, Skip Credits
/// and Up Next as its contextual actions, and the chapters as its
/// navigation markers. Menu
/// closes it (when the transport bar isn't showing), which ends the session.
struct PlayerScreen: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            GeometryReader { geometry in
                PlayerSurface(player: controller.player, state: controller.state,
                              onQuality: { controller.selectQuality($0) }, controller: controller)
                    .onAppear { sized(geometry.size) }
                    .onChange(of: geometry.size) { _, size in sized(size) }
            }
            .ignoresSafeArea()
            switch controller.state.phase {
            case .preparing where controller.player?.currentItem == nil:
                ProgressView()
            case .failed(let message, let canRetry):
                VStack(spacing: 30) {
                    Text(message).multilineTextAlignment(.center)
                    HStack(spacing: 30) {
                        if canRetry {
                            Button("Try again") { controller.retry() }
                        }
                        Button("Close") { Task { await app.closePlayer(controller) } }
                    }
                }
                .padding(60)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 24))
                .focusSection()
                .onExitCommand { Task { await app.closePlayer(controller) } }
            default:
                EmptyView()
            }
        }
        .onAppear { PlaybackAudioSession.activate() }
        .onDisappear { PlaybackAudioSession.deactivate() }
    }

    /// The player's size in pixels: Automatic plays no stream size bigger than the screen.
    private func sized(_ size: CGSize) {
        controller.setViewPixelSize(width: Int(size.width * displayScale), height: Int(size.height * displayScale))
    }
}
