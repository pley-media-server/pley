import PleyScreens
import SwiftUI

/// The signed-in app on Apple TV: the tabs across the top, Search, Home,
/// Discover (or Your requests, Requests for managers) where requests are
/// open to the account, Live TV when it shows, a tab per library while
/// they fit (the first that fit, and a Libraries tab for the rest,
/// otherwise), Playlists, Now Playing while music has a queue, and
/// Settings (which holds the profile, the server, Switch profile, History,
/// Servers and Sign out, and the device's settings). Each tab keeps its
/// own navigation; Menu goes back through it, then to the tabs, then out
/// of the app. The remote's Play/Pause plays and pauses music from any
/// tab. Add to playlist and New playlist are a panel down the right. News
/// of requests ready to watch pops up at the bottom right once the remote
/// rests. While the player is open VoiceOver reads only the player.
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
                tab(section)
                    .accessibilityIdentifier("shell.tab.\(section.key)")
            }
        }
        // Only while music has a queue: a handler here, even one that does
        // nothing, keeps Play/Pause from a video's tile.
        .onPlayPauseCommand(perform: shell.nowPlaying ? { app.music.toggle() } : nil)
        .overlay(alignment: .topTrailing) { ToastView(shell: shell) }
        .overlay(alignment: .bottomTrailing) { RequestNoticeView(app: app) }
        // While the server can't be reached, the screens show what was saved, and say so.
        .overlay(alignment: .bottom) { OfflineBanner(app: app) }
        .fullScreenCover(item: Binding(get: { shell.sheet }, set: { shell.sheet = $0 })) { sheet in
            ShellSheet(app: app, sheet: sheet)
        }
        .accessibilityHidden(app.player != nil || app.livePlayer != nil)
    }

    /// `section`'s tab. Search, Now Playing and Settings show their symbols
    /// alone (Search as the system's search tab), so the tabs fit across the
    /// screen with Live TV, Libraries and Now Playing among them
    /// (`ShellModel.tabRoom`); VoiceOver still reads their names.
    @TabContentBuilder<ShellSection> private func tab(_ section: ShellSection) -> some TabContent<ShellSection> {
        switch section {
        case .search:
            Tab(value: section, role: .search) {
                stack(section)
            } label: {
                Label(title(section), systemImage: symbol(section)).labelStyle(.iconOnly)
            }
        case .nowPlaying, .settings:
            Tab(value: section) {
                stack(section)
            } label: {
                Label(title(section), systemImage: symbol(section)).labelStyle(.iconOnly)
            }
        default:
            Tab(title(section), systemImage: symbol(section), value: section) { stack(section) }
        }
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
        case .search: SearchView(app: app)
        case .playlists: PlaylistsView(app: app)
        case .settings, .me: SettingsView(app: app)
        case .liveTv: LiveTvView(app: app)
        case .nowPlaying: NowPlayingScreen(app: app)
        case .requests:
            if shell.requestsEntry.discover {
                DiscoverView(app: app)
            } else {
                RequestsView(app: app)
            }
        }
    }

    /// A library's tab is its name.
    private func title(_ section: ShellSection) -> String {
        if case .library(let id) = section { return shell.libraryName(id) }
        if section == .requests { return ShellModel.requestsTitle(shell.requestsEntry) }
        return Self.title(section)
    }

    /// A library's tab shows its kind.
    private func symbol(_ section: ShellSection) -> String {
        if case .library(let id) = section, let library = shell.libraries.first(where: { $0.id == id }) {
            return LibraryEntry.symbol(library.kind)
        }
        if section == .requests { return shell.requestsEntry.discover ? "safari" : "list.bullet.rectangle" }
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
        case .liveTv: "Live TV"
        case .nowPlaying: "Now Playing"
        case .requests: "Requests"
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
        case .liveTv: "tv"
        case .nowPlaying: "music.note"
        case .requests: "list.bullet.rectangle"
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
        case .history:
            HistoryView(app: app)
        case .diagnostics:
            DiagnosticsView(app: app)
        case .about:
            AboutView()
        // Apple TV has no downloads: their screens are never opened here.
        case .settings, .trackDefaults, .downloads, .downloadSettings:
            SettingsView(app: app)
        case .discover:
            DiscoverView(app: app)
        case .discoverList(let list):
            DiscoverGridView(app: app, source: .list(list))
        case .discoverResults(let query):
            DiscoverGridView(app: app, source: .words(query))
        case .discoverTitle(let kind, let tmdbId, let pick):
            DiscoverTitleView(app: app, kind: kind, tmdbId: tmdbId, pick: pick)
        case .requests:
            RequestsView(app: app)
        case .liveTv:
            LiveTvView(app: app)
        }
    }
}

/// "Showing what Pley had saved. Reconnecting…" (after "Another device
/// answers there." when that is why), at the foot of the screen
/// while the server can't be reached. It takes no focus: the app tries the
/// server again by itself.
private struct OfflineBanner: View {
    @ObservedObject var app: AppModel

    var body: some View {
        if app.offline {
            Label(app.offlineMessage, systemImage: "wifi.slash")
                .font(.callout)
                .padding(.horizontal, 32)
                .padding(.vertical, 16)
                .background(.regularMaterial, in: Capsule())
                .padding(.bottom, 40)
                .accessibilityIdentifier("offline.banner")
                .task(id: app.offlineMessage) { AccessibilityNotification.Announcement(app.offlineMessage).post() }
        }
    }
}
