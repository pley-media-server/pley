import PleyModel
import PleyScreens
import SwiftUI

/// The signed-in app. On iPhone, and on iPad in a narrow window, a tab bar
/// with Home, Libraries, Playlists and Me; on iPad at regular width and on
/// the Mac, a sidebar with Home and Playlists, the libraries in the
/// profile's order (Edit, or a drag, puts them in another), and at its foot
/// the profile signed in (Me). Each section keeps its own navigation, and
/// choosing the section showing goes back to its start. Add to playlist and
/// New playlist are sheets over it; the toast shows at the bottom; while
/// the player is open VoiceOver reads only the player.
struct ShellView: View {
    @ObservedObject var app: AppModel
    @ObservedObject private var shell: ShellModel
    private let layout = WideLayout()

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
    }

    var body: some View {
        Group {
            if layout.wide {
                split
            } else {
                tabs
            }
        }
        .overlay(alignment: .bottom) { ToastView(shell: shell) }
        .sheet(item: Binding(get: { shell.sheet }, set: { shell.sheet = $0 })) { sheet in
            ShellSheet(app: app, sheet: sheet)
        }
        .accessibilityHidden(app.player != nil)
    }

    // MARK: Tabs

    private var tabs: some View {
        // A library opened from the sidebar shows under Libraries.
        TabView(selection: Binding(get: { Self.tab(shell.section) }, set: { shell.select($0) })) {
            ForEach(shell.sections, id: \.self) { section in
                stack(section == Self.tab(shell.section) ? shell.section : section)
                    .tabItem { Label(Self.title(section), systemImage: Self.symbol(section)) }
                    .tag(section)
            }
        }
    }

    private static func tab(_ section: ShellSection) -> ShellSection {
        if case .library = section { return .libraries }
        return section
    }

    // MARK: Sidebar

    private var split: some View {
        NavigationSplitView {
            Sidebar(app: app)
                .safeAreaInset(edge: .bottom) { meRow }
                .navigationTitle("Pley")
                #if os(macOS)
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 300)
                #endif
        } detail: {
            stack(shell.section)
                // Another section, another navigation.
                .id(shell.section)
        }
    }

    /// The profile signed in, at the sidebar's foot: it opens Me.
    private var meRow: some View {
        Button {
            shell.select(.me)
        } label: {
            HStack(spacing: 10) {
                if let user = app.user {
                    AvatarView(app: app, name: user.displayName ?? user.name, path: user.avatarUrl,
                               color: user.avatarColor, size: 32)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(app.user.map { $0.displayName ?? $0.name } ?? "Me")
                        .lineLimit(1)
                    Text(app.server?.name ?? "")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(shell.section == .me ? Color.accentColor.opacity(0.18) : Color.clear,
                        in: RoundedRectangle(cornerRadius: 8))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(8)
        .accessibilityLabel("Me, \(app.user.map { $0.displayName ?? $0.name } ?? "")")
        .accessibilityAddTraits(shell.section == .me ? .isSelected : [])
        .accessibilityIdentifier("shell.me")
    }

    // MARK: Sections

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
        case .me, .settings: MeView(app: app)
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

/// The sidebar: Home and Playlists, then the libraries in the profile's order, each
/// with its kind's symbol; a drag (or Edit on iPad, or VoiceOver's Move up
/// and Move down) puts them in another order.
private struct Sidebar: View {
    @ObservedObject var app: AppModel
    @ObservedObject private var shell: ShellModel
    @StateObject private var model: LibrariesModel

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
        _model = StateObject(wrappedValue: LibrariesModel(app: app))
    }

    var body: some View {
        List(selection: Binding<ShellSection?>(get: { shell.section == .me ? nil : shell.section },
                                               set: { if let section = $0 { shell.select(section) } })) {
            Label(ShellView.title(.home), systemImage: ShellView.symbol(.home))
                .tag(ShellSection.home)
                .accessibilityIdentifier("shell.sidebar.home")
            Label(ShellView.title(.playlists), systemImage: ShellView.symbol(.playlists))
                .tag(ShellSection.playlists)
                .accessibilityIdentifier("shell.sidebar.playlists")
            Section("Libraries") {
                ForEach(model.entries) { entry in
                    HStack {
                        Label(entry.name, systemImage: entry.symbol)
                        Spacer(minLength: 0)
                        if entry.scanning {
                            ProgressView().controlSize(.small).accessibilityHidden(true)
                        }
                    }
                    .tag(ShellSection.library(entry.id))
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel(entry.spoken)
                    .accessibilityActions {
                        if model.entries.first?.id != entry.id {
                            Button("Move up") { model.move(entry.id, by: -1) }
                        }
                        if model.entries.last?.id != entry.id {
                            Button("Move down") { model.move(entry.id, by: 1) }
                        }
                    }
                    .accessibilityIdentifier("shell.sidebar.library.\(entry.id)")
                }
                .onMove(perform: model.reorderable ? { model.move(fromOffsets: $0, toOffset: $1) } : nil)
            }
        }
        // A container of its own, so its identifier doesn't replace its rows'.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("shell.sidebar")
        #if os(iOS)
        .toolbar {
            if model.reorderable {
                ToolbarItem(placement: .primaryAction) {
                    EditButton().accessibilityIdentifier("shell.sidebar.edit")
                }
            }
        }
        #endif
        .task { await model.reload() }
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
