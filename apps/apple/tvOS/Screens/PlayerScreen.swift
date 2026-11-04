import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

/// The player on Apple TV: the system's player, full screen, with its
/// transport bar's Quality menu (each choice with its second line), Audio
/// menu, Subtitles menu (with the sync and Find Subtitles…), Speed and
/// Sleep Timer menus, Picture in Picture, the subtitles drawn over the
/// video, a Details tab in its info panel, Skip Intro, Skip Credits, Up
/// Next and the sleep timer's Keep Watching as its contextual actions,
/// and the chapters as its navigation markers. The sleep timer's last
/// 30 s show at the top, and the player's notices ("Playing from …", as a
/// cast from another device starts) for 4 seconds. Menu closes it (when
/// the transport bar isn't showing), which ends the session.
struct PlayerScreen: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController
    @State private var search: SubtitleSearchModel?
    @State private var notice: String?
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            GeometryReader { geometry in
                PlayerSurface(player: controller.player, state: controller.state,
                              onQuality: { controller.selectQuality($0) }, controller: controller,
                              onFindSubtitles: { search = SubtitleSearchModel(app: app, controller: controller) })
                    .onAppear { sized(geometry.size) }
                    .onChange(of: geometry.size) { _, size in sized(size) }
            }
            .ignoresSafeArea()
            if let notice = controller.state.sleep.notice {
                VStack {
                    Text(notice)
                        .monospacedDigit()
                        .padding(.horizontal, 30)
                        .padding(.vertical, 14)
                        .background(.regularMaterial, in: Capsule())
                        .accessibilityIdentifier("player.sleepNotice")
                    Spacer()
                }
                .allowsHitTesting(false)
            } else if let notice {
                VStack {
                    Text(notice)
                        .padding(.horizontal, 30)
                        .padding(.vertical, 14)
                        .background(.regularMaterial, in: Capsule())
                        .accessibilityIdentifier("player.notice")
                    Spacer()
                }
                .allowsHitTesting(false)
            }
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
        .fullScreenCover(item: $search) { model in
            SubtitleSearchView(model: model)
        }
        // Said as it opened ("Playing from …", a cast): it shows too.
        .onAppear { showNotice(controller.state.notice) }
        .onChange(of: controller.state.notice) { _, text in
            showNotice(text)
        }
    }

    /// The controller's notice, at the top for 4 seconds, without taking the focus.
    private func showNotice(_ text: String?) {
        guard let text else { return }
        notice = text
        controller.dismissNotice()
        Task {
            try? await Task.sleep(for: .seconds(4))
            if notice == text { notice = nil }
        }
    }

    /// The player's size in pixels: Automatic plays no stream size bigger than the screen.
    private func sized(_ size: CGSize) {
        controller.setViewPixelSize(width: Int(size.width * displayScale), height: Int(size.height * displayScale))
    }
}
