import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

@MainActor
@Suite struct ShellModelTests {
    private func item(_ id: Int, _ kind: ItemKind, parent: Int? = nil) -> ItemSummary {
        var item = ItemSummary(id: id, libraryId: 1, kind: kind, title: "Item \(id)")
        item.parentId = parent
        return item
    }

    @Test func pagesOpenOnTheSectionShowingAndBackTakesOneOff() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(shell.section == .home)
        shell.open(.item(7))
        shell.open(item: item(8, .movie))
        #expect(shell.path(for: .home) == [.item(7), .item(8)])
        shell.select(.me)
        shell.open(.history)
        #expect(shell.path(for: .me) == [.history])
        #expect(shell.path(for: .home) == [.item(7), .item(8)])
        shell.select(.home)
        shell.back()
        #expect(shell.path(for: .home) == [.item(7)])
        // The navigation's own Back sets the path.
        shell.setPath([], for: .home)
        #expect(shell.path(for: .home).isEmpty)
    }

    @Test func choosingTheSectionShowingGoesBackToItsStart() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.open(.item(1))
        shell.open(.item(2))
        shell.select(.me)
        #expect(shell.path(for: .home).count == 2)
        shell.select(.home)
        #expect(shell.path(for: .home).count == 2)
        shell.select(.home)
        #expect(shell.path(for: .home).isEmpty)
        // A section this device doesn't show isn't chosen.
        shell.select(.settings)
        #expect(shell.section == .home)
        shell.select(.playlists)
        #expect(shell.section == .playlists)
    }

    @Test func aLibrarysTitleMenuSwapsTheLibraryShowing() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.setLibraries([Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "TV Shows", kind: .shows)])
        // A library page opened from Libraries is replaced, not stacked.
        shell.select(.libraries)
        shell.open(.library(1))
        shell.switchLibrary(to: 2)
        #expect(shell.path(for: .libraries) == [.library(2)])
        // On top of another page, the library opens on top.
        shell.open(.item(7))
        shell.switchLibrary(to: 1)
        #expect(shell.path(for: .libraries) == [.library(2), .item(7), .library(1)])
        // In a library's own section (the sidebar), the other library's section shows.
        shell.select(.library(1))
        shell.switchLibrary(to: 2)
        #expect(shell.section == .library(2))
    }

    @Test func musicHasItsOwnPages() {
        #expect(Screen.page(of: item(1, .artist)) == .artist(1))
        #expect(Screen.page(of: item(2, .album)) == .album(2))
        #expect(Screen.page(of: item(3, .track, parent: 2)) == .album(2, track: 3))
        #expect(Screen.page(of: item(4, .track)) == .item(4))
        #expect(Screen.page(of: item(5, .episode)) == .item(5))
    }

    @Test func seeAllListsTheLibraryAsTheRowDoes() {
        #expect(ShellModel.seeAll(Hub(key: "recently_added:3", title: "Recently Added in Movies", libraryId: 3))
            == .library(3, view: "sort=added&order=desc"))
        #expect(ShellModel.seeAll(Hub(key: "top_rated:3", title: "Top Rated", libraryId: 3))
            == .library(3, view: "sort=rating&order=desc"))
        #expect(ShellModel.seeAll(Hub(key: "continue_watching", title: "Continue Watching")) == nil)
        #expect(ShellModel.seeAll(Hub(key: "my_list", title: "My List")) == nil)
    }

    @Test func theSectionShownLastIsKeptAndAResetGoesHome() {
        let settings = freshSettings()
        let shell = ShellModel(settings: settings, tv: false)
        shell.select(.me)
        shell.open(.history)
        #expect(ShellModel(settings: settings, tv: false).section == .me)
        // A section another device has isn't restored here.
        #expect(ShellModel(settings: settings, tv: true).section == .home)
        shell.sheet = .addToPlaylist([1])
        shell.show("Added to Favorites")
        shell.reset()
        #expect(shell.section == .home)
        #expect(shell.paths.isEmpty)
        #expect(shell.sheet == nil)
        #expect(shell.toast == nil)
        #expect(settings.lastSection == "home")
    }

    @Test func aLibraryShownLastComesBackOnceTheLibrariesAreRead() {
        let settings = freshSettings()
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        let first = ShellModel(settings: settings, tv: true)
        first.setLibraries(libraries)
        first.select(.library(2))
        // Home until they are read: no tab is a library's yet.
        let shell = ShellModel(settings: settings, tv: true)
        #expect(shell.section == .home)
        shell.setLibraries(libraries)
        #expect(shell.section == .library(2))
        // One that went stays on Home.
        let again = ShellModel(settings: settings, tv: true)
        again.setLibraries([libraries[0]])
        #expect(again.section == .home)
        // A tab chosen meanwhile wins.
        settings.lastSection = ShellSection.library(1).key
        let chosen = ShellModel(settings: settings, tv: true)
        chosen.select(.search)
        chosen.setLibraries(libraries)
        #expect(chosen.section == .search)
    }

    @Test func sectionsKeepTheirNames() {
        for section in [ShellSection.home, .libraries, .search, .playlists, .me, .settings, .liveTv, .requests,
                        .library(12)] {
            #expect(ShellSection(key: section.key) == section)
        }
        #expect(ShellSection(key: "library:x") == nil)
        #expect(ShellSection(key: "live") == nil)
    }

    @Test func searchAndPlaylistsHaveATabASidebarRowAndAnAppleTVTab() {
        // The Android phone's five tabs.
        #expect(ShellModel.sections(tv: false, libraries: []) == [.home, .libraries, .search, .playlists, .me])
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        // Search first on Apple TV, where tvOS puts it.
        #expect(ShellModel.sections(tv: true, libraries: libraries)
            == [.search, .home, .libraries, .playlists, .settings])
        #expect(ShellModel.tabWidth(.playlists, libraries: libraries) == "Listen".count + ShellModel.tabPadding)
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.setLibraries(libraries)
        #expect(shell.sidebar == [.home, .search, .playlists, .library(1), .library(2)])
    }

    @Test func appleTVHasOneLibraryTabWhateverTheLibrariesLiveTVAndMusic() {
        for (live, music) in [(false, false), (true, false), (false, true), (true, true)] {
            for count in 0...9 {
                let libraries = (0..<count).map { Library(id: $0 + 1, name: "L\($0 + 1)", kind: .movies) }
                let tabs = ShellModel.sections(tv: true, libraries: libraries, liveTv: live, nowPlaying: music)
                #expect(Array(tabs.prefix(2)) == [.search, .home])
                let end: [ShellSection] = music ? [.playlists, .nowPlaying, .settings] : [.playlists, .settings]
                #expect(Array(tabs.suffix(end.count)) == end)
                #expect(tabs.contains(.liveTv) == live)
                if live { #expect(tabs[2] == .liveTv) }
                // One Libraries tab for them all; none without a library to show.
                #expect(tabs.contains(.libraries) == (count > 0))
                #expect(!tabs.contains { if case .library = $0 { true } else { false } })
            }
        }
        // Music is under Listen: music libraries alone make no Libraries tab.
        let music = [Library(id: 3, name: "Music", kind: .music)]
        #expect(ShellModel.sections(tv: true, libraries: music) == [.search, .home, .playlists, .settings])
        // The tab is Libraries whatever library it shows.
        let named = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Documentaries", kind: .movies)]
        #expect(ShellModel.tabWidth(.libraries, libraries: named) == "Libraries".count + ShellModel.tabPadding)
        // Discover, Live TV and Now Playing: Listen gives up its words first.
        let discover = RequestsEntry(discover: true, requests: true)
        let long = [Library(id: 1, name: "Documentaries and Concerts", kind: .movies)]
        let crowded = ShellModel.sections(tv: true, libraries: long, liveTv: true, nowPlaying: true, requests: discover)
        #expect(crowded == [.search, .home, .requests, .liveTv, .libraries, .playlists, .nowPlaying, .settings])
        let symbols = ShellModel.symbolOnly(crowded, libraries: long, requests: discover)
        #expect(symbols.contains(.playlists) && !symbols.contains(.home))
        let shown = crowded.reduce(0) {
            $0 + (symbols.contains($1) ? ShellModel.tabPadding : ShellModel.tabWidth($1, libraries: long, requests: discover))
        }
        #expect(shown <= ShellModel.tabRoom)
        // While they fit, every tab keeps its words.
        let two = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        #expect(ShellModel.symbolOnly(ShellModel.sections(tv: true, libraries: two, liveTv: true), libraries: two)
            == ShellModel.symbolTabs)

        // The phone's five tabs don't change: Live TV is a row of Libraries.
        #expect(ShellModel.sections(tv: false, libraries: two, liveTv: true) == [.home, .libraries, .search, .playlists, .me])
    }

    @Test func appleTVsLibraryTabShowsTheLibraryChosenLast() {
        let settings = freshSettings()
        let shell = ShellModel(settings: settings, tv: true)
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows),
                         Library(id: 3, name: "Music", kind: .music)]
        shell.setLibraries(libraries)
        // The first movie or show library, before one is chosen.
        #expect(shell.libraryTabSection == .library(1))
        shell.openLibraryTab()
        #expect(shell.section == .library(1))
        // Another chosen from the title's menu stays the tab's, after other tabs.
        shell.switchLibrary(to: 2)
        #expect(shell.section == .library(2))
        shell.select(.home)
        #expect(shell.libraryTabSection == .library(2))
        shell.openLibraryTab()
        #expect(shell.section == .library(2) && settings.lastSection == ShellSection.library(2).key)
        // Live TV has its own tab on Apple TV: it never becomes the Library tab's.
        shell.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        shell.select(.liveTv)
        #expect(shell.libraryTabSection == .library(2))
        // A music library is a page under Listen, never a section.
        #expect(!shell.shows(.library(3)))
        shell.select(.library(3))
        #expect(shell.section == .liveTv)
        // The library chosen last went: the first again.
        shell.setLibraries(Array(libraries.prefix(1)))
        #expect(shell.libraryTabSection == .library(1))
    }

    @Test func liveTVShowsOnceTheServerSaysItIsAvailable() {
        let settings = freshSettings()
        let shell = ShellModel(settings: settings, tv: false)
        #expect(!shell.showsLiveTv && !shell.shows(.liveTv))
        shell.setLiveTv(LiveSummary(available: false, allowed: true))
        #expect(!shell.sidebar.contains(.liveTv) && !shell.viewMenu.contains { $0.section == .liveTv })
        shell.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        #expect(shell.sidebar.prefix(2) == [.home, .liveTv])
        let menu = shell.viewMenu
        #expect(menu.map(\.title).prefix(3) == ["Home", "Live TV", "Playlists"])
        #expect(menu[1].key == "l" && menu[0].key == "1" && menu[2].key == "2")
        shell.select(.liveTv)
        #expect(shell.section == .liveTv && settings.lastSection == "liveTv")
        // Shown last: it comes back once Live TV is known to show.
        let again = ShellModel(settings: settings, tv: true)
        #expect(again.section == .home)
        again.setLibraries([Library(id: 1, name: "Movies", kind: .movies)])
        #expect(again.section == .home)
        again.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        #expect(again.section == .liveTv)
        #expect(again.sections == [.search, .home, .liveTv, .libraries, .playlists, .settings])
        // It went: back to Home.
        again.setLiveTv(LiveSummary(available: false, allowed: false))
        #expect(again.section == .home && !again.sections.contains(.liveTv))
        again.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        again.reset()
        #expect(again.liveTv == nil && !again.sections.contains(.liveTv))
    }

    @Test func aToastsUndoRunsOnce() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        var undone = 0
        shell.show("Removed Arrival") { undone += 1 }
        #expect(shell.toast?.text == "Removed Arrival")
        shell.runToastAction()
        shell.runToastAction()
        #expect(undone == 1)
        #expect(shell.toast == nil)
    }

    @Test func aToastCarriesItsOneButtonAndSaysAnError() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        var opened = 0
        shell.show("“Arrival” is ready to watch.", action: ToastAction("Open") { opened += 1 })
        #expect(shell.toast?.action?.label == "Open" && shell.toast?.action?.identifier == "toast.action")
        #expect(shell.toast?.style == .plain)
        shell.runToastAction()
        #expect(opened == 1 && shell.toast == nil)
        shell.show("TMDB isn't answering.", action: nil, error: true)
        #expect(shell.toast?.style == .error && shell.toast?.action == nil)
        // Undo keeps its own identifier.
        shell.show("Removed Arrival") {}
        #expect(shell.toast?.action?.identifier == "toast.undo")
    }

    @Test func theWayIntoRequestsCarriesItsNumberUntilAReset() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(shell.requestsEntry == .none)
        let entry = RequestsEntry(discover: true, requests: true, manage: true, badge: 3, failed: 1)
        shell.setRequestsEntry(entry)
        #expect(shell.requestsEntry == entry && shell.requestsEntry.alarm && shell.requestsEntry.badgeText == "3")
        shell.reset()
        #expect(shell.requestsEntry == .none)
    }

    @Test func appleTVHasATabIntoRequestsAfterHome() {
        let two = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        let discover = RequestsEntry(discover: true, requests: true, badge: 1)
        #expect(ShellModel.sections(tv: true, libraries: two, requests: discover)
            == [.search, .home, .requests, .libraries, .playlists, .settings])
        // Before Live TV.
        #expect(ShellModel.sections(tv: true, libraries: two, liveTv: true, requests: discover)
            == [.search, .home, .requests, .liveTv, .libraries, .playlists, .settings])
        // Nothing to see or ask for: no tab. The phone's five don't change.
        #expect(!ShellModel.sections(tv: true, libraries: two, requests: .none).contains(.requests))
        #expect(!ShellModel.sections(tv: true, libraries: two, requests: RequestsEntry(badge: 2)).contains(.requests))
        #expect(ShellModel.sections(tv: false, libraries: two, requests: discover)
            == [.home, .libraries, .search, .playlists, .me])

        // Discover where the account may ask; else its requests, or the managers' Requests.
        #expect(ShellModel.requestsTitle(discover) == "Discover")
        #expect(ShellModel.requestsTitle(RequestsEntry(requests: true)) == "Your requests")
        #expect(ShellModel.requestsTitle(RequestsEntry(requests: true, manage: true, badge: 4)) == "Requests")

        // Its title takes room across the screen, and no number with it (tvOS's tabs have no badges).
        let yours = RequestsEntry(requests: true, badge: 2)
        #expect(ShellModel.tabWidth(.requests, libraries: two, requests: yours)
            == ShellModel.tabWidth(.requests, libraries: two, requests: RequestsEntry(requests: true)))
        #expect(ShellModel.sections(tv: true, libraries: two, requests: yours)
            == [.search, .home, .requests, .libraries, .playlists, .settings])
        for entry in [discover, yours, RequestsEntry(requests: true, manage: true, badge: 120)] {
            for (live, music) in [(false, false), (true, false), (false, true)] {
                for count in 0...6 {
                    let libraries = (0..<count).map { Library(id: $0 + 1, name: "L\($0 + 1)", kind: .movies) }
                    let tabs = ShellModel.sections(tv: true, libraries: libraries, liveTv: live, nowPlaying: music,
                                                   requests: entry)
                    #expect(tabs[2] == .requests)
                    // Whatever shows, the tabs' words fit, or give way to their symbols.
                    let symbols = ShellModel.symbolOnly(tabs, libraries: libraries, requests: entry)
                    let width = tabs.reduce(0) {
                        $0 + (symbols.contains($1) ? ShellModel.tabPadding
                            : ShellModel.tabWidth($1, libraries: libraries, requests: entry))
                    }
                    #expect(width <= ShellModel.tabRoom)
                }
            }
        }
        // With Live TV and music too: eight tabs.
        #expect(ShellModel.sections(tv: true, libraries: two, liveTv: true, nowPlaying: true, requests: discover)
            == [.search, .home, .requests, .liveTv, .libraries, .playlists, .nowPlaying, .settings])
        #expect(ShellModel.sections(tv: true, libraries: [], liveTv: true, nowPlaying: true, requests: discover)
            == [.search, .home, .requests, .liveTv, .playlists, .nowPlaying, .settings])
    }

    @Test func appleTVsTabFollowsTheSummary() {
        let shell = ShellModel(settings: freshSettings(), tv: true)
        let two = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        shell.setLibraries(two)
        shell.setRequestsEntry(RequestsEntry(discover: true, requests: true))
        #expect(shell.sections == [.search, .home, .requests, .libraries, .playlists, .settings])
        shell.select(.requests)
        #expect(shell.section == .requests)
        // Asking no longer works: the tab is the person's requests.
        shell.setRequestsEntry(RequestsEntry(requests: true, badge: 1))
        #expect(shell.sections == [.search, .home, .requests, .libraries, .playlists, .settings])
        #expect(shell.section == .requests)
        // Requests went: back to Home.
        shell.setRequestsEntry(.none)
        #expect(!shell.sections.contains(.requests) && shell.section == .home)
    }

    @Test func appleTVsRequestsTabShownLastComesBackOnceTheSummaryIsRead() {
        let settings = freshSettings()
        let libraries = [Library(id: 1, name: "Movies", kind: .movies)]
        let entry = RequestsEntry(discover: true, requests: true)
        let first = ShellModel(settings: settings, tv: true)
        first.setRequestsEntry(entry)
        first.select(.requests)
        #expect(settings.lastSection == "requests")
        // Home until the summary is read: the libraries alone don't decide.
        let shell = ShellModel(settings: settings, tv: true)
        #expect(shell.section == .home)
        shell.setRequestsEntry(.none, known: false)
        shell.setLibraries(libraries)
        #expect(shell.section == .home)
        shell.setRequestsEntry(entry)
        #expect(shell.section == .requests)
        // Read, and nothing to see: it stays on Home.
        let gone = ShellModel(settings: settings, tv: true)
        gone.setLibraries(libraries)
        gone.setRequestsEntry(.none)
        #expect(gone.section == .home && !gone.sections.contains(.requests))
        // A tab chosen meanwhile wins.
        settings.lastSection = ShellSection.requests.key
        let chosen = ShellModel(settings: settings, tv: true)
        chosen.select(.search)
        chosen.setRequestsEntry(entry)
        #expect(chosen.section == .search)
        // A reset forgets the summary: the next one is waited for again.
        chosen.select(.requests)
        chosen.reset()
        #expect(chosen.section == .home && !chosen.sections.contains(.requests))
    }
}

@MainActor
@Suite struct ShellNavigationTests {
    /// Me, or Apple TV's Settings, which holds the same.
    private let meSection = ShellModel.sections(tv: ShellModel.isTV, libraries: []).last!

    private func signedIn(_ server: FixtureServer) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        try #require(app.root == .signedIn)
        return app
    }

    @Test func appleTVReadsItsLibraryTabsWhateverTabShows() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        settings.lastSection = ShellSection.search.key
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, tv: true)
        await app.start()
        #expect(app.shell.section == .search)
        // The fixture's libraries are under one Libraries tab.
        #expect(await eventually { app.shell.sections.contains(.libraries) })
        #expect(app.shell.section == .search)
    }

    @Test func aProfileChangeStartsEverySectionAgainAtHome() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.open(.item(7))
        app.shell.select(meSection)
        app.shell.open(.history)
        let me = MeModel(app: app)
        await me.load()
        #expect(me.canSwitch)
        me.switchProfile()
        #expect(app.root == .profiles)
        #expect(app.profilesFromShell)
        let profiles = ProfilesModel(app: app)
        await profiles.load()
        await profiles.choose(try #require(profiles.profiles.first { $0.name == "Kids" }))
        #expect(app.user?.name == "Kids")
        #expect(app.root == .signedIn)
        #expect(app.shell.section == .home)
        #expect(app.shell.paths.isEmpty)
    }

    @Test func choosingTheProfileInUseGoesBackAsItWas() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.select(meSection)
        app.showProfiles()
        let profiles = ProfilesModel(app: app)
        await profiles.load()
        await profiles.choose(try #require(profiles.profiles.first(where: \.current)))
        #expect(app.root == .signedIn)
        #expect(app.shell.section == meSection)
        #expect(!app.profilesFromShell)
    }

    @Test func signingOutStartsAgainAtHomeNextTime() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.select(meSection)
        app.shell.open(.history)
        await MeModel(app: app).signOut()
        #expect(app.root == .servers)
        #expect(app.shell.section == .home)
        #expect(app.shell.paths.isEmpty)
        #expect(app.user == nil)
    }

    @Test func meSaysWhoAndWhere() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let me = MeModel(app: app)
        #expect(me.name == "Alex")
        #expect(me.kind == nil)
        #expect(me.serverName == "Living Room")
        #expect(!me.serverAddress.isEmpty)
        me.showServers()
        #expect(app.root == .servers)
    }

    @Test func oneQueueOfLateReportsPerServerAndProfile() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let first = app.lateReports(origin: server.origin, profileId: 2)
        #expect(app.lateReports(origin: server.origin, profileId: 2) === first)
        #expect(app.lateReports(origin: server.origin, profileId: 3) !== first)
    }
}

@Suite struct TileTests {
    @Test func rowsAreShapedByWhatTheyHold() {
        var episode = ItemSummary(id: 1, libraryId: 1, kind: .episode, title: "Glasshouse")
        episode.grandparentTitle = "The Office"
        episode.grandparentId = 9
        episode.parentIndex = 2
        episode.index = 5
        episode.durationMs = 3_480_000
        episode.userState.positionMs = 1_920_000
        let movie = ItemSummary(id: 2, libraryId: 1, kind: .movie, title: "Arrival")
        let album = ItemSummary(id: 3, libraryId: 4, kind: .album, title: "Discovery")
        #expect(RowShape.of(Hub(key: "continue_watching", title: "", items: [movie])) == .landscape)
        #expect(RowShape.of(Hub(key: "recently_added:5", title: "", items: [episode])) == .landscape)
        #expect(RowShape.of(Hub(key: "recently_added:4", title: "", items: [album])) == .square)
        #expect(RowShape.of(Hub(key: "recently_added:1", title: "", items: [movie])) == .poster)

        let still = Tile(episode, shape: .landscape)
        #expect(still.title == "The Office")
        #expect(still.subtitle == "S2 · E5 · 26 min left")
        #expect(still.pictures == ["/api/items/9/images/backdrop"])
        #expect(still.progress > 0.5)
        let poster = Tile(episode, shape: .poster)
        #expect(poster.title == "Glasshouse")
        #expect(poster.subtitle == "26 min left")
        #expect(poster.pictures == ["/api/items/9/images/poster"])
    }

    @Test func homeRowsCarryTheirSeeAll() {
        let row = HomeRow(Hub(key: "recently_added:3", title: "Recently Added in Movies", libraryId: 3,
                              items: [ItemSummary(id: 2, libraryId: 3, kind: .movie, title: "Arrival")]))
        #expect(row.seeAll == .library(3, view: "sort=added&order=desc"))
        #expect(row.tiles.map(\.id) == [2])
        #expect(row.shape == .poster)
    }
}
