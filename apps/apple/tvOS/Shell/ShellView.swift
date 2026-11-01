import PleyScreens
import SwiftUI

/// The signed-in app on Apple TV: tabs across the top, Home, a tab per
/// library (or one Libraries tab when there are more than four), Playlists
/// and Settings (which holds the profile, the server, Switch profile,
/// Servers and Sign out). Each tab keeps its own navigation; Menu goes back
/// through it, then to the tabs, then out of the app. Add to playlist and
/// New playlist are a panel down the right. While the player is open
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
                Tab(title(section), systemImage: symbol(section), value: section) {
                    stack(section)
                }
                .accessibilityIdentifier("shell.tab.\(section.key)")
            }
        }
        .overlay(alignment: .topTrailing) { ToastView(shell: shell) }
        .fullScreenCover(item: Binding(get: { shell.sheet }, set: { shell.sheet = $0 })) { sheet in
            ShellSheet(app: app, sheet: sheet)
        }
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
        case .libraries: LibrariesView(app: app)
        case .library(let id): LibraryView(app: app, libraryId: id)
        case .playlists: PlaylistsView(app: app)
        case .settings, .me: SettingsView(app: app)
        default: EmptyView()
        }
    }

    /// A library's tab is its name.
    private func title(_ section: ShellSection) -> String {
        if case .library(let id) = section { return shell.libraryName(id) }
        return Self.title(section)
    }

    /// A library's tab shows its kind.
    private func symbol(_ section: ShellSection) -> String {
        if case .library(let id) = section, let library = shell.libraries.first(where: { $0.id == id }) {
            return LibraryEntry.symbol(library.kind)
        }
        return Self.symbol(section)
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

/// The page a `Screen` names.
struct ScreenView: View {
    let app: AppModel
    let screen: Screen

    var body: some View {
        switch screen {
        case .item(let id):
            ItemView(app: app, itemId: id)
        case .artist(let id):
            ArtistView(app: app, itemId: id)
        case .album(let id, let track):
            AlbumView(app: app, itemId: id, track: track)
        case .person(let id):
            PersonView(app: app, personId: id)
        case .library(let id, let view):
            LibraryView(app: app, libraryId: id, view: view)
        case .libraries:
            LibrariesView(app: app)
        case .collection(let id):
            CollectionView(app: app, collectionId: id)
        case .collections(let library):
            CollectionsPage(app: app, libraryId: library)
        case .playlist(let id):
            PlaylistView(app: app, playlistId: id)
        default:
            EmptyView()
        }
    }
}
