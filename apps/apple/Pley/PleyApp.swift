import PleyCommon
import PleyPlayer
#if os(iOS)
import PleyPlayerViews
#endif
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
                .frame(minWidth: 800, minHeight: 560)
                #endif
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 720)
        .commands {
            MacCommands(app: app)
            PlaybackCommands(app: app)
        }
        #endif

        #if os(macOS)
        // Settings (⌘,): Playback, Subtitles and Browsing.
        Settings {
            MacSettingsView(app: app)
        }
        // About Pley, in place of the standard panel.
        AboutWindow()
        // Music's Now Playing, a window of its own (⌥⌘P).
        NowPlayingWindow(app: app)
        #endif

        #if os(macOS)
        // The player, in a window of its own per playback.
        WindowGroup("Player", id: PlayerWindow.id, for: PlayRequest.self) { $request in
            PlayerWindow(app: app, request: request)
        }
        .defaultSize(width: 960, height: 540)
        .restorationBehavior(.disabled)
        // Live TV's player, a window of its own too.
        WindowGroup("Live TV", id: LiveWindow.id, for: LiveRequest.self) { $request in
            LiveWindow(app: app, request: request)
        }
        .defaultSize(width: 960, height: 540)
        .restorationBehavior(.disabled)
        #endif
    }
}

/// What `app.root` names: the servers with a server's sign-in on top,
/// "Who's watching?", or the signed-in shell. On iPhone and iPad the
/// player covers it all.
struct RootView: View {
    @ObservedObject var app: AppModel
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        root
            // Another root starts again.
            .id(app.root)
        #if os(iOS)
        // Picture in Picture puts the player away and keeps the playback.
        .fullScreenCover(item: Binding(get: { app.playerShown },
                                       set: { if $0 == nil, !app.pictureInPicture { Task { await app.closePlayer() } } })) { controller in
            PlayerScreen(app: app, controller: controller)
        }
        .fullScreenCover(item: Binding(get: { app.livePlayer },
                                       set: { if $0 == nil { app.closeLivePlayer() } })) { model in
            LivePlayerScreen(app: app, model: model) { app.closeLivePlayer() }
        }
        .onValueChange(of: app.player?.id) { _ in
            PictureInPictureKeeper.shared.playerChanged(app.player?.player)
        }
        .task { keepPictureInPicture() }
        .onChange(of: scenePhase) { phase in
            switch phase {
            case .background: background()
            case .active: Task { await app.player?.appDidBecomeActive() }
            default: break
            }
        }
        #endif
        // The server's events are read while Pley is in front; back in
        // front, what shows is read again.
        .onValueChange(of: scenePhase) { phase in
            app.sceneActive(phase != .background)
        }
    }

    @ViewBuilder private var root: some View {
        switch app.root {
        case .starting:
            ProgressView()
                .controlSize(.large)
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
                    .toolbar {
                        if app.profilesFromShell {
                            ToolbarItem(placement: .cancellationAction) {
                                Button("Cancel") { app.keepProfile() }
                                    .accessibilityIdentifier("profiles.cancel")
                            }
                        }
                    }
            }
        case .signedIn:
            ShellView(app: app)
        }
    }

    #if os(iOS)
    /// Going to the background pauses, and the system gives the pause's
    /// report time to go out. A video going on in Picture in Picture
    /// (started as the app left, a moment later) plays on.
    private func background() {
        guard let player = app.player else { return }
        let task = UIApplication.shared.beginBackgroundTask(withName: "Pley playback report")
        Task {
            try? await Task.sleep(for: .milliseconds(300))
            if !app.pictureInPicture { await player.appWillBackground() }
            UIApplication.shared.endBackgroundTask(task)
        }
    }

    /// Picture in Picture's start, restore and ✕ go to the app.
    private func keepPictureInPicture() {
        let keeper = PictureInPictureKeeper.shared
        keeper.onStart = { [app] in app.pictureInPictureStarted() }
        keeper.onRestore = { [app] in app.pictureInPictureRestored() }
        keeper.onClosed = { [app] in app.pictureInPictureClosed() }
    }
    #endif
}
