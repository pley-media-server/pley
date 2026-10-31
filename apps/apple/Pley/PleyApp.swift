import PleyScreens
import SwiftUI

/// Pley for iPhone, iPad and Mac.
@main
struct PleyApp: App {
    @StateObject private var app = AppModel.live()

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .task { await app.start() }
                #if os(macOS)
                .frame(minWidth: 640, minHeight: 480)
                #endif
        }
        #if os(macOS)
        .defaultSize(width: 1000, height: 700)
        #endif
    }
}

/// The screen `app.route` names: the root at the bottom of a navigation
/// stack, sign-in or the profiles on top, so Back goes where people expect.
struct RootView: View {
    @ObservedObject var app: AppModel

    var body: some View {
        NavigationStack(path: $app.path) {
            root
                .navigationDestination(for: AppModel.Screen.self) { screen in
                    switch screen {
                    case .signIn(let entry): SignInView(app: app, entry: entry)
                    case .profiles: ProfilesView(app: app)
                    }
                }
        }
        // Another root starts a new stack.
        .id(app.root)
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
}
