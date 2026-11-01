import PleyPlayer
import PleyScreens
import SwiftUI

/// Pley for iPhone, iPad and Mac.
@main
struct PleyApp: App {
    @StateObject private var app = AppModel.live()
    #if os(macOS)
    @NSApplicationDelegateAdaptor(MacAppDelegate.self) private var delegate
    #endif

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .task {
                    #if os(macOS)
                    delegate.app = app
                    #endif
                    await app.start()
                }
                #if os(macOS)
                .frame(minWidth: 640, minHeight: 480)
                #endif
        }
        #if os(macOS)
        .defaultSize(width: 1000, height: 700)
        .commands { PlaybackCommands() }
        #endif

        #if os(macOS)
        // The player, in a window of its own per playback.
        WindowGroup("Player", id: PlayerWindow.id, for: PlayRequest.self) { $request in
            PlayerWindow(app: app, request: request)
        }
        .defaultSize(width: 960, height: 540)
        .restorationBehavior(.disabled)
        #endif
    }
}

/// The screen `app.route` names: the root at the bottom of a navigation
/// stack, sign-in, the profiles or an item on top, so Back goes where
/// people expect. On iPhone and iPad the player covers it all.
struct RootView: View {
    @ObservedObject var app: AppModel
    #if os(iOS)
    @Environment(\.scenePhase) private var scenePhase
    #endif

    var body: some View {
        NavigationStack(path: $app.path) {
            root
                .navigationDestination(for: AppModel.Screen.self) { screen in
                    switch screen {
                    case .signIn(let entry): SignInView(app: app, entry: entry)
                    case .profiles: ProfilesView(app: app)
                    case .item(let id): ItemView(app: app, itemId: id)
                    }
                }
        }
        // Another root starts a new stack.
        .id(app.root)
        #if os(iOS)
        .fullScreenCover(item: Binding(get: { app.player },
                                       set: { if $0 == nil { Task { await app.closePlayer() } } })) { controller in
            PlayerScreen(app: app, controller: controller)
        }
        .onChange(of: scenePhase) { phase in
            switch phase {
            case .background: background()
            case .active: Task { await app.player?.appDidBecomeActive() }
            default: break
            }
        }
        #endif
    }

    @ViewBuilder private var root: some View {
        switch app.root {
        case .starting:
            ProgressView()
                .controlSize(.large)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .servers:
            ServersView(app: app)
        case .home:
            HomeView(app: app)
        }
    }

    #if os(iOS)
    /// Going to the background pauses, and the system gives the pause's
    /// report time to go out.
    private func background() {
        guard let player = app.player else { return }
        let task = UIApplication.shared.beginBackgroundTask(withName: "Pley playback report")
        Task {
            await player.appWillBackground()
            UIApplication.shared.endBackgroundTask(task)
        }
    }
    #endif
}
