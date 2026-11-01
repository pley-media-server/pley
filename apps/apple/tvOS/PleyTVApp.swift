import PleyScreens
import SwiftUI
import UIKit

/// Pley for Apple TV.
@main
struct PleyTVApp: App {
    @StateObject private var app = AppModel.live()

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .task { await app.start() }
        }
    }
}

/// The screen `app.route` names: the root at the bottom of a navigation
/// stack, sign-in, the profiles or an item on top, so Menu goes back, and
/// leaves the app from the root as the system does. The player covers it
/// all; Menu closes it.
struct RootView: View {
    @ObservedObject var app: AppModel
    @Environment(\.scenePhase) private var scenePhase

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
        .fullScreenCover(item: Binding(get: { app.player },
                                       set: { if $0 == nil { Task { await app.closePlayer() } } })) { controller in
            PlayerScreen(app: app, controller: controller)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: background()
            case .active: Task { await app.player?.appDidBecomeActive() }
            default: break
            }
        }
    }

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

    @ViewBuilder private var root: some View {
        switch app.root {
        case .starting:
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .servers:
            ServersView(app: app)
        case .home:
            HomeView(app: app)
        }
    }
}
