import PleyCommon
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

/// What `app.root` names: the servers with a server's sign-in on top,
/// "Who's watching?", or the signed-in shell. Menu goes back, and leaves
/// the app from the root as the system does. The player covers it all;
/// Menu closes it.
struct RootView: View {
    @ObservedObject var app: AppModel
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        root
            // Another root starts again.
            .id(app.root)
        .fullScreenCover(item: Binding(get: { app.player },
                                       set: { if $0 == nil { Task { await app.closePlayer() } } })) { controller in
            PlayerScreen(app: app, controller: controller)
        }
        .onChange(of: scenePhase) { _, phase in
            // The server's events are read while Pley is in front; back in
            // front, what shows is read again.
            app.sceneActive(phase != .background)
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
            NavigationStack(path: $app.signingIn) {
                ServersView(app: app)
                    .navigationDestination(for: ServerEntry.self) { entry in
                        SignInView(app: app, entry: entry)
                    }
            }
        case .profiles:
            NavigationStack {
                ProfilesView(app: app)
            }
            // Menu, from the shell's Switch profile: the profile in use stays.
            .onExitCommand(perform: app.profilesFromShell ? { app.keepProfile() } : nil)
        case .signedIn:
            ShellView(app: app)
        }
    }
}
