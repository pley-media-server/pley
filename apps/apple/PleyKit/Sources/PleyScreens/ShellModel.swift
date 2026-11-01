import Combine
import Foundation
import PleyData
import PleyModel

/// A part of the app with a navigation of its own: a tab on iPhone, a row
/// of the sidebar on iPad and Mac, a tab across the top on Apple TV.
public enum ShellSection: Hashable, Codable, Sendable {
    case home, libraries, search, playlists, me
    /// One library: a row of the sidebar, a tab on Apple TV.
    case library(Int)
    /// Apple TV's Settings tab, which holds what Me holds elsewhere.
    case settings

    /// How the section is kept between launches: `home`, `library:3`.
    public var key: String {
        switch self {
        case .home: "home"
        case .libraries: "libraries"
        case .search: "search"
        case .playlists: "playlists"
        case .me: "me"
        case .settings: "settings"
        case .library(let id): "library:\(id)"
        }
    }

    /// The section `key` names; `nil` for anything else.
    public init?(key: String) {
        switch key {
        case "home": self = .home
        case "libraries": self = .libraries
        case "search": self = .search
        case "playlists": self = .playlists
        case "me": self = .me
        case "settings": self = .settings
        default:
            guard key.hasPrefix("library:"), let id = Int(key.dropFirst("library:".count)) else { return nil }
            self = .library(id)
        }
    }
}

/// A page pushed onto a section's navigation.
public enum Screen: Hashable, Codable, Sendable {
    case item(Int)
    /// A library's page, opened with a view (`sort=added&order=desc`) or as it was left.
    case library(Int, view: String? = nil)
    case libraries
    case collections(libraryId: Int)
    case collection(Int)
    case person(Int)
    case artist(Int)
    /// An album, with the track it was opened from.
    case album(Int, track: Int? = nil)
    case playlist(Int)
    case history, settings, trackDefaults, diagnostics, about

    /// The page `item` opens: music has its own pages; a track opens its
    /// album, an extra its movie's or show's page.
    public static func page(of item: ItemSummary) -> Screen {
        switch item.kind {
        case .artist: .artist(item.id)
        case .album: .album(item.id)
        case .track: item.parentId.map { .album($0, track: item.id) } ?? .item(item.id)
        case .extra: item.parentId.map { .item($0) } ?? .item(item.id)
        default: .item(item.id)
        }
    }
}

/// A sheet over the shell.
public enum Sheet: Hashable, Identifiable, Sendable {
    case addToPlaylist([Int])
    case newPlaylist([Int])
    case saveSmartPlaylist(libraryId: Int, query: String)

    public var id: Self { self }
}

/// A short message at the bottom of the screen (top trailing on Apple TV),
/// with an Undo while it shows.
public struct Toast: Identifiable, Equatable {
    public let id = UUID()
    public let text: String
    public let undo: (@MainActor () -> Void)?

    public init(_ text: String, undo: (@MainActor () -> Void)? = nil) {
        self.text = text
        self.undo = undo
    }

    public static func == (a: Toast, b: Toast) -> Bool { a.id == b.id }
}

/// The signed-in app's navigation: the section showing, a navigation path
/// per section (as Android keeps a back stack per tab), the sheet and the
/// toast. Choosing the section already showing goes back to its start, as
/// a tab bar does. A profile change, a sign-out or another sign-in starts
/// every section again at Home (`reset`).
@MainActor
public final class ShellModel: ObservableObject {
    /// How long a toast shows.
    public static let toastSeconds: Double = 5

    @Published public private(set) var section: ShellSection
    @Published public private(set) var paths: [ShellSection: [Screen]] = [:]
    @Published public var sheet: Sheet?
    @Published public private(set) var toast: Toast?
    /// The tabs this device shows, in order: the iPhone's tab bar (and the
    /// iPad's in a narrow window), or Apple TV's tabs across the top.
    @Published public private(set) var sections: [ShellSection]
    /// The libraries, in the profile's order: the sidebar's rows, and
    /// Apple TV's library tabs.
    @Published public private(set) var libraries: [Library] = []
    /// Whether the server's Live TV shows (from Phase 4).
    public let liveTv = false

    /// Apple TV shows a tab per library up to this many; more go under one Libraries tab.
    public static let maxLibraryTabs = 4

    private let settings: AppSettings
    private let tv: Bool
    private var toastTask: Task<Void, Never>?

    /// The shell on this device, opening on the section shown last: on
    /// iPhone, iPad and Mac, Home, Libraries, Search, Playlists and Me (the
    /// Android phone's five); on Apple TV, Search, Home, the libraries,
    /// Playlists and Settings.
    public init(settings: AppSettings = AppSettings(), tv: Bool = ShellModel.isTV) {
        self.settings = settings
        self.tv = tv
        sections = Self.sections(tv: tv, libraries: [])
        let last = settings.lastSection.flatMap(ShellSection.init(key:))
        // A library's section waits for the libraries to be known.
        section = last.flatMap { last in
            if case .library = last { return last }
            return Self.sections(tv: tv, libraries: []).contains(last) ? last : nil
        } ?? .home
    }

    /// Whether this is the Apple TV app.
    public static var isTV: Bool {
        #if os(tvOS)
        true
        #else
        false
        #endif
    }

    /// The tabs of a shell, with `libraries`.
    public static func sections(tv: Bool, libraries: [Library]) -> [ShellSection] {
        guard tv else { return [.home, .libraries, .search, .playlists, .me] }
        let middle: [ShellSection] = libraries.count > maxLibraryTabs ? [.libraries] : libraries.map { .library($0.id) }
        // Search first, where tvOS puts it.
        return [.search, .home] + middle + [.playlists, .settings]
    }

    /// The sidebar's rows on iPad and Mac: Home, Search and Playlists, then
    /// each library (Me is at its foot).
    public var sidebar: [ShellSection] { [.home, .search, .playlists] + libraries.map { .library($0.id) } }

    /// Whether `section` is one this device can show now.
    public func shows(_ section: ShellSection) -> Bool {
        if sections.contains(section) { return true }
        if case .library(let id) = section { return libraries.contains { $0.id == id } }
        return false
    }

    /// The libraries, as last read: the sections follow them. A library
    /// section that went away goes back to Home.
    public func setLibraries(_ libraries: [Library]) {
        guard libraries != self.libraries else { return }
        self.libraries = libraries
        sections = Self.sections(tv: tv, libraries: libraries)
        if !shows(section) {
            section = .home
            settings.lastSection = ShellSection.home.key
        }
    }

    /// A library's name, for its tab or row.
    public func libraryName(_ id: Int) -> String {
        libraries.first { $0.id == id }?.name ?? "Library"
    }

    /// The pages on top of `section`'s start.
    public func path(for section: ShellSection) -> [Screen] { paths[section] ?? [] }

    /// Sets `section`'s pages: its navigation's Back took one off.
    public func setPath(_ path: [Screen], for section: ShellSection) {
        paths[section] = path
    }

    /// Shows `section`; showing it again goes back to its start.
    public func select(_ section: ShellSection) {
        guard shows(section) else { return }
        if section == self.section {
            popToRoot()
        } else {
            self.section = section
            settings.lastSection = section.key
        }
    }

    /// `screen` on top of the section showing.
    public func open(_ screen: Screen) {
        paths[section, default: []].append(screen)
    }

    /// The page `item` opens.
    public func open(item: ItemSummary) {
        open(Screen.page(of: item))
    }

    /// The library page a row's See all goes to: the row's library, listed
    /// as the row is; nothing for a row without one.
    public func seeAll(_ hub: Hub) {
        guard let screen = Self.seeAll(hub) else { return }
        open(screen)
    }

    /// Where `hub`'s See all goes, `nil` for a row without one.
    public nonisolated static func seeAll(_ hub: Hub) -> Screen? {
        guard let library = hub.libraryId, let view = seeAllView(hub.key) else { return nil }
        return .library(library, view: view)
    }

    /// The library's sort a row of `key` lists its items in.
    public nonisolated static func seeAllView(_ key: String) -> String? {
        let views = [("recently_added", "sort=added&order=desc"), ("recently_released", "sort=release&order=desc"),
                     ("top_rated", "sort=rating&order=desc")]
        return views.first { key.hasPrefix($0.0) }?.1
    }

    /// Back from the page on top of the section showing.
    public func back() {
        guard var path = paths[section], !path.isEmpty else { return }
        path.removeLast()
        paths[section] = path
    }

    /// The section showing, back at its start.
    public func popToRoot() {
        paths[section] = []
    }

    /// Every section back at its start, and Home showing: another profile
    /// or sign-in sees other things.
    public func reset() {
        paths = [:]
        libraries = []
        sections = Self.sections(tv: tv, libraries: [])
        sheet = nil
        dismissToast()
        section = .home
        settings.lastSection = ShellSection.home.key
    }

    // MARK: Commands: the Mac's menus, the iPad's keyboard

    /// A section in the View menu, with its ⌘ number.
    public struct MenuEntry: Equatable, Sendable {
        public let title: String
        public let section: ShellSection
        /// The key that goes with ⌘ ("1"), `nil` past the ninth.
        public let key: Character?
    }

    /// The View menu's sections, as the sidebar lists them: Home ⌘1,
    /// Playlists ⌘2, then the libraries in the profile's order, the first
    /// seven ⌘3 to ⌘9. Search has ⌘F of its own.
    public var viewMenu: [MenuEntry] {
        let fixed = [MenuEntry(title: "Home", section: .home, key: "1"),
                     MenuEntry(title: "Playlists", section: .playlists, key: "2")]
        let libraries = libraries.enumerated().map { index, library in
            MenuEntry(title: library.name, section: .library(library.id),
                      key: index < 7 ? Character(String(index + 3)) : nil)
        }
        return fixed + libraries
    }

    /// Search's field is asked for (⌘F): Search shows, and its field
    /// takes the keys, then says so (`searchFieldShown`).
    @Published public private(set) var wantsSearchField = false

    /// Search, with its field ready to type in (⌘F).
    public func findInSearch() {
        if section != .search { select(.search) }
        wantsSearchField = true
    }

    /// Search's field has the keys.
    public func searchFieldShown() {
        wantsSearchField = false
    }

    /// Whether the page showing can be read again (View → Reload, ⌘R).
    @Published public private(set) var canReload = false
    private var reloader: (owner: UUID, run: @MainActor () async -> Void)?

    /// The page `owner` shows now, and reads itself again with `reload`.
    /// The page that showed last wins: one pushed on top, or one shown again.
    public func showing(_ owner: UUID, reload: @escaping @MainActor () async -> Void) {
        reloader = (owner, reload)
        canReload = true
    }

    /// `owner` went from the screen; another page may have taken its place already.
    public func leaving(_ owner: UUID) {
        guard reloader?.owner == owner else { return }
        reloader = nil
        canReload = false
    }

    /// Reads the page showing again (⌘R).
    public func reloadShowing() async {
        await reloader?.run()
    }

    // MARK: Toasts

    /// Shows `text` for 5 seconds, with Undo when `undo` is given.
    public func show(_ text: String, undo: (@MainActor () -> Void)? = nil) {
        let toast = Toast(text, undo: undo)
        self.toast = toast
        toastTask?.cancel()
        toastTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.toastSeconds))
            guard !Task.isCancelled, self?.toast == toast else { return }
            self?.toast = nil
        }
    }

    /// The toast's Undo.
    public func undoToast() {
        let undo = toast?.undo
        dismissToast()
        undo?()
    }

    public func dismissToast() {
        toastTask?.cancel()
        toastTask = nil
        toast = nil
    }
}
