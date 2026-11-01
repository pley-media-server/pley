import PleyModel
import PleyScreens
import SwiftUI

/// The signed-in app. On iPhone, and on iPad in a narrow window, a tab bar
/// with Home and Me; on iPad at regular width and on the Mac, a sidebar
/// with Home and, at its foot, the profile signed in (Me). Each section
/// keeps its own navigation, and choosing the section showing goes back
/// to its start. The toast shows at the bottom; while the player is open
/// VoiceOver reads only the player.
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
        .accessibilityHidden(app.player != nil)
    }

    // MARK: Tabs

    private var tabs: some View {
        TabView(selection: Binding(get: { shell.section }, set: { shell.select($0) })) {
            ForEach(shell.sections, id: \.self) { section in
                stack(section)
                    .tabItem { Label(Self.title(section), systemImage: Self.symbol(section)) }
                    .tag(section)
            }
        }
    }

    // MARK: Sidebar

    private var split: some View {
        NavigationSplitView {
            List(selection: Binding<ShellSection?>(get: { shell.section == .me ? nil : shell.section },
                                                   set: { if let section = $0 { shell.select(section) } })) {
                ForEach(shell.sections.filter { $0 != .me }, id: \.self) { section in
                    Label(Self.title(section), systemImage: Self.symbol(section))
                        .tag(section)
                        .accessibilityIdentifier("shell.sidebar.\(section.key)")
                }
            }
            // A container of its own, so its identifier doesn't replace its rows'.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("shell.sidebar")
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
