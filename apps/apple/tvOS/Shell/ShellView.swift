import PleyScreens
import SwiftUI

/// The signed-in app on Apple TV: the tabs across the top, Search, Home,
/// Discover (or Your requests, Requests for managers) where requests are
/// open to the account, Live TV when it shows, Libraries (the movie and
/// show library chosen last, with its title a menu of the others),
/// Listen (playlists and music), Now Playing while music has a queue, and
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
        TabView(selection: Binding(get: { Self.tab(shell.section) }, set: { choose(tab: $0) })) {
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

    /// The tab a section shows under: a library's is the Libraries tab.
    private static func tab(_ section: ShellSection) -> ShellSection {
        if case .library = section { return .libraries }
        return section
    }

    /// A tab chosen: Libraries shows the library chosen last (`ShellModel.openLibraryTab`).
    private func choose(tab: ShellSection) {
        if tab == .libraries { shell.openLibraryTab() } else { shell.select(tab) }
    }

    /// The tabs that show their symbol alone (`ShellModel.symbolOnly`).
    private var symbolOnly: Set<ShellSection> {
        ShellModel.symbolOnly(shell.sections, libraries: shell.libraries, requests: shell.requestsEntry)
    }

    /// `section`'s tab. Search, Now Playing and Settings show their symbols
    /// alone (Search as the system's search tab), and, while the words
    /// wouldn't fit across the screen, Listen, Libraries, Live TV and
    /// Discover too (`ShellModel.tabRoom`); VoiceOver still reads their names.
    /// Libraries' navigation is the library it shows.
    @TabContentBuilder<ShellSection> private func tab(_ section: ShellSection) -> some TabContent<ShellSection> {
        switch section {
        case .search:
            Tab(value: section, role: .search) {
                stack(content(section))
            } label: {
                Label(title(section), systemImage: symbol(section)).labelStyle(.iconOnly)
            }
        case _ where symbolOnly.contains(section):
            Tab(value: section) {
                stack(content(section))
            } label: {
                Label(title(section), systemImage: symbol(section)).labelStyle(.iconOnly)
            }
        default:
            Tab(title(section), systemImage: symbol(section), value: section) { stack(content(section)) }
        }
    }

    /// What `tab` shows: the Libraries tab, its library.
    private func content(_ tab: ShellSection) -> ShellSection {
        tab == .libraries ? shell.libraryTabSection : tab
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
        // Another library chosen under the tab is a page of its own, never the last one's.
        case .library(let id): LibraryView(app: app, libraryId: id).id(id)
        case .search: SearchView(app: app)
        // Listen holds the playlists, beside the music.
        case .playlists: ListenView(app: app)
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

    private func title(_ section: ShellSection) -> String {
        if section == .requests { return ShellModel.requestsTitle(shell.requestsEntry) }
        return Self.title(section)
    }

    private func symbol(_ section: ShellSection) -> String {
        if section == .requests { return shell.requestsEntry.discover ? "safari" : "list.bullet.rectangle" }
        return Self.symbol(section)
    }

    /// In the language shown.
    static func title(_ section: ShellSection) -> String {
        switch section {
        case .home: localized("Home")
        case .libraries: localized("Libraries")
        case .search: localized("Search")
        case .playlists: localized("Listen")
        case .me: localized("Me")
        case .settings: localized("Settings")
        case .liveTv: localized("Live TV")
        case .nowPlaying: localized("Now Playing")
        case .requests: localized("Requests")
        case .library: localized("Library")
        }
    }

    static func symbol(_ section: ShellSection) -> String {
        switch section {
        case .home: "house"
        case .libraries: "square.stack"
        case .search: "magnifyingglass"
        case .playlists: "headphones"
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
        case .mergedCollection(let ids):
            MergedCollectionView(app: app, ids: ids)
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

/// "Pley is offline", at the foot of the screen while the server can't be
/// reached. It takes no focus: the app tries the server again by itself.
private struct OfflineBanner: View {
    @ObservedObject var app: AppModel

    var body: some View {
        if app.offline {
            Label(DownloadWords.offline, systemImage: "wifi.slash")
                .font(.callout)
                .padding(.horizontal, 32)
                .padding(.vertical, 16)
                .background(.regularMaterial, in: Capsule())
                .padding(.bottom, 40)
                .accessibilityIdentifier("offline.banner")
                .task { AccessibilityNotification.Announcement(DownloadWords.offline).post() }
        }
    }
}
