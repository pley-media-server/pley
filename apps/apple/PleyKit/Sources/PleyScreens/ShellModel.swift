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
    /// Live TV: a row of the sidebar after Home, a tab after Home on Apple
    /// TV; on iPhone a row of Libraries opens it (`Screen.liveTv`).
    case liveTv
    /// Apple TV's Now Playing tab, while music has a queue. iPhone, iPad
    /// and the Mac show a mini player instead.
    case nowPlaying
    /// Apple TV's way into requests, after Home: Discover where the account
    /// may ask, else Your requests (Requests for managers). iPhone, iPad
    /// and the Mac have them under Me.
    case requests

    /// How the section is kept between launches: `home`, `library:3`.
    public var key: String {
        switch self {
        case .home: "home"
        case .libraries: "libraries"
        case .search: "search"
        case .playlists: "playlists"
        case .me: "me"
        case .settings: "settings"
        case .liveTv: "liveTv"
        case .nowPlaying: "nowPlaying"
        case .requests: "requests"
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
        case "liveTv": self = .liveTv
        case "nowPlaying": self = .nowPlaying
        case "requests": self = .requests
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
    /// A Home row of several libraries' collections of one title, opened together.
    case mergedCollection([Int])
    case person(Int)
    case artist(Int)
    /// An album, with the track it was opened from.
    case album(Int, track: Int? = nil)
    case playlist(Int)
    case history, settings, trackDefaults, diagnostics, about
    /// Me → Downloads, and Settings → Downloads (iPhone, iPad and Mac).
    case downloads, downloadSettings
    /// Live TV, opened from Libraries on iPhone.
    case liveTv
    /// Requests: Discover's page, TMDB's titles matching a search (See
    /// all), one of Discover's lists whole (a row's See all, on Apple TV),
    /// a title's page (its season picker open with `pick`), and the list of
    /// requests (everyone's, for managers).
    case discover
    case discoverResults(query: String)
    case discoverList(DiscoverList)
    case discoverTitle(TitleKind, tmdbId: Int, pick: Bool = false)
    case requests

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

/// A toast's one button: Undo, Open, Sign in.
public struct ToastAction {
    public let label: String
    /// The button's accessibility identifier.
    public let identifier: String
    public let run: @MainActor () -> Void

    public init(_ label: String, identifier: String = "toast.action", run: @escaping @MainActor () -> Void) {
        self.label = label
        self.identifier = identifier
        self.run = run
    }

    public static func undo(_ run: @escaping @MainActor () -> Void) -> ToastAction {
        ToastAction(localized("Undo"), identifier: "toast.undo", run: run)
    }
}

/// A short message at the bottom of the screen (top trailing on Apple TV),
/// with a button while it shows; `error` ones say something went wrong.
public struct Toast: Identifiable, Equatable {
    public enum Style: Sendable { case plain, error }

    public let id = UUID()
    public let text: String
    public let action: ToastAction?
    public let style: Style

    public init(_ text: String, action: ToastAction? = nil, style: Style = .plain) {
        self.text = text
        self.action = action
        self.style = style
    }

    public init(_ text: String, undo: (@MainActor () -> Void)?) {
        self.init(text, action: undo.map(ToastAction.undo))
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

    @Published public private(set) var section: ShellSection {
        didSet { if Self.underLibraryTab(section, tv: tv), section != .libraries { lastLibrary = section } }
    }
    /// The library (or Live TV, on iPhone) shown last under the iPhone's
    /// Library tab, which takes its name, or Apple TV's Libraries tab.
    @Published public private(set) var lastLibrary: ShellSection?
    @Published public private(set) var paths: [ShellSection: [Screen]] = [:]
    @Published public var sheet: Sheet?
    @Published public private(set) var toast: Toast?
    /// The tabs this device shows, in order: the iPhone's tab bar (and the
    /// iPad's in a narrow window), or Apple TV's tabs across the top.
    @Published public private(set) var sections: [ShellSection]
    /// The libraries, in the profile's order: the sidebar's rows, and what
    /// Apple TV's Libraries tab switches between.
    @Published public private(set) var libraries: [Library] = []
    /// Live TV on the open server for this profile (`GET /api/live`);
    /// `nil` while not known. It shows when it is `available`.
    @Published public private(set) var liveTv: LiveSummary?
    /// The ways into requests (Discover, and Your requests or Requests with
    /// its number), from the summary; `none` while not known.
    @Published public private(set) var requestsEntry = RequestsEntry.none
    /// Requests' summary was read: Apple TV knows whether its tab shows.
    private var requestsKnown = false
    /// Music has a queue: Apple TV shows its Now Playing tab.
    @Published public private(set) var nowPlaying = false
    /// iPhone and iPad: Now Playing shows, in a sheet over the shell.
    @Published public var nowPlayingOpen = false

    /// How much of Apple TV's tab bar the tabs may take, counted in
    /// characters: a tab with words takes its title's and `tabPadding`
    /// more, one with its symbol alone `tabPadding`. Past it, tvOS scrolls
    /// the bar and its ends go off the screen.
    public static let tabRoom = 64
    static let tabPadding = 4

    /// Apple TV's tabs that show their symbol alone (VoiceOver still reads
    /// their names): Search, as the system's search tab, Now Playing and
    /// Settings.
    public static let symbolTabs: Set<ShellSection> = [.search, .nowPlaying, .settings]

    private let settings: AppSettings
    private let tv: Bool
    private var toastTask: Task<Void, Never>?
    /// The library section shown last, waiting for the libraries: until
    /// they are read it has no tab, so Home shows meanwhile.
    private var restoring: ShellSection?

    /// The shell on this device, opening on the section shown last: on
    /// iPhone, iPad and Mac, Home, Libraries, Search, Playlists and Me (the
    /// Android phone's five); on Apple TV, Search, Home, Libraries, Listen
    /// and Settings.
    public init(settings: AppSettings = AppSettings(), tv: Bool = ShellModel.isTV) {
        self.settings = settings
        self.tv = tv
        sections = Self.sections(tv: tv, libraries: [])
        let last = settings.lastSection.flatMap(ShellSection.init(key:))
        if let last, Self.underLibraryTab(last, tv: tv), last != .libraries { lastLibrary = last }
        // A library's section, Live TV's and requests', wait on Home for what decides whether they show.
        if case .library = last {
            restoring = last
            section = .home
        } else if last == .liveTv || last == .requests {
            restoring = last
            section = .home
        } else {
            section = last.flatMap { Self.sections(tv: tv, libraries: []).contains($0) ? $0 : nil } ?? .home
        }
    }

    /// Whether this is the Apple TV app.
    public static var isTV: Bool {
        #if os(tvOS)
        true
        #else
        false
        #endif
    }

    /// The tabs of a shell, with `libraries`, Live TV when it shows, the
    /// way into requests (`requests`) and Now Playing while music has a
    /// queue. On Apple TV: Search, Home, Discover or the requests, Live TV,
    /// Libraries (while there is a movie or show library: one tab for them
    /// all, showing the one chosen last), Listen (the Playlists tab, which
    /// has the music libraries), Now Playing and Settings.
    public static func sections(tv: Bool, libraries: [Library], liveTv: Bool = false, nowPlaying: Bool = false,
                                requests: RequestsEntry = .none) -> [ShellSection] {
        guard tv else { return [.home, .libraries, .search, .playlists, .me] }
        // Search first, where tvOS puts it.
        let start: [ShellSection] = [.search, .home] + (requests.shows ? [.requests] : []) + (liveTv ? [.liveTv] : [])
        let library: [ShellSection] = libraries.contains { $0.kind != .music } ? [.libraries] : []
        return start + library + [.playlists] + (nowPlaying ? [.nowPlaying] : []) + [.settings]
    }

    /// Whether `section` shows under the Library tab: on Apple TV (its
    /// Libraries tab) a movie or show library's, as Live TV has its own
    /// tab; on iPhone the picker's places (`LibraryPicker.underTab`).
    static func underLibraryTab(_ section: ShellSection, tv: Bool) -> Bool {
        guard tv else { return LibraryPicker.underTab(section) }
        switch section {
        case .libraries, .library: return true
        default: return false
        }
    }

    /// Apple TV's Libraries tab chosen: the library shown last under it,
    /// else the first movie or show library in the profile's order.
    public func openLibraryTab() {
        select(libraryTabSection)
    }

    /// What Apple TV's Libraries tab shows: the library showing, or shown
    /// last, else the first movie or show library; Libraries without one.
    public var libraryTabSection: ShellSection {
        if case .library = section, shows(section) { return section }
        if let last = lastLibrary, case .library = last, shows(last) { return last }
        return libraries.first { $0.kind != .music }.map { .library($0.id) } ?? .libraries
    }

    /// The tabs of `sections` that show their symbol alone on Apple TV:
    /// Search, Now Playing and Settings, and while the tabs' words would
    /// take more than `tabRoom`, Playlists, then Libraries, then Live TV,
    /// then the requests' tab, until they fit. So the bar never scrolls
    /// Settings, which holds Switch profile and Sign out, off the screen.
    public static func symbolOnly(_ sections: [ShellSection], libraries: [Library],
                                  requests: RequestsEntry = .none) -> Set<ShellSection> {
        var symbols = symbolTabs
        func total() -> Int {
            sections.reduce(0) { sum, section in
                sum + (symbols.contains(section) ? tabPadding : tabWidth(section, libraries: libraries, requests: requests))
            }
        }
        for section in [ShellSection.playlists, .libraries, .liveTv, .requests] where total() > tabRoom {
            if sections.contains(section) { symbols.insert(section) }
        }
        return symbols
    }

    /// How much of the bar `section`'s tab takes (`tabRoom`).
    static func tabWidth(_ section: ShellSection, libraries: [Library], requests: RequestsEntry = .none) -> Int {
        if symbolTabs.contains(section) { return tabPadding }
        let title = switch section {
        case .home: localized("Home")
        case .libraries: localized("Libraries")
        case .playlists: localized("Listen")
        case .liveTv: localized("Live TV")
        case .requests: requestsTitle(requests)
        case .library(let id): libraries.first { $0.id == id }?.name ?? localized("Library")
        default: localized("Settings")
        }
        return title.count + tabPadding
    }

    /// Apple TV's tab into requests: "Discover" where the account may ask,
    /// else "Your requests" ("Requests" for managers).
    public nonisolated static func requestsTitle(_ entry: RequestsEntry) -> String {
        entry.discover ? RequestWords.discover : entry.requestsTitle
    }

    /// The sidebar's rows on iPad and Mac: Home, Live TV when it shows,
    /// Search and Playlists, then each library (Me is at its foot).
    public var sidebar: [ShellSection] {
        [.home] + (showsLiveTv ? [.liveTv] : []) + [.search, .playlists] + libraries.map { .library($0.id) }
    }

    /// Whether Live TV shows: open to this profile, with channels.
    public var showsLiveTv: Bool { liveTv?.available == true }

    /// Whether `section` is one this device can show now.
    public func shows(_ section: ShellSection) -> Bool {
        if sections.contains(section) { return true }
        // Apple TV's music libraries are pages under Listen, never a section.
        if case .library(let id) = section { return libraries.contains { $0.id == id && (!tv || $0.kind != .music) } }
        // The sidebar's Live TV row: in a tab bar it shows under Libraries, as a library does.
        if section == .liveTv { return showsLiveTv }
        return false
    }

    /// Live TV as last read (`nil`: not known, or another server). When
    /// it goes, its section goes back to Home.
    public func setLiveTv(_ liveTv: LiveSummary?) {
        guard liveTv != self.liveTv else { return }
        self.liveTv = liveTv
        refreshSections()
    }

    /// The ways into requests, as the summary last said; `known`, the
    /// summary was read (Apple TV's requests tab, shown last, waits for it).
    public func setRequestsEntry(_ entry: RequestsEntry, known: Bool = true) {
        if known { requestsKnown = true }
        guard entry != requestsEntry || restoring == .requests else { return }
        let before = requestsEntry
        requestsEntry = entry
        // Apple TV's tab comes or goes, and its title and number take room in the bar.
        if tv && (before.shows != entry.shows || before.discover != entry.discover
                  || before.manage != entry.manage
                  || restoring == .requests) {
            refreshSections()
        }
    }

    /// The libraries, as last read: the sections follow them. A library
    /// section that went away goes back to Home.
    public func setLibraries(_ libraries: [Library]) {
        guard libraries != self.libraries else { return }
        self.libraries = libraries
        refreshSections()
    }

    /// Music has a queue, or no longer: Apple TV's Now Playing tab comes
    /// or goes (and the tab showing goes back to Home with it).
    public func setNowPlaying(_ active: Bool) {
        if !active { nowPlayingOpen = false }
        guard active != nowPlaying else { return }
        nowPlaying = active
        refreshSections()
    }

    private func refreshSections() {
        sections = Self.sections(tv: tv, libraries: libraries, liveTv: showsLiveTv, nowPlaying: nowPlaying,
                                 requests: requestsEntry)
        // Waiting for what it needs: the libraries, Live TV, or requests' summary.
        if let restoring, waiting(for: restoring) { return }
        if let restoring {
            self.restoring = nil
            if shows(restoring) {
                section = restoring
                return
            }
        }
        if !shows(section) {
            section = .home
            settings.lastSection = ShellSection.home.key
        }
    }

    /// Whether `section`, shown last, waits for what decides if it shows.
    private func waiting(for section: ShellSection) -> Bool {
        switch section {
        case .liveTv: liveTv == nil
        case .requests: !requestsKnown
        default: libraries.isEmpty
        }
    }

    /// A library's name, for its tab or row.
    public func libraryName(_ id: Int) -> String {
        libraries.first { $0.id == id }?.name ?? localized("Library")
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
        // Chosen by the person: the section shown last stays put.
        restoring = nil
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

    /// A library's page title menu: library `id` instead of the one
    /// showing. A library's own section (the sidebar's rows) goes to that
    /// library's section; a library page opened in another section is
    /// replaced, so Back doesn't walk through every library chosen.
    public func switchLibrary(to id: Int) {
        if case .library = section, shows(.library(id)) {
            select(.library(id))
            return
        }
        var path = paths[section, default: []]
        if case .library = path.last {
            path[path.count - 1] = .library(id)
            paths[section] = path
        } else {
            open(.library(id))
        }
    }

    /// The page `item` opens.
    public func open(item: ItemSummary) {
        open(Screen.page(of: item))
    }

    /// Where a row's title and See all go: its collection, or the merged
    /// page of several; else the row's library, listed as the row is;
    /// nothing for a row without one.
    public func seeAll(_ hub: Hub) {
        guard let screen = Self.seeAll(hub) else { return }
        open(screen)
    }

    /// Where `hub`'s See all goes, `nil` for a row without one.
    public nonisolated static func seeAll(_ hub: Hub) -> Screen? {
        if hub.collectionIds.count == 1 { return .collection(hub.collectionIds[0]) }
        if hub.collectionIds.count > 1 { return .mergedCollection(hub.collectionIds) }
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
        restoring = nil
        paths = [:]
        libraries = []
        liveTv = nil
        requestsEntry = .none
        requestsKnown = false
        nowPlaying = false
        nowPlayingOpen = false
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

    /// The View menu's sections, as the sidebar lists them: Home ⌘1, Live
    /// TV ⌘L when it shows, Playlists ⌘2, then the libraries in the
    /// profile's order, the first seven ⌘3 to ⌘9. Search has ⌘F of its own.
    public var viewMenu: [MenuEntry] {
        let fixed = [MenuEntry(title: localized("Home"), section: .home, key: "1")]
            + (showsLiveTv ? [MenuEntry(title: localized("Live TV"), section: .liveTv, key: "l")] : [])
            + [MenuEntry(title: localized("Playlists"), section: .playlists, key: "2")]
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
        show(Toast(text, undo: undo))
    }

    /// Shows `text` for 5 seconds, with `action`'s button; `error` says something went wrong.
    public func show(_ text: String, action: ToastAction?, error: Bool = false) {
        show(Toast(text, action: action, style: error ? .error : .plain))
    }

    public func show(_ toast: Toast) {
        self.toast = toast
        toastTask?.cancel()
        toastTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.toastSeconds))
            guard !Task.isCancelled, self?.toast == toast else { return }
            self?.toast = nil
        }
    }

    /// The toast's button: Undo, Open.
    public func runToastAction() {
        let action = toast?.action
        dismissToast()
        action?.run()
    }

    public func dismissToast() {
        toastTask?.cancel()
        toastTask = nil
        toast = nil
    }
}
