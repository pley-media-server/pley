import PleyCommon
import PleyDownloads
import PleyPlayer
#if os(iOS)
import PleyPlayerViews
#endif
import PleyScreens
import SwiftUI

/// Pley for iPhone, iPad and Mac.
@main
struct PleyApp: App {
    /// The app's model, made once: the system may wake the app in the
    /// background for the downloads before any window shows.
    @MainActor static let model = AppModel.live { settings, gate in Downloads.live(settings: settings, gate: gate) }

    @StateObject private var app = PleyApp.model
    #if os(macOS)
    @NSApplicationDelegateAdaptor(MacAppDelegate.self) private var delegate
    #else
    @UIApplicationDelegateAdaptor(PhoneAppDelegate.self) private var phoneDelegate
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
            InLanguage(app: app) { MacSettingsView(app: app) }
        }
        // About Pley, in place of the standard panel.
        AboutWindow(app: app)
        // Music's Now Playing, a window of its own (⌥⌘P).
        NowPlayingWindow(app: app)
        #endif

        #if os(macOS)
        // The player, in a window of its own per playback.
        WindowGroup("Player", id: PlayerWindow.id, for: PlayRequest.self) { $request in
            PlayerWindow(app: app, request: request)
                .environment(\.locale, app.locale)
        }
        .defaultSize(width: 960, height: 540)
        .restorationBehavior(.disabled)
        // Live TV's player, a window of its own too.
        WindowGroup("Live TV", id: LiveWindow.id, for: LiveRequest.self) { $request in
            LiveWindow(app: app, request: request)
                .environment(\.locale, app.locale)
        }
        .defaultSize(width: 960, height: 540)
        .restorationBehavior(.disabled)
        #endif
    }
}

#if os(iOS)
/// iPhone and iPad: the system wakes the app when the downloads' background
/// session has finished pieces while it was away. The session, made again
/// with the model, hands them over; the system is told once they are handled.
/// A closed app relaunched this way shows no window, so the sign-in the
/// next piece needs is read here, without the server, before the handler
/// is handed over.
final class PhoneAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, handleEventsForBackgroundURLSession identifier: String,
                     completionHandler: @escaping () -> Void) {
        let app = PleyApp.model
        guard let downloads = app.downloads as? Downloads,
              identifier == PieceTransfer.backgroundIdentifier(bundle: Bundle.main.bundleIdentifier ?? "")
        else { return completionHandler() }
        // The sign-in first: the system may suspend the app once the handler is called.
        Task {
            await app.wakeForDownloads()
            downloads.handleBackgroundEvents(completionHandler)
        }
    }
}
#endif

/// What `app.root` names: the servers with a server's sign-in on top,
/// "Who's watching?", or the signed-in shell. On iPhone and iPad the
/// player covers it all; on the Mac it opens in a window of its own, also
/// when a cast from another device starts it.
struct RootView: View {
    @ObservedObject var app: AppModel
    @Environment(\.scenePhase) private var scenePhase
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    var body: some View {
        root
            // Another root starts again.
            .id(app.root)
            // Another language too, so the words made in code change with
            // it (docs/APPLE.md, "Languages"); the player and its covers stay.
            .id(app.language)
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
        // SwiftUI looks the words up in the language shown.
        .environment(\.locale, app.locale)
        #if os(macOS)
        // What a cast from another device plays opens as a page's Play opens it.
        .onValueChange(of: app.castOpening) { opening in
            guard let opening else { return }
            app.castOpened()
            switch opening {
            case .video(let request): openWindow(id: PlayerWindow.id, value: request)
            case .live(let request): openWindow(id: LiveWindow.id, value: request)
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
            NavigationStack(path: $app.signingIn) {
                ServersView(app: app)
                    .languageGlobe(app)
                    .navigationDestination(for: ServerEntry.self) { entry in
                        SignInView(app: app, entry: entry)
                            .languageGlobe(app)
                            .toolbar {
                                // Signing in again (for requests): Cancel goes back to the app as it was.
                                if app.user != nil && app.server?.origin == entry.origin {
                                    ToolbarItem(placement: .cancellationAction) {
                                        Button("Cancel") { app.cancelSignInAgain() }
                                            .accessibilityIdentifier("signIn.keep")
                                    }
                                }
                            }
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
