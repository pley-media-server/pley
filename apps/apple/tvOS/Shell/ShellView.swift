import PleyScreens
import SwiftUI

/// The signed-in app on Apple TV: tabs across the top, Home and Settings
/// (which holds the profile, the server, Switch profile, Servers and Sign
/// out). Each tab keeps its own navigation; Menu goes back through it,
/// then to the tabs, then out of the app. While the player is open
/// VoiceOver reads only the player.
struct ShellView: View {
    @ObservedObject var app: AppModel
    @ObservedObject private var shell: ShellModel

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
    }

    var body: some View {
        TabView(selection: Binding(get: { shell.section }, set: { shell.select($0) })) {
            ForEach(shell.sections, id: \.self) { section in
                Tab(Self.title(section), systemImage: Self.symbol(section), value: section) {
                    stack(section)
                }
                .accessibilityIdentifier("shell.tab.\(section.key)")
            }
        }
        .overlay(alignment: .topTrailing) { ToastView(shell: shell) }
        .accessibilityHidden(app.player != nil)
    }

    /// `section`'s navigation: its start, with the pages opened from it on top.
    private func stack(_ section: ShellSection) -> some View {
        NavigationStack(path: Binding(get: { shell.path(for: section) },
                                      set: { shell.setPath($0, for: section) })) {
            start(section)
                .navigationDestination(for: Screen.self) { screen in
                    ScreenView(app: app, screen: screen)
                }
        }
    }

    @ViewBuilder private func start(_ section: ShellSection) -> some View {
        switch section {
        case .home: HomeView(app: app)
        case .settings, .me: SettingsView(app: app)
        default: EmptyView()
        }
    }

    static func title(_ section: ShellSection) -> String {
        switch section {
        case .home: "Home"
        case .libraries: "Libraries"
        case .search: "Search"
        case .playlists: "Playlists"
        case .me: "Me"
        case .settings: "Settings"
        case .library: "Library"
        }
    }

    static func symbol(_ section: ShellSection) -> String {
        switch section {
        case .home: "house"
        case .libraries: "square.stack"
        case .search: "magnifyingglass"
        case .playlists: "music.note.list"
        case .me: "person.crop.circle"
        case .settings: "gearshape"
        case .library: "folder"
        }
    }
}

/// The page a `Screen` names. Artists and albums open the item's page
/// until they have pages of their own.
struct ScreenView: View {
    let app: AppModel
    let screen: Screen

    var body: some View {
        switch screen {
        case .item(let id), .artist(let id), .album(let id, _):
            ItemView(app: app, itemId: id)
        default:
            EmptyView()
        }
    }
}
